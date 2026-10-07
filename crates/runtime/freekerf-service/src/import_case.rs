//! Importer golden cases of `conformance` (`fixtures/importers/cases/*.json`, format
//! in `lasergrbl_harness/importers.py`): `freekerf import-case` runs one and prints
//! the same JSON as the harness's `run_case` does for LaserGRBL, so the goldens can
//! be compared directly (`LASERGRBL_HOST=rust`).
//!
//! A case names LaserGRBL settings (`Support Hardware PWM`, `Firmware Type`...);
//! they map to FreeKerf's planner options. Input files are read through the
//! caller's reader (relative names under the cases' `inputs` directory).

use freekerf_gcode::report::{millis_to_seconds, num};
use freekerf_gcode::{AnalysisOptions, Command, Decimal, Program, TimingConfig};
use freekerf_planner::generators::{self, CuttingTest, GreyscaleTest, ShakeTest};
use freekerf_planner::{Firmware, MachineOptions, hershey};
use serde_json::{Value, json};

/// Case kinds this build runs.
pub const SUPPORTED_KINDS: &[&str] = &["generator", "hershey"];

/// Error of a case: malformed, unsupported or failing to read its input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CaseError {
    /// The case is not valid.
    #[error("invalid case: {0}")]
    Invalid(String),
    /// The case kind is not implemented.
    #[error("unsupported case kind `{0}`")]
    Unsupported(String),
    /// An input file could not be read.
    #[error("input `{0}`: {1}")]
    Input(String, String),
}

/// Reads an input file by its name relative to the cases' `inputs` directory.
pub type Reader<'a> = dyn Fn(&str) -> Result<Vec<u8>, String> + 'a;

/// LaserGRBL settings a case may set (defaults are LaserGRBL's).
#[derive(Debug, Clone, PartialEq)]
pub struct CaseSettings {
    /// Planner options.
    pub machine: MachineOptions,
    /// `Vector.UseSmartBezier`.
    pub smart_bezier: bool,
    /// `Vector.SplineArcTolerance`, mm.
    pub spline_tolerance: f64,
    /// `GrayScaleConversion.Gcode.Offset.X/Y`, mm.
    pub offset: (f64, f64),
}

impl CaseSettings {
    /// Settings from the case's `settings` object.
    ///
    /// # Errors
    /// Unknown firmware or a value of the wrong type.
    pub fn from_case(case: &Value) -> Result<Self, CaseError> {
        let mut s = Self {
            machine: MachineOptions::default(),
            smart_bezier: true,
            spline_tolerance: 0.01,
            offset: (0.0, 0.0),
        };
        let Some(map) = case.get("settings").and_then(Value::as_object) else {
            return Ok(s);
        };
        let bad = |k: &str| CaseError::Invalid(format!("setting `{k}`"));
        for (k, v) in map {
            match k.as_str() {
                "Firmware Type" => {
                    s.machine.firmware = match v.as_str() {
                        Some("Grbl") => Firmware::Grbl,
                        Some("Smoothie") => Firmware::Smoothie,
                        Some("Marlin") => Firmware::Marlin,
                        Some("VigoWork") => Firmware::VigoWork,
                        _ => return Err(bad(k)),
                    };
                }
                "Support Hardware PWM" => s.machine.support_pwm = v.as_bool().ok_or_else(|| bad(k))?,
                "Disable G0 fast skip" => s.machine.disable_g0 = v.as_bool().ok_or_else(|| bad(k))?,
                "Unidirectional Engraving" => s.machine.unidirectional = v.as_bool().ok_or_else(|| bad(k))?,
                "GrayScaleConversion.VectorizeOptions.BorderSpeed" => {
                    s.machine.default_border_speed = as_u32(v).ok_or_else(|| bad(k))?;
                }
                "GrayScaleConversion.Gcode.LaserOptions.PowerMax" => {
                    s.machine.default_power_max = as_u32(v).ok_or_else(|| bad(k))?;
                }
                "GrayScaleConversion.Gcode.LaserOptions.LaserOn" => {
                    s.machine.laser_on = v.as_str().ok_or_else(|| bad(k))?.to_owned();
                }
                "GrayScaleConversion.Gcode.LaserOptions.LaserOff" => {
                    s.machine.laser_off = v.as_str().ok_or_else(|| bad(k))?.to_owned();
                }
                "Vector.UseSmartBezier" => s.smart_bezier = v.as_bool().ok_or_else(|| bad(k))?,
                "Vector.SplineArcTolerance" => s.spline_tolerance = v.as_f64().ok_or_else(|| bad(k))?,
                "GrayScaleConversion.Gcode.Offset.X" => s.offset.0 = v.as_f64().ok_or_else(|| bad(k))?,
                "GrayScaleConversion.Gcode.Offset.Y" => s.offset.1 = v.as_f64().ok_or_else(|| bad(k))?,
                _ => return Err(CaseError::Invalid(format!("unknown setting `{k}`"))),
            }
        }
        Ok(s)
    }

