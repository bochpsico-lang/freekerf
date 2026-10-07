//! Test pattern generators (`GrblFile.GenerateCuttingTest`, `GenerateGreyscaleTest`,
//! `GenerateShakeTest`): jobs made directly as G-code, with Hershey labels.

use freekerf_gcode::dotnet::double_format;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::hershey;

fn num(v: f64) -> String {
    double_format(v, "0.###")
}

/// Parameters of the cutting test: squares cut with more passes per row and a
/// different speed per column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CuttingTest {
    /// Columns (speeds).
    pub f_col: i32,
    /// Speed of the first column, mm/min.
    pub f_start: i32,
    /// Speed of the last column, mm/min.
    pub f_end: i32,
    /// Passes of the first row.
    pub p_start: i32,
    /// Passes of the last row.
    pub p_end: i32,
    /// Power (`S`).
    pub s_fixed: i32,
    /// Label speed, mm/min.
    pub f_text: i32,
    /// Label power.
    pub s_text: i32,
    /// Title (empty: the default).
    pub title: String,
    /// Laser on command (`M3`/`M4`).
    pub ton: String,
}

/// `GenerateCuttingTest`.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn cutting_test(t: &CuttingTest) -> Vec<String> {
    let p_row = t.p_end - t.p_start + 1;
    let (ox, oy) = (3.0, 3.0);
    let x_size = t.f_col * 14 - 4;
    let y_size = p_row * 14 - 4;
    let f_delta = if t.f_col > 1 {
        f64::from(t.f_end - t.f_start) / f64::from(t.f_col - 1)
    } else {
        0.0
    };
    let ton = &t.ton;
    let mut list = vec![
        format!("G0 X{} Y{} S{}", num(ox), num(oy), num(f64::from(t.s_fixed))),
        format!("G1 {ton} F{}", num(f64::from(t.f_start))),
    ];
    for p in 0..p_row {
        let cy = oy + 14.0 * f64::from(p);
        for f in 0..t.f_col {
            let cx = ox + 14.0 * f64::from(f);
            for _ in 0..t.p_start + p {
                list.push(format!("G0 X{} Y{}", num(cx), num(cy)));
                list.push(format!(
                    "G1 X{} F{} {ton}",
                    num(cx + 10.0),
                    num(f64::from(t.f_start) + f_delta * f64::from(f))
                ));
                list.push(format!("G1 Y{}", num(cy + 10.0)));
                list.push(format!("G1 X{}", num(cx)));
                list.push(format!("G1 Y{}", num(cy)));
                list.push("M5".into());
            }
        }
    }
    list.push(format!("G0 X{} Y{} S0", num(ox), num(oy)));
    list.push(format!("G1 {ton} F{}", num(f64::from(t.f_text))));
    let (ft, st) = (i64::from(t.f_text), i64::from(t.s_text));
    for x in 0..t.f_col {
        let label = format!("F{}", (f64::from(t.f_start) + f64::from(x) * f_delta) as i32);
        list.extend(hershey::text(
            &label,
            f64::from(x * 14) + 5.0 + ox,
            oy / 2.0,
            ft,
            st,
            true,
            "M4",
        ));
    }
    for y in 0..p_row {
        let label = format!("{}pass", t.p_start + y);
        list.extend(hershey::text(
            &label,
            ox / 2.0,
            f64::from(y * 14) + 5.0 + oy,
            ft,
            st,
            false,
            "M4",
        ));
    }
    let srange = format!("S{}", t.s_fixed);
    let frange = if t.f_start == t.f_end {
        format!("F{}", t.f_end)
    } else {
        format!("F{} - F{}", t.f_start, t.f_end)
    };
    // F-39: LaserGRBL writes "1 - F2 pass" for a pass range (DIV-039: kept)
    let prange = if t.p_start == t.p_end {
        format!("{} pass", t.p_end)
    } else {
        format!("{} - F{} pass", t.p_start, t.p_end)
    };
    let title = if t.title.is_empty() {
        "LaserGRBL cutting test".to_owned()
    } else {
        format!("LaserGRBL cutting test [{}]", t.title)
    };
    let parmessage = format!("{srange}, {frange}, {prange}, {ton}");
    let half = f64::from(x_size / 2);
    list.extend(hershey::text(
        &title,
        half + ox,
        f64::from(y_size) + oy + oy + oy / 2.0,
        ft,
        st,
        true,
        "M4",
    ));
    list.extend(hershey::text(
        &parmessage,
        half + ox,
        f64::from(y_size) + oy + oy / 2.0,
        ft,
        st,
        true,
        "M4",
    ));
    list.push("M5".into());
    list
}

