//! GDI+ image drawing as LaserGRBL gets it on Mono's libgdiplus: `DrawImage`
//! scaling through Cairo and pixman, and libgdiplus' premultiplied alpha tables.
//!
//! libgdiplus sets the requested interpolation on a pattern it never uses, so every
//! scaled draw uses Cairo's default `CAIRO_FILTER_GOOD`: a box filter as wide as
//! the scale factor (a bilinear-like filter below 4/3), computed by pixman as a
//! separable convolution in 16.16 fixed point, outside pixels transparent. Ported
//! from libgdiplus 6.1 (`image.c`, MIT), Cairo 1.18 (`cairo-image-source.c`,
//! `cairo-matrix.c`, LGPL-2.1 or MPL-1.1) and pixman 0.42 (`pixman-fast-path.c`,
//! `pixman-matrix.c`, MIT), so the results are libgdiplus' to the last bit
//! (ADR 0008).

use crate::image::{Argb, Image, argb, split};

/// `pre_multiplied_table_reverse[color][alpha]` of libgdiplus.
static UNPREMULTIPLY: &[u8; 65536] = include_bytes!("gdi_unpremultiply.bin");

/// `pre_multiplied_table[color][alpha]`: `(c · a + 127) / 255`.
#[allow(clippy::cast_possible_truncation)]
const fn premultiply_channel(c: u8, a: u8) -> u8 {
    ((c as u32 * a as u32 + 127) / 255) as u8
}

/// A pixel as libgdiplus hands it to Cairo (premultiplied when not opaque).
#[must_use]
pub fn premultiply(p: Argb) -> Argb {
    let (a, r, g, b) = split(p);
    if a == 255 {
        p
    } else {
        argb(
            a,
            premultiply_channel(r, a),
            premultiply_channel(g, a),
            premultiply_channel(b, a),
        )
    }
}

/// A premultiplied pixel back to the bitmap data (`GetPixel`), with libgdiplus'
/// table.
#[must_use]
pub fn unpremultiply(p: Argb) -> Argb {
    let (a, r, g, b) = split(p);
    if a == 255 {
        return p;
    }
    let u = |c: u8| UNPREMULTIPLY[usize::from(c) * 256 + usize::from(a)];
    argb(a, u(r), u(g), u(b))
}

/// `_cairo_fixed_16_16_from_double`: round to the nearest 1/65536, ties to even.
#[allow(clippy::cast_possible_truncation)]
fn fixed(d: f64) -> i32 {
    (d * 65536.0).round_ties_even() as i32
}

/// `box_kernel` of Cairo.
fn box_kernel(x: f64, r: f64) -> f64 {
    0.0f64.max(r.min(1.0).min(((r + 1.0) / 2.0 - x).min((r + 1.0) / 2.0 + x)))
}

#[allow(clippy::cast_possible_truncation)]
fn box_width(r: f64) -> i32 {
    if r < 1.0 { 2 } else { (r + 1.0).ceil() as i32 }
}

/// One axis of `create_separable_convolution` with the box kernel: the width,
/// the phase bits and `2^bits · width` weights.
#[allow(clippy::cast_possible_truncation)]
fn filter_axis(r: f64) -> (i32, i32, Vec<i32>) {
    let width = box_width(r);
    let mut subsample = 0;
    if width > 1 {
        while r * f64::from(1 << subsample) <= 128.0 {
            subsample += 1;
        }
    }
    let n_phases = 1 << subsample;
    let step = 1.0 / f64::from(n_phases);
    let w = width as usize;
    let mut out = Vec::with_capacity(n_phases as usize * w);
    for i in 0..n_phases {
        let frac = (f64::from(i) + 0.5) * step;
        let x1 = (frac - f64::from(width) / 2.0 - 0.5).ceil() - frac + 0.5;
        let mut p = vec![0i32; w];
        let mut total = 0.0;
        for (j, pj) in p.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let v = box_kernel(x1 + j as f64, r);
            total += v;
            *pj = (v * 65536.0) as i32;
        }
        let total = 1.0 / total;
        let mut new_total = 0i32;
        for pj in &mut p {
            *pj = (f64::from(*pj) * total) as i32;
            new_total += *pj;
        }
        p[w / 2] += 65536 - new_total;
        out.extend(p);
    }
    (width, subsample, out)
}

