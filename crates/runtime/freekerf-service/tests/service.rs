//! The service through its action registry, with the emulator transport.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use freekerf_machine::MachineEvent;
use freekerf_protocol::Event;
use freekerf_service::{FileStore, Service, ServiceEvent, Stores};
use freekerf_session::{ActionError, Caller, CallerKind, Failure, FailureCause, Tier, valid_name};
use serde_json::{Value, json};

fn user() -> Caller {
    Caller::user()
}

fn call(s: &Service, action: &str, params: Value) -> Value {
    s.dispatch(&user(), action, params)
        .unwrap_or_else(|e| panic!("{action}: {e}"))
}

fn wait_state(s: &Service, pred: impl Fn(&Value) -> bool, secs: u64) -> Value {
    let end = Instant::now() + Duration::from_secs(secs);
    loop {
        let st = call(s, "state.get", Value::Null);
        if pred(&st) {
            return st;
        }
        assert!(Instant::now() < end, "state not reached: {st}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn connected() -> Service {
    let s = Service::in_memory().unwrap();
    call(&s, "machine.connect", json!({"emulator": true}));
    wait_state(
        &s,
        |st| st["machine"]["ready"] == true && st["machine"]["status"] == "Idle",
        10,
    );
    s
}

#[test]
fn registry_invariants() {
    let s = Service::in_memory().unwrap();
    let specs = s.specs();
    assert!(specs.len() >= 25);
    for spec in &specs {
        assert!(valid_name(&spec.name), "{}", spec.name);
        assert!(!spec.description.is_empty());
        assert!(spec.params.is_object());
    }
    // what can fire the laser is in the laser tier; STOP is always allowed
    for (name, tier) in [
        ("job.run", Tier::Laser),
        ("job.resume", Tier::Laser),
        ("machine.send", Tier::Laser),
        ("machine.stop", Tier::Safety),
        ("machine.feed_hold", Tier::Safety),
        ("machine.safety_door", Tier::Safety),
        ("job.abort", Tier::Safety),
        ("machine.jog", Tier::Machine),
        ("state.get", Tier::Read),
    ] {
        assert_eq!(s.registry().spec(name).unwrap().tier, tier, "{name}");
    }
    assert!(s.registry().to_markdown().contains("machine.connect"));
    assert!(format!("{s:?}").contains("Service"));
}

#[test]
fn job_runs_end_to_end_on_the_emulator() {
    let s = connected();
    let events = s.subscribe();
    let info = call(
        &s,
        "job.load_text",
        json!({"name": "square.nc", "gcode": "G0 X10 Y10\nM3 S800\nG1 X20 F6000\nG1 Y20\nM5"}),
    );
    assert_eq!(info["lines"], 5);
    assert_eq!(call(&s, "job.info", Value::Null)["name"], "square.nc");
    call(&s, "job.run", json!({"passes": 2}));
    let end = Instant::now() + Duration::from_secs(20);
    let mut ended = false;
    while Instant::now() < end && !ended {
        if let Ok(ServiceEvent::Machine(MachineEvent::Host(Event::JobEnded { lines, errors, .. }))) =
            events.recv_timeout(Duration::from_millis(100))
        {
            assert_eq!((lines, errors), (5, 0));
            ended = true;
        }
    }
    assert!(ended, "job did not end");
    let console = call(&s, "console.get", json!({"last": 50}));
    assert!(
        console
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r.to_string().contains("G1 X20 F6000"))
    );
    assert!(
        call(&s, "console.get", Value::Null).as_array().unwrap().len() >= console.as_array().unwrap().len()
    );
    call(&s, "machine.disconnect", Value::Null);
    assert_eq!(
        call(&s, "state.get", Value::Null)["machine"]["status"],
        "Disconnected"
    );
}

#[test]
fn manual_commands_settings_and_jog() {
    let s = connected();
    call(
        &s,
        "machine.write_settings",
        json!({"settings": {"22": "1", "20": "0"}}),
    );
    call(&s, "machine.unlock", Value::Null);
    call(&s, "machine.home", Value::Null);
    call(&s, "machine.send", json!({"line": "G0 X5 Y5"}));
    call(
        &s,
        "machine.jog",
        json!({"direction": "NE", "step_mm": 2.5, "feed_mm_min": 800}),
    );
    let st = wait_state(
        &s,
        |st| st["machine"]["machine_position"]["x"].as_f64() == Some(7.5),
        10,
    );
    assert_eq!(st["firmware_version"], "1.1f");
    call(&s, "machine.jog_to", json!({"x": 1.0, "y": 2.0}));
    wait_state(
        &s,
        |st| {
            st["machine"]["status"] == "Idle" && st["machine"]["machine_position"]["x"].as_f64() == Some(1.0)
        },
        10,
    );
    call(&s, "machine.set_zero", Value::Null);
    call(
        &s,
        "machine.overrides",
        json!({"feed": 120, "rapid": 50, "power": 90}),
    );
    wait_state(&s, |st| st["machine"]["overrides"] == json!([120, 50, 90]), 10);
    call(&s, "machine.read_settings", Value::Null);
    call(&s, "machine.custom_code", json!({"code": "G0 X[left]"}));
    call(&s, "machine.jog_stop", Value::Null);
    let e = s
        .dispatch(&user(), "machine.jog", json!({"direction": "UP"}))
        .unwrap_err();
    assert!(matches!(e, ActionError::InvalidParams(_)));
    let e = s
        .dispatch(&user(), "machine.select_laser", json!({"id": "nope"}))
        .unwrap_err();
    assert!(matches!(e, ActionError::InvalidParams(_)));
    // board errors come back as failures
    let e = s
        .dispatch(
            &user(),
            "machine.write_settings",
            json!({"settings": {"999": "1"}}),
        )
        .unwrap_err();
    assert!(e.to_string().contains("$999=1"));
    assert!(call(&s, "machine.list_ports", Value::Null).is_array());
}

#[test]
fn hold_resume_stop_and_abort() {
    let s = connected();
    let moves: String = (0..60).map(|i| format!("G1 X{} F600\n", (i % 2) * 2)).collect();
    let info = call(&s, "job.load_text", json!({"name": "long.nc", "gcode": moves}));
    assert_eq!(info["from_document"], Value::Null, "G-code loaded as such");
    call(&s, "job.run", Value::Null);
    wait_state(&s, |st| st["machine"]["status"] == "Run", 10);
    call(&s, "machine.feed_hold", Value::Null);
    wait_state(&s, |st| st["machine"]["status"] == "Hold", 10);
    call(&s, "machine.resume", Value::Null);
    wait_state(&s, |st| st["machine"]["status"] == "Run", 10);
    call(&s, "job.abort", Value::Null);
    // the board still runs the lines it buffered: wait until it is idle
    wait_state(
        &s,
        |st| st["machine"]["in_program"] == false && st["machine"]["status"] == "Idle",
        30,
    );
    call(&s, "job.run", Value::Null);
    // STOP counts as an interruption (ManualReset, resume offer) only once the
    // board reported Run and executed a line, as in LaserGRBL
    wait_state(
        &s,
        |st| {
            st["machine"]["in_program"] == true
                && st["machine"]["status"] == "Run"
                && st["machine"]["job_executed"].as_u64() > Some(0)
        },
        10,
    );
    call(&s, "machine.stop", Value::Null);
    let st = wait_state(&s, |st| st["machine"]["in_program"] == false, 10);
    assert!(st["issues"].as_array().unwrap().is_empty()); // user actions are not issues
    let st = wait_state(&s, |st| !st["resume"].is_null(), 10);
    assert!(st["resume"]["executed"].as_u64().unwrap() > 0);
    s.stop();
}

#[test]
fn safety_door_holds_until_resumed() {
    let s = connected();
    let moves: String = (0..60).map(|i| format!("G1 X{} F600\n", (i % 2) * 2)).collect();
    call(&s, "job.load_text", json!({"name": "long.nc", "gcode": moves}));
    call(&s, "job.run", Value::Null);
    wait_state(&s, |st| st["machine"]["status"] == "Run", 10);
    call(&s, "machine.safety_door", Value::Null);
    wait_state(&s, |st| st["machine"]["status"] == "Door", 10);
    call(&s, "machine.resume", Value::Null);
    wait_state(&s, |st| st["machine"]["status"] == "Run", 10);
    call(&s, "machine.stop", Value::Null);
    s.stop();
}

#[test]
fn refusals_and_permissions() {
    let s = Service::in_memory().unwrap();
    assert!(matches!(
        s.dispatch(&user(), "job.run", Value::Null),
        Err(ActionError::Refused(_))
    ));
    assert!(matches!(
        s.dispatch(&user(), "machine.connect", Value::Null),
        Err(ActionError::InvalidParams(_))
    ));
    assert!(matches!(
        s.dispatch(&user(), "machine.connect", json!({"port": "/nonexistent/fk"})),
        Err(ActionError::Failed(_))
    ));
    let agent = Caller::new(CallerKind::Agent, "bot", Tier::Edit);
    assert!(matches!(
        s.dispatch(&agent, "job.run", Value::Null),
        Err(ActionError::PermissionDenied { .. })
    ));
    assert!(s.dispatch(&agent, "machine.stop", Value::Null).is_ok());
    assert!(matches!(
        s.dispatch(&user(), "job.load", json!({"path": "/nonexistent.nc"})),
        Err(ActionError::Failed(_))
    ));
    assert!(s.dispatch(&user(), "nope.nope", Value::Null).is_err());
    assert_eq!(call(&s, "job.info", Value::Null), Value::Null);
}

/// A port this user may not open fails with a cause the window recognises. A
/// temporary file with mode 000 stands for the device (no real serial port).
#[cfg(unix)]
#[test]
fn a_denied_port_fails_with_its_cause() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let port = dir.path().join("ttyFAKE0");
    std::fs::write(&port, b"").unwrap();
    std::fs::set_permissions(&port, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::File::open(&port).is_ok() {
        return; // root opens anything
    }
    let path = port.to_string_lossy().into_owned();
    let s = Service::in_memory().unwrap();
    let e = s
        .dispatch(&user(), "machine.connect", json!({"port": path}))
        .unwrap_err();
    let ActionError::Failed(f) = &e else {
        panic!("expected a failure, got {e:?}")
    };
    let Some(FailureCause::PortPermissionDenied { port: p, group }) = &f.cause else {
        panic!("expected PortPermissionDenied, got {f:?}")
    };
    assert_eq!(p, &path);
    // the temporary file belongs to this user's group, which has a name
    assert!(group.is_some());
    assert!(e.to_string().contains("permission denied"));
    let wire = serde_json::to_value(&e).unwrap();
    assert_eq!(wire["kind"], "failed");
    assert_eq!(wire["message"]["cause"]["code"], "port_permission_denied");
    assert_eq!(wire["message"]["cause"]["port"], path.as_str());
    // other failures carry no cause
    let e = s
        .dispatch(&user(), "machine.connect", json!({"port": "/nonexistent/fk"}))
        .unwrap_err();
    assert!(matches!(e, ActionError::Failed(Failure { cause: None, .. })));
    s.stop();
}

#[test]
fn settings_are_saved_and_automation_is_user_only() {
    let dir = tempfile::tempdir().unwrap();
    let stores = || Stores {
        settings: Some(Arc::new(FileStore(dir.path().join("settings.toml")))),
        cache: Some(Arc::new(FileStore(dir.path().join("cache.json")))),
        ..Stores::default()
    };
    {
        let s = Service::new(stores()).unwrap();
        let events = s.subscribe();
        call(
            &s,
            "settings.set",
            json!({"key": "machine.streaming_mode", "value": "Synchronous"}),
        );
        assert!(events.recv_timeout(Duration::from_secs(1)).is_ok());
        let e = s
            .dispatch(
                &user(),
                "settings.set",
                json!({"key": "automation.mcp.enabled", "value": true}),
            )
            .unwrap_err();
        assert!(e.to_string().contains("user"));
        assert!(
            s.dispatch(&user(), "settings.get", json!({"key": "nope"}))
                .is_err()
        );
        // connecting records the version and $$ in the cache
        call(&s, "machine.connect", json!({"emulator": true}));
        wait_state(&s, |st| st["machine"]["ready"] == true, 10);
        call(&s, "machine.disconnect", Value::Null);
        assert!(format!("{:?}", stores()).contains("settings: true"));
    }
    let text = std::fs::read_to_string(dir.path().join("settings.toml")).unwrap();
    assert!(text.contains("streaming_mode = \"Synchronous\""));
    let s = Service::new(stores()).unwrap();
    assert_eq!(
        call(&s, "settings.get", json!({"key": "machine.streaming_mode"})),
        "Synchronous"
    );
    assert_eq!(call(&s, "settings.get", Value::Null)["version"], 2);
    let end = Instant::now() + Duration::from_secs(5);
    while !dir.path().join("cache.json").exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    let cache = std::fs::read_to_string(dir.path().join("cache.json")).unwrap();
    assert!(cache.contains("\"build\": \"f\""), "{cache}");
}

#[test]
fn load_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.nc");
    std::fs::write(&p, "G0 X1\nG1 X2 F100\n").unwrap();
    let s = Service::in_memory().unwrap();
    let info = call(&s, "job.load", json!({"path": p.to_string_lossy()}));
    assert_eq!(info["name"], "a.nc");
    assert_eq!(info["lines"], 2);
    assert!(s.snapshot().program_lines == 2);
}

