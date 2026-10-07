//! Paths of shared fixtures.

use std::path::PathBuf;

/// The workspace root.
#[must_use]
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// `crates/core/freekerf-gcode/tests/fixtures/<rel>` (conformance G-code and goldens).
#[must_use]
pub fn gcode_fixture(rel: &str) -> PathBuf {
    workspace_root().join("crates/core/freekerf-gcode/tests/fixtures").join(rel)
}