/// Cairo's `SCALING_EPSILON` (one 24.8 fixed point unit).
const SCALING_EPSILON: f64 = 1.0 / 256.0;

/// `pixman_transform_point_31_16_3d` for the axis-aligned scale used here.
fn transform(m: &[[i64; 3]; 3], v: [i64; 3]) -> [i64; 3] {
    let mut out = [0i64; 3];
    for (i, row) in m.iter().enumerate() {
        let mut hi = 0i64;
        let mut lo = 0i64;
        for k in 0..3 {
            hi += row[k] * (v[k] >> 16);
            lo += row[k] * (v[k] & 0xFFFF);
        }
        out[i] = hi + ((lo + 0x8000) >> 16);
    }
    out
}

/// The pixman matrix of a scale by `(sx, sy)`, with Cairo's adjustment that
/// keeps the centre `(xc, yc)` of the drawn area where Cairo's own matrix puts it.
fn pixman_matrix(sx: f64, sy: f64, xc: f64, yc: f64) -> [[i64; 3]; 3] {
    let mut m = [
        [i64::from(fixed(sx)), 0, 0],
        [0, i64::from(fixed(sy)), 0],
        [0, 0, 1 << 16],
    ];
    // `_cairo_matrix_has_unity_scale`: no adjustment when the determinant is ±1
    let det = sx * sy;
    if (det * det - 1.0).abs() < SCALING_EPSILON || det == 0.0 {
        return m;
    }
    for _ in 0..5 {
        let v = transform(&m, [i64::from(fixed(xc)), i64::from(fixed(yc)), 1 << 16]);
        #[allow(clippy::cast_precision_loss)]
        let (x, y) = (v[0] as f64 / 65536.0, v[1] as f64 / 65536.0);
        // back to user space with the inverse of Cairo's matrix, then the error
        // measured in pattern space
        let (x, y) = (x * (1.0 / sx) - xc, y * (1.0 / sy) - yc);
        let (dx, dy) = (fixed(x * sx), fixed(y * sy));
        m[0][2] -= i64::from(dx);
        m[1][2] -= i64::from(dy);
        if dx == 0 && dy == 0 {
            break;
        }
    }
    m
}

