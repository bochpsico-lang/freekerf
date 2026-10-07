//! The black-box protocol scenarios of `conformance/tests/protocol`, run in memory
//! against the emulator with a simulated clock (the same scenarios also run over a
//! real PTY through `freekerf host --stdio` and the conformance suite).
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use freekerf_emulator::DeviceConfig;
use freekerf_gcode::Decimal;
use freekerf_protocol::{
    Event, HostOptions, Issue, JogDirection, MachineStatus, StreamingMode, SystemOpKind,
};
use freekerf_testkit::{Bench, job_lines, wire};
use regex::Regex;

const HEADER: &str = "G90";
const FOOTER: &str = "G0X0Y0Z0";

fn classic() -> DeviceConfig {
    DeviceConfig {
        report_buffer: false,
        opt_line: "[OPT:V,15,127]".into(),
        ..DeviceConfig::default()
    }
}

fn next_fits(b: &Bench, expected: &[String], budget: usize) -> bool {
    let i = b.device.lines().len();
    i < expected.len() && b.device.rx_used() + expected[i].len() < budget
}

// ---------------------------------------------------------------- connection

#[test]
fn connect_sends_soft_reset_then_status_query_then_reads_settings() {
    let mut b = Bench::new();
    b.open();
    assert!(b.run_until(
        |b| b.host.snapshot().connected && b.lines().contains(&"$I".to_owned()),
        10_000
    ));
    assert_eq!(b.device.realtime()[0], 0x18);
    assert_eq!(b.device.realtime()[1], 0x3F);
    assert_eq!(b.lines()[..2], ["$$".to_owned(), "$I".to_owned()]);
    assert_eq!(b.host.version().unwrap().to_string(), "1.1f");
    assert!(b.run_until(|b| b.host.status() == MachineStatus::Idle, 5000));
    assert!(b.run_until(|b| b.host.ready(), 5000));
    assert_eq!(b.host.settings().max_rate_x(), Decimal::from(6000));
    assert_eq!(b.host.buffer_size(), 128);
}

#[test]
fn connect_without_soft_reset_waits_for_the_board_banner() {
    let opts = HostOptions {
        reset_on_connect: false,
        query_machine_info: false,
        ..HostOptions::default()
    };
    let mut b = Bench::with(
        opts,
        DeviceConfig {
            send_welcome_on_open: false,
            ..DeviceConfig::default()
        },
    );
    b.open();
    assert!(b.device.realtime().contains(&0x3F));
    assert!(!b.device.realtime().contains(&0x18));
    b.device.announce();
    b.exchange();
    assert_eq!(b.host.version().unwrap().to_string(), "1.1f");
    assert!(b.run_until(|b| b.lines().contains(&"$$".to_owned()), 5000));
    assert!(!b.lines().contains(&"$I".to_owned()));
}

#[test]
fn status_is_polled_periodically() {
    let mut b = Bench::new();
    b.connect();
    b.advance(1600);
    assert!(b.device.realtime().iter().filter(|r| **r == 0x3F).count() >= 3);
}

#[test]
fn disconnect_closes_the_line() {
    let mut b = Bench::new();
    b.connect();
    b.host.close(true, b.now);
    assert!(!b.host.snapshot().connected);
    assert_eq!(b.host.status(), MachineStatus::Disconnected);
}

// ---------------------------------------------------------------- streaming

#[test]
fn stream_sends_header_job_and_footer_in_order() {
    let mut b = Bench::new();
    b.connect();
    let lines = job_lines(120);
    b.load(&lines.join("\n"));
    b.host.run_program(false, b.now).unwrap();
    b.exchange();
    assert!(b.run_until(|b| b.lines().contains(&FOOTER.to_owned()), 20_000));
    let mut expected = vec![HEADER.to_owned()];
    expected.extend(lines.iter().map(|l| wire(l)));
    expected.push(FOOTER.to_owned());
    assert_eq!(b.lines(), expected);
    assert!(b.run_until(|b| !b.host.in_program(), 5000));
    assert!(b.events.iter().any(|e| matches!(
        e,
        Event::JobEnded {
            lines: 120,
            errors: 0,
            ..
        }
    )));
}

fn stream_with_budget(b: &mut Bench, n: usize, budget: usize) {
    b.device.set_auto_ack(false);
    let lines = job_lines(n);
    let mut expected = vec![HEADER.to_owned()];
    expected.extend(lines.iter().map(|l| wire(l)));
    b.load(&lines.join("\n"));
    b.host.run_program(false, b.now).unwrap();
    b.exchange();
    while b.device.lines().len() < expected.len() {
        // the host filled the buffer as much as it could
        assert!(!next_fits(b, &expected, budget), "host left room in the buffer");
        b.device.release(1);
        b.exchange();
    }
    b.device.set_auto_ack(true);
    b.exchange();
    assert!(b.run_until(|b| b.lines().contains(&FOOTER.to_owned()), 10_000));
    expected.push(FOOTER.to_owned());
    assert_eq!(b.lines(), expected);
    assert_eq!(b.device.overflows(), 0);
}

