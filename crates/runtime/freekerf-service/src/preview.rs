//! The tool path of a G-code program as a compact binary buffer for the canvas
//! (ADR 0006: bulk data crosses the IPC as raw bytes, never JSON arrays).
//!
//! # Format (version 1, little endian)
//!
//! ```text
//! offset  size  field
//!      0     4  magic "FKTP"
//!      4     4  u32 version (1)
//!      8     4  u32 segment count N
//!     12     4  u32 command count of the program
//!     16    16  f32 x4 bounds of all segments [xmin, ymin, xmax, ymax] (mm; zeros when N = 0)
//!     32     4  f32 highest S of the program (to normalise power; 0 when none)
//!     36     4  u32 reserved (0)
//!     40  24×N  segments: f32 x0, y0, x1, y1 (mm, work coordinates, Y up),
//!               f32 power (S while burning, -1 for a move with the laser off),
//!               u32 index of the program command that produced it
//! ```
//!
//! Arcs (G2/G3) are flattened into chords no farther than [`ARC_TOLERANCE_MM`] from
//! the true arc. The output only depends on the program and the analysis options
//! (deterministic).
//!
//! The Design mode's document geometry uses the same format ([`encode_document`]):
//! `power` is the index of the object's layer in the layer list, the command index
//! is the index of the object in drawing order, the command count is the number of
//! objects; objects on hidden layers are left out, images are drawn as their frame,
//! and filling layers add the hatch lines their job engraves (command index:
//! [`NO_OBJECT`], they belong to no object; the canvas picks objects by their
//! segments).

use freekerf_document::{Document, ObjectKind};
use freekerf_gcode::arc::ArcHelper;
use freekerf_gcode::number::f64_from_decimal;
use freekerf_gcode::{AnalysisOptions, MachineState, Program};

/// Magic number at the start of the buffer.
pub const MAGIC: &[u8; 4] = b"FKTP";
/// Format version.
pub const VERSION: u32 = 1;
/// Header size in bytes.
pub const HEADER_BYTES: usize = 40;
/// Size of one segment in bytes.
pub const SEGMENT_BYTES: usize = 24;
/// Largest distance between an arc and its chords, mm.
pub const ARC_TOLERANCE_MM: f64 = 0.01;
/// Upper bound of chords per arc (a full circle of 5 m radius at 0.01 mm needs ~2200).
const MAX_ARC_CHORDS: usize = 4096;

/// One straight piece of the tool path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// Start, mm.
    pub from: [f32; 2],
    /// End, mm.
    pub to: [f32; 2],
    /// `S` while burning, `-1.0` for a move with the laser off.
    pub power: f32,
    /// Index of the program command.
    pub command: u32,
}

/// The tool path of `program`.
#[must_use]
pub fn segments(program: &Program, options: &AnalysisOptions) -> Vec<Segment> {
    let mut state = MachineState::new(options.support_pwm);
    let mut out = Vec::new();
    for (index, cmd) in program.commands().enumerate() {
        state.analyze(cmd, None);
        let command = u32::try_from(index).unwrap_or(u32::MAX);
        let power = if state.laser_burning() {
            f64_from_decimal(state.s().value()) as f32
        } else {
            -1.0
        };
        if !cmd.is_movement() || cmd.is_jog() {
            continue;
        }
        let from = point(
            f64_from_decimal(state.x().previous()),
            f64_from_decimal(state.y().previous()),
        );
        let to = point(
            f64_from_decimal(state.x().value()),
            f64_from_decimal(state.y().value()),
        );
        if state.is_g2g3() && cmd.is_arc_movement() {
            let arc = ArcHelper::new(&state, cmd);
            let chords = arc_chords(arc.radius, arc.angular_width);
            let step = arc.angular_width / chords as f64;
            let mut prev = from;
            for i in 1..=chords {
                let next = if i == chords {
                    to
                } else {
                    let a = arc.start_angle + step * i as f64;
                    point(
                        arc.center_x + arc.radius * a.cos(),
                        arc.center_y + arc.radius * a.sin(),
                    )
                };
                out.push(Segment {
                    from: prev,
                    to: next,
                    power,
                    command,
                });
                prev = next;
            }
        } else if from != to {
            out.push(Segment {
                from,
                to,
                power,
                command,
            });
        }
    }
    out
}

#[allow(clippy::cast_possible_truncation)] // positions in mm fit f32 with µm precision
fn point(x: f64, y: f64) -> [f32; 2] {
    [x as f32, y as f32]
}

