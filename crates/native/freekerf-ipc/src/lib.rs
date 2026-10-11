//! The local endpoint of a running FreeKerf (PLAN §9.1): one instance per user,
//! reached through a Unix domain socket (`$XDG_RUNTIME_DIR/freekerf/ipc/ipc.sock`,
//! directory `0700`, socket `0600` on Linux) or a Windows named pipe restricted to its owner,
//! with a per-run **token** in a user-only file next to it. `freekerf mcp`, the
//! Python client and tests use it.
//!
//! The wire format is JSON Lines (one JSON object per line, at most
//! [`MAX_LINE`] bytes). This crate knows nothing of the messages beyond the
//! request/response correlation of [`Client`]: requests are
//! `{"id": N, "method": "...", "params": ...}`, answers `{"id": N, "result": ...}`
//! or `{"id": N, "error": {"kind": "...", "message": "..."}}`, and anything without
//! an `id` is an event. The first line a client sends is its hello (with the
//! token); the server's first line answers it (`freekerf-service` defines both,
//! `doc/features/ipc.md`). A client leaving sends `{"bye": true}`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{ListenerOptions, Name, RecvHalf, SendHalf, Stream};
use serde_json::{Value, json};

/// Longest accepted line (a script, a document), bytes.
pub const MAX_LINE: usize = 64 << 20;

/// Errors of the endpoint.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// Another instance is listening on the endpoint.
    #[error("another FreeKerf instance is already running ({0})")]
    AlreadyRunning(String),
    /// Nothing listens on the endpoint (or it is stale).
    #[error("no running FreeKerf instance at {0}")]
    NotRunning(String),
    /// The token file is missing or unreadable.
    #[error("cannot read the instance token {path}: {message}")]
    Token {
        /// Token file.
        path: PathBuf,
        /// Cause.
        message: String,
    },
    /// The peer sent something that is not a JSON object line.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// The server refused the hello (wrong token, disabled...).
    #[error("{kind}: {message}")]
    Refused {
        /// Error kind (`unauthorized`, `disabled`...).
        kind: String,
        /// Message for people.
        message: String,
    },
    /// The connection closed.
    #[error("the connection to FreeKerf closed")]
    Closed,
    /// I/O error.
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// An error answered to a request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct RemoteError {
    /// Kind (`permission_denied`, `refused`, `invalid_params`...).
    pub kind: String,
    /// Message for people.
    pub message: String,
}

impl RemoteError {
    /// An error of `kind`.
    #[must_use]
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }

    /// As the `error` member of an answer.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({"kind": self.kind, "message": self.message})
    }

    fn from_json(v: &Value) -> Self {
        Self::new(
            v["kind"].as_str().unwrap_or("error"),
            v["message"].as_str().unwrap_or("unknown error"),
        )
    }
}

/// Where an instance listens and keeps its token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// Directory of the socket and the token (created `0700` on Unix).
    pub dir: PathBuf,
    /// Unix: the socket path; Windows: the pipe name.
    pub name: String,
}

impl Endpoint {
    /// The endpoint of the instance whose runtime directory is `runtime`
    /// (`freekerf_platform::Dirs::runtime`).
    #[must_use]
    pub fn in_dir(runtime: &Path) -> Self {
        let dir = runtime.join("ipc");
        let name = if cfg!(windows) {
            // one pipe per runtime directory: per user, and per test home
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for b in runtime.to_string_lossy().bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
            format!("freekerf-{h:016x}")
        } else {
            dir.join("ipc.sock").to_string_lossy().into_owned()
        };
        Self { dir, name }
    }

    /// The token file.
    #[must_use]
    pub fn token_file(&self) -> PathBuf {
        self.dir.join("ipc.token")
    }

