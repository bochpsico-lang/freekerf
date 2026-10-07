//! Jog directions and the jog commands sent for them.

use freekerf_gcode::Decimal;
use freekerf_gcode::number::format_fixed;
use serde::{Deserialize, Serialize};

/// Direction of a jog step (`JogDirection`, without `Abort`/`Position`, which are
/// separate calls).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JogDirection {
    /// Back to X0 Y0.
    Home,
    /// +Y.
    N,
    /// −Y.
    S,
    /// −X.
    W,
    /// +X.
    E,
    /// −X +Y.
    NW,
    /// +X +Y.
    NE,
    /// −X −Y.
    SW,
    /// +X −Y.
    SE,
    /// +Z.
    Zup,
    /// −Z.
    Zdown,
}

impl JogDirection {
    /// Parses `N`, `NE`, `Zup`, `Home`...
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "Home" => Self::Home,
            "N" => Self::N,
            "S" => Self::S,
            "W" => Self::W,
            "E" => Self::E,
            "NW" => Self::NW,
            "NE" => Self::NE,
            "SW" => Self::SW,
            "SE" => Self::SE,
            "Zup" => Self::Zup,
            "Zdown" => Self::Zdown,
            _ => return None,
        })
    }

    const fn east(self) -> bool {
        matches!(self, Self::NE | Self::E | Self::SE)
    }

    const fn west(self) -> bool {
        matches!(self, Self::NW | Self::W | Self::SW)
    }

    const fn north(self) -> bool {
        matches!(self, Self::NW | Self::N | Self::NE)
    }

    const fn south(self) -> bool {
        matches!(self, Self::SW | Self::S | Self::SE)
    }

    /// True for the Z directions.
    #[must_use]
    pub const fn is_z(self) -> bool {
        matches!(self, Self::Zup | Self::Zdown)
    }
}

/// A jog step formatted with at least one and at most three decimals: `2.5`, `10.0`,
/// `0.25`. LaserGRBL rounds to one decimal (FINDINGS F-22 fixed: DIV-022).
#[must_use]
pub fn format_step(step: Decimal) -> String {
    let s = format_fixed(step.abs(), 3);
    let trimmed = s.trim_end_matches('0');
    if trimmed.ends_with('.') { format!("{trimmed}0") } else { trimmed.to_owned() }
}

/// A feed or speed as `float.ToString()`, invariant (FINDINGS F-21 fixed: DIV-021).
#[must_use]
pub fn format_speed(speed: f32) -> String {
    format!("{speed}")
}

/// A target coordinate with two decimals (`"0.00"`).
#[must_use]
pub fn format_coord(v: f32) -> String {
    let s = format!("{:.2}", f64::from(v));
    if s == "-0.00" { "0.00".to_owned() } else { s }
}

/// The relative axis words of a direction (`X2.5Y-2.5`).
#[must_use]
pub fn step_axes(dir: JogDirection, step: Decimal) -> String {
    let s = format_step(step);
    let mut out = String::new();
    if dir.east() {
        out.push_str(&format!("X{s}"));
    }
    if dir.west() {
        out.push_str(&format!("X-{s}"));
    }
    if dir.north() {
        out.push_str(&format!("Y{s}"));
    }
    if dir.south() {
        out.push_str(&format!("Y-{s}"));
    }
    match dir {
        JogDirection::Zdown => out.push_str(&format!("Z-{s}")),
        JogDirection::Zup => out.push_str(&format!("Z{s}")),
        _ => {}
    }
    out
}

/// The absolute targets of a continuous jog towards the table edges (`$J=G53...`).
#[must_use]
pub fn edge_axes(dir: JogDirection, width: Decimal, height: Decimal) -> String {
    let mut out = String::new();
    if dir.east() {
        out.push_str(&format!("X{}", format_fixed(width, 1)));
    }
    if dir.west() {
        out.push_str("X0.0");
    }
    if dir.north() {
        out.push_str(&format!("Y{}", format_fixed(height, 1)));
    }
    if dir.south() {
        out.push_str("Y0.0");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn steps_and_speeds() {
        assert_eq!(format_step(d("2.5")), "2.5");
        assert_eq!(format_step(d("10")), "10.0");
        assert_eq!(format_step(d("0.25")), "0.25");
        assert_eq!(format_step(d("0.04")), "0.04");
        assert_eq!(format_step(d("0.0004")), "0.0");
        assert_eq!(format_speed(800.0), "800");
        assert_eq!(format_speed(1234.5), "1234.5");
        assert_eq!(format_coord(1.0), "1.00");
        assert_eq!(format_coord(-0.001), "0.00");
    }

    #[test]
    fn axes_per_direction() {
        let s = d("1");
        assert_eq!(step_axes(JogDirection::NE, s), "X1.0Y1.0");
        assert_eq!(step_axes(JogDirection::SW, s), "X-1.0Y-1.0");
        assert_eq!(step_axes(JogDirection::NW, s), "X-1.0Y1.0");
        assert_eq!(step_axes(JogDirection::SE, s), "X1.0Y-1.0");
        assert_eq!(step_axes(JogDirection::Zup, s), "Z1.0");
        assert_eq!(step_axes(JogDirection::Zdown, s), "Z-1.0");
        assert_eq!(step_axes(JogDirection::Home, s), "");
        assert_eq!(edge_axes(JogDirection::NE, d("300"), d("200")), "X300.0Y200.0");
        assert_eq!(edge_axes(JogDirection::SW, d("300"), d("200")), "X0.0Y0.0");
        assert!(JogDirection::Zup.is_z() && !JogDirection::N.is_z());
        for name in ["Home", "N", "S", "W", "E", "NW", "NE", "SW", "SE", "Zup", "Zdown"] {
            assert!(JogDirection::parse(name).is_some());
        }
        assert_eq!(JogDirection::parse("Up"), None);
    }
}
