//! Console rows (what the host sent and received) and the decoding of Grbl codes
//! (`error:N`, `ALARM:N`, `$N=V`) into readable text.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Kind of a received message (`GrblMessage.MessageType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageKind {
    /// Welcome banner.
    Startup,
    /// `$N=V`.
    Config,
    /// `ALARM:N`.
    Alarm,
    /// `[...]`.
    Feedback,
    /// `<...>` (status reports are not logged).
    Position,
    /// Anything else.
    Other,
    /// Raised by the host (e.g. buffer stuck recovery).
    Warning,
    /// Host diagnostics (job summary).
    Diagnostic,
}

impl MessageKind {
    /// Classifies a line received from the board.
    #[must_use]
    pub fn classify(line: &str) -> Self {
        let lower = line.to_lowercase();
        if lower.starts_with('$') && line.contains('=') {
            Self::Config
        } else if lower.starts_with("grbl") {
            Self::Startup
        } else if lower.starts_with("alarm") {
            Self::Alarm
        } else if line.starts_with('<') && line.ends_with('>') {
            Self::Position
        } else if line.starts_with('[') && line.ends_with(']') {
            Self::Feedback
        } else {
            Self::Other
        }
    }
}

/// Status of a sent command (`CommandStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandStatus {
    /// Not sent yet.
    Queued,
    /// Sent, no answer yet.
    WaitingResponse,
    /// `ok`.
    ResponseGood,
    /// `error:N`.
    ResponseBad,
    /// Something else.
    InvalidResponse,
}

impl CommandStatus {
    /// Status for a coded result (upper case, as stored by LaserGRBL).
    #[must_use]
    pub fn from_result(result: &str) -> Self {
        if result.is_empty() {
            Self::WaitingResponse
        } else if result.starts_with("OK") {
            Self::ResponseGood
        } else if result.starts_with("ERROR") {
            Self::ResponseBad
        } else {
            Self::InvalidResponse
        }
    }
}

/// One row of the console.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsoleRow {
    /// A command was sent. `id` matches the [`ConsoleRow::Result`] that follows.
    Command {
        /// Row id.
        id: u64,
        /// Command text (with ` (Retry n)` for retries).
        text: String,
    },
    /// The answer to a command.
    Result {
        /// Id of the command row.
        id: u64,
        /// Coded result (`OK`, `ERROR:22`).
        result: String,
        /// Status.
        status: CommandStatus,
        /// Short description of the error, when known.
        decoded: Option<String>,
    },
    /// A message from the board or from the host.
    Message {
        /// Kind.
        kind: MessageKind,
        /// Text as received.
        text: String,
        /// Readable form (setting name, alarm description), when known.
        decoded: Option<String>,
    },
}

struct Tables {
    errors: HashMap<String, Vec<String>>,
    alarms: HashMap<String, Vec<String>>,
    settings: HashMap<String, Vec<String>>,
}

fn parse_csv(text: &str, columns: usize) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let mut parts = line.splitn(columns + 1, ',');
        let key = parts.next().unwrap_or("").trim().to_owned();
        let values: Vec<String> = parts.map(|p| p.trim().to_owned()).collect();
        map.entry(key).or_insert(values);
    }
    map
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| Tables {
        errors: parse_csv(include_str!("../data/error_codes.csv"), 2),
        alarms: parse_csv(include_str!("../data/alarm_codes.csv"), 2),
        settings: parse_csv(include_str!("../data/setting_codes.v1.1.csv"), 3),
    })
}

/// `(brief, description)` of `error:N`.
#[must_use]
pub fn describe_error(code: &str) -> Option<(&'static str, &'static str)> {
    tables().errors.get(code).and_then(|v| Some((v.first()?.as_str(), v.get(1)?.as_str())))
}

/// `(brief, description)` of `ALARM:N`.
#[must_use]
pub fn describe_alarm(code: &str) -> Option<(&'static str, &'static str)> {
    tables().alarms.get(code).and_then(|v| Some((v.first()?.as_str(), v.get(1)?.as_str())))
}

/// `(name, unit, description)` of setting `$N`.
#[must_use]
pub fn describe_setting(number: &str) -> Option<(&'static str, &'static str, &'static str)> {
    tables().settings.get(number).and_then(|v| Some((v.first()?.as_str(), v.get(1)?.as_str(), v.get(2)?.as_str())))
}

/// Readable form of a received line (only for Grbl 1.1, which uses these codes).
/// Unknown alarms keep their text (FINDINGS F-03 fixed: DIV-003b).
#[must_use]
pub fn decode_message(line: &str) -> Option<String> {
    match MessageKind::classify(line) {
        MessageKind::Config => {
            let key = line.get(1..line.find('=')?)?;
            describe_setting(key).map(|(name, _, _)| format!("{line} ({name})"))
        }
        MessageKind::Alarm => {
            let key = &line[line.find(':')? + 1..];
            describe_alarm(key).map(|(brief, _)| brief.to_owned())
        }
        _ => None,
    }
}

/// Short description of an `ERROR:N` result.
#[must_use]
pub fn decode_result(result: &str) -> Option<String> {
    let key = &result[result.find(':')? + 1..];
    describe_error(key).map(|(brief, _)| brief.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        assert_eq!(MessageKind::classify("$110=500"), MessageKind::Config);
        assert_eq!(MessageKind::classify("Grbl 1.1f"), MessageKind::Startup);
        assert_eq!(MessageKind::classify("ALARM:1"), MessageKind::Alarm);
        assert_eq!(MessageKind::classify("<Idle>"), MessageKind::Position);
        assert_eq!(MessageKind::classify("[MSG:x]"), MessageKind::Feedback);
        assert_eq!(MessageKind::classify("hello"), MessageKind::Other);
        assert_eq!(MessageKind::classify("$H"), MessageKind::Other);
    }

    #[test]
    fn decoding() {
        assert_eq!(describe_error("2").unwrap().0, "Bad number format");
        assert_eq!(describe_alarm("1").unwrap().0, "Hard limit");
        assert!(describe_setting("110").unwrap().0.contains("X"));
        assert_eq!(decode_message("ALARM:1").as_deref(), Some("Hard limit"));
        assert_eq!(decode_message("ALARM:99"), None);
        assert!(decode_message("$110=500").unwrap().starts_with("$110=500 ("));
        assert_eq!(decode_message("$999=1"), None);
        assert_eq!(decode_message("[MSG:x]"), None);
        assert_eq!(decode_result("ERROR:2").as_deref(), Some("Bad number format"));
        assert_eq!(decode_result("OK"), None);
        assert_eq!(decode_result("ERROR:999"), None);
    }

    #[test]
    fn command_status_from_result() {
        assert_eq!(CommandStatus::from_result(""), CommandStatus::WaitingResponse);
        assert_eq!(CommandStatus::from_result("OK"), CommandStatus::ResponseGood);
        assert_eq!(CommandStatus::from_result("ERROR:1"), CommandStatus::ResponseBad);
        assert_eq!(CommandStatus::from_result("WHAT"), CommandStatus::InvalidResponse);
    }
}
