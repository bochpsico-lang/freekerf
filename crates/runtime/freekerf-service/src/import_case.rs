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
use freekerf_raster::centerline::CenterlineOptions;
use freekerf_raster::dither::DitherMode;
use freekerf_raster::image::{Argb, Image, argb};
use freekerf_raster::l2l::{Direction, L2lConf, L2lOptions, load_image_l2l};
use freekerf_raster::ops::{self, Formula, Interpolation};
use freekerf_raster::potrace::TraceOptions;

use crate::autotrace;
use freekerf_raster::vectorize::{Filling, PotraceOptions, load_image_potrace};
use serde_json::{Value, json};

/// Case kinds this build runs.
pub const SUPPORTED_KINDS: &[&str] = &[
    "generator",
    "hershey",
    "svg",
    "dxf",
    "svg_text",
    "centerline",
    "raster",
    "potrace",
    "image_op",
];

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

/// The harness' autotrace double: prints a recorded SVG.
struct Recorded(String);

impl autotrace::Tracer for Recorded {
    fn trace(&self, _png: &[u8], _args: &[String]) -> Result<String, autotrace::CenterlineError> {
        Ok(self.0.clone())
    }
}

/// A pixel token of the harness: 2 (grey), 6 (RGB) or 8 (ARGB) hex digits.
fn pixel_token(tok: &str) -> Result<Argb, CaseError> {
    let v =
        u32::from_str_radix(tok, 16).map_err(|_| CaseError::Invalid(format!("bad pixel token `{tok}`")))?;
    #[allow(clippy::cast_possible_truncation)]
    Ok(match tok.len() {
        2 => argb(255, v as u8, v as u8, v as u8),
        6 => 0xFF00_0000 | v,
        8 => v,
        _ => return Err(CaseError::Invalid(format!("bad pixel token `{tok}`"))),
    })
}

/// `parse_pixels` of the harness: `{"pixels": ["RRGGBB ...", ...]}` or
/// `{"ascii": ["..##..", ...]}` (`#` black, `.` white).
fn case_image(v: &Value) -> Result<Image, CaseError> {
    let rows: Vec<Vec<Argb>> = if let Some(ascii) = v.get("ascii").and_then(Value::as_array) {
        ascii
            .iter()
            .map(|r| {
                r.as_str()
                    .unwrap_or("")
                    .chars()
                    .map(|c| {
                        if c == '#' {
                            Ok(0xFF00_0000)
                        } else {
                            Ok(0xFFFF_FFFF)
                        }
                    })
                    .collect()
            })
            .collect::<Result<_, CaseError>>()?
    } else {
        v.get("pixels")
            .and_then(Value::as_array)
            .ok_or_else(|| CaseError::Invalid("image without pixels".into()))?
            .iter()
            .map(|r| {
                r.as_str()
                    .unwrap_or("")
                    .split_whitespace()
                    .map(pixel_token)
                    .collect()
            })
            .collect::<Result<_, CaseError>>()?
    };
    let width = rows.first().map_or(0, Vec::len);
    if rows.iter().any(|r| r.len() != width) {
        return Err(CaseError::Invalid("rows of different length".into()));
    }
    let height = rows.len();
    Image::from_pixels(
        u32::try_from(width).unwrap_or(0),
        u32::try_from(height).unwrap_or(0),
        rows.concat(),
    )
    .map_err(|e| CaseError::Invalid(e.to_string()))
}

