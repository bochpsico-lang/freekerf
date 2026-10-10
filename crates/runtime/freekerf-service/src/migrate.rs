//! LaserGRBL migration (`freekerf migrate lasergrbl`, later the welcome flow): reads
//! LaserGRBL's data folder with `freekerf-migrate` and, when asked, writes the
//! result into FreeKerf's settings and machine cache. LaserGRBL's files are only
//! read.

use std::path::{Path, PathBuf};

use freekerf_settings::{MachineCache, Settings};
use serde::Serialize;

use crate::Stores;

/// LaserGRBL's file names in its data folder.
const SETTINGS_FILE: &str = "LaserGRBL.Settings.bin";
const LIFE_FILE: &str = "LaserLifeCounter.bin";
const MATERIALS_FILE: &str = "UserMaterials.psh";

/// What a migration found and did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MigrationReport {
    /// The LaserGRBL data folder read.
    pub source: String,
    /// LaserGRBL setting → FreeKerf setting, for every value taken.
    pub settings_applied: Vec<(String, String)>,
    /// LaserGRBL settings not taken, with the reason.
    pub settings_skipped: Vec<(String, String)>,
    /// Laser module counters found (names).
    pub laser_counters: Vec<String>,
    /// Whether `UserMaterials.psh` exists (imported with `freekerf-materials` and
    /// a library package; storing user materials comes with the material library UI).
    pub user_materials_found: bool,
    /// Files that could not be read, with the error.
    pub errors: Vec<(String, String)>,
    /// Whether the result was written.
    pub applied: bool,
}

/// LaserGRBL's data folder on this system (`%APPDATA%\LaserGRBL` on Windows).
#[must_use]
pub fn default_lasergrbl_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("LaserGRBL"))
}

/// Migrates LaserGRBL's settings and laser counters from `from`. With `apply`,
/// the settings are merged into FreeKerf's settings file and the counters into
/// the machine cache (a counter with the same id is replaced).
///
/// # Errors
/// FreeKerf's own files cannot be read or written.
pub fn lasergrbl(from: &Path, stores: &Stores, apply: bool) -> Result<MigrationReport, String> {
    let mut report = MigrationReport {
        source: from.display().to_string(),
        ..MigrationReport::default()
    };
    let read = |name: &str| freekerf_platform::read_optional(&from.join(name)).map_err(|e| e.to_string());

    let mut settings = match &stores.settings {
        Some(s) => Settings::load(s.as_ref()).map_err(|e| e.to_string())?,
        None => Settings::default(),
    };
    match read(SETTINGS_FILE) {
        Ok(Some(bytes)) => match freekerf_migrate::settings::read(&bytes) {
            Ok(lg) => {
                let r = lg.apply(&mut settings);
                report.settings_applied = r.applied;
                report.settings_skipped = r.skipped;
            }
            Err(e) => report.errors.push((SETTINGS_FILE.into(), e.to_string())),
        },
        Ok(None) => {}
        Err(e) => report.errors.push((SETTINGS_FILE.into(), e)),
    }

    let mut counters = Vec::new();
    match read(LIFE_FILE) {
        Ok(Some(bytes)) => match freekerf_migrate::laser_life::read(&bytes) {
            Ok(c) => counters = c,
            Err(e) => report.errors.push((LIFE_FILE.into(), e.to_string())),
        },
        Ok(None) => {}
        Err(e) => report.errors.push((LIFE_FILE.into(), e)),
    }
    report.laser_counters = counters.iter().map(|c| c.name.clone()).collect();
    report.user_materials_found = from.join(MATERIALS_FILE).is_file();

    if apply {
        if let Some(s) = &stores.settings {
            settings.save(s.as_ref()).map_err(|e| e.to_string())?;
        }
        if let Some(c) = &stores.cache {
            let mut cache = c
                .load()
                .map_err(|e| e.to_string())?
                .map(|t| MachineCache::from_json(&t))
                .unwrap_or_default();
            for counter in counters {
                cache.laser_life.retain(|k| k.id != counter.id);
                cache.laser_life.push(counter);
            }
            c.save(&cache.to_json()).map_err(|e| e.to_string())?;
        }
        report.applied = true;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/freekerf-migrate/tests/fixtures"
    );

    #[test]
    fn migration_report_and_apply() {
        let home = tempfile::tempdir().unwrap();
        let stores = Stores::files(Some(home.path())).unwrap();
        let dry = lasergrbl(Path::new(FIXTURES), &stores, false).unwrap();
        assert!(!dry.applied && dry.errors.is_empty());
        assert_eq!(dry.settings_applied.len(), 15);
        assert_eq!(dry.laser_counters, ["Default", "Diode 10 W"]);
        assert!(!dry.user_materials_found);
        // a dry run writes nothing
        assert!(stores.settings.as_deref().unwrap().load().unwrap().is_none());

        let done = lasergrbl(Path::new(FIXTURES), &stores, true).unwrap();
        assert!(done.applied);
        let s = Settings::load(stores.settings.as_deref().unwrap()).unwrap();
        assert_eq!(s.machine.baud, 250_000);
        let cache = MachineCache::from_json(&stores.cache.as_deref().unwrap().load().unwrap().unwrap());
        assert_eq!(cache.laser_life.len(), 2);
        // again: counters replaced, not duplicated
        let _ = lasergrbl(Path::new(FIXTURES), &stores, true).unwrap();
        let cache = MachineCache::from_json(&stores.cache.as_deref().unwrap().load().unwrap().unwrap());
        assert_eq!(cache.laser_life.len(), 2);
    }

    #[test]
    fn unreadable_or_missing_files() {
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join(SETTINGS_FILE), b"junk").unwrap();
        std::fs::write(src.path().join(LIFE_FILE), b"junk").unwrap();
        std::fs::write(src.path().join(MATERIALS_FILE), b"<x/>").unwrap();
        let r = lasergrbl(src.path(), &Stores::default(), true).unwrap();
        assert_eq!(r.errors.len(), 2);
        assert!(r.user_materials_found && r.applied);
        let empty = tempfile::tempdir().unwrap();
        let r = lasergrbl(empty.path(), &Stores::default(), false).unwrap();
        assert!(r.errors.is_empty() && r.laser_counters.is_empty());
        let _ = default_lasergrbl_dir();
    }
}
