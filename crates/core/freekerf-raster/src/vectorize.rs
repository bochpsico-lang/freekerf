//! LaserGRBL's "vectorize" raster mode (`GrblFile.LoadImagePotrace`): the image is
//! traced with Potrace, optionally filled (hatch lines clipped by the outlines, or
//! a raster engraving of the inside), and the outlines are cut.

use freekerf_geometry::gdipath::{DEFAULT_FLATNESS, GdiPath};
use freekerf_geometry::hatch::{self, FillPattern};
use freekerf_geometry::{Point, order};

use crate::image::Image;
use crate::l2l::{Direction, L2lConf, L2lOptions};
use crate::potrace::gcode::{ExportOptions, export};
use crate::potrace::{Curve, DPoint, TraceOptions, trace};

/// How the traced shapes are filled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Filling {
    /// Outlines only (`Direction.None`).
    #[default]
    None,
    /// Hatch lines (`Direction.New*`).
    Vector(FillPattern),
    /// A Line2Line engraving of the filled shapes (`Horizontal`, `Vertical`,
    /// `Diagonal`).
    Raster(Direction),
}

/// Vectorize settings of the raster dialog.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PotraceOptions {
    /// Tracing parameters.
    pub trace: TraceOptions,
    /// Order the outlines nearest-neighbour ("optimize fast"); otherwise the inner
    /// ones first.
    pub optimize_fast: bool,
    /// Filling.
    pub filling: Filling,
}

/// The polylines of a filling as curve lists (`PotraceClipper.ToPotraceList`).
fn to_curves(lines: Vec<Vec<Point>>) -> Vec<Vec<Curve>> {
    let p = |q: Point| DPoint::new(q.x, q.y);
    lines
        .into_iter()
        .map(|l| l.windows(2).map(|w| Curve::line(p(w[0]), p(w[1]))).collect())
        .collect()
}

/// `PotraceClipper.BuildFilling`: hatch lines inside the shapes, in pixels.
#[allow(clippy::cast_possible_truncation)]
fn vector_filling(
    plist: &[Vec<Curve>],
    w: u32,
    h: u32,
    pattern: FillPattern,
    c: &L2lConf,
) -> Vec<Vec<Curve>> {
    let f = |p: DPoint| (p.x as f32, p.y as f32);
    let shapes: Vec<Vec<(f32, f32)>> = plist
        .iter()
        .map(|curves| {
            let mut gp = GdiPath::new();
            for cv in curves {
                match cv.kind {
                    crate::potrace::CurveKind::Line => gp.add_line(f(cv.a), f(cv.b)),
                    crate::potrace::CurveKind::Bezier => {
                        gp.add_bezier(f(cv.a), f(cv.control_a), f(cv.control_b), f(cv.b));
                    }
                }
            }
            gp.flatten(DEFAULT_FLATNESS)
        })
        .collect();
    let lines =
        hatch::fill_flat(&shapes, f64::from(w), f64::from(h), pattern, c.res / c.fres).unwrap_or_default();
    to_curves(lines)
}

/// `OptimizePaths` on curve lists: by the first and last point of every path.
fn ends(list: &[Vec<Curve>]) -> Vec<(Point, Point)> {
    list.iter()
        .map(|l| {
            let (a, b) = (
                l.first().map_or(DPoint::default(), |c| c.a),
                l.last().map_or(DPoint::default(), |c| c.b),
            );
            (Point::new(a.x, a.y), Point::new(b.x, b.y))
        })
        .collect()
}

/// `LoadImagePotrace`: the whole job. `support_pwm` is the machine's `Support
/// Hardware PWM` setting (the outlines are cut with `S` power or with the laser
/// switched on and off).
#[must_use]
pub fn load_image_potrace(
    img: &Image,
    p: &PotraceOptions,
    c: &L2lConf,
    opts: &L2lOptions,
    support_pwm: bool,
) -> Vec<String> {
    let skip = if opts.disable_g0 { "G1" } else { "G0" };
    let flipped = img.flipped_y();
    let mut plist = trace(&flipped, &p.trace);
    let mut list = Vec::new();

    let flist = match p.filling {
        Filling::Vector(pattern) => {
            let f = vector_filling(&plist, flipped.width(), flipped.height(), pattern, c);
            Some(order::apply(&f, &order::nearest_neighbour_blocks(&ends(&f))))
        }
        _ => None,
    };
    if let Filling::Raster(direction) = p.filling {
        list.extend(crate::render::raster_filling(
            &plist, &flipped, direction, c, opts,
        ));
    }

    list.push(if support_pwm {
        format!("{} S0", c.laser_on)
    } else {
        format!("{} S{}", c.laser_off, opts.max_pwm)
    });
    let export_opts = |speed: i32| {
        let (on, off) = if support_pwm {
            (format!("S{}", c.max_power), "S0".to_owned())
        } else {
            (c.laser_on.clone(), c.laser_off.clone())
        };
        (
            format!("F{speed}"),
            ExportOptions {
                offset_x: c.offset_x,
                offset_y: c.offset_y,
                scale: c.res,
                laser_on: on,
                laser_off: off,
                skip_cmd: skip.to_owned(),
            },
        )
    };
    if let Some(flist) = flist {
        let (feed, o) = export_opts(c.mark_speed);
        list.push(feed);
        list.extend(export(&flist, &o));
    }
    if p.optimize_fast {
        plist = order::apply(&plist, &order::nearest_neighbour(&ends(&plist)));
    } else {
        // Potrace gives the outer outlines first; cutting the inner ones first is better
        plist.reverse();
    }
    let (feed, o) = export_opts(c.border_speed);
    list.push(feed);
    list.extend(export(&plist, &o));
    if support_pwm {
        list.push(c.laser_off.clone());
    }
    list
}
