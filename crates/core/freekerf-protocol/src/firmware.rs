//! Firmware variants. A new firmware is a new [`Firmware`] implementation; the
//! defaults are Grbl's behaviour, the variants override what differs
//! (`SmoothieCore`, `MarlinCore`, `VigoCore` of LaserGRBL).

use serde::{Deserialize, Serialize};

use freekerf_gcode::Command;

use crate::host::{Core, StreamingMode};
use crate::status::{MachineStatus, Point3, parse_f32};
use crate::version::FirmwareVersion;

/// The firmware families FreeKerf talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FirmwareKind {
    /// Grbl 0.9/1.1 and grblHAL (and the many vendor builds of them).
    Grbl,
    /// Smoothieware.
    Smoothie,
    /// Marlin.
    Marlin,
    /// VigoWork (Grbl-Vigo).
    VigoWork,
}

impl FirmwareKind {
    /// A new firmware instance of this kind.
    #[must_use]
    pub fn create(self) -> Box<dyn Firmware> {
        match self {
            Self::Grbl => Box::new(Grbl),
            Self::Smoothie => Box::new(Smoothie),
            Self::Marlin => Box::new(Marlin),
            Self::VigoWork => Box::new(Vigo::default()),
        }
    }

    /// Parses `Grbl`, `Smoothie`, `Marlin` or `VigoWork`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "Grbl" => Some(Self::Grbl),
            "Smoothie" => Some(Self::Smoothie),
            "Marlin" => Some(Self::Marlin),
            "VigoWork" => Some(Self::VigoWork),
            _ => None,
        }
    }
}

/// What a firmware did with a received line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineHandled {
    /// Not handled: use Grbl's rules.
    No,
    /// Handled completely.
    Yes,
    /// A status line: apply this machine state.
    Status(MachineStatus),
    /// Treat as these command responses (`ok`, `error:N`).
    Responses(Vec<String>),
}

/// Behaviour that changes with the firmware. Every method has Grbl's default.
pub trait Firmware: Send {
    /// The kind.
    fn kind(&self) -> FirmwareKind;

    /// Streaming mode actually used.
    fn streaming_mode(&self, configured: StreamingMode) -> StreamingMode {
        configured
    }

    /// `$J=` jogging (Grbl 1.1).
    fn supports_true_jogging(&self, version: Option<&FirmwareVersion>) -> bool {
        version.is_some_and(FirmwareVersion::is_v11)
    }

    /// A fixed RX buffer size, if the firmware has one.
    fn fixed_buffer_size(&self) -> Option<usize> {
        None
    }

    /// `$$`/`$I` are understood.
    fn supports_config_queries(&self) -> bool {
        true
    }

    /// The unlock command exists.
    fn shows_unlock(&self) -> bool {
        true
    }

    /// Resets the host state when the port opens (and soft-resets the board when the
    /// option asks for it).
    fn internal_reset_on_start(&self) -> bool {
        true
    }

    /// Asks the position (`?`).
    fn query_position(&self, core: &mut Core) {
        core.send_immediate(b'?');
    }

    /// First bytes after opening.
    fn initialize_board(&self, core: &mut Core) {
        self.query_position(core);
    }

    /// Soft reset of the board (ctrl-x).
    fn send_board_reset(&self, core: &mut Core) {
        core.send_immediate(0x18);
    }

    /// Lines the firmware handles itself, before Grbl's rules.
    fn handle_line(&mut self, _core: &mut Core, _line: &str) -> LineHandled {
        LineHandled::No
    }

    /// Machine state from the state field of a status report.
    fn parse_status_name(&self, name: &str) -> Option<MachineStatus> {
        MachineStatus::parse(name)
    }

    /// `F:` field of a status report (Grbl: feed only).
    fn parse_feed_field(&self, core: &mut Core, text: &str) {
        if let Some(f) = parse_f32(text) {
            core.set_feed_speed(f, 0.0);
        }
    }

    /// Before the program of a job (header).
    fn on_job_begin(&mut self, core: &mut Core) {
        let header = core.header();
        core.execute_custom_code(&header);
    }

    /// After the program of a job (footer).
    fn on_job_end(&mut self, core: &mut Core) {
        let footer = core.footer();
        core.execute_custom_code(&footer);
    }

    /// Puts a line on the wire.
    fn send_to_serial(&mut self, core: &mut Core, cmd: &Command) {
        core.write_counted(cmd);
    }

