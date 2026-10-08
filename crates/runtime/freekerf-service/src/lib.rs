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
use freekerf_protocol::Clock;
use freekerf_protocol::host::decimal;
use freekerf_protocol::{
    ConsoleRow, Event, FirmwareKind, Host, HostError, HostSnapshot, Issue, JogDirection, ResumeSuggestion,
};
use freekerf_session::fire::{FireGate, FireRequest, FireSummary};
use freekerf_session::view::{Dialog, Mode, Panel, ToastLevel, ViewCommand, ViewState, Viewport};
use freekerf_session::{NoParams, Registry};
use freekerf_settings::{MachineCache, Settings, SettingsStore};
use freekerf_transport::{EmulatorTransport, SerialTransport, Transport};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod analysis;
pub mod preview;

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
    /// The view state changed (`view.*` actions).
    ViewChanged(Box<ViewState>),
}

/// A [`ServiceEvent`] as front ends receive it (JSON, tagged by `event`).
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EventMessage {
    /// The link opened.
    Connected {
        /// Transport description.
        transport: String,
    },
    /// The link closed.
    Disconnected {
        /// Closed by the user (else: lost or failed).
        user: bool,
    },
    /// A protocol event.
    Host {
        /// The event.
        data: Event,
    },
    /// Settings changed.
    SettingsChanged,
    /// A program was loaded.
    ProgramLoaded,
    /// The view state changed.
    View {
        /// The new view state.
        view: Box<ViewState>,
    },
}

impl ServiceEvent {
    /// The event as JSON for front ends (`{"event": "...", ...}`, see [`EventMessage`]).
    #[must_use]
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self.message()).unwrap_or(Value::Null)
    }

    /// The event in its wire shape.
    #[must_use]
    pub fn message(&self) -> EventMessage {
        match self {
            Self::SettingsChanged => EventMessage::SettingsChanged,
            Self::ProgramLoaded => EventMessage::ProgramLoaded,
            Self::Machine(MachineEvent::Connected { transport }) => EventMessage::Connected {
                transport: transport.clone(),
            },
            Self::Machine(MachineEvent::Disconnected { user }) => EventMessage::Disconnected { user: *user },
            Self::Machine(MachineEvent::Host(e)) => EventMessage::Host { data: e.clone() },
            Self::ViewChanged(v) => EventMessage::View { view: v.clone() },
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
    /// Tool path buffer ([`preview`]), built on the first request.
    preview: Option<Arc<Vec<u8>>>,
}

/// State shared by the actions.
pub struct Inner {
    machine: Machine,
    settings: Mutex<Settings>,
    stores: Stores,
    loaded: Mutex<Option<Loaded>>,
    view: Mutex<ViewState>,
    fire: Mutex<FireGate>,
    fire_clock: Mutex<Arc<dyn Clock>>,
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
            view: Mutex::new(ViewState::default()),
            fire: Mutex::new(FireGate::default()),
            fire_clock: Mutex::new(Arc::new(SystemClock::default())),
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

    /// The tool path of the loaded program as the binary buffer of [`preview`]
    /// (built once per program), or `None` when no program is loaded. Bulk data for
    /// the canvas: front ends read it directly, not through an action.
    #[must_use]
    pub fn preview(&self) -> Option<Arc<Vec<u8>>> {
        let options = self.inner.analysis_options();
        let mut loaded = lock(&self.inner.loaded);
        let l = loaded.as_mut()?;
        Some(Arc::clone(l.preview.get_or_insert_with(|| {
            Arc::new(preview::encode(&l.program, &options))
        })))
    }

    /// Runs the pending laser request `id` after the person confirmed it on screen
    /// (PLAN §9.5). Not an action of the registry on purpose: scripts and agents can
    /// ask for a laser action (`fire.request`) but never confirm one. Only a front
    /// end with a person in front of it calls this (the desktop window's
    /// "Fire laser" button).
    ///
    /// # Errors
    /// No such request, countdown not over, or the action's own error.
    pub fn confirm_fire(&self, id: u64) -> Result<Value, ActionError> {
        let now = lock(&self.inner.fire_clock).now_ms();
        let request = lock(&self.inner.fire)
            .confirm(id, now)
            .map_err(|e| ActionError::Refused(e.to_string()))?;
        // the dialog closes whatever happens next
        self.inner.view(ViewCommand::SetFireRequest(None))?;
        tracing::info!(action = %request.action, by = %request.requested_by, "laser action confirmed");
        self.registry
            .dispatch(&self.inner, &Caller::user(), &request.action, request.params)
    }

    /// Replaces the clock of the fire confirmation countdown (tests).
    pub fn set_fire_clock(&self, clock: Arc<dyn Clock>) {
        *lock(&self.inner.fire_clock) = clock;
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

    /// What the fire confirmation shows for a request.
    fn fire_summary(&self, action: &str, params: &Value) -> FireSummary {
        let code = params
            .get("line")
            .or_else(|| params.get("code"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let program = match action {
            "job.run" | "job.resume" => lock(&self.loaded).as_ref().map(program_info),
            _ => None,
        };
        FireSummary {
            program: program.as_ref().map(|p| p.name.clone()),
            lines: program.as_ref().map(|p| p.lines),
            estimated_ms: program.as_ref().map(|p| p.estimated_ms),
            bounds: program.and_then(|p| p.drawing_range),
            code,
        }
    }

    /// Applies a view command and publishes the new view state when it changed.
    fn view(&self, cmd: ViewCommand) -> Result<Option<u64>, ActionError> {
        let (result, changed) = {
            let mut v = lock(&self.view);
            let before = v.clone();
            let r = v
                .apply(cmd)
                .map_err(|e| ActionError::InvalidParams(e.to_string()))?;
            (r, (*v != before).then(|| v.clone()))
        };
        if let Some(v) = changed {
            self.publish(&ServiceEvent::ViewChanged(Box::new(v)));
        }
        Ok(result)
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

/// `machine.system_command`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SystemCommandParams {
    /// A `$` command (`$$`, `$I`, `$H`, `$X`, `$#`, `$G`, `$N=V`, `$J=...`): never turns
    /// the laser on. Other G-code goes through `machine.send` (tier `laser`).
    pub line: String,
}

/// `job.frame`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameParams {
    /// Feed in mm/min (default: the jog setting).
    pub feed_mm_min: Option<f64>,
}

/// Result of `machine.info`: what the board told about itself.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MachineInfo {
    /// Work area width (`$130`), mm.
    pub table_width_mm: f64,
    /// Work area height (`$131`), mm.
    pub table_height_mm: f64,
    /// Laser mode (`$32`).
    pub laser_mode: bool,
    /// Highest S (`$30`).
    pub max_power: f64,
    /// Homing enabled (`$22`).
    pub homing_enabled: bool,
    /// The `$$` table was read.
    pub settings_known: bool,
    /// Laser module life counters.
    pub laser_life: Vec<freekerf_protocol::LaserLifeCounter>,
    /// Index of the module in use, if known.
    pub laser_in_use: Option<usize>,
}

/// `fire.request`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FireRequestParams {
    /// An action of tier `laser` (`job.run`, `job.resume`, `machine.send`, `machine.custom_code`).
    pub action: String,
    /// Its parameters.
    #[serde(default)]
    pub params: Value,
}

/// `fire.cancel`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FireCancelParams {
    /// The request; the pending one when absent.
    pub id: Option<u64>,
}

/// `view.set_mode`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetModeParams {
    /// `design` or `produce`.
    pub mode: Mode,
}

