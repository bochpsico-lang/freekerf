//! `LaserGRBL.Settings.bin`: LaserGRBL keeps its settings as a
//! `Dictionary<string, object>` serialized with `BinaryFormatter`
//! (`Settings.InternalSaveFile`). The values FreeKerf has an equivalent for are
//! applied to [`freekerf_settings::Settings`]; every other key is reported.

use std::collections::BTreeMap;

use freekerf_settings::{Firmware, Settings, Streaming};

use crate::MigrateError;
use crate::nrbf::{self, Value};

/// The dictionary read from the file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LaserGrblSettings {
    /// Every key with its value.
    pub values: BTreeMap<String, Value>,
}

/// What [`LaserGrblSettings::apply`] did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    /// LaserGRBL key → FreeKerf setting key, for every value applied.
    pub applied: Vec<(String, String)>,
    /// LaserGRBL keys not applied, with the reason.
    pub skipped: Vec<(String, String)>,
}

/// Reads `LaserGRBL.Settings.bin`.
///
/// # Errors
/// Not a `BinaryFormatter` stream, or not a dictionary of strings.
pub fn read(bytes: &[u8]) -> Result<LaserGrblSettings, MigrateError> {
    let root = nrbf::read(bytes)?;
    if !root
        .class()
        .is_some_and(|c| c.starts_with("System.Collections.Generic.Dictionary`2"))
    {
        return Err(MigrateError::Content(format!(
            "not a dictionary: {:?}",
            root.class()
        )));
    }
    let mut values = BTreeMap::new();
    for pair in root
        .member("KeyValuePairs")
        .and_then(Value::as_array)
        .unwrap_or_default()
    {
        let key = pair
            .member("key")
            .and_then(Value::as_str)
            .ok_or_else(|| MigrateError::Content("a key that is not a string".into()))?;
        values.insert(
            key.to_owned(),
            pair.member("value").cloned().unwrap_or(Value::Null),
        );
    }
    Ok(LaserGrblSettings { values })
}

/// LaserGRBL's custom code uses CRLF line ends; FreeKerf keeps LF.
fn text(v: &Value) -> Option<String> {
    v.as_str().map(|s| s.replace("\r\n", "\n"))
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn seconds(v: &Value) -> Option<u64> {
    match v {
        Value::TimeSpan(t) if *t >= 0 => Some((*t / 10_000_000) as u64),
        _ => None,
    }
}

impl LaserGrblSettings {
    /// Applies the values with a FreeKerf equivalent to `settings`. Values of an
    /// unexpected type are skipped (LaserGRBL ignores them too: `GetObject` returns
    /// the default when the stored type differs).
    #[allow(
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub fn apply(&self, settings: &mut Settings) -> Report {
        let mut report = Report::default();
        for (key, value) in &self.values {
            let m = &mut settings.machine;
            let target: Option<&str> = match key.as_str() {
                "Firmware Type" => match value.as_i64() {
                    Some(0) => {
                        m.firmware = Firmware::Grbl;
                        Some("machine.firmware")
                    }
                    Some(1) => {
                        m.firmware = Firmware::Smoothie;
                        Some("machine.firmware")
                    }
                    Some(2) => {
                        m.firmware = Firmware::Marlin;
                        Some("machine.firmware")
                    }
                    Some(3) => {
                        m.firmware = Firmware::VigoWork;
                        Some("machine.firmware")
                    }
                    _ => None,
                },
                "Serial Speed" => value.as_i64().and_then(|v| u32::try_from(v).ok()).map(|v| {
                    m.baud = v;
                    "machine.baud"
                }),
                "Streaming Mode" => match value.as_i64() {
                    Some(0) => {
                        m.streaming_mode = Streaming::Buffered;
                        Some("machine.streaming_mode")
                    }
                    Some(1) => {
                        m.streaming_mode = Streaming::Synchronous;
                        Some("machine.streaming_mode")
                    }
                    Some(2) => {
                        m.streaming_mode = Streaming::RepeatOnError;
                        Some("machine.streaming_mode")
                    }
                    _ => None,
                },
                "Reset Grbl On Connect" => value.as_bool().map(|v| {
                    m.reset_on_connect = v;
                    "machine.reset_on_connect"
                }),
                "Query MachineInfo ($I) at connect" => value.as_bool().map(|v| {
                    m.query_machine_info = v;
                    "machine.query_machine_info"
                }),
                "Support Hardware PWM" => value.as_bool().map(|v| {
                    m.support_pwm = v;
                    "machine.support_pwm"
                }),
                "Enable Continuous Jog" => value.as_bool().map(|v| {
                    m.continuous_jog = v;
                    "machine.continuous_jog"
                }),
                "AutoCooling" => value.as_bool().map(|v| {
                    m.auto_cooling.enabled = v;
                    "machine.auto_cooling.enabled"
                }),
                "AutoCooling TOn" => seconds(value).map(|v| {
                    m.auto_cooling.on_s = v;
                    "machine.auto_cooling.on_s"
                }),
                "AutoCooling TOff" => seconds(value).map(|v| {
                    m.auto_cooling.off_s = v;
                    "machine.auto_cooling.off_s"
                }),
                "GCode.CustomHeader" => text(value).map(|v| {
                    settings.job.header = v;
                    "job.header"
                }),
                "GCode.CustomFooter" => text(value).map(|v| {
                    settings.job.footer = v;
                    "job.footer"
                }),
                "GCode.CustomPasses" => text(value).map(|v| {
                    settings.job.passes = v;
                    "job.passes"
                }),
                "Jog Step" => match value {
                    Value::Decimal(_) => value.as_f64().map(|v| {
                        settings.jog.step_mm = v;
                        "jog.step_mm"
                    }),
                    _ => None,
                },
                "Jog Speed" => match value {
                    Value::Int(v) => {
                        #[allow(clippy::cast_precision_loss)]
                        let v = *v as f64;
                        settings.jog.speed_mm_min = v;
                        Some("jog.speed_mm_min")
                    }
                    _ => None,
                },
                _ => {
                    report
                        .skipped
                        .push((key.clone(), "no FreeKerf equivalent".into()));
                    continue;
                }
            };
            match target {
                Some(t) => report.applied.push((key.clone(), t.to_owned())),
                None => report.skipped.push((key.clone(), "unexpected value type".into())),
            }
        }
        report
    }
}
