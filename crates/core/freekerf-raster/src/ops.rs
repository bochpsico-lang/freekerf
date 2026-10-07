//! Image operations of LaserGRBL's raster dialog (`RasterConverter/ImageTransform.cs`).
//!
//! Grey conversion, inversion and threshold are GDI+ operations in LaserGRBL (a
//! `ColorMatrix` or a threshold applied while drawing). They are computed here as
//! libgdiplus computes them (single-precision products truncated to integers;
//! values above 255 clamped, negative ones wrapping as a C byte cast does), so opaque
//! images give the same pixels as LaserGRBL on Mono. Resizing is drawn by cairo in
//! libgdiplus and by GDI+ on Windows: FreeKerf uses its own filters, and the
//! `conformance` cases of these operations are compared with a tolerance (ADR 0008).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::image::{Argb, BLACK, Image, TRANSPARENT, WHITE, argb, split};

/// A 5×5 colour matrix (`System.Drawing.Imaging.ColorMatrix`, row vectors).
pub type ColorMatrix = [[f32; 5]; 5];

/// Grey conversion formula (`ImageTransform.Formula`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Formula {
    /// 0.333 each.
    #[default]
    SimpleAverage,
    /// 0.333, 0.444, 0.222.
    WeightAverage,
    /// 0.299, 0.587, 0.114.
    OpticalCorrect,
    /// 0.333 times the given red, green and blue weights.
    Custom,
}

/// Composites a pixel of alpha `a` over `under` (cairo `OVER` on 8-bit premultiplied
/// values, read back as non-premultiplied).
fn over(c: Argb, under: Argb) -> Argb {
    let (a, r, g, b) = split(c);
    if a == 255 {
        return c;
    }
    let (ua, ur, ug, ub) = split(under);
    let mul = |x: u8, y: u8| (u32::from(x) * u32::from(y) + 127) / 255;
    let inv = 255 - a;
    let out_a = u32::from(a) + mul(ua, inv);
    if out_a == 0 {
        return 0;
    }
    let ch = |s: u8, d: u8| {
        let premul = mul(s, a) + mul(mul(d, ua) as u8, inv);
        u8::try_from((premul * 255 + out_a / 2) / out_a).unwrap_or(255)
    };
    argb(
        u8::try_from(out_a).unwrap_or(255),
        ch(r, ur),
        ch(g, ug),
        ch(b, ub),
    )
}

/// One channel of libgdiplus's colour matrix: the float sum truncated to an integer.
#[allow(clippy::cast_possible_truncation)]
fn channel(cm: &ColorMatrix, col: usize, r: u8, g: u8, b: u8, a: u8) -> i32 {
    (f32::from(r) * cm[0][col]
        + f32::from(g) * cm[1][col]
        + f32::from(b) * cm[2][col]
        + f32::from(a) * cm[3][col]
        + 255.0 * cm[4][col]) as i32
}

/// Clamps above 255; below 0 the value wraps, as libgdiplus's `(BYTE)` cast does.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_byte(v: i32) -> u8 {
    if v > 255 { 255 } else { v as u8 }
}

/// Applies a colour matrix as `DrawImage` with `ImageAttributes.SetColorMatrix` does
/// on libgdiplus, drawn onto a transparent bitmap.
#[must_use]
pub fn color_matrix(img: &Image, cm: &ColorMatrix) -> Image {
    let mut out = Image::new(img.width(), img.height(), 0);
    for y in 0..img.height() {
        for x in 0..img.width() {
            let (a, r, g, b) = split(img.get(x, y));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let na = channel(cm, 3, r, g, b, a) as u8;
            let c = argb(
                na,
                to_byte(channel(cm, 0, r, g, b, a)),
                to_byte(channel(cm, 1, r, g, b, a)),
                to_byte(channel(cm, 2, r, g, b, a)),
            );
            out.set(x, y, over(c, 0));
        }
    }
    out
}

