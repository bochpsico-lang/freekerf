//! The sandbox (PLAN §9.2, §9.6): nothing but the `app` API gets out, and limits stop
//! runaway scripts.

#![allow(clippy::unwrap_used, missing_docs)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use freekerf_script::{Limits, ScriptHost, run};
use freekerf_session::{ActionError, ActionSpec, Tier};
use serde_json::{Value, json};

#[derive(Default)]
struct Counter(AtomicUsize);

impl ScriptHost for Counter {
    fn specs(&self) -> Vec<ActionSpec> {
        vec![ActionSpec {
            name: "test.ping".into(),
            description: "Counts calls.".into(),
            tier: Tier::Read,
            params: json!({}),
        }]
    }
    fn dispatch(&self, _: &str, _: Value) -> Result<Value, ActionError> {
        Ok(json!(self.0.fetch_add(1, Ordering::SeqCst) + 1))
    }
}

fn eval(code: &str) -> freekerf_script::ScriptOutcome {
    run(
        code,
        Arc::new(Counter::default()),
        &Limits::default(),
        &Arc::new(AtomicBool::new(false)),
    )
}

#[test]
fn nothing_but_app_and_console() {
    let out = eval(
        "[typeof require, typeof process, typeof std, typeof os, typeof fetch, typeof setTimeout, \
          typeof XMLHttpRequest, typeof __fk_dispatch, typeof __fk_log, typeof Deno, typeof scriptArgs]",
    );
    assert_eq!(out.error, None);
    assert!(
        out.value.as_array().unwrap().iter().all(|v| v == "undefined"),
        "{:?}",
        out.value
    );
    // no module loader
    let imported = eval("import('fs')");
    assert!(
        imported.error.is_some() || imported.value == json!({}),
        "{imported:?}"
    );
    assert!(eval("import * as fs from 'fs'; 1").error.is_some());
    // the global object reached another way has nothing more either
    let g =
        eval("const g = Function('return this')(); [typeof g.__fk_dispatch, Object.keys(g).includes('app')]");
    assert_eq!(g.value, json!(["undefined", false]));
}

#[test]
fn the_api_cannot_be_tampered_with() {
    for code in [
        "'use strict'; app.test = {}",
        "'use strict'; app.test.ping = () => 1",
        "Object.defineProperty(app, 'x', {value: 1})",
        "'use strict'; delete app.test",
        "'use strict'; globalThis.app = {}",
        "'use strict'; console.log = null",
    ] {
        assert!(eval(code).error.is_some(), "{code}");
    }
    // tampering with built-ins only changes what the script itself sends
    let out = eval("JSON.stringify = () => '{\"evil\": true}'; app.test.ping({})");
    assert_eq!(out.error, None);
    assert_eq!(out.actions[0].action, "test.ping");
    assert_eq!(out.actions[0].params, json!({"evil": true}));
    // the dispatcher is not reachable through the functions
    let src = eval("app.test.ping.toString()");
    assert!(!src.value.as_str().unwrap().contains("native"));
    assert!(
        eval("app.test.ping.prototype.constructor('return dispatch')()")
            .error
            .is_some()
    );
}

#[test]
fn limits_stop_runaway_scripts() {
    let quick = Limits {
        timeout: Duration::from_millis(300),
        ..Limits::default()
    };
    let flag = Arc::new(AtomicBool::new(false));
    let out = run("while (true) {}", Arc::new(Counter::default()), &quick, &flag);
    assert_eq!(out.error.as_deref(), Some("script stopped: timeout"));
    assert!(out.elapsed_ms < 3000);

    let few = Limits {
        max_interrupts: 3,
        ..Limits::default()
    };
    let out = run("for (;;) {}", Arc::new(Counter::default()), &few, &flag);
    assert_eq!(out.error.as_deref(), Some("script stopped: instruction limit"));

    let small = Limits {
        memory_bytes: 4 << 20,
        ..Limits::default()
    };
    let out = run(
        "const a = []; for (;;) a.push(new Array(10000).fill(1));",
        Arc::new(Counter::default()),
        &small,
        &flag,
    );
    assert!(
        out.error.as_deref().unwrap().contains("memory") || out.error.as_deref().unwrap().contains("stopped"),
        "{out:?}"
    );

    let out = eval("function f() { return f() + 1 } f()");
    assert!(out.error.is_some(), "stack overflow is an error");

    // STOP: the cancel flag from another thread
    let cancel = Arc::new(AtomicBool::new(false));
    let c = Arc::clone(&cancel);
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        c.store(true, Ordering::SeqCst);
    });
    let out = run(
        "for (;;) {}",
        Arc::new(Counter::default()),
        &Limits::default(),
        &cancel,
    );
    t.join().unwrap();
    assert_eq!(out.error.as_deref(), Some("script stopped: cancelled"));
}

#[test]
fn many_calls_in_one_script() {
    let out = eval("let n = 0; for (let i = 0; i < 1000; i++) n = app.test.ping(); n");
    assert_eq!(out.value, json!(1000));
    assert_eq!(out.actions.len(), 1000);
}
