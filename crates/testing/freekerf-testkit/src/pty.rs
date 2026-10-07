//! Pseudo terminals for serial tests (no real hardware is ever opened).

use std::os::fd::OwnedFd;

/// The two sides of a PTY: the test keeps `master`, the code under test opens `path`.
#[derive(Debug)]
pub struct Pty {
    /// Master side (the "device" end).
    pub master: OwnedFd,
    /// Slave side, kept open so the line stays up.
    pub slave: OwnedFd,
    /// Path of the slave (`/dev/pts/N`).
    pub path: String,
}

/// Opens a raw PTY pair.
///
/// # Errors
/// When the system has no free PTY.
pub fn open() -> nix::Result<Pty> {
    use nix::pty::openpty;
    use nix::sys::termios::{SetArg, cfmakeraw, tcgetattr, tcsetattr};
    let p = openpty(None, None)?;
    let mut t = tcgetattr(&p.slave)?;
    cfmakeraw(&mut t);
    tcsetattr(&p.slave, SetArg::TCSANOW, &t)?;
    let path = nix::unistd::ttyname(&p.slave)?.to_string_lossy().into_owned();
    Ok(Pty { master: p.master, slave: p.slave, path })
}