/// Chords for an arc of `radius` spanning `angle` radians (signed).
fn arc_chords(radius: f64, angle: f64) -> usize {
    let angle = angle.abs();
    if radius.is_nan() || radius <= ARC_TOLERANCE_MM || !angle.is_finite() || angle == 0.0 {
        return 1;
    }
    // sagitta of a chord spanning θ: r (1 - cos(θ/2)) <= tolerance
    let max_step = 2.0 * (1.0 - ARC_TOLERANCE_MM / radius).acos();
    let n = (angle / max_step).ceil();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = if n.is_finite() && n >= 1.0 { n as usize } else { 1 };
    n.min(MAX_ARC_CHORDS)
}

/// The binary buffer described in the module documentation.
#[must_use]
pub fn encode(program: &Program, options: &AnalysisOptions) -> Vec<u8> {
    encode_segments(&segments(program, options), program.len())
}

/// Command index of the document segments that belong to no object (the hatch of
/// filling layers).
pub const NO_OBJECT: u32 = u32::MAX;

/// The drawable segments of a document (see the module documentation).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // mm fit f32; indexes fit u32
pub fn document_segments(doc: &Document) -> Vec<Segment> {
    let mut out = Vec::new();
    let f = |p: freekerf_geometry::Point| [p.x as f32, p.y as f32];
    for (index, o) in doc.objects.iter().enumerate() {
        let Some(layer) = doc.layers.iter().position(|l| l.id == o.layer) else {
            continue;
        };
        if !doc.layers[layer].visible {
            continue;
        }
        let mut push = |pts: &[freekerf_geometry::Point]| {
            for w in pts.windows(2) {
                out.push(Segment {
                    from: f(w[0]),
                    to: f(w[1]),
                    power: layer as f32,
                    command: index as u32,
                });
            }
        };
        match &o.kind {
            ObjectKind::Shape { paths } => {
                for p in paths {
                    push(&p.to_polyline(ARC_TOLERANCE_MM));
                }
            }
            ObjectKind::Image(_) => {
                let b = o.bounds();
                let corners = [
                    freekerf_geometry::Point::new(b.min_x, b.min_y),
                    freekerf_geometry::Point::new(b.max_x, b.min_y),
                    freekerf_geometry::Point::new(b.max_x, b.max_y),
                    freekerf_geometry::Point::new(b.min_x, b.max_y),
                    freekerf_geometry::Point::new(b.min_x, b.min_y),
                ];
                push(&corners);
            }
            ObjectKind::Unsupported { .. } => {}
        }
    }
    // the hatch of filling layers, as the job engraves it
    for (index, layer) in doc.layers.iter().enumerate() {
        if !layer.visible || !layer.mode.has_fill() {
            continue;
        }
        if !doc.objects.iter().any(|o| o.layer == layer.id) {
            continue;
        }
        // a filling that cannot be computed is only missing from the drawing; the
        // job reports the error
        for line in freekerf_planner::vector::layer_filling(doc, layer).unwrap_or_default() {
            for w in line.windows(2) {
                out.push(Segment {
                    from: f(w[0]),
                    to: f(w[1]),
                    power: index as f32,
                    command: NO_OBJECT,
                });
            }
        }
    }
    out
}

/// The document's geometry as a buffer of this format.
#[must_use]
pub fn encode_document(doc: &Document) -> Vec<u8> {
    encode_segments(&document_segments(doc), doc.objects.len())
}