#[test]
fn document_session_actions_and_events() {
    use freekerf_service::ServiceEvent;
    let s = Service::in_memory().unwrap();
    let user = Caller::user();
    let dir = tempfile::tempdir().unwrap();
    let svg = dir.path().join("a.svg");
    std::fs::write(
        &svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20">
           <rect x="1" y="1" width="5" height="5" stroke="red" fill="none"/></svg>"##,
    )
    .unwrap();
    let events = s.subscribe();
    let out = s
        .dispatch(&user, "document.import", json!({"path": svg.to_string_lossy()}))
        .unwrap();
    let id = out["objects"][0].as_u64().unwrap();
    let ev: Vec<Value> = events.try_iter().map(|e: ServiceEvent| e.to_json()).collect();
    assert!(
        ev.iter()
            .any(|e| e["event"] == "session" && e["change"] == "document"),
        "{ev:?}"
    );
    s.dispatch(&user, "document.select", json!({"ids": [id]}))
        .unwrap();
    s.dispatch(&user, "document.translate", json!({"dx": 5.0, "dy": 5.0}))
        .unwrap();
    let snap = s.dispatch(&user, "session.snapshot", Value::Null).unwrap();
    assert_eq!(snap["selection"], json!([id]));
    assert_eq!(snap["undo"], json!(["import", "move"]));
    assert!(snap["dirty"].as_bool().unwrap());
    // save, change, open again
    let proj = dir.path().join("p.fkproj");
    s.dispatch(&user, "document.save", json!({"path": proj.to_string_lossy()}))
        .unwrap();
    assert_eq!(
        s.dispatch(&user, "session.snapshot", Value::Null).unwrap()["dirty"],
        json!(false)
    );
    s.dispatch(&user, "document.delete", Value::Null).unwrap();
    s.dispatch(&user, "document.open", json!({"path": proj.to_string_lossy()}))
        .unwrap();
    let snap = s.dispatch(&user, "session.snapshot", Value::Null).unwrap();
    assert_eq!(snap["document"]["objects"].as_array().unwrap().len(), 1);
    assert_eq!(snap["undo"], json!([]));
    // plan the document into the job
    let info = s.dispatch(&user, "job.from_document", Value::Null).unwrap();
    assert!(info["lines"].as_u64().unwrap() > 3);
    assert_eq!(
        s.dispatch(&user, "job.info", Value::Null).unwrap()["name"],
        "document"
    );
    // placed with its centre on the user origin
    let centred = s
        .dispatch(
            &user,
            "job.from_document",
            json!({"start": "user_origin", "anchor": "center"}),
        )
        .unwrap();
    // the job records the document's revision and how it was placed
    let revision = s.dispatch(&user, "session.snapshot", Value::Null).unwrap()["document_revision"].clone();
    assert_eq!(centred["document_revision"], revision);
    assert_eq!(
        centred["from_document"],
        json!({"start": "user_origin", "anchor": "center"})
    );
    assert_eq!(
        s.dispatch(&user, "state.get", Value::Null).unwrap()["program"]["from_document"]["anchor"],
        "center"
    );
    let r: [f64; 4] = serde_json::from_value(centred["drawing_range"].clone()).unwrap();
    assert!((r[0] + r[2]).abs() < 0.01 && (r[1] + r[3]).abs() < 0.01, "{r:?}");
    let absolute: [f64; 4] = serde_json::from_value(info["drawing_range"].clone()).unwrap();
    assert!(
        (absolute[0] - r[0]).abs() > 0.5,
        "absolute keeps the document's place"
    );
    // the current position needs a machine
    assert!(
        s.dispatch(&user, "job.from_document", json!({"start": "current_position"}))
            .is_err()
    );
    // errors
    assert!(
        s.dispatch(&user, "document.open", json!({"path": "/no/such.fkproj"}))
            .is_err()
    );
    assert!(
        s.dispatch(&user, "document.import", json!({"path": proj.to_string_lossy()}))
            .is_err()
    );
    assert!(
        s.dispatch(&user, "document.import", json!({"path": "/no/such.svg"}))
            .is_err()
    );
    // a regular file used as a directory fails on every system
    let under_a_file = svg.join("x.fkproj");
    assert!(
        s.dispatch(
            &user,
            "document.save",
            json!({"path": under_a_file.to_string_lossy()})
        )
        .is_err()
    );
}

