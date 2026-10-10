//! The raster dialog's pipeline (`RasterConverter.ImageProcessor`): the edits of
//! the image, then the bitmap a tool engraves — resized to the target resolution,
//! converted to grey, white-clipped and thresholded or dithered — with the
//! resolutions the job uses. Turning that bitmap into G-code is the planner's job.

use freekerf_document::{
    Adjustments, DitherMode as DocDither, GrayFormula, ImageEdit, Interpolation as DocInterpolation,
    LineDirection, RasterSettings, RasterTool, VectorFill,
};

use crate::dither::{self, DitherMode};
use crate::gdi;
use crate::image::{Argb, Image, argb, split};
use crate::l2l::Direction;
use crate::ops::{self, Formula, Interpolation};
use crate::vectorize::Filling;

/// The largest bitmap the dialog generates (`22000 × 22000` pixels, its 64-bit limit).
pub const MAX_PIXELS: f64 = 22000.0 * 22000.0;

/// Errors of the pipeline.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProcessError {
    /// The resolution came out as zero (quality 0, or a target too big).
    #[error("target size too big for processing")]
    TargetTooBig,
}

/// The bitmap to engrave and its resolutions.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    /// The processed bitmap (rows from the top, as the image is shown).
    pub bitmap: Image,
    /// Dots per mm of the engraving (`L2LConf.res`).
    pub res: f64,
    /// Lines per mm of a filling (`L2LConf.fres`).
    pub fres: f64,
}

/// The image as LaserGRBL holds it after loading: `new Bitmap(new Bitmap(file))`
/// draws it once through GDI+, which rounds semi-transparent pixels through
/// premultiplied alpha.
#[must_use]
pub fn normalize(img: &Image) -> Image {
    let px = img
        .pixels()
        .iter()
        .map(|&p| gdi::unpremultiply(gdi::premultiply(p)))
        .collect();
    Image::from_pixels(img.width(), img.height(), px).unwrap_or_else(|_| img.clone())
}

/// `TestGrayScale`: every tenth pixel in both directions has its channels within 20.
#[must_use]
pub fn is_gray(img: &Image) -> bool {
    let mut maxdiff = 0i32;
    for x in (0..img.width()).step_by(10) {
        for y in (0..img.height()).step_by(10) {
            let (_, r, g, b) = split(img.get(x, y));
            let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
            maxdiff = maxdiff.max((r - g).abs()).max((g - b).abs()).max((r - b).abs());
        }
    }
    maxdiff < 20
}

fn similar(a: Argb, b: Argb) -> bool {
    let (a, b) = (split(a), split(b));
    let d = |x: u8, y: u8| (i32::from(x) - i32::from(y)).abs() < 20;
    d(a.0, b.0) && d(a.1, b.1) && d(a.2, b.2) && d(a.3, b.3)
}

#[allow(clippy::cast_possible_truncation)]
fn avg(a: Argb, b: Argb) -> Argb {
    let (a, b) = (split(a), split(b));
    let m = |x: u8, y: u8| ((u16::from(x) + u16::from(y)) / 2) as u8;
    argb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2), m(a.3, b.3))
}

/// `GetLineColor`: the average colour of a border line (`mode`: 0 top, 1 bottom,
/// 2 left, 3 right; `line` counted from that border), `None` when its pixels are not
/// all similar to the first one.
fn line_color(img: &Image, mode: u8, line: u32) -> Option<Argb> {
    let (w, h) = (img.width(), img.height());
    let limit = if mode < 2 { w } else { h };
    let limit2 = if mode < 2 { h } else { w };
    let mut first = None;
    let mut rv: Option<Argb> = None;
    for i in 0..limit {
        let p = match mode {
            0 => img.get(i, line),
            1 => img.get(i, limit2 - 1 - line),
            2 => img.get(line, i),
            _ => img.get(limit2 - 1 - line, i),
        };
        let f = *first.get_or_insert(p);
        rv = match rv {
            None => Some(p),
            Some(c) if similar(p, f) => Some(avg(c, p)),
            Some(_) => return None,
        };
    }
    rv
}

