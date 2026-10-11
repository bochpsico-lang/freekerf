//! Permission grants (PLAN §9.5, §9.6): each scope expires when it should, editing
//! a macro invalidates its grants, revoke works at once, denial is readable,
//! headless refuses anything that needs a person, and no grant skips the laser
//! confirmation.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use freekerf_service::permissions::{GrantScope, NoOnePresent, PermissionRequest, Prompter, RunGrants};
use freekerf_service::script::ScriptRequest;
use freekerf_service::{Caller, CallerKind, FileStore, Service, Stores, Tier};
use freekerf_settings::Settings;
use serde_json::Value;

/// Answers from a queue and records the questions.
#[derive(Default)]
struct Person {
    answers: Mutex<VecDeque<Option<GrantScope>>>,
    asked: Mutex<Vec<(String, String, Tier)>>,
}

impl Person {
    fn will(&self, answer: Option<GrantScope>) {
        self.answers.lock().unwrap().push_back(answer);
    }
    fn asked(&self) -> usize {
        self.asked.lock().unwrap().len()
    }
}

impl Prompter for Person {
    fn ask(&self, r: &PermissionRequest<'_>) -> Option<GrantScope> {
        self.asked
            .lock()
            .unwrap()
            .push((r.caller.name.clone(), r.action.to_owned(), r.tier));
        self.answers.lock().unwrap().pop_front().flatten()
    }
}

/// `machine.disconnect` (tier `machine`) succeeds when nothing is connected.
const MACHINE: &str = "app.machine.disconnect(); 'ok'";

fn script(s: &Service, caller: &Caller, code: &str) -> Result<Value, String> {
    let r = s.run_script(caller, &ScriptRequest::new(caller.name.clone(), code));
    r.outcome.error.map_or(Ok(r.outcome.value), Err)
}

fn with_person() -> (Service, Arc<Person>) {
    let s = Service::in_memory().unwrap();
    let p = Arc::new(Person::default());
    s.set_prompter(p.clone());
    (s, p)
}

#[test]
fn scopes_expire_when_they_should() {
    let (s, person) = with_person();
    let m = Caller::new(CallerKind::Macro, "frame.js", Tier::Edit);
    // this run: asked once per run, not again inside the run
    person.will(Some(GrantScope::Run));
    assert_eq!(script(&s, &m, &format!("{MACHINE}; {MACHINE}")).unwrap(), "ok");
    assert_eq!(person.asked(), 1);
    person.will(None);
    let err = script(&s, &m, MACHINE).unwrap_err();
    assert!(
        err.contains("permission denied: `machine.disconnect` needs tier `machine`"),
        "{err}"
    );
    assert_eq!(person.asked(), 2);
    // this session: kept until the service goes away
    person.will(Some(GrantScope::Session));
    script(&s, &m, MACHINE).unwrap();
    script(&s, &m, MACHINE).unwrap();
    assert_eq!(person.asked(), 3);
    assert_eq!(s.grants().len(), 1);
    assert_eq!(s.grants()[0].scope, GrantScope::Session);
    // editing the macro invalidates it
    person.will(None);
    assert!(script(&s, &m, &format!("{MACHINE} // edited")).is_err());
    assert_eq!(person.asked(), 4);
    // revoke works at once
    assert_eq!(s.revoke(CallerKind::Macro, "frame.js").unwrap(), 1);
    person.will(None);
    assert!(script(&s, &m, MACHINE).is_err());
    assert_eq!(person.asked(), 5);
    // another service (the app restarted) forgets session grants
    let (s2, p2) = with_person();
    p2.will(None);
    assert!(script(&s2, &m, MACHINE).is_err());
}