/// `view.show_panel`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShowPanelParams {
    /// `layers`, `object`, `machine` or `console`.
    pub panel: Panel,
}

/// `view.set_drawer`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetDrawerParams {
    /// Open or closed.
    pub open: bool,
}

/// `view.open_dialog`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenDialogParams {
    /// The dialog.
    pub dialog: Dialog,
}

/// `view.close_dialog`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CloseDialogParams {
    /// The dialog; the one on top when absent.
    pub dialog: Option<Dialog>,
}

/// `view.toast`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToastParams {
    /// `info` (default), `ok`, `warning` or `error`.
    #[serde(default)]
    pub level: ToastLevel,
    /// i18n key (or plain text).
    pub message: String,
    /// Placeholders of the message.
    #[serde(default)]
    pub args: BTreeMap<String, String>,
}

/// `view.dismiss_toast`.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DismissToastParams {
    /// Id returned by `view.toast`.
    pub id: u64,
}

/// `view.focus`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FocusParams {
    /// Element with the focus (`canvas`, `console`...), none when absent.
    pub target: Option<String>,
}

// ====================================================================== results

/// Summary of the loaded program.
#[derive(Debug, Clone, Serialize, JsonSchema)]
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
#[derive(Debug, Clone, Serialize, JsonSchema)]
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
        "machine.system_command",
        "Sends a `$` command (`$$`, `$I`, `$H`, `$X`, `$N=V`, `$J=...`; not during a job). It cannot turn the laser on.",
        Tier::Machine,
        |s: &Inner, p: SystemCommandParams| {
            let line = p.line.trim().to_owned();
            if !is_system_command(&line) {
                return Err(ActionError::InvalidParams(
                    "only `$` commands; other G-code needs machine.send".into(),
                ));
            }
            s.host_result(move |h, now| h.send_command(&line, now))
        },
    ));
    add(r.register(
        "machine.info",
        "Work area, laser mode, highest power and laser module counters, from the board settings.",
        Tier::Read,
        |s: &Inner, _: NoParams| {
            s.host(|h, _| {
                let st = h.settings();
                let life = h.laser_life();
                MachineInfo {
                    table_width_mm: decimal_to_f64(st.table_width()),
                    table_height_mm: decimal_to_f64(st.table_height()),
                    laser_mode: st.laser_mode(),
                    max_power: decimal_to_f64(st.max_pwm()),
                    homing_enabled: st.homing_enabled(),
                    settings_known: !st.is_empty(),
                    laser_life: life.counters.clone(),
                    laser_in_use: life.current,
                }
            })
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
        "job.frame",
        "Runs the head around the bounds of the loaded program with the laser off (jog moves).",
        Tier::Machine,
        |s: &Inner, p: FrameParams| {
            let range = lock(&s.loaded)
                .as_ref()
                .and_then(|l| program_info(l).drawing_range)
                .ok_or_else(|| ActionError::Refused("no program with a drawing loaded".into()))?;
            let feed = p
                .feed_mm_min
                .unwrap_or_else(|| lock(&s.settings).jog.speed_mm_min);
            #[allow(clippy::cast_possible_truncation)]
            let (x0, y0, x1, y1, feed) = (
                range[0] as f32,
                range[1] as f32,
                range[2] as f32,
                range[3] as f32,
                feed as f32,
            );
            s.host_result(move |h, now| {
                for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)] {
                    h.jog_to(x, y, feed, now)?;
                }
                Ok(())
            })
        },
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
    register_view(&mut r);
    register_fire(&mut r);
    describe_results(&mut r);
    r
}

