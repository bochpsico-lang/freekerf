//! Importer golden cases of `conformance` (`fixtures/importers/cases/*.json`, format
//! in `lasergrbl_harness/importers.py`): `freekerf import-case` runs one and prints
//! the same JSON as the harness's `run_case` does for LaserGRBL, so the goldens can
//! be compared directly (`LASERGRBL_HOST=rust`).
//!
//! A case names LaserGRBL settings (`Support Hardware PWM`, `Firmware Type`...);
//! they map to FreeKerf's planner options. Input files are read through the
//! caller's reader (relative names under the cases' `inputs` directory).

use freekerf_document::{Color, Document, FillSettings, Layer, LayerId, LayerMode, Source, SourceFormat};
use freekerf_gcode::report::{millis_to_seconds, num};
use freekerf_gcode::{AnalysisOptions, Command, Decimal, Program, TimingConfig};
use freekerf_geometry::hatch::FillPattern;
use freekerf_import::{Import, dxf, svg};
use freekerf_planner::generators::{self, CuttingTest, GreyscaleTest, ShakeTest};
use freekerf_planner::{Firmware, Job, MachineOptions, PlanOptions, VectorStyle, hershey};
use serde_json::{Value, json};

/// Case kinds this build runs.
pub const SUPPORTED_KINDS: &[&str] = &["generator", "hershey", "svg", "dxf", "svg_text"];

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

/// LaserGRBL's `ImageProcessor.Direction` names of hatch fillings; other values
/// (raster directions, `None`) give no filling.
fn fill_pattern(name: &str) -> Option<FillPattern> {
    Some(match name {
        "NewHorizontal" => FillPattern::Horizontal,
        "NewVertical" => FillPattern::Vertical,
        "NewDiagonal" => FillPattern::Diagonal,
        "NewReverseDiagonal" => FillPattern::ReverseDiagonal,
        "NewGrid" => FillPattern::Grid,
        "NewDiagonalGrid" => FillPattern::DiagonalGrid,
        "NewCross" => FillPattern::Cross,
        "NewDiagonalCross" => FillPattern::DiagonalCross,
        "NewSquares" => FillPattern::Squares,
        "NewZigZag" => FillPattern::ZigZag,
        "NewHilbert" => FillPattern::Hilbert,
        "NewInsetFilling" => FillPattern::Inset,
        _ => return None,
    })
}

fn layer_mode(name: &str) -> Result<LayerMode, CaseError> {
    Ok(match name {
        "Line" => LayerMode::Line,
        "Fill" => LayerMode::Fill,
        "FillAndLine" => LayerMode::FillAndLine,
        "Cut" => LayerMode::Cut,
        "Ignore" => LayerMode::Ignore,
        other => return Err(CaseError::Invalid(format!("unknown layer mode `{other}`"))),
    })
}

/// The layers of a case (`make_layers` of the harness): the listed ones, or every
/// scanned colour with `layer_defaults` over `LAYER_DEFAULTS`.
fn case_layers(case: &Value, import: &Import) -> Result<Vec<Layer>, CaseError> {
    let defaults = json!({"mode": "Line", "speed": 1000, "power": 1000, "passes": 1});
    let merge = |spec: &Value| {
        let mut m = defaults.as_object().cloned().unwrap_or_default();
        if let Some(d) = case.get("layer_defaults").and_then(Value::as_object) {
            m.extend(d.clone());
        }
        if let Some(o) = spec.as_object() {
            m.extend(o.clone());
        }
        Value::Object(m)
    };
    let specs: Vec<Value> = match case.get("layers").and_then(Value::as_array) {
        Some(list) => list.iter().map(merge).collect(),
        None => import
            .colors
            .iter()
            .map(|c| merge(&json!({"color": c.color.to_string()})))
            .collect(),
    };
    specs
        .iter()
        .map(|s| {
            let color: Color = text(s, "color")?
                .parse()
                .map_err(|_| CaseError::Invalid("layer colour".into()))?;
            let mut l = Layer::new(
                LayerId(0),
                color,
                as_u32(&s["speed"]).unwrap_or(0),
                as_u32(&s["power"]).unwrap_or(0),
            );
            l.mode = layer_mode(&text(s, "mode")?)?;
            l.passes = as_u32(&s["passes"]).unwrap_or(1);
            let mut fill = FillSettings {
                pattern: FillPattern::Horizontal,
                lines_per_mm: s.get("lines_per_mm").and_then(Value::as_f64).unwrap_or(10.0),
            };
            if let Some(f) = s.get("fill").and_then(Value::as_str) {
                match fill_pattern(f) {
                    Some(p) => fill.pattern = p,
                    None => fill.lines_per_mm = 0.0, // a raster direction: no hatch lines
                }
            }
            l.fill = fill;
            Ok(l)
        })
        .collect()
}

