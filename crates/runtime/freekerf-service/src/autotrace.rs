//! Centerline vectorization with the external `autotrace` program (ADR 0010):
//! the bitmap goes to it as a PNG, the SVG it prints is converted like any SVG,
//! scaled to a tenth of the image's pixel size.

use std::path::{Path, PathBuf};

use freekerf_document::{Source, SourceFormat};
use freekerf_import::svg;
use freekerf_planner::{Job, MachineOptions, PlanOptions, VectorStyle};
use freekerf_raster::centerline::{CenterlineOptions, autotrace_args, max_size_mm};
use freekerf_raster::image::Image;

/// Errors of the centerline conversion.
#[derive(Debug, thiserror::Error)]
pub enum CenterlineError {
    /// The tracer failed.
    #[error("autotrace: {0}")]
    Trace(String),
    /// The SVG could not be converted.
    #[error("converting the traced svg: {0}")]
    Convert(String),
}

/// Runs a tracer: `args` (from [`autotrace_args`]) name the input with
/// [`INPUT_PLACEHOLDER`], which the tracer replaces with the file it writes `png`
/// to. Returns the SVG text. A trait so tests and headless runs can use a double.
pub trait Tracer {
    /// Traces `png`.
    ///
    /// # Errors
    /// The program failed.
    fn trace(&self, png: &[u8], args: &[String]) -> Result<String, CenterlineError>;
}

/// Stands for the input file in the arguments given to a [`Tracer`].
pub const INPUT_PLACEHOLDER: &str = "<png>";

/// The real `autotrace` executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAutotrace {
    /// The program (`autotrace` on the `PATH` by default).
    pub program: PathBuf,
    /// Where the temporary PNG goes.
    pub temp_dir: PathBuf,
}

impl Default for ExternalAutotrace {
    fn default() -> Self {
        Self {
            program: PathBuf::from(if cfg!(windows) {
                "autotrace.exe"
            } else {
                "autotrace"
            }),
            temp_dir: std::env::temp_dir(),
        }
    }
}

impl Tracer for ExternalAutotrace {
    fn trace(&self, png: &[u8], args: &[String]) -> Result<String, CenterlineError> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let file = self
            .temp_dir
            .join(format!("freekerf-centerline-{}-{nanos}.png", std::process::id()));
        let err = |e: freekerf_platform::PlatformError| CenterlineError::Trace(e.to_string());
        freekerf_platform::write_atomic(&file, png).map_err(err)?;
        let name = file.display().to_string();
        let args: Vec<String> = args
            .iter()
            .map(|a| {
                if a == INPUT_PLACEHOLDER {
                    name.clone()
                } else {
                    a.clone()
                }
            })
            .collect();
        let result = freekerf_platform::run_capture(Path::new(&self.program), &args);
        let _ = freekerf_platform::remove_file(&file);
        Ok(String::from_utf8_lossy(&result.map_err(err)?).into_owned())
    }
}

/// The job of an SVG written by the centerline tracer (`LoadImageCenterline`'s
/// conversion half, also the `svg_text` conformance kind): every element, one
/// layer, scaled so its larger side is `max_size` mm, moved by `offset` mm. Speed,
/// power and laser commands are the machine options' defaults (LaserGRBL takes
/// them from the raster dialog's settings).
///
/// # Errors
/// The SVG cannot be read.
pub fn replay_svg(
    svg_bytes: &[u8],
    max_size: f32,
    offset: (f32, f32),
    smart_bezier: bool,
    machine: &MachineOptions,
) -> Result<Job, CenterlineError> {
    let opts = svg::SvgOptions {
        scale_to_mm: Some(max_size),
        user_offset: offset,
        legacy_bezier: !smart_bezier,
    };
    let import = svg::import(svg_bytes, &opts).map_err(|e| CenterlineError::Convert(e.to_string()))?;
    let doc = import.into_single_layer(Some(&Source {
        format: SourceFormat::Svg,
        file: String::new(),
    }));
    freekerf_planner::plan(
        &doc,
        &PlanOptions {
            machine: machine.clone(),
            vector_style: Some(VectorStyle::Svg),
            layer_preamble: false,
        },
    )
    .map_err(|e| CenterlineError::Convert(e.to_string()))
}

