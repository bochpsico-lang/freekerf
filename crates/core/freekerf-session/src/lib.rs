//! The action registry: every thing a user can do is a named, typed action with a
//! JSON schema, a description and a permission tier (PLAN §9, ADR 0004).
//!
//! Front ends (CLI, `host --stdio`, later the desktop UI, scripts and MCP) never
//! call internals directly: they [`Registry::dispatch`] an action by name with
//! JSON parameters, and the tier of the caller is checked on every call.
//!
//! ```
//! use freekerf_session::{Caller, Registry, Tier};
//!
//! #[derive(serde::Deserialize, schemars::JsonSchema)]
//! struct Add { a: i64, b: i64 }
//!
//! let mut r: Registry<()> = Registry::new();
//! r.register("math.add", "Adds two numbers.", Tier::Read, |_: &(), p: Add| Ok(p.a + p.b)).unwrap();
//! let v = r.dispatch(&(), &Caller::user(), "math.add", serde_json::json!({"a": 1, "b": 2})).unwrap();
//! assert_eq!(v, serde_json::json!(3));
//! ```

use std::collections::BTreeMap;
use std::fmt;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Permission tier of an action, from least to most powerful (`Safety` is special:
/// always allowed, because it only makes the machine safer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// STOP, feed hold, abort: always allowed.
    Safety,
    /// Reading state.
    Read,
    /// Selection, panels, view.
    Ui,
    /// Changing the document or settings.
    Edit,
    /// Moving the machine, changing its settings.
    Machine,
    /// Anything that can turn the laser on.
    Laser,
}

impl Tier {
    /// Name as used in docs and schemas.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Safety => "safety",
            Self::Read => "read",
            Self::Ui => "ui",
            Self::Edit => "edit",
            Self::Machine => "machine",
            Self::Laser => "laser",
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Who calls an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CallerKind {
    /// A person, through the UI or the CLI.
    User,
    /// A user macro (script).
    Macro,
    /// An AI agent through MCP.
    Agent,
    /// The external Python client.
    PythonClient,
    /// A test harness (`host --stdio`).
    Test,
}

/// A caller and the highest tier it may use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caller {
    /// Kind.
    pub kind: CallerKind,
    /// Name (agent name, macro name...).
    pub name: String,
    /// Highest tier allowed without asking.
    pub max_tier: Tier,
}

impl Caller {
    /// The person in front of the machine: everything allowed.
    #[must_use]
    pub fn user() -> Self {
        Self {
            kind: CallerKind::User,
            name: "user".into(),
            max_tier: Tier::Laser,
        }
    }

    /// A caller with a limit.
    #[must_use]
    pub fn new(kind: CallerKind, name: impl Into<String>, max_tier: Tier) -> Self {
        Self {
            kind,
            name: name.into(),
            max_tier,
        }
    }

    /// True when the caller may run an action of `tier`.
    #[must_use]
    pub fn allows(&self, tier: Tier) -> bool {
        tier == Tier::Safety || tier <= self.max_tier
    }
}

/// Error of an action.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum ActionError {
    /// No action with this name.
    #[error("unknown action `{0}`")]
    Unknown(String),
    /// Parameters do not match the schema.
    #[error("invalid parameters: {0}")]
    InvalidParams(String),
    /// The caller's tier is too low.
    #[error("permission denied: `{action}` needs tier `{needed}`")]
    PermissionDenied {
        /// Action.
        action: String,
        /// Tier it needs.
        needed: Tier,
    },
    /// Not possible in the current state.
    #[error("{0}")]
    Refused(String),
    /// Something failed while doing it.
    #[error("{0}")]
    Failed(String),
}

/// Registration error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// The name is taken.
    #[error("action `{0}` is already registered")]
    Duplicate(String),
    /// The name is not `namespace.snake_case`.
    #[error("invalid action name `{0}` (expected `namespace.snake_case`)")]
    BadName(String),
    /// The description is empty.
    #[error("action `{0}` has no description")]
    NoDescription(String),
    /// No action with this name (describing a result).
    #[error("action `{0}` is not registered")]
    NotRegistered(String),
}

