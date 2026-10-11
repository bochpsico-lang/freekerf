//! Settings files on disk: migration of an old file, persistence of changes.

use std::sync::Arc;

use freekerf_service::{Caller, FileStore, Service, Stores};
use serde_json::{Value, json};

#[test]
fn an_unversioned_file_is_migrated_and_saved_as_the_current_version() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.toml");
    std::fs::write(&file, "[machine]\nstreaming = \"RepeatOnError\"\nbaud = 250000\n").unwrap();
    let stores = Stores {
        settings: Some(Arc::new(FileStore(file.clone()))),
        cache: None,
        ..Stores::default()
    };
    let s = Service::new(stores).unwrap();
    let user = Caller::user();
    let all = s.dispatch(&user, "settings.get", Value::Null).unwrap();
    assert_eq!(all["version"], 2);
    assert_eq!(all["machine"]["streaming_mode"], "RepeatOnError");
    assert_eq!(all["machine"]["baud"], 250_000);
    s.dispatch(&user, "settings.set", json!({"key": "jog.step_mm", "value": 0.1}))
        .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("version = 2") && text.contains("step_mm = 0.1") && !text.contains("streaming ="));
}

#[test]
fn a_file_from_a_newer_freekerf_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("settings.toml");
    std::fs::write(&file, "version = 99\n").unwrap();
    let e = Service::new(Stores {
        settings: Some(Arc::new(FileStore(file))),
        cache: None,
        ..Stores::default()
    })
    .unwrap_err();
    assert!(e.to_string().contains("newer"));
}
