//! The MCP server of FreeKerf (PLAN §9.3, ADR 0012): JSON-RPC 2.0 over stdio
//! (one message per line), proxied to the running app through the local endpoint.
//! It has no logic of its own: the app checks tiers, runs scripts and refuses
//! laser actions without a person.
//!
//! * **Tools** (fixed; new actions never add tools): `run_script(code, dry_run?)`,
//!   `get_state(path?)`, `screenshot()`, `search_api(query, limit?)`, `stop()`.
//! * **Resources:** `freekerf://state/view`, `freekerf://state/document`,
//!   `freekerf://state/machine`, `freekerf://state/console`, `freekerf://api.d.ts`;
//!   subscriptions send `notifications/resources/updated` when they change (the
//!   machine and the console at most every [`THROTTLE`]).
//! * Requests run on their own threads, so `stop` is answered while a script runs.

use std::collections::{BTreeSet, HashMap};
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use freekerf_ipc::{Client, RemoteError};
pub use freekerf_ipc::{Endpoint, IpcError};
use serde_json::{Value, json};

/// MCP protocol versions this server speaks, newest first.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// Shortest time between two updates of the machine or console resources.
pub const THROTTLE: Duration = Duration::from_millis(500);

/// Instructions given to the agent at initialization.
pub const INSTRUCTIONS: &str = "FreeKerf designs laser jobs and drives laser machines. Read the app with get_state (or the freekerf://state/* resources); act with run_script, which runs JavaScript against the `app` API generated from the app's actions (find them with search_api, or read freekerf://api.d.ts). A script's document changes are one undo step and are rolled back on error; use dry_run to preview. Your tier is limited by the user's settings; actions that can turn the laser on always need a person to confirm them on the machine. stop() stops the machine and any running script at once.";

/// The running app, as seen by the MCP server.
pub trait App: Send + Sync {
    /// One request to the app (`run_script`, `get_state`...).
    ///
    /// # Errors
    /// The app's error.
    fn call(&self, method: &str, params: Value) -> Result<Value, RemoteError>;
    /// Events after a `subscribe` call.
    fn events(&self) -> Receiver<Value>;
    /// Whether the app has no person in front of it (no screenshots).
    fn headless(&self) -> bool;
}

impl App for Client {
    fn call(&self, method: &str, params: Value) -> Result<Value, RemoteError> {
        Self::call(self, method, params)
    }
    fn events(&self) -> Receiver<Value> {
        Self::events(self).clone()
    }
    fn headless(&self) -> bool {
        self.welcome()["headless"].as_bool().unwrap_or(true)
    }
}

/// Connects to the app once the client has said who it is (`clientInfo.name`).
pub type Connect = Box<dyn FnOnce(&str) -> Result<Arc<dyn App>, String> + Send>;

/// Attach or launch (PLAN §9.4): connects to the instance on `endpoint` as the
/// agent `name`; when none is running, calls `launch` (which starts one) and
/// connects as soon as it listens, waiting at most `wait`. A refusal (MCP
/// disabled, wrong token) is returned at once, never retried.
///
/// # Errors
/// The refusal, the launch error, or the last connection error.
pub fn attach_or_launch(
    endpoint: &Endpoint,
    name: &str,
    launch: impl FnOnce() -> std::io::Result<()>,
    wait: Duration,
) -> Result<Arc<dyn App>, String> {
    let hello = json!({"client": "agent", "name": name});
    match Client::connect(endpoint, hello.clone()) {
        Ok(c) => return Ok(Arc::new(c)),
        Err(e @ (IpcError::Refused { .. } | IpcError::Protocol(_))) => return Err(e.to_string()),
        Err(e) => tracing::info!("no running instance ({e}): launching one"),
    }
    launch().map_err(|e| format!("cannot start FreeKerf: {e}"))?;
    Client::connect_retry(endpoint, &hello, wait)
        .map(|c| Arc::new(c) as Arc<dyn App>)
        .map_err(|e| e.to_string())
}

