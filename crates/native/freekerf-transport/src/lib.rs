//! Transports to the machine. A [`Transport`] opens a [`Link`]: a reader and a
//! writer that may be used from two threads.
//!
//! * [`SerialTransport`]: a serial port (USB-serial boards; a PTY in tests);
//! * [`EmulatorTransport`]: the built-in Grbl emulator, in process;
//! * [`pty::serve_emulator`]: the emulator behind a pseudo terminal, so any
//!   program (LaserGRBL included) can open it like a real port (Unix only).
//!
//! Automated tests never open real serial devices: they use PTYs or the emulator.

use std::time::Duration;

pub mod emulator;
#[cfg(unix)]
pub mod pty;
pub mod serial;

pub use emulator::EmulatorTransport;
pub use serial::SerialTransport;

/// Errors of a transport.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// The link could not be opened.
    #[error("cannot open {target}: {reason}")]
    Open {
        /// What was opened.
        target: String,
        /// Why it failed.
        reason: String,
    },
    /// The other side hung up or the device disappeared.
    #[error("connection closed")]
    Closed,
    /// I/O error while reading or writing.
    #[error("i/o error: {0}")]
    Io(String),
}

/// Reading half of a link.
pub trait LinkReader: Send {
    /// Reads available bytes, waiting at most `timeout`. `Ok(0)` means nothing
    /// arrived in time; [`TransportError::Closed`] means the link is gone.
    ///
    /// # Errors
    /// When the link is closed or failed.
    fn read(&mut self, buf: &mut [u8], timeout: Duration) -> Result<usize, TransportError>;
}

/// Writing half of a link.
pub trait LinkWriter: Send {
    /// Writes all bytes.
    ///
    /// # Errors
    /// When the link is closed or failed.
    fn write_all(&mut self, data: &[u8]) -> Result<(), TransportError>;
}

/// An open connection.
pub struct Link {
    /// Bytes from the machine.
    pub reader: Box<dyn LinkReader>,
    /// Bytes to the machine.
    pub writer: Box<dyn LinkWriter>,
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Link")
    }
}

/// A way to reach a machine.
pub trait Transport: Send {
    /// Opens the link.
    ///
    /// # Errors
    /// When the port or device cannot be opened.
    fn open(&self) -> Result<Link, TransportError>;

    /// Human description (`serial /dev/ttyUSB0 @115200`).
    fn describe(&self) -> String;
}
