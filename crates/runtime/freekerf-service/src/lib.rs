//! The application facade: the machine driver, the settings and the loaded
//! program behind **one action registry** (PLAN §9.1). Every front end (the CLI,
//! `host --stdio`, later the desktop UI, scripts and MCP) calls
//! [`Service::dispatch`] with an action name and JSON parameters; nothing else
//! changes the state.
//!
//! The catalogue of actions is generated into `doc/features/actions.md`.

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use freekerf_gcode::{AnalysisOptions, Program};
use freekerf_machine::{Machine, MachineEvent, SystemClock};
use freekerf_protocol::host::decimal;
use freekerf_protocol::{
    ConsoleRow, Event, FirmwareKind, Host, HostError, HostSnapshot, Issue, JogDirection, ResumeSuggestion,
};
use freekerf_session::{NoParams, Registry};
use freekerf_settings::{MachineCache, Settings, SettingsStore};
use freekerf_transport::{EmulatorTransport, SerialTransport, Transport};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod analysis;
pub mod autotrace;
pub mod import_case;
pub mod migrate;

pub use freekerf_session::{ActionError, ActionSpec, Caller, CallerKind, Tier};

/// Rows kept in the console history.
pub const CONSOLE_ROWS: usize = 5000;
const OP_TIMEOUT: Duration = Duration::from_secs(30);

/// Events for front ends.
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceEvent {
    /// From the machine driver.
    Machine(MachineEvent),
    /// Settings changed.
    SettingsChanged,
    /// A program was loaded.
    ProgramLoaded,
}

impl ServiceEvent {
    /// The event as JSON for front ends (`{"event": "...", ...}`).
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::SettingsChanged => serde_json::json!({"event": "settings_changed"}),
            Self::ProgramLoaded => serde_json::json!({"event": "program_loaded"}),
            Self::Machine(MachineEvent::Connected { transport }) => {
                serde_json::json!({"event": "connected", "transport": transport})
            }
            Self::Machine(MachineEvent::Disconnected { user }) => {
                serde_json::json!({"event": "disconnected", "user": user})
            }
            Self::Machine(MachineEvent::Host(e)) => {
                serde_json::json!({"event": "host", "data": serde_json::to_value(e).unwrap_or(Value::Null)})
            }
        }
    }

    /// One line of text for a terminal console, when the event has one.
    #[must_use]
    pub fn console_line(&self) -> Option<String> {
        use freekerf_protocol::CommandStatus;
        match self {
            Self::Machine(MachineEvent::Host(Event::Console(row))) => Some(match row {
                ConsoleRow::Command { text, .. } => format!("> {text}"),
                ConsoleRow::Result {
                    result,
                    status,
                    decoded,
                    ..
                } => match (status, decoded) {
                    (CommandStatus::ResponseGood, _) => return None,
                    (_, Some(d)) => format!("  {result} ({d})"),
                    _ => format!("  {result}"),
                },
                ConsoleRow::Message { text, decoded, .. } => decoded.clone().unwrap_or_else(|| text.clone()),
            }),
            Self::Machine(MachineEvent::Host(Event::Issue(i))) => Some(format!("! issue: {i}")),
            Self::Machine(MachineEvent::Host(Event::StatusChanged { new, .. })) => Some(format!("# {new}")),
            Self::Machine(MachineEvent::Host(Event::JobEnded {
                lines,
                errors,
                time_ms,
            })) => Some(format!(
                "# job ended: {lines} lines, {errors} errors, {:.1} s",
                *time_ms as f64 / 1000.0
            )),
            Self::Machine(MachineEvent::Connected { transport }) => Some(format!("# connected: {transport}")),
            Self::Machine(MachineEvent::Disconnected { .. }) => Some("# disconnected".into()),
            _ => None,
        }
    }
}

