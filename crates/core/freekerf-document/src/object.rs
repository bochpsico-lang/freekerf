//! Objects of a document.

use freekerf_geometry::{Path, Rect};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ImageObject, LayerId};

/// Identifier of an object inside a document (never reused).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ObjectId(pub u64);

/// The file format an object was imported from: how LaserGRBL wrote its G-code
/// differs per source (pen comments in SVG, none in DXF), and the planner keeps
/// that when asked for LaserGRBL-compatible output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    /// SVG drawing.
    Svg,
    /// DXF drawing.
    Dxf,
    /// Raster image.
    Image,
    /// Drawn in FreeKerf.
    Native,
}

/// Where an object came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Source {
    /// Format.
    pub format: SourceFormat,
    /// File name (no directory), empty when unknown.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
}

/// Why an element of an imported file was not converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedReason {
    /// The element kind is not converted (SVG `text`, `image`).
    NotConverted,
    /// A `polyline`/`polygon` whose points have no commas.
    MissingComma,
}

/// What an object is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjectKind {
    /// Vector paths in millimetres (Y up), in drawing order.
    Shape {
        /// Paths.
        paths: Vec<Path>,
    },
    /// A raster image.
    Image(Box<ImageObject>),
    /// An element of an imported file that was not converted; kept so the user
    /// sees it (and LaserGRBL-compatible output writes its comment).
    Unsupported {
        /// Element name (`text`, `image`, `polygon`...).
        element: String,
        /// Reason.
        reason: UnsupportedReason,
    },
}

/// An object on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Object {
    /// Identifier.
    pub id: ObjectId,
    /// Layer it belongs to.
    pub layer: LayerId,
    /// Display name (element id, file name...).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Origin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// Content.
    #[serde(flatten)]
    pub kind: ObjectKind,
}

impl Object {
    /// Bounds in mm (empty for unsupported elements).
    #[must_use]
    pub fn bounds(&self) -> Rect {
        let mut r = Rect::EMPTY;
        match &self.kind {
            ObjectKind::Shape { paths } => {
                for p in paths {
                    r.union(&p.bounds());
                }
            }
            ObjectKind::Image(img) => {
                r.add(img.position);
                r.add(freekerf_geometry::Point::new(
                    img.position.x + img.size_mm[0],
                    img.position.y + img.size_mm[1],
                ));
            }
            ObjectKind::Unsupported { .. } => {}
        }
        r
    }

    /// Moves the object.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        match &mut self.kind {
            ObjectKind::Shape { paths } => {
                for p in paths {
                    p.translate(dx, dy);
                }
            }
            ObjectKind::Image(img) => {
                img.position.x += dx;
                img.position.y += dy;
            }
            ObjectKind::Unsupported { .. } => {}
        }
    }
}