/// Parameters of the power/speed (greyscale) test: a grid of filled cells, power
/// changing along X and speed along Y.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GreyscaleTest {
    /// Rows (speeds).
    pub f_row: i32,
    /// Columns (powers).
    pub s_col: i32,
    /// Speed of the first row.
    pub f_start: i32,
    /// Speed of the last row.
    pub f_end: i32,
    /// Power of the first column.
    pub s_start: i32,
    /// Power of the last column.
    pub s_end: i32,
    /// Width, mm.
    pub x_size: i32,
    /// Height, mm.
    pub y_size: i32,
    /// Filling lines per mm.
    pub resolution: f64,
    /// Grid speed.
    pub f_grid: i32,
    /// Grid power.
    pub s_grid: i32,
    /// Title (empty: the default).
    pub title: String,
    /// Label speed.
    pub f_text: i32,
    /// Label power.
    pub s_text: i32,
    /// Laser on command.
    pub ton: String,
}

/// `GenerateGreyscaleTest`.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
pub fn greyscale_test(t: &GreyscaleTest) -> Vec<String> {
    let (ox, oy) = (3.0, 3.0);
    let f_delta = if t.f_row > 1 {
        f64::from(t.f_end - t.f_start) / f64::from(t.f_row - 1)
    } else {
        0.0
    };
    let s_delta = if t.s_col > 1 {
        f64::from(t.s_end - t.s_start) / f64::from(t.s_col - 1)
    } else {
        0.0
    };
    let x_size = f64::from(t.x_size);
    let y_size = f64::from(t.y_size);
    let x_step = x_size / f64::from(t.s_col);
    let y_step = y_size / f64::from(t.f_row);
    let filling_step = 1.0 / t.resolution;
    let ton = &t.ton;

    let mut list = vec![
        format!("G0 X{} Y{} S0", num(ox), num(oy)),
        format!("G1 {ton} F{}", num(f64::from(t.f_start))),
    ];

    // filling
    let mut prev_f = f64::NAN;
    let mut forward = true;
    let mut y = 0.0;
    while y <= y_size {
        let cur_f = f64::from(t.f_start) + f64::from((y / y_step) as i32) * f_delta;
        if cur_f != prev_f {
            list.push(format!("F{}", num(cur_f)));
            prev_f = cur_f;
        }
        list.push(format!("Y{} S0", num(oy + y)));
        for x in 0..t.s_col {
            let xf = f64::from(x);
            let cx = if forward {
                (xf + 1.0) * x_step
            } else {
                x_size - (xf + 1.0) * x_step
            };
            let cs = if forward {
                f64::from(t.s_start) + xf * s_delta
            } else {
                f64::from(t.s_end) - xf * s_delta
            };
            list.push(format!("X{} S{}", num(ox + cx), num(cs)));
        }
        forward = !forward;
        y += filling_step;
    }

    // grid X
    list.push(format!("G0 X{} Y{} S0", num(ox), num(oy)));
    list.push(format!("G1 {ton} F{}", num(f64::from(t.f_grid))));
    forward = true;
    for yi in 0..=t.f_row {
        let cy = f64::from(yi) * y_step;
        list.push(format!("Y{} S0", num(oy + cy)));
        for x in 0..=t.s_col {
            let xf = f64::from(x);
            let cx = if forward {
                xf * x_step
            } else {
                x_size - xf * x_step
            };
            list.push(format!("X{} S{}", num(ox + cx), num(f64::from(t.s_grid))));
        }
        forward = !forward;
    }

    // grid Y
    list.push(format!("G0 X{} Y{} S0", num(ox), num(oy)));
    list.push(format!("G1 {ton} F{}", num(f64::from(t.f_grid))));
    forward = true;
    for x in 0..=t.s_col {
        let cx = f64::from(x) * x_step;
        list.push(format!("X{} S0", num(ox + cx)));
        for yi in 0..=t.f_row {
            let yf = f64::from(yi);
            let cy = if forward {
                yf * y_step
            } else {
                y_size - yf * y_step
            };
            list.push(format!("Y{} S{}", num(oy + cy), num(f64::from(t.s_grid))));
        }
        forward = !forward;
    }

    let (ft, st) = (i64::from(t.f_text), i64::from(t.s_text));
    for x in 0..t.s_col {
        let xf = f64::from(x);
        let label = format!("S{}", (f64::from(t.s_start) + xf * s_delta) as i32);
        list.extend(hershey::text(
            &label,
            xf * x_step + x_step / 2.0 + ox,
            oy / 2.0,
            ft,
            st,
            true,
            "M4",
        ));
    }
    for y in 0..t.f_row {
        let yf = f64::from(y);
        let label = format!("F{}", (f64::from(t.f_start) + yf * f_delta) as i32);
        list.extend(hershey::text(
            &label,
            ox / 2.0,
            yf * y_step + y_step / 2.0 + oy,
            ft,
            st,
            false,
            "M4",
        ));
    }

    let srange = if t.s_start == t.s_end {
        format!("S{}", t.s_end)
    } else {
        format!("S{} - S{}", t.s_start, t.s_end)
    };
    let frange = if t.f_start == t.f_end {
        format!("F{}", t.f_end)
    } else {
        format!("F{} - F{}", t.f_start, t.f_end)
    };
    let title = if t.title.is_empty() {
        "LaserGRBL power/speed test".to_owned()
    } else {
        format!("LaserGRBL power/speed test [{}]", t.title)
    };
    let parmessage = format!("{srange}, {frange}, {ton},  {} line/mm", num(t.resolution));
    let half = f64::from(t.x_size / 2);
    list.extend(hershey::text(
        &title,
        half + ox,
        y_size + oy + oy + oy / 2.0,
        ft,
        st,
        true,
        "M4",
    ));
    list.extend(hershey::text(
        &parmessage,
        half + ox,
        y_size + oy + oy / 2.0,
        ft,
        st,
        true,
        "M4",
    ));
    list.push("M5".into());
    list
}