    fn socket_name(&self) -> std::io::Result<Name<'_>> {
        #[cfg(windows)]
        {
            use interprocess::local_socket::GenericNamespaced;
            self.name.as_str().to_ns_name::<GenericNamespaced>()
        }
        #[cfg(not(windows))]
        {
            use interprocess::local_socket::GenericFilePath;
            self.name.as_str().to_fs_name::<GenericFilePath>()
        }
    }

    /// Reads the token of the running instance.
    ///
    /// # Errors
    /// [`IpcError::Token`].
    pub fn read_token(&self) -> Result<String, IpcError> {
        let path = self.token_file();
        match std::fs::read_to_string(&path) {
            Ok(t) if !t.trim().is_empty() => Ok(t.trim().to_owned()),
            Ok(_) => Err(IpcError::Token {
                path,
                message: "empty".into(),
            }),
            Err(e) => Err(IpcError::Token {
                path,
                message: e.to_string(),
            }),
        }
    }

    fn connect_stream(&self) -> Result<Stream, IpcError> {
        Stream::connect(self.socket_name()?).map_err(|_| IpcError::NotRunning(self.name.clone()))
    }
}

/// Compares secrets in time independent of where they differ.
#[must_use]
pub fn token_matches(given: &str, expected: &str) -> bool {
    let (a, b) = (given.as_bytes(), expected.as_bytes());
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn new_token() -> Result<String, IpcError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| IpcError::Io(std::io::Error::other(e.to_string())))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn private_dir(dir: &Path) -> Result<(), IpcError> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Writes a user-only file atomically (a reader never sees it half written).
fn write_private(path: &Path, contents: &str) -> Result<(), IpcError> {
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options.open(&tmp)?;
    f.write_all(contents.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// A listening instance. Dropping it removes the token file.
pub struct Server {
    listener: interprocess::local_socket::Listener,
    endpoint: Endpoint,
    token: String,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl Server {
    /// Listens on `endpoint` with a new token. A live instance there makes it fail
    /// ([`IpcError::AlreadyRunning`]); a stale socket (nothing accepts) is replaced.
    ///
    /// # Errors
    /// Another instance, or I/O.
    pub fn bind(endpoint: &Endpoint) -> Result<Self, IpcError> {
        private_dir(&endpoint.dir)?;
        if endpoint.connect_stream().is_ok() {
            return Err(IpcError::AlreadyRunning(endpoint.name.clone()));
        }
        #[cfg(unix)]
        {
            // nothing accepted: a socket left by a crashed instance
            let path = Path::new(&endpoint.name);
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        let options = ListenerOptions::new().name(endpoint.socket_name()?);
        // the socket's own mode where supported (Linux); elsewhere on Unix the
        // `0700` directory alone keeps other users out
        #[cfg(target_os = "linux")]
        let options = {
            use interprocess::os::unix::local_socket::ListenerOptionsExt;
            options.mode(0o600)
        };
        #[cfg(windows)]
        let options = {
            use interprocess::os::windows::local_socket::ListenerOptionsExt;
            use interprocess::os::windows::security_descriptor::SecurityDescriptor;
            // protected DACL: full access for the owner (the user who started the
            // instance) and the system; nobody else can open the pipe
            let sddl = widestring::U16CString::from_str("D:P(A;;GA;;;OW)(A;;GA;;;SY)")
                .map_err(|e| IpcError::Io(std::io::Error::other(e.to_string())))?;
            options.security_descriptor(SecurityDescriptor::deserialize(&sddl)?)
        };
        let listener = options.create_sync().map_err(|e| {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                IpcError::AlreadyRunning(endpoint.name.clone())
            } else {
                IpcError::Io(e)
            }
        })?;
        let token = new_token()?;
        write_private(&endpoint.token_file(), &token)?;
        tracing::info!(endpoint = %endpoint.name, "listening");
        Ok(Self {
            listener,
            endpoint: endpoint.clone(),
            token,
        })
    }

    /// The token clients must present.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The endpoint.
    #[must_use]
    pub const fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Waits for the next client.
    ///
    /// # Errors
    /// I/O.
    pub fn accept(&self) -> Result<Connection, IpcError> {
        Ok(Connection::new(self.listener.accept()?))
    }
}

impl Server {
    /// Removes the token file (and the socket file on Unix) so that clients see
    /// no instance; used before the process exits while `accept` is blocked.
    pub fn close(&self) {
        // only our own files: a newer instance may have replaced them
        if self.endpoint.read_token().is_ok_and(|t| t == self.token) {
            let _ = std::fs::remove_file(self.endpoint.token_file());
            #[cfg(unix)]
            {
                let _ = std::fs::remove_file(&self.endpoint.name);
            }
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.close();
    }
}

/// Reads JSON lines.
pub struct LineReader {
    inner: BufReader<RecvHalf>,
    buf: Vec<u8>,
}

impl std::fmt::Debug for LineReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LineReader").finish_non_exhaustive()
    }
}

impl LineReader {
    /// The next object; `None` at the end of the stream. Blank lines are skipped.
    ///
    /// # Errors
    /// A line that is not a JSON object or is longer than [`MAX_LINE`], or I/O.
    pub fn read(&mut self) -> Result<Option<Value>, IpcError> {
        loop {
            self.buf.clear();
            let n = (&mut self.inner)
                .take(MAX_LINE as u64 + 1)
                .read_until(b'\n', &mut self.buf)?;
            if n == 0 {
                return Ok(None);
            }
            if self.buf.len() > MAX_LINE {
                return Err(IpcError::Protocol(format!("line longer than {MAX_LINE} bytes")));
            }
            let line = String::from_utf8_lossy(&self.buf);
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let v: Value = serde_json::from_str(line).map_err(|e| IpcError::Protocol(e.to_string()))?;
            if !v.is_object() {
                return Err(IpcError::Protocol("expected a JSON object".into()));
            }
            return Ok(Some(v));
        }
    }
}

/// Writes JSON lines; clones share the connection (answers and events from
/// different threads never interleave inside a line).
#[derive(Clone)]
pub struct LineWriter {
    inner: Arc<Mutex<SendHalf>>,
}

impl std::fmt::Debug for LineWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LineWriter").finish_non_exhaustive()
    }
}

