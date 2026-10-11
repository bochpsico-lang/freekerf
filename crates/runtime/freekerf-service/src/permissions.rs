//! Who may do what (PLAN §9.5). Every action has a tier; a caller has a baseline
//! (the highest tier allowed without asking: from the settings for agents, chosen
//! by the app for macros and the user). Above it, the
//! person in front of the app can grant more, for **this run** (a script or an
//! MCP call), **this session** (until the app closes) or **always** (stored in
//! `automation.grants`). A macro's grants carry a hash of its code, so editing the
//! macro invalidates them.
//!
//! With no one present (headless), nothing is asked: only the baseline and the
//! "always" grants apply. No grant ever replaces the human confirmation of
//! `laser` actions (`script::ScriptRequest::confirm_laser`).

use std::sync::{Arc, Mutex};

use freekerf_session::{ActionError, Caller, CallerKind, Tier};
use freekerf_settings::Grant;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{Service, lock};

/// How long a grant lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrantScope {
    /// Until the current script or MCP call finishes.
    Run,
    /// Until the app closes.
    Session,
    /// Until revoked (stored in the settings).
    Always,
}

/// A call above the caller's baseline, shown to the person.
#[derive(Debug, Clone, Copy)]
pub struct PermissionRequest<'a> {
    /// Who asks.
    pub caller: &'a Caller,
    /// Macros: hash of the code.
    pub code_hash: Option<&'a str>,
    /// The action.
    pub action: &'a str,
    /// Its tier.
    pub tier: Tier,
}

/// Asks the person in front of the app (the desktop app's dialog, M4a).
pub trait Prompter: Send + Sync {
    /// The scope granted, or `None` when denied or closed.
    fn ask(&self, request: &PermissionRequest<'_>) -> Option<GrantScope>;
    /// Whether someone can answer (and confirm laser actions).
    fn present(&self) -> bool {
        true
    }
    /// Asks the person to confirm one `laser` action on the machine's screen
    /// (the fire confirmation). Refused unless implemented.
    fn confirm_laser(&self, _caller: &Caller, _action: &str, _params: &Value) -> bool {
        false
    }
}

/// Headless: no one to ask; everything above the baseline is refused.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOnePresent;

impl Prompter for NoOnePresent {
    fn ask(&self, _: &PermissionRequest<'_>) -> Option<GrantScope> {
        None
    }
    fn present(&self) -> bool {
        false
    }
}

/// Grants of one run (a script, an MCP call).
#[derive(Debug, Default)]
pub struct RunGrants(Mutex<Option<Tier>>);

impl RunGrants {
    fn covers(&self, tier: Tier) -> bool {
        lock(&self.0).is_some_and(|t| tier <= t)
    }
    fn add(&self, tier: Tier) {
        let mut t = lock(&self.0);
        *t = Some(t.map_or(tier, |old| old.max(tier)));
    }
}

/// A grant and its scope (`Service::grants`, `automation.grants`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct GrantEntry {
    /// Scope.
    pub scope: GrantScope,
    /// The grant.
    #[serde(flatten)]
    pub grant: Grant,
}

/// The hash of a macro's code that its grants carry.
#[must_use]
pub fn code_hash(code: &str) -> String {
    Sha256::digest(code.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn matches(grant: &Grant, caller: &Caller, code_hash: Option<&str>, tier: Tier) -> bool {
    grant.caller == caller.kind
        && grant.name == caller.name
        && tier <= grant.tier
        && grant.code_hash.as_deref().is_none_or(|h| Some(h) == code_hash)
}

/// Agents never run laser actions, not even through the fire confirmation: they
/// cannot ask for one (`fire.request`).
///
/// # Errors
/// [`ActionError::Refused`] for an agent asking.
pub fn check_agent_laser(caller: &Caller, action: &str) -> Result<(), ActionError> {
    if caller.kind == CallerKind::Agent && action == "fire.request" {
        return Err(ActionError::Refused(
            "agents never run laser actions, not even after a confirmation".into(),
        ));
    }
    Ok(())
}

/// What agents may write (PLAN §9.5): `document.save` only to a new `.fkproj`
/// file; an existing file is never overwritten. Reading (`document.import`,
/// `document.open`) is not limited.
///
/// # Errors
/// [`ActionError::Refused`] with the reason.
pub fn check_agent_files(caller: &Caller, action: &str, params: &Value) -> Result<(), ActionError> {
    if caller.kind != CallerKind::Agent || action != "document.save" {
        return Ok(());
    }
    let Some(path) = params.get("path").and_then(Value::as_str) else {
        return Ok(()); // the action reports the missing parameter
    };
    let p = std::path::Path::new(path);
    let is_project = p
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("fkproj"));
    if !is_project {
        return Err(ActionError::Refused(format!(
            "agents can only save projects (`.fkproj`), not `{path}`"
        )));
    }
    if p.exists() {
        return Err(ActionError::Refused(format!(
            "`{path}` exists; agents never overwrite files (choose a new name)"
        )));
    }
    Ok(())
}

/// The state of the permission system held by the service.
pub(crate) struct Permissions {
    prompter: Mutex<Arc<dyn Prompter>>,
    session: Mutex<Vec<Grant>>,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            prompter: Mutex::new(Arc::new(NoOnePresent)),
            session: Mutex::new(Vec::new()),
        }
    }
}

