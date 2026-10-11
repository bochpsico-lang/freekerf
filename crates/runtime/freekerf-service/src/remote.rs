//! The service on the local endpoint (`freekerf serve`, PLAN §9.1, §9.4): agents
//! (through `freekerf mcp`) and the Python client attach to the running instance
//! and use the same session as everyone else. Protocol: `doc/features/ipc.md`.
//!
//! * **Hello.** The first line is `{"hello": {"token": "...", "client": "agent" |
//!   "python_client", "name": "..."}}`. A wrong token is refused; agents are
//!   refused while `automation.mcp.enabled` is off. The caller's tier is its
//!   baseline from the settings ([`Service::baseline`]), never what it claims.
//! * **Requests** run on their own threads, so `stop` is answered while a script
//!   of the same client is still running.
//! * **Laser** actions need the person's confirmation ([`Service::confirm_laser`]);
//!   headless, they are refused.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use freekerf_ipc::{Connection, LineWriter, RemoteError, Server, token_matches};
use freekerf_session::{ActionError, Caller, CallerKind, Tier};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::Service;
use crate::permissions::RunGrants;
use crate::script::{Limits, ScriptRequest};

pub use freekerf_ipc::{Endpoint, IpcError};

/// The endpoint of the instance using the files in `home` (the per-user
/// directories when `None`, `$FREEKERF_HOME` overrides them).
///
/// # Errors
/// No home directory.
pub fn endpoint(home: Option<&std::path::Path>) -> Result<Endpoint, String> {
    let dirs = match home {
        Some(h) => freekerf_platform::Dirs::under(h),
        None => freekerf_platform::Dirs::user().map_err(|e| e.to_string())?,
    };
    Ok(Endpoint::in_dir(&dirs.runtime))
}

/// Version of the local protocol.
pub const PROTOCOL_VERSION: u32 = 1;

/// Message when an agent connects while MCP is off.
pub const MCP_DISABLED: &str = "MCP is disabled. The user can enable it in the app settings (Automation) or with `freekerf config set automation.mcp.enabled true`.";

/// How `serve` runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ServeOptions {
    /// Return once no client has been connected for this long (an instance
    /// started by `freekerf mcp`); `None` serves until the process ends.
    pub idle_exit: Option<Duration>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hello {
    token: String,
    client: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: Option<u32>,
}

fn action_error(e: &ActionError) -> RemoteError {
    let kind = serde_json::to_value(e)
        .ok()
        .and_then(|v| v.get("kind").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| "failed".into());
    RemoteError::new(kind, e.to_string())
}