/// `fire.request` and `fire.cancel` (PLAN §9.5). Confirming is
/// [`Service::confirm_fire`], not an action.
fn register_fire(r: &mut Registry<Inner>) {
    let laser: Vec<String> = r
        .specs()
        .filter(|s| s.tier == Tier::Laser)
        .map(|s| s.name.clone())
        .collect();
    let add = |result: Result<(), freekerf_session::RegistryError>| {
        if let Err(e) = result {
            tracing::error!("action registration: {e}");
        }
    };
    add(r.register(
        "fire.request",
        "Asks to run a laser action: it opens the fire confirmation (checklist and countdown) and runs only when a person confirms on screen. Returns the pending request.",
        Tier::Machine,
        move |s: &Inner, p: FireRequestParams| {
            if !laser.contains(&p.action) {
                return Err(ActionError::InvalidParams(format!(
                    "`{}` is not a laser action ({})",
                    p.action,
                    laser.join(", ")
                )));
            }
            let summary = s.fire_summary(&p.action, &p.params);
            let countdown_ms = lock(&s.settings).safety.fire_countdown_s.saturating_mul(1000);
            let now = lock(&s.fire_clock).now_ms();
            let request = lock(&s.fire).request(&p.action, p.params, "desktop", summary, countdown_ms, now);
            s.view(ViewCommand::SetFireRequest(Some(request.clone())))?;
            Ok(request)
        },
    ));
    add(r.register(
        "fire.cancel",
        "Cancels the pending laser request (the one given, or any). Always allowed.",
        Tier::Safety,
        |s: &Inner, p: FireCancelParams| {
            if lock(&s.fire).cancel(p.id) {
                s.view(ViewCommand::SetFireRequest(None))?;
            }
            Ok(())
        },
    ));
}

