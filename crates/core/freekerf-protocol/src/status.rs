//! Machine states and the parsing of real-time status reports (`<...>`).

use serde::{Deserialize, Serialize};

use crate::version::FirmwareVersion;

/// State of the machine as the host sees it (`MacStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MachineStatus {
    /// No connection.
    Disconnected,
    /// Port open, no status report yet.
    Connecting,
    /// Ready.
    Idle,
    /// Executing motion.
    Run,
    /// Feed hold requested by the user.
    Hold,
    /// Safety door open.
    Door,
    /// Homing cycle.
    Home,
    /// Alarm: G-code locked out until unlock or homing.
    Alarm,
    /// Check G-code mode (`$C`).
    Check,
    /// Jogging.
    Jog,
    /// Queue (Smoothie).
    Queue,
    /// Hold requested by the automatic cooling cycle.
    Cooling,
    /// Hold not requested by the host (e.g. from a button on the machine).
    AutoHold,
    /// Tool change (grblHAL).
    Tool,
}

impl MachineStatus {
    /// Parses a state name as Grbl prints it (`Idle`, `Hold:0` → `Hold`).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let name = text.split(':').next().unwrap_or(text).trim();
        Some(match name {
            "Disconnected" => Self::Disconnected,
            "Connecting" => Self::Connecting,
            "Idle" => Self::Idle,
            "Run" => Self::Run,
            "Hold" => Self::Hold,
            "Door" => Self::Door,
            "Home" => Self::Home,
            "Alarm" => Self::Alarm,
            "Check" => Self::Check,
            "Jog" => Self::Jog,
            "Queue" => Self::Queue,
            "Cooling" => Self::Cooling,
            "AutoHold" => Self::AutoHold,
            "Tool" => Self::Tool,
            _ => return None,
        })
    }

    /// Any kind of hold (user, cooling or machine).
    #[must_use]
    pub const fn is_any_hold(self) -> bool {
        matches!(self, Self::Hold | Self::Cooling | Self::AutoHold)
    }

    /// Name as printed by Grbl.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "Disconnected",
            Self::Connecting => "Connecting",
            Self::Idle => "Idle",
            Self::Run => "Run",
            Self::Hold => "Hold",
            Self::Door => "Door",
            Self::Home => "Home",
            Self::Alarm => "Alarm",
            Self::Check => "Check",
            Self::Jog => "Jog",
            Self::Queue => "Queue",
            Self::Cooling => "Cooling",
            Self::AutoHold => "AutoHold",
            Self::Tool => "Tool",
        }
    }
}

impl std::fmt::Display for MachineStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A position (machine or work), in mm, as `float` like LaserGRBL (`GPoint`).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point3 {
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Z.
    pub z: f32,
}

