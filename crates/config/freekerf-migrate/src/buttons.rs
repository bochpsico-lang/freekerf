//! LaserGRBL custom buttons → FreeKerf scripts (PLAN §9.2).
//!
//! LaserGRBL keeps its buttons in `CustomButtons.bin` and exports them as `.zbn`
//! files (its own `StandardButtons.zbn` is one): a `List<CustomButton>` saved by
//! `Tools.Serializer.ObjToFile`, compressed (raw deflate) in `.zbn` files. A button
//! has a caption, a tooltip, an icon, G-code with `[expr]` expressions, a second
//! G-code for two-state and push buttons, and when it is enabled.
//!
//! Each piece of G-code becomes a script that runs it with
//! `app.machine.customCode`, which evaluates the same expressions (`[left]`,
//! `[$30*3/100]`...) when the script runs, so every button migrates. That action
//! is tier `laser`: running a migrated button always needs a person's
//! confirmation, as anything that can turn the laser on.

use serde::Serialize;

use crate::MigrateError;
use crate::nrbf::{self, Value};

/// Where `ObjToFile`'s header ends.
const DATA_START: &[u8] = b"<!-- OBJECT DATA BEGIN HERE-->\r\n";
/// Largest decompressed `.zbn` accepted.
const MAX_INFLATED: usize = 32 << 20;

/// What a button does when used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonKind {
    /// Runs its code when clicked.
    Button,
    /// Alternates: the first click runs the code, the second the second code.
    TwoState,
    /// Runs the code when pressed and the second code when released.
    Push,
}

/// When LaserGRBL enables a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnableWhen {
    /// Always.
    Always,
    /// While connected.
    Connected,
    /// While the machine is idle.
    Idle,
    /// While a job runs.
    Run,
    /// While idle with a program loaded.
    IdleWithProgram,
}

/// A LaserGRBL custom button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaserGrblButton {
    /// Caption (often empty when there is an icon).
    pub caption: String,
    /// Tooltip.
    pub tooltip: String,
    /// G-code with `[expr]` parts.
    pub gcode: String,
    /// Second G-code (two-state and push buttons).
    pub gcode2: String,
    /// When it is enabled.
    pub enable: EnableWhen,
    /// Kind.
    pub kind: ButtonKind,
    /// Icon (PNG, as LaserGRBL serialized it).
    pub icon_png: Option<Vec<u8>>,
}

/// The `BinaryFormatter` data of a file written by `Tools.Serializer.ObjToFile`,
/// inflated when the header says it is compressed.
///
/// # Errors
/// No header, an encrypted file, or bad compressed data.
pub fn object_data(bytes: &[u8]) -> Result<Vec<u8>, MigrateError> {
    let end = bytes
        .windows(DATA_START.len())
        .take(1024)
        .position(|w| w == DATA_START)
        .ok_or_else(|| MigrateError::Content("no serializer header".into()))?;
    let header = String::from_utf8_lossy(&bytes[..end]);
    if header.contains("Encription: Enabled") {
        return Err(MigrateError::Content("encrypted file".into()));
    }
    let data = &bytes[end + DATA_START.len()..];
    if header.contains("Compression: Enabled") {
        miniz_oxide::inflate::decompress_to_vec_with_limit(data, MAX_INFLATED)
            .map_err(|e| MigrateError::Content(format!("bad compressed data: {e:?}")))
    } else {
        Ok(data.to_vec())
    }
}

fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn png(v: Option<&Value>) -> Option<Vec<u8>> {
    let bytes = v?.member("Data")?.as_array()?;
    let out: Option<Vec<u8>> = bytes
        .iter()
        .map(|b| b.as_i64().and_then(|n| u8::try_from(n).ok()))
        .collect();
    out.filter(|b| b.starts_with(b"\x89PNG"))
}

/// Reads `CustomButtons.bin` or a `.zbn` export.
///
/// # Errors
/// Not a button list.
pub fn read(bytes: &[u8]) -> Result<Vec<LaserGrblButton>, MigrateError> {
    let root = nrbf::read(&object_data(bytes)?)?;
    if !root.class().is_some_and(|c| c.contains("LaserGRBL.CustomButton")) {
        return Err(MigrateError::Content(format!(
            "not a button list: {:?}",
            root.class()
        )));
    }
    let size = root
        .member("_size")
        .and_then(Value::as_i64)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    let items = root
        .member("_items")
        .and_then(Value::as_array)
        .unwrap_or_default();
    Ok(items
        .iter()
        .take(size)
        .filter(|b| b.class().is_some_and(|c| c.ends_with("CustomButton")))
        .map(|b| LaserGrblButton {
            caption: text(b.member("Caption")),
            tooltip: text(b.member("ToolTip")),
            gcode: text(b.member("GCode")),
            gcode2: text(b.member("GCode2")),
            enable: match b.member("EnableStyle").and_then(Value::as_i64) {
                Some(1) => EnableWhen::Connected,
                Some(3) => EnableWhen::Idle,
                Some(4) => EnableWhen::Run,
                Some(10) => EnableWhen::IdleWithProgram,
                _ => EnableWhen::Always,
            },
            kind: match b.member("ButtonType").and_then(Value::as_i64) {
                Some(1) => ButtonKind::TwoState,
                Some(2) => ButtonKind::Push,
                _ => ButtonKind::Button,
            },
            icon_png: png(b.member("Image")),
        })
        .collect())
}

