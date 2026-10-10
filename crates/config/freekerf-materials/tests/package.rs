//! A library package built here with `freekerf-library`'s own packaging function
//! (no published release needed), read back and used.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::Path;

use freekerf_document::{Color, Layer, LayerId, LayerMode};
use freekerf_library::layout::Layout;
use freekerf_library::package::{BuildOptions, build};
use freekerf_materials::model::{LaserKind, MaterialCategory, Operation};
use freekerf_materials::{MaterialLibrary, Query, apply_recipe};

const PLYWOOD: &str = r#"id = "plywood-3mm"
name = "Plywood, birch"
category = "engineered_wood"
thickness_mm = 3

[[recipes]]
id = "diode-10w-cut"
operation = "cut"
laser = { kind = "diode", power_w = 10 }
speed_mm_min = 300
power_min_pct = 100
power_max_pct = 100
passes = 2
confidence = { level = "estimated", method = "Starting values, run a test grid first." }

[[recipes]]
id = "co2-40w-engrave"
operation = "engrave_raster"
laser = { kind = "co2", power_w = 40 }
speed_mm_min = 12000
power_min_pct = 10
power_max_pct = 25
passes = 1
line_interval_mm = 0.1
confidence = { level = "estimated", method = "Starting values, run a test grid first." }
"#;

const LEATHER: &str = r#"id = "leather-2mm"
name = "Leather, vegetable tanned"
category = "leather"
thickness_mm = 2

[[recipes]]
id = "diode-5w-engrave"
operation = "engrave_vector"
laser = { kind = "diode", power_w = 5 }
speed_mm_min = 1500
power_min_pct = 0
power_max_pct = 40
passes = 1
confidence = { level = "estimated", method = "Starting values, run a test grid first." }
"#;

const MACHINE: &str = r#"id = "generic-diode-10w"
manufacturer = "Generic"
model = "Diode 10 W"
origin = "front_left"
max_speed_mm_min = 6000

[work_area]
x_mm = 400
y_mm = 400

[firmware]
kind = "grbl"

[features]
air_assist = false
rotary = false
camera = false
z_axis = false

[laser]
kind = "diode"
power_w = 10
"#;

const HAZARDS: &str = r#"[[hazards]]
id = "pvc"
name = "PVC / vinyl"
severity = "block"
keywords = ["pvc", "vinyl"]
reason = "Releases hydrogen chloride."
emissions = ["hydrogen chloride"]

[[hazards]]
id = "leather"
name = "Leather"
severity = "warn"
keywords = ["leather"]
reason = "Chrome-tanned leather releases toxic fumes; ventilate."
emissions = ["chromium compounds"]
"#;

const PSH: &str = r#"<?xml version="1.0" standalone="yes"?>
<MaterialDB xmlns="http://tempuri.org/MaterialDB.xsd">
  <Materials>
    <id>3d3e2cff-f117-4df7-a7c3-09e4fd62ce44</id>
    <Visible>true</Visible>
    <Model>My diode (5W)</Model>
    <Material>Cork</Material>
    <Thickness>3</Thickness>
    <Action>Cut</Action>
    <Power>100</Power>
    <Speed>400</Speed>
    <Cycles>2</Cycles>
  </Materials>
  <Materials>
    <id>d22539c7-dd4a-41b6-b004-c5de2f5fdea2</id>
    <Visible>true</Visible>
    <Model>My diode (5W)</Model>
    <Material>PVC sheet</Material>
    <Thickness>2</Thickness>
    <Action>Cut</Action>
    <Power>100</Power>
    <Speed>400</Speed>
    <Cycles>1</Cycles>
  </Materials>