#[test]
fn always_grants_are_stored_and_survive_restarts() {
    let home = tempfile::tempdir().unwrap();
    let open = || Service::new(Stores::files(Some(home.path())).unwrap()).unwrap();
    let s = open();
    let person = Arc::new(Person::default());
    s.set_prompter(person.clone());
    let agent = Caller::new(CallerKind::Agent, "claude", Tier::Edit);
    person.will(Some(GrantScope::Always));
    s.dispatch_authorized(
        &agent,
        None,
        "machine.disconnect",
        Value::Null,
        &RunGrants::default(),
        true,
    )
    .unwrap();
    let stored = Settings::load(&FileStore(home.path().join("settings.toml"))).unwrap();
    assert_eq!(stored.automation.grants.len(), 1);
    assert_eq!(stored.automation.grants[0].tier, Tier::Machine);
    drop(s);
    // headless after a restart: the "always" grant applies, nothing else does
    let s = open();
    assert!(!s.human_present());
    s.dispatch_authorized(
        &agent,
        None,
        "machine.disconnect",
        Value::Null,
        &RunGrants::default(),
        true,
    )
    .unwrap();
    let other = Caller::new(CallerKind::Agent, "other", Tier::Edit);
    let err = s
        .dispatch_authorized(
            &other,
            None,
            "machine.disconnect",
            Value::Null,
            &RunGrants::default(),
            true,
        )
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"));
    // an "always" grant is still limited to its tier
    assert!(
        s.dispatch_authorized(&agent, None, "job.run", Value::Null, &RunGrants::default(), true)
            .unwrap_err()
            .to_string()
            .contains("needs tier `laser`")
    );
    assert_eq!(s.revoke(CallerKind::Agent, "claude").unwrap(), 1);
    assert!(
        Settings::load(&FileStore(home.path().join("settings.toml")))
            .unwrap()
            .automation
            .grants
            .is_empty()
    );
    assert!(
        s.dispatch_authorized(
            &agent,
            None,
            "machine.disconnect",
            Value::Null,
            &RunGrants::default(),
            true
        )
        .is_err()
    );
}

#[test]
fn headless_asks_no_one() {
    let s = Service::in_memory().unwrap();
    s.set_prompter(Arc::new(NoOnePresent));
    assert!(!s.human_present());
    let m = Caller::new(CallerKind::Macro, "m", Tier::Edit);
    assert!(script(&s, &m, MACHINE).unwrap_err().contains("permission denied"));
    // below the baseline nothing is asked
    assert_eq!(
        script(&s, &m, "app.view.setMode({mode: 'produce'}); 1").unwrap(),
        1
    );
}

#[test]
fn no_grant_skips_the_laser_confirmation() {
    let (s, person) = with_person();
    let m = Caller::new(CallerKind::Macro, "burn.js", Tier::Edit);
    person.will(Some(GrantScope::Session));
    let err = script(&s, &m, "app.job.run({})").unwrap_err();
    assert!(err.contains("needs a person to confirm"), "{err}");
    assert_eq!(person.asked(), 1);
    // the grant is kept, the confirmation is still needed every time
    let err = script(&s, &m, "app.job.run({})").unwrap_err();
    assert!(err.contains("needs a person to confirm"), "{err}");
    assert_eq!(person.asked(), 1);
}

#[test]
fn dry_runs_check_without_asking() {
    let (s, person) = with_person();
    let m = Caller::new(CallerKind::Macro, "m", Tier::Edit);
    let mut req = ScriptRequest::new("m", MACHINE);
    req.dry_run = true;
    let r = s.run_script(&m, &req);
    assert!(r.outcome.error.unwrap().contains("permission denied"));
    assert_eq!(person.asked(), 0);
}

#[test]
fn baselines_come_from_the_settings() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("settings.toml");
    let s = Service::new(Stores::files(Some(home.path())).unwrap()).unwrap();
    assert_eq!(s.baseline(CallerKind::Agent), None);
    assert_eq!(s.baseline(CallerKind::PythonClient), Some(Tier::Edit));
    assert_eq!(s.baseline(CallerKind::Macro), None);
    let mut st = Settings::default();
    st.automation.mcp.enabled = true;
    st.automation.python.allow_machine = true;
    std::fs::write(&path, st.to_toml().unwrap()).unwrap();
    let s = Service::new(Stores::files(Some(home.path())).unwrap()).unwrap();
    assert_eq!(s.baseline(CallerKind::Agent), Some(Tier::Edit));
    assert_eq!(s.baseline(CallerKind::PythonClient), Some(Tier::Machine));
    st.automation.mcp.allow_machine = true;
    std::fs::write(&path, st.to_toml().unwrap()).unwrap();
    let s = Service::new(Stores::files(Some(home.path())).unwrap()).unwrap();
    assert_eq!(s.baseline(CallerKind::Agent), Some(Tier::Machine));
    st.automation.mcp.allow_laser = true;
    std::fs::write(&path, st.to_toml().unwrap()).unwrap();
    let s = Service::new(Stores::files(Some(home.path())).unwrap()).unwrap();
    assert_eq!(s.baseline(CallerKind::Agent), Some(Tier::Laser));
}
