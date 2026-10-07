//! Ranges covered by a program (`ProgramRange`): where the laser burns, where the
//! head moves and which powers are used.

use rust_decimal::Decimal;
use serde::Serialize;

use crate::number::decimal_from_f64;

/// Minimum and maximum of a value, empty until the first update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Range {
    bounds: Option<(Decimal, Decimal)>,
}

impl Range {
    /// Widens the range to include `value`.
    pub fn update(&mut self, value: Decimal) {
        self.bounds = Some(match self.bounds {
            None => (value, value),
            Some((lo, hi)) => (lo.min(value), hi.max(value)),
        });
    }

    /// `(min, max)`, `None` when never updated.
    #[must_use]
    pub const fn bounds(&self) -> Option<(Decimal, Decimal)> {
        self.bounds
    }

    /// Minimum.
    #[must_use]
    pub fn min(&self) -> Option<Decimal> {
        self.bounds.map(|b| b.0)
    }

    /// Maximum.
    #[must_use]
    pub fn max(&self) -> Option<Decimal> {
        self.bounds.map(|b| b.1)
    }
}

/// An X/Y range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct XyRange {
    /// X extent.
    pub x: Range,
    /// Y extent.
    pub y: Range,
}

impl XyRange {
    /// Includes a point (either coordinate may be missing).
    pub fn update(&mut self, x: Option<Decimal>, y: Option<Decimal>) {
        if let Some(x) = x {
            self.x.update(x);
        }
        if let Some(y) = y {
            self.y.update(y);
        }
    }

    /// Includes a rectangle given in `f64` (an arc bounding box).
    pub fn update_rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        for v in [x, x + w].map(decimal_from_f64).into_iter().flatten() {
            self.x.update(v);
        }
        for v in [y, y + h].map(decimal_from_f64).into_iter().flatten() {
            self.y.update(v);
        }
    }

    /// True when both X and Y have values.
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.x.bounds.is_some() && self.y.bounds.is_some()
    }

    /// `[xmin, ymin, xmax, ymax]` when valid.
    #[must_use]
    pub fn as_box(&self) -> Option<[Decimal; 4]> {
        match (self.x.bounds, self.y.bounds) {
            (Some((x0, x1)), Some((y0, y1))) => Some([x0, y0, x1, y1]),
            _ => None,
        }
    }

    /// Width (0 when empty).
    #[must_use]
    pub fn width(&self) -> Decimal {
        self.x.bounds.map_or(Decimal::ZERO, |(a, b)| b - a)
    }

    /// Height (0 when empty).
    #[must_use]
    pub fn height(&self) -> Decimal {
        self.y.bounds.map_or(Decimal::ZERO, |(a, b)| b - a)
    }
}

/// Power (S) range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SRange {
    /// Values seen.
    pub s: Range,
}

impl SRange {
    /// Valid when it has two different values and a positive maximum (LaserGRBL).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        matches!(self.s.bounds, Some((lo, hi)) if lo != hi && hi > Decimal::ZERO)
    }
}

/// Where a program draws and moves, relative to the origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Quadrant {
    /// x ≥ 0, y ≥ 0.
    I,
    /// x ≤ 0, y ≥ 0.
    II,
    /// x ≤ 0, y ≤ 0.
    III,
    /// x ≥ 0, y ≤ 0.
    IV,
    /// Across quadrants.
    Mix,
    /// Nothing drawn.
    Unknown,
}

impl std::fmt::Display for Quadrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::I => "I",
            Self::II => "II",
            Self::III => "III",
            Self::IV => "IV",
            Self::Mix => "Mix",
            Self::Unknown => "Unknown",
        };
        f.write_str(s)
    }
}

/// All ranges of a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProgramRange {
    /// Where the laser burns.
    pub drawing: XyRange,
    /// Where the head goes (origin included).
    pub moving: XyRange,
    /// Powers.
    pub spindle: SRange,
}

impl ProgramRange {
    /// Includes a point in the moving range, and in the drawing range when `drawing`.
    pub fn update_xy(&mut self, x: Option<Decimal>, y: Option<Decimal>, drawing: bool) {
        if drawing {
            self.drawing.update(x, y);
        }
        self.moving.update(x, y);
    }

    /// Same as [`ProgramRange::update_xy`] for a rectangle.
    pub fn update_rect(&mut self, x: f64, y: f64, w: f64, h: f64, drawing: bool) {
        if drawing {
            self.drawing.update_rect(x, y, w, h);
        }
        self.moving.update_rect(x, y, w, h);
    }

    /// Quadrant of the drawing range.
    #[must_use]
    pub fn quadrant(&self) -> Quadrant {
        let Some([x0, y0, x1, y1]) = self.drawing.as_box() else {
            return Quadrant::Unknown;
        };
        let zero = Decimal::ZERO;
        if x0 >= zero && y0 >= zero {
            Quadrant::I
        } else if x1 <= zero && y0 >= zero {
            Quadrant::II
        } else if x1 <= zero && y1 <= zero {
            Quadrant::III
        } else if x0 >= zero && y1 <= zero {
            Quadrant::IV
        } else {
            Quadrant::Mix
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(v: i64) -> Decimal {
        Decimal::from(v)
    }

    #[test]
    fn quadrants() {
        let mut r = ProgramRange::default();
        assert_eq!(r.quadrant(), Quadrant::Unknown);
        r.update_xy(Some(d(1)), Some(d(1)), true);
        assert_eq!(r.quadrant(), Quadrant::I);
        let mut r = ProgramRange::default();
        r.update_xy(Some(d(-1)), Some(d(1)), true);
        assert_eq!(r.quadrant(), Quadrant::II);
        let mut r = ProgramRange::default();
        r.update_xy(Some(d(-1)), Some(d(-1)), true);
        assert_eq!(r.quadrant(), Quadrant::III);
        let mut r = ProgramRange::default();
        r.update_xy(Some(d(1)), Some(d(-1)), true);
        assert_eq!(r.quadrant(), Quadrant::IV);
        r.update_xy(Some(d(-1)), Some(d(1)), true);
        assert_eq!(r.quadrant(), Quadrant::Mix);
        assert_eq!(
            [Quadrant::I, Quadrant::II, Quadrant::III, Quadrant::IV, Quadrant::Mix, Quadrant::Unknown]
                .map(|q| q.to_string()),
            ["I", "II", "III", "IV", "Mix", "Unknown"]
        );
    }

    #[test]
    fn spindle_range_validity() {
        let mut s = SRange::default();
        assert!(!s.is_valid());
        s.s.update(d(0));
        assert!(!s.is_valid());
        s.s.update(d(10));
        assert!(s.is_valid());
        let mut neg = SRange::default();
        neg.s.update(d(-1));
        neg.s.update(d(0));
        assert!(!neg.is_valid());
    }

    #[test]
    fn xy_range_helpers() {
        let mut r = XyRange::default();
        assert_eq!(r.width(), Decimal::ZERO);
        r.update(Some(d(1)), None);
        assert!(!r.is_valid() && r.as_box().is_none());
        r.update_rect(0.0, 0.0, 2.0, 3.0);
        assert_eq!(r.width(), d(2));
        assert_eq!(r.height(), d(3));
        assert_eq!(r.x.min(), Some(d(0)));
        assert_eq!(r.x.max(), Some(d(2)));
        let mut p = ProgramRange::default();
        p.update_rect(0.0, 0.0, 1.0, 1.0, false);
        assert!(!p.drawing.is_valid() && p.moving.is_valid());
    }
}
