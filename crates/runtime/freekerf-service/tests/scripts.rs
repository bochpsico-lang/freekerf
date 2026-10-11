//! Scripts over the service: one undo step, rollback, dry run, laser refusal,
//! tier denial, STOP.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use freekerf_service::script::{ScriptReport, ScriptRequest};
use freekerf_service::{Caller, CallerKind, Service, ServiceEvent, Tier};
use serde_json::{Value, json};

const IMPORT: &str = r##"
const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20">'
  + '<rect x="1" y="1" width="5" height="5" stroke="red" fill="none"/>'
  + '<path d="M1 1 L5 5" stroke="#00f"/></svg>';
const imported = app.document.importData({format: "svg", text: svg, name: "gen.svg"});
"##;

fn run(service: &Service, caller: &Caller, code: &str) -> ScriptReport {
    service.run_script(caller, &ScriptRequest::new("t", format!("{IMPORT}{code}")))
}

fn snapshot(service: &Service) -> Value {
    service
        .dispatch(&Caller::user(), "session.snapshot", Value::Null)
        .unwrap()
}

fn macro_caller(tier: Tier) -> Caller {
    Caller::new(CallerKind::Macro, "test", tier)
}

#[test]
fn a_script_is_one_undo_step() {
    let s = Service::in_memory().unwrap();
    let r = run(
        &s,
        &macro_caller(Tier::Edit),
        "app.document.selectAll(); app.document.translate({dx: 5, dy: 0}); app.view.setMode({mode: 'produce'}); imported.objects.length",
    );
    assert_eq!(r.outcome.error, None);
    assert_eq!(r.outcome.value, json!(2));
    assert!(!r.rolled_back);
    assert_eq!(r.outcome.actions.len(), 4);
    let snap = snapshot(&s);
    assert_eq!(snap["undo"], json!(["script t"]));
    assert_eq!(snap["document"]["objects"].as_array().unwrap().len(), 2);
    assert_eq!(snap["view"]["mode"], json!("produce"));
    // undo removes everything the script did to the document
    s.dispatch(&Caller::user(), "history.undo", Value::Null).unwrap();
    assert_eq!(snapshot(&s)["document"]["objects"].as_array().unwrap().len(), 0);
    // a script that changes nothing leaves no step
    let r = s.run_script(&macro_caller(Tier::Edit), &ScriptRequest::new("noop", "1 + 1"));
    assert_eq!(r.outcome.value, json!(2));
    assert_eq!(snapshot(&s)["undo"], json!([]));
}

#[test]
fn an_error_rolls_everything_back() {
    let s = Service::in_memory().unwrap();
    let r = run(
        &s,
        &macro_caller(Tier::Edit),
        "app.view.setMode({mode: 'produce'}); console.log('half way'); throw new Error('boom')",
    );
    assert!(r.outcome.error.as_deref().unwrap().contains("boom"));
    assert!(r.rolled_back);
    assert_eq!(r.outcome.log, ["half way"]);
    let snap = snapshot(&s);
    assert_eq!(snap["document"]["objects"].as_array().unwrap().len(), 0);
    assert_eq!(snap["view"]["mode"], json!("design"));
    assert_eq!(snap["undo"], json!([]));
    // a failing action the script does not catch also rolls back
    let r = run(
        &s,
        &macro_caller(Tier::Edit),
        "app.document.fromFkproj({text: '{}'})",
    );
    assert!(r.rolled_back);
    assert_eq!(snapshot(&s)["document"]["objects"].as_array().unwrap().len(), 0);
}

#[test]
fn a_dry_run_applies_nothing() {
    let s = Service::in_memory().unwrap();
    let events = s.subscribe();
    let mut req = ScriptRequest::new(
        "dry",
        format!(
            "{IMPORT} app.document.selectAll(); app.document.translate({{dx: 5, dy: 0}});
             const ran = app.job.run({{}});
             app.machine.jog({{direction: 'n', step_mm: 1}});
             app.machine.stop();
             const state = app.state.get();
             [app.session.snapshot().document.objects.length, ran.simulated, typeof state]"
        ),
    );
    req.dry_run = true;
    let r = s.run_script(&Caller::user(), &req);
    assert_eq!(r.outcome.error, None, "{r:?}");
    assert!(r.dry_run);
    assert_eq!(r.outcome.value, json!([2, true, "object"]));
    assert_eq!(r.simulated, ["job.run", "machine.jog", "machine.stop"]);
    assert_eq!(snapshot(&s)["document"]["objects"].as_array().unwrap().len(), 0);
    assert_eq!(snapshot(&s)["undo"], json!([]));
    assert!(
        events.try_iter().all(|e| !matches!(e, ServiceEvent::Session(_))),
        "a dry run publishes no session change"
    );
    // tiers still apply in a dry run
    let mut req = ScriptRequest::new("dry", "app.job.run({})");
    req.dry_run = true;
    let r = s.run_script(&macro_caller(Tier::Edit), &req);
    assert!(r.outcome.error.unwrap().contains("permission denied"));
    let mut req = ScriptRequest::new("dry", "app.nothing.here()");
    req.dry_run = true;
    assert!(s.run_script(&Caller::user(), &req).outcome.error.is_some());
}