/// Public description of an action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActionSpec {
    /// `namespace.name`.
    pub name: String,
    /// What it does (first line is the summary).
    pub description: String,
    /// Tier.
    pub tier: Tier,
    /// JSON schema of the parameters.
    pub params: Value,
    /// JSON schema of the result, when the registrant described it
    /// ([`Registry::describe_result`]); front ends type the result as unknown otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
}

type Handler<C> = Box<dyn Fn(&C, Value) -> Result<Value, ActionError> + Send + Sync>;

struct Entry<C> {
    spec: ActionSpec,
    handler: Handler<C>,
}

/// The registry of actions over a context `C` (the service, the session...).
pub struct Registry<C> {
    actions: BTreeMap<String, Entry<C>>,
}

impl<C> fmt::Debug for Registry<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Registry")
            .field("actions", &self.actions.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl<C> Default for Registry<C> {
    fn default() -> Self {
        Self::new()
    }
}

/// True for `namespace.snake_case` (one dot, lower-case letters, digits, `_`).
#[must_use]
pub fn valid_name(name: &str) -> bool {
    let ok = |s: &str| {
        !s.is_empty()
            && s.starts_with(|c: char| c.is_ascii_lowercase())
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    matches!(name.split_once('.'), Some((ns, n)) if ok(ns) && ok(n))
}

impl<C> Registry<C> {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            actions: BTreeMap::new(),
        }
    }

    /// Registers an action with typed parameters `P` and result `R`.
    ///
    /// # Errors
    /// Duplicate or malformed name, missing description.
    pub fn register<P, R, F>(
        &mut self,
        name: &str,
        description: &str,
        tier: Tier,
        f: F,
    ) -> Result<(), RegistryError>
    where
        P: DeserializeOwned + JsonSchema,
        R: Serialize,
        F: Fn(&C, P) -> Result<R, ActionError> + Send + Sync + 'static,
    {
        if !valid_name(name) {
            return Err(RegistryError::BadName(name.into()));
        }
        if description.trim().is_empty() {
            return Err(RegistryError::NoDescription(name.into()));
        }
        if self.actions.contains_key(name) {
            return Err(RegistryError::Duplicate(name.into()));
        }
        let params = serde_json::to_value(schemars::schema_for!(P)).unwrap_or(Value::Null);
        let handler: Handler<C> = Box::new(move |ctx, value| {
            // `null` means "no parameters" for parameter structs with defaults
            let value = if value.is_null() {
                Value::Object(serde_json::Map::new())
            } else {
                value
            };
            let p: P =
                serde_json::from_value(value).map_err(|e| ActionError::InvalidParams(e.to_string()))?;
            let r = f(ctx, p)?;
            serde_json::to_value(r).map_err(|e| ActionError::Failed(e.to_string()))
        });
        let spec = ActionSpec {
            name: name.into(),
            description: description.trim().into(),
            tier,
            params,
            result: None,
        };
        self.actions.insert(name.into(), Entry { spec, handler });
        Ok(())
    }

    /// Records the JSON schema of the result of `name` (used to generate front-end
    /// types; dispatching is not affected).
    ///
    /// # Errors
    /// No action with this name.
    pub fn describe_result<R: JsonSchema>(&mut self, name: &str) -> Result<(), RegistryError> {
        let entry = self
            .actions
            .get_mut(name)
            .ok_or_else(|| RegistryError::NotRegistered(name.into()))?;
        entry.spec.result = Some(serde_json::to_value(schemars::schema_for!(R)).unwrap_or(Value::Null));
        Ok(())
    }

    /// Runs an action after checking the caller's tier.
    ///
    /// # Errors
    /// Unknown action, permission denied, invalid parameters, or the action's error.
    pub fn dispatch(
        &self,
        ctx: &C,
        caller: &Caller,
        name: &str,
        params: Value,
    ) -> Result<Value, ActionError> {
        let entry = self
            .actions
            .get(name)
            .ok_or_else(|| ActionError::Unknown(name.into()))?;
        if !caller.allows(entry.spec.tier) {
            return Err(ActionError::PermissionDenied {
                action: name.into(),
                needed: entry.spec.tier,
            });
        }
        (entry.handler)(ctx, params)
    }

    /// Spec of one action.
    #[must_use]
    pub fn spec(&self, name: &str) -> Option<&ActionSpec> {
        self.actions.get(name).map(|e| &e.spec)
    }

    /// All specs, sorted by name.
    pub fn specs(&self) -> impl Iterator<Item = &ActionSpec> {
        self.actions.values().map(|e| &e.spec)
    }

    /// Number of actions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// The catalogue as Markdown (`doc/features/actions.md`, checked in CI).
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut s = String::from(
            "# Actions\n\n<!-- Generated by `cargo xtask gen-docs` from the action registry. Do not edit. -->\n\n\
             Every action can be dispatched by name with JSON parameters (`host --stdio`, later the UI, \
             scripts and MCP). The tier is checked on every call; `safety` actions are always allowed.\n\n\
             | Action | Tier | Description |\n| --- | --- | --- |\n",
        );
        for spec in self.specs() {
            let summary = spec.description.lines().next().unwrap_or("");
            s.push_str(&format!(
                "| `{}` | {} | {} |\n",
                spec.name,
                spec.tier,
                summary.replace('|', "\\|")
            ));
        }
        s.push_str("\n## Parameters\n");
        for spec in self.specs() {
            s.push_str(&format!("\n### `{}`\n\n{}\n\n", spec.name, spec.description));
            let props = spec.params.get("properties").and_then(Value::as_object);
            match props {
                Some(p) if !p.is_empty() => {
                    let required: Vec<&str> = spec
                        .params
                        .get("required")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(Value::as_str).collect())
                        .unwrap_or_default();
                    s.push_str("| Parameter | Required | Description |\n| --- | --- | --- |\n");
                    for (k, v) in p {
                        let desc = v
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .replace('\n', " ");
                        let req = if required.contains(&k.as_str()) {
                            "yes"
                        } else {
                            "no"
                        };
                        s.push_str(&format!("| `{k}` | {req} | {desc} |\n"));
                    }
                }
                _ => s.push_str("No parameters.\n"),
            }
        }
        s
    }
}

