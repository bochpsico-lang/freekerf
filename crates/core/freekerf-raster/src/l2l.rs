//! Line-to-line engraving (`GrblFile.LoadImageL2L` / `ImageLine2Line`): the image
//! is scanned in lines (horizontal, vertical or diagonal, alternating direction
//! unless unidirectional), each run of pixels of the same power becomes one move, and
//! runs of zero power become fast moves.

use freekerf_gcode::Command;
use freekerf_gcode::dotnet::{double_format, double_to_string};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::image::{Image, split};

/// Scan direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Rows.
    #[default]
    Horizontal,
    /// Columns.
    Vertical,
    /// Diagonal strips.
    Diagonal,
}

/// Raster job settings (`GrblFile.L2LConf`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct L2lConf {
    /// Pixels per mm of the image.
    pub res: f64,
    /// Lines per mm of a filling (used instead of `res` with `vector_filling`).
    pub fres: f64,
    /// X offset, mm (single precision, as LaserGRBL stores it).
    pub offset_x: f32,
    /// Y offset, mm.
    pub offset_y: f32,
    /// Engraving feed, mm/min.
    pub mark_speed: i32,
    /// Border feed (vectorize), mm/min.
    pub border_speed: i32,
    /// `S` for the lightest burned pixel.
    pub min_power: i32,
    /// `S` for black.
    pub max_power: i32,
    /// Laser on command.
    pub laser_on: String,
    /// Laser off command.
    pub laser_off: String,
    /// Scan direction.
    pub direction: Direction,
    /// Power with `S` (PWM); otherwise the laser is switched on and off.
    pub pwm: bool,
    /// Coordinates from `fres` (raster filling of vectorized shapes).
    pub vector_filling: bool,
    /// Smoothieware: power as 0..1 with two decimals.
    pub smoothie: bool,
}

impl Default for L2lConf {
    fn default() -> Self {
        Self {
            res: 10.0,
            fres: 10.0,
            offset_x: 0.0,
            offset_y: 0.0,
            mark_speed: 1000,
            border_speed: 1000,
            min_power: 0,
            max_power: 1000,
            laser_on: "M3".into(),
            laser_off: "M5".into(),
            direction: Direction::Horizontal,
            pwm: true,
            vector_filling: false,
            smoothie: false,
        }
    }
}

/// Options that are LaserGRBL settings rather than raster settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct L2lOptions {
    /// Travel with `G1` (`Disable G0 fast skip`).
    pub disable_g0: bool,
    /// Every line in the same direction (`Unidirectional Engraving`).
    pub unidirectional: bool,
    /// The board's `$30`, used to set the power without PWM.
    pub max_pwm: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    X,
    Y,
    D,
    VSeparator,
    HSeparator,
}

#[derive(Debug, Clone, Copy)]
struct Segment {
    kind: Kind,
    color: i32,
    len: i32,
}

impl Segment {
    fn new(kind: Kind, color: i32, len: i32, reverse: bool) -> Self {
        Self {
            kind,
            color,
            len: if reverse { -len } else { len },
        }
    }

    fn is_separator(&self) -> bool {
        matches!(self.kind, Kind::VSeparator | Kind::HSeparator)
    }

    fn fast(&self, c: &L2lConf) -> bool {
        if c.pwm { self.color == 0 } else { self.color <= 125 }
    }
}

/// `formatnumber(int, float, c)`: pixels to mm, plus the offset, `Math.Round(…, 3)`.
fn coord(number: i32, offset: f32, c: &L2lConf) -> String {
    let res = if c.vector_filling { c.fres } else { c.res };
    let v = f64::from(number) / res + f64::from(offset);
    double_to_string((v * 1000.0).round_ties_even() / 1000.0)
}

fn power(color: i32, c: &L2lConf) -> String {
    if c.smoothie {
        format!("S{}", double_format(f64::from(color) / 255.0, "0.00"))
    } else {
        format!("S{color}")
    }
}

impl Segment {
    fn gcode(&self, cum_x: &mut i32, cum_y: &mut i32, c: &L2lConf) -> String {
        let tail = |s: &Self| {
            if c.pwm {
                power(s.color, c)
            } else if s.fast(c) {
                c.laser_off.clone()
            } else {
                c.laser_on.clone()
            }
        };
        match self.kind {
            Kind::X => {
                *cum_x += self.len;
                format!("X{} {}", coord(*cum_x, c.offset_x, c), tail(self))
            }
            Kind::Y => {
                *cum_y += self.len;
                format!("Y{} {}", coord(*cum_y, c.offset_y, c), tail(self))
            }
            Kind::D => {
                *cum_x += self.len;
                *cum_y -= self.len;
                format!(
                    "X{} Y{} {}",
                    coord(*cum_x, c.offset_x, c),
                    coord(*cum_y, c.offset_y, c),
                    tail(self)
                )
            }
            Kind::VSeparator => {
                *cum_y += self.len;
                format!("Y{}", coord(*cum_y, c.offset_y, c))
            }
            Kind::HSeparator => {
                *cum_x += self.len;
                format!("X{}", coord(*cum_x, c.offset_x, c))
            }
        }
    }
}

