//! Approximation of cubic Béziers by biarcs (`CsPotrace/BezierToBiarc`, after
//! Szabolcs Dancsházi's algorithm), in single precision where the C# code uses
//! `float` (`Vector2`), so the arcs are LaserGRBL's to the last digit.

/// `Vector2`: a single precision vector.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vector2 {
    /// x.
    pub x: f32,
    /// y.
    pub y: f32,
}

impl Vector2 {
    /// A vector.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y)
    }

    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }

    /// `float * Vector2`.
    fn scale(k: f32, v: Self) -> Self {
        Self::new(k * v.x, k * v.y)
    }

    /// `Vector2 / float`: multiplied by the reciprocal.
    fn div(self, d: f32) -> Self {
        let num = 1.0 / d;
        Self::new(self.x * num, self.y * num)
    }

    /// `Length`: computed in double precision, rounded to single.
    #[allow(clippy::cast_possible_truncation)]
    fn length(self) -> f32 {
        let (x, y) = (f64::from(self.x), f64::from(self.y));
        (x * x + y * y).sqrt() as f32
    }

    fn distance(a: Self, b: Self) -> f32 {
        a.sub(b).length()
    }
}

/// `System.Numerics.Complex` as LaserGRBL's copy implements it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Complex {
    re: f64,
    im: f64,
}

impl Complex {
    const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    fn neg(self) -> Self {
        Self::new(-self.re, -self.im)
    }

    fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.im + o.im)
    }

    fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.im - o.im)
    }

    fn mul(self, o: Self) -> Self {
        Self::new(self.re * o.re - self.im * o.im, self.im * o.re + self.re * o.im)
    }

    fn div(self, o: Self) -> Self {
        let (r1, i1, r2, i2) = (self.re, self.im, o.re, o.im);
        if i2.abs() < r2.abs() {
            let num = i2 / r2;
            Self::new(
                (r1 + i1 * num) / (r2 + i2 * num),
                (i1 - r1 * num) / (r2 + i2 * num),
            )
        } else {
            let num1 = r2 / i2;
            Self::new(
                (i1 + r1 * num1) / (i2 + r2 * num1),
                (-r1 + i1 * num1) / (i2 + r2 * num1),
            )
        }
    }

    fn abs(self) -> f64 {
        if self.re.is_infinite() || self.im.is_infinite() {
            return f64::INFINITY;
        }
        let (n1, n2) = (self.re.abs(), self.im.abs());
        if n1 > n2 {
            let n3 = n2 / n1;
            return n1 * (1.0 + n3 * n3).sqrt();
        }
        if n2 == 0.0 {
            return n1;
        }
        let n4 = n1 / n2;
        n2 * (1.0 + n4 * n4).sqrt()
    }

    fn sqrt(self) -> Self {
        let magnitude = self.abs().sqrt();
        let phase = self.im.atan2(self.re) / 2.0;
        Self::new(magnitude * phase.cos(), magnitude * phase.sin())
    }
}

/// A cubic Bézier (`CubicBezier`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    /// Start.
    pub p1: Vector2,
    /// First control point.
    pub c1: Vector2,
    /// Second control point.
    pub c2: Vector2,
    /// End.
    pub p2: Vector2,
}

impl CubicBezier {
    #[allow(clippy::cast_possible_truncation)]
    fn point_at(&self, t: f32) -> Vector2 {
        let u = f64::from(1.0 - t);
        let td = f64::from(t);
        let k1 = u.powf(3.0) as f32;
        let k2 = (3.0 * u.powf(2.0) * td) as f32;
        // `3 * (1 - t)` is single precision in the C# expression
        let k3 = (f64::from(3.0 * (1.0 - t)) * td.powf(2.0)) as f32;
        let k4 = td.powf(3.0) as f32;
        Vector2::scale(k1, self.p1)
            .add(Vector2::scale(k2, self.c1))
            .add(Vector2::scale(k3, self.c2))
            .add(Vector2::scale(k4, self.p2))
    }

