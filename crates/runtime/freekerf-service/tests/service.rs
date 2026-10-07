//! The service through its action registry, with the emulator transport.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use freekerf_machine::MachineEvent;
use freekerf_protocol::Event;
use freekerf_service::{FileStore, Service, ServiceEvent, Stores};
use freekerf_session::{ActionError, Caller, CallerKind, Tier, valid_name};
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
    call(&s, "job.load_text", json!({"name": "long.nc", "gcode": moves}));
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
    wait_state(&s, |st| st["machine"]["in_program"] == true, 10);
    call(&s, "machine.stop", Value::Null);
    let st = wait_state(&s, |st| st["machine"]["in_program"] == false, 10);
    assert!(st["issues"].as_array().unwrap().is_empty()); // user actions are not issues
    let st = wait_state(&s, |st| !st["resume"].is_null(), 10);
    assert!(st["resume"]["executed"].as_u64().unwrap() > 0);
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

#[test]
fn settings_are_saved_and_automation_is_user_only() {
    let dir = tempfile::tempdir().unwrap();
    let stores = || Stores {
        settings: Some(Arc::new(FileStore(dir.path().join("settings.toml")))),
        cache: Some(Arc::new(FileStore(dir.path().join("cache.json")))),
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
    assert_eq!(call(&s, "settings.get", Value::Null)["version"], 1);
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
