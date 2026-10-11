//! Scripts over the service (PLAN §9.2, ADR 0011): the engine of
//! `freekerf-script` with the registry as its only way out.
//!
//! * **Transaction.** A run's document, selection and view changes form one undo
//!   step labelled `script <name>`; on an uncaught error they are rolled back
//!   (machine actions cannot be, and are never retried).
//! * **Dry run.** The script runs against a copy of the session: document and view
//!   actions change the copy, `read` actions outside the session read the real
//!   state, and every other action (machine, laser, files, settings, jobs) is
//!   simulated: it returns `{"simulated": true}` and is listed in
//!   [`ScriptReport::simulated`]. Nothing is applied.
//! * **Permissions.** Every call is checked by [`Service::authorize`]: the
//!   caller's tier, its grants, and the person (who can grant this run, this
//!   session or always). A macro is identified by its name and the hash of its code.
//! * **Laser.** An action of tier `laser` runs only when a person confirms it
//!   ([`ScriptRequest::confirm_laser`]); without a confirmation (headless, the CLI,
//!   agents) it is refused.
//! * **STOP.** [`Service::stop`] and `machine.stop` cancel every running script
//!   before stopping the machine.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use freekerf_script::ScriptHost;
pub use freekerf_script::{ActionRecord, Limits, ScriptOutcome};
use freekerf_session::actions::{SessionHost, register_session_actions};
use freekerf_session::session::Session;
use freekerf_session::{ActionError, ActionSpec, Caller, CallerKind, Registry, Tier};
use serde::Serialize;
use serde_json::{Value, json};

use crate::permissions::{RunGrants, code_hash};
use crate::{Service, lock};

/// Asked before a `laser` action runs: `(action, params)` → confirmed by a person.
pub type ConfirmLaser = Arc<dyn Fn(&str, &Value) -> bool + Send + Sync>;

/// One run.
#[derive(Clone)]
pub struct ScriptRequest {
    /// Name of the script (macro name, file name); labels the undo step.
    pub name: String,
    /// JavaScript source.
    pub code: String,
    /// Run against a copy of the session and simulate everything else.
    pub dry_run: bool,
    /// Limits of the run.
    pub limits: Limits,
    /// Human confirmation of `laser` actions; `None` refuses them.
    pub confirm_laser: Option<ConfirmLaser>,
}

impl std::fmt::Debug for ScriptRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptRequest")
            .field("name", &self.name)
            .field("dry_run", &self.dry_run)
            .field("limits", &self.limits)
            .field("confirm_laser", &self.confirm_laser.is_some())
            .finish_non_exhaustive()
    }
}

impl ScriptRequest {
    /// A run of `code` named `name` with the default limits, no laser.
    #[must_use]
    pub fn new(name: impl Into<String>, code: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            code: code.into(),
            dry_run: false,
            limits: Limits::default(),
            confirm_laser: None,
        }
    }
}

/// The result of a run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScriptReport {
    /// Value, log, actions, error, time.
    #[serde(flatten)]
    pub outcome: ScriptOutcome,
    /// Whether it was a dry run.
    pub dry_run: bool,
    /// Actions that were simulated (dry run), in order.
    pub simulated: Vec<String>,
    /// Whether the run's document and view changes were rolled back (on an error).
    pub rolled_back: bool,
}

impl ScriptReport {
    fn failed(error: String, dry_run: bool) -> Self {
        Self {
            outcome: ScriptOutcome {
                value: Value::Null,
                log: Vec::new(),
                actions: Vec::new(),
                error: Some(error),
                elapsed_ms: 0,
            },
            dry_run,
            simulated: Vec::new(),
            rolled_back: false,
        }
    }
}

/// The session copy of a dry run.
pub struct DryContext {
    session: Mutex<Session>,
}

impl SessionHost for DryContext {
    fn session(&self) -> &Mutex<Session> {
        &self.session
    }
}

/// The session actions over a copy (built once per service).
pub(crate) fn dry_registry() -> Registry<DryContext> {
    let mut r = Registry::new();
    if let Err(e) = register_session_actions(&mut r) {
        tracing::error!("cannot register the dry-run session actions: {e}");
    }
    r
}

fn laser_refused(action: &str) -> ActionError {
    ActionError::Refused(format!(
        "`{action}` can turn the laser on and needs a person to confirm it; it cannot run from this script"
    ))
}

/// A live run: every call goes to the service with the script's caller, its
/// grants and the person's answers.
struct LiveHost {
    service: Service,
    caller: Caller,
    code_hash: Option<String>,
    run: RunGrants,
    confirm: Option<ConfirmLaser>,
}

impl ScriptHost for LiveHost {
    fn specs(&self) -> Vec<ActionSpec> {
        self.service.specs()
    }