const VIEW: &str = "freekerf://state/view";
const DOCUMENT: &str = "freekerf://state/document";
const MACHINE: &str = "freekerf://state/machine";
const CONSOLE: &str = "freekerf://state/console";
const API: &str = "freekerf://api.d.ts";

fn tools() -> Value {
    json!([
        {
            "name": "run_script",
            "title": "Run a script",
            "description": "Runs JavaScript against the FreeKerf app. `app.<namespace>.<action>(params)` dispatches one action (for example app.document.selectAll(), app.view.openPanel({name: 'layers'})); console.log writes to the log. The script's value, log, dispatched actions and error are returned. Document and view changes form one undo step and are rolled back on an error. dry_run runs against a copy and simulates machine, laser and file actions. Every call is checked against your tier; laser actions need a person's confirmation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": {"type": "string", "description": "JavaScript source; the value of the last expression is returned."},
                    "dry_run": {"type": "boolean", "description": "Run against a copy of the session and apply nothing.", "default": false}
                },
                "required": ["code"]
            }
        },
        {
            "name": "get_state",
            "title": "Read the app state",
            "description": "The semantic snapshot of the app: session (view, selection, document layers and objects, undo history), machine (status, position, loaded program, issues) and the last console rows. `path` selects a part, e.g. `session.view.mode` or `machine.machine.status`.",
            "inputSchema": {
                "type": "object",
                "properties": {"path": {"type": "string", "description": "Dotted path into the state (array items by index)."}}
            },
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "screenshot",
            "title": "Screenshot of the app window",
            "description": "An image of the app window (desktop app only; a headless instance answers with an error). Prefer get_state, the semantic snapshot.",
            "inputSchema": {"type": "object", "properties": {}},
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "search_api",
            "title": "Search the script API",
            "description": "Finds actions whose name or description contain every word of `query`, with their script signature, tier and parameter schema.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 200}
                },
                "required": ["query"]
            },
            "annotations": {"readOnlyHint": true}
        },
        {
            "name": "stop",
            "title": "STOP",
            "description": "Stops any running script and soft-resets the machine (laser off, motion stopped). Always allowed.",
            "inputSchema": {"type": "object", "properties": {}}
        }
    ])
}

fn resources(headless: bool) -> Value {
    let mut list = vec![
        json!({"uri": VIEW, "name": "view", "title": "What the app shows", "description": "Mode, open panels and dialogs, viewport, selection.", "mimeType": "application/json"}),
        json!({"uri": DOCUMENT, "name": "document", "title": "The document", "description": "Layers and objects (without geometry), undo/redo labels, unsaved changes.", "mimeType": "application/json"}),
        json!({"uri": MACHINE, "name": "machine", "title": "The machine", "description": "Connection, status, position, loaded program, issues.", "mimeType": "application/json"}),
        json!({"uri": CONSOLE, "name": "console", "title": "The console", "description": "The last console rows (commands, answers, messages).", "mimeType": "application/json"}),
        json!({"uri": API, "name": "api.d.ts", "title": "Script API declarations", "description": "TypeScript declarations of the `app` object used by run_script.", "mimeType": "text/plain"}),
    ];
    if !headless {
        list.push(json!({"uri": "freekerf://state/screenshot", "name": "screenshot", "title": "Screenshot", "mimeType": "image/png"}));
    }
    Value::Array(list)
}

/// Which resources an app event changes.
fn touched(event: &Value) -> &'static [&'static str] {
    let e = &event["event"];
    match e["event"].as_str() {
        Some("session") => match e["change"].as_str() {
            Some("view" | "selection") => &[VIEW],
            Some("document") => &[DOCUMENT, VIEW],
            Some("history") => &[DOCUMENT],
            _ => &[VIEW, DOCUMENT],
        },
        Some("host") => {
            let d = &e["data"];
            if d.get("Console").is_some() || d.as_str() == Some("ConsoleCleared") {
                &[CONSOLE]
            } else {
                &[MACHINE]
            }
        }
        Some("connected" | "disconnected" | "program_loaded") => &[MACHINE],
        _ => &[],
    }
}