/// The emulator served on a pseudo terminal (`freekerf emulate --pty`).
#[cfg(unix)]
pub mod emulator {
    pub use freekerf_transport::pty::PtyEmulator;

    /// Starts the emulator on a PTY; `timed`: motions take their real time.
    ///
    /// # Errors
    /// No PTY available.
    pub fn serve_pty(timed: bool) -> Result<PtyEmulator, String> {
        let cfg = freekerf_emulator::DeviceConfig {
            timed,
            ..freekerf_emulator::DeviceConfig::default()
        };
        freekerf_transport::pty::serve_emulator(cfg).map_err(|e| e.to_string())
    }
}

/// Where the service keeps its files (none: memory only).
#[derive(Default)]
pub struct Stores {
    /// `settings.toml`.
    pub settings: Option<Arc<dyn SettingsStore>>,
    /// Machine cache (last version, `$$`, laser counters).
    pub cache: Option<Arc<dyn SettingsStore>>,
}

impl std::fmt::Debug for Stores {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stores")
            .field("settings", &self.settings.is_some())
            .field("cache", &self.cache.is_some())
            .finish()
    }
}

impl Stores {
    /// Files in `dir`, or in the per-user directories when `None` (`$FREEKERF_HOME`
    /// overrides them).
    ///
    /// # Errors
    /// No home directory.
    pub fn files(dir: Option<&Path>) -> Result<Self, String> {
        let dirs = match dir {
            Some(d) => freekerf_platform::Dirs::under(d),
            None => freekerf_platform::Dirs::user().map_err(|e| e.to_string())?,
        };
        Ok(Self {
            settings: Some(Arc::new(FileStore(dirs.settings_file()))),
            cache: Some(Arc::new(FileStore(dirs.data_file("machine-cache.json")))),
        })
    }

    /// Path of the settings file these stores use, if any.
    #[must_use]
    pub fn settings_path(dir: Option<&Path>) -> Option<std::path::PathBuf> {
        match dir {
            Some(d) => Some(freekerf_platform::Dirs::under(d).settings_file()),
            None => freekerf_platform::Dirs::user().ok().map(|d| d.settings_file()),
        }
    }
}

/// A file store (settings or cache) in the platform directories.
#[derive(Debug, Clone)]
pub struct FileStore(pub std::path::PathBuf);

impl SettingsStore for FileStore {
    fn load(&self) -> Result<Option<String>, freekerf_settings::SettingsError> {
        freekerf_platform::read_optional(&self.0)
            .map(|o| o.map(|b| String::from_utf8_lossy(&b).into_owned()))
            .map_err(|e| freekerf_settings::SettingsError::Storage(e.to_string()))
    }

    fn save(&self, text: &str) -> Result<(), freekerf_settings::SettingsError> {
        freekerf_platform::write_atomic(&self.0, text.as_bytes())
            .map_err(|e| freekerf_settings::SettingsError::Storage(e.to_string()))
    }
}

/// The loaded program.
#[derive(Debug, Clone)]
struct Loaded {
    name: String,
    program: Program,
}

/// State shared by the actions.
pub struct Inner {
    machine: Machine,
    settings: Mutex<Settings>,
    stores: Stores,
    loaded: Mutex<Option<Loaded>>,
    console: Arc<Mutex<VecDeque<ConsoleRow>>>,
    issues: Arc<Mutex<Vec<Issue>>>,
    subscribers: Arc<Mutex<Vec<Sender<ServiceEvent>>>>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("machine", &self.machine)
            .finish_non_exhaustive()
    }
}

