//! Font faces: single-line ([`StrokeFont`]) or outline (TrueType / OpenType, read
//! with `ttf-parser` and shaped with `rustybuzz`), behind one interface: metrics
//! and the glyphs of a line of text, in font units, Y up.

use std::sync::Arc;

use freekerf_geometry::{Path, Point, Segment};

use crate::TextError;
use crate::svgfont::StrokeFont;

/// Vertical metrics of a face, font units (Y up, descent negative).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Metrics {
    pub units_per_em: f64,
    pub ascent: f64,
    pub descent: f64,
    pub line_gap: f64,
    pub cap_height: f64,
    /// The capitals' height as drawn (differs from `cap_height` in LaserGRBL's
    /// Hershey only); letters from a fallback font match it.
    pub drawn_cap_height: f64,
    pub x_height: f64,
    pub underline_position: f64,
    pub underline_thickness: f64,
    pub strikeout_position: f64,
    pub strikeout_thickness: f64,
}

impl Metrics {
    /// Distance between baselines at line spacing 1.
    pub(crate) fn line_height(&self) -> f64 {
        self.ascent - self.descent + self.line_gap
    }
}

/// A glyph of a shaped line: its outline at the pen position and how far it moves
/// the pen, font units.
#[derive(Debug, Clone, Default)]
pub(crate) struct ShapedGlyph {
    pub paths: Vec<Path>,
    pub x_advance: f64,
    pub x_offset: f64,
    pub y_offset: f64,
    /// White space (justified lines stretch these).
    pub space: bool,
}

/// A TrueType or OpenType font (or one face of a collection).
#[derive(Debug, Clone)]
pub struct OutlineFont {
    data: Arc<[u8]>,
    index: u32,
    metrics: Metrics,
}

#[derive(Default)]
struct Outline {
    paths: Vec<Path>,
    current: Option<Path>,
}

impl Outline {
    fn path(&mut self) -> &mut Path {
        self.current.get_or_insert_with(|| Path::new(Point::ZERO))
    }

    fn finish(&mut self) {
        if let Some(p) = self.current.take()
            && !p.segments.is_empty()
        {
            self.paths.push(p);
        }
    }
}

