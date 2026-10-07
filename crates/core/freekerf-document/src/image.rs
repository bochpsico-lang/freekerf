//! Raster images in a document and how they are turned into a job (LaserGRBL's
//! raster dialog: `RasterToLaserForm` + `ImageProcessor`).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use freekerf_geometry::Point;
use freekerf_geometry::hatch::FillPattern;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Bytes stored as base64 in JSON.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bytes(pub Vec<u8>);

impl Serialize for Bytes {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&B64.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        B64.decode(s.as_bytes())
            .map(Bytes)
            .map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for Bytes {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Bytes".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "contentEncoding": "base64",
            "description": "binary data, base64"
        })
    }
}

/// The pixels of an image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImageData {
    /// The file as imported (PNG, JPEG, BMP, GIF): decoded when needed.
    Encoded {
        /// Media type (`image/png`...).
        media_type: String,
        /// The file bytes.
        bytes: Bytes,
    },
    /// Decoded pixels, 8-bit RGBA, row by row from the top.
    Rgba {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// `width * height * 4` bytes.
        pixels: Bytes,
    },
}

/// What the raster dialog does with the image (`ImageProcessor.Tool`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RasterTool {
    /// Line by line, power proportional to the grey level.
    #[default]
    LineToLine,
    /// Line by line on a dithered (black and white) image.
    Dithering,
    /// Trace the borders as vectors (Potrace), optionally fill them.
    Vectorize,
    /// Trace the centre lines of strokes (autotrace).
    Centerline,
    /// Line by line on the image as it is (no grey processing).
    Passthrough,
}

/// Direction of raster lines (`ImageProcessor.Direction` for Line2Line).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LineDirection {
    /// Left to right, then back.
    #[default]
    Horizontal,
    /// Bottom to top, then back.
    Vertical,
    /// Diagonal strips.
    Diagonal,
}

/// Filling of vectorized shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "direction", rename_all = "snake_case")]
pub enum VectorFill {
    /// Raster lines inside the shapes (drawn and resampled, then line by line).
    Raster(LineDirection),
    /// Vector hatch lines clipped by the shapes.
    Hatch(FillPattern),
}

/// Grey conversion formula (`ImageTransform.Formula`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrayFormula {
    /// (R + G + B) / 3.
    #[default]
    SimpleAverage,
    /// Weighted average.
    WeightAverage,
    /// Luminance with the "optical correct" weights.
    OpticalCorrect,
    /// The red, green and blue percentages of [`Adjustments`].
    Custom,
}

/// Dithering algorithm (`ImageTransform.DitheringMode`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DitherMode {
    /// Atkinson.
    Atkinson,
    /// Floyd–Steinberg.
    #[default]
    FloydSteinberg,
    /// Burkes.
    Burks,
    /// Jarvis, Judice and Ninke.
    Jarvis,
    /// Random threshold.
    Random,
    /// Sierra (two rows).
    Sierra2,
    /// Sierra (three rows).
    Sierra3,
    /// Sierra Lite.
    SierraLight,
    /// Stucki.
    Stucki,
}

/// Resampling filter for resizing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    /// Nearest neighbour (pixel art, sharp edges).
    NearestNeighbor,
    /// Bilinear.
    Bilinear,
    /// Bicubic.
    Bicubic,
    /// High-quality bilinear.
    HighQualityBilinear,
    /// High-quality bicubic (LaserGRBL's default).
    #[default]
    HighQualityBicubic,
}

/// Grey-level adjustments, in the units of LaserGRBL's dialog.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Adjustments {
    /// Grey formula.
    pub formula: GrayFormula,
    /// Red weight, percent (`Custom` formula).
    pub red: u32,
    /// Green weight, percent.
    pub green: u32,
    /// Blue weight, percent.
    pub blue: u32,
    /// Brightness, percent (100 = unchanged).
    pub brightness: u32,
    /// Contrast, percent (100 = unchanged).
    pub contrast: u32,
    /// Black/white threshold, percent (`None`: keep the grey levels).
    pub threshold: Option<u32>,
    /// Lighter greys become white, percent.
    pub white_clip: u32,
}

impl Default for Adjustments {
    fn default() -> Self {
        Self {
            formula: GrayFormula::SimpleAverage,
            red: 100,
            green: 100,
            blue: 100,
            brightness: 100,
            contrast: 100,
            threshold: None,
            white_clip: 5,
        }
    }
}

/// Vectorize (Potrace) options; `None` disables an option.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VectorizeSettings {
    /// Drop shapes up to this many pixels ("turd size").
    pub spot_removal: Option<u32>,
    /// Corner smoothing (`alphamax`).
    pub smoothing: Option<f64>,
    /// Curve optimization tolerance.
    pub optimize: Option<f64>,
    /// Lower the resolution where detail allows ("adaptive quality").
    pub adaptive_quality: bool,
    /// Down-sampling factor before tracing.
    pub down_sampling: Option<f64>,
    /// Reorder the traced paths to reduce travel.
    pub optimize_fast: bool,
    /// Filling of the traced shapes.
    pub fill: Option<VectorFill>,
    /// Filling quality, lines per mm.
    pub fill_lines_per_mm: f64,
}