impl LineWriter {
    /// Sends one object.
    ///
    /// # Errors
    /// I/O (the peer is gone).
    pub fn send(&self, v: &Value) -> Result<(), IpcError> {
        let mut line = serde_json::to_vec(v).map_err(|e| IpcError::Protocol(e.to_string()))?;
        line.push(b'\n');
        let mut w = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        w.write_all(&line)?;
        w.flush()?;
        Ok(())
    }
}

/// One connection, either side.
#[derive(Debug)]
pub struct Connection {
    /// Incoming lines.
    pub reader: LineReader,
    /// Outgoing lines.
    pub writer: LineWriter,
}

impl Connection {
    fn new(stream: Stream) -> Self {
        let (recv, send) = stream.split();
        Self {
            reader: LineReader {
                inner: BufReader::new(recv),
                buf: Vec::new(),
            },
            writer: LineWriter {
                inner: Arc::new(Mutex::new(send)),
            },
        }
    }
}

type Pending = Arc<Mutex<HashMap<u64, Sender<Result<Value, RemoteError>>>>>;

/// A client of a running instance: requests with answers, and events.
pub struct Client {
    writer: LineWriter,
    pending: Pending,
    next: AtomicU64,
    events: Receiver<Value>,
    welcome: Value,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("welcome", &self.welcome)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Connects and says hello: `hello` is sent as `{"hello": hello}` with the
    /// token added; the server's answer is `{"welcome": ...}` or `{"error": ...}`.
    ///
    /// # Errors
    /// Nothing running, no token, or the server refused the hello.
    pub fn connect(endpoint: &Endpoint, hello: Value) -> Result<Self, IpcError> {
        let stream = endpoint.connect_stream()?;
        let token = endpoint.read_token()?;
        let Connection { mut reader, writer } = Connection::new(stream);
        let mut hello = hello;
        if let Some(obj) = hello.as_object_mut() {
            obj.insert("token".into(), Value::String(token));
        }
        writer.send(&json!({ "hello": hello }))?;
        let answer = reader.read()?.ok_or(IpcError::Closed)?;
        if let Some(err) = answer.get("error") {
            let e = RemoteError::from_json(err);
            return Err(IpcError::Refused {
                kind: e.kind,
                message: e.message,
            });
        }
        let welcome = answer
            .get("welcome")
            .cloned()
            .ok_or_else(|| IpcError::Protocol("expected a welcome".into()))?;
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (tx, events) = unbounded();
        let routes = Arc::clone(&pending);
        thread::Builder::new()
            .name("freekerf-ipc-client".into())
            .spawn(move || route(reader, &routes, &tx))?;
        Ok(Self {
            writer,
            pending,
            next: AtomicU64::new(1),
            events,
            welcome,
        })
    }

