//! Vectorization of black and white bitmaps with Potrace, as LaserGRBL's
//! "vectorize" raster mode does it (`GrblFile.LoadImagePotrace`).
//!
//! [`trace`] ports CsPotrace (C#, after potrace.js, after Peter Selinger's Potrace,
//! GPL-2.0-or-later); [`gcode`] writes the outlines as LaserGRBL does, Béziers as
//! biarcs (`G2`/`G3`) with the `BezierToBiarc` algorithm; ADR 0009.

pub mod biarc;
pub mod gcode;
mod trace;

pub use trace::trace;

/// A point of a traced path, in pixels.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DPoint {
    /// x.
    pub x: f64,
    /// y.
    pub y: f64,
}

impl DPoint {
    /// A point.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Line or cubic Bézier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveKind {
    /// A straight line from `a` to `b`.
    Line,
    /// A cubic Bézier.
    Bezier,
}

/// One segment of a traced path (`CsPotrace.Curve`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    /// Line or Bézier.
    pub kind: CurveKind,
    /// Start point.
    pub a: DPoint,
    /// First control point (`a` for a line).
    pub control_a: DPoint,
    /// Second control point (`b` for a line).
    pub control_b: DPoint,
    /// End point.
    pub b: DPoint,
}

impl Curve {
    /// A line from `a` to `b`.
    #[must_use]
    pub const fn line(a: DPoint, b: DPoint) -> Self {
        Self {
            kind: CurveKind::Line,
            a,
            control_a: a,
            control_b: b,
            b,
        }
    }

    /// `LinearLenght`: the distance from `a` to `b`.
    #[must_use]
    pub fn linear_length(&self) -> f64 {
        let (dx, dy) = (self.b.x - self.a.x, self.b.y - self.a.y);
        (dx * dx + dy * dy).sqrt()
    }
}

/// Tracing parameters (LaserGRBL's dialog values).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceOptions {
    /// Areas of at most this many pixels are ignored ("spot removal"; 2 when off).
    pub turdsize: i32,
    /// Corner threshold ("smoothing"; 0 when off: every vertex is a corner).
    pub alphamax: f64,
    /// Join Béziers when possible ("optimize").
    pub curve_optimizing: bool,
    /// Tolerance of the joining (0.2 when off).
    pub opttolerance: f64,
}

impl Default for TraceOptions {
    /// LaserGRBL's values with every option off.
    fn default() -> Self {
        Self {
            turdsize: 2,
            alphamax: 0.0,
            curve_optimizing: false,
            opttolerance: 0.2,
        }
    }
}
