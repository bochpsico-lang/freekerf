//! Raster images for laser jobs: decoding, the adjustments of LaserGRBL's raster
//! dialog (grey conversion, brightness, contrast, threshold, inversion, white
//! removal, flood fill, outline, resizing), dithering, and line-to-line engraving.
//!
//! Pixel operations follow LaserGRBL (`RasterConverter/ImageTransform.cs`, the Cyotek
//! dithering classes, `GrblFile.LoadImageL2L`) so the G-code is the same; the GDI+
//! operations (grey matrices, threshold, resize) are computed as libgdiplus computes
//! them where that is plain arithmetic, and compared with a tolerance where it is not
//! (ADR 0008). Core layer: no I/O.
//!
//! ```
//! use freekerf_raster::image::{Image, BLACK, WHITE};
//! use freekerf_raster::l2l::{load_image_l2l, L2lConf, L2lOptions};
//!
//! let img = Image::from_pixels(3, 1, vec![WHITE, BLACK, WHITE]).unwrap();
//! let gcode = load_image_l2l(&img, &L2lConf::default(), &L2lOptions::default());
//! assert_eq!(gcode[0], "G0 X0 Y0 F1000");
//! ```

pub mod centerline;
pub mod dither;
pub mod gdi;
pub mod image;
pub mod l2l;
pub mod ops;
pub mod potrace;
mod render;
pub mod vectorize;

/// Errors of raster operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RasterError {
    /// Pixel data that does not match the size.
    #[error("bad image size: {0}")]
    Size(String),
    /// Not a supported image file.
    #[error("cannot decode the image: {0}")]
    Decode(String),
}