/// Parameters of the shake test (axis backlash and ringing).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShakeTest {
    /// `X` or `Y`.
    pub axis: String,
    /// Speed of the oscillations, mm/min.
    pub flimit: i32,
    /// Length of the axis to test, mm.
    pub axislen: i32,
    /// Power of the crosses.
    pub cpower: i32,
    /// Speed of the crosses.
    pub cspeed: i32,
}

fn cross(list: &mut Vec<String>, t: &ShakeTest) {
    let (cspeed, cpower) = (t.cspeed, t.cpower);
    list.push(format!("G1 F{cspeed} X7 Y10"));
    list.push("M4".into());
    list.push(format!("G1 F{cspeed} S{cpower} X13 Y10"));
    list.push("M5".into());
    list.push(format!("G1 F{cspeed} X10 Y7"));
    list.push("M4".into());
    list.push(format!("G1 F{cspeed} S{cpower} X10 Y13"));
    list.push("M5".into());
}

/// `GenerateShakeTest`.
#[must_use]
pub fn shake_test(t: &ShakeTest) -> Vec<String> {
    let mut list = vec!["M5".to_owned(), "G1 F1000 X0 Y0 S0".to_owned()];
    cross(&mut list, t);
    list.push("G1 F1000 X10 Y10 S0".into());
    for (trip, step) in [(50, 0.5), (100, 2.0), (200, 4.0), (400, 8.0)] {
        shake(&mut list, t, 10, trip, step);
    }
    list.push(format!("G1 F{} X10 Y10 S0", t.cspeed));
    cross(&mut list, t);
    list.push("G1 F1000 X0 Y0 S0".into());
    list
}

/// `GenerateShakeTest2`: oscillations around the centres of `trip`-long stretches.
fn shake(list: &mut Vec<String>, t: &ShakeTest, o: i32, trip: i32, step: f64) {
    let mut c = trip / 2;
    while c < t.axislen - trip / 2 {
        let mut i = 0.0;
        while i < f64::from(trip / 3) {
            list.push(format!(
                "G1 F{} {}{}",
                t.flimit,
                t.axis,
                num(f64::from(o + c) + i)
            ));
            list.push(format!(
                "G1 F{} {}{}",
                t.flimit,
                t.axis,
                num(f64::from(o + c) - i)
            ));
            i += step;
        }
        c += trip;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutting_test_structure() {
        let t = CuttingTest {
            f_col: 2,
            f_start: 500,
            f_end: 1000,
            p_start: 1,
            p_end: 2,
            s_fixed: 1000,
            f_text: 1000,
            s_text: 500,
            title: String::new(),
            ton: "M3".into(),
        };
        let l = cutting_test(&t);
        assert_eq!(l[0], "G0 X3 Y3 S1000");
        assert_eq!(l[1], "G1 M3 F500");
        // 2 columns x (1 + 2) passes x 6 lines
        assert_eq!(l.iter().filter(|s| s.starts_with("G1 Y")).count(), 2 * 3 * 2);
        assert_eq!(l.last().unwrap(), "M5");
        let titled = cutting_test(&CuttingTest {
            title: "x".into(),
            p_end: 1,
            f_end: 500,
            f_col: 1,
            ..t
        });
        assert!(titled.len() < l.len());
    }

    #[test]
    fn greyscale_and_shake_tests() {
        let g = greyscale_test(&GreyscaleTest {
            f_row: 2,
            s_col: 2,
            f_start: 1000,
            f_end: 1000,
            s_start: 100,
            s_end: 100,
            x_size: 10,
            y_size: 10,
            resolution: 1.0,
            f_grid: 2000,
            s_grid: 50,
            title: "t".into(),
            f_text: 1000,
            s_text: 300,
            ton: "M4".into(),
        });
        assert_eq!(g[2], "F1000");
        assert!(g.contains(&"X8 S100".to_owned()));
        let s = shake_test(&ShakeTest {
            axis: "Y".into(),
            flimit: 5000,
            axislen: 100,
            cpower: 10,
            cspeed: 600,
        });
        assert_eq!(s[0], "M5");
        assert!(s.iter().any(|l| l.starts_with("G1 F5000 Y")));
        assert_eq!(s.last().unwrap(), "G1 F1000 X0 Y0 S0");
    }
}