#[test]
fn stream_never_overflows_the_127_byte_rx_buffer() {
    let mut b = Bench::with(HostOptions::default(), classic());
    b.connect();
    stream_with_budget(&mut b, 60, 127);
    assert!(127 - 21 < b.device.max_rx_used() && b.device.max_rx_used() <= 127);
}

#[test]
fn stream_uses_128_bytes_when_grbl_reports_it() {
    let mut b = Bench::new();
    b.connect();
    stream_with_budget(&mut b, 60, 128);
    assert!(b.device.max_rx_used() <= 128);
}

#[test]
fn stream_waits_when_rx_buffer_full() {
    let mut b = Bench::with(HostOptions::default(), classic());
    b.connect();
    b.device.set_auto_ack(false);
    let lines = job_lines(40);
    let mut expected = vec![HEADER.to_owned()];
    expected.extend(lines.iter().map(|l| wire(l)));
    b.load(&lines.join("\n"));
    b.host.run_program(false, b.now).unwrap();
    b.exchange();
    assert!(!next_fits(&b, &expected, 127));
    let sent = b.device.lines().len();
    b.advance(500);
    assert_eq!(b.device.lines().len(), sent);
    // the first ok frees only the 4-byte header: still nothing fits
    b.device.release(1);
    b.exchange();
    assert_eq!(b.device.lines().len(), sent);
    b.device.release(1);
    b.exchange();
    assert!(b.device.lines().len() > sent);
    b.device.set_auto_ack(true);
    assert!(b.run_until(|b| b.lines().contains(&FOOTER.to_owned()), 10_000));
}

#[test]
fn synchronous_mode_keeps_one_line_in_flight() {
    let opts = HostOptions {
        streaming_mode: StreamingMode::Synchronous,
        ..HostOptions::default()
    };
    let mut b = Bench::with(opts, DeviceConfig::default());
    b.connect();
    b.device.set_auto_ack(false);
    b.load(&job_lines(10).join("\n"));
    b.host.run_program(false, b.now).unwrap();
    b.exchange();
    assert_eq!(b.device.pending_lines(), 1);
    b.advance(300);
    assert_eq!(b.device.pending_lines(), 1);
    b.device.set_auto_ack(true);
    assert!(b.run_until(|b| b.lines().contains(&FOOTER.to_owned()), 10_000));
}

// ---------------------------------------------------------------- pause / resume / reset

fn run_manual(b: &mut Bench, n: usize) {
    b.device.set_auto_ack(false);
    b.load(&job_lines(n).join("\n"));
    b.host.run_program(false, b.now).unwrap();
    assert!(b.run_until(|b| b.host.status() == MachineStatus::Run, 5000));
}

#[test]
fn feed_hold_pauses_and_cycle_start_resumes() {
    let mut b = Bench::new();
    b.connect();
    run_manual(&mut b, 30);
    b.host.feed_hold(b.now).unwrap();
    b.exchange();
    assert!(b.device.realtime().contains(&0x21));
    assert!(b.run_until(|b| b.host.status() == MachineStatus::Hold, 5000));
    b.device.set_auto_ack(true);
    let executed = b.device.responses().len();
    b.advance(300);
    assert!(b.device.hold());
    b.host.cycle_start(b.now).unwrap();
    b.exchange();
    assert!(b.device.realtime().contains(&0x7E));
    assert!(b.run_until(|b| b.lines().contains(&FOOTER.to_owned()), 10_000));
    assert!(b.device.responses().len() > executed);
}

#[test]
fn soft_reset_mid_job_stops_streaming() {
    let mut b = Bench::new();
    b.connect();
    run_manual(&mut b, 80);
    b.host.soft_reset(b.now).unwrap();
    b.exchange();
    assert!(b.device.resets() >= 1);
    let after = b.device.lines().len();
    b.advance(500);
    assert_eq!(b.device.lines().len(), after);
    assert!(!b.lines().contains(&FOOTER.to_owned()));
    assert!(!b.host.in_program());
    assert!(!b.issues().contains(&Issue::ManualReset));
}

#[test]
fn abort_job_flushes_and_turns_laser_off() {
    let mut b = Bench::new();
    b.connect();
    run_manual(&mut b, 80);
    b.host.abort_program(b.now).unwrap();
    b.device.set_auto_ack(true);
    assert!(b.run_until(|b| b.lines().last().map(String::as_str) == Some("M5"), 5000));
    assert!(!b.lines().contains(&FOOTER.to_owned()));
    assert!(!b.host.in_program());
}

// ---------------------------------------------------------------- errors and alarms

#[test]
fn error_mid_job_is_counted_and_streaming_continues() {
    let mut b = Bench::new();
    b.connect();
    let lines = job_lines(20);
    b.device.config_mut().error_rules = vec![(Regex::new(&format!("{}$", wire(&lines[5]))).unwrap(), 33)];
    b.load(&lines.join("\n"));
    b.host.run_program(false, b.now).unwrap();
    assert!(b.run_until(|b| b.lines().contains(&FOOTER.to_owned()), 10_000));
    assert_eq!(b.lines().iter().filter(|l| **l == wire(&lines[5])).count(), 1);
    assert_eq!(b.host.projection().errors(), 1);
}