/// Parameters of actions that take none.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, JsonSchema)]
    struct Echo {
        /// Text to echo.
        text: String,
        /// Times.
        #[serde(default)]
        times: Option<u32>,
    }

    #[test]
    fn results_can_be_described() {
        let mut r = registry();
        assert_eq!(r.spec("test.echo").unwrap().result, None);
        r.describe_result::<String>("test.echo").unwrap();
        assert_eq!(
            r.spec("test.echo").unwrap().result.as_ref().unwrap()["type"],
            "string"
        );
        assert_eq!(
            r.describe_result::<String>("test.nope"),
            Err(RegistryError::NotRegistered("test.nope".into()))
        );
        // not serialized when absent (older catalogues stay the same)
        let v = serde_json::to_value(r.spec("machine.stop").unwrap()).unwrap();
        assert!(v.get("result").is_none());
    }

    fn registry() -> Registry<()> {
        let mut r = Registry::new();
        r.register(
            "test.echo",
            "Echoes text.\nMore details.",
            Tier::Read,
            |_: &(), p: Echo| Ok(p.text.repeat(p.times.unwrap_or(1) as usize)),
        )
        .unwrap();
        r.register("machine.fire", "Fires.", Tier::Laser, |_: &(), _: NoParams| {
            Ok(true)
        })
        .unwrap();
        r.register("machine.stop", "Stops.", Tier::Safety, |_: &(), _: NoParams| {
            Ok(true)
        })
        .unwrap();
        r.register(
            "test.fail",
            "Fails.",
            Tier::Read,
            |_: &(), _: NoParams| -> Result<(), _> { Err(ActionError::Refused("no".into())) },
        )
        .unwrap();
        r
    }

    #[test]
    fn dispatch_and_errors() {
        let r = registry();
        let u = Caller::user();
        assert_eq!(
            r.dispatch(&(), &u, "test.echo", serde_json::json!({"text": "a", "times": 3}))
                .unwrap(),
            "aaa"
        );
        assert!(matches!(
            r.dispatch(&(), &u, "test.nope", Value::Null),
            Err(ActionError::Unknown(_))
        ));
        assert!(matches!(
            r.dispatch(&(), &u, "test.echo", serde_json::json!({})),
            Err(ActionError::InvalidParams(_))
        ));
        assert_eq!(
            r.dispatch(&(), &u, "test.fail", Value::Null),
            Err(ActionError::Refused("no".into()))
        );
        assert_eq!(r.dispatch(&(), &u, "machine.stop", Value::Null).unwrap(), true);
        assert!(
            r.dispatch(&(), &u, "machine.stop", serde_json::json!({"x": 1}))
                .is_err()
        );
    }

    #[test]
    fn tiers_are_checked_on_every_call() {
        let r = registry();
        let agent = Caller::new(CallerKind::Agent, "bot", Tier::Edit);
        let e = r.dispatch(&(), &agent, "machine.fire", Value::Null).unwrap_err();
        assert_eq!(
            e,
            ActionError::PermissionDenied {
                action: "machine.fire".into(),
                needed: Tier::Laser
            }
        );
        assert!(e.to_string().contains("laser"));
        // STOP is always allowed
        let reader = Caller::new(CallerKind::Macro, "m", Tier::Read);
        assert!(r.dispatch(&(), &reader, "machine.stop", Value::Null).is_ok());
        assert!(Caller::user().allows(Tier::Laser));
    }

    #[test]
    fn registration_invariants() {
        let mut r = registry();
        assert_eq!(
            r.register("test.echo", "x", Tier::Read, |_: &(), _: NoParams| Ok(())),
            Err(RegistryError::Duplicate("test.echo".into()))
        );
        for bad in ["echo", "Test.echo", "test.", ".x", "test.Echo", "a.b.c", "1a.b"] {
            assert_eq!(
                r.register(bad, "x", Tier::Read, |_: &(), _: NoParams| Ok(())),
                Err(RegistryError::BadName(bad.into()))
            );
        }
        assert_eq!(
            r.register("test.quiet", " ", Tier::Read, |_: &(), _: NoParams| Ok(())),
            Err(RegistryError::NoDescription("test.quiet".into()))
        );
        assert_eq!(r.len(), 4);
        assert!(!r.is_empty());
        for s in r.specs() {
            assert!(valid_name(&s.name) && !s.description.is_empty() && s.params.is_object());
        }
        assert_eq!(r.spec("test.echo").unwrap().tier, Tier::Read);
        assert!(format!("{r:?}").contains("test.echo"));
    }

    #[test]
    fn markdown_catalogue() {
        let md = registry().to_markdown();
        assert!(md.contains("| `test.echo` | read | Echoes text. |"));
        assert!(md.contains("| `text` | yes | Text to echo. |"));
        assert!(md.contains("| `times` | no | Times. |"));
        assert!(md.contains("No parameters."));
        for t in [
            Tier::Safety,
            Tier::Read,
            Tier::Ui,
            Tier::Edit,
            Tier::Machine,
            Tier::Laser,
        ] {
            assert_eq!(t.to_string(), t.as_str());
        }
        assert!(Registry::<()>::default().is_empty());
    }
}
