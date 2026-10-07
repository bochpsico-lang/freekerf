//! Dithering to black and white (`ImageTransform.DitherImage` with the Cyotek
//! error-diffusion classes, MIT): each pixel becomes black or white by its luminance,
//! and the error is spread to its neighbours.
//!
//! Kept as LaserGRBL does it (DIV-041): the error never reaches row 0 or column 0
//! (`offsetX > 0`), and the green and blue errors are swapped (no effect on grey
//! images). Random dithering is seeded by the caller, so the same image always gives
//! the same result (DIV-042; LaserGRBL seeds with the clock).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::image::{Image, argb, split};

/// Dithering algorithm.
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
    /// Sierra, two rows.
    Sierra2,
    /// Sierra, three rows.
    Sierra3,
    /// Sierra Lite.
    SierraLight,
    /// Stucki.
    Stucki,
}

struct Diffusion {
    matrix: &'static [&'static [i32]],
    divisor: i32,
    shift: bool,
}

const FLOYD_STEINBERG: Diffusion = Diffusion {
    matrix: &[&[0, 0, 7], &[3, 5, 1]],
    divisor: 4,
    shift: true,
};
const ATKINSON: Diffusion = Diffusion {
    matrix: &[&[0, 0, 1, 1], &[1, 1, 1, 0], &[0, 1, 0, 0]],
    divisor: 3,
    shift: true,
};
const BURKES: Diffusion = Diffusion {
    matrix: &[&[0, 0, 0, 8, 4], &[2, 4, 8, 4, 2]],
    divisor: 5,
    shift: true,
};
const JARVIS: Diffusion = Diffusion {
    matrix: &[&[0, 0, 0, 7, 5], &[3, 5, 7, 5, 3], &[1, 3, 5, 3, 1]],
    divisor: 48,
    shift: false,
};
const SIERRA2: Diffusion = Diffusion {
    matrix: &[&[0, 0, 0, 4, 3], &[1, 2, 3, 2, 1]],
    divisor: 4,
    shift: true,
};
const SIERRA3: Diffusion = Diffusion {
    matrix: &[&[0, 0, 0, 5, 3], &[2, 4, 5, 4, 2], &[0, 2, 3, 2, 0]],
    divisor: 5,
    shift: true,
};
const SIERRA_LITE: Diffusion = Diffusion {
    matrix: &[&[0, 0, 2], &[1, 1, 0]],
    divisor: 2,
    shift: true,
};
const STUCKI: Diffusion = Diffusion {
    matrix: &[&[0, 0, 0, 8, 4], &[2, 4, 8, 4, 2], &[1, 2, 4, 2, 1]],
    divisor: 42,
    shift: false,
};

/// `TransformPixel`: black or white (alpha kept) by the luminance.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn transform(c: (u8, u8, u8, u8)) -> (u8, u8, u8, u8) {
    let (a, r, g, b) = c;
    let gray = (0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b)) as u8;
    if gray < 128 {
        (a, 0, 0, 0)
    } else {
        (a, 255, 255, 255)
    }
}

/// A small deterministic generator for random dithering (`next(0, 255)`).
#[derive(Debug, Clone)]
struct Rng(u64);

impl Rng {
    fn next_below_255(&mut self) -> i32 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        i32::try_from(z % 255).unwrap_or(0)
    }
}

