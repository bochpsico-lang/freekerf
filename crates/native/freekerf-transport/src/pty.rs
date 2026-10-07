//! The emulator behind a pseudo terminal (`freekerf emulate --pty`).

use std::os::fd::{AsFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use freekerf_emulator::{Device, DeviceConfig};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

use crate::TransportError;

/// An emulator served on a PTY. Dropping it hangs up the line.
#[derive(Debug)]
pub struct PtyEmulator {
    path: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl PtyEmulator {
    /// Path a program opens as a serial port (`/dev/pts/N`).
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Stops the emulator and hangs up.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for PtyEmulator {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn err(e: nix::Error) -> TransportError {
    TransportError::Open { target: "pty".into(), reason: e.to_string() }
}

/// Opens a PTY and runs an emulated device behind it. The banner is printed when
/// the first byte arrives from the other side (a real board prints it when the
/// port opens and resets it).
///
/// # Errors
/// When no PTY can be opened.
pub fn serve_emulator(config: DeviceConfig) -> Result<PtyEmulator, TransportError> {
    use nix::pty::openpty;
    use nix::sys::termios::{SetArg, cfmakeraw, tcgetattr, tcsetattr};
    let p = openpty(None, None).map_err(err)?;
    let mut t = tcgetattr(&p.slave).map_err(err)?;
    cfmakeraw(&mut t);
    tcsetattr(&p.slave, SetArg::TCSANOW, &t).map_err(err)?;
    let path = nix::unistd::ttyname(&p.slave).map_err(err)?.to_string_lossy().into_owned();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let device = Device::new(DeviceConfig { send_welcome_on_open: false, ..config });
    let thread = thread::Builder::new()
        .name("freekerf-pty-emulator".into())
        .spawn(move || pump(device, &p.master, &p.slave, &flag))
        .map_err(|e| TransportError::Open { target: "pty".into(), reason: e.to_string() })?;
    Ok(PtyEmulator { path, stop, thread: Some(thread) })
}

fn pump(mut device: Device, master: &OwnedFd, _slave: &OwnedFd, stop: &AtomicBool) {
    let start = Instant::now();
    let now = || u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let mut buf = [0u8; 4096];
    let mut greeted = false;
    while !stop.load(Ordering::SeqCst) {
        let out = device.take_output();
        if !out.is_empty() && write_all(master, &out).is_err() {
            return;
        }
        let wait = device.next_deadline().map_or(20, |d| d.saturating_sub(now()).clamp(1, 20));
        let mut fds = [PollFd::new(master.as_fd(), PollFlags::POLLIN)];
        let ready = poll(&mut fds, PollTimeout::try_from(Duration::from_millis(wait)).unwrap_or(PollTimeout::ZERO));
        match ready {
            Ok(n) if n > 0 => match nix::unistd::read(master, &mut buf) {
                Ok(0) | Err(_) => {
                    // nobody has the slave open: wait for a client
                    thread::sleep(Duration::from_millis(20));
                }
                Ok(n) => {
                    if !greeted {
                        greeted = true;
                        device.announce();
                    }
                    device.receive(&buf[..n], now());
                }
            },
            _ => device.tick(now()),
        }
    }
}

fn write_all(fd: &OwnedFd, mut data: &[u8]) -> nix::Result<()> {
    while !data.is_empty() {
        let n = nix::unistd::write(fd, data)?;
        data = &data[n..];
    }
    Ok(())
}