fn pt(x: f32, y: f32) -> Point {
    Point::new(f64::from(x), f64::from(y))
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        self.current = Some(Path::new(pt(x, y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path().segments.push(Segment::line(pt(x, y)));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p = self.path();
        let (p0, c, end) = (p.end(), pt(x1, y1), pt(x, y));
        p.segments.push(Segment::Cubic {
            c1: p0 + (c - p0) * (2.0 / 3.0),
            c2: end + (c - end) * (2.0 / 3.0),
            end,
        });
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path().segments.push(Segment::Cubic {
            c1: pt(x1, y1),
            c2: pt(x2, y2),
            end: pt(x, y),
        });
    }

    fn close(&mut self) {
        if let Some(p) = &mut self.current {
            if p.end() != p.start {
                let start = p.start;
                p.segments.push(Segment::line(start));
            }
            p.closed = true;
        }
        self.finish();
    }
}

impl OutlineFont {
    /// Reads face `index` of a font file.
    ///
    /// # Errors
    /// Not a font this reader understands.
    pub fn new(data: Arc<[u8]>, index: u32) -> Result<Self, TextError> {
        let face = ttf_parser::Face::parse(&data, index).map_err(|e| TextError::Font(e.to_string()))?;
        let upem = f64::from(face.units_per_em());
        let measured = |c: char| {
            face.glyph_index(c)
                .and_then(|g| face.glyph_bounding_box(g))
                .map(|r| f64::from(r.y_max))
                .filter(|h| *h > 0.0)
        };
        let cap_height = face
            .capital_height()
            .map(f64::from)
            .filter(|h| *h > 0.0)
            .or_else(|| measured('H'))
            .unwrap_or(upem * 0.7);
        let x_height = face
            .x_height()
            .map(f64::from)
            .filter(|h| *h > 0.0)
            .or_else(|| measured('x'))
            .unwrap_or(cap_height * 0.65);
        let underline = face.underline_metrics();
        let strikeout = face.strikeout_metrics();
        let metrics = Metrics {
            units_per_em: upem,
            ascent: f64::from(face.ascender()),
            descent: f64::from(face.descender()),
            line_gap: f64::from(face.line_gap()),
            cap_height,
            drawn_cap_height: cap_height,
            x_height,
            underline_position: underline.map_or(-upem * 0.1, |m| f64::from(m.position)),
            underline_thickness: underline
                .map(|m| f64::from(m.thickness))
                .filter(|t| *t > 0.0)
                .unwrap_or(upem * 0.05),
            strikeout_position: strikeout.map_or(x_height / 2.0, |m| f64::from(m.position)),
            strikeout_thickness: strikeout
                .map(|m| f64::from(m.thickness))
                .filter(|t| *t > 0.0)
                .unwrap_or(upem * 0.05),
        };
        Ok(Self { data, index, metrics })
    }

    pub(crate) const fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// Whether the font has a glyph for `c`.
    #[must_use]
    pub fn covers(&self, c: char) -> bool {
        ttf_parser::Face::parse(&self.data, self.index).is_ok_and(|f| f.glyph_index(c).is_some())
    }

    /// The glyphs of one line: kerning, ligatures, marks and right-to-left scripts
    /// as the font's OpenType tables say.
    pub(crate) fn shape(&self, text: &str) -> Vec<ShapedGlyph> {
        let (Ok(face), Some(shaper)) = (
            ttf_parser::Face::parse(&self.data, self.index),
            rustybuzz::Face::from_slice(&self.data, self.index),
        ) else {
            return Vec::new();
        };
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let shaped = rustybuzz::shape(&shaper, &[], buffer);
        shaped
            .glyph_infos()
            .iter()
            .zip(shaped.glyph_positions())
            .map(|(info, pos)| {
                let mut outline = Outline::default();
                if let Ok(id) = u16::try_from(info.glyph_id)
                    && id != 0
                {
                    face.outline_glyph(ttf_parser::GlyphId(id), &mut outline);
                }
                outline.finish();
                let space = usize::try_from(info.cluster)
                    .ok()
                    .and_then(|i| text.get(i..))
                    .and_then(|s| s.chars().next())
                    .is_some_and(char::is_whitespace);
                ShapedGlyph {
                    paths: outline.paths,
                    x_advance: f64::from(pos.x_advance),
                    x_offset: f64::from(pos.x_offset),
                    y_offset: f64::from(pos.y_offset),
                    space,
                }
            })
            .collect()
    }
}

/// A font face ready to lay text out.
#[derive(Debug, Clone)]
pub enum Face {
    /// Open strokes, drawn once.
    Stroke(Arc<StrokeFont>),
    /// Closed outlines.
    Outline(Arc<OutlineFont>),
}

impl Face {
    pub(crate) fn metrics(&self) -> &Metrics {
        match self {
            Self::Stroke(f) => f.metrics(),
            Self::Outline(f) => f.metrics(),
        }
    }

    /// Height of the capitals over the em (an SVG `font-size` is the em).
    #[must_use]
    pub fn cap_height_per_em(&self) -> f64 {
        let m = self.metrics();
        m.cap_height / m.units_per_em
    }

    /// Single-line (open strokes).
    #[must_use]
    pub const fn single_line(&self) -> bool {
        matches!(self, Self::Stroke(_))
    }

    pub(crate) fn shape(&self, text: &str) -> Vec<ShapedGlyph> {
        match self {
            Self::Stroke(f) => f.shape(text),
            Self::Outline(f) => f.shape(text),
        }
    }

    /// Whether the face has a glyph for `c`.
    #[must_use]
    pub fn covers(&self, c: char) -> bool {
        match self {
            Self::Stroke(f) => f.covers(c),
            Self::Outline(f) => f.covers(c),
        }
    }
}