/// `ImageTransform.GrayScale`: grey from the formula's weights, times `contrast`, plus
/// `brightness` (both as fractions: 1 and 0 leave the image unchanged).
#[must_use]
pub fn grayscale(
    img: &Image,
    r: f32,
    g: f32,
    b: f32,
    brightness: f32,
    contrast: f32,
    formula: Formula,
) -> Image {
    let (mut rf, mut gf, mut bf) = match formula {
        Formula::SimpleAverage => (0.333f32, 0.333f32, 0.333f32),
        Formula::WeightAverage => (0.333, 0.444, 0.222),
        Formula::OpticalCorrect => (0.299, 0.587, 0.114),
        Formula::Custom => (0.333 * r, 0.333 * g, 0.333 * b),
    };
    rf *= contrast;
    gf *= contrast;
    bf *= contrast;
    let cm = [
        [rf, rf, rf, 0.0, 0.0],
        [gf, gf, gf, 0.0, 0.0],
        [bf, bf, bf, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0, 0.0],
        [brightness, brightness, brightness, 0.0, 1.0],
    ];
    color_matrix(img, &cm)
}

/// `ImageTransform.InvertingImage`: the negative.
#[must_use]
pub fn invert(img: &Image) -> Image {
    let cm = [
        [-1.0, 0.0, 0.0, 0.0, 0.0],
        [0.0, -1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, -1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 1.0, 1.0, 0.0, 1.0],
    ];
    color_matrix(img, &cm)
}

/// `ImageTransform.Threshold`: the image over white, then (when `apply`) every channel
/// above `threshold × 255` (rounded half away from zero) becomes 255, the others 0.
#[must_use]
pub fn threshold(img: &Image, threshold: f32, apply: bool) -> Image {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cutoff = (f64::from(threshold) * 255.0).round() as u8;
    let mut out = Image::new(img.width(), img.height(), WHITE);
    for y in 0..img.height() {
        for x in 0..img.width() {
            let mut c = over(img.get(x, y), WHITE);
            if apply {
                let (a, r, g, b) = split(c);
                let t = |v: u8| if v > cutoff { 255 } else { 0 };
                c = argb(a, t(r), t(g), t(b));
            }
            out.set(x, y, c);
        }
    }
    out
}

/// `ImageTransform.Whitenize`: pixels within `threshold` of white on every channel
/// become transparent (or light pink with `demo`), keeping their offset from white.
#[must_use]
pub fn whitenize(img: &Image, threshold: i32, demo: bool) -> Image {
    let new = if demo { 0xFFFF_B6C1 } else { TRANSPARENT };
    let (na, nr, ng, nb) = split(new);
    let mut out = img.clone();
    let near = |v: u8| i32::from(v) < 255 + threshold && i32::from(v) > 255 - threshold;
    let res = |v: u8, n: u8| u8::try_from((255 - i32::from(v) + i32::from(n)).clamp(0, 255)).unwrap_or(0);
    for y in 0..img.height() {
        for x in 0..img.width() {
            let (a, r, g, b) = split(img.get(x, y));
            if a != 0 && near(b) && near(g) && near(r) {
                out.set(x, y, argb(na, res(r, nr), res(g, ng), res(b, nb)));
            }
        }
    }
    out
}

