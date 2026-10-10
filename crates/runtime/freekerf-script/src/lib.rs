//! Embedded JavaScript for user macros and AI agents (PLAN §9.2, ADR 0011):
//! QuickJS through `rquickjs`, with the action registry as the only way out.
//!
//! The script sees the ECMAScript built-ins, `console.log` (to the run's log) and a
//! frozen `app` object generated from the registry: `app.document.selectAll()`
//! dispatches `document.select_all`, `app.view.openPanel({name: "layers"})`
//! dispatches `view.open_panel`. There is no file system, network, process,
//! module loader or timer. Every call goes through [`ScriptHost::dispatch`], where
//! the host checks the caller's tier; transactions and dry runs are the host's
//! (`freekerf-service`).
//!
//! Limits: wall-clock timeout, memory cap, stack cap and an instruction budget,
//! checked by QuickJS' interrupt handler, which also stops the script when the
//! host's cancel flag is set (STOP).

pub mod dts;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use freekerf_session::{ActionError, ActionSpec};
use rquickjs::{Context, Ctx, Function, Runtime, Value as JsValue};
use serde::Serialize;
use serde_json::Value;

/// What a script can reach: the actions and a log.
pub trait ScriptHost {
    /// The actions the script API is generated from.
    fn specs(&self) -> Vec<ActionSpec>;
    /// Runs an action for the script (the host checks the tier).
    ///
    /// # Errors
    /// The action's error, given to the script as an exception.
    fn dispatch(&self, action: &str, params: Value) -> Result<Value, ActionError>;
}

/// Limits of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Wall-clock time.
    pub timeout: Duration,
    /// Memory of the JavaScript heap, bytes.
    pub memory_bytes: usize,
    /// Native stack for the interpreter, bytes.
    pub stack_bytes: usize,
    /// Interrupt checks (about every 10 000 bytecode operations each).
    pub max_interrupts: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            memory_bytes: 64 << 20,
            stack_bytes: 512 << 10,
            max_interrupts: 2_000_000,
        }
    }
}

/// An action dispatched by a script.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionRecord {
    /// Action name.
    pub action: String,
    /// Parameters.
    pub params: Value,
    /// Error message when it failed.
    pub error: Option<String>,
}

/// The result of a run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScriptOutcome {
    /// The script's completion value as JSON (`null` when not serializable).
    pub value: Value,
    /// `console.log` lines.
    pub log: Vec<String>,
    /// Every action the script dispatched, in order.
    pub actions: Vec<ActionRecord>,
    /// The uncaught error, if any (the host rolls back its changes).
    pub error: Option<String>,
    /// Time taken.
    pub elapsed_ms: u64,
}

/// `snake_case` → `camelCase` (`select_all` → `selectAll`).
#[must_use]
pub fn camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut up = false;
    for c in name.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// The `app` object: a namespace per action prefix, a function per action. The
/// native dispatcher stays in this closure only.
fn prelude(specs: &[ActionSpec]) -> String {
    let mut table: Vec<(String, String, String)> = specs
        .iter()
        .filter_map(|s| {
            s.name
                .split_once('.')
                .map(|(ns, n)| (ns.to_owned(), camel(n), s.name.clone()))
        })
        .collect();
    table.sort();
    let entries: Vec<String> = table
        .iter()
        .map(|(ns, f, full)| format!("[{ns:?}, {f:?}, {full:?}]"))
        .collect();
    format!(
        r#"(function (dispatch, log) {{
  "use strict";
  const app = {{}};
  for (const [ns, fn, action] of [{entries}]) {{
    if (!app[ns]) app[ns] = {{}};
    app[ns][fn] = function (params) {{
      return JSON.parse(dispatch(action, JSON.stringify(params === undefined ? null : params)));
    }};
  }}
  for (const ns of Object.keys(app)) Object.freeze(app[ns]);
  Object.freeze(app);
  const text = (v) => typeof v === "string" ? v : (() => {{ try {{ return JSON.stringify(v); }} catch (e) {{ return String(v); }} }})();
  const consoleObject = Object.freeze({{ log: (...args) => log(args.map(text).join(" ")) }});
  Object.defineProperty(globalThis, "app", {{ value: app, writable: false, configurable: false }});
  Object.defineProperty(globalThis, "console", {{ value: consoleObject, writable: false, configurable: false }});
}})(globalThis.__fk_dispatch, globalThis.__fk_log);
delete globalThis.__fk_dispatch;
delete globalThis.__fk_log;
"#,
        entries = entries.join(", ")
    )
}

