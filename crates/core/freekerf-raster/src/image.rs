//! A bitmap: 32-bit ARGB pixels, not premultiplied, row by row from the top (the
//! `Format32bppArgb` of `System.Drawing`).

use crate::RasterError;

/// A pixel as `0xAARRGGBB`.
pub type Argb = u32;

/// The components of a pixel.
#[must_use]
pub const fn split(c: Argb) -> (u8, u8, u8, u8) {
    ((c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// A pixel from its components.
#[must_use]
pub const fn argb(a: u8, r: u8, g: u8, b: u8) -> Argb {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

/// Opaque white.
pub const WHITE: Argb = 0xFFFF_FFFF;
/// Opaque black.
pub const BLACK: Argb = 0xFF00_0000;
/// Transparent (`Color.Transparent`: white with alpha 0).
pub const TRANSPARENT: Argb = 0x00FF_FFFF;

/// A bitmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<Argb>,
}

impl Image {
    /// A bitmap of `width × height` pixels of `fill`.
    #[must_use]
    pub fn new(width: u32, height: u32, fill: Argb) -> Self {
        Self {
            width,
            height,
            pixels: vec![fill; (width as usize) * (height as usize)],
        }
    }

    /// A bitmap from its pixels (row by row).
    ///
    /// # Errors
    /// The pixel count does not match the size.
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<Argb>) -> Result<Self, RasterError> {
        if pixels.len() != (width as usize) * (height as usize) {
            return Err(RasterError::Size(format!(
                "{} pixels for {width}×{height}",
                pixels.len()
            )));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// A bitmap from 8-bit RGBA bytes.
    ///
    /// # Errors
    /// The byte count does not match the size.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Self, RasterError> {
        if rgba.len() != (width as usize) * (height as usize) * 4 {
            return Err(RasterError::Size(format!(
                "{} bytes for {width}×{height}",
                rgba.len()
            )));
        }
        let pixels = rgba
            .chunks_exact(4)
            .map(|p| argb(p[3], p[0], p[1], p[2]))
            .collect();
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Decodes a PNG, JPEG, BMP or GIF file.
    ///
    /// # Errors
    /// Not an image of those formats.
    pub fn decode(bytes: &[u8]) -> Result<Self, RasterError> {
        let img = image::load_from_memory(bytes).map_err(|e| RasterError::Decode(e.to_string()))?;
        let rgba = img.to_rgba8();
        Self::from_rgba(rgba.width(), rgba.height(), rgba.as_raw())
    }

    /// The image as a 32-bit RGBA PNG file.
    ///
    /// # Errors
    /// The encoder failed (empty image).
    pub fn encode_png(&self) -> Result<Vec<u8>, RasterError> {
        let mut out = Vec::new();
        image::RgbaImage::from_raw(self.width, self.height, self.to_rgba())
            .ok_or_else(|| RasterError::Size(format!("{}×{}", self.width, self.height)))?
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .map_err(|e| RasterError::Decode(e.to_string()))?;
        Ok(out)
    }

    /// The image as 8-bit RGBA bytes.
    #[must_use]
    pub fn to_rgba(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|&p| {
                let (a, r, g, b) = split(p);
                [r, g, b, a]
            })
            .collect()
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The pixels, row by row.
    #[must_use]
    pub fn pixels(&self) -> &[Argb] {
        &self.pixels
    }

    fn index(&self, x: u32, y: u32) -> usize {
        (y as usize) * (self.width as usize) + (x as usize)
    }

    /// The pixel at `(x, y)` (`GetPixel`).
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Argb {
        self.pixels[self.index(x, y)]
    }

    /// Sets the pixel at `(x, y)`.
    pub fn set(&mut self, x: u32, y: u32, c: Argb) {
        let i = self.index(x, y);
        self.pixels[i] = c;
    }

    /// Upside down (`RotateFlip(RotateNoneFlipY)`).
    #[must_use]
    pub fn flipped_y(&self) -> Self {
        let mut out = self.clone();
        let w = self.width as usize;
        for (dst, src) in out
            .pixels
            .chunks_exact_mut(w)
            .zip(self.pixels.chunks_exact(w).rev())
        {
            dst.copy_from_slice(src);
        }
        out
    }

    /// Mirrored left to right (`RotateNoneFlipX`).
    #[must_use]
    pub fn flipped_x(&self) -> Self {
        let mut out = self.clone();
        let w = self.width as usize;
        for row in out.pixels.chunks_exact_mut(w) {
            row.reverse();
        }
        out
    }

    /// Rotated 90° clockwise.
    #[must_use]
    pub fn rotated_cw(&self) -> Self {
        let (w, h) = (self.width, self.height);
        let mut out = Self::new(h, w, 0);
        for y in 0..h {
            for x in 0..w {
                out.set(h - 1 - y, x, self.get(x, y));
            }
        }
        out
    }

    /// Rotated 90° counterclockwise.
    #[must_use]
    pub fn rotated_ccw(&self) -> Self {
        let (w, h) = (self.width, self.height);
        let mut out = Self::new(h, w, 0);
        for y in 0..h {
            for x in 0..w {
                out.set(y, w - 1 - x, self.get(x, y));
            }
        }
        out
    }

    /// The rectangle `[x, y, w, h]` (clipped to the image).
    #[must_use]
    pub fn cropped(&self, x: u32, y: u32, w: u32, h: u32) -> Self {
        let x = x.min(self.width);
        let y = y.min(self.height);
        let w = w.min(self.width - x);
        let h = h.min(self.height - y);
        let mut out = Self::new(w, h, 0);
        for j in 0..h {
            for i in 0..w {
                out.set(i, j, self.get(x + i, y + j));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_and_transforms() {
        let img = Image::from_pixels(2, 3, vec![1, 2, 3, 4, 5, 6]).unwrap();
        assert_eq!(img.flipped_y().pixels(), [5, 6, 3, 4, 1, 2]);
        assert_eq!(img.flipped_x().pixels(), [2, 1, 4, 3, 6, 5]);
        let cw = img.rotated_cw();
        assert_eq!((cw.width(), cw.height()), (3, 2));
        assert_eq!(cw.pixels(), [5, 3, 1, 6, 4, 2]);
        assert_eq!(img.rotated_ccw().pixels(), [2, 4, 6, 1, 3, 5]);
        assert_eq!(cw.rotated_ccw(), img);
        assert_eq!(img.cropped(1, 1, 5, 5).pixels(), [4, 6]);
        assert!(Image::from_pixels(2, 2, vec![0]).is_err());
        let rgba = Image::from_rgba(1, 1, &[1, 2, 3, 4]).unwrap();
        assert_eq!(rgba.get(0, 0), 0x0401_0203);
        assert_eq!(rgba.to_rgba(), [1, 2, 3, 4]);
        assert!(Image::from_rgba(1, 1, &[0]).is_err());
        assert_eq!(split(argb(1, 2, 3, 4)), (1, 2, 3, 4));
    }

    #[test]
    fn decodes_png() {
        let mut png = Vec::new();
        {
            let img = image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 128]).unwrap();
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
        }
        let img = Image::decode(&png).unwrap();
        assert_eq!(img.pixels(), [0xFFFF_0000, 0x8000_00FF]);
        assert_eq!(Image::decode(&img.encode_png().unwrap()).unwrap(), img);
        assert!(Image::new(0, 0, 0).encode_png().is_err());
        assert!(Image::decode(b"not an image").is_err());
    }
}