impl Service {
    /// Sets who answers permission requests (the desktop app); the default is
    /// [`NoOnePresent`].
    pub fn set_prompter(&self, prompter: Arc<dyn Prompter>) {
        *lock(&self.inner.permissions.prompter) = prompter;
    }

    /// Whether a person can answer requests and confirm laser actions.
    #[must_use]
    pub fn human_present(&self) -> bool {
        lock(&self.inner.permissions.prompter).present()
    }

    /// Asks the person to confirm a `laser` action (refused headless).
    #[must_use]
    pub fn confirm_laser(&self, caller: &Caller, action: &str, params: &Value) -> bool {
        let prompter = Arc::clone(&lock(&self.inner.permissions.prompter));
        prompter.present() && prompter.confirm_laser(caller, action, params)
    }

    /// The baseline tier of an agent from the settings: `None` while
    /// `automation.mcp.enabled` is off, else `edit`, or `machine` with
    /// `allow_machine`. Other kinds are given their tier by the app (`None`).
    #[must_use]
    pub fn baseline(&self, kind: CallerKind) -> Option<Tier> {
        let st = lock(&self.inner.settings);
        match kind {
            CallerKind::Agent => {
                let mcp = &st.automation.mcp;
                mcp.enabled.then_some(if mcp.allow_machine {
                    Tier::Machine
                } else {
                    Tier::Edit
                })
            }
            CallerKind::User | CallerKind::Macro | CallerKind::Test => None,
        }
    }

    /// Checks that `caller` may run `action`: its baseline, then the run, session
    /// and "always" grants, then (when `prompt`) the person, whose grant is kept
    /// for the scope they choose.
    ///
    /// # Errors
    /// Unknown action, or permission denied.
    pub fn authorize(
        &self,
        caller: &Caller,
        code_hash: Option<&str>,
        action: &str,
        run: &RunGrants,
        prompt: bool,
    ) -> Result<Tier, ActionError> {
        let r = self.authorize_inner(caller, code_hash, action, run, prompt);
        if let Err(e) = &r {
            self.inner.log_action(caller, action, Some(e.to_string()));
        }
        r
    }

    fn authorize_inner(
        &self,
        caller: &Caller,
        code_hash: Option<&str>,
        action: &str,
        run: &RunGrants,
        prompt: bool,
    ) -> Result<Tier, ActionError> {
        let tier = self
            .registry
            .spec(action)
            .map(|s| s.tier)
            .ok_or_else(|| ActionError::Unknown(action.into()))?;
        check_agent_laser(caller, action)?;
        if caller.kind == CallerKind::Agent && tier == Tier::Laser {
            // no setting or grant lets an agent turn the laser on
            return Err(ActionError::Refused(format!(
                "`{action}` can turn the laser on; agents never run laser actions"
            )));
        }
        if caller.allows(tier) || run.covers(tier) {
            return Ok(tier);
        }
        let granted = lock(&self.inner.permissions.session)
            .iter()
            .chain(lock(&self.inner.settings).automation.grants.iter())
            .any(|g| matches(g, caller, code_hash, tier));
        if granted {
            return Ok(tier);
        }
        let denied = || ActionError::PermissionDenied {
            action: action.into(),
            needed: tier,
        };
        if !prompt {
            return Err(denied());
        }
        let prompter = Arc::clone(&lock(&self.inner.permissions.prompter));
        let request = PermissionRequest {
            caller,
            code_hash,
            action,
            tier,
        };
        let scope = prompter.ask(&request).ok_or_else(denied)?;
        let grant = Grant {
            caller: caller.kind,
            name: caller.name.clone(),
            code_hash: code_hash.map(str::to_owned),
            tier,
        };
        match scope {
            GrantScope::Run => run.add(tier),
            GrantScope::Session => lock(&self.inner.permissions.session).push(grant),
            GrantScope::Always => {
                lock(&self.inner.settings).automation.grants.push(grant);
                self.inner.save_settings()?;
            }
        }
        tracing::info!(caller = %caller.name, action, %tier, ?scope, "permission granted");
        Ok(tier)
    }