    /// The firmware does not report Idle by itself at the end of a job.
    fn force_idle_at_end(&self) -> bool {
        false
    }

    /// No status for too long during a job.
    fn hang_detected(&self, core: &Core) -> bool {
        let silent = core.now().saturating_sub(core.last_status_at());
        silent > core.query_period() * 10 && silent > 5000
    }
}

/// Grbl, grblHAL and compatible boards.
#[derive(Debug, Clone, Copy, Default)]
pub struct Grbl;

impl Firmware for Grbl {
    fn kind(&self) -> FirmwareKind {
        FirmwareKind::Grbl
    }
}

/// Smoothieware.
#[derive(Debug, Clone, Copy, Default)]
pub struct Smoothie;

impl Firmware for Smoothie {
    fn kind(&self) -> FirmwareKind {
        FirmwareKind::Smoothie
    }

    fn streaming_mode(&self, _configured: StreamingMode) -> StreamingMode {
        StreamingMode::Synchronous
    }

    fn supports_true_jogging(&self, _version: Option<&FirmwareVersion>) -> bool {
        false
    }

    fn supports_config_queries(&self) -> bool {
        true
    }

    fn shows_unlock(&self) -> bool {
        false
    }

    fn internal_reset_on_start(&self) -> bool {
        false
    }

    fn initialize_board(&self, core: &mut Core) {
        core.send_immediate(b'\n');
        self.query_position(core);
    }

    fn send_board_reset(&self, core: &mut Core) {
        core.write_raw("reset\r\n");
    }

    fn parse_feed_field(&self, core: &mut Core, text: &str) {
        let mut it = text.split(',');
        let f = it.next().and_then(parse_f32);
        let s = it.next().and_then(parse_f32);
        if let (Some(f), Some(s)) = (f, s) {
            core.set_feed_speed(f, s);
        }
    }
}

/// Marlin: no real-time commands, the position is polled with `M114`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Marlin;

impl Firmware for Marlin {
    fn kind(&self) -> FirmwareKind {
        FirmwareKind::Marlin
    }

    fn streaming_mode(&self, _configured: StreamingMode) -> StreamingMode {
        StreamingMode::Synchronous
    }

    fn supports_true_jogging(&self, _version: Option<&FirmwareVersion>) -> bool {
        false
    }

    fn supports_config_queries(&self) -> bool {
        false
    }

    fn shows_unlock(&self) -> bool {
        false
    }

    fn query_position(&self, core: &mut Core) {
        if core.status() != MachineStatus::Run {
            core.write_raw("M114\n");
        }
    }

    fn send_board_reset(&self, _core: &mut Core) {}

    /// `X:10.00 Y:0.00 Z:0.00 E:0.00 Count X:1600 Y:0 Z:0`.
    fn handle_line(&mut self, core: &mut Core, line: &str) -> LineHandled {
        if !line.starts_with("X:") {
            return LineHandled::No;
        }
        core.status_seen();
        let value = |i: usize| line.split(' ').nth(i).and_then(|p| p.split(':').nth(1)).and_then(parse_f32);
        if let (Some(x), Some(y), Some(z)) = (value(0), value(1), value(2)) {
            core.set_machine_position(Point3::new(x, y, z));
        }
        LineHandled::Status(MachineStatus::Idle)
    }

    fn parse_status_name(&self, name: &str) -> Option<MachineStatus> {
        Some(if name.contains("ok") { MachineStatus::Idle } else { MachineStatus::Disconnected })
    }

    fn force_idle_at_end(&self) -> bool {
        true
    }

    fn hang_detected(&self, core: &Core) -> bool {
        let inactive = core.now().saturating_sub(core.last_activity_at());
        let silent = core.now().saturating_sub(core.last_status_at());
        inactive > 10_000 && inactive > core.query_period() * 10 && silent > 5000
    }
}

/// VigoWork: jobs are wrapped in vendor commands and acknowledged through `SBuf`
/// counters in `<VSta...>` reports.
#[derive(Debug, Clone, Default)]
pub struct Vigo {
    in_job: bool,
    old_state: i32,
    old_received: i32,
    old_managed: i32,
    old_error: i32,
}

const VIGO_JOB_BEGIN: &str = ">Ofk9gsd8IKjKBahP0OGS9BrhZCPeWCBALCbyGf";
const VIGO_JOB_END: &str = ">NPredMjdFaeaJajf6OHy:hkRUygcpBXwtV";

