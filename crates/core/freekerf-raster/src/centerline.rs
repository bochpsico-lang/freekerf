//! Centerline vectorization (`GrblFile.LoadImageCenterline`): LaserGRBL hands the
//! bitmap, as a PNG, to the external `autotrace` program with `-centerline` and
//! converts the SVG it prints. This module builds that request; running the
//! program is the runtime's job (ADR 0010), converting the SVG the importer's.

/// Thresholds of the centerline dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CenterlineOptions {
    /// `-corner-t` (degrees), when set.
    pub corner_threshold: Option<i32>,
    /// `-line-t` in tenths (10 is `1.0`), when set.
    pub line_threshold: Option<i32>,
}

impl Default for CenterlineOptions {
    /// The dialog's defaults: corner 110, line 1.0.
    fn default() -> Self {
        Self {
            corner_threshold: Some(110),
            line_threshold: Some(10),
        }
    }
}

/// `Autotrace.BitmapToSvgString`'s arguments, the input file last.
#[must_use]
pub fn autotrace_args(options: &CenterlineOptions, input: &str) -> Vec<String> {
    let mut args: Vec<String> = ["-output-fo", "svg", "-background-color", "FFFFFF", "-centerline"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    if let Some(ct) = options.corner_threshold {
        args.push("-corner-t".into());
        args.push(ct.to_string());
    }
    if let Some(lt) = options.line_threshold {
        args.push("-line-t".into());
        args.push(freekerf_gcode::dotnet::double_format(f64::from(lt) / 10.0, "0.0"));
    }
    args.push(input.to_owned());
    args
}

/// The largest side of the converted drawing, mm: a tenth of the image's larger
/// side in pixels (`SvgMaxSize`).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn max_size_mm(width: u32, height: u32) -> f32 {
    (f64::from(width) / 10.0).max(f64::from(height) / 10.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments() {
        assert_eq!(
            autotrace_args(&CenterlineOptions::default(), "in.png").join(" "),
            "-output-fo svg -background-color FFFFFF -centerline -corner-t 110 -line-t 1.0 in.png"
        );
        let off = CenterlineOptions {
            corner_threshold: None,
            line_threshold: None,
        };
        assert_eq!(autotrace_args(&off, "x").len(), 6);
        let custom = CenterlineOptions {
            corner_threshold: Some(100),
            line_threshold: Some(25),
        };
        assert!(
            autotrace_args(&custom, "x")
                .join(" ")
                .contains("-corner-t 100 -line-t 2.5")
        );
        assert!((max_size_mm(12, 8) - 1.2).abs() < 1e-6);
    }
}