/// `GetColor`: the power of a pixel (darker and more opaque is more power).
fn color(img: &Image, x: u32, y: u32, c: &L2lConf) -> i32 {
    let (a, r, _, _) = split(img.get(x, y));
    let rv = (255 - i32::from(r)) * i32::from(a) / 255;
    if rv == 0 {
        0
    } else if c.pwm {
        rv * (c.max_power - c.min_power) / 255 + c.min_power
    } else {
        rv
    }
}

struct Extractor<'a> {
    img: &'a Image,
    c: &'a L2lConf,
    out: Vec<Segment>,
    len: i32,
    prev: i32,
}

impl Extractor<'_> {
    fn start(&mut self) {
        self.len = -1;
        self.prev = -1;
    }

    /// `ExtractSegment`.
    fn pixel(&mut self, x: u32, y: u32, reverse: bool) {
        self.len += 1;
        let col = color(self.img, x, y, self.c);
        if self.prev == -1 {
            self.prev = col;
        }
        if self.prev != col {
            let kind = match self.c.direction {
                Direction::Horizontal => Kind::X,
                Direction::Vertical => Kind::Y,
                Direction::Diagonal => Kind::D,
            };
            self.out.push(Segment::new(kind, self.prev, self.len, reverse));
            self.len = 0;
        }
        self.prev = col;
    }
}

/// `GetSegments`.
#[allow(clippy::cast_possible_wrap)]
fn segments(img: &Image, c: &L2lConf, uni: bool) -> Vec<Segment> {
    let mut e = Extractor {
        img,
        c,
        out: Vec::new(),
        len: -1,
        prev: -1,
    };
    let (w, h) = (img.width() as i32, img.height() as i32);
    match c.direction {
        Direction::Horizontal | Direction::Vertical => {
            let horizontal = c.direction == Direction::Horizontal;
            let (lines, along) = if horizontal { (h, w) } else { (w, h) };
            for i in 0..lines {
                let d = uni || i % 2 == 0;
                e.start();
                let js: Box<dyn Iterator<Item = i32>> = if d {
                    Box::new(0..along)
                } else {
                    Box::new((0..along).rev())
                };
                for j in js {
                    #[allow(clippy::cast_sign_loss)]
                    let (x, y) = if horizontal {
                        (j as u32, i as u32)
                    } else {
                        (i as u32, j as u32)
                    };
                    e.pixel(x, y, !d);
                }
                let kind = if horizontal { Kind::X } else { Kind::Y };
                let (prev, len) = (e.prev, e.len);
                e.out.push(Segment::new(kind, prev, len + 1, !d)); // close last segment
                if uni {
                    e.out.push(Segment::new(kind, 0, along, true)); // go back
                }
                if i < lines - 1 {
                    let sep = if horizontal {
                        Kind::VSeparator
                    } else {
                        Kind::HSeparator
                    };
                    e.out.push(Segment::new(sep, 0, 1, false));
                }
            }
        }
        Direction::Diagonal => {
            e.out.push(Segment::new(Kind::VSeparator, 0, 1, false));
            for slice in 0..w + h - 1 {
                let d = uni || slice % 2 == 0;
                e.start();
                let z1 = if slice < h { 0 } else { slice - h + 1 };
                let z2 = if slice < w { 0 } else { slice - w + 1 };
                let mut j = if d { z1 } else { slice - z2 };
                while if d { j <= slice - z2 } else { j >= z1 } {
                    #[allow(clippy::cast_sign_loss)]
                    e.pixel(j as u32, (slice - j) as u32, !d);
                    j = if d { j + 1 } else { j - 1 };
                }
                let (prev, len) = (e.prev, e.len);
                e.out.push(Segment::new(Kind::D, prev, len + 1, !d));
                if uni {
                    e.out.push(Segment::new(Kind::D, 0, (slice - z1 - z2) + 1, true)); // go back
                }
                let sep = if slice < w.min(h) - 1 {
                    if d && !uni {
                        Kind::HSeparator
                    } else {
                        Kind::VSeparator
                    }
                } else if slice >= w.max(h) - 1 {
                    if d && !uni {
                        Kind::VSeparator
                    } else {
                        Kind::HSeparator
                    }
                } else if w > h {
                    Kind::HSeparator
                } else {
                    Kind::VSeparator
                };
                e.out.push(Segment::new(sep, 0, 1, false));
            }
        }
    }
    e.out
}

/// `(decimal)float`: the value rounded to 7 significant digits.
fn decimal_of_float(v: f32) -> f64 {
    if v == 0.0 {
        return 0.0;
    }
    format!("{:.6e}", f64::from(v))
        .parse()
        .unwrap_or_else(|_| f64::from(v))
}