#[test]
fn results_are_described_for_the_front_ends() {
    let service = Service::in_memory().unwrap();
    // the machine, job, fire and view-reading actions the desktop app uses are typed
    let untyped: Vec<String> = service
        .specs()
        .into_iter()
        .filter(|s| s.result.is_none())
        .map(|s| s.name)
        .collect();
    // every result is typed, from the handler's type or described; a setting's value
    // is any JSON
    assert_eq!(untyped, ["settings.get"]);
    let state = service.registry().spec("state.get").unwrap();
    let schema = state.result.as_ref().unwrap();
    assert!(schema["properties"]["machine"].is_object(), "{schema}");
}

#[test]
fn view_actions_change_the_view_and_publish_it() {
    let service = Service::in_memory().unwrap();
    let events = service.subscribe();
    let ui = Caller::new(CallerKind::Agent, "agent", Tier::Ui);
    let v = service.dispatch(&ui, "view.get", json!({})).unwrap();
    assert_eq!(v["mode"], "design");
    service
        .dispatch(&ui, "view.set_mode", json!({"mode": "produce"}))
        .unwrap();
    service
        .dispatch(&ui, "view.set_mode", json!({"mode": "design"}))
        .unwrap();
    service
        .dispatch(&ui, "view.set_tool", json!({"tool": "rect"}))
        .unwrap();
    assert_eq!(
        service.dispatch(&ui, "view.get", json!({})).unwrap()["tool"],
        "rect"
    );
    service
        .dispatch(&ui, "view.open_panel", json!({"name": "layers"}))
        .unwrap();
    service
        .dispatch(&ui, "view.open_dialog", json!({"name": "settings"}))
        .unwrap();
    let id = service
        .dispatch(
            &ui,
            "view.toast",
            json!({"message": "toast.saved", "level": "ok"}),
        )
        .unwrap();
    service
        .dispatch(
            &ui,
            "view.set_viewport",
            json!({"center": [10.0, 5.0], "zoom": 3.0}),
        )
        .unwrap();
    service.dispatch(&ui, "view.fit", json!({})).unwrap();
    let v = service.dispatch(&ui, "view.get", json!({})).unwrap();
    assert_eq!(v["mode"], "design");
    assert_eq!(v["panel"], "layers");
    assert_eq!(v["drawer_open"], true);
    assert_eq!(v["dialogs"], json!(["settings"]));
    assert_eq!(v["toasts"][0]["id"], id);
    assert_eq!(v["viewport"]["zoom"], 3.0);
    assert_eq!(v["fit_request"], 1);
    service
        .dispatch(&ui, "view.dismiss_toast", json!({"id": id}))
        .unwrap();
    service.dispatch(&ui, "view.close_dialog", json!({})).unwrap();
    // each change was published
    let mut views = 0;
    while let Ok(e) = events.recv_timeout(Duration::from_millis(100)) {
        if let ServiceEvent::Session(s) = &e
            && s.change == freekerf_session::session::Change::View
        {
            views += 1;
            assert_eq!(e.to_json()["event"], "session");
            assert_eq!(e.to_json()["change"], "view");
        }
    }
    assert_eq!(views, 10);
    // invalid values are refused, and a read-only caller cannot change the view
    let bad = service.dispatch(
        &ui,
        "view.set_viewport",
        json!({"center": [0.0, 0.0], "zoom": 0.0}),
    );
    assert!(matches!(bad, Err(ActionError::InvalidParams(_))));
    let reader = Caller::new(CallerKind::Agent, "reader", Tier::Read);
    let denied = service.dispatch(&reader, "view.set_mode", json!({"mode": "produce"}));
    assert!(matches!(denied, Err(ActionError::PermissionDenied { .. })));
}