/// `Graphics.DrawImage(src, new Rectangle(0, 0, w, h))` on libgdiplus, onto a
/// white bitmap (`over_white`, LaserGRBL's `killalfa`) or replacing a transparent
/// one (`SourceCopy`). The result is unpremultiplied as `GetPixel` reads it.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]
pub fn draw_scaled(src: &Image, w: u32, h: u32, over_white: bool) -> Image {
    let (sw, sh) = (src.width(), src.height());
    let mut out = Image::new(w, h, 0);
    if sw == 0 || sh == 0 || w == 0 || h == 0 {
        return out;
    }
    let pre: Vec<Argb> = src.pixels().iter().map(|&p| premultiply(p)).collect();
    // `srcwidth / dstwidth` is a REAL (single precision) division in libgdiplus
    #[allow(clippy::cast_precision_loss)]
    let sx = f64::from(sw as f32 / w as f32);
    #[allow(clippy::cast_precision_loss)]
    let sy = f64::from(sh as f32 / h as f32);
    // CAIRO_FILTER_GOOD
    let clamp = |d: f64| {
        let d = d.min(16.0);
        if d < 1.0 / 0.75 { 1.0 } else { d }
    };
    let (cwidth, xbits, xparams) = filter_axis(clamp(sx));
    let (cheight, ybits, yparams) = filter_axis(clamp(sy));
    let m = pixman_matrix(sx, sy, f64::from(w) / 2.0, f64::from(h) / 2.0);
    let x_off = ((cwidth << 16) - 65536) >> 1;
    let y_off = ((cheight << 16) - 65536) >> 1;
    let (xshift, yshift) = (16 - xbits, 16 - ybits);
    let pixel = |x: i32, y: i32| -> Argb {
        if x < 0 || y < 0 || x >= sw as i32 || y >= sh as i32 {
            0
        } else {
            pre[y as usize * sw as usize + x as usize]
        }
    };
    for line in 0..h {
        let v = transform(&m, [1 << 15, (i64::from(line) << 16) + (1 << 15), 1 << 16]);
        let ux = m[0][0] as i32;
        let mut vx = v[0] as i32;
        let vy = v[1] as i32;
        for k in 0..w {
            let x = ((vx >> xshift) << xshift) + ((1 << xshift) >> 1);
            let y = ((vy >> yshift) << yshift) + ((1 << yshift) >> 1);
            let px = ((x & 0xffff) >> xshift) as usize;
            let py = ((y & 0xffff) >> yshift) as usize;
            let x1 = (x - 1 - x_off) >> 16;
            let y1 = (y - 1 - y_off) >> 16;
            let (mut sa, mut sr, mut sg, mut sb) = (0i32, 0i32, 0i32, 0i32);
            let ycoef = &yparams[py * cheight as usize..][..cheight as usize];
            let xcoef = &xparams[px * cwidth as usize..][..cwidth as usize];
            for (i, &fy) in ycoef.iter().enumerate() {
                if fy == 0 {
                    continue;
                }
                for (j, &fx) in xcoef.iter().enumerate() {
                    if fx == 0 {
                        continue;
                    }
                    let p = pixel(x1 + j as i32, y1 + i as i32);
                    let f = ((i64::from(fx) * i64::from(fy) + 0x8000) >> 16) as i32;
                    let (a, r, g, b) = split(p);
                    sr += i32::from(r) * f;
                    sg += i32::from(g) * f;
                    sb += i32::from(b) * f;
                    sa += i32::from(a) * f;
                }
            }
            let c = |t: i32| ((t + 0x8000) >> 16).clamp(0, 255) as u8;
            let (a, r, g, b) = (c(sa), c(sr), c(sg), c(sb));
            let p = if over_white {
                // OVER a white (premultiplied 0xFFFFFFFF) background
                let ia = 255 - u32::from(a);
                let add = |c: u8| (u32::from(c) + ia).min(255) as u8;
                argb(255, add(r), add(g), add(b))
            } else {
                unpremultiply(argb(a, r, g, b))
            };
            out.set(k, line, p);
            vx += ux;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplied_tables() {
        assert_eq!(premultiply(0x80FF_FFFF), 0x8080_8080);
        assert_eq!(premultiply(0xFF12_3456), 0xFF12_3456);
        assert_eq!(unpremultiply(0x8080_8080), 0x80FF_FFFF);
        assert_eq!(unpremultiply(0x0900_0300), 0x0900_5400); // libgdiplus' table: 84, not 85
        assert_eq!(unpremultiply(0xFF01_0203), 0xFF01_0203);
        assert_eq!(fixed(0.5), 32768);
        assert_eq!(fixed(1.0 / 131_072.0), 0); // tie to even
    }

    #[test]
    fn box_filter_weights() {
        let (w, bits, p) = filter_axis(1.0);
        assert_eq!((w, bits), (2, 8));
        assert_eq!(p.len(), 512);
        for phase in p.chunks(2) {
            assert_eq!(phase[0] + phase[1], 65536);
        }
        let (w, _, _) = filter_axis(1.777);
        assert_eq!(w, 3);
        assert_eq!(box_width(0.5), 2);
    }

    #[test]
    fn scaling_keeps_flat_colours_and_fades_borders() {
        let grey = Image::new(4, 4, 0xFF80_8080);
        let up = draw_scaled(&grey, 8, 8, true);
        assert_eq!(up.get(4, 4), 0xFF80_8080);
        // the transparent outside blends into the border, over white
        assert!(split(up.get(0, 0)).1 > 0x80);
        let copy = draw_scaled(&grey, 8, 8, false);
        assert!(split(copy.get(0, 0)).0 < 255);
        assert_eq!(copy.get(4, 4), 0xFF80_8080);
        let down = draw_scaled(&Image::new(16, 16, 0xFF00_0000), 9, 9, true);
        assert_eq!(down.get(4, 4), 0xFF00_0000);
        assert_eq!(draw_scaled(&grey, 0, 3, true).width(), 0);
    }
}
