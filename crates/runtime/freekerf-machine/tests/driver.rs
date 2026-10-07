//! The driver with the emulator transport (in process) and over a PTY.
#![allow(missing_docs)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use freekerf_gcode::{AnalysisOptions, Program};
use freekerf_machine::{Machine, MachineEvent, SystemClock};
use freekerf_protocol::{Event, FirmwareKind, Host, HostOptions, MachineStatus};
use freekerf_transport::{EmulatorTransport, SerialTransport};

fn wait(m: &Machine, mut pred: impl FnMut(&freekerf_protocol::HostSnapshot) -> bool, secs: u64) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if pred(&m.snapshot()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn machine() -> Machine {
    Machine::start(Host::for_kind(FirmwareKind::Grbl, HostOptions::default()), Arc::new(SystemClock::default()))
}

#[test]
fn runs_a_job_on_the_emulator() {
    let m = machine();
    let events = m.subscribe();
    m.connect(Box::new(EmulatorTransport::default())).unwrap();
    assert!(matches!(m.connect(Box::new(EmulatorTransport::default())), Err(freekerf_machine::MachineError::AlreadyConnected)));
    assert!(wait(&m, |s| s.ready && s.status == MachineStatus::Idle, 10));
    let program = Program::parse("G0 X1\nM3 S100\nG1 X2 F6000\nM5", &AnalysisOptions::default());
    m.call(move |h, now| {
        h.set_program(program).unwrap();
        h.run_program(false, now)
    })
    .unwrap()
    .unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    let mut ended = false;
    while Instant::now() < end && !ended {
        if let Ok(MachineEvent::Host(Event::JobEnded { lines, .. })) = events.recv_timeout(Duration::from_millis(100)) {
            assert_eq!(lines, 4);
            ended = true;
        }
    }
    assert!(ended);
    m.disconnect().unwrap();
    assert_eq!(m.snapshot().status, MachineStatus::Disconnected);
    assert!(format!("{m:?}").contains("Machine"));
}

#[test]
fn stop_bypasses_queued_work() {
    let m = machine();
    m.connect(Box::new(EmulatorTransport::default())).unwrap();
    assert!(wait(&m, |s| s.ready, 10));
    // a long job, then STOP
    let text: String = (0..200).map(|i| format!("G1 X{} F600\n", i % 2 * 10)).collect();
    let program = Program::parse(&text, &AnalysisOptions::default());
    m.call(move |h, now| {
        h.set_program(program).unwrap();
        h.run_program(false, now).unwrap();
    })
    .unwrap();
    assert!(wait(&m, |s| s.in_program, 5));
    m.stop();
    assert!(wait(&m, |s| !s.in_program, 5));
    m.disconnect().unwrap();
    m.stop(); // not connected: nothing happens
}

#[cfg(unix)]
#[test]
fn hang_up_on_a_pty_disconnects() {
    let emu = freekerf_transport::pty::serve_emulator(freekerf_emulator::DeviceConfig::default()).unwrap();
    let m = machine();
    let events = m.subscribe();
    m.connect(Box::new(SerialTransport::new(emu.path(), 115_200))).unwrap();
    assert!(wait(&m, |s| s.ready, 10));
    emu.stop();
    assert!(wait(&m, |s| s.status == MachineStatus::Disconnected, 5));
    let got: Vec<MachineEvent> = events.try_iter().collect();
    assert!(got.contains(&MachineEvent::Disconnected { user: false }));
}

#[test]
fn open_failure_is_reported() {
    let m = machine();
    let e = m.connect(Box::new(SerialTransport::new("/nonexistent/fk-port", 115_200))).unwrap_err();
    assert!(e.to_string().contains("fk-port"));
    assert_eq!(m.snapshot().failed_connections, 1);
}