struct Shared {
    out: Mutex<Box<dyn Write + Send>>,
    app: Mutex<Option<Arc<dyn App>>>,
    subscribed: Mutex<BTreeSet<String>>,
    watching: Mutex<bool>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    fn send(&self, msg: &Value) {
        let mut out = lock(&self.out);
        let line = msg.to_string();
        if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
            tracing::warn!("cannot write to the MCP client");
        }
    }

    fn app(&self) -> Result<Arc<dyn App>, RpcError> {
        lock(&self.app)
            .clone()
            .ok_or_else(|| RpcError::new(-32002, "not initialized"))
    }
}

#[derive(Debug)]
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Serves one MCP client on `input`/`output` until the input ends.
///
/// # Errors
/// Reading the input fails.
pub fn serve(
    connect: Connect,
    input: impl BufRead,
    output: impl Write + Send + 'static,
) -> std::io::Result<()> {
    let shared = Arc::new(Shared {
        out: Mutex::new(Box::new(output)),
        app: Mutex::new(None),
        subscribed: Mutex::new(BTreeSet::new()),
        watching: Mutex::new(false),
    });
    let mut connect = Some(connect);
    let mut workers = Vec::new();
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                shared.send(&json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}}));
                continue;
            }
        };
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("").to_owned();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = id else {
            // a notification (initialized, cancelled...): nothing to answer
            continue;
        };
        if method.is_empty() {
            // an answer to a request of ours (we send none)
            continue;
        }
        if method == "initialize" {
            // synchronous: nothing else is valid before it
            let answer = initialize(&shared, connect.take(), &params);
            respond(&shared, &id, answer);
            continue;
        }
        let shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("freekerf-mcp-request".into())
            .spawn(move || {
                let answer = handle(&shared, &method, &params);
                respond(&shared, &id, answer);
            })?;
        workers.push(worker);
        workers.retain(|w| !w.is_finished());
    }
    for w in workers {
        let _ = w.join();
    }
    Ok(())
}

fn respond(shared: &Shared, id: &Value, answer: Result<Value, RpcError>) {
    shared.send(&match answer {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": e.code, "message": e.message}}),
    });
}

fn initialize(shared: &Shared, connect: Option<Connect>, params: &Value) -> Result<Value, RpcError> {
    let Some(connect) = connect else {
        return Err(RpcError::new(-32600, "already initialized"));
    };
    let name = params["clientInfo"]["name"].as_str().unwrap_or("mcp-client");
    let app = connect(name).map_err(|e| RpcError::new(-32603, e))?;
    let headless = app.headless();
    *lock(&shared.app) = Some(app);
    let asked = params["protocolVersion"].as_str().unwrap_or("");
    let version = PROTOCOL_VERSIONS
        .iter()
        .find(|v| **v == asked)
        .unwrap_or(&PROTOCOL_VERSIONS[0]);
    Ok(json!({
        "protocolVersion": version,
        "capabilities": {
            "tools": {"listChanged": false},
            "resources": {"subscribe": true, "listChanged": false}
        },
        "serverInfo": {"name": "freekerf", "title": "FreeKerf", "version": env!("CARGO_PKG_VERSION")},
        "instructions": if headless {
            format!("{INSTRUCTIONS} This instance is headless: no screenshots, and laser actions are refused.")
        } else {
            INSTRUCTIONS.to_owned()
        }
    }))
}