/// `ImageTransform.Fill`: flood fill (4-connected) from `at` with `color`, over the
/// pixels whose channels are all within `tolerance` (exclusive) of the seed's.
/// Returns the image and the pixels the fill looked at (filled or bordering).
#[must_use]
pub fn fill(img: &Image, at: (u32, u32), color: Argb, tolerance: i32) -> (Image, Vec<bool>) {
    let (w, h) = (img.width(), img.height());
    let mut out = img.clone();
    let mut touched = vec![false; (w as usize) * (h as usize)];
    if at.0 >= w || at.1 >= h {
        return (out, touched);
    }
    let (_, r0, g0, b0) = split(img.get(at.0, at.1));
    let similar = |c: Argb| {
        let (_, r, g, b) = split(c);
        let ok = |c0: u8, cx: u8| {
            let (c0, cx) = (i32::from(c0), i32::from(cx));
            c0 > cx - tolerance && c0 < cx + tolerance
        };
        ok(r0, r) && ok(g0, g) && ok(b0, b)
    };
    let mut stack = vec![(i64::from(at.0), i64::from(at.1))];
    while let Some((px, py)) = stack.pop() {
        if px < 0 || py < 0 || px >= i64::from(w) || py >= i64::from(h) {
            continue;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (x, y) = (px as u32, py as u32);
        touched[(y as usize) * (w as usize) + x as usize] = true;
        if similar(out.get(x, y)) {
            out.set(x, y, color);
            for (nx, ny) in [(px, py + 1), (px, py - 1), (px + 1, py), (px - 1, py)] {
                if nx >= 0 && ny >= 0 && nx < i64::from(w) && ny < i64::from(h) {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let i = (ny as usize) * (w as usize) + nx as usize;
                    if !touched[i] {
                        stack.push((nx, ny));
                    }
                }
            }
        }
    }
    (out, touched)
}

/// `ImageTransform.Outliner`: the region at `at` (and its border) stays, the rest
/// becomes black: the region filled white, everything the fill did not reach black.
#[must_use]
pub fn outline(img: &Image, at: (u32, u32)) -> Image {
    let (mut out, touched) = fill(img, at, WHITE, 127);
    let w = img.width() as usize;
    for y in 0..img.height() {
        for x in 0..img.width() {
            if !touched[(y as usize) * w + x as usize] {
                out.set(x, y, BLACK);
            }
        }
    }
    out
}

/// Resampling filters of [`resize`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    /// Nearest pixel.
    NearestNeighbor,
    /// Bilinear.
    Bilinear,
    /// Bicubic (Catmull-Rom).
    #[default]
    Bicubic,
}