#[allow(clippy::needless_pass_by_value)] // used as `map_err(|e| ...)`
fn js_error_text(ctx: &Ctx<'_>, err: rquickjs::Error) -> String {
    if let rquickjs::Error::Exception = err {
        let v: JsValue = ctx.catch();
        if let Some(e) = v.as_exception() {
            let msg = e.message().unwrap_or_default();
            return if msg.is_empty() { "exception".into() } else { msg };
        }
        if let Some(s) = v.as_string().and_then(|s| s.to_string().ok()) {
            return s;
        }
        return format!("{v:?}");
    }
    err.to_string()
}

/// Runs `code` with `host`. The cancel flag stops it at the next interrupt check.
#[must_use]
#[allow(clippy::too_many_lines, clippy::needless_pass_by_value)] // the host is shared with the engine's callbacks
pub fn run(
    code: &str,
    host: Arc<dyn ScriptHost>,
    limits: &Limits,
    cancel: &Arc<AtomicBool>,
) -> ScriptOutcome {
    let started = Instant::now();
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let actions = Arc::new(Mutex::new(Vec::<ActionRecord>::new()));
    let finish = |value: Value, error: Option<String>| ScriptOutcome {
        value,
        log: log.lock().map(|l| l.clone()).unwrap_or_default(),
        actions: actions.lock().map(|a| a.clone()).unwrap_or_default(),
        error,
        #[allow(clippy::cast_possible_truncation)]
        elapsed_ms: started.elapsed().as_millis() as u64,
    };
    let Ok(rt) = Runtime::new() else {
        return finish(Value::Null, Some("cannot start the script engine".into()));
    };
    rt.set_memory_limit(limits.memory_bytes);
    rt.set_max_stack_size(limits.stack_bytes);
    let deadline = started + limits.timeout;
    let max = limits.max_interrupts;
    let flag = Arc::clone(cancel);
    let stopped = Arc::new(Mutex::new(None::<&'static str>));
    let why = Arc::clone(&stopped);
    let mut count = 0u64;
    rt.set_interrupt_handler(Some(Box::new(move || {
        count += 1;
        let reason = if flag.load(Ordering::SeqCst) {
            Some("cancelled")
        } else if Instant::now() > deadline {
            Some("timeout")
        } else if count > max {
            Some("instruction limit")
        } else {
            None
        };
        if let Some(r) = reason {
            if let Ok(mut w) = why.lock() {
                *w = Some(r);
            }
            return true;
        }
        false
    })));
    let Ok(context) = Context::full(&rt) else {
        return finish(Value::Null, Some("cannot create the script context".into()));
    };
    let specs = host.specs();
    let result: Result<Value, String> = context.with(|ctx| {
        let globals = ctx.globals();
        let records = Arc::clone(&actions);
        let host_ptr = Arc::clone(&host);
        let dispatch = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'_>, action: String, params: String| -> rquickjs::Result<String> {
                let params: Value = serde_json::from_str(&params).unwrap_or(Value::Null);
                let r = host_ptr.dispatch(&action, params.clone());
                if let Ok(mut recs) = records.lock() {
                    recs.push(ActionRecord {
                        action: action.clone(),
                        params,
                        error: r.as_ref().err().map(ToString::to_string),
                    });
                }
                match r {
                    Ok(v) => Ok(serde_json::to_string(&v).unwrap_or_else(|_| "null".into())),
                    Err(e) => Err(rquickjs::Exception::throw_message(
                        &ctx,
                        &format!("{action}: {e}"),
                    )),
                }
            },
        )
        .map_err(|e| e.to_string())?;
        let lines = Arc::clone(&log);
        let console = Function::new(ctx.clone(), move |line: String| {
            if let Ok(mut l) = lines.lock() {
                l.push(line);
            }
        })
        .map_err(|e| e.to_string())?;
        globals
            .set("__fk_dispatch", dispatch)
            .map_err(|e| e.to_string())?;
        globals.set("__fk_log", console).map_err(|e| e.to_string())?;
        ctx.eval::<(), _>(prelude(&specs))
            .map_err(|e| js_error_text(&ctx, e))?;
        let value: JsValue = ctx.eval(code).map_err(|e| js_error_text(&ctx, e))?;
        if value.is_undefined() {
            return Ok(Value::Null);
        }
        let json = ctx
            .json_stringify(value)
            .ok()
            .flatten()
            .and_then(|s| s.to_string().ok())
            .unwrap_or_else(|| "null".into());
        Ok(serde_json::from_str(&json).unwrap_or(Value::Null))
    });
    let reason = stopped.lock().ok().and_then(|r| *r);
    match (result, reason) {
        (Ok(v), _) => finish(v, None),
        (Err(_), Some(r)) => finish(Value::Null, Some(format!("script stopped: {r}"))),
        (Err(e), None) if e.contains("out of memory") => {
            finish(Value::Null, Some("script stopped: memory limit".into()))
        }
        (Err(e), None) => finish(Value::Null, Some(e)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn camel_case() {
        assert_eq!(camel("select_all"), "selectAll");
        assert_eq!(camel("get"), "get");
        assert_eq!(camel("open_panel_now"), "openPanelNow");
    }

    struct Echo;

    impl ScriptHost for Echo {
        fn specs(&self) -> Vec<ActionSpec> {
            vec![ActionSpec {
                name: "math.add_one".into(),
                description: "Adds one.".into(),
                tier: freekerf_session::Tier::Read,
                params: json!({}),
            }]
        }
        fn dispatch(&self, action: &str, params: Value) -> Result<Value, ActionError> {
            assert_eq!(action, "math.add_one");
            params["n"]
                .as_i64()
                .map(|n| json!(n + 1))
                .ok_or_else(|| ActionError::InvalidParams("n".into()))
        }
    }

    #[test]
    fn values_logs_and_errors() {
        let cancel = Arc::new(AtomicBool::new(false));
        let out = run(
            "console.log('hi', {a: 1}); app.math.addOne({n: 41})",
            Arc::new(Echo),
            &Limits::default(),
            &cancel,
        );
        assert_eq!(out.error, None);
        assert_eq!(out.value, json!(42));
        assert_eq!(out.log, ["hi {\"a\":1}"]);
        assert_eq!(out.actions.len(), 1);
        let bad = run("app.math.addOne({})", Arc::new(Echo), &Limits::default(), &cancel);
        assert!(bad.error.unwrap().contains("math.add_one: invalid parameters"));
        assert_eq!(bad.actions[0].error.as_deref(), Some("invalid parameters: n"));
        let caught = run(
            "try { app.math.addOne({}) } catch (e) { 'caught' }",
            Arc::new(Echo),
            &Limits::default(),
            &cancel,
        );
        assert_eq!(caught.value, json!("caught"));
        assert!(
            run(
                "throw new Error('boom')",
                Arc::new(Echo),
                &Limits::default(),
                &cancel
            )
            .error
            .unwrap()
            .contains("boom")
        );
        assert!(
            run("throw 'plain'", Arc::new(Echo), &Limits::default(), &cancel)
                .error
                .unwrap()
                .contains("plain")
        );
        assert!(
            run("syntax error here", Arc::new(Echo), &Limits::default(), &cancel)
                .error
                .is_some()
        );
        assert_eq!(
            run("undefined", Arc::new(Echo), &Limits::default(), &cancel).value,
            Value::Null
        );
        assert_eq!(
            run("(function(){})", Arc::new(Echo), &Limits::default(), &cancel).value,
            Value::Null
        );
    }
}