#[test]
fn produce_mode_helpers_frame_info_and_system_commands() {
    let s = connected();
    // machine.info: the emulator's $$ table was read on connect
    let info = call(&s, "machine.info", json!({}));
    assert_eq!(info["settings_known"], true, "{info}");
    assert!(info["table_width_mm"].as_f64().unwrap() > 0.0);
    // system commands only; they need the machine tier, not laser
    let machine = Caller::new(CallerKind::Agent, "agent", Tier::Machine);
    s.dispatch(&machine, "machine.system_command", json!({"line": "$I"}))
        .unwrap();
    for bad in ["G1 X10 S1000", "M3 S500", "$$\nM3"] {
        let e = s
            .dispatch(&machine, "machine.system_command", json!({"line": bad}))
            .unwrap_err();
        assert!(matches!(e, ActionError::InvalidParams(_)), "{bad}: {e:?}");
    }
    // frame: refused without a drawing, then runs around the drawing range
    let e = s.dispatch(&machine, "job.frame", json!({})).unwrap_err();
    assert!(matches!(e, ActionError::Refused(_)), "{e:?}");
    call(
        &s,
        "job.load_text",
        json!({"name": "sq.nc", "gcode": "G0 X10 Y5\nM3 S800\nG1 X30 F3000\nG1 Y25\nM5\n"}),
    );
    s.dispatch(&machine, "job.frame", json!({"feed_mm_min": 6000.0}))
        .unwrap();
    // the head visits the corners and ends back at the first one
    wait_state(
        &s,
        |st| {
            let p = &st["machine"]["work_position"];
            st["machine"]["status"] == "Idle"
                && (p["x"].as_f64().unwrap() - 10.0).abs() < 0.01
                && (p["y"].as_f64().unwrap() - 5.0).abs() < 0.01
        },
        20,
    );
}