/// The service: state plus the action registry.
#[derive(Clone)]
pub struct Service {
    inner: Arc<Inner>,
    registry: Arc<Registry<Inner>>,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field("registry", &self.registry)
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Error of service setup.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    /// Settings could not be loaded.
    #[error(transparent)]
    Settings(#[from] freekerf_settings::SettingsError),
}

impl Service {
    /// A service with settings loaded from `stores` (defaults when absent).
    ///
    /// # Errors
    /// The settings file is invalid or too new.
    pub fn new(stores: Stores) -> Result<Self, ServiceError> {
        let settings = match &stores.settings {
            Some(s) => Settings::load(s.as_ref())?,
            None => Settings::default(),
        };
        let cache = stores
            .cache
            .as_ref()
            .and_then(|c| c.load().ok().flatten())
            .map(|t| MachineCache::from_json(&t))
            .unwrap_or_default();
        let mut options = settings.host_options();
        options.last_known_version = cache.version.clone();
        options.last_known_settings = cache.settings.clone();
        let mut host = Host::for_kind(settings.machine.firmware.into(), options);
        if !cache.laser_life.is_empty() {
            host.set_laser_life(freekerf_protocol::LaserLife::with_counters(cache.laser_life));
        }
        let machine = Machine::start(host, Arc::new(SystemClock::default()));
        let inner = Arc::new(Inner {
            machine,
            settings: Mutex::new(settings),
            stores,
            loaded: Mutex::new(None),
            console: Arc::new(Mutex::new(VecDeque::new())),
            issues: Arc::new(Mutex::new(Vec::new())),
            subscribers: Arc::new(Mutex::new(Vec::new())),
        });
        start_collector(&inner);
        Ok(Self {
            inner,
            registry: Arc::new(build_registry()),
        })
    }

    /// A service in memory (no files).
    ///
    /// # Errors
    /// Never in practice.
    pub fn in_memory() -> Result<Self, ServiceError> {
        Self::new(Stores::default())
    }

    /// Runs an action.
    ///
    /// # Errors
    /// See [`ActionError`].
    pub fn dispatch(&self, caller: &Caller, action: &str, params: Value) -> Result<Value, ActionError> {
        tracing::debug!(caller = %caller.name, action, "dispatch");
        self.registry.dispatch(&self.inner, caller, action, params)
    }

    /// The registry (specs, docs).
    #[must_use]
    pub fn registry(&self) -> &Registry<Inner> {
        &self.registry
    }

    /// Every action spec.
    #[must_use]
    pub fn specs(&self) -> Vec<ActionSpec> {
        self.registry.specs().cloned().collect()
    }

    /// Events from now on.
    #[must_use]
    pub fn subscribe(&self) -> Receiver<ServiceEvent> {
        let (tx, rx) = unbounded();
        lock(&self.inner.subscribers).push(tx);
        rx
    }

    /// Machine state.
    #[must_use]
    pub fn snapshot(&self) -> HostSnapshot {
        self.inner.machine.snapshot()
    }

    /// STOP without going through the registry (also bound to Ctrl-C by the CLI).
    pub fn stop(&self) {
        self.inner.machine.stop();
    }
}

/// Collects console rows and issues and forwards machine events.
fn start_collector(inner: &Arc<Inner>) {
    let events = inner.machine.subscribe();
    let console = Arc::clone(&inner.console);
    let issues = Arc::clone(&inner.issues);
    let subscribers = Arc::clone(&inner.subscribers);
    let weak = Arc::downgrade(inner);
    let spawned = thread::Builder::new()
        .name("freekerf-service-events".into())
        .spawn(move || {
            for e in events {
                match &e {
                    MachineEvent::Host(Event::Console(row)) => {
                        let mut c = lock(&console);
                        c.push_back(row.clone());
                        while c.len() > CONSOLE_ROWS {
                            c.pop_front();
                        }
                    }
                    MachineEvent::Host(Event::ConsoleCleared) => lock(&console).clear(),
                    MachineEvent::Host(Event::Issue(i)) => lock(&issues).push(*i),
                    MachineEvent::Host(
                        Event::VersionDetected(_) | Event::SettingsChanged | Event::JobEnded { .. },
                    ) => {
                        if let Some(inner) = weak.upgrade() {
                            inner.save_cache();
                        }
                    }
                    _ => {}
                }
                lock(&subscribers).retain(|s| s.send(ServiceEvent::Machine(e.clone())).is_ok());
            }
        });
    if let Err(e) = spawned {
        tracing::error!("cannot start the event collector: {e}");
    }
}

impl Inner {
    fn publish(&self, e: &ServiceEvent) {
        lock(&self.subscribers).retain(|s| s.send(e.clone()).is_ok());
    }