/// `ImageLine2Line` + `OptimizeLine2Line`: the moves of the image (already upside
/// down, as `LoadImageL2L` flips it), runs of fast moves merged into one.
#[must_use]
pub fn image_line_to_line(img: &Image, c: &L2lConf, opts: &L2lOptions) -> Vec<String> {
    let skip = if opts.disable_g0 { "G1" } else { "G0" };
    let mut fast = true;
    let (mut cum_x, mut cum_y) = (0, 0);
    let mut temp: Vec<String> = Vec::new();
    for seg in segments(img, c, opts.unidirectional) {
        let change = fast != seg.fast(c);
        if seg.is_separator() && !fast {
            temp.push(if c.pwm {
                "S0".to_owned()
            } else {
                c.laser_off.clone()
            });
        }
        fast = seg.fast(c);
        let text = seg.gcode(&mut cum_x, &mut cum_y, c);
        temp.push(if change {
            format!("{} {text}", if fast { skip } else { "G1" })
        } else {
            text
        });
    }

    // OptimizeLine2Line: fast moves between burns are merged into one
    let mut rv = Vec::with_capacity(temp.len());
    let mut cur_x = decimal_of_float(c.offset_x);
    let mut cur_y = decimal_of_float(c.offset_y);
    let mut cumulate = false;
    let num = |v: f64| double_format(v, "0.###");
    for text in temp {
        let cmd = Command::new(&text);
        let old = cumulate;
        if c.pwm {
            if let Some(s) = cmd.get('S') {
                cumulate = s.is_zero();
            }
        } else if cmd.is_laser_off() {
            cumulate = true;
        } else if cmd.is_laser_on() {
            cumulate = false;
        }
        if old && !cumulate {
            rv.push(if c.pwm {
                format!("{skip} X{} Y{} S0", num(cur_x), num(cur_y))
            } else {
                format!("{skip} X{} Y{} {}", num(cur_x), num(cur_y), c.laser_off)
            });
        }
        if cmd.is_movement() {
            if let Some(x) = cmd.get('X') {
                cur_x = freekerf_gcode::number::f64_from_decimal(x);
            }
            if let Some(y) = cmd.get('Y') {
                cur_y = freekerf_gcode::number::f64_from_decimal(y);
            }
        }
        if !cmd.is_movement() || !cumulate {
            rv.push(text);
        }
    }
    rv
}

/// `LoadImageL2L`: the whole raster job (move to the offset at mark speed, laser on at
/// zero power or off with the power set, the lines, laser off).
#[must_use]
pub fn load_image_l2l(img: &Image, c: &L2lConf, opts: &L2lOptions) -> Vec<String> {
    let skip = if opts.disable_g0 { "G1" } else { "G0" };
    let flipped = img.flipped_y();
    let num = |v: f32| double_format(f64::from(v), "0.###");
    let mut list = vec![format!(
        "{skip} X{} Y{} F{}",
        num(c.offset_x),
        num(c.offset_y),
        c.mark_speed
    )];
    list.push(if c.pwm {
        format!("{} S0", c.laser_on)
    } else {
        format!("{} S{}", c.laser_off, opts.max_pwm)
    });
    list.extend(image_line_to_line(&flipped, c, opts));
    list.push(c.laser_off.clone());
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{BLACK, WHITE};

    fn img(rows: &[&str]) -> Image {
        let px: Vec<u32> = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| if c == '#' { BLACK } else { WHITE }))
            .collect();
        Image::from_pixels(
            u32::try_from(rows[0].len()).unwrap(),
            u32::try_from(rows.len()).unwrap(),
            px,
        )
        .unwrap()
    }

    #[test]
    fn a_horizontal_job() {
        let i = img(&["..##", "#..."]);
        let l = load_image_l2l(&i, &L2lConf::default(), &L2lOptions::default());
        assert_eq!(l[0], "G0 X0 Y0 F1000");
        assert_eq!(l[1], "M3 S0");
        assert_eq!(l.last().unwrap(), "M5");
        assert!(l.iter().any(|s| s.contains("S1000")));
    }

    #[test]
    fn directions_and_switching() {
        let i = img(&["#..#", ".##.", "#..#"]);
        for dir in [Direction::Horizontal, Direction::Vertical, Direction::Diagonal] {
            for uni in [false, true] {
                for pwm in [true, false] {
                    let c = L2lConf {
                        direction: dir,
                        pwm,
                        smoothie: !pwm,
                        ..L2lConf::default()
                    };
                    let o = L2lOptions {
                        unidirectional: uni,
                        disable_g0: uni,
                        max_pwm: 1000,
                    };
                    let l = load_image_l2l(&i, &c, &o);
                    assert!(l.len() > 4, "{dir:?} {uni} {pwm}");
                }
            }
        }
        assert_eq!(coord(3, 0.5, &L2lConf::default()), "0.8");
        assert_eq!(
            power(
                128,
                &L2lConf {
                    smoothie: true,
                    ..L2lConf::default()
                }
            ),
            "S0.50"
        );
        assert_eq!(decimal_of_float(12.5), 12.5);
        assert_eq!(decimal_of_float(0.0), 0.0);
        assert_eq!(decimal_of_float(0.1), 0.1);
    }
}