fn handle(shared: &Arc<Shared>, method: &str, params: &Value) -> Result<Value, RpcError> {
    if method != "ping" {
        // only ping is valid before initialize
        shared.app()?;
    }
    match method {
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => call_tool(&shared.app()?, params),
        "resources/list" => Ok(json!({"resources": resources(shared.app()?.headless())})),
        "resources/templates/list" => Ok(json!({"resourceTemplates": []})),
        "resources/read" => read_resource(&shared.app()?, params["uri"].as_str().unwrap_or("")),
        "resources/subscribe" => {
            let uri = params["uri"].as_str().unwrap_or("").to_owned();
            if ![VIEW, DOCUMENT, MACHINE, CONSOLE].contains(&uri.as_str()) {
                return Err(RpcError::new(-32602, format!("cannot subscribe to `{uri}`")));
            }
            watch(shared)?;
            lock(&shared.subscribed).insert(uri);
            Ok(json!({}))
        }
        "resources/unsubscribe" => {
            lock(&shared.subscribed).remove(params["uri"].as_str().unwrap_or(""));
            Ok(json!({}))
        }
        "prompts/list" => Ok(json!({"prompts": []})),
        other => Err(RpcError::new(-32601, format!("method not found: {other}"))),
    }
}

fn text_result(value: &Value, is_error: bool) -> Value {
    let text = if let Value::String(s) = value {
        s.clone()
    } else {
        serde_json::to_string_pretty(value).unwrap_or_default()
    };
    let mut result = json!({"content": [{"type": "text", "text": text}], "isError": is_error});
    if value.is_object() {
        result["structuredContent"] = value.clone();
    }
    result
}

fn call_tool(app: &Arc<dyn App>, params: &Value) -> Result<Value, RpcError> {
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let name = params["name"].as_str().unwrap_or("");
    let answer = match name {
        "run_script" => {
            let Some(code) = args["code"].as_str() else {
                return Err(RpcError::new(-32602, "run_script needs `code`"));
            };
            let dry_run = args["dry_run"].as_bool().unwrap_or(false);
            app.call("run_script", json!({"code": code, "dry_run": dry_run}))
                .map(|report| {
                    let failed = !report["error"].is_null();
                    (report, failed)
                })
        }
        "get_state" => app
            .call("get_state", json!({"path": args.get("path").cloned().unwrap_or(Value::Null)}))
            .map(|v| (v, false)),
        "screenshot" => app.call("screenshot", Value::Null).map(|v| (v, false)),
        "search_api" => app
            .call(
                "search_api",
                json!({"query": args["query"].as_str().unwrap_or(""), "limit": args.get("limit").cloned().unwrap_or(Value::Null)}),
            )
            .map(|v| (json!({"results": v}), false)),
        "stop" => app.call("stop", Value::Null).map(|_| (json!("stopped"), false)),
        other => return Err(RpcError::new(-32602, format!("unknown tool `{other}`"))),
    };
    Ok(match answer {
        Ok((value, failed)) => text_result(&value, failed),
        // the app's refusals are tool errors the agent can read, not protocol errors
        Err(e) => text_result(&Value::String(format!("{}: {}", e.kind, e.message)), true),
    })
}

fn state_part(app: &Arc<dyn App>, uri: &str) -> Result<Value, RemoteError> {
    let state = app.call("get_state", Value::Null)?;
    let s = &state["session"];
    Ok(match uri {
        VIEW => json!({"revision": s["revision"], "view": s["view"], "selection": s["selection"]}),
        DOCUMENT => json!({
            "revision": s["revision"],
            "dirty": s["dirty"],
            "document": s["document"],
            "undo": s["undo"],
            "redo": s["redo"],
        }),
        MACHINE => state["machine"].clone(),
        _ => state["console"].clone(),
    })
}