#[test]
fn error_mid_job_is_retried_in_repeat_on_error_mode() {
    let opts = HostOptions {
        streaming_mode: StreamingMode::RepeatOnError,
        ..HostOptions::default()
    };
    let mut b = Bench::with(opts, DeviceConfig::default());
    b.connect();
    let lines = job_lines(8);
    b.device.config_mut().error_rules = vec![(Regex::new(&format!("{}$", wire(&lines[3]))).unwrap(), 33)];
    b.load(&lines.join("\n"));
    b.host.run_program(false, b.now).unwrap();
    assert!(b.run_until(|b| b.lines().contains(&FOOTER.to_owned()), 10_000));
    assert_eq!(b.lines().iter().filter(|l| **l == wire(&lines[3])).count(), 4);
}

#[test]
fn alarm_mid_job_is_reported() {
    let mut b = Bench::new();
    b.connect();
    run_manual(&mut b, 50);
    b.device.alarm_now(1);
    b.exchange();
    assert!(b.issues().contains(&Issue::MachineAlarm));
    assert!(b.run_until(|b| b.host.status() == MachineStatus::Alarm, 5000));
}

#[test]
fn unlock_and_home() {
    let mut b = Bench::new();
    b.connect();
    let id = b.host.write_settings(&[(22, "1".into())], b.now).unwrap();
    b.exchange();
    assert!(
        b.events
            .iter()
            .any(|e| matches!(e, Event::SystemOpFinished { id: i, result: Ok(()), .. } if *i == id))
    );
    b.host.unlock(b.now).unwrap();
    b.host.home(b.now).unwrap();
    b.exchange();
    let l = b.lines();
    assert_eq!(l[l.len() - 2..], ["$X".to_owned(), "$H".to_owned()]);
}

// ---------------------------------------------------------------- manual commands, jog, overrides, settings

#[test]
fn manual_command_is_sent_compressed() {
    let mut b = Bench::new();
    b.connect();
    b.host.send_command("g1 x10 y5 f300", b.now).unwrap();
    b.exchange();
    assert!(b.lines().contains(&"G1X10Y5F300".to_owned()));
}

#[test]
fn jog_uses_dollar_j_relative() {
    let mut b = Bench::new();
    b.connect();
    b.host
        .jog(JogDirection::NE, 800.0, Decimal::new(25, 1), b.now)
        .unwrap();
    b.exchange();
    assert_eq!(b.lines().last().unwrap(), "$J=G91X2.5Y2.5F800");
}

#[test]
fn overrides_converge_to_targets() {
    let mut b = Bench::new();
    b.connect();
    b.host.set_override_targets(130, 25, 80);
    assert!(b.run_until(|b| b.device.overrides() == [130, 25, 80], 10_000));
    for r in [0x91, 0x97, 0x9B] {
        assert!(b.device.realtime().contains(&r));
    }
    b.host.set_override_targets(100, 100, 100);
    assert!(b.run_until(|b| b.device.overrides() == [100, 100, 100], 10_000));
    for r in [0x90, 0x95, 0x99] {
        assert!(b.device.realtime().contains(&r));
    }
}

#[test]
fn write_settings_sends_one_dollar_line_per_setting() {
    let mut b = Bench::new();
    b.connect();
    b.host
        .write_settings(&[(110, "4000".into()), (111, "4500".into())], b.now)
        .unwrap();
    b.exchange();
    let l = b.lines();
    assert_eq!(l[l.len() - 2..], ["$110=4000".to_owned(), "$111=4500".to_owned()]);
    assert_eq!(b.device.settings()[&110], "4000");
    assert_eq!(b.host.settings().get(111), Some("4500"));
    assert!(b.events.iter().any(|e| matches!(
        e,
        Event::SystemOpFinished {
            kind: SystemOpKind::WriteSettings,
            result: Ok(()),
            ..
        }
    )));
}

#[test]
fn host_uses_the_rx_buffer_size_reported_by_the_board() {
    let dev = DeviceConfig {
        rx_size: 256,
        opt_line: "[OPT:V,15,256]".into(),
        report_buffer: false,
        ..DeviceConfig::default()
    };
    let mut b = Bench::with(HostOptions::default(), dev);
    b.connect();
    assert_eq!(b.host.buffer_size(), 256);
    b.device.set_auto_ack(false);
    b.load(&job_lines(40).join("\n"));
    b.host.run_program(false, b.now).unwrap();
    b.exchange();
    assert!(b.device.rx_used() > 200);
    b.device.set_auto_ack(true);
    assert!(b.run_until(|b| b.lines().contains(&FOOTER.to_owned()), 10_000));
    assert_eq!(b.device.overflows(), 0);
    assert!(b.device.max_rx_used() <= 256);
}