    /// Options of the analysis (`summary`): the golden machine configuration.
    #[must_use]
    pub fn analysis(&self) -> AnalysisOptions {
        AnalysisOptions {
            timing: TimingConfig {
                max_rate_x_mm_min: Decimal::from(crate::analysis::GOLDEN_MAX_RATE),
            },
            support_pwm: self.machine.support_pwm,
        }
    }
}

fn as_u32(v: &Value) -> Option<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok())
}

fn field<'a>(case: &'a Value, name: &str) -> Result<&'a Value, CaseError> {
    case.get(name)
        .ok_or_else(|| CaseError::Invalid(format!("missing `{name}`")))
}

fn int(v: &Value, name: &str) -> Result<i32, CaseError> {
    v.get(name)
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .ok_or_else(|| CaseError::Invalid(format!("`{name}` must be an integer")))
}

fn text(v: &Value, name: &str) -> Result<String, CaseError> {
    v.get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| CaseError::Invalid(format!("`{name}` must be a string")))
}

fn float(v: &Value, name: &str) -> Result<f64, CaseError> {
    v.get(name)
        .and_then(Value::as_f64)
        .ok_or_else(|| CaseError::Invalid(format!("`{name}` must be a number")))
}

/// `file_result` of the harness: command texts and the summary of the analysis.
#[must_use]
pub fn file_result(program: &Program) -> Value {
    let range = program.range();
    let to_box = |b: Option<[Decimal; 4]>| b.map(|v| v.map(num));
    json!({
        "gcode": program.commands().map(|c| c.text().to_owned()).collect::<Vec<_>>(),
        "summary": {
            "count": program.len(),
            "estimated_time_s": millis_to_seconds(program.estimated_ms()),
            "drawing_range": to_box(range.drawing.as_box()),
            "moving_range": to_box(range.moving.as_box()),
        }
    })
}

/// A program from commands added one by one (`list.Add(new GrblCommand(...))`).
fn program_of(lines: &[String], settings: &CaseSettings) -> Program {
    let mut p = Program::new();
    p.extend(lines.iter().map(|l| Command::new(l)));
    p.analyze(&settings.analysis());
    p
}