/// `dump_bitmap` of the harness: rows of `AARRGGBB` tokens.
fn dump_image(img: &Image) -> Value {
    let w = img.width() as usize;
    Value::Array(
        img.pixels()
            .chunks(w.max(1))
            .map(|row| {
                Value::from(
                    row.iter()
                        .map(|p| format!("{p:08X}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            })
            .collect(),
    )
}

/// `GrblFile.L2LConf` of a case: `L2L_DEFAULTS` of the harness, then the case's `conf`.
#[allow(clippy::cast_possible_truncation)]
fn l2l_conf(case: &Value) -> Result<L2lConf, CaseError> {
    let mut c = L2lConf::default();
    let Some(conf) = case.get("conf").and_then(Value::as_object) else {
        return Ok(c);
    };
    let bad = |k: &str| CaseError::Invalid(format!("conf `{k}`"));
    for (k, v) in conf {
        let f = || v.as_f64().ok_or_else(|| bad(k));
        let i = || {
            v.as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or_else(|| bad(k))
        };
        match k.as_str() {
            "res" => c.res = f()?,
            "fres" => c.fres = f()?,
            "oX" => c.offset_x = f()? as f32,
            "oY" => c.offset_y = f()? as f32,
            "markSpeed" => c.mark_speed = i()?,
            "borderSpeed" => c.border_speed = i()?,
            "minPower" => c.min_power = i()?,
            "maxPower" => c.max_power = i()?,
            "lOn" => c.laser_on = v.as_str().ok_or_else(|| bad(k))?.to_owned(),
            "lOff" => c.laser_off = v.as_str().ok_or_else(|| bad(k))?.to_owned(),
            "pwm" => c.pwm = v.as_bool().ok_or_else(|| bad(k))?,
            "vectorfilling" => c.vector_filling = v.as_bool().ok_or_else(|| bad(k))?,
            "firmwareType" => c.smoothie = v.as_str() == Some("Smoothie"),
            "dir" => {
                c.direction = match v.as_str() {
                    Some("Horizontal") => Direction::Horizontal,
                    Some("Vertical") => Direction::Vertical,
                    Some("Diagonal") => Direction::Diagonal,
                    _ => return Err(bad(k)),
                }
            }
            _ => return Err(CaseError::Invalid(format!("unknown conf `{k}`"))),
        }
    }
    Ok(c)
}

/// The vectorize settings of a `potrace` case and its filling (`conf.dir`).
fn potrace_options(case: &Value) -> Result<(PotraceOptions, Value), CaseError> {
    let po = case.get("potrace").cloned().unwrap_or_else(|| json!({}));
    let opt = |k: &str| po.get(k).and_then(Value::as_f64);
    #[allow(clippy::cast_possible_truncation)]
    let trace = TraceOptions {
        turdsize: opt("spot_removal").map_or(2, |v| v as i32),
        alphamax: opt("smoothing").unwrap_or(0.0),
        curve_optimizing: opt("optimize").is_some(),
        opttolerance: opt("optimize").unwrap_or(0.2),
    };
    let mut rest = case.clone();
    let dir = rest
        .get_mut("conf")
        .and_then(Value::as_object_mut)
        .and_then(|c| c.remove("dir"))
        .unwrap_or_else(|| json!("Horizontal"));
    let filling = match dir.as_str().unwrap_or_default() {
        "None" => Filling::None,
        "Horizontal" => Filling::Raster(Direction::Horizontal),
        "Vertical" => Filling::Raster(Direction::Vertical),
        "Diagonal" => Filling::Raster(Direction::Diagonal),
        other => Filling::Vector(
            fill_pattern(other).ok_or_else(|| CaseError::Invalid(format!("direction `{other}`")))?,
        ),
    };
    Ok((
        PotraceOptions {
            trace,
            optimize_fast: po.get("optimize_fast").and_then(Value::as_bool).unwrap_or(false),
            filling,
        },
        rest,
    ))
}

fn image_op(case: &Value) -> Result<Image, CaseError> {
    let img = case_image(field(case, "image")?)?;
    let args = case.get("args").cloned().unwrap_or_else(|| json!({}));
    #[allow(clippy::cast_possible_truncation)]
    let f32_arg = |k: &str, d: f64| args.get(k).and_then(Value::as_f64).unwrap_or(d) as f32;
    let i32_arg = |k: &str| int(&args, k);
    let bool_arg = |k: &str, d: bool| args.get(k).and_then(Value::as_bool).unwrap_or(d);
    let at = |k: &str| -> Result<(u32, u32), CaseError> {
        let a = args
            .get(k)
            .and_then(Value::as_array)
            .ok_or_else(|| CaseError::Invalid(format!("`{k}`")))?;
        let n = |i: usize| {
            a.get(i)
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0)
        };
        Ok((n(0), n(1)))
    };
    Ok(match field(case, "op")?.as_str().unwrap_or_default() {
        "grayscale" => {
            let formula = match args
                .get("formula")
                .and_then(Value::as_str)
                .unwrap_or("SimpleAverage")
            {
                "SimpleAverage" => Formula::SimpleAverage,
                "WeightAverage" => Formula::WeightAverage,
                "OpticalCorrect" => Formula::OpticalCorrect,
                "Custom" => Formula::Custom,
                other => return Err(CaseError::Invalid(format!("formula `{other}`"))),
            };
            ops::grayscale(
                &img,
                f32_arg("r", 1.0),
                f32_arg("g", 1.0),
                f32_arg("b", 1.0),
                f32_arg("brightness", 0.0),
                f32_arg("contrast", 1.0),
                formula,
            )
        }
        "threshold" => ops::threshold(&img, f32_arg("threshold", 0.0), bool_arg("apply", true)),
        "invert" => ops::invert(&img),
        "dither" => {
            let mode = match args.get("mode").and_then(Value::as_str).unwrap_or("") {
                "Atkinson" => DitherMode::Atkinson,
                "FloydSteinberg" => DitherMode::FloydSteinberg,
                "Burks" => DitherMode::Burks,
                "Jarvis" => DitherMode::Jarvis,
                "Random" => DitherMode::Random,
                "Sierra2" => DitherMode::Sierra2,
                "Sierra3" => DitherMode::Sierra3,
                "SierraLight" => DitherMode::SierraLight,
                "Stucki" => DitherMode::Stucki,
                other => return Err(CaseError::Invalid(format!("dither `{other}`"))),
            };
            freekerf_raster::dither::dither(&img, mode, 0)
        }
        "whitenize" => ops::whitenize(&img, i32_arg("threshold")?, bool_arg("demo", false)),
        "resize" => {
            let filter = match args
                .get("interpolation")
                .and_then(Value::as_str)
                .unwrap_or("NearestNeighbor")
            {
                "NearestNeighbor" => Interpolation::NearestNeighbor,
                "Bilinear" | "HighQualityBilinear" | "Low" => Interpolation::Bilinear,
                _ => Interpolation::Bicubic,
            };
            let dim =
                |k: &str| i32_arg(k).and_then(|v| u32::try_from(v).map_err(|_| CaseError::Invalid(k.into())));
            ops::resize(
                &img,
                dim("width")?,
                dim("height")?,
                bool_arg("killalfa", true),
                filter,
            )
        }
        "fill" => {
            let color = pixel_token(args.get("color").and_then(Value::as_str).unwrap_or(""))?;
            ops::fill(&img, at("at")?, color, i32_arg("tolerance")?).0
        }
        "outline" => ops::outline(&img, at("at")?),
        other => return Err(CaseError::Invalid(format!("unknown image op `{other}`"))),
    })
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
            let max_size = float(case, "max_size")? as f32;
            let job =
                autotrace::replay_svg(&bytes, max_size, offset, settings.smart_bezier, &settings.machine)
                    .map_err(|e| CaseError::Invalid(e.to_string()))?;
            // LaserGRBL returns the converter text: lines joined with CRLF, split on LF
            let mut lines = job.lines;
            lines.push(String::new());
            Ok(json!({ "converter_output": lines }))
        }
        "centerline" => {
            let img = case_image(field(case, "image")?)?;
            let th = case.get("thresholds").cloned().unwrap_or_else(|| json!({}));
            let on = |k: &str| th.get(k).and_then(Value::as_bool).unwrap_or(true);
            let value = |k: &str, d: i64| th.get(k).and_then(Value::as_i64).unwrap_or(d);
            #[allow(clippy::cast_possible_truncation)]
            let options = CenterlineOptions {
                corner_threshold: on("use_corner").then(|| value("corner", 110) as i32),
                line_threshold: on("use_line").then(|| value("line", 10) as i32),
            };
            let name = field(case, "autotrace_output")?
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let svg_text = read(&name).map_err(|e| CaseError::Input(name.clone(), e))?;
            let tracer = Recorded(String::from_utf8_lossy(&svg_text).into_owned());
            #[allow(clippy::cast_possible_truncation)]
            let offset = (settings.offset.0 as f32, settings.offset.1 as f32);
            let c = autotrace::centerline(
                &img,
                &options,
                &tracer,
                offset,
                settings.smart_bezier,
                &settings.machine,
            )
            .map_err(|e| CaseError::Invalid(e.to_string()))?;
            let given = Image::decode(&c.png).map_err(|e| CaseError::Invalid(e.to_string()))?;
            let mut out = file_result(&program_of(&c.lines, &settings));
            out["autotrace_args"] = json!(c.args);
            out["autotrace_input"] = dump_image(&given);
            Ok(out)
        }
        "raster" => {
            let img = case_image(field(case, "image")?)?;
            let conf = l2l_conf(case)?;
            let opts = L2lOptions {
                disable_g0: settings.machine.disable_g0,
                unidirectional: settings.machine.unidirectional,
                max_pwm: settings.machine.max_pwm,
            };
            Ok(file_result(&program_of(
                &load_image_l2l(&img, &conf, &opts),
                &settings,
            )))
        }
        "potrace" => {
            let img = case_image(field(case, "image")?)?;
            let (options, rest) = potrace_options(case)?;
            let conf = l2l_conf(&rest)?;
            let opts = L2lOptions {
                disable_g0: settings.machine.disable_g0,
                unidirectional: settings.machine.unidirectional,
                max_pwm: settings.machine.max_pwm,
            };
            let lines = load_image_potrace(&img, &options, &conf, &opts, settings.machine.support_pwm);
            Ok(file_result(&program_of(&lines, &settings)))
        }
        "image_op" => Ok(json!({ "pixels": dump_image(&image_op(case)?) })),
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
    fn image_cases() {
        let img = json!({"pixels": ["00 FF0000", "80FFFFFF 7F"]});
        let px = |v: &Value| {
            v["pixels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        let op = |op: &str, args: Value| {
            run(
                &json!({"kind": "image_op", "op": op, "image": img, "args": args}),
                &no_files,
            )
        };
        assert_eq!(
            px(&op("invert", json!({})).unwrap()),
            ["FFFFFFFF FF00FFFF", "80000000 FF808080"]
        );
        for (name, args) in [
            ("grayscale", json!({"formula": "SimpleAverage"})),
            (
                "grayscale",
                json!({"formula": "WeightAverage", "brightness": 0.1, "contrast": 1.2}),
            ),
            ("grayscale", json!({"formula": "OpticalCorrect"})),
            (
                "grayscale",
                json!({"formula": "Custom", "r": 0.5, "g": 0.5, "b": 0.5}),
            ),
            ("threshold", json!({"threshold": 0.5, "apply": false})),
            ("whitenize", json!({"threshold": 100, "demo": true})),
            (
                "resize",
                json!({"width": 3, "height": 1, "interpolation": "Bilinear", "killalfa": false}),
            ),
            (
                "resize",
                json!({"width": 3, "height": 1, "interpolation": "HighQualityBicubic"}),
            ),
            ("fill", json!({"at": [0, 0], "color": "00FF00", "tolerance": 5})),
            ("outline", json!({"at": [1, 1]})),
        ] {
            assert_eq!(
                px(&op(name, args).unwrap()).len(),
                if name == "resize" { 1 } else { 2 },
                "{name}"
            );
        }
        for mode in [
            "Atkinson",
            "FloydSteinberg",
            "Burks",
            "Jarvis",
            "Random",
            "Sierra2",
            "Sierra3",
            "SierraLight",
            "Stucki",
        ] {
            assert!(op("dither", json!({"mode": mode})).is_ok(), "{mode}");
        }
        for (name, args) in [
            ("dither", json!({"mode": "Nope"})),
            ("grayscale", json!({"formula": "Nope"})),
            ("whitenize", json!({})),
            ("fill", json!({"at": [0, 0], "color": "12345", "tolerance": 1})),
            ("outline", json!({})),
            ("resize", json!({"width": -1, "height": 1})),
            ("blur", json!({})),
        ] {
            assert!(op(name, args).is_err(), "{name}");
        }
        let ascii = run(
            &json!({"kind": "image_op", "op": "invert", "image": {"ascii": ["#."]}}),
            &no_files,
        )
        .unwrap();
        assert_eq!(px(&ascii), ["FFFFFFFF FF000000"]);
        for bad in [
            json!({}),
            json!({"pixels": ["00 00", "00"]}),
            json!({"pixels": ["GG"]}),
        ] {
            assert!(
                run(
                    &json!({"kind": "image_op", "op": "invert", "image": bad}),
                    &no_files
                )
                .is_err()
            );
        }

        let raster = |conf: Value| {
            run(
                &json!({"kind": "raster", "image": {"ascii": ["#.#", ".#."]}, "conf": conf}),
                &no_files,
            )
        };
        let r = raster(
            json!({"res": 5, "fres": 5, "oX": 1, "oY": 2, "markSpeed": 800, "borderSpeed": 900,
            "minPower": 10, "maxPower": 500, "lOn": "M4", "lOff": "M5", "pwm": false, "vectorfilling": true,
            "firmwareType": "Smoothie", "dir": "Vertical"}),
        )
        .unwrap();
        assert!(r["gcode"].as_array().is_some_and(|g| !g.is_empty()));
        assert!(raster(json!({"dir": "Diagonal"})).is_ok());
        for bad in [
            json!({"dir": "Up"}),
            json!({"res": "x"}),
            json!({"markSpeed": 1.5}),
            json!({"lOn": 3}),
            json!({"pwm": 1}),
            json!({"zoom": 1}),
        ] {
            assert!(raster(bad).is_err());
        }
    }

    #[test]
    fn potrace_and_centerline_cases() {
        let image = json!({"ascii": ["......", ".####.", ".####.", "......"]});
        let run_po = |conf: Value, po: Value| {
            run(
                &json!({"kind": "potrace", "image": image, "conf": conf, "potrace": po}),
                &no_files,
            )
        };
        for dir in [
            "None",
            "Horizontal",
            "Vertical",
            "Diagonal",
            "NewGrid",
            "NewInsetFilling",
        ] {
            let r = run_po(
                json!({"dir": dir, "res": 2.0}),
                json!({"optimize": 0.3, "smoothing": 1.0, "spot_removal": 0}),
            );
            assert!(r.is_ok(), "{dir}: {r:?}");
        }
        assert!(run(&json!({"kind": "potrace", "image": image}), &no_files).is_ok());
        assert!(run_po(json!({"dir": "Sideways"}), json!({})).is_err());

        let svg = br#"<svg width="60" height="40"><path style="stroke:#000" d="M10 10L50 30"/></svg>"#;
        let files = |name: &str| {
            if name == "out.svg" {
                Ok(svg.to_vec())
            } else {
                Err(format!("no file {name}"))
            }
        };
        let c = run(
            &json!({"kind": "centerline", "image": image, "autotrace_output": "out.svg",
                    "thresholds": {"use_corner": false, "line": 25}}),
            &files,
        )
        .unwrap();
        let args = c["autotrace_args"].as_array().unwrap();
        assert!(!args.iter().any(|a| a == "-corner-t"));
        assert!(args.iter().any(|a| a == "2.5"));
        assert_eq!(c["autotrace_input"].as_array().unwrap().len(), 4);
        assert!(
            run(
                &json!({"kind": "centerline", "image": image, "autotrace_output": "x.svg"}),
                &files
            )
            .is_err()
        );
        assert!(run(&json!({"kind": "centerline", "image": image}), &files).is_err());
        let empty = |_: &str| Ok(Vec::new());
        assert!(
            run(
                &json!({"kind": "centerline", "image": image, "autotrace_output": "e.svg"}),
                &empty
            )
            .is_err()
        );
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