fn read_resource(app: &Arc<dyn App>, uri: &str) -> Result<Value, RpcError> {
    let (text, mime) = match uri {
        API => (
            app.call("api_dts", Value::Null)
                .map_err(|e| RpcError::new(-32603, e.message))?
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            "text/plain",
        ),
        VIEW | DOCUMENT | MACHINE | CONSOLE => {
            let v = state_part(app, uri).map_err(|e| RpcError::new(-32603, e.message))?;
            (
                serde_json::to_string_pretty(&v).unwrap_or_default(),
                "application/json",
            )
        }
        "freekerf://state/screenshot" => {
            let e = app
                .call("screenshot", Value::Null)
                .err()
                .map_or_else(|| "no screenshot".to_owned(), |e| e.message);
            return Err(RpcError::new(-32603, e));
        }
        other => return Err(RpcError::new(-32002, format!("resource not found: {other}"))),
    };
    Ok(json!({"contents": [{"uri": uri, "mimeType": mime, "text": text}]}))
}

/// Starts forwarding app events as resource updates (once).
fn watch(shared: &Arc<Shared>) -> Result<(), RpcError> {
    let mut watching = lock(&shared.watching);
    if *watching {
        return Ok(());
    }
    let app = shared.app()?;
    app.call("subscribe", Value::Null)
        .map_err(|e| RpcError::new(-32603, e.message))?;
    let events = app.events();
    let weak = Arc::downgrade(shared);
    thread::Builder::new()
        .name("freekerf-mcp-events".into())
        .spawn(move || {
            let mut last: HashMap<&'static str, Instant> = HashMap::new();
            let mut due: BTreeSet<&'static str> = BTreeSet::new();
            loop {
                let event = events.recv_timeout(THROTTLE / 4);
                let Some(shared) = weak.upgrade() else { break };
                match event {
                    Ok(e) => due.extend(touched(&e).iter().copied()),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
                let now = Instant::now();
                let subscribed = lock(&shared.subscribed).clone();
                due.retain(|uri| {
                    if !subscribed.contains(*uri) {
                        return false;
                    }
                    let throttled = matches!(*uri, MACHINE | CONSOLE);
                    if throttled && last.get(uri).is_some_and(|t| now.duration_since(*t) < THROTTLE) {
                        return true;
                    }
                    last.insert(uri, now);
                    shared.send(&json!({"jsonrpc": "2.0", "method": "notifications/resources/updated", "params": {"uri": uri}}));
                    false
                });
            }
        })
        .map_err(|e| RpcError::new(-32603, e.to_string()))?;
    *watching = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_touch_resources() {
        let e = |v: Value| json!({"event": v});
        assert_eq!(
            touched(&e(json!({"event": "session", "change": "view"}))),
            &[VIEW]
        );
        assert_eq!(
            touched(&e(json!({"event": "session", "change": "document"}))),
            &[DOCUMENT, VIEW]
        );
        assert_eq!(
            touched(&e(json!({"event": "session", "change": "history"}))),
            &[DOCUMENT]
        );
        assert_eq!(touched(&e(json!({"event": "session"}))), &[VIEW, DOCUMENT]);
        assert_eq!(
            touched(&e(json!({"event": "host", "data": {"Console": {}}}))),
            &[CONSOLE]
        );
        assert_eq!(
            touched(&e(json!({"event": "host", "data": "ConsoleCleared"}))),
            &[CONSOLE]
        );
        assert_eq!(
            touched(&e(json!({"event": "host", "data": {"StatusChanged": {}}}))),
            &[MACHINE]
        );
        assert_eq!(touched(&e(json!({"event": "connected"}))), &[MACHINE]);
        assert!(touched(&e(json!({"event": "settings_changed"}))).is_empty());
    }

    #[test]
    fn tool_and_resource_lists() {
        let names: Vec<String> = tools()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            names,
            ["run_script", "get_state", "screenshot", "search_api", "stop"]
        );
        assert_eq!(resources(true).as_array().unwrap().len(), 5);
        assert_eq!(resources(false).as_array().unwrap().len(), 6);
        let r = text_result(&json!({"a": 1}), false);
        assert_eq!(r["structuredContent"]["a"], 1);
        assert!(text_result(&json!("x"), true)["structuredContent"].is_null());
    }
}