/// What a centerline conversion did: the tracer's arguments (input as
/// [`INPUT_PLACEHOLDER`]), the PNG and the program lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Centerline {
    /// Arguments given to the tracer.
    pub args: Vec<String>,
    /// The PNG given to the tracer.
    pub png: Vec<u8>,
    /// Program lines, as LaserGRBL loads them (trimmed, comment-only lines dropped).
    pub lines: Vec<String>,
}

/// `GrblFile.LoadImageCenterline`.
///
/// # Errors
/// The tracer fails or prints nothing, or its SVG cannot be converted.
pub fn centerline(
    img: &Image,
    options: &CenterlineOptions,
    tracer: &dyn Tracer,
    offset: (f32, f32),
    smart_bezier: bool,
    machine: &MachineOptions,
) -> Result<Centerline, CenterlineError> {
    let png = img
        .encode_png()
        .map_err(|e| CenterlineError::Convert(e.to_string()))?;
    let args = autotrace_args(options, INPUT_PLACEHOLDER);
    // LaserGRBL logs a failing autotrace and then fails on the empty output as
    // "Root element is missing"; the cause is reported instead (DIV-048)
    let svg_text = tracer.trace(&png, &args)?;
    if svg_text.trim().is_empty() {
        return Err(CenterlineError::Trace("printed nothing".into()));
    }
    let lines = replay_svg(
        svg_text.as_bytes(),
        max_size_mm(img.width(), img.height()),
        offset,
        smart_bezier,
        machine,
    )?
    .lines;
    // LaserGRBL trims every line and keeps the commands that are not empty once
    // their comments are removed (`S0 (M)` stays as `S0 `, as `GrblCommand` reads it)
    let lines = lines
        .iter()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty() && !freekerf_gcode::Command::new(l).is_empty())
        .collect();
    Ok(Centerline { args, png, lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(Result<String, String>);

    impl Tracer for Fixed {
        fn trace(&self, png: &[u8], args: &[String]) -> Result<String, CenterlineError> {
            assert!(png.starts_with(b"\x89PNG"));
            assert_eq!(args.last().map(String::as_str), Some(INPUT_PLACEHOLDER));
            self.0.clone().map_err(CenterlineError::Trace)
        }
    }

    const SVG: &str =
        r#"<svg width="120" height="80"><path style="stroke:#000000; fill:none;" d="M10 70L50 70"/></svg>"#;

    #[test]
    fn centerline_with_a_double() {
        let img = Image::new(12, 8, 0xFFFF_FFFF);
        let c = centerline(
            &img,
            &CenterlineOptions::default(),
            &Fixed(Ok(SVG.into())),
            (0.0, 0.0),
            true,
            &MachineOptions::default(),
        )
        .unwrap();
        assert!(c.lines.iter().any(|l| l.starts_with("G1")), "{:?}", c.lines);
        assert!(
            c.lines
                .iter()
                .all(|l| !freekerf_gcode::Command::new(l).is_empty())
        );
        let run = |r: Result<String, String>| {
            centerline(
                &img,
                &CenterlineOptions::default(),
                &Fixed(r),
                (0.0, 0.0),
                true,
                &MachineOptions::default(),
            )
        };
        assert!(run(Err("boom".into())).unwrap_err().to_string().contains("boom"));
        assert!(run(Ok("  ".into())).unwrap_err().to_string().contains("nothing"));
        assert!(run(Ok("<svg".into())).is_err());
        assert!(replay_svg(b"<svg", 1.0, (0.0, 0.0), true, &MachineOptions::default()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn external_program() {
        let dir = tempfile::tempdir().unwrap();
        // a stand-in that prints the file name it was given
        let fake = ExternalAutotrace {
            program: PathBuf::from("echo"),
            temp_dir: dir.path().to_path_buf(),
        };
        let out = fake
            .trace(b"png", &["-x".into(), INPUT_PLACEHOLDER.into()])
            .unwrap();
        assert!(out.starts_with("-x ") && out.contains("freekerf-centerline-"));
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            0,
            "temporary file removed"
        );
        let missing = ExternalAutotrace {
            program: PathBuf::from("/no/such/autotrace"),
            temp_dir: dir.path().to_path_buf(),
        };
        assert!(missing.trace(b"png", &[]).is_err());
        assert!(
            ExternalAutotrace::default()
                .program
                .to_string_lossy()
                .contains("autotrace")
        );
    }
}