impl Point3 {
    /// The origin.
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// A point.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

impl std::ops::Add for Point3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl std::ops::Sub for Point3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

/// One field of a status report, in the order it appeared.
#[derive(Debug, Clone, PartialEq)]
pub enum StatusField {
    /// `MPos:x,y,z`.
    MachinePosition(Point3),
    /// `WPos:x,y,z`.
    WorkPosition(Point3),
    /// `WCO:x,y,z`.
    WorkOffset(Point3),
    /// `Bf:blocks,bytes` (free planner blocks, free RX bytes).
    Buffer {
        /// Free planner blocks.
        blocks: i32,
        /// Free RX buffer bytes.
        bytes: i32,
    },
    /// `FS:feed,speed`.
    FeedSpeed(f32, f32),
    /// `F:feed` (Smoothie: `F:feed,speed`), raw text after `F:`.
    Feed(String),
    /// `Ov:feed,rapid,spindle`.
    Overrides(i32, i32, i32),
}

/// A parsed status report: the state name and the fields understood before the
/// first malformed one (LaserGRBL stops at the first error).
#[derive(Debug, Clone, PartialEq)]
pub struct StatusReport {
    /// The state text (`Hold:0` keeps its sub-state).
    pub state: String,
    /// Fields in order.
    pub fields: Vec<StatusField>,
    /// False when a field could not be parsed (later fields are missing).
    pub complete: bool,
}

/// The version used to read a report: the known one, or a guess from its shape.
#[must_use]
pub fn report_version(known: Option<&FirmwareVersion>, body: &str) -> FirmwareVersion {
    if let Some(v) = known {
        return v.clone();
    }
    if body.contains('|') && !body.contains("Pin:") {
        FirmwareVersion::new(1, 1)
    } else if body.contains('|') && body.contains("Pin:") {
        FirmwareVersion::with_build(1, 0, 'c')
    } else {
        FirmwareVersion::new(0, 9)
    }
}

fn floats(text: &str) -> Option<Point3> {
    let mut it = text.split(',');
    let mut next = || it.next().map_or(Some(0.0), parse_f32);
    Some(Point3::new(next()?, next()?, next()?))
}

/// `float.Parse(..., Invariant)`.
#[must_use]
pub fn parse_f32(s: &str) -> Option<f32> {
    s.trim().parse::<f32>().ok()
}

/// Parses the body of `<...>` (without the angle brackets).
#[must_use]
pub fn parse_report(body: &str, version: &FirmwareVersion) -> StatusReport {
    if version.is_v11() {
        parse_v11(body)
    } else {
        parse_legacy(body)
    }
}

fn parse_v11(body: &str) -> StatusReport {
    let mut parts = body.split('|');
    let state = parts.next().unwrap_or("").to_owned();
    let mut report = StatusReport {
        state,
        fields: Vec::new(),
        complete: true,
    };
    for part in parts {
        let field = if let Some(v) = part.strip_prefix("Ov:") {
            let n: Vec<Option<i32>> = v.split(',').map(|x| x.trim().parse().ok()).collect();
            match n.as_slice() {
                [Some(a), Some(b), Some(c), ..] => Some(StatusField::Overrides(*a, *b, *c)),
                _ => None,
            }
        } else if let Some(v) = part.strip_prefix("Bf:") {
            let n: Vec<Option<i32>> = v.split(',').map(|x| x.trim().parse().ok()).collect();
            match n.as_slice() {
                [Some(a), Some(b), ..] => Some(StatusField::Buffer {
                    blocks: *a,
                    bytes: *b,
                }),
                _ => None,
            }
        } else if let Some(v) = part.strip_prefix("WPos:") {
            floats(v).map(StatusField::WorkPosition)
        } else if let Some(v) = part.strip_prefix("MPos:") {
            floats(v).map(StatusField::MachinePosition)
        } else if let Some(v) = part.strip_prefix("WCO:") {
            floats(v).map(StatusField::WorkOffset)
        } else if let Some(v) = part.strip_prefix("FS:") {
            let mut it = v.split(',');
            let f = it.next().map_or(Some(0.0), parse_f32);
            let s = it.next().map_or(Some(0.0), parse_f32);
            f.zip(s).map(|(f, s)| StatusField::FeedSpeed(f, s))
        } else if let Some(v) = part.strip_prefix("F:") {
            Some(StatusField::Feed(v.to_owned()))
        } else {
            continue; // unknown fields (Pn:, A:, Ln:...) are ignored
        };
        match field {
            Some(f) => report.fields.push(f),
            None => {
                report.complete = false;
                break;
            }
        }
    }
    report
}

/// `<Idle,MPos:0.000,0.000,0.000,WPos:0.000,0.000,0.000>` of Grbl 0.9.
fn parse_legacy(body: &str) -> StatusReport {
    let arr: Vec<&str> = body.split(',').collect();
    let mut report = StatusReport {
        state: arr[0].to_owned(),
        fields: Vec::new(),
        complete: true,
    };
    let point = |a: &str, b: &str, c: &str| -> Option<Point3> {
        Some(Point3::new(parse_f32(a.get(5..)?)?, parse_f32(b)?, parse_f32(c)?))
    };
    if arr.len() > 3 {
        match point(arr[1], arr[2], arr[3]) {
            Some(p) => report.fields.push(StatusField::MachinePosition(p)),
            None => {
                report.complete = false;
                return report;
            }
        }
    }
    if arr.len() > 6 {
        match point(arr[4], arr[5], arr[6]) {
            Some(p) => report.fields.push(StatusField::WorkPosition(p)),
            None => report.complete = false,
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_names() {
        assert_eq!(MachineStatus::parse("Hold:0"), Some(MachineStatus::Hold));
        assert_eq!(MachineStatus::parse("Sleep"), None);
        for s in [
            MachineStatus::Disconnected,
            MachineStatus::Connecting,
            MachineStatus::Idle,
            MachineStatus::Run,
            MachineStatus::Hold,
            MachineStatus::Door,
            MachineStatus::Home,
            MachineStatus::Alarm,
            MachineStatus::Check,
            MachineStatus::Jog,
            MachineStatus::Queue,
            MachineStatus::Cooling,
            MachineStatus::AutoHold,
            MachineStatus::Tool,
        ] {
            assert_eq!(MachineStatus::parse(&s.to_string()), Some(s));
        }
        assert!(MachineStatus::AutoHold.is_any_hold() && !MachineStatus::Door.is_any_hold());
    }

    #[test]
    fn parses_every_v11_field() {
        let v = FirmwareVersion::new(1, 1);
        let r = parse_report(
            "Run|MPos:1.000,2.000,3.000|Bf:15,128|FS:500,250|WCO:1,1,0|Ov:100,50,120|Pn:X",
            &v,
        );
        assert_eq!(r.state, "Run");
        assert!(r.complete);
        assert_eq!(
            r.fields,
            vec![
                StatusField::MachinePosition(Point3::new(1.0, 2.0, 3.0)),
                StatusField::Buffer {
                    blocks: 15,
                    bytes: 128
                },
                StatusField::FeedSpeed(500.0, 250.0),
                StatusField::WorkOffset(Point3::new(1.0, 1.0, 0.0)),
                StatusField::Overrides(100, 50, 120),
            ]
        );
        let r = parse_report("Idle|WPos:1,2|F:300,10", &v);
        assert_eq!(r.fields[0], StatusField::WorkPosition(Point3::new(1.0, 2.0, 0.0)));
        assert_eq!(r.fields[1], StatusField::Feed("300,10".into()));
    }

    #[test]
    fn malformed_field_stops_the_report() {
        let v = FirmwareVersion::new(1, 1);
        let r = parse_report("Idle|Ov:1,x,3|MPos:1,2,3", &v);
        assert!(!r.complete && r.fields.is_empty());
        assert!(!parse_report("Idle|Bf:1", &v).complete);
        assert!(!parse_report("Idle|FS:a,1", &v).complete);
        assert!(!parse_report("Idle|MPos:a", &v).complete);
    }

    #[test]
    fn legacy_reports() {
        let v = FirmwareVersion::new(0, 9);
        let r = parse_report("Idle,MPos:1.000,2.000,3.000,WPos:0.000,1.000,2.000", &v);
        assert_eq!(r.fields.len(), 2);
        assert_eq!(r.fields[1], StatusField::WorkPosition(Point3::new(0.0, 1.0, 2.0)));
        let r = parse_report("Idle", &v);
        assert!(r.fields.is_empty() && r.complete);
        assert!(!parse_report("Idle,MPos:x,1,2", &v).complete);
        assert!(!parse_report("Idle,MPos:1,1,2,WPos:a,b,c", &v).complete);
    }

    #[test]
    fn version_is_guessed_from_the_report_shape() {
        assert_eq!(
            report_version(None, "Idle|MPos:0,0,0"),
            FirmwareVersion::new(1, 1)
        );
        assert_eq!(
            report_version(None, "Idle|Pin:000"),
            FirmwareVersion::with_build(1, 0, 'c')
        );
        assert_eq!(
            report_version(None, "Idle,MPos:0,0,0"),
            FirmwareVersion::new(0, 9)
        );
        let known = FirmwareVersion::with_build(1, 1, 'f');
        assert_eq!(report_version(Some(&known), "x"), known);
    }

    #[test]
    fn point_math() {
        let a = Point3::new(1.0, 2.0, 3.0);
        assert_eq!(a + a - a, a);
        assert_eq!(Point3::default(), Point3::ZERO);
    }
}