#[test]
fn laser_actions_run_only_after_a_confirmed_countdown() {
    let s = connected();
    let clock = freekerf_testkit::SimClock::new(10_000);
    s.set_fire_clock(Arc::new(clock.clone()));
    call(
        &s,
        "job.load_text",
        json!({"name": "sq.nc", "gcode": "G0 X1 Y1\nM3 S500\nG1 X5 F3000\nM5\n"}),
    );
    // the window's caller cannot dispatch any laser action itself
    let window = Caller::new(CallerKind::User, "desktop", Tier::Machine);
    let laser: Vec<String> = s
        .specs()
        .into_iter()
        .filter(|a| a.tier == Tier::Laser)
        .map(|a| a.name)
        .collect();
    assert_eq!(
        laser,
        ["job.resume", "job.run", "machine.custom_code", "machine.send"]
    );
    for a in &laser {
        let e = s.dispatch(&window, a, json!({})).unwrap_err();
        assert!(matches!(e, ActionError::PermissionDenied { .. }), "{a}: {e:?}");
    }
    // agents never run laser actions: they cannot even ask
    let agent = Caller::new(CallerKind::Agent, "claude-code", Tier::Machine);
    let e = s
        .dispatch(&agent, "fire.request", json!({"action": "job.run", "params": {}}))
        .unwrap_err();
    assert!(
        matches!(e, ActionError::Refused(_) | ActionError::PermissionDenied { .. }),
        "{e:?}"
    );
    // the window can ask; only laser actions can be asked for
    let bad = s
        .dispatch(&window, "fire.request", json!({"action": "machine.home"}))
        .unwrap_err();
    assert!(matches!(bad, ActionError::InvalidParams(_)), "{bad:?}");
    let req = s
        .dispatch(
            &window,
            "fire.request",
            json!({"action": "job.run", "params": {}}),
        )
        .unwrap();
    let id = req["id"].as_u64().unwrap();
    assert_eq!(req["countdown_ms"], 3000);
    assert_eq!(req["summary"]["program"], "sq.nc");
    let view = call(&s, "view.get", json!({}));
    assert_eq!(view["fire_request"]["id"], id);
    assert_eq!(view["dialogs"], json!(["fire_confirmation"]));
    // too early, wrong id: refused, nothing runs
    assert!(matches!(s.confirm_fire(id, false), Err(ActionError::Refused(_))));
    clock.advance(2999);
    assert!(matches!(s.confirm_fire(id, false), Err(ActionError::Refused(_))));
    assert!(matches!(
        s.confirm_fire(id + 1, false),
        Err(ActionError::Refused(_))
    ));
    assert_eq!(call(&s, "state.get", json!({}))["machine"]["in_program"], false);
    // after the countdown the person's confirmation runs it, once
    clock.advance(1);
    let events = s.subscribe();
    s.confirm_fire(id, false).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < end, "the job did not start");
        if let Ok(ServiceEvent::Machine(MachineEvent::Host(Event::JobStarted { .. }))) =
            events.recv_timeout(Duration::from_millis(100))
        {
            break;
        }
    }
    assert!(matches!(s.confirm_fire(id, false), Err(ActionError::Refused(_))));
    let view = call(&s, "view.get", json!({}));
    assert!(view["fire_request"].is_null());
    assert_eq!(view["dialogs"], json!([]));
    wait_state(
        &s,
        |st| st["machine"]["in_program"] == false && st["machine"]["status"] == "Idle",
        20,
    );
    // a request can be cancelled (always allowed)
    let req = s
        .dispatch(
            &window,
            "fire.request",
            json!({"action": "machine.send", "params": {"line": "M3 S10"}}),
        )
        .unwrap();
    assert_eq!(req["summary"]["code"], "M3 S10");
    let reader = Caller::new(CallerKind::Agent, "reader", Tier::Read);
    s.dispatch(&reader, "fire.cancel", json!({})).unwrap();
    clock.advance(10_000);
    assert!(matches!(
        s.confirm_fire(req["id"].as_u64().unwrap(), false),
        Err(ActionError::Refused(_))
    ));
    // closing its dialog cancels it too
    let req = s
        .dispatch(
            &window,
            "fire.request",
            json!({"action": "job.run", "params": {}}),
        )
        .unwrap();
    call(&s, "view.close_dialog", json!({}));
    clock.advance(10_000);
    assert!(matches!(
        s.confirm_fire(req["id"].as_u64().unwrap(), false),
        Err(ActionError::Refused(_))
    ));
    assert!(call(&s, "view.get", json!({}))["fire_request"].is_null());
}

