//! Where a job made from a document goes on the machine (design/ux.md flow 3: "job
//! start" and the 3×3 anchor): the document is moved so the chosen point of its
//! bounds lands on the start point before it is planned.

use freekerf_document::Document;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A point of the job's bounding box (the 3×3 anchor).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    /// Bottom-left corner (default).
    #[default]
    BottomLeft,
    /// Middle of the bottom edge.
    Bottom,
    /// Bottom-right corner.
    BottomRight,
    /// Middle of the left edge.
    Left,
    /// Centre.
    Center,
    /// Middle of the right edge.
    Right,
    /// Top-left corner.
    TopLeft,
    /// Middle of the top edge.
    Top,
    /// Top-right corner.
    TopRight,
}

impl Anchor {
    /// Fractions of the width and height, from the bottom-left corner.
    #[must_use]
    pub const fn fractions(self) -> (f64, f64) {
        match self {
            Self::BottomLeft => (0.0, 0.0),
            Self::Bottom => (0.5, 0.0),
            Self::BottomRight => (1.0, 0.0),
            Self::Left => (0.0, 0.5),
            Self::Center => (0.5, 0.5),
            Self::Right => (1.0, 0.5),
            Self::TopLeft => (0.0, 1.0),
            Self::Top => (0.5, 1.0),
            Self::TopRight => (1.0, 1.0),
        }
    }
}

/// Moves every object of `doc` so its bounds' `anchor` lands on `at` (mm). Returns
/// the distance moved; nothing moves when the document has no bounds.
pub fn place(doc: &mut Document, anchor: Anchor, at: (f64, f64)) -> (f64, f64) {
    let b = doc.bounds();
    if b.is_empty() {
        return (0.0, 0.0);
    }
    let (fx, fy) = anchor.fractions();
    let (dx, dy) = (
        at.0 - (b.min_x + fx * b.width()),
        at.1 - (b.min_y + fy * b.height()),
    );
    for o in &mut doc.objects {
        o.translate(dx, dy);
    }
    (dx, dy)
}

#[cfg(test)]
mod tests {
    use freekerf_document::{Color, ObjectKind, rect_path};

    use super::*;

    #[test]
    fn anchors_land_on_the_start_point() {
        let mut d = Document::new();
        let l = d.layer_for_color(Color::BLACK, 1000, 1000);
        d.add_object(
            l,
            ObjectKind::Shape {
                paths: vec![rect_path(10.0, 20.0, 40.0, 30.0)],
            },
        )
        .unwrap();
        let mut c = d.clone();
        assert_eq!(place(&mut c, Anchor::Center, (0.0, 0.0)), (-30.0, -35.0));
        let b = c.bounds();
        assert_eq!((b.min_x, b.min_y, b.max_x, b.max_y), (-20.0, -15.0, 20.0, 15.0));
        let mut c = d.clone();
        place(&mut c, Anchor::TopRight, (100.0, 100.0));
        assert_eq!((c.bounds().max_x, c.bounds().max_y), (100.0, 100.0));
        for a in [
            Anchor::BottomLeft,
            Anchor::Bottom,
            Anchor::BottomRight,
            Anchor::Left,
            Anchor::Right,
            Anchor::TopLeft,
            Anchor::Top,
        ] {
            let mut c = d.clone();
            place(&mut c, a, (5.0, 5.0));
            let (fx, fy) = a.fractions();
            let b = c.bounds();
            assert!(
                (b.min_x + fx * b.width() - 5.0).abs() < 1e-9
                    && (b.min_y + fy * b.height() - 5.0).abs() < 1e-9
            );
        }
        // an empty document does not move
        assert_eq!(
            place(&mut Document::new(), Anchor::Center, (1.0, 1.0)),
            (0.0, 0.0)
        );
    }
}
