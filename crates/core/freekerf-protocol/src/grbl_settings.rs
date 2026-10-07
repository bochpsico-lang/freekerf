//! The board's `$$` settings table (`GrblConfST`).

use std::collections::BTreeMap;
use std::str::FromStr;

use freekerf_gcode::Decimal;
use freekerf_gcode::TimingConfig;
use freekerf_gcode::command::parse_settings_line;
use serde::{Deserialize, Serialize};

use crate::version::FirmwareVersion;

const MAX_SPEED: i64 = 2_000_000;
const MAX_PWM: i64 = 2_000_000;
const MAX_RESOLUTION: i64 = 10_000;
const MAX_SIZE: i64 = 2_000_000;
const MAX_ACCEL: i64 = 2_000_000;

/// Settings read with `$$` (number → raw value), and the version they came from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrblSettings {
    version: Option<FirmwareVersion>,
    values: BTreeMap<u32, String>,
}

impl GrblSettings {
    /// An empty table for `version`.
    #[must_use]
    pub const fn new(version: Option<FirmwareVersion>) -> Self {
        Self {
            version,
            values: BTreeMap::new(),
        }
    }

    /// A table with values.
    #[must_use]
    pub fn with_values<I: IntoIterator<Item = (u32, String)>>(
        version: Option<FirmwareVersion>,
        values: I,
    ) -> Self {
        Self {
            version,
            values: values.into_iter().collect(),
        }
    }

    /// Version the values belong to.
    #[must_use]
    pub const fn version(&self) -> Option<&FirmwareVersion> {
        self.version.as_ref()
    }

    /// Number of values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// True without values.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Raw value of `$n`.
    #[must_use]
    pub fn get(&self, n: u32) -> Option<&str> {
        self.values.get(&n).map(String::as_str)
    }

