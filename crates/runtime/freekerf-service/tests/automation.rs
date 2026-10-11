//! Automation for the person in the window (PLAN §9.4): permission requests wait
//! for an answer from the window, scripts and agents are listed while they run,
//! their actions are logged, and only the person changes `automation.*`.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::time::Duration;

use freekerf_service::automation::AutomationEvent;
use freekerf_service::permissions::GrantScope;
use freekerf_service::script::ScriptRequest;
use freekerf_service::{Caller, CallerKind, Service, ServiceEvent, Tier};
use serde_json::{Value, json};

const MACHINE: &str = "app.machine.disconnect(); 'ok'";

/// Answers every permission request of `s` with `scope`, from another thread.
fn answer_all(s: &Service, scope: Option<GrantScope>) -> std::thread::JoinHandle<Vec<String>> {
    let events = s.subscribe();
    let s = s.clone();
    std::thread::spawn(move || {
        let mut asked = Vec::new();
        while let Ok(e) = events.recv_timeout(Duration::from_secs(5)) {
            if let ServiceEvent::Automation(AutomationEvent::PermissionRequested { request: p }) = e {
                let status = s
                    .dispatch(&Caller::user(), "automation.status", Value::Null)
                    .unwrap();
                assert_eq!(status["requests"][0]["id"], p.id);
                assert_eq!(status["scripts"][0]["name"], "frame.js");
                asked.push(format!("{} {} {}", p.actor.name, p.action, p.tier));
                assert!(s.answer_permission(p.id, scope));
                assert!(!s.answer_permission(p.id, scope), "answered once");
                return asked;
            }
        }
        asked
    })
}

#[test]
fn the_window_answers_permission_requests() {
    let s = Service::in_memory().unwrap();
    assert!(!s.human_present());
    s.install_window_prompter(Duration::from_secs(5));
    assert!(s.human_present());
    let m = Caller::new(CallerKind::Macro, "frame.js", Tier::Edit);
    let person = answer_all(&s, Some(GrantScope::Run));
    let r = s.run_script(&m, &ScriptRequest::new("frame.js", MACHINE));
    assert_eq!(r.outcome.error, None);
    assert_eq!(person.join().unwrap(), ["frame.js machine.disconnect machine"]);
    // denied
    let person = answer_all(&s, None);
    let r = s.run_script(&m, &ScriptRequest::new("frame.js", MACHINE));
    assert!(r.outcome.error.unwrap().contains("permission"));
    assert_eq!(person.join().unwrap().len(), 1);
    // nothing runs or waits any more
    let status = s
        .dispatch(&Caller::user(), "automation.status", Value::Null)
        .unwrap();
    assert_eq!(status, json!({"agents": [], "scripts": [], "requests": []}));
    // the log: the run, then the refusal; the person's own actions are not logged
    let log = s
        .dispatch(&Caller::user(), "automation.log", Value::Null)
        .unwrap();
    let log = log.as_array().unwrap();
    assert_eq!(log.len(), 2, "{log:?}");
    assert_eq!(log[0]["action"], "machine.disconnect");
    assert_eq!(log[0]["error"], Value::Null);
    assert_eq!(log[0]["actor"], json!({"kind": "macro", "name": "frame.js"}));
    assert!(log[1]["error"].as_str().unwrap().contains("permission"));
    let after = log[0]["seq"].as_u64().unwrap();
    let tail = s
        .dispatch(&Caller::user(), "automation.log", json!({"after": after}))
        .unwrap();
    assert_eq!(tail.as_array().unwrap().len(), 1);
}

#[test]
fn an_unanswered_request_is_denied_after_the_timeout() {
    let s = Service::in_memory().unwrap();
    s.install_window_prompter(Duration::from_millis(50));
    let m = Caller::new(CallerKind::Macro, "slow.js", Tier::Edit);
    let r = s.run_script(&m, &ScriptRequest::new("slow.js", MACHINE));
    assert!(r.outcome.error.unwrap().contains("permission"));
    assert!(!s.answer_permission(1, Some(GrantScope::Run)));
}

#[test]
fn only_the_person_changes_automation_settings() {
    let s = Service::in_memory().unwrap();
    let user = Caller::user();
    assert!(
        s.dispatch(
            &user,
            "settings.set",
            json!({"key": "automation.mcp.enabled", "value": true})
        )
        .is_err()
    );
    let events = s.subscribe();
    s.set_user_setting("automation.mcp.enabled", json!(true)).unwrap();
    assert_eq!(
        s.dispatch(&user, "settings.get", json!({"key": "automation.mcp.enabled"}))
            .unwrap(),
        json!(true)
    );
    assert!(events.try_iter().any(|e| e == ServiceEvent::SettingsChanged));
    assert_eq!(s.baseline(CallerKind::Agent), Some(Tier::Edit));
    assert!(s.set_user_setting("automation.nope", json!(1)).is_err());
}