impl Firmware for Vigo {
    fn kind(&self) -> FirmwareKind {
        FirmwareKind::VigoWork
    }

    fn fixed_buffer_size(&self) -> Option<usize> {
        Some(127)
    }

    fn query_position(&self, core: &mut Core) {
        core.send_immediate(0x88);
    }

    fn handle_line(&mut self, _core: &mut Core, line: &str) -> LineHandled {
        if !(line.starts_with("<VSta") && line.ends_with('>')) {
            return LineHandled::No;
        }
        let mut responses = Vec::new();
        for part in line[1..line.len() - 1].split('|') {
            if let Some(v) = part.strip_prefix("VSta:") {
                #[allow(clippy::cast_possible_truncation)]
                let Some(state) = parse_f32(v).map(|f| f as i32) else { break };
                if self.old_state == 2 && state == 0 {
                    self.in_job = false;
                }
                self.old_state = state;
            } else if let Some(v) = part.strip_prefix("SBuf:") {
                #[allow(clippy::cast_possible_truncation)]
                let n: Vec<Option<i32>> = v.split(',').map(|x| parse_f32(x).map(|f| f as i32)).collect();
                let [Some(received), Some(managed), Some(errors), ..] = n.as_slice() else { break };
                if self.old_received > *received {
                    // the board restarted its counters
                    self.old_received = 0;
                    self.old_managed = 0;
                    self.old_error = 0;
                }
                for _ in self.old_managed..*managed {
                    responses.push("ok".to_owned());
                }
                for _ in self.old_error..*errors {
                    responses.push("error:99".to_owned());
                }
                self.old_received = *received;
                self.old_managed = *managed;
                self.old_error = *errors;
            }
        }
        LineHandled::Responses(responses)
    }

    fn on_job_begin(&mut self, core: &mut Core) {
        self.in_job = true;
        core.enqueue(Command::preserving_case(VIGO_JOB_BEGIN));
        let header = core.header();
        core.execute_custom_code(&header);
    }

    fn on_job_end(&mut self, core: &mut Core) {
        let footer = core.footer();
        core.execute_custom_code(&footer);
        core.enqueue(Command::preserving_case("G0X0Y0M5"));
        core.enqueue(Command::preserving_case(VIGO_JOB_END));
    }

    /// Outside a job the board does not acknowledge lines: they are written without
    /// counting them (they stay pending, as in LaserGRBL: FINDINGS F-08, DIV-008).
    fn send_to_serial(&mut self, core: &mut Core, cmd: &Command) {
        if self.in_job {
            core.write_counted(cmd);
        } else {
            core.write_uncounted(cmd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        for k in [FirmwareKind::Grbl, FirmwareKind::Smoothie, FirmwareKind::Marlin, FirmwareKind::VigoWork] {
            assert_eq!(k.create().kind(), k);
            assert_eq!(FirmwareKind::parse(&format!("{k:?}")), Some(k));
        }
        assert_eq!(FirmwareKind::parse("x"), None);
    }

    #[test]
    fn capabilities() {
        let v11 = FirmwareVersion::new(1, 1);
        assert!(Grbl.supports_true_jogging(Some(&v11)));
        assert!(!Grbl.supports_true_jogging(None));
        assert!(!Smoothie.supports_true_jogging(Some(&v11)));
        assert!(!Marlin.supports_true_jogging(Some(&v11)));
        assert_eq!(Smoothie.streaming_mode(StreamingMode::Buffered), StreamingMode::Synchronous);
        assert_eq!(Marlin.streaming_mode(StreamingMode::Buffered), StreamingMode::Synchronous);
        assert_eq!(Grbl.streaming_mode(StreamingMode::Buffered), StreamingMode::Buffered);
        assert_eq!(Vigo::default().fixed_buffer_size(), Some(127));
        assert!(!Marlin.supports_config_queries() && Grbl.supports_config_queries());
        assert!(!Smoothie.shows_unlock() && !Marlin.shows_unlock() && Grbl.shows_unlock());
        assert!(Marlin.force_idle_at_end() && !Grbl.force_idle_at_end());
        assert!(!Smoothie.internal_reset_on_start());
        assert_eq!(Marlin.parse_status_name("ok"), Some(MachineStatus::Idle));
        assert_eq!(Marlin.parse_status_name("x"), Some(MachineStatus::Disconnected));
    }
}
