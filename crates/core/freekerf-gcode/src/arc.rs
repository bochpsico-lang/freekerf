//! Arc geometry for G2/G3 (`G2G3Helper`): center, radius, angular width and
//! bounding box. Computed in `f64` as in LaserGRBL.
//!
//! With `R` the center is assumed at the chord midpoint (FINDINGS F-12, kept:
//! DIV-003), which is only right for half circles.

use std::f64::consts::PI;

use crate::command::Command;
use crate::number::{f64_from_decimal, round_half_even};
use crate::state::MachineState;

/// An axis-aligned rectangle (`System.Windows.Rect`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left.
    pub x: f64,
    /// Bottom (smaller Y).
    pub y: f64,
    /// Width (never negative).
    pub width: f64,
    /// Height (never negative).
    pub height: f64,
}

impl Rect {
    fn from_points(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x: x0.min(x1), y: y0.min(y1), width: (x1 - x0).abs(), height: (y1 - y0).abs() }
    }
}

/// Geometry of one arc move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArcHelper {
    /// Center X.
    pub center_x: f64,
    /// Center Y.
    pub center_y: f64,
    /// Radius.
    pub radius: f64,
    /// Clockwise (G2).
    pub cw: bool,
    /// Start angle in radians, [0, 2π).
    pub start_angle: f64,
    /// End angle in radians, [0, 2π).
    pub end_angle: f64,
    /// Signed angular width.
    pub angular_width: f64,
    /// Bounding box of the arc.
    pub bbox: Rect,
}

impl ArcHelper {
    /// Arc of `cmd`, from the previous position of `state` to its current one.
    #[must_use]
    pub fn new(state: &MachineState, cmd: &Command) -> Self {
        let cw = state.is_g2();
        let ax = f64_from_decimal(state.x().previous());
        let ay = f64_from_decimal(state.y().previous());
        let bx = f64_from_decimal(state.x().value());
        let by = f64_from_decimal(state.y().value());
        let (ox, oy) = if cmd.get('R').is_some() {
            ((ax + bx) / 2.0 - ax, (ay + by) / 2.0 - ay)
        } else {
            (cmd.get('I').map_or(0.0, f64_from_decimal), cmd.get('J').map_or(0.0, f64_from_decimal))
        };
        let center_x = ax + ox;
        let center_y = ay + oy;
        let radius = (ox * ox + oy * oy).sqrt();
        let start_angle = angle(center_x, center_y, ax, ay);
        let end_angle = angle(center_x, center_y, bx, by);
        let angular_width = angular_distance(start_angle, end_angle, cw);
        #[allow(clippy::float_cmp)] // exact comparison, as in the original
        let full_circle = ax == bx && ay == by;
        let bbox = if full_circle {
            Rect { x: center_x - radius, y: center_y - radius, width: 2.0 * radius, height: 2.0 * radius }
        } else if cw {
            arc_bbox(end_angle, start_angle, radius, center_x, center_y)
        } else {
            arc_bbox(start_angle, end_angle, radius, center_x, center_y)
        };
        Self { center_x, center_y, radius, cw, start_angle, end_angle, angular_width, bbox }
    }

    /// Signed length (angular width × radius).
    #[must_use]
    pub fn length(&self) -> f64 {
        self.angular_width * self.radius
    }

    /// Length of the arc.
    #[must_use]
    pub fn abs_length(&self) -> f64 {
        self.length().abs()
    }
}

#[allow(clippy::float_cmp)] // exact comparisons, as in the original
fn angle(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    if x1 == x2 && y1 == y2 {
        0.0
    } else if x1 == x2 {
        if y1 > y2 { PI / 2.0 + PI } else { PI / 2.0 }
    } else if y1 == y2 {
        if x1 > x2 { PI } else { 0.0 }
    } else {
        let a = ((y2 - y1) / (x2 - x1)).abs().atan();
        if x1 > x2 && y1 < y2 {
            PI - a
        } else if x1 > x2 && y1 > y2 {
            a + PI
        } else if x1 < x2 && y1 > y2 {
            2.0 * PI - a
        } else {
            a
        }
    }
}

fn angular_distance(a: f64, b: f64, cw: bool) -> f64 {
    if cw {
        if b >= a { b - 2.0 * PI - a } else { b - a }
    } else {
        -(if a >= b { a - 2.0 * PI - b } else { a - b })
    }
}

/// Quadrant (1-4) of an angle.
#[must_use]
pub fn quadrant(angle: f64) -> usize {
    let a = angle % (2.0 * PI);
    if (0.0..PI / 2.0).contains(&a) {
        1
    } else if (PI / 2.0..PI).contains(&a) {
        2
    } else if (PI..PI * 3.0 / 2.0).contains(&a) {
        3
    } else {
        4
    }
}