    fn split(&self, t: f32) -> (Self, Self) {
        let p0 = self.p1.add(Vector2::scale(t, self.c1.sub(self.p1)));
        let p1 = self.c1.add(Vector2::scale(t, self.c2.sub(self.c1)));
        let p2 = self.c2.add(Vector2::scale(t, self.p2.sub(self.c2)));
        let p01 = p0.add(Vector2::scale(t, p1.sub(p0)));
        let p12 = p1.add(Vector2::scale(t, p2.sub(p1)));
        let dp = p01.add(Vector2::scale(t, p12.sub(p01)));
        (
            Self {
                p1: self.p1,
                c1: p0,
                c2: p01,
                p2: dp,
            },
            Self {
                p1: dp,
                c1: p12,
                c2: p2,
                p2: self.p2,
            },
        )
    }

    fn inflexion_points(&self) -> (Complex, Complex) {
        let a = self.c1.sub(self.p1);
        let b = self.c2.sub(self.c1).sub(a);
        let c = self.p2.sub(self.c2).sub(a).sub(Vector2::scale(2.0, b));
        let ca = Complex::new(f64::from(b.x * c.y - b.y * c.x), 0.0);
        let cb = Complex::new(f64::from(a.x * c.y - a.y * c.x), 0.0);
        let cc = Complex::new(f64::from(a.x * b.y - a.y * b.x), 0.0);
        let four = Complex::new(4.0, 0.0);
        let two = Complex::new(2.0, 0.0);
        let disc = cb.mul(cb).sub(four.mul(ca).mul(cc)).sqrt();
        let t1 = cb.neg().add(disc).div(two.mul(ca));
        let disc = cb.mul(cb).sub(four.mul(ca).mul(cc)).sqrt();
        let t2 = cb.neg().sub(disc).div(two.mul(ca));
        (t1, t2)
    }
}

/// `Line`: a point and a slope (`NaN` for vertical lines).
#[derive(Debug, Clone, Copy)]
struct Line {
    m: f32,
    p: Vector2,
}

impl Line {
    fn through(p1: Vector2, p2: Vector2) -> Self {
        Self {
            m: slope(p1, p2),
            p: p1,
        }
    }

    fn intersection(&self, l: &Self) -> Vector2 {
        if self.m.is_nan() {
            vertical_intersection(self, l)
        } else if l.m.is_nan() {
            vertical_intersection(l, self)
        } else {
            let x = (self.m * self.p.x - l.m * l.p.x - self.p.y + l.p.y) / (self.m - l.m);
            let y = self.m * x - self.m * self.p.x + self.p.y;
            Vector2::new(x, y)
        }
    }

    fn perpendicular_at(p: Vector2, p1: Vector2) -> Self {
        let m = slope(p, p1);
        let m = if m == 0.0 {
            f32::NAN
        } else if m.is_nan() {
            0.0
        } else {
            -1.0 / m
        };
        Self { m, p }
    }
}

fn vertical_intersection(vl: &Line, l: &Line) -> Vector2 {
    let x = vl.p.x;
    Vector2::new(x, l.m * (x - l.p.x) + l.p.y)
}

fn slope(p1: Vector2, p2: Vector2) -> f32 {
    #[allow(clippy::float_cmp)]
    if p2.x == p1.x {
        f32::NAN
    } else {
        (p2.y - p1.y) / (p2.x - p1.x)
    }
}

/// A circular arc (`Arc`): centre, radius, angles in radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Arc {
    /// Centre.
    pub c: Vector2,
    /// Radius.
    pub r: f32,
    /// Start angle.
    pub start_angle: f32,
    /// Sweep (positive: clockwise in LaserGRBL's terms).
    pub sweep_angle: f32,
    /// Start point.
    pub p1: Vector2,
    /// End point.
    pub p2: Vector2,
}