/// Runs a case. `read` gives the bytes of an input file.
///
/// # Errors
/// Malformed or unsupported cases, unreadable inputs.
pub fn run(case: &Value, read: &Reader<'_>) -> Result<Value, CaseError> {
    let _ = read;
    let settings = CaseSettings::from_case(case)?;
    let kind = case.get("kind").and_then(Value::as_str).unwrap_or_default();
    match kind {
        "hershey" => {
            let lines = hershey::text(
                &text(case, "text")?,
                float(case, "x")?,
                float(case, "y")?,
                i64::from(int(case, "speed")?),
                i64::from(int(case, "power")?),
                field(case, "horizontal")?.as_bool().unwrap_or(false),
                &text(case, "ton")?,
            );
            Ok(
                json!({ "gcode": lines.iter().map(|l| Command::new(l).text().to_owned()).collect::<Vec<_>>() }),
            )
        }
        "generator" => {
            let args = field(case, "args")?;
            let lines = match field(case, "generator")?.as_str() {
                Some("cutting") => generators::cutting_test(&CuttingTest {
                    f_col: int(args, "f_col")?,
                    f_start: int(args, "f_start")?,
                    f_end: int(args, "f_end")?,
                    p_start: int(args, "p_start")?,
                    p_end: int(args, "p_end")?,
                    s_fixed: int(args, "s_fixed")?,
                    f_text: int(args, "f_text")?,
                    s_text: int(args, "s_text")?,
                    title: text(args, "title")?,
                    ton: text(args, "ton")?,
                }),
                Some("greyscale") => generators::greyscale_test(&GreyscaleTest {
                    f_row: int(args, "f_row")?,
                    s_col: int(args, "s_col")?,
                    f_start: int(args, "f_start")?,
                    f_end: int(args, "f_end")?,
                    s_start: int(args, "s_start")?,
                    s_end: int(args, "s_end")?,
                    x_size: int(args, "x_size")?,
                    y_size: int(args, "y_size")?,
                    resolution: float(args, "resolution")?,
                    f_grid: int(args, "f_grid")?,
                    s_grid: int(args, "s_grid")?,
                    title: text(args, "title")?,
                    f_text: int(args, "f_text")?,
                    s_text: int(args, "s_text")?,
                    ton: text(args, "ton")?,
                }),
                Some("shake") => generators::shake_test(&ShakeTest {
                    axis: text(args, "axis")?,
                    flimit: int(args, "flimit")?,
                    axislen: int(args, "axislen")?,
                    cpower: int(args, "cpower")?,
                    cspeed: int(args, "cspeed")?,
                }),
                other => return Err(CaseError::Invalid(format!("unknown generator {other:?}"))),
            };
            Ok(file_result(&program_of(&lines, &settings)))
        }
        other => Err(CaseError::Unsupported(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_files(name: &str) -> Result<Vec<u8>, String> {
        Err(format!("no file {name}"))
    }

    #[test]
    fn hershey_and_generators() {
        let h = run(
            &json!({"kind": "hershey", "text": "a", "x": 0.0, "y": 0.0, "speed": 1000, "power": 500,
                    "horizontal": true, "ton": "M4"}),
            &no_files,
        )
        .unwrap();
        assert_eq!(h["gcode"][0], "M5 S500 F1000");
        let g = run(
            &json!({"kind": "generator", "generator": "shake",
                    "args": {"axis": "X", "flimit": 3000, "axislen": 60, "cpower": 100, "cspeed": 500}}),
            &no_files,
        )
        .unwrap();
        assert_eq!(g["gcode"][0], "M5");
        assert!(g["summary"]["count"].as_u64().unwrap() > 10);
        assert!(g["summary"]["estimated_time_s"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn settings_and_errors() {
        let s = CaseSettings::from_case(&json!({"settings": {
            "Firmware Type": "Smoothie", "Support Hardware PWM": false, "Disable G0 fast skip": true,
            "Unidirectional Engraving": true, "GrayScaleConversion.VectorizeOptions.BorderSpeed": 300,
            "GrayScaleConversion.Gcode.LaserOptions.PowerMax": 900,
            "GrayScaleConversion.Gcode.LaserOptions.LaserOn": "M4",
            "GrayScaleConversion.Gcode.LaserOptions.LaserOff": "M5",
            "Vector.UseSmartBezier": false, "Vector.SplineArcTolerance": 0.5,
            "GrayScaleConversion.Gcode.Offset.X": 1.5, "GrayScaleConversion.Gcode.Offset.Y": -2.0}}))
        .unwrap();
        assert_eq!(s.machine.firmware, Firmware::Smoothie);
        assert!(!s.machine.support_pwm && s.machine.disable_g0 && s.machine.unidirectional);
        assert_eq!(
            (s.machine.default_border_speed, s.machine.default_power_max),
            (300, 900)
        );
        assert_eq!(s.offset, (1.5, -2.0));
        assert!(!s.analysis().support_pwm);
        for bad in [
            json!({"settings": {"Firmware Type": "X"}}),
            json!({"settings": {"Nope": 1}}),
            json!({"settings": {"Support Hardware PWM": 1}}),
        ] {
            assert!(CaseSettings::from_case(&bad).is_err(), "{bad}");
        }
        assert_eq!(
            run(&json!({"kind": "svg_unknown"}), &no_files),
            Err(CaseError::Unsupported("svg_unknown".into()))
        );
        assert!(run(&json!({"kind": "hershey"}), &no_files).is_err());
        assert!(
            run(
                &json!({"kind": "generator", "generator": "x", "args": {}}),
                &no_files
            )
            .is_err()
        );
        assert!(no_files("a").is_err());
    }
}