    fn save_cache(&self) {
        let Some(store) = &self.stores.cache else { return };
        let Ok((version, table, life)) = self.machine.call(|h, _| {
            (
                h.version().cloned(),
                h.settings().clone(),
                h.laser_life().counters.clone(),
            )
        }) else {
            return;
        };
        let cache = MachineCache {
            version,
            settings: (!table.is_empty()).then_some(table),
            laser_life: life,
            extra: BTreeMap::new(),
        };
        if let Err(e) = store.save(&cache.to_json()) {
            tracing::warn!("cannot save the machine cache: {e}");
        }
    }

    fn save_settings(&self) -> Result<(), ActionError> {
        if let Some(store) = &self.stores.settings {
            lock(&self.settings)
                .save(store.as_ref())
                .map_err(|e| ActionError::Failed(e.to_string()))?;
        }
        Ok(())
    }

    fn host<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Host, u64) -> R + Send + 'static,
    ) -> Result<R, ActionError> {
        self.machine
            .call(f)
            .map_err(|e| ActionError::Failed(e.to_string()))
    }

    fn host_result(
        &self,
        f: impl FnOnce(&mut Host, u64) -> Result<(), HostError> + Send + 'static,
    ) -> Result<(), ActionError> {
        self.host(f)?.map_err(host_err)
    }

    /// Waits for the end of a system operation started by `f`.
    fn system_op(
        &self,
        f: impl FnOnce(&mut Host, u64) -> Result<u64, HostError> + Send + 'static,
    ) -> Result<(), ActionError> {
        let events = self.machine.subscribe();
        let id = self.host(f)?.map_err(host_err)?;
        let end = Instant::now() + OP_TIMEOUT;
        while let Some(left) = end.checked_duration_since(Instant::now()) {
            match events.recv_timeout(left) {
                Ok(MachineEvent::Host(Event::SystemOpFinished { id: i, result, .. })) if i == id => {
                    return result.map_err(ActionError::Failed);
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
        Err(ActionError::Failed("no answer from the machine".into()))
    }

    fn analysis_options(&self) -> AnalysisOptions {
        let support_pwm = lock(&self.settings).machine.support_pwm;
        let timing = self
            .machine
            .call(|h, _| h.settings().timing())
            .unwrap_or_default();
        AnalysisOptions { timing, support_pwm }
    }
}

#[allow(clippy::needless_pass_by_value)] // used as `map_err(host_err)`
fn host_err(e: HostError) -> ActionError {
    match e {
        HostError::NotConnected | HostError::NotAllowed(_) | HostError::NoProgram | HostError::Busy => {
            ActionError::Refused(e.to_string())
        }
        HostError::Unsupported(_) => ActionError::Failed(e.to_string()),
    }
}

// ====================================================================== parameters

/// `machine.connect`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectParams {
    /// Serial port (`/dev/ttyUSB0`, `COM3`); default: the last one used.
    pub port: Option<String>,
    /// Baud rate; default: from the settings (115200).
    pub baud: Option<u32>,
    /// Connect to the built-in emulator instead of a serial port.
    #[serde(default)]
    pub emulator: bool,
}

/// `machine.jog`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JogParams {
    /// `N`, `S`, `E`, `W`, `NE`, `NW`, `SE`, `SW`, `Zup`, `Zdown` or `Home`.
    pub direction: String,
    /// Step in mm (default: the jog setting).
    pub step_mm: Option<f64>,
    /// Feed in mm/min (default: the jog setting).
    pub feed_mm_min: Option<f64>,
}

