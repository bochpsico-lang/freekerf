//! The MCP server against a headless app on the local endpoint (PLAN §9.6): the
//! fixed tool set, run_script reaching every namespace (machine on the built-in
//! emulator), resources and notifications, tiers per call, laser refused.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_pass_by_value,
    missing_docs
)]

use std::io::{BufReader, Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, unbounded};
use freekerf_ipc::{Client, Endpoint};
use freekerf_mcp::{App, Connect, serve};
use freekerf_service::remote::{ServeOptions, serve as serve_app};
use freekerf_service::{Service, Stores};
use freekerf_settings::Settings;
use serde_json::{Value, json};

/// Bytes in from a channel (the MCP client's writes).
struct ChanReader {
    rx: Receiver<Vec<u8>>,
    buf: Vec<u8>,
}

impl Read for ChanReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.buf.is_empty() {
            match self.rx.recv() {
                Ok(b) => self.buf = b,
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len());
        out[..n].copy_from_slice(&self.buf[..n]);
        self.buf.drain(..n);
        Ok(n)
    }
}

/// Lines out to a channel (what the client reads).
struct ChanWriter {
    tx: Sender<Value>,
    pending: Vec<u8>,
}

impl Write for ChanWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.pending.extend_from_slice(b);
        while let Some(i) = self.pending.iter().position(|&c| c == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=i).collect();
            let _ = self.tx.send(serde_json::from_slice(&line).unwrap());
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// An MCP client talking to `serve` in memory.
struct Mcp {
    to: Sender<Vec<u8>>,
    from: Receiver<Value>,
    next: u64,
    notifications: Vec<Value>,
}

impl Mcp {
    fn start(connect: Connect) -> Self {
        let (to, rx) = unbounded();
        let (tx, from) = unbounded();
        thread::spawn(move || {
            serve(
                connect,
                BufReader::new(ChanReader { rx, buf: Vec::new() }),
                ChanWriter {
                    tx,
                    pending: Vec::new(),
                },
            )
        });
        Self {
            to,
            from,
            next: 1,
            notifications: Vec::new(),
        }
    }

    fn send(&self, msg: &Value) {
        let mut line = msg.to_string().into_bytes();
        line.push(b'\n');
        self.to.send(line).unwrap();
    }

    /// Sends a request and returns its `result` (or the whole error message).
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let msg = self.from.recv_timeout(Duration::from_secs(20)).unwrap();
            if msg["id"] == id {
                return msg.get("result").cloned().unwrap_or(msg);
            }
            self.notifications.push(msg);
        }
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": arguments}))
    }

    fn wait_notification(&mut self, uri: &str) {
        let seen = |n: &Value| n["method"] == "notifications/resources/updated" && n["params"]["uri"] == uri;
        if self.notifications.iter().any(seen) {
            return;
        }
        loop {
            let msg = self.from.recv_timeout(Duration::from_secs(10)).unwrap();
            if seen(&msg) {
                return;
            }
            self.notifications.push(msg);
        }
    }
}

fn app(home: &Path, f: impl FnOnce(&mut Settings)) -> Endpoint {
    let mut s = Settings::default();
    f(&mut s);
    std::fs::write(home.join("settings.toml"), s.to_toml().unwrap()).unwrap();
    let service = Service::new(Stores::files(Some(home)).unwrap()).unwrap();
    let endpoint = Endpoint::in_dir(home);
    let ep = endpoint.clone();
    thread::spawn(move || serve_app(&service, &ep, ServeOptions::default()));
    endpoint
}

fn connector(endpoint: Endpoint) -> Connect {
    Box::new(move |name: &str| {
        let c = Client::connect_retry(
            &endpoint,
            &json!({"client": "agent", "name": name}),
            Duration::from_secs(5),
        )
        .map_err(|e| e.to_string())?;
        Ok(Arc::new(c) as Arc<dyn App>)
    })
}

fn initialize(mcp: &mut Mcp) -> Value {
    let init = mcp.request(
        "initialize",
        json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test-agent", "version": "1"}}),
    );
    mcp.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    init
}

fn text(result: &Value) -> String {
    result["content"][0]["text"].as_str().unwrap().to_owned()
}

