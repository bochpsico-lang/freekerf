//! Serial ports through the `serialport` crate.

use std::io::{ErrorKind, Read, Write};
use std::path::Path;
use std::time::Duration;

use crate::{Link, LinkReader, LinkWriter, Transport, TransportError};

/// A serial port (`/dev/ttyUSB0`, `COM3`, or a PTY in tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialTransport {
    /// Port name or path.
    pub path: String,
    /// Baud rate (Grbl: 115200).
    pub baud: u32,
}

impl SerialTransport {
    /// A serial transport.
    #[must_use]
    pub fn new(path: impl Into<String>, baud: u32) -> Self {
        Self {
            path: path.into(),
            baud,
        }
    }
}

/// Whether this user may open a port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortAccess {
    /// Readable and writable (or not checked: other systems than Linux).
    Ok,
    /// The device exists but this user may not read and write it.
    Denied,
}

/// A serial port the system knows, and whether it may be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortInfo {
    /// Port name or path (`/dev/ttyUSB0`, `COM3`).
    pub path: String,
    /// Whether this user may open it.
    pub access: PortAccess,
    /// Group that owns the device (Unix), when known.
    pub group: Option<String>,
}

/// The serial ports the system knows (USB adapters and so on), with their access.
/// The ports are never opened: opening one toggles DTR, which resets many boards.
#[must_use]
pub fn available_ports() -> Vec<PortInfo> {
    // serialport (built without libudev) scans /sys/class/tty and panics when it is
    // missing (a chroot or a container without /sys): no ports there
    #[cfg(target_os = "linux")]
    if !Path::new("/sys/class/tty").is_dir() {
        return Vec::new();
    }
    serialport::available_ports()
        .map(|v| {
            v.into_iter()
                .map(|p| {
                    let (access, group) = port_access(Path::new(&p.port_name));
                    PortInfo {
                        path: p.port_name,
                        access,
                        group,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether this user may read and write the device at `path`, and the group that
/// owns it, without opening it (`access(2)` and `stat(2)`). Only Linux reports
/// [`PortAccess::Denied`]: elsewhere the system answers when the port is opened.
#[must_use]
pub fn port_access(path: &Path) -> (PortAccess, Option<String>) {
    #[cfg(target_os = "linux")]
    {
        use nix::unistd::{AccessFlags, access};
        let access = match access(path, AccessFlags::R_OK | AccessFlags::W_OK) {
            Err(nix::errno::Errno::EACCES | nix::errno::Errno::EPERM) => PortAccess::Denied,
            _ => PortAccess::Ok,
        };
        (access, device_group(path))
    }
    #[cfg(not(target_os = "linux"))]
    {
        (PortAccess::Ok, device_group(path))
    }
}

/// The kernel's overflow gid: what a group outside the user namespace looks like
/// from inside one (a Flatpak sandbox). Its name (`nogroup`) is not the device's.
#[cfg(unix)]
const OVERFLOW_GID: u32 = 65534;

/// Name of the group that owns `path` (Unix; `None` when unknown: not in
/// `/etc/group`, or hidden by a user namespace as in a Flatpak sandbox).
#[cfg(unix)]
fn device_group(path: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let gid = std::fs::metadata(path).ok()?.gid();
    if gid == OVERFLOW_GID {
        return None;
    }
    nix::unistd::Group::from_gid(nix::unistd::Gid::from_raw(gid))
        .ok()
        .flatten()
        .map(|g| g.name)
}

#[cfg(not(unix))]
fn device_group(_: &Path) -> Option<String> {
    None
}

struct Reader(Box<dyn serialport::SerialPort>);
struct Writer(Box<dyn serialport::SerialPort>);

impl LinkReader for Reader {
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError> {
        if self.0.timeout() != timeout {
            self.0
                .set_timeout(timeout)
                .map_err(|e| TransportError::Io(e.to_string()))?;
        }
        match self.0.read(buf) {
            Ok(0) => Err(TransportError::Closed),
            Ok(n) => Ok(n),
            Err(e)
                if matches!(
                    e.kind(),
                    ErrorKind::TimedOut | ErrorKind::WouldBlock | ErrorKind::Interrupted
                ) =>
            {
                Ok(0)
            }
            Err(_) => Err(TransportError::Closed),
        }
    }
}

impl LinkWriter for Writer {
    fn write_all(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.0
            .write_all(data)
            .map_err(|e| TransportError::Io(e.to_string()))?;
        self.0.flush().map_err(|e| TransportError::Io(e.to_string()))
    }
}

impl Transport for SerialTransport {
    fn open(&self) -> Result<Link, TransportError> {
        let open_err = |e: serialport::Error| {
            if e.kind() == serialport::ErrorKind::Io(ErrorKind::PermissionDenied) {
                TransportError::PermissionDenied {
                    target: self.path.clone(),
                    group: device_group(Path::new(&self.path)),
                }
            } else {
                TransportError::Open {
                    target: self.path.clone(),
                    reason: e.to_string(),
                }
            }
        };
        let port = serialport::new(&self.path, self.baud)
            .timeout(Duration::from_millis(20))
            .open()
            .map_err(open_err)?;
        let writer = port.try_clone().map_err(open_err)?;
        Ok(Link {
            reader: Box::new(Reader(port)),
            writer: Box::new(Writer(writer)),
        })
    }

    fn describe(&self) -> String {
        format!("serial {} @{}", self.path, self.baud)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Temporary files stand for devices: no test opens a real serial port.
    #[cfg(target_os = "linux")]
    #[test]
    fn access_follows_the_file_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ttyFAKE0");
        std::fs::write(&path, b"").unwrap();

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let (access, group) = port_access(&path);
        assert_eq!(access, PortAccess::Ok);
        // the file's group is this user's: its name is in /etc/group on any sane system
        assert!(group.is_some());

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        // root reads and writes anything
        if !nix::unistd::geteuid().is_root() {
            assert_eq!(port_access(&path).0, PortAccess::Denied);
            match SerialTransport::new(path.to_string_lossy(), 115_200).open() {
                Err(TransportError::PermissionDenied { target, group: g }) => {
                    assert_eq!(target, path.to_string_lossy());
                    assert_eq!(g, group);
                }
                Err(e) => panic!("expected PermissionDenied, got {e}"),
                Ok(_) => panic!("opened a file with mode 000"),
            }
        }
    }

    #[test]
    fn a_missing_device_is_not_denied() {
        let (access, group) = port_access(Path::new("/nonexistent/freekerf-test-port"));
        assert_eq!(access, PortAccess::Ok);
        assert_eq!(group, None);
    }
}
