//! A golden program streamed by the service to the emulator over a serial PTY.

use std::time::{Duration, Instant};

use freekerf_service::{Caller, Service, ServiceEvent};
use freekerf_testkit::fixtures::gcode_fixture;
use serde_json::{Value, json};

fn wait(s: &Service, pred: impl Fn(&Value) -> bool) -> Value {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let st = s.dispatch(&Caller::user(), "state.get", Value::Null).unwrap();
        if pred(&st) {
            return st;
        }
        assert!(Instant::now() < end, "timeout: {st}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn every_golden_program_streams_without_errors() {
    let emu = freekerf_transport::pty::serve_emulator(freekerf_emulator::DeviceConfig::default()).unwrap();
    let s = Service::in_memory().unwrap();
    let user = Caller::user();
    s.dispatch(&user, "machine.connect", json!({"port": emu.path()})).unwrap();
    wait(&s, |st| st["machine"]["ready"] == true && st["machine"]["status"] == "Idle");
    for name in ["square", "arcs", "relative", "raster_like", "offsets"] {
        let events = s.subscribe();
        let path = gcode_fixture(&format!("gcode/{name}.nc"));
        s.dispatch(&user, "job.load", json!({"path": path.to_string_lossy()})).unwrap();
        s.dispatch(&user, "job.run", Value::Null).unwrap();
        let end = Instant::now() + Duration::from_secs(20);
        let mut done = None;
        while done.is_none() && Instant::now() < end {
            if let Ok(e @ ServiceEvent::Machine(_)) = events.recv_timeout(Duration::from_millis(100))
                && let Some(j) = e.to_json()["data"].get("JobEnded")
            {
                done = Some(j.clone());
            }
        }
        let done = done.unwrap_or_else(|| panic!("{name}: job did not end"));
        assert_eq!(done["errors"], 0, "{name}");
        wait(&s, |st| st["machine"]["status"] == "Idle" && st["machine"]["ready"] == true);
    }
    s.dispatch(&user, "machine.disconnect", Value::Null).unwrap();
    emu.stop();
}