#[test]
fn tools_resources_and_notifications() {
    let home = tempfile::tempdir().unwrap();
    let endpoint = app(home.path(), |s| {
        s.automation.mcp.enabled = true;
        s.automation.mcp.allow_machine = true;
    });
    let mut mcp = Mcp::start(connector(endpoint));
    // nothing before initialize
    assert_eq!(mcp.request("tools/list", json!({}))["error"]["code"], -32002);
    let init = initialize(&mut mcp);
    assert_eq!(init["protocolVersion"], "2025-06-18");
    assert_eq!(init["serverInfo"]["name"], "freekerf");
    assert!(init["instructions"].as_str().unwrap().contains("headless"));
    assert_eq!(init["capabilities"]["resources"]["subscribe"], true);
    assert!(
        mcp.request("initialize", json!({}))["error"]["message"]
            .as_str()
            .unwrap()
            .contains("already")
    );
    assert_eq!(mcp.request("ping", json!({})), json!({}));

    // the fixed tool set
    let tools = mcp.request("tools/list", json!({}));
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["run_script", "get_state", "screenshot", "search_api", "stop"]
    );

    // resources, and a notification when the view changes
    let list = mcp.request("resources/list", json!({}));
    assert_eq!(
        list["resources"].as_array().unwrap().len(),
        5,
        "no screenshot headless"
    );
    let api = mcp.request("resources/read", json!({"uri": "freekerf://api.d.ts"}));
    assert!(
        api["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("interface AppView")
    );
    let view = mcp.request("resources/read", json!({"uri": "freekerf://state/view"}));
    let view: Value = serde_json::from_str(view["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(view["view"]["mode"], "design");
    assert_eq!(
        mcp.request("resources/read", json!({"uri": "freekerf://nope"}))["error"]["code"],
        -32002
    );
    assert_eq!(
        mcp.request("resources/subscribe", json!({"uri": "freekerf://nope"}))["error"]["code"],
        -32602
    );
    assert_eq!(
        mcp.request("resources/subscribe", json!({"uri": "freekerf://state/view"})),
        json!({})
    );
    assert_eq!(
        mcp.request("resources/subscribe", json!({"uri": "freekerf://state/machine"})),
        json!({})
    );
    let r = mcp.tool(
        "run_script",
        json!({"code": "app.view.openPanel({name: 'layers'}); 'ok'"}),
    );
    assert_eq!(r["isError"], false, "{r}");
    mcp.wait_notification("freekerf://state/view");

    // run_script reaches every namespace (the machine is the built-in emulator)
    let every = r#"
        const out = {};
        out.console = Array.isArray(app.console.get({last: 5}));
        const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="9mm" height="9mm"><rect width="5" height="5" stroke="red" fill="none"/></svg>';
        out.document = app.document.importData({format: 'svg', text: svg}).objects.length;
        out.job = app.job.fromDocument() !== undefined;
        out.jobInfo = app.job.info().lines > 0;
        out.machine = app.machine.connect({emulator: true}) === null;
        out.status = app.state.get().machine.status;
        app.machine.disconnect();
        out.session = app.session.snapshot().document.objects.length;
        out.settings = typeof app.settings.get({key: 'machine.baud'});
        out.view = app.view.get().mode;
        out.history = typeof app.history.undo;
        out"#;
    let r = mcp.tool("run_script", json!({"code": every}));
    assert_eq!(r["isError"], false, "{}", text(&r));
    let v = &r["structuredContent"]["value"];
    assert_eq!(v["console"], true);
    assert_eq!(v["document"], 1);
    assert_eq!(v["job"], true);
    assert_eq!(v["jobInfo"], true, "{v}");
    assert_eq!(v["machine"], true);
    assert_eq!(v["session"], 1);
    assert_eq!(v["settings"], "number");
    assert_eq!(v["view"], "design");
    mcp.wait_notification("freekerf://state/machine");
    // one undo step for the whole script (opening a panel is not a step)
    let undo = mcp.tool("get_state", json!({"path": "session.undo"}));
    assert_eq!(
        undo["content"][0]["text"]
            .as_str()
            .unwrap()
            .matches("script test-agent")
            .count(),
        1,
        "{undo}"
    );

    // tiers per call: laser is refused (headless), even inside a script
    let r = mcp.tool("run_script", json!({"code": "app.job.run({})"}));
    assert_eq!(r["isError"], true);
    let report = &r["structuredContent"];
    assert!(
        report["error"].as_str().unwrap().contains("needs tier `laser`"),
        "{report}"
    );
    // a dry run applies nothing
    let r = mcp.tool(
        "run_script",
        json!({"code": "app.document.delete(); app.machine.home(); 1", "dry_run": true}),
    );
    assert_eq!(r["structuredContent"]["simulated"], json!(["machine.home"]));

    // the other tools
    let s = mcp.tool("search_api", json!({"query": "open panel"}));
    assert_eq!(s["structuredContent"]["results"][0]["action"], "view.open_panel");
    let shot = mcp.tool("screenshot", json!({}));
    assert_eq!(shot["isError"], true);
    assert!(text(&shot).contains("headless"));
    assert_eq!(
        text(&mcp.tool("get_state", json!({"path": "session.view.mode"}))),
        "design"
    );
    assert_eq!(mcp.tool("get_state", json!({"path": "no.such"}))["isError"], true);
    assert_eq!(text(&mcp.tool("stop", json!({}))), "stopped");
    assert_eq!(
        mcp.request("tools/call", json!({"name": "nope"}))["error"]["code"],
        -32602
    );
    assert_eq!(
        mcp.request("tools/call", json!({"name": "run_script", "arguments": {}}))["error"]["code"],
        -32602
    );
    assert_eq!(mcp.request("nope/nope", json!({}))["error"]["code"], -32601);
    assert_eq!(
        mcp.request("resources/unsubscribe", json!({"uri": "freekerf://state/view"})),
        json!({})
    );
    assert_eq!(
        mcp.request("resources/templates/list", json!({})),
        json!({"resourceTemplates": []})
    );
    assert_eq!(mcp.request("prompts/list", json!({})), json!({"prompts": []}));
}

#[test]
fn stop_answers_while_a_script_runs() {
    let home = tempfile::tempdir().unwrap();
    let endpoint = app(home.path(), |s| s.automation.mcp.enabled = true);
    let mut mcp = Mcp::start(connector(endpoint));
    let init = initialize(&mut mcp);
    assert_eq!(init["serverInfo"]["name"], "freekerf", "{init}");
    assert_eq!(
        mcp.request("resources/subscribe", json!({"uri": "freekerf://state/view"})),
        json!({})
    );
    mcp.send(&json!({"jsonrpc": "2.0", "id": 900, "method": "tools/call", "params": {"name": "run_script", "arguments": {"code": "app.view.setMode({mode: 'produce'}); while (true) {}"}}}));
    mcp.wait_notification("freekerf://state/view");
    assert_eq!(text(&mcp.tool("stop", json!({}))), "stopped");
    let stuck = loop {
        let msg = mcp.from.recv_timeout(Duration::from_secs(10)).unwrap();
        if msg["id"] == 900 {
            break msg;
        }
    };
    assert_eq!(stuck["result"]["isError"], true);
    assert_eq!(
        stuck["result"]["structuredContent"]["error"],
        "script stopped: cancelled"
    );
}

#[test]
fn a_disabled_app_fails_initialize() {
    let home = tempfile::tempdir().unwrap();
    let endpoint = app(home.path(), |_| {});
    let mut mcp = Mcp::start(connector(endpoint));
    let init = initialize(&mut mcp);
    let msg = init["error"]["message"].as_str().unwrap();
    assert!(msg.contains("MCP is disabled"), "{msg}");
    // bad JSON is answered, not fatal
    mcp.to.send(b"{not json\n".to_vec()).unwrap();
    let err = mcp.from.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(err["error"]["code"], -32700);
}

#[test]
fn attach_to_a_running_instance_or_launch_one() {
    use freekerf_mcp::attach_or_launch;
    // running: attach, never launch
    let home = tempfile::tempdir().unwrap();
    let endpoint = app(home.path(), |s| s.automation.mcp.enabled = true);
    Client::connect_retry(
        &endpoint,
        &json!({"client": "python_client"}),
        Duration::from_secs(5),
    )
    .unwrap();
    let attached = attach_or_launch(
        &endpoint,
        "a",
        || panic!("must not launch"),
        Duration::from_secs(5),
    )
    .unwrap();
    assert!(attached.headless());
    // not running: launch, then attach once it listens
    let home2 = tempfile::tempdir().unwrap();
    let path = home2.path().to_path_buf();
    let endpoint2 = freekerf_service::remote::endpoint(Some(&path)).unwrap();
    let launched = attach_or_launch(
        &endpoint2,
        "b",
        || {
            app(&path, |s| s.automation.mcp.enabled = true);
            Ok(())
        },
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(launched.call("ping", Value::Null).unwrap(), "pong");
    // a refusal is returned at once
    let home3 = tempfile::tempdir().unwrap();
    let endpoint3 = app(home3.path(), |_| {});
    Client::connect_retry(
        &endpoint3,
        &json!({"client": "python_client"}),
        Duration::from_secs(5),
    )
    .unwrap();
    let err = attach_or_launch(
        &endpoint3,
        "c",
        || panic!("must not launch"),
        Duration::from_secs(5),
    )
    .err()
    .unwrap();
    assert!(err.contains("MCP is disabled"), "{err}");
    // the launch fails
    let home4 = tempfile::tempdir().unwrap();
    let err = attach_or_launch(
        &Endpoint::in_dir(home4.path()),
        "d",
        || Err(std::io::Error::other("no binary")),
        Duration::from_secs(1),
    )
    .err()
    .unwrap();
    assert!(err.contains("cannot start FreeKerf: no binary"));
}
