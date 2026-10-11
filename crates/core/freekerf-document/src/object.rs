//! Objects of a document.

use freekerf_geometry::{Matrix, Path, Point, Rect};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{DocumentError, ImageObject, LayerId};

/// Tolerance, mm, of arcs that a transform turns into lines.
const TRANSFORM_TOLERANCE_MM: f64 = 0.01;

fn is_identity(m: &Matrix) -> bool {
    *m == Matrix::IDENTITY
}

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
    /// A path command without all its arguments (a curve missing a coordinate).
    MissingArgument,
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

/// The text an object was made from (editable: the editor writes the strokes again
/// when the text or its height change, at the origin, then applies the object's
/// `transform`). Single-stroke Hershey font of the test generators.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TextSpec {
    /// The text (printable ASCII is drawn; other characters are skipped).
    pub text: String,
    /// Height of the font, mm.
    pub height_mm: f64,
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
    /// The text of a text object (its paths are the strokes); none otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextSpec>,
    /// Every transform applied since the object was created or imported. The
    /// geometry already includes it: the editor reads the rotation from it, and a
    /// text's strokes are written at the origin and then transformed by it.
    #[serde(default, skip_serializing_if = "is_identity")]
    pub transform: Matrix,
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
                r.add(Point::new(
                    img.position.x + img.size_mm[0],
                    img.position.y + img.size_mm[1],
                ));
            }
            ObjectKind::Unsupported { .. } => {}
        }
        r
    }

    /// Applies a transform to the geometry and adds it to `transform`. Images take
    /// only translations and positive scalings (they stay axis-aligned).
    ///
    /// # Errors
    /// A rotation, skew or mirror of an image; nothing changes then.
    pub fn apply_transform(&mut self, m: &Matrix) -> Result<(), DocumentError> {
        match &mut self.kind {
            ObjectKind::Shape { paths } => {
                for p in paths.iter_mut() {
                    *p = p.transformed(m, TRANSFORM_TOLERANCE_MM);
                }
            }
            ObjectKind::Image(img) => {
                if m.m12 != 0.0 || m.m21 != 0.0 || m.m11 <= 0.0 || m.m22 <= 0.0 {
                    return Err(DocumentError::Invalid(
                        "images can only be moved and scaled".into(),
                    ));
                }
                img.position = m.transform(img.position);
                img.size_mm = [img.size_mm[0] * m.m11, img.size_mm[1] * m.m22];
            }
            ObjectKind::Unsupported { .. } => {}
        }
        self.transform.append(m);
        Ok(())
    }

    /// Writes a text object's strokes again from `paths` given at the origin
    /// (the new text or height), through the object's `transform`.
    pub fn set_text_paths(&mut self, spec: TextSpec, paths: &[Path]) {
        let m = self.transform;
        self.kind = ObjectKind::Shape {
            paths: paths
                .iter()
                .map(|p| p.transformed(&m, TRANSFORM_TOLERANCE_MM))
                .collect(),
        };
        self.text = Some(spec);
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
        self.transform.translate(dx, dy);
    }
}

/// A closed rectangle path, corners `(x, y)` and `(x + width, y + height)`.
#[must_use]
pub fn rect_path(x: f64, y: f64, width: f64, height: f64) -> Path {
    let pts = [
        Point::new(x, y),
        Point::new(x + width, y),
        Point::new(x + width, y + height),
        Point::new(x, y + height),
    ];
    Path::polyline(&pts, true)
}

/// A closed ellipse path inscribed in the rectangle, four cubic Béziers
/// (counterclockwise from the right).
#[must_use]
pub fn ellipse_path(x: f64, y: f64, width: f64, height: f64) -> Path {
    use freekerf_geometry::Segment;
    // the usual cubic approximation of a quarter circle
    const K: f64 = 0.552_284_749_830_793_4;
    let (rx, ry) = (width / 2.0, height / 2.0);
    let (cx, cy) = (x + rx, y + ry);
    let (kx, ky) = (rx * K, ry * K);
    let mut p = Path::new(Point::new(cx + rx, cy));
    let quarter = |c1: (f64, f64), c2: (f64, f64), end: (f64, f64)| Segment::Cubic {
        c1: Point::new(c1.0, c1.1),
        c2: Point::new(c2.0, c2.1),
        end: Point::new(end.0, end.1),
    };
    p.segments = vec![
        quarter((cx + rx, cy + ky), (cx + kx, cy + ry), (cx, cy + ry)),
        quarter((cx - kx, cy + ry), (cx - rx, cy + ky), (cx - rx, cy)),
        quarter((cx - rx, cy - ky), (cx - kx, cy - ry), (cx, cy - ry)),
        quarter((cx + kx, cy - ry), (cx + rx, cy - ky), (cx + rx, cy)),
    ];
    p.closed = true;
    p
}