/// `machine.jog_to`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JogToParams {
    /// Target X (work coordinates, mm).
    pub x: f64,
    /// Target Y (work coordinates, mm).
    pub y: f64,
    /// Feed in mm/min (default: the jog setting).
    pub feed_mm_min: Option<f64>,
}

/// `machine.overrides`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OverrideParams {
    /// Feed override target, % (10-200).
    pub feed: i32,
    /// Rapid override target, % (25, 50 or 100).
    pub rapid: i32,
    /// Power override target, % (10-200).
    pub power: i32,
}

/// `machine.write_settings`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WriteSettingsParams {
    /// Values by setting number (`{"110": "6000"}`).
    pub settings: BTreeMap<u32, String>,
}

/// `machine.send`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SendParams {
    /// One line of G-code or a `$` command.
    pub line: String,
}

/// `machine.custom_code`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomCodeParams {
    /// Lines of custom code (`[expr]`, `!`, `~`, `?`, `ctrl-x`, `0xNN` allowed).
    pub code: String,
}

/// `machine.select_laser`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectLaserParams {
    /// Id of the laser life counter.
    pub id: String,
}

/// `job.load`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadParams {
    /// Path of a G-code file.
    pub path: String,
}

/// `job.load_text`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadTextParams {
    /// Name shown for the program.
    pub name: String,
    /// The G-code.
    pub gcode: String,
}

/// `job.run`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunParams {
    /// Home (`$H`) before the job.
    #[serde(default)]
    pub homing: bool,
    /// Number of passes (default 1).
    pub passes: Option<u32>,
}

/// `job.resume`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResumeParams {
    /// 0-based index of the first command to run.
    pub position: usize,
    /// Home first.
    #[serde(default)]
    pub homing: bool,
    /// Restore the work offset of the interrupted job.
    #[serde(default)]
    pub set_wco: bool,
}

/// `settings.get`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsGetParams {
    /// Dotted key (`machine.baud`); all settings when absent.
    pub key: Option<String>,
}

/// `settings.set`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsSetParams {
    /// Dotted key (`machine.streaming_mode`). `automation.*` is user-only.
    pub key: String,
    /// New value (JSON).
    pub value: Value,
}

/// `console.get`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConsoleParams {
    /// Only the last `last` rows.
    pub last: Option<usize>,
}

// ====================================================================== results

/// Summary of the loaded program.
#[derive(Debug, Clone, Serialize)]
pub struct ProgramInfo {
    /// Name.
    pub name: String,
    /// Commands.
    pub lines: usize,
    /// Estimated time, ms.
    pub estimated_ms: i64,
    /// `[xmin, ymin, xmax, ymax]` where the laser burns.
    pub drawing_range: Option<[f64; 4]>,
    /// Quadrant of the drawing.
    pub quadrant: String,
}

/// Result of `state.get`.
#[derive(Debug, Clone, Serialize)]
pub struct State {
    /// Machine.
    pub machine: HostSnapshot,
    /// Firmware version as text (`1.1f`).
    pub firmware_version: Option<String>,
    /// Problems reported during jobs since the app started.
    pub issues: Vec<Issue>,
    /// Loaded program.
    pub program: Option<ProgramInfo>,
    /// Resume offer after an interrupted job.
    pub resume: Option<ResumeSuggestion>,
}

fn program_info(l: &Loaded) -> ProgramInfo {
    let r = freekerf_gcode::report::Report::new(&l.name, &l.program);
    ProgramInfo {
        name: l.name.clone(),
        lines: l.program.len(),
        estimated_ms: l.program.estimated_ms(),
        drawing_range: r.drawing_range,
        quadrant: r.quadrant,
    }
}

// ====================================================================== registry