impl Default for VectorizeSettings {
    fn default() -> Self {
        Self {
            spot_removal: None,
            smoothing: None,
            optimize: None,
            adaptive_quality: false,
            down_sampling: None,
            optimize_fast: false,
            fill: None,
            fill_lines_per_mm: 3.0,
        }
    }
}

/// Centerline (autotrace) thresholds; `None` disables one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CenterlineSettings {
    /// Corner threshold, degrees.
    pub corner_threshold: Option<u32>,
    /// Line threshold, tenths of a pixel.
    pub line_threshold: Option<u32>,
}

impl Default for CenterlineSettings {
    fn default() -> Self {
        Self {
            corner_threshold: Some(110),
            line_threshold: Some(10),
        }
    }
}

/// How an image becomes a job (the raster dialog values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RasterSettings {
    /// Processing tool.
    pub tool: RasterTool,
    /// Line direction for line-by-line tools.
    pub direction: LineDirection,
    /// Lines per millimetre.
    pub lines_per_mm: f64,
    /// Grey adjustments.
    pub adjustments: Adjustments,
    /// Dithering algorithm (`dithering` tool).
    pub dithering: DitherMode,
    /// Resampling filter.
    pub interpolation: Interpolation,
    /// Vectorize options.
    pub vectorize: VectorizeSettings,
    /// Centerline options.
    pub centerline: CenterlineSettings,
    /// Feed rate of borders (vectorize, centerline), mm/min.
    pub border_speed_mm_min: u32,
    /// Feed rate of raster lines, mm/min.
    pub mark_speed_mm_min: u32,
    /// `S` for white (raster) / unused below.
    pub min_power: u32,
    /// `S` for black.
    pub max_power: u32,
    /// Laser on command (`M3` constant power, `M4` dynamic).
    pub laser_on: String,
    /// Laser off command.
    pub laser_off: String,
}

impl Default for RasterSettings {
    fn default() -> Self {
        Self {
            tool: RasterTool::LineToLine,
            direction: LineDirection::Horizontal,
            lines_per_mm: 3.0,
            adjustments: Adjustments::default(),
            dithering: DitherMode::FloydSteinberg,
            interpolation: Interpolation::HighQualityBicubic,
            vectorize: VectorizeSettings::default(),
            centerline: CenterlineSettings::default(),
            border_speed_mm_min: 1000,
            mark_speed_mm_min: 1000,
            min_power: 0,
            max_power: 1000,
            laser_on: "M3".into(),
            laser_off: "M5".into(),
        }
    }
}

/// A non-destructive edit of the image, applied in order before processing.
/// Coordinates refer to a preview of `reference` size (as in the raster dialog,
/// where the user clicks on a scaled preview).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ImageEdit {
    /// Keep a rectangle `[x, y, w, h]`.
    Crop {
        /// Rectangle in preview coordinates.
        rect: [i32; 4],
        /// Size of the preview.
        reference: [i32; 2],
    },
    /// Remove white borders.
    AutoTrim,
    /// Rotate 90° clockwise.
    RotateCw,
    /// Rotate 90° counterclockwise.
    RotateCcw,
    /// Invert colours.
    Invert,
    /// Upside down.
    FlipH,
    /// Mirror.
    FlipV,
    /// Flood fill from a point with a colour (`#AARRGGBB` as four bytes).
    Fill {
        /// Seed point in preview coordinates.
        at: [i32; 2],
        /// Size of the preview.
        reference: [i32; 2],
        /// ARGB colour.
        argb: [u8; 4],
        /// Colour tolerance.
        tolerance: u32,
    },
    /// Keep only the outline of the region at a point.
    Outline {
        /// Seed point in preview coordinates.
        at: [i32; 2],
        /// Size of the preview.
        reference: [i32; 2],
    },
}

/// An image placed on the bed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImageObject {
    /// Pixels.
    pub data: ImageData,
    /// Lower-left corner on the bed, mm.
    pub position: Point,
    /// Size on the bed, mm `[width, height]`.
    pub size_mm: [f64; 2],
    /// How it becomes a job.
    pub settings: RasterSettings,
    /// Edits applied before processing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<ImageEdit>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_base64_in_json() {
        let d = ImageData::Rgba {
            width: 1,
            height: 1,
            pixels: Bytes(vec![255, 0, 0, 255]),
        };
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"/wAA/w==\""), "{json}");
        assert_eq!(serde_json::from_str::<ImageData>(&json).unwrap(), d);
        assert!(serde_json::from_str::<Bytes>("\"***\"").is_err());
    }

    #[test]
    fn defaults_match_the_dialog() {
        let s = RasterSettings::default();
        assert_eq!(s.tool, RasterTool::LineToLine);
        assert_eq!(s.adjustments.white_clip, 5);
        assert_eq!(s.centerline.corner_threshold, Some(110));
        assert_eq!(s.vectorize.fill, None);
        let edit = ImageEdit::Fill {
            at: [1, 2],
            reference: [100, 100],
            argb: [255, 0, 0, 0],
            tolerance: 10,
        };
        let back: ImageEdit = serde_json::from_str(&serde_json::to_string(&edit).unwrap()).unwrap();
        assert_eq!(back, edit);
        let fill: VectorFill = serde_json::from_str(r#"{"kind":"hatch","direction":"cross"}"#).unwrap();
        assert_eq!(fill, VectorFill::Hatch(FillPattern::Cross));
    }
}
