//! The built-in Grbl emulator as a transport (the "Emulator" connection).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use freekerf_emulator::{Device, DeviceConfig};

use crate::{Link, LinkReader, LinkWriter, Transport, TransportError};

/// Runs an emulated device on its own thread, with the real clock.
#[derive(Debug, Clone)]
pub struct EmulatorTransport {
    /// Device configuration (timed by default: motions take their time).
    pub config: DeviceConfig,
}

impl Default for EmulatorTransport {
    fn default() -> Self {
        Self { config: DeviceConfig { timed: true, ..DeviceConfig::default() } }
    }
}

struct Reader {
    rx: Receiver<Vec<u8>>,
    pending: Vec<u8>,
}

struct Writer {
    tx: Sender<Vec<u8>>,
    alive: Arc<AtomicBool>,
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
    }
}

impl LinkReader for Reader {
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        if self.pending.is_empty() {
            match self.rx.recv_timeout(timeout) {
                Ok(data) => self.pending = data,
                Err(RecvTimeoutError::Timeout) => return Ok(0),
                Err(RecvTimeoutError::Disconnected) => return Err(TransportError::Closed),
            }
        }
        let n = buf.len().min(self.pending.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

impl LinkWriter for Writer {
    fn write_all(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.tx.send(data.to_vec()).map_err(|_| TransportError::Closed)
    }
}

/// Moves bytes between a device and channels until the host side goes away.
pub(crate) fn run_device(mut device: Device, from_host: &Receiver<Vec<u8>>, to_host: &Sender<Vec<u8>>, alive: &AtomicBool) {
    let start = Instant::now();
    let now = || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    device.connected();
    loop {
        let out = device.take_output();
        if !out.is_empty() && to_host.send(out).is_err() {
            return;
        }
        if !alive.load(Ordering::SeqCst) {
            return;
        }
        let wait = device.next_deadline().map_or(20, |d| d.saturating_sub(now()).clamp(1, 20));
        match from_host.recv_timeout(Duration::from_millis(wait)) {
            Ok(data) => device.receive(&data, now()),
            Err(RecvTimeoutError::Timeout) => device.tick(now()),
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

impl Transport for EmulatorTransport {
    fn open(&self) -> Result<Link, TransportError> {
        let (host_tx, dev_rx) = unbounded::<Vec<u8>>();
        let (dev_tx, host_rx) = unbounded::<Vec<u8>>();
        let alive = Arc::new(AtomicBool::new(true));
        let device = Device::new(self.config.clone());
        let flag = Arc::clone(&alive);
        thread::Builder::new()
            .name("freekerf-emulator".into())
            .spawn(move || run_device(device, &dev_rx, &dev_tx, &flag))
            .map_err(|e| TransportError::Open { target: "emulator".into(), reason: e.to_string() })?;
        Ok(Link {
            reader: Box::new(Reader { rx: host_rx, pending: Vec::new() }),
            writer: Box::new(Writer { tx: host_tx, alive }),
        })
    }

    fn describe(&self) -> String {
        "emulator (Grbl 1.1)".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_until(r: &mut dyn LinkReader, needle: &str) -> String {
        let mut text = String::new();
        let mut buf = [0u8; 256];
        for _ in 0..200 {
            let n = r.read(&mut buf, Duration::from_millis(20)).unwrap();
            text.push_str(&String::from_utf8_lossy(&buf[..n]));
            if text.contains(needle) {
                return text;
            }
        }
        panic!("{needle:?} not received, got {text:?}");
    }

    #[test]
    fn emulator_link_round_trip() {
        let t = EmulatorTransport::default();
        assert!(t.describe().contains("emulator"));
        let mut link = t.open().unwrap();
        read_until(link.reader.as_mut(), "Grbl 1.1f");
        link.writer.write_all(b"?").unwrap();
        read_until(link.reader.as_mut(), "<Idle|");
        link.writer.write_all(b"G1 X1 F6000\n").unwrap();
        read_until(link.reader.as_mut(), "ok");
        // small reads split the pending data
        link.writer.write_all(b"$I\n").unwrap();
        let mut one = [0u8; 1];
        let mut got = Vec::new();
        for _ in 0..500 {
            if link.reader.read(&mut one, Duration::from_millis(5)).unwrap() == 1 {
                got.push(one[0]);
                if got.ends_with(b"ok\r\n") {
                    break;
                }
            }
        }
        assert!(String::from_utf8_lossy(&got).contains("[OPT:V,15,128]"));
        drop(link.writer);
        // the device thread stops; the reader then sees the link closed
        let mut buf = [0u8; 16];
        let mut closed = false;
        for _ in 0..200 {
            match link.reader.read(&mut buf, Duration::from_millis(10)) {
                Err(TransportError::Closed) => {
                    closed = true;
                    break;
                }
                Ok(_) => {}
                Err(e) => panic!("{e}"),
            }
        }
        assert!(closed);
    }
}