/// `ImageTransform.ResizeImage`: the image scaled to `width × height`, over white
/// when `kill_alpha`, otherwise replacing a transparent bitmap. Same size: an
/// unchanged copy.
///
/// Smooth filters give what LaserGRBL gets on libgdiplus whatever mode it asks
/// for: Cairo's box filter ([`crate::gdi::draw_scaled`], ADR 0008).
/// [`Interpolation::NearestNeighbor`] copies the nearest pixel (libgdiplus blends,
/// DIV-108).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn resize(img: &Image, width: u32, height: u32, kill_alpha: bool, filter: Interpolation) -> Image {
    if img.width() == width && img.height() == height {
        return img.clone();
    }
    if filter != Interpolation::NearestNeighbor {
        return crate::gdi::draw_scaled(img, width, height, kill_alpha);
    }
    let mut out = Image::new(width, height, if kill_alpha { WHITE } else { 0 });
    if img.width() == 0 || img.height() == 0 {
        return out;
    }
    let sx = f64::from(img.width()) / f64::from(width.max(1));
    let sy = f64::from(img.height()) / f64::from(height.max(1));
    for y in 0..height {
        for x in 0..width {
            let px = (((f64::from(x) + 0.5) * sx) as u32).min(img.width() - 1);
            let py = (((f64::from(y) + 0.5) * sy) as u32).min(img.height() - 1);
            let p = img.get(px, py);
            out.set(x, y, if kill_alpha { over(p, WHITE) } else { p });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(px: &[Argb], w: u32) -> Image {
        Image::from_pixels(w, u32::try_from(px.len()).unwrap() / w, px.to_vec()).unwrap()
    }

    #[test]
    fn grey_matrices_truncate_like_libgdiplus() {
        let i = img(&[0xFFFF_0000, 0xFF00_FF00, 0xFFFF_FFFF, 0xFF80_8080], 4);
        let g = grayscale(&i, 1.0, 1.0, 1.0, 0.0, 1.0, Formula::SimpleAverage);
        assert_eq!(g.pixels(), [0xFF54_5454, 0xFF54_5454, 0xFFFE_FEFE, 0xFF7F_7F7F]);
        let c = grayscale(&i, 2.0, 0.5, 0.5, 0.1, 1.5, Formula::Custom);
        assert_eq!(c.get(0, 0), 0xFFFF_FFFF);
        assert_eq!(c.get(1, 0), 0xFF59_5959);
        let w = grayscale(&i, 1.0, 1.0, 1.0, 0.0, 1.0, Formula::WeightAverage);
        assert_eq!(w.get(1, 0), 0xFF71_7171);
        let o = grayscale(&i, 1.0, 1.0, 1.0, -0.5, 1.0, Formula::OpticalCorrect);
        // 0.299*255 - 127.5 < 0 wraps as a byte cast
        assert_eq!(o.get(0, 0) & 0xFF, 0xCD); // -51 as a byte
        assert_eq!(invert(&i).get(3, 0), 0xFF7F_7F7F);
        assert_eq!(color_matrix(&img(&[0], 1), &[[0.0; 5]; 5]).get(0, 0), 0);
    }

    #[test]
    fn threshold_and_whitenize() {
        let i = img(&[0xFF70_7070, 0xFF90_9090, 0x0000_0000], 3);
        assert_eq!(threshold(&i, 0.5, true).pixels(), [BLACK, WHITE, WHITE]);
        assert_eq!(threshold(&i, 0.5, false).get(0, 0), 0xFF70_7070);
        let wz = img(&[0xFFFF_FFFF, 0xFFFA_FAFA, 0xFFF0_F0F0, 0x00FF_FFFF], 4);
        assert_eq!(
            whitenize(&wz, 10, false).pixels(),
            [0x00FF_FFFF, 0x00FF_FFFF, 0xFFF0_F0F0, 0x00FF_FFFF]
        );
        assert_eq!(whitenize(&wz, 10, true).get(0, 0), 0xFFFF_B6C1);
        // semi-transparent over white
        assert_eq!(over(0x80FF_0000, WHITE), 0xFFFF_7F7F);
        assert_eq!(over(0x0000_0000, 0), 0);
    }

    #[test]
    fn fill_and_outline() {
        let i = img(
            &[
                WHITE,
                WHITE,
                BLACK,
                WHITE,
                0xFFFA_FAFA,
                BLACK,
                BLACK,
                WHITE,
                WHITE,
                WHITE,
                WHITE,
                0xFF64_6464,
            ],
            4,
        );
        let (f, touched) = fill(&i, (0, 0), 0xFFFF_0000, 10);
        assert_eq!(f.get(0, 0), 0xFFFF_0000);
        assert_eq!(f.get(0, 1), 0xFFFF_0000);
        assert_eq!(f.get(2, 0), BLACK);
        assert_eq!(f.get(3, 0), WHITE); // not connected
        assert!(touched[2]);
        let o = outline(&i, (0, 0));
        assert_eq!(o.get(3, 0), BLACK);
        assert_eq!(fill(&i, (9, 9), 0, 1).0, i);
    }

    #[test]
    fn resize_keeps_size() {
        let i = img(&[BLACK, WHITE, 0, 0xFF80_8080], 2);
        assert_eq!(resize(&i, 2, 2, true, Interpolation::Bicubic), i);
        for f in [
            Interpolation::NearestNeighbor,
            Interpolation::Bilinear,
            Interpolation::Bicubic,
        ] {
            let r = resize(&i, 4, 4, true, f);
            assert_eq!((r.width(), r.height()), (4, 4));
            assert!(r.pixels().iter().all(|p| p >> 24 == 0xFF));
            let k = resize(&i, 4, 4, false, f);
            assert!(k.pixels().iter().any(|p| p >> 24 != 0xFF));
        }
        // nearest: the nearest pixel, no blending (DIV-108)
        let n = resize(&i, 4, 4, true, Interpolation::NearestNeighbor);
        assert_eq!(n.pixels()[..4], [BLACK, BLACK, WHITE, WHITE]);
        assert_eq!(
            resize(&Image::new(0, 0, 0), 2, 2, false, Interpolation::Bilinear).get(0, 0),
            0
        );
        assert_eq!(
            resize(&Image::new(0, 0, 0), 2, 2, false, Interpolation::NearestNeighbor).get(0, 0),
            0
        );
    }
}