/// Listens on `endpoint` and serves clients (blocks).
///
/// # Errors
/// Another instance is running, or the endpoint cannot be created.
pub fn serve(service: &Service, endpoint: &Endpoint, options: ServeOptions) -> Result<(), IpcError> {
    let server = Arc::new(Server::bind(endpoint)?);
    let clients = Arc::new(AtomicUsize::new(0));
    let last_seen = Arc::new(std::sync::Mutex::new(Instant::now()));
    let accepting = {
        let server = Arc::clone(&server);
        let service = service.clone();
        let clients = Arc::clone(&clients);
        let last_seen = Arc::clone(&last_seen);
        thread::Builder::new()
            .name("freekerf-ipc-accept".into())
            .spawn(move || {
                loop {
                    match server.accept() {
                        Ok(conn) => {
                            let service = service.clone();
                            let token = server.token().to_owned();
                            let clients = Arc::clone(&clients);
                            let last_seen = Arc::clone(&last_seen);
                            clients.fetch_add(1, Ordering::SeqCst);
                            let spawned =
                                thread::Builder::new()
                                    .name("freekerf-ipc-client".into())
                                    .spawn(move || {
                                        handle(&service, &token, conn);
                                        clients.fetch_sub(1, Ordering::SeqCst);
                                        *crate::lock(&last_seen) = Instant::now();
                                    });
                            if let Err(e) = spawned {
                                tracing::error!("cannot serve a client: {e}");
                            }
                        }
                        Err(e) => {
                            tracing::error!("accept failed: {e}");
                            thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            })?
    };
    match options.idle_exit {
        None => {
            let _ = accepting.join();
            Ok(())
        }
        Some(idle) => loop {
            thread::sleep(Duration::from_millis(100).min(idle));
            let quiet = clients.load(Ordering::SeqCst) == 0 && crate::lock(&last_seen).elapsed() >= idle;
            if quiet {
                tracing::info!("no clients for {idle:?}: exiting");
                server.close();
                return Ok(());
            }
        },
    }
}

/// One client: hello, then requests until it disconnects.
fn handle(service: &Service, token: &str, conn: Connection) {
    let Connection { mut reader, writer } = conn;
    let refuse = |kind: &str, message: &str| {
        let _ = writer.send(&json!({"error": RemoteError::new(kind, message).to_json()}));
    };
    let Ok(Some(hello)) = reader.read() else {
        return;
    };
    let Some(hello) = hello
        .get("hello")
        .cloned()
        .and_then(|h| serde_json::from_value::<Hello>(h).ok())
    else {
        refuse("protocol", "the first message must be {\"hello\": {...}}");
        return;
    };
    if !token_matches(&hello.token, token) {
        refuse("unauthorized", "wrong token");
        return;
    }
    if hello.version.is_some_and(|v| v > PROTOCOL_VERSION) {
        refuse(
            "unsupported",
            &format!(
                "protocol version {} is newer than this FreeKerf ({PROTOCOL_VERSION})",
                hello.version.unwrap_or(0)
            ),
        );
        return;
    }
    let kind = match hello.client.as_str() {
        "agent" => CallerKind::Agent,
        "python_client" => CallerKind::PythonClient,
        other => {
            refuse(
                "unsupported",
                &format!("unknown client kind `{other}` (agent, python_client)"),
            );
            return;
        }
    };
    let Some(tier) = service.baseline(kind) else {
        refuse("disabled", MCP_DISABLED);
        return;
    };
    let name = if hello.name.trim().is_empty() {
        hello.client.clone()
    } else {
        hello.name.trim().chars().take(64).collect()
    };
    let caller = Caller::new(kind, name, tier);
    tracing::info!(client = %caller.name, kind = ?kind, %tier, "client attached");
    let welcome = json!({"welcome": {
        "version": PROTOCOL_VERSION,
        "app_version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "headless": !service.human_present(),
        "tier": tier,
    }});
    if writer.send(&welcome).is_err() {
        return;
    }
    let closed = Arc::new(AtomicBool::new(false));
    while let Ok(Some(msg)) = reader.read() {
        if msg.get("bye").is_some() {
            break;
        }
        let id = msg.get("id").cloned().unwrap_or(Value::Null);
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("").to_owned();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let service = service.clone();
        let caller = caller.clone();
        let writer = writer.clone();
        let closed = Arc::clone(&closed);
        let spawned = thread::Builder::new()
            .name("freekerf-ipc-request".into())
            .spawn(move || {
                let answer = match request(&service, &caller, &method, params, &writer, &closed) {
                    Ok(result) => json!({"id": id, "result": result}),
                    Err(e) => json!({"id": id, "error": e.to_json()}),
                };
                let _ = writer.send(&answer);
            });
        if let Err(e) = spawned {
            tracing::error!("cannot run a request: {e}");
        }
    }
    closed.store(true, Ordering::SeqCst);
    tracing::info!(client = %caller.name, "client detached");
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DispatchParams {
    action: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunScriptParams {
    code: String,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    timeout_s: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchParams {
    #[serde(default)]
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateParams {
    #[serde(default)]
    path: Option<String>,
}

fn parse<T: for<'de> Deserialize<'de> + Default>(params: Value) -> Result<T, RemoteError> {
    if params.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(params).map_err(|e| RemoteError::new("invalid_params", e.to_string()))
}

fn parse_required<T: for<'de> Deserialize<'de>>(params: Value) -> Result<T, RemoteError> {
    serde_json::from_value(params).map_err(|e| RemoteError::new("invalid_params", e.to_string()))
}

fn laser_refused(action: &str) -> RemoteError {
    RemoteError::new(
        "refused",
        format!(
            "`{action}` can turn the laser on and needs a person to confirm it on the machine's screen; it was not confirmed (a headless instance refuses it)"
        ),
    )
}

fn request(
    service: &Service,
    caller: &Caller,
    method: &str,
    params: Value,
    writer: &LineWriter,
    closed: &Arc<AtomicBool>,
) -> Result<Value, RemoteError> {
    match method {
        "ping" => Ok(json!("pong")),
        "specs" => {
            serde_json::to_value(service.specs()).map_err(|e| RemoteError::new("failed", e.to_string()))
        }
        "api_dts" => Ok(Value::String(service.api_dts())),
        "search_api" => Ok(search_api(service, &parse::<SearchParams>(params)?)),
        "get_state" => get_state(service, caller, parse::<StateParams>(params)?.path.as_deref()),
        "dispatch" => {
            let p: DispatchParams = parse_required(params)?;
            let run = RunGrants::default();
            let tier = service
                .authorize(caller, None, &p.action, &run, true)
                .map_err(|e| action_error(&e))?;
            if tier == Tier::Laser && !service.confirm_laser(caller, &p.action, &p.params) {
                return Err(laser_refused(&p.action));
            }
            service
                .dispatch_authorized(caller, None, &p.action, p.params, &run, false)
                .map_err(|e| action_error(&e))
        }
        "run_script" => {
            let p: RunScriptParams = parse_required(params)?;
            let mut req = ScriptRequest::new(p.name.unwrap_or_else(|| caller.name.clone()), p.code);
            req.dry_run = p.dry_run;
            req.limits = Limits {
                timeout: Duration::from_secs(p.timeout_s.unwrap_or(30).clamp(1, 600)),
                ..Limits::default()
            };
            let confirmer = service.clone();
            let who = caller.clone();
            req.confirm_laser = Some(Arc::new(move |action, params| {
                confirmer.confirm_laser(&who, action, params)
            }));
            serde_json::to_value(service.run_script(caller, &req))
                .map_err(|e| RemoteError::new("failed", e.to_string()))
        }
        "stop" => {
            service.stop();
            Ok(Value::Null)
        }
        "subscribe" => {
            let events = service.subscribe();
            let writer = writer.clone();
            let closed = Arc::clone(closed);
            thread::Builder::new()
                .name("freekerf-ipc-events".into())
                .spawn(move || {
                    while !closed.load(Ordering::SeqCst) {
                        match events.recv_timeout(Duration::from_millis(200)) {
                            Ok(e) => {
                                if writer.send(&json!({"event": e.to_json()})).is_err() {
                                    break;
                                }
                            }
                            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                        }
                    }
                })
                .map_err(|e| RemoteError::new("failed", e.to_string()))?;
            Ok(Value::Null)
        }
        "screenshot" => Err(RemoteError::new(
            "unavailable",
            if service.human_present() {
                "screenshots are not implemented yet; read freekerf://state/view (get_state) instead"
            } else {
                "screenshots need the desktop app; this instance is headless. Use get_state (the semantic snapshot) instead"
            },
        )),
        other => Err(RemoteError::new(
            "unknown_method",
            format!("unknown method `{other}`"),
        )),
    }
}

/// Actions whose name or description contain every word of the query, those
/// matching in the name first.
fn search_api(service: &Service, p: &SearchParams) -> Value {
    let words: Vec<String> = p.query.split_whitespace().map(str::to_lowercase).collect();
    let limit = p.limit.unwrap_or(20).clamp(1, 200);
    let mut found: Vec<(usize, freekerf_session::ActionSpec)> = service
        .specs()
        .into_iter()
        .filter_map(|s| {
            let name = format!("{} {}", s.name, freekerf_script::camel(&s.name)).to_lowercase();
            let description = s.description.to_lowercase();
            let mut in_name = 0;
            for w in &words {
                if name.contains(w.as_str()) {
                    in_name += 1;
                } else if !description.contains(w.as_str()) {
                    return None;
                }
            }
            Some((in_name, s))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    let results: Vec<Value> = found
        .iter()
        .take(limit)
        .map(|(_, s)| {
            json!({
                "action": s.name,
                "tier": s.tier,
                "description": s.description,
                "signature": freekerf_script::dts::signature(s),
                "params": s.params,
            })
        })
        .collect();
    Value::Array(results)
}

/// The semantic snapshot: session (view, selection, document summary, history),
/// machine, last console rows; `path` selects a part (`session.view.mode`).
fn get_state(service: &Service, caller: &Caller, path: Option<&str>) -> Result<Value, RemoteError> {
    let read = |action: &str, params: Value| {
        service
            .dispatch(caller, action, params)
            .map_err(|e| action_error(&e))
    };
    let state = json!({
        "session": read("session.snapshot", Value::Null)?,
        "machine": read("state.get", Value::Null)?,
        "console": read("console.get", json!({"last": 50}))?,
        "headless": !service.human_present(),
    });
    let Some(path) = path.filter(|p| !p.is_empty()) else {
        return Ok(state);
    };
    path.split('.')
        .try_fold(&state, |v, part| match v {
            Value::Array(items) => part.parse::<usize>().ok().and_then(|i| items.get(i)),
            _ => v.get(part),
        })
        .cloned()
        .ok_or_else(|| RemoteError::new("invalid_params", format!("no state at `{path}`")))
}
