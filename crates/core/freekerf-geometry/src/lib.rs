//! Geometry for FreeKerf: points, affine transforms, paths made of lines, circular
//! arcs and cubic Bézier curves, flattening, arc fitting, path ordering, polygon
//! booleans and offsets, and the hatch fillings of LaserGRBL.
//!
//! Part of the core layer: no I/O, builds for `wasm32-unknown-unknown`. Where
//! LaserGRBL's output depends on the exact arithmetic (Bézier flattening, hatch
//! clipping, path ordering, Mono's `Matrix`), the code follows it operation by
//! operation so imported jobs match the `conformance` goldens. Clipping is a port of
//! Clipper 6.4.2 (Boost Software License, see `NOTICE` and ADR 0007).
//!
//! ```
//! use freekerf_geometry::{Path, Point, Segment};
//!
//! let mut p = Path::new(Point::new(1.0, 0.0));
//! p.segments.push(Segment::arc(Point::new(-1.0, 0.0), Point::ZERO, true));
//! assert!((p.length() - std::f64::consts::PI).abs() < 1e-12);
//! ```

pub mod arcfit;
pub mod bezier;
pub mod boolean;
pub mod clipper;
pub mod gdipath;
pub mod hatch;
mod matrix;
pub mod order;
mod path;
mod point;

pub use matrix::Matrix;
pub use path::{Path, Segment, arc_sweep};
pub use point::{Point, Rect};
