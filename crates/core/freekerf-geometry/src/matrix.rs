//! 2D affine transforms with the conventions (and the floating-point operation
//! order) of `System.Windows.Media.Matrix` as implemented by Mono's WindowsBase:
//! row vectors, `p' = p · M`, so `a.append(b)` applies `a` first, then `b`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Point;

/// An affine transform `x' = x·m11 + y·m21 + offset_x`, `y' = x·m12 + y·m22 + offset_y`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Matrix {
    /// Row 1, column 1.
    pub m11: f64,
    /// Row 1, column 2.
    pub m12: f64,
    /// Row 2, column 1.
    pub m21: f64,
    /// Row 2, column 2.
    pub m22: f64,
    /// Translation X.
    pub offset_x: f64,
    /// Translation Y.
    pub offset_y: f64,
}

impl Default for Matrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Matrix {
    /// The identity.
    pub const IDENTITY: Self = Self::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    /// A matrix from its six values (`Matrix(m11, m12, m21, m22, offsetX, offsetY)`).
    #[must_use]
    pub const fn new(m11: f64, m12: f64, m21: f64, m22: f64, offset_x: f64, offset_y: f64) -> Self {
        Self {
            m11,
            m12,
            m21,
            m22,
            offset_x,
            offset_y,
        }
    }

    /// A translation.
    #[must_use]
    pub const fn translation(dx: f64, dy: f64) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, dx, dy)
    }

    /// A scaling about the origin.
    #[must_use]
    pub const fn scaling(sx: f64, sy: f64) -> Self {
        Self::new(sx, 0.0, 0.0, sy, 0.0, 0.0)
    }

    /// `this = this · m` (apply `self`, then `m`).
    pub fn append(&mut self, m: &Self) {
        let m11 = self.m11 * m.m11 + self.m12 * m.m21;
        let m12 = self.m11 * m.m12 + self.m12 * m.m22;
        let m21 = self.m21 * m.m11 + self.m22 * m.m21;
        let m22 = self.m21 * m.m12 + self.m22 * m.m22;
        let offset_x = self.offset_x * m.m11 + self.offset_y * m.m21 + m.offset_x;
        let offset_y = self.offset_x * m.m12 + self.offset_y * m.m22 + m.offset_y;
        *self = Self::new(m11, m12, m21, m22, offset_x, offset_y);
    }

    /// `Matrix.Multiply(a, b)`: `a` then `b`.
    #[must_use]
    pub fn multiply(a: &Self, b: &Self) -> Self {
        let mut m = *a;
        m.append(b);
        m
    }

    /// Adds a translation (applied last).
    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.offset_x += dx;
        self.offset_y += dy;
    }

    /// Appends a scaling about the origin.
    pub fn scale(&mut self, sx: f64, sy: f64) {
        self.append(&Self::scaling(sx, sy));
    }

    /// Appends a rotation by `angle_deg` degrees (counterclockwise for Y up).
    pub fn rotate(&mut self, angle_deg: f64) {
        let theta = angle_deg * std::f64::consts::PI / 180.0;
        self.append(&Self::new(
            theta.cos(),
            theta.sin(),
            -theta.sin(),
            theta.cos(),
            0.0,
            0.0,
        ));
    }

    /// Appends a rotation about `(cx, cy)` (Mono: translate, rotate, translate back).
    pub fn rotate_at(&mut self, angle_deg: f64, cx: f64, cy: f64) {
        self.translate(-cx, -cy);
        self.rotate(angle_deg);
        self.translate(cx, cy);
    }

    /// Transforms a point.
    #[must_use]
    pub fn transform(&self, p: Point) -> Point {
        Point::new(
            p.x * self.m11 + p.y * self.m21 + self.offset_x,
            p.x * self.m12 + p.y * self.m22 + self.offset_y,
        )
    }

    /// Transforms a vector (no translation).
    #[must_use]
    pub fn transform_vector(&self, v: Point) -> Point {
        Point::new(v.x * self.m11 + v.y * self.m21, v.x * self.m12 + v.y * self.m22)
    }

    /// Determinant of the linear part.
    #[must_use]
    pub fn determinant(&self) -> f64 {
        self.m11 * self.m22 - self.m12 * self.m21
    }

    /// True when the transform keeps circles circles (a rotation, uniform scale,
    /// mirror and translation): arcs can then be transformed as arcs.
    #[must_use]
    pub fn is_similarity(&self) -> bool {
        let eps = 1e-12 * (self.m11.abs() + self.m12.abs() + self.m21.abs() + self.m22.abs()).max(1.0);
        let col1 = self.m11 * self.m11 + self.m12 * self.m12;
        let col2 = self.m21 * self.m21 + self.m22 * self.m22;
        let dot = self.m11 * self.m21 + self.m12 * self.m22;
        (col1 - col2).abs() <= eps && dot.abs() <= eps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_applies_left_first() {
        let mut m = Matrix::translation(10.0, 0.0);
        m.scale(2.0, 3.0);
        assert_eq!(m.transform(Point::new(1.0, 1.0)), Point::new(22.0, 3.0));
        let n = Matrix::multiply(&Matrix::scaling(2.0, 3.0), &Matrix::translation(10.0, 0.0));
        assert_eq!(n.transform(Point::new(1.0, 1.0)), Point::new(12.0, 3.0));
        assert_eq!(n.transform_vector(Point::new(1.0, 1.0)), Point::new(2.0, 3.0));
        assert_eq!(Matrix::default(), Matrix::IDENTITY);
    }

    #[test]
    fn rotation_about_a_point() {
        let mut m = Matrix::IDENTITY;
        m.rotate_at(90.0, 1.0, 1.0);
        let p = m.transform(Point::new(2.0, 1.0));
        assert!((p.x - 1.0).abs() < 1e-12 && (p.y - 2.0).abs() < 1e-12);
        assert!(m.is_similarity());
        assert!((m.determinant() - 1.0).abs() < 1e-12);
        assert!(!Matrix::scaling(1.0, 2.0).is_similarity());
        assert!(Matrix::scaling(-2.0, 2.0).is_similarity());
    }
}