    /// [`Service::authorize`], then the action (with the granted tier).
    ///
    /// # Errors
    /// Permission denied, or the action's error.
    pub fn dispatch_authorized(
        &self,
        caller: &Caller,
        code_hash: Option<&str>,
        action: &str,
        params: Value,
        run: &RunGrants,
        prompt: bool,
    ) -> Result<Value, ActionError> {
        let tier = self.authorize(caller, code_hash, action, run, prompt)?;
        let mut elevated = caller.clone();
        elevated.max_tier = elevated.max_tier.max(tier);
        self.dispatch(&elevated, action, params)
    }

    /// The session and "always" grants (user-only: not an action).
    #[must_use]
    pub fn grants(&self) -> Vec<GrantEntry> {
        self.inner.grant_entries()
    }

    /// Revokes every session and "always" grant of a caller at once (user-only:
    /// not an action). Returns how many were removed.
    ///
    /// # Errors
    /// The settings file cannot be saved.
    pub fn revoke(&self, kind: CallerKind, name: &str) -> Result<usize, ActionError> {
        let keep = |g: &Grant| !(g.caller == kind && g.name == name);
        let mut removed = 0;
        {
            let mut s = lock(&self.inner.permissions.session);
            let before = s.len();
            s.retain(keep);
            removed += before - s.len();
        }
        let always = {
            let mut st = lock(&self.inner.settings);
            let before = st.automation.grants.len();
            st.automation.grants.retain(keep);
            before - st.automation.grants.len()
        };
        if always > 0 {
            self.inner.save_settings()?;
        }
        Ok(removed + always)
    }
}

impl crate::Inner {
    pub(crate) fn grant_entries(&self) -> Vec<GrantEntry> {
        let session = lock(&self.permissions.session).clone();
        let always = lock(&self.settings).automation.grants.clone();
        session
            .into_iter()
            .map(|grant| GrantEntry {
                scope: GrantScope::Session,
                grant,
            })
            .chain(always.into_iter().map(|grant| GrantEntry {
                scope: GrantScope::Always,
                grant,
            }))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_save_only_new_projects() {
        let dir = tempfile::tempdir().unwrap();
        let agent = Caller::new(CallerKind::Agent, "a", Tier::Edit);
        let path = |name: &str| serde_json::json!({"path": dir.path().join(name).to_string_lossy()});
        assert!(check_agent_files(&agent, "document.save", &path("new.fkproj")).is_ok());
        assert!(check_agent_files(&agent, "document.save", &path("new.FKPROJ")).is_ok());
        let e = check_agent_files(&agent, "document.save", &path("x.svg")).unwrap_err();
        assert!(e.to_string().contains("only save projects"));
        assert!(check_agent_files(&agent, "document.save", &path("noext")).is_err());
        std::fs::write(dir.path().join("old.fkproj"), "{}").unwrap();
        let e = check_agent_files(&agent, "document.save", &path("old.fkproj")).unwrap_err();
        assert!(e.to_string().contains("never overwrite"));
        // others, other actions, a missing path: not this check's business
        let user = Caller::user();
        assert!(check_agent_files(&user, "document.save", &path("old.fkproj")).is_ok());
        assert!(check_agent_files(&agent, "document.import", &path("old.fkproj")).is_ok());
        assert!(check_agent_files(&agent, "document.save", &Value::Null).is_ok());
    }

    #[test]
    fn grant_matching() {
        let caller = Caller::new(CallerKind::Macro, "m", Tier::Edit);
        let g = Grant {
            caller: CallerKind::Macro,
            name: "m".into(),
            code_hash: Some(code_hash("a")),
            tier: Tier::Machine,
        };
        assert!(matches(&g, &caller, Some(&code_hash("a")), Tier::Machine));
        assert!(!matches(&g, &caller, Some(&code_hash("b")), Tier::Machine));
        assert!(!matches(&g, &caller, None, Tier::Machine));
        assert!(!matches(&g, &caller, Some(&code_hash("a")), Tier::Laser));
        let agent = Caller::new(CallerKind::Agent, "m", Tier::Edit);
        assert!(!matches(&g, &agent, Some(&code_hash("a")), Tier::Machine));
        let run = RunGrants::default();
        assert!(!run.covers(Tier::Ui));
        run.add(Tier::Machine);
        run.add(Tier::Ui);
        assert!(run.covers(Tier::Machine));
        assert!(!run.covers(Tier::Laser));
        assert_eq!(code_hash("").len(), 64);
        assert!(
            NoOnePresent
                .ask(&PermissionRequest {
                    caller: &caller,
                    code_hash: None,
                    action: "x",
                    tier: Tier::Read
                })
                .is_none()
        );
    }
}
