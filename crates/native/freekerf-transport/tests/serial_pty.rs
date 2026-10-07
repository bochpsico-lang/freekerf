//! The serial transport against the emulator served on a PTY (no real hardware).
#![cfg(unix)]
#![allow(missing_docs)]

use std::time::Duration;

use freekerf_emulator::DeviceConfig;
use freekerf_transport::pty::serve_emulator;
use freekerf_transport::{LinkReader, SerialTransport, Transport, TransportError};

fn read_until(r: &mut dyn LinkReader, needle: &str) -> String {
    let mut text = String::new();
    let mut buf = [0u8; 256];
    for _ in 0..300 {
        let n = r.read(&mut buf, Duration::from_millis(20)).unwrap();
        text.push_str(&String::from_utf8_lossy(&buf[..n]));
        if text.contains(needle) {
            return text;
        }
    }
    panic!("{needle:?} not received, got {text:?}");
}

#[test]
fn serial_transport_talks_to_the_emulator_on_a_pty() {
    let emu = serve_emulator(DeviceConfig::default()).unwrap();
    let t = SerialTransport::new(emu.path(), 115_200);
    assert!(t.describe().contains("@115200"));
    let mut link = t.open().unwrap();
    link.writer.write_all(b"?").unwrap();
    let text = read_until(link.reader.as_mut(), "<Idle|");
    assert!(text.contains("Grbl 1.1f"));
    link.writer.write_all(b"$I\n").unwrap();
    read_until(link.reader.as_mut(), "[OPT:V,15,128]");
    // hanging up the line is seen by the reader
    emu.stop();
    let mut buf = [0u8; 64];
    let mut closed = false;
    for _ in 0..100 {
        match link.reader.read(&mut buf, Duration::from_millis(20)) {
            Err(TransportError::Closed) => {
                closed = true;
                break;
            }
            Ok(_) => {}
            Err(e) => panic!("{e}"),
        }
    }
    assert!(closed, "hang-up not detected");
}

#[test]
fn opening_a_missing_port_fails() {
    let t = SerialTransport::new("/nonexistent/freekerf-test-port", 115_200);
    let e = t.open().unwrap_err();
    assert!(matches!(e, TransportError::Open { .. }));
    assert!(e.to_string().contains("freekerf-test-port"));
    let _ = freekerf_transport::serial::available_ports();
}