/// The `view.*` actions (tier `ui`): what the app shows, as data.
fn register_view(r: &mut Registry<Inner>) {
    let add = |result: Result<(), freekerf_session::RegistryError>| {
        if let Err(e) = result {
            tracing::error!("action registration: {e}");
        }
    };
    let unit = |s: &Inner, c: ViewCommand| s.view(c).map(|_| ());
    add(r.register(
        "view.get",
        "The view state: mode, panel, dialogs, toasts, viewport, focus.",
        Tier::Read,
        |s: &Inner, _: NoParams| Ok(lock(&s.view).clone()),
    ));
    add(r.register(
        "view.set_mode",
        "Switches between the Design and Produce modes.",
        Tier::Ui,
        move |s: &Inner, p: SetModeParams| unit(s, ViewCommand::SetMode(p.mode)),
    ));
    add(r.register(
        "view.show_panel",
        "Shows a panel of the dock (opens the drawer in the medium layout).",
        Tier::Ui,
        move |s: &Inner, p: ShowPanelParams| unit(s, ViewCommand::ShowPanel(p.panel)),
    ));
    add(r.register(
        "view.set_drawer",
        "Opens or closes the panel drawer (medium layout).",
        Tier::Ui,
        move |s: &Inner, p: SetDrawerParams| unit(s, ViewCommand::SetDrawer(p.open)),
    ));
    add(r.register(
        "view.open_dialog",
        "Opens a dialog on top of the others (dialogs are not modal: STOP stays reachable).",
        Tier::Ui,
        move |s: &Inner, p: OpenDialogParams| unit(s, ViewCommand::OpenDialog(p.dialog)),
    ));
    add(r.register(
        "view.close_dialog",
        "Closes a dialog (the one on top when none is given).",
        Tier::Ui,
        move |s: &Inner, p: CloseDialogParams| {
            let target = p.dialog.or_else(|| lock(&s.view).top_dialog());
            if target == Some(Dialog::FireConfirmation) {
                // closing the fire confirmation cancels its request
                lock(&s.fire).cancel(None);
                return unit(s, ViewCommand::SetFireRequest(None));
            }
            unit(s, ViewCommand::CloseDialog(p.dialog))
        },
    ));
    add(r.register(
        "view.toast",
        "Shows a short message (at most 3 at once); returns its id.",
        Tier::Ui,
        |s: &Inner, p: ToastParams| {
            s.view(ViewCommand::Toast {
                level: p.level,
                message: p.message,
                args: p.args,
            })
            .map(|id| id.unwrap_or(0))
        },
    ));
    add(r.register(
        "view.dismiss_toast",
        "Removes a toast.",
        Tier::Ui,
        move |s: &Inner, p: DismissToastParams| unit(s, ViewCommand::DismissToast(p.id)),
    ));
    add(r.register(
        "view.set_viewport",
        "Shows the table around a point at a zoom (CSS pixels per mm).",
        Tier::Ui,
        move |s: &Inner, p: Viewport| unit(s, ViewCommand::SetViewport(p)),
    ));
    add(r.register(
        "view.fit",
        "Fits the work area in the canvas.",
        Tier::Ui,
        move |s: &Inner, _: NoParams| unit(s, ViewCommand::Fit),
    ));
    add(r.register(
        "view.focus",
        "Records which element has the keyboard focus.",
        Tier::Ui,
        move |s: &Inner, p: FocusParams| unit(s, ViewCommand::Focus(p.target)),
    ));
}

/// Result schemas, used to generate the front-end types (`cargo xtask gen-ts`).
/// Actions not listed here (`settings.get`, and new actions until they are added)
/// are typed as unknown.
fn describe_results(r: &mut Registry<Inner>) {
    let add = |result: Result<(), freekerf_session::RegistryError>| {
        if let Err(e) = result {
            tracing::error!("result description: {e}");
        }
    };
    add(r.describe_result::<State>("state.get"));
    add(r.describe_result::<Vec<ConsoleRow>>("console.get"));
    add(r.describe_result::<Vec<String>>("machine.list_ports"));
    add(r.describe_result::<ProgramInfo>("job.load"));
    add(r.describe_result::<ProgramInfo>("job.load_text"));
    add(r.describe_result::<Option<ProgramInfo>>("job.info"));
    add(r.describe_result::<ViewState>("view.get"));
    add(r.describe_result::<MachineInfo>("machine.info"));
    add(r.describe_result::<FireRequest>("fire.request"));
    add(r.describe_result::<u64>("view.toast"));
    for name in [
        "settings.set",
        "machine.connect",
        "machine.disconnect",
        "machine.stop",
        "machine.feed_hold",
        "machine.jog_stop",
        "machine.safety_door",
        "machine.resume",
        "machine.unlock",
        "machine.home",
        "machine.set_zero",
        "machine.jog",
        "machine.jog_to",
        "machine.overrides",
        "machine.read_settings",
        "machine.write_settings",
        "machine.send",
        "machine.system_command",
        "machine.custom_code",
        "machine.select_laser",
        "job.frame",
        "fire.cancel",
        "job.run",
        "job.resume",
        "job.abort",
        "view.set_mode",
        "view.show_panel",
        "view.set_drawer",
        "view.open_dialog",
        "view.close_dialog",
        "view.dismiss_toast",
        "view.set_viewport",
        "view.fit",
        "view.focus",
    ] {
        add(r.describe_result::<()>(name));
    }
}

/// A `$` command (never turns the laser on).
fn is_system_command(line: &str) -> bool {
    line.starts_with('$') && !line.contains(['\n', '\r'])
}

fn decimal_to_f64(d: freekerf_gcode::Decimal) -> f64 {
    freekerf_gcode::number::f64_from_decimal(d)
}

fn load_program(s: &Inner, name: String, bytes: &[u8]) -> Result<ProgramInfo, ActionError> {
    let program = Program::parse_bytes(bytes, &s.analysis_options());
    let copy = program.clone();
    s.host_result(move |h, _| h.set_program(copy))?;
    let loaded = Loaded {
        name,
        program,
        preview: None,
    };
    let info = program_info(&loaded);
    *lock(&s.loaded) = Some(loaded);
    s.publish(&ServiceEvent::ProgramLoaded);
    Ok(info)
}