/// A migrated button: its scripts and how the app shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MigratedButton {
    /// Name (the caption, else the tooltip, else `button N`).
    pub name: String,
    /// Tooltip.
    pub tooltip: String,
    /// Kind.
    pub kind: ButtonKind,
    /// When it is enabled.
    pub enable_when: EnableWhen,
    /// Script run on click (or press).
    pub script: String,
    /// File of [`MigratedButton::script`] in the macro folder.
    pub script_file: String,
    /// Two-state: second click; push: release.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub second_script: Option<String>,
    /// File of the second script.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub second_script_file: Option<String>,
    /// Icon file (PNG) in the macro folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_file: Option<String>,
}

fn slug(name: &str) -> String {
    let mut s = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    let s = s.trim_end_matches('-');
    if s.is_empty() {
        "button".into()
    } else {
        s.chars().take(40).collect()
    }
}

/// The JavaScript of one piece of button G-code.
#[must_use]
pub fn script(name: &str, what: &str, gcode: &str) -> String {
    // LaserGRBL separates lines with CRLF; customCode takes any line ending
    let code = gcode.replace("\r\n", "\n");
    let literal = serde_json::to_string(&code).unwrap_or_else(|_| "\"\"".into());
    let title = serde_json::to_string(name).unwrap_or_default();
    format!(
        "// Migrated from the LaserGRBL button {title} ({what}).\n\
         // `[expr]` parts are evaluated when the code runs, as in LaserGRBL.\n\
         app.machine.customCode({{ code: {literal} }});\n"
    )
}

/// The buttons as scripts, with unique file names (`frame.js`, `air.js` and
/// `air.off.js`, `frame.png`...).
#[must_use]
pub fn to_scripts(buttons: &[LaserGrblButton]) -> Vec<MigratedButton> {
    let mut used = std::collections::HashSet::new();
    buttons
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let name = [b.caption.trim(), b.tooltip.trim()]
                .into_iter()
                .find(|s| !s.is_empty())
                .map_or_else(|| format!("button {}", i + 1), str::to_owned);
            let base = slug(&name);
            let mut file = base.clone();
            let mut n = 2;
            while !used.insert(file.clone()) {
                file = format!("{base}-{n}");
                n += 1;
            }
            let (first, second) = match b.kind {
                ButtonKind::Button => ("click", None),
                ButtonKind::TwoState => ("first click", Some(("second click", "off"))),
                ButtonKind::Push => ("press", Some(("release", "release"))),
            };
            let second = second.filter(|_| !b.gcode2.trim().is_empty());
            MigratedButton {
                tooltip: b.tooltip.clone(),
                kind: b.kind,
                enable_when: b.enable,
                script: script(&name, first, &b.gcode),
                script_file: format!("{file}.js"),
                second_script: second.map(|(what, _)| script(&name, what, &b.gcode2)),
                second_script_file: second.map(|(_, suffix)| format!("{file}.{suffix}.js")),
                icon_file: b.icon_png.as_ref().map(|_| format!("{file}.png")),
                name,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_scripts() {
        assert_eq!(slug("Frame (3%)"), "frame-3");
        assert_eq!(slug("  "), "button");
        assert_eq!(slug("Été"), "t");
        let s = script("Air \"x\"", "click", "M8\r\nG0 X[left]\\");
        assert!(
            s.contains(r#"app.machine.customCode({ code: "M8\nG0 X[left]\\" });"#),
            "{s}"
        );
        assert!(s.contains(r#"button "Air \"x\"""#));
    }

    #[test]
    fn headers() {
        assert!(object_data(b"junk").is_err());
        let enc = b"<!-- Encription: Enabled -->\r\n<!-- OBJECT DATA BEGIN HERE-->\r\n\0";
        assert!(object_data(enc).is_err());
        let bad = b"<!-- Compression: Enabled -->\r\n<!-- OBJECT DATA BEGIN HERE-->\r\n\xff\xff";
        assert!(object_data(bad).is_err());
    }
}