impl Arc {
    /// `IsClockwise`.
    #[must_use]
    pub fn is_clockwise(&self) -> bool {
        self.sweep_angle > 0.0
    }

    #[allow(clippy::cast_possible_truncation)]
    fn point_at(&self, t: f32) -> Vector2 {
        let angle = f64::from(self.start_angle + t * self.sweep_angle);
        let x = f64::from(self.c.x) + f64::from(self.r) * angle.cos();
        let y = f64::from(self.c.y) + f64::from(self.r) * angle.sin();
        Vector2::new(x as f32, y as f32)
    }

    /// `Length`: `r · |sweep|`.
    #[must_use]
    pub fn length(&self) -> f32 {
        self.r * self.sweep_angle.abs()
    }

    /// `LinearLength` as LaserGRBL computes it: only the x distance counts (its
    /// `dY` subtracts `P2.Y` from itself).
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn linear_length(&self) -> f32 {
        let dx = self.p2.x - self.p1.x;
        let dy = 0.0f32;
        f64::from(dx * dx + dy * dy).sqrt() as f32
    }
}

/// Two arcs meeting tangentially (`BiArc`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiArc {
    /// First arc.
    pub a1: Arc,
    /// Second arc.
    pub a2: Arc,
}

impl BiArc {
    #[allow(clippy::cast_possible_truncation)]
    fn new(p1: Vector2, t1: Vector2, p2: Vector2, t2: Vector2, t: Vector2) -> Self {
        let mut sum = 0.0f64;
        sum += f64::from((t.x - p1.x) * (t.y + p1.y));
        sum += f64::from((p2.x - t.x) * (p2.y + t.y));
        sum += f64::from((p1.x - p2.x) * (p1.y + p2.y));
        let cw = sum < 0.0;
        let tl1 = Line::perpendicular_at(p1, p1.add(t1));
        let tl2 = Line::perpendicular_at(p2, p2.add(t2));
        let pb_p1t = Line::perpendicular_at(p1.add(t).div(2.0), t);
        let pb_p2t = Line::perpendicular_at(p2.add(t).div(2.0), t);
        let c1 = tl1.intersection(&pb_p1t);
        let c2 = tl2.intersection(&pb_p2t);
        let r1 = c1.sub(p1).length();
        let r2 = c2.sub(p2).length();
        let sv1 = p1.sub(c1);
        let ev1 = t.sub(c1);
        let start1 = f64::from(sv1.y).atan2(f64::from(sv1.x));
        let mut sweep1 = f64::from(ev1.y).atan2(f64::from(ev1.x)) - start1;
        let sv2 = t.sub(c2);
        let ev2 = p2.sub(c2);
        let start2 = f64::from(sv2.y).atan2(f64::from(sv2.x));
        let mut sweep2 = f64::from(ev2.y).atan2(f64::from(ev2.x)) - start2;
        let two_pi = 2.0 * std::f64::consts::PI;
        if cw && sweep1 < 0.0 {
            sweep1 += two_pi;
        }
        if !cw && sweep1 > 0.0 {
            sweep1 -= two_pi;
        }
        if cw && sweep2 < 0.0 {
            sweep2 += two_pi;
        }
        if !cw && sweep2 > 0.0 {
            sweep2 -= two_pi;
        }
        Self {
            a1: Arc {
                c: c1,
                r: r1,
                start_angle: start1 as f32,
                sweep_angle: sweep1 as f32,
                p1,
                p2: t,
            },
            a2: Arc {
                c: c2,
                r: r2,
                start_angle: start2 as f32,
                sweep_angle: sweep2 as f32,
                p1: t,
                p2,
            },
        }
    }

    fn point_at(&self, t: f32) -> Vector2 {
        let s = self.a1.length() / (self.a1.length() + self.a2.length());
        if t <= s {
            self.a1.point_at(t / s)
        } else {
            self.a2.point_at((t - s) / (1.0 - s))
        }
    }