    /// Iterates `(number, value)` in number order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &str)> {
        self.values.iter().map(|(k, v)| (*k, v.as_str()))
    }

    fn v11(&self) -> bool {
        self.version.as_ref().is_some_and(FirmwareVersion::is_v11)
    }

    fn v09(&self) -> bool {
        self.version.as_ref().is_some_and(FirmwareVersion::is_v09)
    }

    /// How many settings `$$` should return for this version.
    #[must_use]
    pub fn expected_count(&self) -> usize {
        if self.v11() {
            34
        } else if self.v09() {
            31
        } else {
            23
        }
    }

    /// `$22` (`$17` before 0.9): homing cycle enabled.
    #[must_use]
    pub fn homing_enabled(&self) -> bool {
        !self
            .read(Some(if self.v09() { 22 } else { 17 }), 1, 0, 1)
            .is_zero()
    }

    /// `$110`: maximum X rate, mm/min.
    #[must_use]
    pub fn max_rate_x(&self) -> Decimal {
        self.read(Some(if self.v09() { 110 } else { 4 }), 4000, 1, MAX_SPEED)
    }

    /// `$111`: maximum Y rate, mm/min.
    #[must_use]
    pub fn max_rate_y(&self) -> Decimal {
        self.read(Some(if self.v09() { 111 } else { 5 }), 4000, 1, MAX_SPEED)
    }

    /// `$32`: laser mode (true when the version is unknown).
    #[must_use]
    pub fn laser_mode(&self) -> bool {
        if self.version.is_none() {
            return true;
        }
        !self.read(self.v11().then_some(32), 0, 0, 1).is_zero()
    }

    /// `$31`: minimum spindle (power) value.
    #[must_use]
    pub fn min_pwm(&self) -> Decimal {
        self.read(self.v11().then_some(31), 0, 0, MAX_PWM)
    }

    /// `$30`: maximum spindle (power) value.
    #[must_use]
    pub fn max_pwm(&self) -> Decimal {
        self.read(self.v11().then_some(30), 1000, 1, MAX_PWM)
    }

    /// `$100`: X steps/mm.
    #[must_use]
    pub fn resolution_x(&self) -> Decimal {
        self.read(Some(if self.v09() { 100 } else { 0 }), 250, 1, MAX_RESOLUTION)
    }

    /// `$101`: Y steps/mm.
    #[must_use]
    pub fn resolution_y(&self) -> Decimal {
        self.read(Some(if self.v09() { 101 } else { 1 }), 250, 1, MAX_RESOLUTION)
    }

    /// `$130`: X travel, mm.
    #[must_use]
    pub fn table_width(&self) -> Decimal {
        self.read(self.v09().then_some(130), 300, 1, MAX_SIZE)
    }

    /// `$131`: Y travel, mm.
    #[must_use]
    pub fn table_height(&self) -> Decimal {
        self.read(self.v09().then_some(131), 200, 1, MAX_SIZE)
    }

    /// `$20`: soft limits.
    #[must_use]
    pub fn soft_limit(&self) -> bool {
        !self.read(Some(20), 0, 0, 1).is_zero()
    }

    /// Mean of `$120` and `$121`, mm/s².
    #[must_use]
    pub fn acceleration_xy(&self) -> Decimal {
        let x = self.read(self.v09().then_some(120), 2000, 1, MAX_ACCEL);
        let y = self.read(self.v09().then_some(121), 2000, 1, MAX_ACCEL);
        (x + y) / Decimal::TWO
    }

    /// `$74`: Wi-Fi SSID (Ortur).
    #[must_use]
    pub fn wifi_ssid(&self) -> Option<&str> {
        self.read_string(74)
    }

    /// `$75`: Wi-Fi password (Ortur).
    #[must_use]
    pub fn wifi_password(&self) -> Option<&str> {
        self.read_string(75)
    }

    /// `$305`: telnet port (default 23).
    #[must_use]
    pub fn telnet_port(&self) -> &str {
        self.read_string(305).unwrap_or("23")
    }

    /// The limits the time estimate needs.
    #[must_use]
    pub fn timing(&self) -> TimingConfig {
        TimingConfig {
            max_rate_x_mm_min: self.max_rate_x(),
        }
    }

    fn read_string(&self, n: u32) -> Option<&str> {
        self.version.as_ref()?;
        self.get(n)
    }

    /// Number of `$key`, clamped; LaserGRBL falls back to the first number in the text
    /// (`500.5 mm/min`). Parsed invariantly (FINDINGS F-13 fixed: DIV-013).
    fn read(&self, key: Option<u32>, default: i64, min: i64, max: i64) -> Decimal {
        let default = Decimal::from(default);
        let value = match (self.version.as_ref(), key.and_then(|k| self.values.get(&k))) {
            (Some(_), Some(raw)) => parse_lenient(raw).unwrap_or(default),
            _ => default,
        };
        value.min(Decimal::from(max)).max(Decimal::from(min))
    }

    /// Adds or replaces a value from a `$N=V` line (Foxalien descriptions removed).
    pub fn add_or_update(&mut self, line: &str) {
        if let Some((n, value)) = parse_settings_line(line) {
            let value = foxalien_cleanup(value).unwrap_or(value);
            self.values.insert(n, value.to_owned());
        }
    }

    /// Sets a value only when the key exists; true on success.
    pub fn set_if_exists(&mut self, line: &str) -> bool {
        match parse_settings_line(line) {
            Some((n, value)) if self.values.contains_key(&n) => {
                self.values.insert(n, value.to_owned());
                true
            }
            _ => false,
        }
    }

    /// Gives a table that belongs to no version (no `$$` read yet) the board's
    /// version, so the values written since are used (DIV-016).
    pub fn adopt_version(&mut self, version: Option<&FirmwareVersion>) {
        if self.version.is_none() {
            self.version = version.cloned();
        }
    }

    /// Sets `$n` directly.
    pub fn set(&mut self, n: u32, value: impl Into<String>) {
        self.values.insert(n, value.into());
    }

    /// True when `value` differs from the stored one (or the key is new).
    #[must_use]
    pub fn has_changes(&self, n: u32, value: &str) -> bool {
        self.get(n) != Some(value)
    }

    /// Refuses changes LaserGRBL protects (Ortur's `$33` safety feature).
    #[must_use]
    pub fn validate(&self, n: u32) -> Option<&'static str> {
        (n == 33 && self.version.as_ref().is_some_and(FirmwareVersion::is_ortur))
            .then_some("This param control an Ortur safety feature. Please do not change this value!")
    }
}

fn parse_lenient(raw: &str) -> Option<Decimal> {
    if let Ok(v) = Decimal::from_str(raw.trim()) {
        return Some(v);
    }
    // first `\d+\.?\d*`
    let start = raw.find(|c: char| c.is_ascii_digit())?;
    let rest = &raw[start..];
    let int = rest.bytes().take_while(u8::is_ascii_digit).count();
    let mut end = int;
    if rest[int..].starts_with('.') {
        end += 1 + rest[int + 1..].bytes().take_while(u8::is_ascii_digit).count();
    }
    Decimal::from_str(rest[..end].trim_end_matches('.')).ok()
}