fn scan_json(import: &Import) -> Value {
    Value::Array(
        import
            .colors
            .iter()
            .map(|c| json!({"color": c.color.to_string(), "count": c.count}))
            .collect(),
    )
}

fn input<'a>(case: &Value, read: &Reader<'a>) -> Result<Vec<u8>, CaseError> {
    let name = match (
        case.get("input").and_then(Value::as_str),
        case.get("resource").and_then(Value::as_str),
    ) {
        (Some(i), _) => i.to_owned(),
        // an svg embedded in LaserGRBL.exe: a file of inputs/resources/ named by the resource
        (None, Some(r)) => format!("resources/{r}"),
        (None, None) => return Err(CaseError::Invalid("missing `input`".into())),
    };
    read(&name).map_err(|e| CaseError::Input(name, e))
}

fn planned(
    doc: &Document,
    settings: &CaseSettings,
    style: VectorStyle,
    preamble: bool,
) -> Result<Job, CaseError> {
    freekerf_planner::plan(
        doc,
        &PlanOptions {
            machine: settings.machine.clone(),
            vector_style: Some(style),
            layer_preamble: preamble,
        },
    )
    .map_err(|e| CaseError::Invalid(e.to_string()))
}

/// Runs a case. `read` gives the bytes of an input file.
///
/// # Errors
/// Malformed or unsupported cases, unreadable inputs.
pub fn run(case: &Value, read: &Reader<'_>) -> Result<Value, CaseError> {
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
        "svg" | "dxf" => {
            let bytes = input(case, read)?;
            let (import, format, style) = if kind == "svg" {
                let opts = svg::SvgOptions {
                    legacy_bezier: !settings.smart_bezier,
                    ..svg::SvgOptions::default()
                };
                let i = svg::import(&bytes, &opts).map_err(|e| CaseError::Invalid(e.to_string()))?;
                (i, SourceFormat::Svg, VectorStyle::Svg)
            } else {
                let tolerance = case
                    .get("spline_tolerance")
                    .and_then(Value::as_f64)
                    .unwrap_or(settings.spline_tolerance);
                let opts = dxf::DxfOptions {
                    spline_tolerance: tolerance,
                };
                let i = dxf::import(&bytes, &opts).map_err(|e| CaseError::Invalid(e.to_string()))?;
                (i, SourceFormat::Dxf, VectorStyle::Dxf)
            };
            let scan = scan_json(&import);
            let layers = case_layers(case, &import)?;
            let source = Source {
                format,
                file: String::new(),
            };
            let doc = import.into_document(layers, Some(&source));
            let job = planned(&doc, &settings, style, true)?;
            let mut out = file_result(&job.program(&settings.analysis()));
            out["scan"] = scan;
            Ok(out)
        }
        "svg_text" => {
            // the conversion half of LaserGRBL's centerline: every element, no layers,
            // scaled to `max_size`, moved by `offset`
            let bytes = input(case, read)?;
            #[allow(clippy::cast_possible_truncation)]
            let offset = case
                .get("offset")
                .and_then(Value::as_array)
                .map_or((0.0, 0.0), |a| {
                    (
                        a[0].as_f64().unwrap_or(0.0) as f32,
                        a[1].as_f64().unwrap_or(0.0) as f32,
                    )
                });
            #[allow(clippy::cast_possible_truncation)]
            let opts = svg::SvgOptions {
                scale_to_mm: Some(float(case, "max_size")? as f32),
                user_offset: offset,
                legacy_bezier: !settings.smart_bezier,
            };
            let import = svg::import(&bytes, &opts).map_err(|e| CaseError::Invalid(e.to_string()))?;
            let doc = import.into_single_layer(Some(&Source {
                format: SourceFormat::Svg,
                file: String::new(),
            }));
            let job = planned(&doc, &settings, VectorStyle::Svg, false)?;
            // LaserGRBL returns the converter text: lines joined with CRLF, split on LF
            let mut lines = job.lines;
            lines.push(String::new());
            Ok(json!({ "converter_output": lines }))
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
    fn layer_specs_and_inputs() {
        for (name, p) in [
            ("NewHorizontal", FillPattern::Horizontal),
            ("NewVertical", FillPattern::Vertical),
            ("NewDiagonal", FillPattern::Diagonal),
            ("NewReverseDiagonal", FillPattern::ReverseDiagonal),
            ("NewGrid", FillPattern::Grid),
            ("NewDiagonalGrid", FillPattern::DiagonalGrid),
            ("NewCross", FillPattern::Cross),
            ("NewDiagonalCross", FillPattern::DiagonalCross),
            ("NewSquares", FillPattern::Squares),
            ("NewZigZag", FillPattern::ZigZag),
            ("NewHilbert", FillPattern::Hilbert),
            ("NewInsetFilling", FillPattern::Inset),
        ] {
            assert_eq!(fill_pattern(name), Some(p));
        }
        assert_eq!(fill_pattern("Horizontal"), None);
        for m in ["Line", "Fill", "FillAndLine", "Cut", "Ignore"] {
            assert!(layer_mode(m).is_ok());
        }
        assert!(layer_mode("x").is_err());
        let import = Import::default();
        let layers = case_layers(
            &json!({"layers": [{"color": "#FF0000", "mode": "Cut", "fill": "Vertical", "passes": 3}],
                    "layer_defaults": {"speed": 77}}),
            &import,
        )
        .unwrap();
        assert_eq!(
            (layers[0].speed_mm_min, layers[0].passes, layers[0].mode),
            (77, 3, LayerMode::Cut)
        );
        assert_eq!(layers[0].fill.lines_per_mm, 0.0);
        assert!(case_layers(&json!({"layers": [{"color": "red"}]}), &import).is_err());
        assert!(
            case_layers(
                &json!({"layers": [{"color": "#000000", "mode": "Nope"}]}),
                &import
            )
            .is_err()
        );
        // a resource is a file of inputs/resources/
        let seen = std::cell::RefCell::new(String::new());
        let read = |n: &str| {
            *seen.borrow_mut() = n.to_owned();
            Err("missing".to_owned())
        };
        let e = run(&json!({"kind": "svg", "resource": "LaserGRBL.X.svg"}), &read).unwrap_err();
        assert_eq!(*seen.borrow(), "resources/LaserGRBL.X.svg");
        assert!(matches!(e, CaseError::Input(..)));
        assert!(run(&json!({"kind": "dxf"}), &no_files).is_err());
        assert!(
            run(
                &json!({"kind": "svg_text", "input": "a.svg", "max_size": 1.0}),
                &no_files
            )
            .is_err()
        );
        let bad_svg = |_: &str| Ok(b"<svg".to_vec());
        assert!(run(&json!({"kind": "svg", "input": "a.svg"}), &bad_svg).is_err());
        assert!(run(&json!({"kind": "dxf", "input": "a.dxf"}), &bad_svg).is_err());
        assert!(
            run(
                &json!({"kind": "svg_text", "input": "a.svg", "max_size": 1.0}),
                &bad_svg
            )
            .is_err()
        );
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