/// `ImageProcessor.AutoTrim`: removes border lines of the same colour as the
/// borders (when the four borders agree).
#[must_use]
pub fn auto_trim(img: &Image) -> Image {
    if img.width() == 0 || img.height() == 0 {
        return img.clone();
    }
    let mut bg: Option<Argb> = None;
    for mode in 0..4 {
        if let Some(c) = line_color(img, mode, 0) {
            bg = match bg {
                None => Some(c),
                Some(b) if similar(b, c) => Some(avg(b, c)),
                Some(_) => return img.clone(),
            };
        }
    }
    if bg.is_none() {
        return img.clone();
    }
    // `FindLimit` passes the guessed colour to `GetLineColor`, which ignores it: a
    // border line goes as long as it is uniform, whatever its colour
    let limit = |mode: u8| {
        let max = if mode < 2 { img.height() } else { img.width() };
        (0..max)
            .find(|&i| line_color(img, mode, i).is_none())
            .unwrap_or(max)
    };
    let (top, bottom, left, right) = (limit(0), limit(1), limit(2), limit(3));
    let w = i64::from(img.width()) - i64::from(left) - i64::from(right);
    let h = i64::from(img.height()) - i64::from(top) - i64::from(bottom);
    if w <= 0 || h <= 0 {
        return img.clone();
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    img.cropped(left, top, w as u32, h as u32)
}

/// A point or rectangle of the preview scaled to the image (`x * W / previewW`).
fn scale(v: i32, image: u32, preview: i32) -> i64 {
    if preview == 0 {
        return 0;
    }
    i64::from(v) * i64::from(image) / i64::from(preview)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn point(at: [i32; 2], reference: [i32; 2], img: &Image) -> Option<(u32, u32)> {
    let x = scale(at[0], img.width(), reference[0]);
    let y = scale(at[1], img.height(), reference[1]);
    (x >= 0 && y >= 0 && x < i64::from(img.width()) && y < i64::from(img.height()))
        .then_some((x as u32, y as u32))
}

/// Applies the dialog's edits in order (`CropImage`, `AutoTrim`, `RotateCW`...).
/// Points and rectangles are in coordinates of a `reference`-sized preview.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
pub fn apply_edits(original: &Image, edits: &[ImageEdit]) -> Image {
    let mut img = original.clone();
    for e in edits {
        img = match e {
            ImageEdit::Crop { rect, reference } => {
                if rect[2] <= 0 || rect[3] <= 0 {
                    continue;
                }
                let x = scale(rect[0], img.width(), reference[0]);
                let y = scale(rect[1], img.height(), reference[1]);
                let w = scale(rect[2], img.width(), reference[0]);
                let h = scale(rect[3], img.height(), reference[1]);
                if w <= 0 || h <= 0 || x < 0 || y < 0 {
                    continue;
                }
                img.cropped(x as u32, y as u32, w as u32, h as u32)
            }
            ImageEdit::AutoTrim => auto_trim(&img),
            ImageEdit::RotateCw => img.rotated_cw(),
            ImageEdit::RotateCcw => img.rotated_ccw(),
            ImageEdit::Invert => ops::invert(&img),
            ImageEdit::FlipH => img.flipped_y(),
            ImageEdit::FlipV => img.flipped_x(),
            ImageEdit::Fill {
                at,
                reference,
                argb: c,
                tolerance,
            } => match point(*at, *reference, &img) {
                Some(p) => ops::fill(&img, p, argb(c[0], c[1], c[2], c[3]), *tolerance as i32).0,
                None => img,
            },
            ImageEdit::Outline { at, reference } => match point(*at, *reference, &img) {
                Some(p) => ops::outline(&img, p),
                None => img,
            },
        };
    }
    img
}

/// `GetVectorQuality`: 10 dots/mm, or with adaptive quality `255 / √side` clamped
/// to 4..255.
fn vector_quality(size: f64, adaptive: bool) -> f64 {
    if !adaptive {
        return 10.0;
    }
    (255.0 * size.sqrt().powf(-0.5)).clamp(4.0, 255.0)
}

/// The document's resampling filter as the raster operations know it.
#[must_use]
pub fn interpolation(i: DocInterpolation) -> Interpolation {
    match i {
        DocInterpolation::NearestNeighbor => Interpolation::NearestNeighbor,
        DocInterpolation::Bilinear | DocInterpolation::HighQualityBilinear => Interpolation::Bilinear,
        DocInterpolation::Bicubic | DocInterpolation::HighQualityBicubic => Interpolation::Bicubic,
    }
}

/// The document's line direction.
#[must_use]
pub const fn direction(d: LineDirection) -> Direction {
    match d {
        LineDirection::Horizontal => Direction::Horizontal,
        LineDirection::Vertical => Direction::Vertical,
        LineDirection::Diagonal => Direction::Diagonal,
    }
}

/// The document's vectorize filling.
#[must_use]
pub fn filling(f: Option<VectorFill>) -> Filling {
    match f {
        None => Filling::None,
        Some(VectorFill::Raster(d)) => Filling::Raster(direction(d)),
        Some(VectorFill::Hatch(p)) => Filling::Vector(p),
    }
}

const fn formula(f: GrayFormula) -> Formula {
    match f {
        GrayFormula::SimpleAverage => Formula::SimpleAverage,
        GrayFormula::WeightAverage => Formula::WeightAverage,
        GrayFormula::OpticalCorrect => Formula::OpticalCorrect,
        GrayFormula::Custom => Formula::Custom,
    }
}

const fn dither_mode(d: DocDither) -> DitherMode {
    match d {
        DocDither::Atkinson => DitherMode::Atkinson,
        DocDither::FloydSteinberg => DitherMode::FloydSteinberg,
        DocDither::Burks => DitherMode::Burks,
        DocDither::Jarvis => DitherMode::Jarvis,
        DocDither::Random => DitherMode::Random,
        DocDither::Sierra2 => DitherMode::Sierra2,
        DocDither::Sierra3 => DitherMode::Sierra3,
        DocDither::SierraLight => DitherMode::SierraLight,
        DocDither::Stucki => DitherMode::Stucki,
    }
}

/// `ProduceBitmap2` for every tool but pass-through: grey, white clip, threshold or
/// dithering.
#[allow(clippy::cast_precision_loss)]
fn adjust(resized: &Image, a: &Adjustments, gray_source: bool, s: &RasterSettings) -> Image {
    #[allow(clippy::cast_possible_truncation)]
    let pct = |v: u32| v as f32 / 100.0;
    #[allow(clippy::cast_possible_wrap)]
    let brightness = -((100 - a.brightness as i32) as f32 / 100.0);
    let grey = ops::grayscale(
        resized,
        pct(a.red),
        pct(a.green),
        pct(a.blue),
        brightness,
        pct(a.contrast),
        if gray_source {
            Formula::SimpleAverage
        } else {
            formula(a.formula)
        },
    );
    #[allow(clippy::cast_possible_wrap)]
    let whiten = ops::whitenize(&grey, a.white_clip as i32, false);
    let threshold = a.threshold.map_or(0.0, pct);
    match s.tool {
        RasterTool::Dithering => dither::dither(&whiten, dither_mode(s.dithering), 0),
        RasterTool::Centerline => ops::threshold(
            &ops::threshold(&whiten, threshold, a.threshold.is_some()),
            0.5,
            true,
        ),
        _ => ops::threshold(&whiten, threshold, a.threshold.is_some()),
    }
}

/// `ImageProcessor.DoTrueWork` up to the bitmap: `original` already edited
/// ([`apply_edits`]), `gray_source` whether the file image was grey ([`is_gray`],
/// tested before the edits as LaserGRBL does), `size_mm` the target size, `file_dpi`
/// the file's resolution (pass-through engraves at it).
///
/// # Errors
/// The resolution comes out as zero.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn prepare(
    original: &Image,
    gray_source: bool,
    s: &RasterSettings,
    size_mm: [f64; 2],
    file_dpi: u32,
) -> Result<Prepared, ProcessError> {
    // TargetSize is a SizeF: single precision
    let (tw, th) = (f64::from(size_mm[0] as f32), f64::from(size_mm[1] as f32));
    let filesize = tw * th;
    let max_res = (MAX_PIXELS / filesize).sqrt();
    let mut fres = max_res.min(s.vectorize.fill_lines_per_mm);
    let mut res = match s.tool {
        RasterTool::LineToLine | RasterTool::Dithering => max_res.min(s.lines_per_mm),
        RasterTool::Centerline => 10.0,
        RasterTool::Vectorize | RasterTool::Passthrough => {
            max_res.min(vector_quality(filesize, s.vectorize.adaptive_quality))
        }
    };
    let mut size = ((tw * res) as u32, (th * res) as u32);
    if s.tool == RasterTool::Passthrough {
        size = (original.width(), original.height());
        res = f64::from(file_dpi) / 25.4;
        fres = res;
    }
    if res <= 0.0 || res.is_nan() {
        return Err(ProcessError::TargetTooBig);
    }
    let bitmap = if s.tool == RasterTool::Passthrough {
        ops::grayscale(original, 0.0, 0.0, 0.0, 0.0, 1.0, Formula::SimpleAverage)
    } else {
        let source = match s.vectorize.down_sampling {
            Some(d) if s.tool == RasterTool::Vectorize && d > 1.0 => {
                let ds = ((f64::from(size.0) / d) as u32, (f64::from(size.1) / d) as u32);
                ops::resize(original, ds.0, ds.1, false, Interpolation::Bicubic)
            }
            _ => original.clone(),
        };
        let resized = ops::resize(&source, size.0, size.1, false, interpolation(s.interpolation));
        adjust(&resized, &s.adjustments, gray_source, s)
    };
    Ok(Prepared { bitmap, res, fres })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{BLACK, WHITE};

    fn ascii(rows: &[&str]) -> Image {
        let w = u32::try_from(rows[0].len()).unwrap();
        let h = u32::try_from(rows.len()).unwrap();
        let px = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| if c == '#' { BLACK } else { WHITE }))
            .collect();
        Image::from_pixels(w, h, px).unwrap()
    }

    #[test]
    fn grey_test_and_normalization() {
        assert!(is_gray(&ascii(&["#.", ".#"])));
        assert!(!is_gray(&Image::new(2, 2, 0xFFFF_0000)));
        assert_eq!(normalize(&Image::new(1, 1, 0x80FF_FFFF)).get(0, 0), 0x80FF_FFFF);
        assert_eq!(normalize(&Image::new(1, 1, 0x0912_3456)).get(0, 0) >> 24, 0x09);
    }

    #[test]
    fn trimming_and_edits() {
        let img = ascii(&["......", "..##..", "..#...", "......"]);
        let t = auto_trim(&img);
        assert_eq!((t.width(), t.height()), (2, 2));
        // borders that do not agree: nothing trimmed
        let mixed = ascii(&["######", "..##..", "......"]);
        assert_eq!(auto_trim(&mixed), mixed);
        assert_eq!(auto_trim(&ascii(&["...", "..."])).width(), 3);
        assert_eq!(auto_trim(&Image::new(0, 0, 0)).width(), 0);

        let e = apply_edits(
            &img,
            &[
                ImageEdit::Crop {
                    rect: [50, 0, 50, 100],
                    reference: [100, 100],
                },
                ImageEdit::RotateCw,
                ImageEdit::Invert,
                ImageEdit::FlipH,
                ImageEdit::FlipV,
                ImageEdit::RotateCcw,
            ],
        );
        assert_eq!((e.width(), e.height()), (3, 4));
        let skipped = apply_edits(
            &img,
            &[ImageEdit::Crop {
                rect: [0, 0, 0, 5],
                reference: [10, 10],
            }],
        );
        assert_eq!(skipped, img);
        let filled = apply_edits(
            &img,
            &[
                ImageEdit::Fill {
                    at: [0, 0],
                    reference: [6, 4],
                    argb: [255, 0, 0, 0],
                    tolerance: 10,
                },
                ImageEdit::Fill {
                    at: [60, 0],
                    reference: [6, 4],
                    argb: [255, 0, 0, 0],
                    tolerance: 10,
                },
                ImageEdit::Outline {
                    at: [2, 1],
                    reference: [6, 4],
                },
                ImageEdit::Outline {
                    at: [-1, 1],
                    reference: [6, 4],
                },
            ],
        );
        assert_eq!(filled.width(), 6);
        assert_eq!(scale(5, 10, 0), 0);
    }

    #[test]
    fn preparing_bitmaps() {
        let img = ascii(&["#.#.", ".#.#"]);
        let mut s = RasterSettings {
            lines_per_mm: 10.0,
            ..RasterSettings::default()
        };
        let p = prepare(&img, true, &s, [0.4, 0.2], 96).unwrap();
        assert_eq!((p.bitmap.width(), p.bitmap.height(), p.res), (4, 2, 10.0));
        s.lines_per_mm = 0.0;
        assert_eq!(
            prepare(&img, true, &s, [0.4, 0.2], 96),
            Err(ProcessError::TargetTooBig)
        );
        for tool in [
            RasterTool::Dithering,
            RasterTool::Centerline,
            RasterTool::Vectorize,
            RasterTool::Passthrough,
        ] {
            let mut s = RasterSettings {
                tool,
                lines_per_mm: 10.0,
                ..RasterSettings::default()
            };
            s.vectorize.down_sampling = Some(2.0);
            s.vectorize.adaptive_quality = true;
            s.adjustments.threshold = Some(50);
            let p = prepare(&img, false, &s, [0.4, 0.2], 96).unwrap();
            assert!(p.bitmap.width() > 0, "{tool:?}");
        }
        assert!((vector_quality(1.0, true) - 255.0).abs() < 1e-9);
        assert!((vector_quality(1e9, true) - 4.0).abs() < 1e-9);
        assert_eq!(
            interpolation(DocInterpolation::HighQualityBilinear),
            Interpolation::Bilinear
        );
        assert_eq!(
            interpolation(DocInterpolation::NearestNeighbor),
            Interpolation::NearestNeighbor
        );
        assert_eq!(direction(LineDirection::Diagonal), Direction::Diagonal);
        assert_eq!(
            filling(Some(VectorFill::Raster(LineDirection::Vertical))),
            Filling::Raster(Direction::Vertical)
        );
        assert_eq!(filling(None), Filling::None);
        for f in [GrayFormula::WeightAverage, GrayFormula::OpticalCorrect] {
            let _ = formula(f);
        }
        for d in [
            DocDither::Burks,
            DocDither::Jarvis,
            DocDither::Random,
            DocDither::Sierra2,
            DocDither::Sierra3,
            DocDither::SierraLight,
            DocDither::Stucki,
            DocDither::FloydSteinberg,
        ] {
            let _ = dither_mode(d);
        }
    }
}