/// `$20=0 (soft limits, bool)` → `0`.
fn foxalien_cleanup(value: &str) -> Option<&str> {
    let num_end = value.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let (num, rest) = value.split_at(num_end);
    let rest = rest.trim_start();
    (!num.is_empty() && num.as_bytes()[0].is_ascii_digit() && rest.starts_with('(') && rest.ends_with(')'))
        .then_some(num)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v11() -> Option<FirmwareVersion> {
        Some(FirmwareVersion::with_build(1, 1, 'f'))
    }

    #[test]
    fn defaults_without_version() {
        let s = GrblSettings::with_values(None, [(110, "6000".to_owned())]);
        assert_eq!(s.max_rate_x(), Decimal::from(4000));
        assert!(s.laser_mode());
        assert_eq!(s.expected_count(), 23);
        assert_eq!(s.wifi_ssid(), None);
        assert_eq!(s.telnet_port(), "23");
    }

    #[test]
    fn reads_v11_keys_and_clamps() {
        let mut s = GrblSettings::new(v11());
        for l in [
            "$110=6000",
            "$111=5000.5",
            "$32=1",
            "$30=1000",
            "$31=0",
            "$22=1",
            "$20=1",
        ] {
            s.add_or_update(l);
        }
        s.add_or_update("$130=400 (x travel, mm)");
        s.add_or_update("$131=99999999");
        s.add_or_update("$120=500.5 mm/sec^2");
        assert_eq!(s.expected_count(), 34);
        assert_eq!(s.max_rate_x(), Decimal::from(6000));
        assert_eq!(s.max_rate_y().to_string(), "5000.5");
        assert!(s.laser_mode() && s.homing_enabled() && s.soft_limit());
        assert_eq!(s.max_pwm(), Decimal::from(1000));
        assert_eq!(s.min_pwm(), Decimal::ZERO);
        assert_eq!(s.table_width(), Decimal::from(400));
        assert_eq!(s.table_height(), Decimal::from(2_000_000));
        assert_eq!(s.acceleration_xy(), Decimal::new(125_025, 2));
        assert_eq!(s.resolution_x(), Decimal::from(250));
        assert_eq!(s.resolution_y(), Decimal::from(250));
        assert_eq!(s.timing().max_rate_x_mm_min, Decimal::from(6000));
        assert_eq!(s.get(130), Some("400"));
        assert_eq!(s.len(), 10);
        assert!(!s.is_empty());
    }

    #[test]
    fn older_versions_use_other_keys() {
        let mut s = GrblSettings::new(Some(FirmwareVersion::new(0, 8)));
        s.add_or_update("$4=1234");
        s.add_or_update("$17=0");
        assert_eq!(s.max_rate_x(), Decimal::from(1234));
        assert!(!s.homing_enabled());
        assert!(!s.laser_mode());
        assert_eq!(s.table_width(), Decimal::from(300));
        assert_eq!(
            GrblSettings::new(Some(FirmwareVersion::new(0, 9))).expected_count(),
            31
        );
    }

    #[test]
    fn unparseable_values_fall_back() {
        let mut s = GrblSettings::new(v11());
        s.set(110, "fast");
        assert_eq!(s.max_rate_x(), Decimal::from(4000));
        s.set(110, "about 500.5 mm/min");
        assert_eq!(s.max_rate_x().to_string(), "500.5");
        s.set(110, "7.");
        assert_eq!(s.max_rate_x(), Decimal::from(7));
    }

    #[test]
    fn updates_and_validation() {
        let mut s = GrblSettings::new(v11());
        assert!(!s.set_if_exists("$1=2"));
        s.add_or_update("$1=25");
        assert!(s.set_if_exists("$1 = 30"));
        assert_eq!(s.get(1), Some("30"));
        assert!(!s.set_if_exists("garbage"));
        // a table without version ignores its values until it adopts one
        let mut t = GrblSettings::default();
        t.add_or_update("$130=500");
        assert_eq!(t.table_width(), Decimal::from(300));
        t.adopt_version(Some(&FirmwareVersion::new(1, 1)));
        assert_eq!(t.table_width(), Decimal::from(500));
        t.adopt_version(Some(&FirmwareVersion::new(0, 9)));
        assert!(t.version().is_some_and(FirmwareVersion::is_v11));
        assert!(s.has_changes(1, "31") && !s.has_changes(1, "30") && s.has_changes(2, "x"));
        assert_eq!(s.iter().count(), 1);
        assert_eq!(s.validate(33), None);
        let ortur = GrblSettings::new(Some(FirmwareVersion::with_vendor(
            1,
            1,
            'f',
            Some("Ortur".into()),
            None,
            false,
        )));
        assert!(ortur.validate(33).is_some());
        assert_eq!(ortur.version().unwrap().major, 1);
    }

    #[test]
    fn wifi_strings() {
        let mut s = GrblSettings::new(v11());
        s.set(74, "home");
        s.set(75, "secret");
        s.set(305, "2323");
        assert_eq!(s.wifi_ssid(), Some("home"));
        assert_eq!(s.wifi_password(), Some("secret"));
        assert_eq!(s.telnet_port(), "2323");
    }
}
