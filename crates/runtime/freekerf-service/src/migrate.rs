//! LaserGRBL migration (`freekerf migrate lasergrbl`, later the welcome flow): reads
//! LaserGRBL's data folder with `freekerf-migrate` and, when asked, writes the
//! result into FreeKerf's settings, machine cache and macro folder. LaserGRBL's
//! files are only read.
//!
//! Custom buttons become scripts in the macro folder (`<data>/macros`): one `.js`
//! per button action, its icon as `.png`, and `buttons.json`, the list of buttons
//! the desktop app shows (format version 1; the macro editor comes in M4).

use std::path::{Path, PathBuf};

use freekerf_settings::{MachineCache, Settings};
use serde::Serialize;

use crate::Stores;

/// LaserGRBL's file names in its data folder.
const SETTINGS_FILE: &str = "LaserGRBL.Settings.bin";
const LIFE_FILE: &str = "LaserLifeCounter.bin";
const MATERIALS_FILE: &str = "UserMaterials.psh";
const BUTTONS_FILE: &str = "CustomButtons.bin";
/// The list of migrated buttons in the macro folder.
pub const BUTTONS_INDEX: &str = "buttons.json";

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
    /// Custom buttons found (names), migrated to scripts.
    pub buttons: Vec<String>,
    /// Files that could not be read, with the error.
    pub errors: Vec<(String, String)>,
    /// Whether the result was written.
    pub applied: bool,
}

/// The macro folder of FreeKerf's data in `home` (the per-user one when `None`).
///
/// # Errors
/// No home directory.
pub fn macros_dir(home: Option<&Path>) -> Result<PathBuf, String> {
    let dirs = match home {
        Some(h) => freekerf_platform::Dirs::under(h),
        None => freekerf_platform::Dirs::user().map_err(|e| e.to_string())?,
    };
    Ok(dirs.data.join("macros"))
}

/// Writes migrated buttons into `macros`: their scripts, icons and the index.
fn write_buttons(
    macros: &Path,
    source: &str,
    buttons: &[freekerf_migrate::buttons::LaserGrblButton],
) -> Result<(), String> {
    let migrated = freekerf_migrate::buttons::to_scripts(buttons);
    let write = |name: &str, bytes: &[u8]| {
        freekerf_platform::write_atomic(&macros.join(name), bytes).map_err(|e| e.to_string())
    };
    std::fs::create_dir_all(macros).map_err(|e| format!("{}: {e}", macros.display()))?;
    for (m, b) in migrated.iter().zip(buttons) {
        write(&m.script_file, m.script.as_bytes())?;
        if let (Some(file), Some(code)) = (&m.second_script_file, &m.second_script) {
            write(file, code.as_bytes())?;
        }
        if let (Some(file), Some(png)) = (&m.icon_file, &b.icon_png) {
            write(file, png)?;
        }
    }
    let entries: Vec<serde_json::Value> = migrated
        .iter()
        .map(|m| {
            let mut v = serde_json::to_value(m).unwrap_or_default();
            // the index names the files; the code lives in them
            if let Some(o) = v.as_object_mut() {
                o.remove("script");
                o.remove("second_script");
            }
            v
        })
        .collect();
    let index = serde_json::json!({"version": 1, "migrated_from": source, "buttons": entries});
    write(
        BUTTONS_INDEX,
        serde_json::to_string_pretty(&index)
            .unwrap_or_default()
            .as_bytes(),
    )
}

/// Migrates a LaserGRBL button file (`CustomButtons.bin` or a `.zbn` export) into
/// `macros`. Returns the button names.
///
/// # Errors
/// The file is not a button list, or the macro folder cannot be written.
pub fn lasergrbl_buttons(file: &Path, macros: &Path, apply: bool) -> Result<Vec<String>, String> {
    let bytes = freekerf_platform::read(file).map_err(|e| e.to_string())?;
    let buttons = freekerf_migrate::buttons::read(&bytes).map_err(|e| e.to_string())?;
    if apply {
        write_buttons(macros, &file.display().to_string(), &buttons)?;
    }
    Ok(freekerf_migrate::buttons::to_scripts(&buttons)
        .into_iter()
        .map(|b| b.name)
        .collect())
}

/// LaserGRBL's data folder on this system (`%APPDATA%\LaserGRBL` on Windows).
#[must_use]
pub fn default_lasergrbl_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("LaserGRBL"))
}

