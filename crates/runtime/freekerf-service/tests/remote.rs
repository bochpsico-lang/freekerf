//! The service on the local endpoint: hello and token, MCP disabled by default,
//! single instance, the methods, tiers per call, laser refused headless, STOP
//! while a script runs, idle exit.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::Path;
use std::thread;
use std::time::Duration;

use freekerf_ipc::{Client, Endpoint, IpcError, Server};
use freekerf_service::remote::{ServeOptions, serve};
use freekerf_service::{Service, Stores};
use freekerf_settings::Settings;
use serde_json::{Value, json};

fn settings(home: &Path, f: impl FnOnce(&mut Settings)) {
    let mut s = Settings::default();
    f(&mut s);
    std::fs::write(home.join("settings.toml"), s.to_toml().unwrap()).unwrap();
}

/// A served instance in `home`; returns its endpoint.
fn start(home: &Path, options: ServeOptions) -> (Endpoint, thread::JoinHandle<Result<(), IpcError>>) {
    let service = Service::new(Stores::files(Some(home)).unwrap()).unwrap();
    let endpoint = Endpoint::in_dir(home);
    let ep = endpoint.clone();
    let handle = thread::spawn(move || serve(&service, &ep, options));
    // wait until it listens
    for _ in 0..200 {
        if endpoint.token_file().exists() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    (endpoint, handle)
}

fn agent(endpoint: &Endpoint) -> Result<Client, IpcError> {
    Client::connect_retry(
        endpoint,
        &json!({"client": "agent", "name": "test-agent"}),
        Duration::from_secs(5),
    )
}

#[test]
fn hello_token_and_disabled_mcp() {
    let home = tempfile::tempdir().unwrap();
    let (endpoint, _h) = start(home.path(), ServeOptions::default());
    // MCP is off by default: agents are refused with a clear message
    match agent(&endpoint) {
        Err(IpcError::Refused { kind, message }) => {
            assert_eq!(kind, "disabled");
            assert!(message.contains("automation.mcp.enabled"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn hello_checks() {
    let home = tempfile::tempdir().unwrap();
    settings(home.path(), |s| s.automation.mcp.enabled = true);
    let (endpoint, _h) = start(home.path(), ServeOptions::default());
    let c = agent(&endpoint).unwrap();
    assert_eq!(c.welcome()["tier"], "edit");
    assert_eq!(c.welcome()["headless"], true);
    assert_eq!(c.call("ping", Value::Null).unwrap(), "pong");
    // a client kind the server does not know, or claiming to be the user
    for kind in ["user", "macro", "python_client", "nobody"] {
        assert!(matches!(
            Client::connect(&endpoint, json!({"client": kind})),
            Err(IpcError::Refused { .. })
        ));
    }
    assert!(matches!(
        Client::connect(&endpoint, json!({"client": "agent", "version": 99})),
        Err(IpcError::Refused { .. })
    ));
    // a wrong token
    let token = std::fs::read_to_string(endpoint.token_file()).unwrap();
    std::fs::write(endpoint.token_file(), "0".repeat(64)).unwrap();
    match Client::connect(&endpoint, json!({"client": "agent"})) {
        Err(IpcError::Refused { kind, .. }) => assert_eq!(kind, "unauthorized"),
        other => panic!("{other:?}"),
    }
    std::fs::write(endpoint.token_file(), token).unwrap();
    // one instance per endpoint
    assert!(matches!(
        Server::bind(&endpoint),
        Err(IpcError::AlreadyRunning(_))
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&endpoint.dir), 0o700);
        assert_eq!(mode(&endpoint.token_file()), 0o600);
    }
}

#[test]
fn methods_tiers_and_events() {
    let home = tempfile::tempdir().unwrap();
    settings(home.path(), |s| s.automation.mcp.enabled = true);
    let (endpoint, _h) = start(home.path(), ServeOptions::default());
    let c = agent(&endpoint).unwrap();
    assert_eq!(c.welcome()["tier"], "edit");
    // discovery
    let specs = c.call("specs", Value::Null).unwrap();
    assert!(specs.as_array().unwrap().len() > 40);
    assert!(
        c.call("api_dts", Value::Null)
            .unwrap()
            .as_str()
            .unwrap()
            .contains("interface AppDocument")
    );
    let found = c.call("search_api", json!({"query": "select all"})).unwrap();
    assert_eq!(found[0]["action"], "document.select_all");
    assert!(
        found[0]["signature"]
            .as_str()
            .unwrap()
            .starts_with("app.document.selectAll(")
    );
    assert_eq!(c.call("search_api", json!({"query": "zzz"})).unwrap(), json!([]));
    // state, whole or a part
    let state = c.call("get_state", Value::Null).unwrap();
    assert_eq!(state["session"]["view"]["mode"], "design");
    assert_eq!(state["headless"], true);
    assert_eq!(
        c.call("get_state", json!({"path": "session.view.mode"})).unwrap(),
        "design"
    );
    assert_eq!(
        c.call("get_state", json!({"path": "session.undo.0"}))
            .unwrap_err()
            .kind,
        "invalid_params"
    );
    // events after subscribe
    c.call("subscribe", Value::Null).unwrap();
    c.call(
        "dispatch",
        json!({"action": "view.set_mode", "params": {"mode": "produce"}}),
    )
    .unwrap();
    let ev = c.events().recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(ev["event"]["event"], "session");
    assert_eq!(ev["event"]["change"], "view");
    // a script: one undo step, as the agent
    let r = c
        .call(
            "run_script",
            json!({"code": "app.document.importData({format: 'svg', text: '<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"9mm\" height=\"9mm\"><rect width=\"5\" height=\"5\" stroke=\"red\" fill=\"none\"/></svg>'}).objects.length"}),
        )
        .unwrap();
    assert_eq!(r["value"], 1, "{r}");
    assert_eq!(
        c.call("get_state", json!({"path": "session.undo"})).unwrap(),
        json!(["script test-agent"])
    );
    // tiers per call: machine and laser are above an agent's default baseline
    let e = c
        .call(
            "dispatch",
            json!({"action": "machine.jog", "params": {"direction": "N"}}),
        )
        .unwrap_err();
    assert_eq!(e.kind, "permission_denied");
    let r = c
        .call("run_script", json!({"code": "app.machine.home()"}))
        .unwrap();
    assert!(r["error"].as_str().unwrap().contains("permission denied"));
    // errors are readable
    assert_eq!(
        c.call("dispatch", json!({"action": "nope.nope"}))
            .unwrap_err()
            .kind,
        "unknown"
    );
    assert_eq!(c.call("dispatch", json!({})).unwrap_err().kind, "invalid_params");
    assert_eq!(c.call("nope", Value::Null).unwrap_err().kind, "unknown_method");
    let shot = c.call("screenshot", Value::Null).unwrap_err();
    assert_eq!(shot.kind, "unavailable");
    assert!(shot.message.contains("headless"));
}

#[test]
fn agents_never_run_laser_actions_and_save_only_new_projects() {
    let home = tempfile::tempdir().unwrap();
    settings(home.path(), |s| {
        s.automation.mcp.enabled = true;
        s.automation.mcp.allow_machine = true;
    });
    let (endpoint, _h) = start(home.path(), ServeOptions::default());
    let c = agent(&endpoint).unwrap();
    assert_eq!(c.welcome()["tier"], "machine");
    let e = c
        .call("dispatch", json!({"action": "job.run", "params": {}}))
        .unwrap_err();
    assert_eq!(e.kind, "refused");
    assert!(e.message.contains("agents never run laser actions"), "{e}");
    let r = c
        .call(
            "run_script",
            json!({"code": "app.machine.send({line: 'M3 S1000'})"}),
        )
        .unwrap();
    assert!(
        r["error"]
            .as_str()
            .unwrap()
            .contains("agents never run laser actions"),
        "{r}"
    );
    // machine actions are allowed (nothing is connected: the action's own error)
    let e = c.call("dispatch", json!({"action": "machine.home"})).unwrap_err();
    assert_ne!(e.kind, "permission_denied");
    // files: a new project only
    let new = home.path().join("new.fkproj");
    let save = |path: &Path| {
        c.call(
            "dispatch",
            json!({"action": "document.save", "params": {"path": path}}),
        )
    };
    save(&new).unwrap();
    assert!(new.exists());
    let e = save(&new).unwrap_err();
    assert_eq!(e.kind, "refused");
    assert!(e.message.contains("never overwrite"), "{e}");
    let e = save(&home.path().join("settings.toml")).unwrap_err();
    assert!(e.message.contains("only save projects"), "{e}");
    // the same inside scripts, and previewed by dry runs
    let r = c
        .call(
            "run_script",
            json!({"code": format!("app.document.save({{path: {:?}}})", new.display().to_string())}),
        )
        .unwrap();
    assert!(r["error"].as_str().unwrap().contains("never overwrite"), "{r}");
    let r = c
        .call(
            "run_script",
            json!({"code": "app.document.save({path: '/tmp/x.svg'})", "dry_run": true}),
        )
        .unwrap();
    assert!(r["error"].as_str().unwrap().contains("only save projects"), "{r}");
}

#[test]
fn stop_cancels_a_script_of_the_same_client() {
    let home = tempfile::tempdir().unwrap();
    settings(home.path(), |s| s.automation.mcp.enabled = true);
    let (endpoint, _h) = start(home.path(), ServeOptions::default());
    let c = std::sync::Arc::new(agent(&endpoint).unwrap());
    c.call("subscribe", Value::Null).unwrap();
    let runner = std::sync::Arc::clone(&c);
    let script = thread::spawn(move || {
        runner.call(
            "run_script",
            json!({"code": "app.view.setMode({mode: 'produce'}); while (true) {}"}),
        )
    });
    // the script has started when its view change arrives
    let ev = c.events().recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(ev["event"]["change"], "view");
    c.call("stop", Value::Null).unwrap();
    let r = script.join().unwrap().unwrap();
    assert_eq!(r["error"], "script stopped: cancelled");
    assert_eq!(r["rolled_back"], true);
}

#[test]
fn an_instance_started_for_mcp_exits_when_idle() {
    let home = tempfile::tempdir().unwrap();
    settings(home.path(), |s| s.automation.mcp.enabled = true);
    let (endpoint, handle) = start(
        home.path(),
        ServeOptions {
            idle_exit: Some(Duration::from_millis(300)),
        },
    );
    let c = agent(&endpoint).unwrap();
    thread::sleep(Duration::from_millis(500));
    assert!(!handle.is_finished(), "a connected client keeps it alive");
    drop(c);
    handle.join().unwrap().unwrap();
    assert!(!endpoint.token_file().exists());
    assert!(matches!(
        agent(&endpoint),
        Err(IpcError::NotRunning(_) | IpcError::Token { .. })
    ));
}
