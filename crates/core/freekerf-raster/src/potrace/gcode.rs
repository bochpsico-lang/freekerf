//! Traced paths to G-code (`CsPotraceExportGCODE.cs`): a rapid move to the start
//! of every path, the laser on, lines as `G1` and Béziers as biarcs (`G2`/`G3`),
//! the laser off.

use freekerf_gcode::dotnet;

use super::biarc::{Arc, CubicBezier, Vector2, approx_cubic_bezier};
use super::{Curve, CurveKind};

/// How a path is written.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportOptions {
    /// X offset, mm (single precision, as LaserGRBL keeps it).
    pub offset_x: f32,
    /// Y offset, mm.
    pub offset_y: f32,
    /// Pixels per mm.
    pub scale: f64,
    /// Written after the rapid move to a path (laser on, or its power).
    pub laser_on: String,
    /// Written at the end of a path.
    pub laser_off: String,
    /// The rapid move: `G0`, or `G1` when G0 is disabled.
    pub skip_cmd: String,
}

/// `formatnumber`: pixels to mm with up to 3 decimals.
fn number(value: f64, scale: f64) -> String {
    let num = value / scale;
    if num.is_nan() {
        "0".to_owned()
    } else {
        dotnet::double_format(num, "0.###")
    }
}

fn line_to(x: f64, y: f64, o: &ExportOptions, ox: f64, oy: f64) -> String {
    format!("G1 X{} Y{}", number(x + ox, o.scale), number(y + oy, o.scale))
}

/// `CasoLimite`: arcs too small or too flat to be written as arcs.
fn degenerate(arc: &Arc, scale: f64) -> bool {
    let ll = arc.linear_length();
    arc.r == 0.0 || ll == 0.0 || f64::from(ll) / scale <= 0.5 || arc.r / ll > 1000.0
}

fn arc_gcode(arc: &Arc, o: &ExportOptions, ox: f64, oy: f64) -> String {
    if degenerate(arc, o.scale) {
        return line_to(f64::from(arc.p2.x), f64::from(arc.p2.y), o, ox, oy);
    }
    format!(
        "G{} X{} Y{} I{} J{}",
        if arc.is_clockwise() { 3 } else { 2 },
        number(f64::from(arc.p2.x) + ox, o.scale),
        number(f64::from(arc.p2.y) + oy, o.scale),
        number(f64::from(arc.c.x - arc.p1.x), o.scale),
        number(f64::from(arc.c.y - arc.p1.y), o.scale),
    )
}

#[allow(clippy::cast_possible_truncation)]
fn v2(p: super::DPoint) -> Vector2 {
    Vector2::new(p.x as f32, p.y as f32)
}

fn segment(c: &Curve, o: &ExportOptions, ox: f64, oy: f64, out: &mut Vec<String>) {
    if c.linear_length().is_nan() {
        return;
    }
    match c.kind {
        CurveKind::Line => out.push(line_to(c.b.x, c.b.y, o, ox, oy)),
        CurveKind::Bezier => {
            let cb = CubicBezier {
                p1: v2(c.a),
                c1: v2(c.control_a),
                c2: v2(c.control_b),
                p2: v2(c.b),
            };
            match approx_cubic_bezier(cb, 5.0, 2.0) {
                Some(biarcs) => {
                    for ba in biarcs {
                        for arc in [ba.a1, ba.a2] {
                            if !arc.length().is_nan() && !arc.linear_length().is_nan() {
                                out.push(arc_gcode(&arc, o, ox, oy));
                            }
                        }
                    }
                }
                None => out.push(line_to(c.b.x, c.b.y, o, ox, oy)),
            }
        }
    }
}

/// `Potrace.Export2GCode`: the paths in order, coordinates in pixels divided by
/// `scale`, plus the offset.
#[must_use]
pub fn export(paths: &[Vec<Curve>], o: &ExportOptions) -> Vec<String> {
    let ox = f64::from(o.offset_x) * o.scale;
    let oy = f64::from(o.offset_y) * o.scale;
    let mut out = Vec::new();
    for curves in paths {
        let Some(first) = curves.first() else {
            continue;
        };
        out.push(format!(
            "{} X{} Y{}",
            o.skip_cmd,
            number(first.a.x + ox, o.scale),
            number(first.a.y + oy, o.scale)
        ));
        out.push(o.laser_on.clone());
        for c in curves {
            segment(c, o, ox, oy, &mut out);
        }
        out.push(o.laser_off.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::DPoint;
    use super::*;

    fn opts() -> ExportOptions {
        ExportOptions {
            offset_x: 1.0,
            offset_y: 0.0,
            scale: 10.0,
            laser_on: "S1000".into(),
            laser_off: "S0".into(),
            skip_cmd: "G0".into(),
        }
    }

    #[test]
    fn lines_and_arcs() {
        let p = |x, y| DPoint::new(x, y);
        let square = vec![
            Curve::line(p(0.0, 0.0), p(10.0, 0.0)),
            Curve::line(p(10.0, 0.0), p(10.0, 10.0)),
        ];
        let k = 55.228_47;
        let round = vec![Curve {
            kind: CurveKind::Bezier,
            a: p(100.0, 0.0),
            control_a: p(100.0, k),
            control_b: p(k, 100.0),
            b: p(0.0, 100.0),
        }];
        let lines = export(&[square, Vec::new(), round], &opts());
        assert_eq!(lines[..5], ["G0 X1 Y0", "S1000", "G1 X2 Y0", "G1 X2 Y1", "S0"]);
        assert!(lines[7].starts_with("G3 X"), "{lines:?}");
        assert_eq!(lines.last().unwrap(), "S0");
        assert_eq!(number(f64::NAN, 1.0), "0");
        let nan = Curve::line(p(f64::NAN, 0.0), p(1.0, 1.0));
        let mut out = Vec::new();
        segment(&nan, &opts(), 0.0, 0.0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn small_arcs_become_lines() {
        let arc = Arc {
            c: Vector2::new(0.0, 0.0),
            r: 1.0,
            start_angle: 0.0,
            sweep_angle: 1.0,
            p1: Vector2::new(1.0, 0.0),
            p2: Vector2::new(1.2, 0.5),
        };
        assert!(degenerate(&arc, 1.0));
        assert_eq!(arc_gcode(&arc, &opts(), 0.0, 0.0), "G1 X0.12 Y0.05");
        let flat = Arc {
            r: 5000.0,
            p2: Vector2::new(4.0, 0.0),
            ..arc
        };
        assert!(degenerate(&flat, 1.0));
        let ok = Arc {
            r: 10.0,
            p2: Vector2::new(10.0, 0.0),
            ..arc
        };
        assert!(!degenerate(&ok, 1.0));
        assert_eq!(arc_gcode(&ok, &opts(), 0.0, 0.0), "G3 X1 Y0 I-0.1 J0");
    }
}