    /// Connects, retrying until `timeout` (an instance that is starting).
    ///
    /// # Errors
    /// The last error when the time is up; a refused hello at once.
    pub fn connect_retry(endpoint: &Endpoint, hello: &Value, timeout: Duration) -> Result<Self, IpcError> {
        let deadline = Instant::now() + timeout;
        loop {
            match Self::connect(endpoint, hello.clone()) {
                Ok(c) => return Ok(c),
                Err(e @ IpcError::Refused { .. }) => return Err(e),
                Err(e) if Instant::now() >= deadline => return Err(e),
                Err(_) => thread::sleep(Duration::from_millis(50)),
            }
        }
    }

    /// The server's welcome.
    #[must_use]
    pub const fn welcome(&self) -> &Value {
        &self.welcome
    }

    /// Sends a request and waits for its answer.
    ///
    /// # Errors
    /// The server's error, or `closed` when the connection is gone.
    #[allow(clippy::needless_pass_by_value)] // callers build the value for the call
    pub fn call(&self, method: &str, params: Value) -> Result<Value, RemoteError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = bounded(1);
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, tx);
        let closed = || RemoteError::new("closed", IpcError::Closed.to_string());
        if self
            .writer
            .send(&json!({"id": id, "method": method, "params": params}))
            .is_err()
        {
            self.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
            return Err(closed());
        }
        rx.recv().unwrap_or_else(|_| Err(closed()))
    }

    /// Events sent by the server (after a subscription).
    #[must_use]
    pub const fn events(&self) -> &Receiver<Value> {
        &self.events
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // the reader thread keeps its half of the socket open: say goodbye so the
        // server closes the connection
        let _ = self.writer.send(&json!({"bye": true}));
    }
}

fn route(mut reader: LineReader, pending: &Pending, events: &Sender<Value>) {
    while let Ok(Some(msg)) = reader.read() {
        match msg.get("id").and_then(Value::as_u64) {
            Some(id) => {
                let waiter = pending.lock().unwrap_or_else(PoisonError::into_inner).remove(&id);
                if let Some(w) = waiter {
                    let answer = match msg.get("error") {
                        Some(e) => Err(RemoteError::from_json(e)),
                        None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                    };
                    let _ = w.send(answer);
                }
            }
            None => {
                let _ = events.send(msg);
            }
        }
    }
    // the connection is gone: every waiting call fails
    pending.lock().unwrap_or_else(PoisonError::into_inner).clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        assert!(token_matches("abc", "abc"));
        assert!(!token_matches("abd", "abc"));
        assert!(!token_matches("ab", "abc"));
        assert!(!token_matches("", ""));
        let t = new_token().unwrap();
        assert_eq!(t.len(), 64);
        assert_ne!(t, new_token().unwrap());
    }

    #[test]
    fn remote_errors() {
        let e = RemoteError::new("refused", "no");
        assert_eq!(RemoteError::from_json(&e.to_json()), e);
        assert_eq!(RemoteError::from_json(&json!({})).kind, "error");
        assert_eq!(e.to_string(), "no");
    }
}