fn wait_view(s: &Service, pred: impl Fn(&Value) -> bool, secs: u64) -> Value {
    let end = Instant::now() + Duration::from_secs(secs);
    loop {
        let v = call(s, "view.get", Value::Null);
        if pred(&v) {
            return v;
        }
        assert!(Instant::now() < end, "view not reached: {v}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_job_is_fired_from_produce_and_an_out_of_date_one_only_when_accepted() {
    let s = connected();
    let clock = freekerf_testkit::SimClock::new(10_000);
    s.set_fire_clock(Arc::new(clock.clone()));
    let window = Caller::new(CallerKind::User, "desktop", Tier::Machine);
    let ask = || {
        s.dispatch(
            &window,
            "fire.request",
            json!({"action": "job.run", "params": {}}),
        )
        .unwrap()
    };
    let rect = json!({"shape": "rect", "x": 0, "y": 0, "width": 2, "height": 2});
    // a file with an empty design is never out of date; asking shows Produce
    let moves: String = (0..60).map(|i| format!("G1 X{} F600\n", (i % 2) * 2)).collect();
    call(&s, "job.load_text", json!({"name": "long.nc", "gcode": moves}));
    assert_eq!(call(&s, "view.get", Value::Null)["mode"], "design");
    let req = ask();
    assert!(req["summary"]["stale"].is_null());
    let view = call(&s, "view.get", Value::Null);
    assert_eq!(
        (&view["mode"], &view["panel"]),
        (&json!("produce"), &json!("machine"))
    );
    call(&s, "fire.cancel", json!({}));
    // the design changed after the file was loaded: out of date
    call(&s, "view.set_mode", json!({"mode": "design"}));
    call(&s, "document.add_shape", rect.clone());
    let req = ask();
    assert_eq!(req["summary"]["stale"], "file");
    let id = req["id"].as_u64().unwrap();
    clock.advance(10_000);
    let e = s.confirm_fire(id, false).unwrap_err();
    assert!(matches!(e, ActionError::Refused(_)), "{e:?}");
    // still pending: the person can accept it
    assert_eq!(call(&s, "view.get", Value::Null)["fire_request"]["id"], id);
    s.confirm_fire(id, true).unwrap();
    // while it runs or is paused the view stays in Produce
    wait_state(&s, |st| st["machine"]["status"] == "Run", 10);
    call(&s, "machine.feed_hold", Value::Null);
    wait_state(&s, |st| st["machine"]["status"] == "Hold", 10);
    wait_view(&s, |v| v["produce_locked"] == true, 10);
    let e = s
        .dispatch(&user(), "view.set_mode", json!({"mode": "design"}))
        .unwrap_err();
    assert!(matches!(e, ActionError::Refused(_)), "{e:?}");
    call(&s, "machine.stop", Value::Null);
    wait_state(&s, |st| st["machine"]["in_program"] == false, 20);
    wait_view(&s, |v| v["produce_locked"] == false, 20);
    call(&s, "view.set_mode", json!({"mode": "design"}));
    // planned from the design: up to date, then out of date after an edit
    call(&s, "job.from_document", json!({}));
    assert!(ask()["summary"]["stale"].is_null());
    call(&s, "fire.cancel", json!({}));
    call(&s, "document.add_shape", rect.clone());
    assert_eq!(ask()["summary"]["stale"], "design");
    call(&s, "fire.cancel", json!({}));
    // out of date after the request: dropped, asked again
    call(&s, "job.from_document", json!({}));
    let id = ask()["id"].as_u64().unwrap();
    call(&s, "document.add_shape", rect);
    clock.advance(10_000);
    assert!(matches!(s.confirm_fire(id, true), Err(ActionError::Refused(_))));
    assert!(call(&s, "view.get", Value::Null)["fire_request"].is_null());
    // resuming is not checked (it continues the job that was cut)
    let req = s
        .dispatch(
            &window,
            "fire.request",
            json!({"action": "job.resume", "params": {}}),
        )
        .unwrap();
    assert!(req["summary"]["stale"].is_null());
}

#[test]
fn stop_after_a_system_command_clears_the_console_and_resets() {
    let s = Service::in_memory().unwrap();
    let user = Caller::user();
    s.dispatch(&user, "machine.connect", json!({"emulator": true}))
        .unwrap();
    let wait = |what: &str, f: &dyn Fn(&str) -> bool| {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            let c = s.dispatch(&user, "console.get", Value::Null).unwrap().to_string();
            if f(&c) {
                return;
            }
            assert!(Instant::now() < end, "{what}: {c}");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait("ready", &|_| {
        s.dispatch(&user, "state.get", Value::Null).unwrap()["machine"]["status"] == "Idle"
    });
    for i in 0..30 {
        // `$G`: never sent by the host itself (it reads `$$` and `$I` again after a reset)
        s.dispatch(&user, "machine.system_command", json!({"line": "$G"}))
            .unwrap();
        wait("$G answered", &|c| {
            c.contains("[GC:") && c.contains("\"result\":\"OK\"")
        });
        s.dispatch(&user, "machine.stop", Value::Null).unwrap();
        wait(&format!("reset {i}"), &|c| {
            !c.contains("[GC:") && c.contains("Grbl")
        });
    }
}
