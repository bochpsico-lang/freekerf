//! The script engine benchmark of ADR 0011: startup, a 10,000-object batch edit,
//! memory. `cargo run --release -p freekerf-service --example script_bench`.
#![allow(clippy::print_stdout, clippy::unwrap_used, missing_docs)]

use std::time::Instant;

use freekerf_service::script::ScriptRequest;
use freekerf_service::{Caller, Service};

const OBJECTS: usize = 10_000;

fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn main() {
    let s = Service::in_memory().unwrap();
    let user = Caller::user();
    let rss0 = peak_rss_kib();

    // startup: engine, context and the generated `app` object, then a trivial script
    let runs = 200;
    let t = Instant::now();
    for _ in 0..runs {
        let r = s.run_script(&user, &ScriptRequest::new("startup", "1"));
        assert!(r.outcome.error.is_none());
    }
    let startup_ms = t.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);
    println!("startup (engine + app object + `1`): {startup_ms:.2} ms per run");

    // build a 10,000-object document from a script
    let build = format!(
        r#"let parts = ['<svg xmlns="http://www.w3.org/2000/svg" width="1000mm" height="1000mm" viewBox="0 0 1000 1000">'];
for (let i = 0; i < {OBJECTS}; i++) {{
  const x = (i % 100) * 10, y = Math.floor(i / 100) * 10;
  parts.push(`<rect x="${{x}}" y="${{y}}" width="5" height="5" stroke="red" fill="none"/>`);
}}
parts.push('</svg>');
app.document.importData({{format: "svg", text: parts.join("")}}).objects.length"#
    );
    let t = Instant::now();
    let r = s.run_script(&user, &ScriptRequest::new("build", build));
    assert_eq!(r.outcome.error, None);
    println!(
        "import {} objects built in JS: {:.0} ms",
        r.outcome.value,
        t.elapsed().as_secs_f64() * 1000.0
    );

    // batch edit: one action per object (the worst case for dispatch), one undo step
    let edit = "const objs = app.session.snapshot().document.objects;
for (const o of objs) app.document.translate({dx: 1, dy: 0, ids: [o.id]});
objs.length";
    let t = Instant::now();
    let r = s.run_script(&user, &ScriptRequest::new("edit", edit));
    assert_eq!(r.outcome.error, None);
    let per_object = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "translate {} objects one action each: {per_object:.0} ms ({:.1} µs per action)",
        r.outcome.value,
        per_object * 1000.0 / OBJECTS as f64
    );

    // the same edit as one action
    let t = Instant::now();
    let r = s.run_script(
        &user,
        &ScriptRequest::new(
            "edit-all",
            "app.document.selectAll(); app.document.translate({dx: 1, dy: 0})",
        ),
    );
    assert_eq!(r.outcome.error, None);
    println!(
        "translate all objects in one action: {:.1} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );

    if let (Some(a), Some(b)) = (rss0, peak_rss_kib()) {
        println!(
            "peak resident memory: {:.1} MiB (before the scripts: {:.1} MiB)",
            b as f64 / 1024.0,
            a as f64 / 1024.0
        );
    }
}