/// Bounding box of the arc from `start` to `end` counter-clockwise (Oleg Petrochenko).
#[must_use]
pub fn arc_bbox(start: f64, end: f64, r: f64, cx: f64, cy: f64) -> Rect {
    let sq = quadrant(start) - 1;
    let eq = quadrant(end) - 1;
    let (sx, sy) = (round_half_even(r * start.cos(), 14), round_half_even(r * start.sin(), 14));
    let (ex, ey) = (round_half_even(r * end.cos(), 14), round_half_even(r * end.sin(), 14));
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (sx, sy, sx, sy);
    if max_x < ex {
        max_x = ex;
    }
    if max_y < ey {
        max_y = ey;
    }
    if min_x > ex {
        min_x = ex;
    }
    if min_y > ey {
        min_y = ey;
    }
    let x_max = [
        [max_x, r, r, r],
        [max_x, max_x, r, r],
        [max_x, max_x, max_x, r],
        [max_x, max_x, max_x, max_x],
    ];
    let y_max = [
        [max_y, max_y, max_y, max_y],
        [r, max_y, r, r],
        [r, max_y, max_y, r],
        [r, max_y, max_y, max_y],
    ];
    let x_min = [
        [min_x, -r, min_x, min_x],
        [min_x, min_x, min_x, min_x],
        [-r, -r, min_x, -r],
        [-r, -r, min_x, min_x],
    ];
    let y_min = [
        [min_y, -r, -r, min_y],
        [min_y, min_y, -r, min_y],
        [min_y, min_y, min_y, min_y],
        [-r, -r, -r, min_y],
    ];
    let mut rect = Rect::from_points(x_min[eq][sq], y_min[eq][sq], x_max[eq][sq], y_max[eq][sq]);
    rect.x += cx;
    rect.y += cy;
    rect
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TimingConfig;

    fn arc(lines: &[&str]) -> ArcHelper {
        let mut st = MachineState::default();
        let t = TimingConfig::default();
        for l in lines {
            st.analyze(&Command::new(l), Some(&t));
        }
        *st.last_arc().unwrap()
    }

    #[test]
    fn half_circle_ccw_and_cw() {
        let a = arc(&["G1 F100", "G3 X10 Y0 I5 J0"]);
        assert!((a.abs_length() - 5.0 * PI).abs() < 1e-9);
        // counter-clockwise from 180° to 0° passes below the center
        assert!((a.bbox.y + 5.0).abs() < 1e-9 && (a.bbox.height - 5.0).abs() < 1e-9);
        let a = arc(&["G1 F100", "G2 X10 Y0 I5 J0"]);
        assert!(a.cw);
        assert!(a.bbox.y.abs() < 1e-9 && (a.bbox.height - 5.0).abs() < 1e-9);
    }

    #[test]
    fn full_circle_bbox() {
        let a = arc(&["G0 X40 Y40", "G1 F100", "G2 X40 Y40 I0 J-10"]);
        assert_eq!(a.bbox, Rect { x: 30.0, y: 20.0, width: 20.0, height: 20.0 });
        assert!((a.abs_length() - 20.0 * PI).abs() < 1e-9);
    }

    #[test]
    fn r_arcs_use_the_chord_midpoint() {
        let a = arc(&["G1 F100", "G2 X10 Y10 R10"]);
        assert!((a.center_x - 5.0).abs() < 1e-12 && (a.center_y - 5.0).abs() < 1e-12);
    }

    #[test]
    fn angles_and_quadrants() {
        assert_eq!(angle(0.0, 0.0, 0.0, 0.0), 0.0);
        assert_eq!(angle(0.0, 0.0, 0.0, 1.0), PI / 2.0);
        assert_eq!(angle(0.0, 1.0, 0.0, 0.0), 1.5 * PI);
        assert_eq!(angle(1.0, 0.0, 0.0, 0.0), PI);
        assert!((angle(0.0, 0.0, -1.0, 1.0) - 0.75 * PI).abs() < 1e-12);
        assert!((angle(0.0, 0.0, -1.0, -1.0) - 1.25 * PI).abs() < 1e-12);
        assert!((angle(0.0, 0.0, 1.0, -1.0) - 1.75 * PI).abs() < 1e-12);
        assert_eq!([quadrant(0.1), quadrant(2.0), quadrant(4.0), quadrant(5.0)], [1, 2, 3, 4]);
        assert_eq!(angular_distance(1.0, 1.0, true), -2.0 * PI);
        assert_eq!(angular_distance(1.0, 0.5, true), -0.5);
        assert_eq!(angular_distance(1.0, 2.0, false), 1.0);
        assert_eq!(angular_distance(1.0, 1.0, false), 2.0 * PI);
    }
}