fn encode_segments(segs: &[Segment], commands: usize) -> Vec<u8> {
    let mut bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    let mut max_s = 0.0f32;
    for s in segs {
        for p in [s.from, s.to] {
            bounds[0] = bounds[0].min(p[0]);
            bounds[1] = bounds[1].min(p[1]);
            bounds[2] = bounds[2].max(p[0]);
            bounds[3] = bounds[3].max(p[1]);
        }
        max_s = max_s.max(s.power);
    }
    if segs.is_empty() {
        bounds = [0.0; 4];
    }
    let mut out = Vec::with_capacity(HEADER_BYTES + SEGMENT_BYTES * segs.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&u32::try_from(segs.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&u32::try_from(commands).unwrap_or(u32::MAX).to_le_bytes());
    for b in bounds {
        out.extend_from_slice(&b.to_le_bytes());
    }
    out.extend_from_slice(&max_s.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for s in segs {
        for v in [s.from[0], s.from[1], s.to[0], s.to[1], s.power] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&s.command.to_le_bytes());
    }
    out
}

/// Size of a head frame in bytes ([`head_frame`]).
pub const HEAD_FRAME_BYTES: usize = 24;

/// The head position for the canvas, sent up to 60 times a second on a raw channel:
///
/// ```text
/// f32 work X, f32 work Y (mm), f32 power (current S), u32 job commands executed,
/// u32 job commands in total, u32 flags (bit 0: job running, bit 1: connected)
/// ```
#[must_use]
pub fn head_frame(s: &freekerf_protocol::HostSnapshot) -> [u8; HEAD_FRAME_BYTES] {
    let mut out = [0u8; HEAD_FRAME_BYTES];
    let flags = u32::from(s.in_program) | (u32::from(s.connected) << 1);
    let words = [
        s.work_position.x.to_le_bytes(),
        s.work_position.y.to_le_bytes(),
        s.power.to_le_bytes(),
        u32::try_from(s.job_executed).unwrap_or(u32::MAX).to_le_bytes(),
        u32::try_from(s.job_target).unwrap_or(u32::MAX).to_le_bytes(),
        flags.to_le_bytes(),
    ];
    for (i, w) in words.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(w);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(text: &str) -> Program {
        Program::parse(text, &AnalysisOptions::default())
    }

    fn f32_at(b: &[u8], at: usize) -> f32 {
        f32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    fn u32_at(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn document_geometry_by_layer_and_object() {
        use freekerf_document::{
            Bytes, Color, ImageData, ImageObject, Layer, LayerId, RasterSettings, rect_path,
        };
        let mut d = Document::new();
        let a = d.add_layer(Layer::new(LayerId(0), Color::BLACK, 1000, 1000));
        let b = d.add_layer(Layer::new(
            LayerId(0),
            "#FF0000".parse::<Color>().unwrap(),
            1000,
            1000,
        ));
        d.add_object(
            a,
            ObjectKind::Shape {
                paths: vec![rect_path(0.0, 0.0, 10.0, 5.0)],
            },
        )
        .unwrap();
        d.add_object(
            b,
            ObjectKind::Image(Box::new(ImageObject {
                data: ImageData::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Bytes(vec![0, 0, 0, 255]),
                },
                position: freekerf_geometry::Point::new(20.0, 0.0),
                size_mm: [4.0, 4.0],
                settings: RasterSettings::default(),
                edits: vec![],
            })),
        )
        .unwrap();
        let segs = document_segments(&d);
        assert_eq!(segs.len(), 8, "a rectangle and an image frame");
        assert!(segs[..4].iter().all(|s| s.power == 0.0 && s.command == 0));
        assert!(segs[4..].iter().all(|s| s.power == 1.0 && s.command == 1));
        let buf = encode_document(&d);
        assert_eq!(&buf[..4], MAGIC);
        assert_eq!(u32_at(&buf, 8), 8);
        assert_eq!(u32_at(&buf, 12), 2);
        assert_eq!(f32_at(&buf, 24), 24.0, "max x");
        // hidden layers are left out
        d.layers[1].visible = false;
        assert_eq!(document_segments(&d).len(), 4);
        assert_eq!(u32_at(&encode_document(&Document::new()), 8), 0);
        // a filling layer adds the hatch its job engraves, inside the shape
        d.layers[0].mode = freekerf_document::LayerMode::Fill;
        d.layers[0].fill.lines_per_mm = 1.0;
        let segs = document_segments(&d);
        assert!(segs.len() > 4, "{}", segs.len());
        let b = d.objects[0].bounds();
        for s in &segs[4..] {
            assert!(s.power == 0.0 && s.command == NO_OBJECT);
            for [x, y] in [s.from, s.to] {
                assert!(f64::from(x) >= b.min_x - 1e-3 && f64::from(x) <= b.max_x + 1e-3);
                assert!(f64::from(y) >= b.min_y - 1e-3 && f64::from(y) <= b.max_y + 1e-3);
            }
        }
        // hidden: neither outline nor hatch
        d.layers[0].visible = false;
        assert!(document_segments(&d).is_empty());
    }

    #[test]
    fn lines_with_and_without_laser() {
        let p = program("G0 X10 Y0\nM3 S500\nG1 X10 Y10 F1000\nM5\nG0 X0 Y0\n");
        let s = segments(&p, &AnalysisOptions::default());
        assert_eq!(
            s,
            vec![
                Segment {
                    from: [0.0, 0.0],
                    to: [10.0, 0.0],
                    power: -1.0,
                    command: 0
                },
                Segment {
                    from: [10.0, 0.0],
                    to: [10.0, 10.0],
                    power: 500.0,
                    command: 2
                },
                Segment {
                    from: [10.0, 10.0],
                    to: [0.0, 0.0],
                    power: -1.0,
                    command: 4
                },
            ]
        );
    }

    #[test]
    fn arcs_are_flattened_within_tolerance() {
        // a half circle of radius 10 around (10, 0), counter-clockwise
        let p = program("G0 X0 Y0\nM3 S100\nG3 X20 Y0 I10 J0 F500\n");
        let s = segments(&p, &AnalysisOptions::default());
        let arc: Vec<_> = s.iter().filter(|s| s.command == 2).collect();
        assert!(arc.len() > 30, "{}", arc.len());
        assert_eq!(arc.first().unwrap().from, [0.0, 0.0]);
        assert_eq!(arc.last().unwrap().to, [20.0, 0.0]);
        for seg in &arc {
            // chord midpoint distance from the circle <= tolerance
            let mx = f64::from(seg.from[0] + seg.to[0]) / 2.0 - 10.0;
            let my = f64::from(seg.from[1] + seg.to[1]) / 2.0;
            let sagitta = 10.0 - (mx * mx + my * my).sqrt();
            assert!(sagitta <= ARC_TOLERANCE_MM + 1e-4, "{sagitta}");
            // counter-clockwise from (0,0) to (20,0) through negative y
            assert!(seg.from[1] <= 1e-4 && seg.to[1] <= 1e-4, "{seg:?}");
        }
    }

    #[test]
    fn chord_count_limits() {
        assert_eq!(arc_chords(0.0, 1.0), 1);
        assert_eq!(arc_chords(10.0, 0.0), 1);
        assert_eq!(arc_chords(f64::NAN, 1.0), 1);
        assert_eq!(arc_chords(1e9, std::f64::consts::TAU), MAX_ARC_CHORDS);
    }

    #[test]
    fn binary_layout() {
        let p = program("M3 S1000\nG1 X5 Y2 F100\nG1 X-3\n");
        let b = encode(&p, &AnalysisOptions::default());
        assert_eq!(&b[0..4], MAGIC);
        assert_eq!(u32_at(&b, 4), VERSION);
        assert_eq!(u32_at(&b, 8), 2);
        assert_eq!(u32_at(&b, 12), 3);
        assert_eq!(
            [f32_at(&b, 16), f32_at(&b, 20), f32_at(&b, 24), f32_at(&b, 28)],
            [-3.0, 0.0, 5.0, 2.0]
        );
        assert_eq!(f32_at(&b, 32), 1000.0);
        assert_eq!(b.len(), HEADER_BYTES + 2 * SEGMENT_BYTES);
        let second = HEADER_BYTES + SEGMENT_BYTES;
        assert_eq!(f32_at(&b, second), 5.0);
        assert_eq!(f32_at(&b, second + 8), -3.0);
        assert_eq!(f32_at(&b, second + 16), 1000.0);
        assert_eq!(u32_at(&b, second + 20), 2);
    }

    #[test]
    fn head_frames() {
        let host = freekerf_protocol::Host::for_kind(
            freekerf_protocol::FirmwareKind::Grbl,
            freekerf_protocol::HostOptions::default(),
        );
        let mut snap = host.snapshot();
        snap.work_position.x = 12.5;
        snap.work_position.y = -3.0;
        snap.power = 800.0;
        snap.job_executed = 7;
        snap.job_target = 9;
        snap.in_program = true;
        snap.connected = true;
        let f = head_frame(&snap);
        assert_eq!(f32_at(&f, 0), 12.5);
        assert_eq!(f32_at(&f, 4), -3.0);
        assert_eq!(f32_at(&f, 8), 800.0);
        assert_eq!(u32_at(&f, 12), 7);
        assert_eq!(u32_at(&f, 16), 9);
        assert_eq!(u32_at(&f, 20), 3);
    }

    /// The same bytes are decoded by the TypeScript side
    /// (`ui/web/src/canvas/toolpath.test.ts`): a contract test of the format.
    /// `FREEKERF_UPDATE_FIXTURES=1` rewrites the file.
    #[test]
    fn shared_fixture() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../ui/web/tests/fixtures/toolpath-v1.bin"
        );
        let p = program("G0 X1 Y1\nM3 S500\nG1 X11 Y1 F600\nG2 X21 Y1 I5 J0\nM5\nG0 X0 Y0\n");
        let bytes = encode(&p, &AnalysisOptions::default());
        if std::env::var_os("FREEKERF_UPDATE_FIXTURES").is_some() {
            std::fs::write(path, &bytes).unwrap();
        }
        assert_eq!(
            std::fs::read(path).unwrap(),
            bytes,
            "run with FREEKERF_UPDATE_FIXTURES=1"
        );
    }

    #[test]
    fn empty_program() {
        let b = encode(&program(""), &AnalysisOptions::default());
        assert_eq!(b.len(), HEADER_BYTES);
        assert_eq!(u32_at(&b, 8), 0);
        assert_eq!(f32_at(&b, 16), 0.0);
    }
}
