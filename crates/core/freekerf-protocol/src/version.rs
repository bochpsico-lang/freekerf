//! Firmware version as reported by the welcome banner (`GrblVersionInfo`).

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// Version of the firmware on the board, with vendor flags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirmwareVersion {
    /// Major (`1` in `1.1f`).
    pub major: u32,
    /// Minor (`1` in `1.1f`).
    pub minor: u32,
    /// Build letter (`f` in `1.1f`), `None` when absent.
    pub build: Option<char>,
    /// Vendor/model string seen before the banner (Ortur, Longer, SimpleLaser...).
    pub vendor_info: Option<String>,
    /// Vendor firmware version (Ortur OLF, Longer software, Vigo build).
    pub vendor_version: Option<String>,
    /// grblHAL banner.
    pub hal: bool,
}

impl FirmwareVersion {
    /// `major.minor` without build or vendor.
    #[must_use]
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor, build: None, vendor_info: None, vendor_version: None, hal: false }
    }

    /// `major.minor` + build letter.
    #[must_use]
    pub const fn with_build(major: u32, minor: u32, build: char) -> Self {
        Self { major, minor, build: Some(build), vendor_info: None, vendor_version: None, hal: false }
    }

    /// Full version with vendor data.
    #[must_use]
    pub fn with_vendor(
        major: u32,
        minor: u32,
        build: char,
        vendor_info: Option<String>,
        vendor_version: Option<String>,
        hal: bool,
    ) -> Self {
        Self { major, minor, build: Some(build), vendor_info, vendor_version, hal }
    }

    /// Ortur (or Aufero) board.
    #[must_use]
    pub fn is_ortur(&self) -> bool {
        self.vendor_info.as_deref().is_some_and(|v| v.contains("Ortur") || v.contains("Aufero"))
    }

    /// Longer board.
    #[must_use]
    pub fn is_longer(&self) -> bool {
        self.vendor_info.as_deref().is_some_and(|v| v.contains("Longer") || v.contains("NanoDuo"))
    }

    /// Boards with the "lucky" Wi-Fi module (precedence kept from LaserGRBL, FINDINGS F-09).
    #[must_use]
    pub fn is_lucky_wifi(&self) -> bool {
        let info = self.vendor_info.as_deref();
        (self.is_ortur() && info == Some("Ortur Laser Master 3"))
            || (self.is_longer() && info == Some("Longer Nano"))
            || info == Some("NanoDuo")
    }

    /// Vendor name for display: Ortur, Vigotec, Emulator or Unknown (F-05 fixed: DIV-005).
    #[must_use]
    pub fn vendor_name(&self) -> &'static str {
        let lower = self.vendor_info.as_deref().map(str::to_lowercase);
        match lower.as_deref() {
            Some(v) if v.contains("ortur") => "Ortur",
            Some(v) if v.contains("vigotec") || v.contains("grbl-vigo") => "Vigotec",
            _ if self.build == Some('#') => "Emulator",
            _ => "Unknown",
        }
    }

    /// Ortur firmware number (`OLF 170` → 170), -1 when unknown.
    #[must_use]
    pub fn ortur_fw_number(&self) -> i64 {
        self.vendor_version.as_deref().and_then(|v| v.parse().ok()).unwrap_or(-1)
    }

    /// True for Grbl 1.1 or newer (real-time overrides, `$J=` jog, laser mode, CSV).
    #[must_use]
    pub fn is_v11(&self) -> bool {
        self.compare(&Self::new(1, 1)) != Ordering::Less
    }

    /// True for Grbl 0.9 or newer.
    #[must_use]
    pub fn is_v09(&self) -> bool {
        self.compare(&Self::new(0, 9)) != Ordering::Less
    }

    /// Compares major, minor and build letter only (vendor data is ignored).
    #[must_use]
    pub fn compare(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }

    fn key(&self) -> (u32, u32, u32) {
        (self.major, self.minor, self.build.map_or(0, u32::from))
    }
}


impl std::fmt::Display for FirmwareVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.build {
            Some(b) => write!(f, "{}.{}{}", self.major, self.minor, b),
            None => write!(f, "{}.{}", self.major, self.minor),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_ordering() {
        assert_eq!(FirmwareVersion::with_build(1, 1, 'f').to_string(), "1.1f");
        assert_eq!(FirmwareVersion::new(0, 9).to_string(), "0.9");
        let cmp = |a: FirmwareVersion, b: FirmwareVersion| a.compare(&b);
        assert_eq!(cmp(FirmwareVersion::with_build(1, 1, 'f'), FirmwareVersion::new(1, 1)), Ordering::Greater);
        assert_eq!(cmp(FirmwareVersion::with_build(1, 0, 'c'), FirmwareVersion::new(1, 1)), Ordering::Less);
        assert_eq!(cmp(FirmwareVersion::new(2, 0), FirmwareVersion::new(1, 9)), Ordering::Greater);
        assert_eq!(cmp(FirmwareVersion::new(1, 1), FirmwareVersion::new(1, 1)), Ordering::Equal);
        assert!(FirmwareVersion::new(1, 1).is_v11() && FirmwareVersion::new(0, 9).is_v09());
        assert!(!FirmwareVersion::new(0, 8).is_v09());
    }

    #[test]
    fn vendor_flags() {
        let v = |info: &str| FirmwareVersion::with_vendor(1, 1, 'f', Some(info.into()), Some("170".into()), false);
        assert!(v("Ortur Laser Master 3").is_ortur() && v("Ortur Laser Master 3").is_lucky_wifi());
        assert!(v("Aufero AL1").is_ortur());
        assert!(v("Longer Nano").is_longer() && v("Longer Nano").is_lucky_wifi());
        assert!(v("NanoDuo").is_lucky_wifi());
        assert!(!v("Ortur Laser Master 2").is_lucky_wifi());
        assert_eq!(v("Ortur X").vendor_name(), "Ortur");
        assert_eq!(v("Grbl-Vigo").vendor_name(), "Vigotec");
        assert_eq!(FirmwareVersion::with_build(1, 1, '#').vendor_name(), "Emulator");
        assert_eq!(FirmwareVersion::new(1, 1).vendor_name(), "Unknown");
        assert_eq!(v("Ortur").ortur_fw_number(), 170);
        assert_eq!(FirmwareVersion::new(1, 1).ortur_fw_number(), -1);
    }
}