</MaterialDB>
"#;

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn package(dir: &Path) -> std::path::PathBuf {
    let data = dir.join("library");
    write(&data, "data/materials/engineered_wood/plywood-3mm.toml", PLYWOOD);
    write(&data, "data/materials/leather/leather-2mm.toml", LEATHER);
    write(&data, "data/machines/generic/generic-diode-10w.toml", MACHINE);
    write(&data, "data/safety/hazards.toml", HAZARDS);
    let layout = Layout::new(&data);
    let (library, diagnostics) = freekerf_library::check(&layout);
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    let out = build(
        &library,
        &layout,
        &BuildOptions {
            version: semver::Version::new(0, 3, 1),
            out_dir: dir.join("dist"),
            mtime: 0,
        },
    )
    .unwrap();
    out.archive
}

#[test]
fn packages_are_read_searched_and_applied() {
    let dir = tempfile::tempdir().unwrap();
    let lib = MaterialLibrary::open(&package(dir.path())).unwrap();
    assert_eq!(lib.version(), "0.3.1");
    assert_eq!(lib.materials().len(), 2);
    assert!(lib.get("plywood-3mm").is_some() && lib.get("nope").is_none());
    assert!(lib.needs_warning("leather-2mm"));
    assert!(!lib.needs_warning("plywood-3mm"));
    assert!(lib.hazards("nope").is_empty());
    assert_eq!(lib.hazard_list().hazards.len(), 2);

    let ids = |q: Query| lib.search(&q).iter().map(|m| m.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(Query::default()).len(), 2);
    assert_eq!(
        ids(Query {
            text: Some("BIRCH ply".into()),
            ..Query::default()
        }),
        ["plywood-3mm"]
    );
    assert_eq!(
        ids(Query {
            category: Some(MaterialCategory::Leather),
            ..Query::default()
        }),
        ["leather-2mm"]
    );
    assert_eq!(
        ids(Query {
            operation: Some(Operation::Cut),
            ..Query::default()
        }),
        ["plywood-3mm"]
    );
    assert_eq!(
        ids(Query {
            laser: Some(LaserKind::Co2),
            ..Query::default()
        }),
        ["plywood-3mm"]
    );
    assert_eq!(
        ids(Query {
            thickness_mm: Some(2.0),
            ..Query::default()
        }),
        ["leather-2mm"]
    );
    assert!(
        ids(Query {
            text: Some("acrylic".into()),
            ..Query::default()
        })
        .is_empty()
    );

    let ply = lib.get("plywood-3mm").unwrap();
    let mut layer = Layer::new(LayerId(0), Color::BLACK, 1000, 500);
    apply_recipe(&ply.recipes[0], &mut layer, 1000);
    assert_eq!(
        (layer.mode, layer.speed_mm_min, layer.power, layer.passes),
        (LayerMode::Cut, 300, 1000, 2)
    );
    apply_recipe(&ply.recipes[1], &mut layer, 255);
    assert_eq!((layer.mode, layer.power), (LayerMode::Fill, 64));
    assert!((layer.fill.lines_per_mm - 10.0).abs() < 1e-9);
    apply_recipe(&lib.get("leather-2mm").unwrap().recipes[0], &mut layer, 1000);
    assert_eq!((layer.mode, layer.power), (LayerMode::Line, 400));
}

#[test]
fn lasergrbl_user_materials() {
    let dir = tempfile::tempdir().unwrap();
    let lib = MaterialLibrary::open(&package(dir.path())).unwrap();
    let imported = lib.import_lasergrbl(PSH).unwrap();
    // PVC is blocked by the hazard list: only the cork comes in
    assert_eq!(imported.len(), 1);
    assert!(imported[0].name.contains("Cork"));
    assert_eq!(imported[0].recipes[0].laser.kind, LaserKind::Diode);
    assert!(lib.import_lasergrbl("<not psh").is_err());
}

#[test]
fn tampered_or_missing_packages_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let archive = package(dir.path());
    assert!(MaterialLibrary::open(&dir.path().join("missing.tar.zst")).is_err());
    let mut bytes = fs::read(&archive).unwrap();
    let n = bytes.len();
    bytes[n / 2] ^= 0xFF;
    let bad = dir.path().join("bad.tar.zst");
    fs::write(&bad, bytes).unwrap();
    let e = MaterialLibrary::open(&bad).unwrap_err();
    assert!(e.to_string().contains("material package"));
}
