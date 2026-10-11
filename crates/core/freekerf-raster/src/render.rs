//! Drawing traced shapes into a bitmap, for the raster filling of the vectorize
//! mode. LaserGRBL draws them with GDI+ (`Potrace.Export2GDIPlus`: antialiased
//! even-odd fill, then a white outline `inset` pixels wide so the engraving stops a
//! little before the border); this uses tiny-skia, so the pixels are close to, not
//! equal to, GDI+'s (ADR 0008).

use tiny_skia::{FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::image::{Image, argb};
use crate::l2l::{Direction, L2lConf, L2lOptions, image_line_to_line};
use crate::ops::{self, Interpolation};
use crate::potrace::{Curve, CurveKind};

/// libgdiplus puts pixel centres on integer coordinates when it antialiases (the
/// reference drawings are half a pixel off tiny-skia's).
const HALF_PIXEL: Transform = Transform {
    sx: 1.0,
    kx: 0.0,
    ky: 0.0,
    sy: 1.0,
    tx: 0.5,
    ty: 0.5,
};

/// The shapes filled black on a transparent `w × h` bitmap, outlined in white.
#[allow(clippy::cast_possible_truncation)]
fn draw(paths: &[Vec<Curve>], w: u32, h: u32, inset: f64) -> Image {
    let Some(mut pixmap) = Pixmap::new(w, h) else {
        return Image::new(w, h, 0);
    };
    let mut pb = PathBuilder::new();
    for curves in paths {
        let Some(first) = curves.first() else {
            continue;
        };
        pb.move_to(first.a.x as f32, first.a.y as f32);
        for c in curves {
            match c.kind {
                CurveKind::Line => pb.line_to(c.b.x as f32, c.b.y as f32),
                CurveKind::Bezier => pb.cubic_to(
                    c.control_a.x as f32,
                    c.control_a.y as f32,
                    c.control_b.x as f32,
                    c.control_b.y as f32,
                    c.b.x as f32,
                    c.b.y as f32,
                ),
            }
        }
    }
    if let Some(path) = pb.finish() {
        let mut black = Paint::default();
        black.set_color_rgba8(0, 0, 0, 255);
        black.anti_alias = true;
        pixmap.fill_path(&path, &black, FillRule::EvenOdd, HALF_PIXEL, None);
        if inset > 0.0 {
            let mut white = Paint::default();
            white.set_color_rgba8(255, 255, 255, 255);
            white.anti_alias = true;
            let stroke = Stroke {
                width: inset as f32,
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                miter_limit: 10.0,
                dash: None,
            };
            pixmap.stroke_path(&path, &white, &stroke, HALF_PIXEL, None);
        }
    }
    let pixels = pixmap
        .pixels()
        .iter()
        .map(|p| crate::gdi::unpremultiply(argb(p.alpha(), p.red(), p.green(), p.blue())))
        .collect();
    Image::from_pixels(w, h, pixels).unwrap_or_else(|_| Image::new(w, h, 0))
}

/// The raster filling part of `LoadImagePotrace`: the shapes drawn at the image
/// size, resized to the filling resolution and engraved line by line.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn raster_filling(
    paths: &[Vec<Curve>],
    img: &Image,
    direction: Direction,
    c: &L2lConf,
    opts: &L2lOptions,
) -> Vec<String> {
    let inset = (c.res / c.fres).max(1.0);
    let drawn = draw(paths, img.width(), img.height(), inset);
    let nw = (f64::from(img.width()) * c.fres / c.res) as u32 + 1;
    let nh = (f64::from(img.height()) * c.fres / c.res) as u32 + 1;
    let resampled = ops::resize(&drawn, nw, nh, true, Interpolation::Bicubic);
    let mut list = vec![if c.pwm {
        format!("{} S0", c.laser_on)
    } else {
        format!("{} S{}", c.laser_off, opts.max_pwm)
    }];
    list.push(format!("F{}", c.mark_speed));
    let conf = L2lConf {
        vector_filling: true,
        direction,
        ..c.clone()
    };
    list.extend(image_line_to_line(&resampled, &conf, opts));
    list.push(c.laser_off.clone());
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::potrace::{TraceOptions, trace};

    /// Rows 1, 3 and 7 of libgdiplus' drawing of a 3 px ring (16×16, inset 2), as
    /// LaserGRBL's `Export2GDIPlus` makes it on Mono.
    const GDIPLUS_ROWS: [(u32, &str); 3] = [
        (
            1,
            "00000000 40FFFFFF 80FFFFFF 80FFFFFF 80FFFFFF 80FFFFFF 80FFFFFF 80FFFFFF 80FFFFFF 80FFFFFF 80FFFFFF 80FFFFFF 80FFFFFF 80FFFFFF 80FFFFFF 40FFFFFF",
        ),
        (
            3,
            "00000000 80FFFFFF FFFFFFFF FFC0C0C0 FF808080 FF808080 FF808080 FF808080 FF808080 FF808080 FF808080 FF808080 FF808080 FFC0C0C0 FFFFFFFF 80FFFFFF",
        ),
        (
            7,
            "00000000 80FFFFFF FFFFFFFF FF808080 FF808080 FFFFFFFF 80FFFFFF 00000000 00000000 00000000 80FFFFFF FFFFFFFF FF808080 FF808080 FFFFFFFF 80FFFFFF",
        ),
    ];

    #[test]
    fn ring_drawn_like_gdiplus() {
        let rows = [
            "................",
            "................",
            "..############..",
            "..############..",
            "..############..",
            "..###......###..",
            "..###......###..",
            "..###......###..",
            "..###......###..",
            "..###......###..",
            "..###......###..",
            "..############..",
            "..############..",
            "..############..",
            "................",
            "................",
        ];
        let px: Vec<u32> = rows
            .iter()
            .flat_map(|r| {
                r.chars()
                    .map(|c| if c == '#' { 0xFF00_0000 } else { 0xFFFF_FFFF })
            })
            .collect();
        let img = Image::from_pixels(16, 16, px).unwrap().flipped_y();
        let d = draw(&trace(&img, &TraceOptions::default()), 16, 16, 2.0);
        for (y, row) in GDIPLUS_ROWS {
            for (x, tok) in row.split(' ').enumerate() {
                let want = crate::image::split(u32::from_str_radix(tok, 16).unwrap());
                let got = crate::image::split(d.get(x as u32, y));
                let close = |a: u8, b: u8| a.abs_diff(b) <= 1;
                assert!(
                    close(want.0, got.0)
                        && close(want.1, got.1)
                        && close(want.2, got.2)
                        && close(want.3, got.3),
                    "({x}, {y}): {got:?} vs {want:?}"
                );
            }
        }
        assert_eq!(draw(&[], 0, 0, 1.0).width(), 0);
    }
}