/// Dithers `img` with `mode` (`seed` is used by [`DitherMode::Random`] only).
#[must_use]
pub fn dither(img: &Image, mode: DitherMode, seed: u64) -> Image {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut data: Vec<(u8, u8, u8, u8)> = img.pixels().iter().map(|&p| split(p)).collect();
    let diffusion = match mode {
        DitherMode::FloydSteinberg => Some(&FLOYD_STEINBERG),
        DitherMode::Atkinson => Some(&ATKINSON),
        DitherMode::Burks => Some(&BURKES),
        DitherMode::Jarvis => Some(&JARVIS),
        DitherMode::Sierra2 => Some(&SIERRA2),
        DitherMode::Sierra3 => Some(&SIERRA3),
        DitherMode::SierraLight => Some(&SIERRA_LITE),
        DitherMode::Stucki => Some(&STUCKI),
        DitherMode::Random => None,
    };
    let mut rng = Rng(seed);
    for row in 0..h {
        for col in 0..w {
            let index = row * w + col;
            let current = data[index];
            let transformed = transform(current);
            data[index] = transformed;
            match diffusion {
                Some(d) => diffuse(&mut data, d, current, transformed, col, row, w, h),
                None => {
                    let (_, r, g, b) = current;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let gray = (0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b)) as u8;
                    data[index] = if i32::from(gray) > rng.next_below_255() {
                        (255, 255, 255, 255)
                    } else {
                        (255, 0, 0, 0)
                    };
                }
            }
        }
    }
    let pixels = data.into_iter().map(|(a, r, g, b)| argb(a, r, g, b)).collect();
    Image::from_pixels(img.width(), img.height(), pixels).unwrap_or_else(|_| img.clone())
}

#[allow(clippy::too_many_arguments)]
fn diffuse(
    data: &mut [(u8, u8, u8, u8)],
    d: &Diffusion,
    original: (u8, u8, u8, u8),
    transformed: (u8, u8, u8, u8),
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) {
    let red_error = i32::from(original.1) - i32::from(transformed.1);
    // the Cyotek code swaps the green and blue errors
    let blue_error = i32::from(original.2) - i32::from(transformed.2);
    let green_error = i32::from(original.3) - i32::from(transformed.3);
    let starting_offset = d.matrix[0]
        .iter()
        .position(|&c| c != 0)
        .map_or(0, |i| i as i64 - 1);
    let to_byte = |v: i32| u8::try_from(v.clamp(0, 255)).unwrap_or(0);
    for (row, coefficients) in d.matrix.iter().enumerate() {
        let offset_y = (y + row) as i64;
        for (col, &coefficient) in coefficients.iter().enumerate() {
            let offset_x = x as i64 + (col as i64 - starting_offset);
            if coefficient != 0
                && offset_x > 0
                && offset_x < width as i64
                && offset_y > 0
                && offset_y < height as i64
            {
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                let i = offset_y as usize * width + offset_x as usize;
                let (a, r, g, b) = data[i];
                let spread = |e: i32| {
                    if d.shift {
                        (e * coefficient) >> d.divisor
                    } else {
                        (e * coefficient) / d.divisor
                    }
                };
                data[i] = (
                    a,
                    to_byte(i32::from(r) + spread(red_error)),
                    to_byte(i32::from(g) + spread(green_error)),
                    to_byte(i32::from(b) + spread(blue_error)),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray(v: u8, w: u32, h: u32) -> Image {
        Image::new(w, h, argb(255, v, v, v))
    }

    #[test]
    fn every_mode_gives_black_and_white() {
        for m in [
            DitherMode::Atkinson,
            DitherMode::FloydSteinberg,
            DitherMode::Burks,
            DitherMode::Jarvis,
            DitherMode::Random,
            DitherMode::Sierra2,
            DitherMode::Sierra3,
            DitherMode::SierraLight,
            DitherMode::Stucki,
        ] {
            let d = dither(&gray(128, 8, 4), m, 7);
            assert!(
                d.pixels().iter().all(|&p| p == 0xFF00_0000 || p == 0xFFFF_FFFF),
                "{m:?}"
            );
            assert_eq!(dither(&gray(128, 8, 4), m, 7), d, "deterministic");
        }
    }

    #[test]
    fn first_row_and_column_get_no_error() {
        // a uniform mid-gray first row comes out all black (F-41)
        let d = dither(&gray(100, 6, 3), DitherMode::FloydSteinberg, 0);
        assert!((0..6).all(|x| d.get(x, 0) == 0xFF00_0000));
        assert!(d.pixels().contains(&0xFFFF_FFFF));
        assert_eq!(transform((7, 200, 200, 200)), (7, 255, 255, 255));
    }
}
