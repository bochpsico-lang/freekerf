//! Serial ports through the `serialport` crate.

use std::io::{ErrorKind, Read, Write};
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

/// Names of the serial ports the system knows (USB adapters and so on).
#[must_use]
pub fn available_ports() -> Vec<String> {
    serialport::available_ports()
        .map(|v| v.into_iter().map(|p| p.port_name).collect())
        .unwrap_or_default()
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
        let open_err = |e: serialport::Error| TransportError::Open {
            target: self.path.clone(),
            reason: e.to_string(),
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
