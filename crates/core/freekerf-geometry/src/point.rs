//! Points and rectangles in millimetres (or any unit: the geometry is unitless).

use std::ops::{Add, Mul, Sub};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A point (or a vector) with `f64` coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Point {
    /// X.
    pub x: f64,
    /// Y.
    pub y: f64,
}

impl Point {
    /// The origin.
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    /// A point.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Euclidean distance.
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        ((self.x - other.x) * (self.x - other.x) + (self.y - other.y) * (self.y - other.y)).sqrt()
    }

    /// Squared distance.
    #[must_use]
    pub fn distance_sq(self, other: Self) -> f64 {
        let (dx, dy) = (other.x - self.x, other.y - self.y);
        dx * dx + dy * dy
    }

    /// True when both coordinates are finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Point {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

/// An axis-aligned rectangle (`min` ≤ `max` once anything was added).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rect {
    /// Smallest X.
    pub min_x: f64,
    /// Smallest Y.
    pub min_y: f64,
    /// Largest X.
    pub max_x: f64,
    /// Largest Y.
    pub max_y: f64,
}

impl Default for Rect {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Rect {
    /// The empty rectangle (adding a point makes it that point).
    pub const EMPTY: Self = Self {
        min_x: f64::MAX,
        min_y: f64::MAX,
        max_x: f64::MIN,
        max_y: f64::MIN,
    };

    /// A rectangle from two corners.
    #[must_use]
    pub fn from_points(a: Point, b: Point) -> Self {
        let mut r = Self::EMPTY;
        r.add(a);
        r.add(b);
        r
    }

    /// True when nothing was added.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.min_x > self.max_x || self.min_y > self.max_y
    }

    /// Grows the rectangle to contain `p`.
    pub fn add(&mut self, p: Point) {
        self.min_x = self.min_x.min(p.x);
        self.max_x = self.max_x.max(p.x);
        self.min_y = self.min_y.min(p.y);
        self.max_y = self.max_y.max(p.y);
    }

    /// Grows the rectangle to contain `other`.
    pub fn union(&mut self, other: &Self) {
        if !other.is_empty() {
            self.add(Point::new(other.min_x, other.min_y));
            self.add(Point::new(other.max_x, other.max_y));
        }
    }

    /// Width (0 when empty).
    #[must_use]
    pub fn width(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            self.max_x - self.min_x
        }
    }

    /// Height (0 when empty).
    #[must_use]
    pub fn height(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            self.max_y - self.min_y
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_arithmetic_and_distance() {
        let a = Point::new(1.0, 2.0);
        let b = Point::new(4.0, 6.0);
        assert_eq!(a + b, Point::new(5.0, 8.0));
        assert_eq!(b - a, Point::new(3.0, 4.0));
        assert_eq!(a * 2.0, Point::new(2.0, 4.0));
        assert!((a.distance(b) - 5.0).abs() < 1e-12);
        assert!((a.distance_sq(b) - 25.0).abs() < 1e-12);
        assert!(a.is_finite());
        assert!(!Point::new(f64::NAN, 0.0).is_finite());
    }

    #[test]
    fn rect_grows() {
        let mut r = Rect::default();
        assert!(r.is_empty());
        assert_eq!(r.width(), 0.0);
        r.add(Point::new(1.0, 1.0));
        r.union(&Rect::from_points(Point::new(-1.0, 3.0), Point::new(2.0, 0.0)));
        r.union(&Rect::EMPTY);
        assert_eq!((r.min_x, r.min_y, r.max_x, r.max_y), (-1.0, 0.0, 2.0, 3.0));
        assert_eq!((r.width(), r.height()), (3.0, 3.0));
    }
}