/// Migrates LaserGRBL's settings, laser counters and custom buttons from `from`.
/// With `apply`, the settings are merged into FreeKerf's settings file, the
/// counters into the machine cache (a counter with the same id is replaced) and the
/// buttons written as scripts into `macros`.
///
/// # Errors
/// FreeKerf's own files cannot be read or written.
pub fn lasergrbl(
    from: &Path,
    stores: &Stores,
    macros: &Path,
    apply: bool,
) -> Result<MigrationReport, String> {
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
    let mut buttons = Vec::new();
    match read(BUTTONS_FILE) {
        Ok(Some(bytes)) => match freekerf_migrate::buttons::read(&bytes) {
            Ok(b) => buttons = b,
            Err(e) => report.errors.push((BUTTONS_FILE.into(), e.to_string())),
        },
        Ok(None) => {}
        Err(e) => report.errors.push((BUTTONS_FILE.into(), e)),
    }
    report.buttons = freekerf_migrate::buttons::to_scripts(&buttons)
        .into_iter()
        .map(|b| b.name)
        .collect();

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
        if !buttons.is_empty() {
            write_buttons(macros, &from.join(BUTTONS_FILE).display().to_string(), &buttons)?;
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
        let macros = macros_dir(Some(home.path())).unwrap();
        let dry = lasergrbl(Path::new(FIXTURES), &stores, &macros, false).unwrap();
        assert!(!dry.applied && dry.errors.is_empty());
        assert_eq!(dry.buttons, ["Frame", "Air", "Hold to fire", "Pause", "Home"]);
        assert!(!macros.exists());
        assert_eq!(dry.settings_applied.len(), 15);
        assert_eq!(dry.laser_counters, ["Default", "Diode 10 W"]);
        assert!(!dry.user_materials_found);
        // a dry run writes nothing
        assert!(stores.settings.as_deref().unwrap().load().unwrap().is_none());

        let done = lasergrbl(Path::new(FIXTURES), &stores, &macros, true).unwrap();
        assert!(done.applied);
        let index: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(macros.join(BUTTONS_INDEX)).unwrap()).unwrap();
        assert_eq!(index["version"], 1);
        assert_eq!(index["buttons"][1]["second_script_file"], "air.off.js");
        assert!(index["buttons"][0].get("script").is_none());
        let frame = std::fs::read_to_string(macros.join("frame.js")).unwrap();
        assert!(frame.contains("app.machine.customCode"));
        assert!(
            std::fs::read(macros.join("frame.png"))
                .unwrap()
                .starts_with(b"\x89PNG")
        );
        assert!(macros.join("air.off.js").exists());
        let s = Settings::load(stores.settings.as_deref().unwrap()).unwrap();
        assert_eq!(s.machine.baud, 250_000);
        let cache = MachineCache::from_json(&stores.cache.as_deref().unwrap().load().unwrap().unwrap());
        assert_eq!(cache.laser_life.len(), 2);
        // again: counters replaced, not duplicated
        let _ = lasergrbl(Path::new(FIXTURES), &stores, &macros, true).unwrap();
        let cache = MachineCache::from_json(&stores.cache.as_deref().unwrap().load().unwrap().unwrap());
        assert_eq!(cache.laser_life.len(), 2);
    }

    #[test]
    fn unreadable_or_missing_files() {
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join(SETTINGS_FILE), b"junk").unwrap();
        std::fs::write(src.path().join(LIFE_FILE), b"junk").unwrap();
        std::fs::write(src.path().join(MATERIALS_FILE), b"<x/>").unwrap();
        std::fs::write(src.path().join(BUTTONS_FILE), b"junk").unwrap();
        let macros = src.path().join("macros");
        let r = lasergrbl(src.path(), &Stores::default(), &macros, true).unwrap();
        assert_eq!(r.errors.len(), 3);
        assert!(r.user_materials_found && r.applied);
        assert!(!macros.exists());
        let empty = tempfile::tempdir().unwrap();
        let r = lasergrbl(empty.path(), &Stores::default(), &macros, false).unwrap();
        assert!(r.errors.is_empty() && r.laser_counters.is_empty());
        let _ = default_lasergrbl_dir();
    }

    #[test]
    fn a_zbn_export() {
        let home = tempfile::tempdir().unwrap();
        let macros = macros_dir(Some(home.path())).unwrap();
        let file = Path::new(FIXTURES).join("StandardButtons.zbn");
        let names = lasergrbl_buttons(&file, &macros, false).unwrap();
        assert_eq!(&names[..3], ["Center", "Corner", "Frame"]);
        assert!(!macros.exists());
        lasergrbl_buttons(&file, &macros, true).unwrap();
        assert!(macros.join("center.js").exists() && macros.join("center.png").exists());
        assert!(
            lasergrbl_buttons(&Path::new(FIXTURES).join("LaserLifeCounter.bin"), &macros, false).is_err()
        );
        assert!(lasergrbl_buttons(&home.path().join("missing.zbn"), &macros, false).is_err());
    }
}
