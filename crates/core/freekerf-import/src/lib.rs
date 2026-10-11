//! Importers: files (as bytes, never paths) to documents.
//!
//! - [`svg`]: shapes grouped by colour, as LaserGRBL's colour-layer import;
//! - [`dxf`]: lines and true arcs (splines fitted with arcs), one colour per DXF colour.
//!
//! An import gives the colours found (in order of first appearance, with their
//! element count) and the objects in drawing order; [`Import::into_document`] puts
//! them on colour layers. Geometry follows LaserGRBL operation by operation so the
//! planned G-code matches it (`conformance` importer goldens); the LaserGRBL bugs
//! fixed on purpose are listed in `doc/divergences.md`. Core layer: no I/O.
//!
//! ```
//! use freekerf_import::svg::{import, SvgOptions};
//!
//! let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10">
//!   <rect x="1" y="1" width="8" height="8" stroke="red" fill="none"/></svg>"#;
//! let imported = import(svg, &SvgOptions::default()).unwrap();
//! assert_eq!(imported.colors[0].color.to_string(), "#FF0000");
//! let doc = imported.into_document_with_defaults(1000, 1000, None);
//! assert_eq!(doc.objects.len(), 1);
//! ```

pub mod dxf;
mod netparse;
pub mod svg;

use freekerf_document::{Color, Document, Layer, LayerId, ObjectKind, Source};

pub use netparse::{parse_double, parse_long};

/// Errors of importers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    /// Not well-formed XML.
    #[error("not a valid XML file: {0}")]
    Xml(String),
    /// A file the importer cannot read.
    #[error("{0}")]
    Invalid(String),
    /// A binary DXF (only ASCII DXF is read).
    #[error("binary DXF files are not supported; save the drawing as ASCII DXF")]
    BinaryDxf,
    /// Nothing to import.
    #[error("the file has no entities that can be imported")]
    Empty,
}

/// A colour of the file and how many elements use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorCount {
    /// Colour.
    pub color: Color,
    /// Number of elements.
    pub count: usize,
}

/// An object of the file with its colour.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedObject {
    /// Colour that puts it on a layer.
    pub color: Color,
    /// Content.
    pub kind: ObjectKind,
}

/// The result of an import.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Import {
    /// Colours in order of first appearance.
    pub colors: Vec<ColorCount>,
    /// Objects in drawing order.
    pub objects: Vec<ImportedObject>,
}

impl Import {
    /// A document with `layers` (user order, their colours decide which objects go
    /// where); objects of a colour without a layer are left out, as LaserGRBL never
    /// writes them.
    #[must_use]
    pub fn into_document(self, layers: Vec<Layer>, source: Option<&Source>) -> Document {
        let mut doc = Document::new();
        let mut ids: Vec<(Color, LayerId)> = Vec::new();
        for l in layers {
            let color = l.color;
            let id = doc.add_layer(l);
            if !ids.iter().any(|(c, _)| *c == color) {
                ids.push((color, id));
            }
        }
        for o in self.objects {
            if let Some((_, layer)) = ids.iter().find(|(c, _)| *c == o.color) {
                // the layer exists: adding cannot fail
                let _ = doc.add_object_with(*layer, o.kind, String::new(), source.cloned());
            }
        }
        doc
    }

    /// A document with every object, whatever its colour, on one layer (black, `line`,
    /// speed and power 0: the planner's defaults apply when the layer preamble is off).
    #[must_use]
    pub fn into_single_layer(self, source: Option<&Source>) -> Document {
        let mut doc = Document::new();
        let layer = doc.add_layer(Layer::new(LayerId(0), Color::BLACK, 0, 0));
        for o in self.objects {
            let _ = doc.add_object_with(layer, o.kind, String::new(), source.cloned());
        }
        doc
    }

    /// A document with one layer per colour found, all with the same speed and power.
    #[must_use]
    pub fn into_document_with_defaults(
        self,
        speed_mm_min: u32,
        power: u32,
        source: Option<&Source>,
    ) -> Document {
        let layers = self
            .colors
            .iter()
            .map(|c| Layer::new(LayerId(0), c.color, speed_mm_min, power))
            .collect();
        self.into_document(layers, source)
    }
}

#[cfg(test)]
mod tests {
    use freekerf_document::LayerMode;
    use freekerf_geometry::{Path, Point};

    use super::*;

    #[test]
    fn objects_follow_their_layer_colour() {
        let shape = |x: f64| ObjectKind::Shape {
            paths: vec![Path::polyline(
                &[Point::new(x, 0.0), Point::new(x + 1.0, 0.0)],
                false,
            )],
        };
        let red = Color::rgb(255, 0, 0);
        let imp = Import {
            colors: vec![
                ColorCount { color: red, count: 1 },
                ColorCount {
                    color: Color::BLACK,
                    count: 1,
                },
            ],
            objects: vec![
                ImportedObject {
                    color: red,
                    kind: shape(0.0),
                },
                ImportedObject {
                    color: Color::BLACK,
                    kind: shape(1.0),
                },
            ],
        };
        let d = imp.clone().into_document_with_defaults(1000, 500, None);
        assert_eq!(d.layers.len(), 2);
        assert_eq!(d.objects.len(), 2);
        let mut cut = Layer::new(LayerId(0), red, 300, 1000);
        cut.mode = LayerMode::Cut;
        let only_red = imp.into_document(vec![cut.clone(), cut], None);
        assert_eq!(only_red.objects.len(), 1);
        assert_eq!(only_red.layers.len(), 2);
        assert!(ImportError::BinaryDxf.to_string().contains("ASCII"));
    }
}