#[test]
fn laser_actions_need_a_person() {
    let s = Service::in_memory().unwrap();
    // refused without a confirmation, even for the user
    let r = s.run_script(&Caller::user(), &ScriptRequest::new("l", "app.job.run({})"));
    let err = r.outcome.error.unwrap();
    assert!(err.contains("needs a person to confirm"), "{err}");
    assert_eq!(r.outcome.actions[0].action, "job.run");
    // with a confirmation it reaches the service (which refuses: nothing loaded)
    let asked = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&asked);
    let mut req = ScriptRequest::new("l", "app.job.run({})");
    req.confirm_laser = Some(Arc::new(move |action, _| {
        flag.store(true, Ordering::SeqCst);
        action == "job.run"
    }));
    let err = s.run_script(&Caller::user(), &req).outcome.error.unwrap();
    assert!(asked.load(Ordering::SeqCst));
    assert!(!err.contains("needs a person"), "{err}");
    // a caller below `laser` gets the tier error, and is never asked
    asked.store(false, Ordering::SeqCst);
    let err = s
        .run_script(&macro_caller(Tier::Machine), &req)
        .outcome
        .error
        .unwrap();
    assert!(err.contains("permission denied"), "{err}");
    assert!(!asked.load(Ordering::SeqCst));
}

#[test]
fn tiers_apply_inside_scripts() {
    let s = Service::in_memory().unwrap();
    let r = run(&s, &macro_caller(Tier::Read), "");
    assert!(r.outcome.error.unwrap().contains("permission denied"));
    let r = s.run_script(
        &macro_caller(Tier::Read),
        &ScriptRequest::new("r", "app.session.snapshot().revision >= 0"),
    );
    assert_eq!(r.outcome.value, json!(true));
    // settings stay the user's
    let r = s.run_script(
        &macro_caller(Tier::Edit),
        &ScriptRequest::new("r", "try { app.settings.set({key: 'automation.mcp_enabled', value: true}); 'set' } catch (e) { 'refused' }"),
    );
    assert_eq!(r.outcome.value, json!("refused"));
}

#[test]
fn stop_cancels_running_scripts() {
    let s = Service::in_memory().unwrap();
    // machine.stop from the script itself
    let r = s.run_script(
        &Caller::user(),
        &ScriptRequest::new(
            "stop",
            "app.view.setMode({mode: 'produce'}); app.machine.stop(); while (true) {}",
        ),
    );
    assert_eq!(r.outcome.error.as_deref(), Some("script stopped: cancelled"));
    assert!(r.rolled_back);
    assert_eq!(snapshot(&s)["view"]["mode"], json!("design"));
    // STOP from elsewhere, while another script waits
    let events = s.subscribe();
    let runner = s.clone();
    let handle = std::thread::spawn(move || {
        runner.run_script(
            &Caller::user(),
            &ScriptRequest::new("loop", "app.view.setMode({mode: 'produce'}); while (true) {}"),
        )
    });
    let started = events
        .iter()
        .find(|e| matches!(e, ServiceEvent::Session(_)))
        .is_some();
    assert!(started);
    // only one live script at a time
    let second = s.run_script(&Caller::user(), &ScriptRequest::new("second", "1"));
    assert_eq!(second.outcome.error.as_deref(), Some("another script is running"));
    s.stop();
    let r = handle.join().unwrap();
    assert_eq!(r.outcome.error.as_deref(), Some("script stopped: cancelled"));
    // a dry run can run while nothing else does, and is cancelled the same way
    let mut req = ScriptRequest::new("dry", "while (true) {}");
    req.dry_run = true;
    req.limits.timeout = Duration::from_millis(50);
    assert_eq!(
        s.run_script(&Caller::user(), &req).outcome.error.as_deref(),
        Some("script stopped: timeout")
    );
}

#[test]
fn api_declarations() {
    let s = Service::in_memory().unwrap();
    let d = s.api_dts();
    assert!(d.contains("interface AppDocument {"));
    assert!(d.contains("  importData(params: {"));
    assert!(d.contains("  delete(params?: {"));
    assert!(d.contains("tier `laser`"));
}