#[allow(clippy::too_many_lines)] // one registration per action
fn build_registry() -> Registry<Inner> {
    let mut r: Registry<Inner> = Registry::new();
    let add = |result: Result<(), freekerf_session::RegistryError>| {
        if let Err(e) = result {
            tracing::error!("action registration: {e}");
        }
    };

    // ---- state
    add(r.register(
        "state.get",
        "Machine state, loaded program, issues and resume offer.",
        Tier::Read,
        |s: &Inner, _: NoParams| {
            let machine = s.machine.snapshot();
            let resume = s.host(|h, _| h.resume_suggestion())?;
            Ok(State {
                firmware_version: machine.version.as_ref().map(ToString::to_string),
                machine,
                issues: lock(&s.issues).clone(),
                program: lock(&s.loaded).as_ref().map(program_info),
                resume,
            })
        },
    ));
    add(r.register(
        "console.get",
        "Console rows (sent commands, answers, messages).",
        Tier::Read,
        |s: &Inner, p: ConsoleParams| {
            let c = lock(&s.console);
            let skip = p.last.map_or(0, |n| c.len().saturating_sub(n));
            Ok(c.iter().skip(skip).cloned().collect::<Vec<_>>())
        },
    ));

    // ---- settings
    add(r.register(
        "settings.get",
        "Reads the settings (all, or one dotted key).",
        Tier::Read,
        |s: &Inner, p: SettingsGetParams| {
            let st = lock(&s.settings);
            match p.key {
                Some(k) => st.get(&k).map_err(|e| ActionError::InvalidParams(e.to_string())),
                None => serde_json::to_value(&*st).map_err(|e| ActionError::Failed(e.to_string())),
            }
        },
    ));
    add(r.register(
        "settings.set",
        "Changes one setting and saves the file. Automation settings are user-only and refused here.",
        Tier::Edit,
        |s: &Inner, p: SettingsSetParams| {
            let options = {
                let mut st = lock(&s.settings);
                st.set(&p.key, p.value, false)
                    .map_err(|e| ActionError::Refused(e.to_string()))?;
                st.host_options()
            };
            s.save_settings()?;
            s.host(move |h, _| {
                let mut o = options;
                o.last_known_version = h.version().cloned();
                h.set_options(o);
            })?;
            s.publish(&ServiceEvent::SettingsChanged);
            Ok(())
        },
    ));

    // ---- machine: connection
    add(r.register(
        "machine.list_ports",
        "Serial ports found on this computer.",
        Tier::Read,
        |_: &Inner, _: NoParams| Ok(freekerf_transport::serial::available_ports()),
    ));
    add(r.register(
        "machine.connect",
        "Connects to the machine on a serial port, or to the built-in emulator.",
        Tier::Machine,
        |s: &Inner, p: ConnectParams| {
            let (kind, options, transport) = {
                let mut st = lock(&s.settings);
                let transport: Box<dyn Transport> = if p.emulator {
                    Box::new(EmulatorTransport::default())
                } else {
                    let port = p.port.or_else(|| st.machine.port.clone()).ok_or_else(|| {
                        ActionError::InvalidParams("no port given and no port in the settings".into())
                    })?;
                    st.machine.port = Some(port.clone());
                    Box::new(SerialTransport::new(port, p.baud.unwrap_or(st.machine.baud)))
                };
                (
                    FirmwareKind::from(st.machine.firmware),
                    st.host_options(),
                    transport,
                )
            };
            let _ = s.save_settings();
            // a different firmware needs a new host (only while disconnected)
            s.host(move |h, _| {
                if h.firmware() != kind && !h.snapshot().connected {
                    let mut o = options;
                    o.last_known_version = h.version().cloned();
                    o.last_known_settings = Some(h.settings().clone());
                    let life = h.laser_life().clone();
                    *h = Host::for_kind(kind, o);
                    h.set_laser_life(life);
                }
            })?;
            s.machine
                .connect(transport)
                .map_err(|e| ActionError::Failed(e.to_string()))
        },
    ));
    add(r.register(
        "machine.disconnect",
        "Closes the connection.",
        Tier::Machine,
        |s: &Inner, _: NoParams| {
            s.machine
                .disconnect()
                .map_err(|e| ActionError::Failed(e.to_string()))
        },
    ));

    // ---- machine: safety
    add(r.register(
        "machine.stop",
        "STOP: soft reset of the board (laser off, queues flushed). Always allowed.",
        Tier::Safety,
        |s: &Inner, _: NoParams| {
            s.machine.stop();
            Ok(())
        },
    ));
    add(r.register(
        "machine.feed_hold",
        "Feed hold (pause the motion). Always allowed.",
        Tier::Safety,
        |s: &Inner, _: NoParams| s.host_result(Host::feed_hold),
    ));
    add(r.register(
        "machine.jog_stop",
        "Stops a continuous jog. Always allowed.",
        Tier::Safety,
        |s: &Inner, _: NoParams| s.host(Host::jog_abort),
    ));
    add(r.register(
        "machine.safety_door",
        "Safety door (real-time 0x84): the board holds as if its door opened; resume with machine.resume. Always allowed.",
        Tier::Safety,
        |s: &Inner, _: NoParams| s.host(Host::safety_door),
    ));

    // ---- machine: motion and settings
    add(r.register(
        "machine.resume",
        "Resumes after a feed hold or a door (cycle start).",
        Tier::Machine,
        |s: &Inner, _: NoParams| s.host_result(Host::cycle_start),
    ));
    add(r.register(
        "machine.unlock",
        "Unlocks the alarm state (`$X`).",
        Tier::Machine,
        |s: &Inner, _: NoParams| s.host_result(Host::unlock),
    ));
    add(r.register(
        "machine.home",
        "Runs the homing cycle (`$H`).",
        Tier::Machine,
        |s: &Inner, _: NoParams| s.host_result(Host::home),
    ));
    add(r.register(
        "machine.set_zero",
        "Sets the current position as work zero (`G92 X0 Y0 Z0`).",
        Tier::Machine,
        |s: &Inner, _: NoParams| s.host_result(Host::set_new_zero),
    ));
    add(r.register(
        "machine.jog",
        "Jogs one step in a direction (laser off).",
        Tier::Machine,
        |s: &Inner, p: JogParams| {
            let dir = JogDirection::parse(&p.direction)
                .ok_or_else(|| ActionError::InvalidParams(format!("unknown direction `{}`", p.direction)))?;
            let (step, feed) = {
                let st = lock(&s.settings);
                (
                    p.step_mm.unwrap_or(st.jog.step_mm),
                    p.feed_mm_min.unwrap_or(st.jog.speed_mm_min),
                )
            };
            #[allow(clippy::cast_possible_truncation)]
            let feed = feed as f32;
            s.host_result(move |h, now| h.jog(dir, feed, decimal(step), now))
        },
    ));
    add(r.register(
        "machine.jog_to",
        "Jogs to a work position (laser off).",
        Tier::Machine,
        |s: &Inner, p: JogToParams| {
            let feed = p
                .feed_mm_min
                .unwrap_or_else(|| lock(&s.settings).jog.speed_mm_min);
            #[allow(clippy::cast_possible_truncation)]
            let (x, y, feed) = (p.x as f32, p.y as f32, feed as f32);
            s.host_result(move |h, now| h.jog_to(x, y, feed, now))
        },
    ));
    add(r.register(
        "machine.overrides",
        "Sets the override targets; the host steps the board towards them.",
        Tier::Machine,
        |s: &Inner, p: OverrideParams| s.host(move |h, _| h.set_override_targets(p.feed, p.rapid, p.power)),
    ));
    add(r.register(
        "machine.read_settings",
        "Reads the board settings again (`$$`).",
        Tier::Machine,
        |s: &Inner, _: NoParams| s.system_op(Host::refresh_settings),
    ));
    add(r.register(
        "machine.write_settings",
        "Writes board settings (`$N=V`), one at a time; returns when the board answered all of them.",
        Tier::Machine,
        |s: &Inner, p: WriteSettingsParams| {
            let values: Vec<(u32, String)> = p.settings.into_iter().collect();
            s.system_op(move |h, now| h.write_settings(&values, now))
        },
    ));
    add(r.register(
        "machine.send",
        "Sends one line of G-code or a `$` command (not during a job). It can turn the laser on.",
        Tier::Laser,
        |s: &Inner, p: SendParams| s.host_result(move |h, now| h.send_command(&p.line, now)),
    ));
    add(r.register(
        "machine.custom_code",
        "Runs custom code (buttons, macros): `[expr]` evaluated, immediate characters sent at once.",
        Tier::Laser,
        |s: &Inner, p: CustomCodeParams| s.host_result(move |h, now| h.execute_custom_code(&p.code, now)),
    ));
    add(r.register(
        "machine.select_laser",
        "Selects the laser module in use (life counters).",
        Tier::Edit,
        |s: &Inner, p: SelectLaserParams| {
            if s.host(move |h, _| h.select_laser(&p.id))? {
                Ok(())
            } else {
                Err(ActionError::InvalidParams("unknown laser id".into()))
            }
        },
    ));

    // ---- job
    add(r.register(
        "job.load",
        "Loads and analyzes a G-code file.",
        Tier::Edit,
        |s: &Inner, p: LoadParams| {
            let bytes = freekerf_platform::read(Path::new(&p.path))
                .map_err(|e| ActionError::Failed(e.to_string()))?;
            let name = Path::new(&p.path)
                .file_name()
                .map_or_else(|| p.path.clone(), |n| n.to_string_lossy().into_owned());
            load_program(s, name, &bytes)
        },
    ));
    add(r.register(
        "job.load_text",
        "Loads and analyzes G-code given as text.",
        Tier::Edit,
        |s: &Inner, p: LoadTextParams| load_program(s, p.name, p.gcode.as_bytes()),
    ));
    add(r.register(
        "job.info",
        "The loaded program (name, lines, estimated time, range).",
        Tier::Read,
        |s: &Inner, _: NoParams| Ok(lock(&s.loaded).as_ref().map(program_info)),
    ));
    add(r.register(
        "job.run",
        "Runs the loaded program from the start (header, program, footer, passes). Fires the laser.",
        Tier::Laser,
        |s: &Inner, p: RunParams| {
            let passes = p.passes.unwrap_or(1);
            s.host_result(move |h, now| {
                h.set_loop_count(passes);
                h.run_program(p.homing, now)
            })
        },
    ));
    add(r.register(
        "job.resume",
        "Runs the loaded program from a position, rebuilding position, power and modes first. Fires the laser.",
        Tier::Laser,
        |s: &Inner, p: ResumeParams| s.host_result(move |h, now| h.resume_program(p.position, p.homing, p.set_wco, now)),
    ));
    add(r.register(
        "job.abort",
        "Stops streaming the job and sends `M5` (lines already in the board still run; use machine.stop to stop at once). Always allowed.",
        Tier::Safety,
        |s: &Inner, _: NoParams| s.host_result(Host::abort_program),
    ));
    r
}

fn load_program(s: &Inner, name: String, bytes: &[u8]) -> Result<ProgramInfo, ActionError> {
    let program = Program::parse_bytes(bytes, &s.analysis_options());
    let copy = program.clone();
    s.host_result(move |h, _| h.set_program(copy))?;
    let loaded = Loaded { name, program };
    let info = program_info(&loaded);
    *lock(&s.loaded) = Some(loaded);
    s.publish(&ServiceEvent::ProgramLoaded);
    Ok(info)
}