    fn dispatch(&self, action: &str, params: Value) -> Result<Value, ActionError> {
        let tier =
            self.service
                .authorize(&self.caller, self.code_hash.as_deref(), action, &self.run, true)?;
        // no grant replaces the confirmation of each laser action
        if tier == Tier::Laser && !self.confirm.as_ref().is_some_and(|f| f(action, &params)) {
            return Err(laser_refused(action));
        }
        self.service
            .dispatch(&elevated(&self.caller, tier), action, params)
    }
}

fn elevated(caller: &Caller, tier: Tier) -> Caller {
    let mut c = caller.clone();
    c.max_tier = c.max_tier.max(tier);
    c
}

/// A dry run: the session copy, real reads, everything else simulated. Tiers and
/// grants are checked; nobody is asked.
struct DryHost {
    service: Service,
    caller: Caller,
    code_hash: Option<String>,
    copy: DryContext,
    simulated: Mutex<Vec<String>>,
}

impl ScriptHost for DryHost {
    fn specs(&self) -> Vec<ActionSpec> {
        self.service.specs()
    }

    fn dispatch(&self, action: &str, params: Value) -> Result<Value, ActionError> {
        let tier = self.service.authorize(
            &self.caller,
            self.code_hash.as_deref(),
            action,
            &RunGrants::default(),
            false,
        )?;
        let caller = elevated(&self.caller, tier);
        if self.service.dry.spec(action).is_some() {
            return self.service.dry.dispatch(&self.copy, &caller, action, params);
        }
        if tier == Tier::Read {
            return self.service.dispatch(&caller, action, params);
        }
        // a preview reports what the real run would refuse
        crate::permissions::check_agent_files(&caller, action, &params)?;
        crate::permissions::check_agent_laser(&caller, action)?;
        lock(&self.simulated).push(action.to_owned());
        Ok(json!({"simulated": true}))
    }
}

/// Macros are identified by their code too: editing one invalidates its grants.
fn macro_hash(caller: &Caller, request: &ScriptRequest) -> Option<String> {
    (caller.kind == CallerKind::Macro).then(|| code_hash(&request.code))
}

impl Service {
    /// Runs a script as `caller`: its tier, then its grants and the person
    /// ([`Service::authorize`]), limit every call.
    #[must_use]
    pub fn run_script(&self, caller: &Caller, request: &ScriptRequest) -> ScriptReport {
        let cancel = Arc::new(AtomicBool::new(false));
        lock(&self.inner.scripts).push(Arc::clone(&cancel));
        let report = if request.dry_run {
            self.dry_run(caller, request, &cancel)
        } else {
            self.live_run(caller, request, &cancel)
        };
        lock(&self.inner.scripts).retain(|c| !Arc::ptr_eq(c, &cancel));
        report
    }

    fn live_run(&self, caller: &Caller, request: &ScriptRequest, cancel: &Arc<AtomicBool>) -> ScriptReport {
        let label = format!("script {}", request.name);
        if !lock(&self.inner.session).begin(&label) {
            return ScriptReport::failed("another script is running".into(), false);
        }
        let host = Arc::new(LiveHost {
            service: self.clone(),
            caller: caller.clone(),
            code_hash: macro_hash(caller, request),
            run: RunGrants::default(),
            confirm: request.confirm_laser.clone(),
        });
        let outcome = freekerf_script::run(&request.code, host, &request.limits, cancel);
        let rolled_back = outcome.error.is_some();
        {
            let mut session = lock(&self.inner.session);
            if rolled_back {
                session.rollback();
            } else {
                session.commit();
            }
        }
        self.inner.publish_session_events();
        ScriptReport {
            outcome,
            dry_run: false,
            simulated: Vec::new(),
            rolled_back,
        }
    }

    fn dry_run(&self, caller: &Caller, request: &ScriptRequest, cancel: &Arc<AtomicBool>) -> ScriptReport {
        let mut copy = lock(&self.inner.session).clone();
        copy.drain_events();
        let host = Arc::new(DryHost {
            service: self.clone(),
            caller: caller.clone(),
            code_hash: macro_hash(caller, request),
            copy: DryContext {
                session: Mutex::new(copy),
            },
            simulated: Mutex::new(Vec::new()),
        });
        let outcome = freekerf_script::run(
            &request.code,
            Arc::clone(&host) as Arc<dyn ScriptHost>,
            &request.limits,
            cancel,
        );
        let simulated = lock(&host.simulated).clone();
        ScriptReport {
            outcome,
            dry_run: true,
            simulated,
            rolled_back: false,
        }
    }

    /// Stops every running script at its next interrupt check (STOP does it first).
    pub fn cancel_scripts(&self) {
        self.inner.cancel_scripts();
    }

    /// The TypeScript declarations of the script API (`doc/scripting/api.d.ts`).
    #[must_use]
    pub fn api_dts(&self) -> String {
        freekerf_script::dts::api_dts(&self.specs())
    }
}

impl crate::Inner {
    pub(crate) fn cancel_scripts(&self) {
        for c in lock(&self.scripts).iter() {
            c.store(true, Ordering::SeqCst);
        }
    }
}
