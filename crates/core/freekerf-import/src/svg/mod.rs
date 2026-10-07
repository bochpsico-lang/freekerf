//! SVG import: shapes grouped by colour (stroke, else fill), as LaserGRBL's colour
//! layer import reads them (`SvgColorLayer`, `GCodeFromSVG`).

mod color;
mod convert;
mod known_colors;

use freekerf_document::{Color, ObjectKind};

use crate::{ColorCount, Import, ImportError, ImportedObject};

pub use color::normalize as normalize_color;
pub use convert::to_pixel;

/// Options of the SVG import.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SvgOptions {
    /// Scale the drawing so its larger side is this many mm (LaserGRBL's centerline
    /// conversion); `None` keeps the file's size.
    pub scale_to_mm: Option<f32>,
    /// Added to every coordinate, mm.
    pub user_offset: (f32, f32),
    /// Approximate Béziers with 12 equal steps instead of the adaptive flattening
    /// (`Vector.UseSmartBezier` off).
    pub legacy_bezier: bool,
}

/// One element (or part of one) of the file, in drawing order.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SvgItem {
    pub(crate) color: Color,
    pub(crate) kind: ObjectKind,
}

/// The text of a file as `File.ReadAllText` decodes it: byte order marks for UTF-8,
/// UTF-16 and UTF-32, UTF-8 otherwise (invalid sequences replaced).
#[must_use]
pub fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |b: &[u8], be: bool| {
        let units: Vec<u16> = b
            .chunks_exact(2)
            .map(|c| {
                if be {
                    u16::from_be_bytes([c[0], c[1]])
                } else {
                    u16::from_le_bytes([c[0], c[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    let utf32 = |b: &[u8], be: bool| {
        b.chunks_exact(4)
            .map(|c| {
                let v = if be {
                    u32::from_be_bytes([c[0], c[1], c[2], c[3]])
                } else {
                    u32::from_le_bytes([c[0], c[1], c[2], c[3]])
                };
                char::from_u32(v).unwrap_or('\u{fffd}')
            })
            .collect::<String>()
    };
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE, 0x00, 0x00]) {
        utf32(rest, false)
    } else if let Some(rest) = bytes.strip_prefix(&[0x00, 0x00, 0xFE, 0xFF]) {
        utf32(rest, true)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        utf16(rest, false)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        utf16(rest, true)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// `RemoveInvalidUnicode`: drops what XML forbids (control characters, U+FFFE/F) and,
/// as LaserGRBL's pattern does, every character outside the basic plane.
fn remove_invalid(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            matches!(c, '\t' | '\n' | '\r')
                || ('\u{20}'..='\u{d7ff}').contains(&c)
                || ('\u{e000}'..='\u{fffd}').contains(&c)
        })
        .collect()
}

/// Elements LaserGRBL converts (text and image are not).
const CONVERTED: [&str; 7] = ["path", "rect", "circle", "ellipse", "line", "polyline", "polygon"];

/// `SvgColorLayer.Scan`: one entry per colour of the converted elements, in order
/// of first appearance (document order, groups included).
fn scan(node: roxmltree::Node<'_, '_>, ns: Option<&str>, out: &mut Vec<ColorCount>) {
    for e in node.children().filter(roxmltree::Node::is_element) {
        let name = e.tag_name();
        if name.namespace() != ns {
            continue;
        }
        if name.name() == "g" {
            scan(e, ns, out);
        } else if CONVERTED.contains(&name.name()) {
            let c = color::resolve(e);
            match out.iter_mut().find(|x| x.color == c) {
                Some(x) => x.count += 1,
                None => out.push(ColorCount { color: c, count: 1 }),
            }
        }
    }
}

/// Imports an SVG file.
///
/// # Errors
/// Not XML, or a structure LaserGRBL cannot read either (a viewBox without four
/// space-separated numbers, groups nested more than 9 levels).
pub fn import(bytes: &[u8], options: &SvgOptions) -> Result<Import, ImportError> {
    let text = remove_invalid(&decode_text(bytes));
    let doc = roxmltree::Document::parse_with_options(
        &text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..roxmltree::ParsingOptions::default()
        },
    )
    .map_err(|e| ImportError::Xml(e.to_string()))?;
    let root = doc.root_element();
    let mut colors = Vec::new();
    scan(root, root.tag_name().namespace(), &mut colors);
    let items = convert::Converter::new(options).run(root)?;
    Ok(Import {
        colors,
        objects: items
            .into_iter()
            .map(|i| ImportedObject {
                color: i.color,
                kind: i.kind,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoding_and_cleaning() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFabc"), "abc");
        assert_eq!(decode_text(&[0xFF, 0xFE, b'a', 0]), "a");
        assert_eq!(decode_text(&[0xFE, 0xFF, 0, b'a']), "a");
        assert_eq!(decode_text(&[0xFF, 0xFE, 0, 0, b'a', 0, 0, 0]), "a");
        assert_eq!(decode_text(&[0, 0, 0xFE, 0xFF, 0, 0, 0, b'a']), "a");
        assert_eq!(decode_text(b"a\xFFb"), "a\u{fffd}b");
        assert_eq!(remove_invalid("a\u{1}b\u{1F600}c\u{fffe}\td"), "abc\td");
    }

    #[test]
    fn imports_shapes_by_colour() {
        let svg =
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20">
            <rect x="1" y="1" width="5" height="5" stroke="red" fill="none"/>
            <g fill="#00f"><circle cx="10" cy="10" r="2"/><text>hi</text></g>
            <path d="M1 1 L5 5"/></svg>"##;
        let imp = import(svg, &SvgOptions::default()).unwrap();
        let colors: Vec<String> = imp
            .colors
            .iter()
            .map(|c| format!("{}x{}", c.color, c.count))
            .collect();
        assert_eq!(colors, ["#FF0000x1", "#0000FFx1", "#000000x1"]);
        // traversal order: root basic shapes, root paths, then groups
        assert_eq!(imp.objects.len(), 4);
        assert!(matches!(imp.objects[2].kind, ObjectKind::Shape { .. }));
        assert!(matches!(imp.objects[3].kind, ObjectKind::Unsupported { .. }));
        assert!(import(b"<svg", &SvgOptions::default()).is_err());
        assert!(import(br#"<svg viewBox="0,0,1,1"/>"#, &SvgOptions::default()).is_err());
    }
}
