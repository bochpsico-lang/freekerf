//! What scripts and agents do, for the person in front of the app (PLAN §9.4):
//! the agents attached to the local endpoint, the scripts running, a log of their
//! actions, and permission requests answered in the window ([`WindowPrompter`]).
//! Everything is published as [`AutomationEvent`]s; `automation.status` and
//! `automation.log` read the state.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Weak;
use std::time::Duration;

use crossbeam_channel::{Sender, bounded};
use freekerf_session::{Caller, CallerKind, Tier};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::permissions::{GrantScope, PermissionRequest, Prompter};
use crate::{Inner, ServiceEvent, lock};

/// Log entries kept.
pub const LOG_ENTRIES: usize = 500;
/// How long a permission request waits for the person before it is denied.
pub const PERMISSION_TIMEOUT: Duration = Duration::from_secs(120);

/// Who did something.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Actor {
    /// Macro or agent.
    pub kind: CallerKind,
    /// Its name.
    pub name: String,
}

impl From<&Caller> for Actor {
    fn from(c: &Caller) -> Self {
        Self {
            kind: c.kind,
            name: c.name.clone(),
        }
    }
}

/// An action of a script or an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct LogEntry {
    /// Grows with every entry.
    pub seq: u64,
    /// Who.
    pub actor: Actor,
    /// The action.
    pub action: String,
    /// Why it failed or was refused; none when it ran.
    pub error: Option<String>,
}

/// A script running now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RunningScript {
    /// Id of the run.
    pub id: u64,
    /// The script's name.
    pub name: String,
    /// Who runs it.
    pub actor: Actor,
}

/// A permission request waiting for the person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PendingPermission {
    /// Id of the request (answer with it).
    pub id: u64,
    /// Who asks.
    pub actor: Actor,
    /// The action it wants to run.
    pub action: String,
    /// The tier the action needs.
    pub tier: Tier,
}

/// Agents, scripts and pending requests (`automation.status`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AutomationStatus {
    /// Agents attached to the local endpoint, by name.
    pub agents: Vec<String>,
    /// Scripts running.
    pub scripts: Vec<RunningScript>,
    /// Permission requests waiting for the person.
    pub requests: Vec<PendingPermission>,
}

/// What changed in automation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutomationEvent {
    /// An agent attached or detached; scripts started or ended; a request came or
    /// was answered: read `automation.status` again.
    Status,
    /// A permission request waits for the person.
    PermissionRequested {
        /// The request.
        request: PendingPermission,
    },
    /// A script or an agent ran an action (or was refused one).
    Logged {
        /// The log entry.
        entry: LogEntry,
    },
}

/// The automation state of a service.
#[derive(Debug, Default)]
pub(crate) struct State {
    status: AutomationStatus,
    log: VecDeque<LogEntry>,
    next: u64,
    answers: BTreeMap<u64, Sender<Option<GrantScope>>>,
}

impl State {
    fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}

impl Inner {
    fn automation_event(&self, e: AutomationEvent) {
        self.publish(&ServiceEvent::Automation(e));
    }

    /// Records an action of a script or an agent (people's actions are not logged).
    pub(crate) fn log_action(&self, caller: &Caller, action: &str, error: Option<String>) {
        if caller.kind == CallerKind::User {
            return;
        }
        let entry = {
            let mut a = lock(&self.automation);
            let seq = a.id();
            let entry = LogEntry {
                seq,
                actor: caller.into(),
                action: action.to_owned(),
                error,
            };
            a.log.push_back(entry.clone());
            while a.log.len() > LOG_ENTRIES {
                a.log.pop_front();
            }
            entry
        };
        self.automation_event(AutomationEvent::Logged { entry });
    }

    pub(crate) fn agent_attached(&self, name: &str) {
        lock(&self.automation).status.agents.push(name.to_owned());
        self.automation_event(AutomationEvent::Status);
    }

    pub(crate) fn agent_detached(&self, name: &str) {
        {
            let mut a = lock(&self.automation);
            if let Some(i) = a.status.agents.iter().position(|n| n == name) {
                a.status.agents.remove(i);
            }
        }
        self.automation_event(AutomationEvent::Status);
    }

    pub(crate) fn script_started(&self, caller: &Caller, name: &str) -> u64 {
        let id = {
            let mut a = lock(&self.automation);
            let id = a.id();
            a.status.scripts.push(RunningScript {
                id,
                name: name.to_owned(),
                actor: caller.into(),
            });
            id
        };
        self.automation_event(AutomationEvent::Status);
        id
    }

    pub(crate) fn script_ended(&self, id: u64) {
        lock(&self.automation).status.scripts.retain(|s| s.id != id);
        self.automation_event(AutomationEvent::Status);
    }

    pub(crate) fn automation_status(&self) -> AutomationStatus {
        lock(&self.automation).status.clone()
    }

    pub(crate) fn automation_log(&self, after: u64) -> Vec<LogEntry> {
        lock(&self.automation)
            .log
            .iter()
            .filter(|e| e.seq > after)
            .cloned()
            .collect()
    }

    /// Asks the person and waits for the answer (or the timeout: denied).
    fn ask_window(&self, req: &PermissionRequest<'_>, timeout: Duration) -> Option<GrantScope> {
        let (tx, rx) = bounded(1);
        let pending = {
            let mut a = lock(&self.automation);
            let id = a.id();
            let p = PendingPermission {
                id,
                actor: req.caller.into(),
                action: req.action.to_owned(),
                tier: req.tier,
            };
            a.status.requests.push(p.clone());
            a.answers.insert(id, tx);
            p
        };
        let id = pending.id;
        self.automation_event(AutomationEvent::PermissionRequested { request: pending });
        self.automation_event(AutomationEvent::Status);
        let answer = rx.recv_timeout(timeout).ok().flatten();
        {
            let mut a = lock(&self.automation);
            a.answers.remove(&id);
            a.status.requests.retain(|r| r.id != id);
        }
        self.automation_event(AutomationEvent::Status);
        answer
    }

    pub(crate) fn answer_permission(&self, id: u64, scope: Option<GrantScope>) -> bool {
        lock(&self.automation)
            .answers
            .remove(&id)
            .is_some_and(|tx| tx.send(scope).is_ok())
    }
}

/// The prompter of an app with a window: a person is there, and permission
/// requests wait for their answer in the window ([`crate::Service::answer_permission`]).
/// Laser actions of scripts and agents are never confirmed here (agents never run
/// them; a person confirms each one in the fire confirmation).
pub struct WindowPrompter {
    inner: Weak<Inner>,
    timeout: Duration,
}

impl WindowPrompter {
    pub(crate) fn new(inner: Weak<Inner>, timeout: Duration) -> Self {
        Self { inner, timeout }
    }
}

impl Prompter for WindowPrompter {
    fn ask(&self, req: &PermissionRequest<'_>) -> Option<GrantScope> {
        self.inner.upgrade()?.ask_window(req, self.timeout)
    }
}