    fn length(&self) -> f32 {
        self.a1.length() + self.a2.length()
    }
}

fn is_real_inflexion(t: Complex) -> bool {
    t.im == 0.0 && t.re > 0.0 && t.re < 1.0
}

/// Splits beyond this many curves are given up (LaserGRBL would loop for ever on
/// a curve that splits into itself; never reached on real curves).
const MAX_CURVES: usize = 100_000;

/// `Algorithm.ApproxCubicBezier`: biarcs within `tolerance` of the curve, checked
/// every `sampling_step` along it. `None` when a biarc would need more than 1000
/// checks (LaserGRBL then draws a line).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub fn approx_cubic_bezier(bezier: CubicBezier, sampling_step: f32, tolerance: f32) -> Option<Vec<BiArc>> {
    let mut biarcs = Vec::new();
    let mut curves = vec![];
    let (inf1, inf2) = bezier.inflexion_points();
    let (i1, i2) = (is_real_inflexion(inf1), is_real_inflexion(inf2));
    if i1 && !i2 {
        let (a, b) = bezier.split(inf1.re as f32);
        curves.push(b);
        curves.push(a);
    } else if !i1 && i2 {
        let (a, b) = bezier.split(inf2.re as f32);
        curves.push(b);
        curves.push(a);
    } else if i1 && i2 {
        let mut t1 = inf1.re as f32;
        let mut t2 = inf2.re as f32;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        let (s1a, s1b) = bezier.split(t1);
        let t2 = (1.0 - t1) * t2;
        let (s2a, s2b) = s1b.split(t2);
        curves.push(s2b);
        curves.push(s2a);
        curves.push(s1a);
    } else {
        curves.push(bezier);
    }
    let mut steps = 0;
    while let Some(bezier) = curves.pop() {
        steps += 1;
        if steps > MAX_CURVES {
            return None;
        }
        let t1 = Line::through(bezier.p1, bezier.c1);
        let t2 = Line::through(bezier.p2, bezier.c2);
        let v = t1.intersection(&t2);
        let d_p2v = Vector2::distance(bezier.p2, v);
        let d_p1v = Vector2::distance(bezier.p1, v);
        let d_p1p2 = Vector2::distance(bezier.p1, bezier.p2);
        let g = Vector2::scale(d_p2v, bezier.p1)
            .add(Vector2::scale(d_p1v, bezier.p2))
            .add(Vector2::scale(d_p1p2, v))
            .div(d_p2v + d_p1v + d_p1p2);
        let biarc = BiArc::new(
            bezier.p1,
            bezier.p1.sub(bezier.c1),
            bezier.p2,
            bezier.p2.sub(bezier.c2),
            g,
        );
        let mut max_distance = 0.0f32;
        let mut max_distance_at = 0.0f32;
        let nr_points = biarc.length() / sampling_step;
        let parameter_step = 1.0 / nr_points;
        if nr_points > 1000.0 {
            return None;
        }
        let mut i = 0i32;
        while (i as f32) <= nr_points {
            let t = parameter_step * i as f32;
            let distance = biarc.point_at(t).sub(bezier.point_at(t)).length();
            if distance > max_distance {
                max_distance = distance;
                max_distance_at = t;
            }
            i += 1;
        }
        if max_distance > tolerance {
            let (a, b) = bezier.split(max_distance_at);
            curves.push(b);
            curves.push(a);
        } else {
            biarcs.push(biarc);
        }
    }
    Some(biarcs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complex_arithmetic() {
        let a = Complex::new(1.0, 2.0);
        let b = Complex::new(3.0, -1.0);
        assert_eq!(a.mul(b), Complex::new(5.0, 5.0));
        let q = a.div(b);
        assert!((q.re - 0.1).abs() < 1e-12 && (q.im - 0.7).abs() < 1e-12);
        let q = a.div(Complex::new(0.5, 2.0));
        assert!((q.re - 1.058_823_529).abs() < 1e-6);
        assert_eq!(Complex::new(4.0, 0.0).sqrt(), Complex::new(2.0, 0.0));
        assert!(Complex::new(-4.0, 0.0).sqrt().im > 1.99);
        assert_eq!(Complex::new(f64::INFINITY, 0.0).abs(), f64::INFINITY);
        assert_eq!(Complex::new(0.0, 0.0).abs(), 0.0);
        assert_eq!(Complex::new(3.0, 4.0).abs(), 5.0);
    }

    #[test]
    fn quarter_circle_becomes_two_arcs() {
        // a Bézier close to a quarter circle of radius 100
        let k = 55.228_47;
        let b = CubicBezier {
            p1: Vector2::new(100.0, 0.0),
            c1: Vector2::new(100.0, k),
            c2: Vector2::new(k, 100.0),
            p2: Vector2::new(0.0, 100.0),
        };
        let arcs = approx_cubic_bezier(b, 5.0, 2.0).unwrap();
        assert_eq!(arcs.len(), 1);
        let a = arcs[0].a1;
        assert!((a.r - 100.0).abs() < 1.0, "{a:?}");
        assert!(a.is_clockwise()); // positive sweep: LaserGRBL writes it as G3
        assert!(a.linear_length() > 0.0);
        // an S curve has an inflexion point: split first
        let s = CubicBezier {
            p1: Vector2::new(0.0, 0.0),
            c1: Vector2::new(50.0, 100.0),
            c2: Vector2::new(50.0, -100.0),
            p2: Vector2::new(100.0, 0.0),
        };
        let arcs = approx_cubic_bezier(s, 5.0, 2.0).unwrap();
        assert_eq!(arcs[0].a1.p1, s.p1);
        assert_eq!(arcs.last().unwrap().a2.p2, s.p2);
        for w in arcs.windows(2) {
            assert_eq!(w[0].a2.p2, w[1].a1.p1);
        }
        // an asymmetric S has a real inflexion point inside: it is split there
        let s2 = CubicBezier {
            p1: Vector2::new(0.0, 0.0),
            c1: Vector2::new(80.0, 100.0),
            c2: Vector2::new(20.0, -60.0),
            p2: Vector2::new(100.0, 0.0),
        };
        let (t1, t2) = s2.inflexion_points();
        assert!(is_real_inflexion(t1) || is_real_inflexion(t2));
        assert!(approx_cubic_bezier(s2, 5.0, 2.0).unwrap().len() >= 2);
        // too long to check: none
        let long = CubicBezier {
            p1: Vector2::new(0.0, 0.0),
            c1: Vector2::new(10000.0, 50000.0),
            c2: Vector2::new(40000.0, 50000.0),
            p2: Vector2::new(50000.0, 0.0),
        };
        assert!(approx_cubic_bezier(long, 5.0, 2.0).is_none());
    }

    #[test]
    fn lines_and_slopes() {
        let v = Line::through(Vector2::new(1.0, 0.0), Vector2::new(1.0, 5.0));
        assert!(v.m.is_nan());
        let h = Line::through(Vector2::new(0.0, 2.0), Vector2::new(5.0, 2.0));
        assert_eq!(v.intersection(&h), Vector2::new(1.0, 2.0));
        assert_eq!(h.intersection(&v), Vector2::new(1.0, 2.0));
        assert!(
            Line::perpendicular_at(Vector2::new(0.0, 0.0), Vector2::new(3.0, 0.0))
                .m
                .is_nan()
        );
        assert_eq!(
            Line::perpendicular_at(Vector2::new(0.0, 0.0), Vector2::new(0.0, 3.0)).m,
            0.0
        );
        assert_eq!(
            Line::perpendicular_at(Vector2::new(0.0, 0.0), Vector2::new(1.0, 2.0)).m,
            -0.5
        );
    }
}
