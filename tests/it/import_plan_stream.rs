//! Import → plan → stream: SVG, DXF and images become jobs that the service streams
//! to the emulator over a serial PTY; every job ends without errors with the head
//! where the program ends.

use std::path::Path;
use std::time::{Duration, Instant};

use freekerf_document::{Bytes, ImageData, ImageObject, RasterSettings, RasterTool, VectorFill};
use freekerf_gcode::number::f64_from_decimal;
use freekerf_gcode::state::MachineState;
use freekerf_gcode::{AnalysisOptions, Program};
use freekerf_geometry::Point;
use freekerf_geometry::hatch::FillPattern;
use freekerf_import::{dxf, svg};
use freekerf_planner::{PlanOptions, plan};
use freekerf_raster::image::{BLACK, Image, WHITE};
use freekerf_service::{Caller, Service, ServiceEvent};
use serde_json::{Value, json};

const INPUTS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../crates/runtime/freekerf-service/tests/fixtures/importers/inputs"
);

fn input(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(INPUTS).join(name)).unwrap()
}

fn wait(s: &Service, pred: impl Fn(&Value) -> bool) -> Value {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let st = s.dispatch(&Caller::user(), "state.get", Value::Null).unwrap();
        if pred(&st) {
            return st;
        }
        assert!(Instant::now() < end, "timeout: {st}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Where the program leaves the head (absolute or relative moves).
fn end_position(lines: &[String]) -> (f64, f64) {
    let program = Program::parse(&lines.join("\n"), &AnalysisOptions::default());
    let mut state = MachineState::new(true);
    let (mut x, mut y) = (0.0, 0.0);
    for c in program.commands() {
        state.analyze(c, None);
        let abs = state.is_absolute();
        if let Some(v) = c.get('X') {
            let v = f64_from_decimal(v);
            x = if abs { v } else { x + v };
        }
        if let Some(v) = c.get('Y') {
            let v = f64_from_decimal(v);
            y = if abs { v } else { y + v };
        }
    }
    (x, y)
}

fn ring_png() -> Vec<u8> {
    let px = (0..24 * 16)
        .map(|i| {
            let (x, y) = (i % 24, i / 24);
            let inside = (3..21).contains(&x) && (3..13).contains(&y);
            let hole = (7..17).contains(&x) && (6..10).contains(&y);
            if inside && !hole { BLACK } else { WHITE }
        })
        .collect();
    Image::from_pixels(24, 16, px).unwrap().encode_png().unwrap()
}

fn image_job(tool: RasterTool, fill: Option<VectorFill>) -> Vec<String> {
    let mut settings = RasterSettings {
        tool,
        lines_per_mm: 5.0,
        ..RasterSettings::default()
    };
    settings.vectorize.fill = fill;
    let doc = freekerf_planner::image::single_image_document(ImageObject {
        data: ImageData::Encoded {
            media_type: "image/png".into(),
            bytes: Bytes(ring_png()),
        },
        position: Point::new(5.0, 5.0),
        size_mm: [12.0, 8.0],
        settings,
        edits: Vec::new(),
    });
    plan(&doc, &PlanOptions::default()).unwrap().lines
}

fn jobs() -> Vec<(&'static str, Vec<String>)> {
    let svg_job = |name: &str| {
        let imp = svg::import(&input(name), &svg::SvgOptions::default()).unwrap();
        let doc = imp.into_document_with_defaults(2000, 800, None);
        plan(&doc, &PlanOptions::default()).unwrap().lines
    };
    let dxf_job = |name: &str| {
        let imp = dxf::import(&input(name), &dxf::DxfOptions::default()).unwrap();
        let doc = imp.into_document_with_defaults(1500, 1000, None);
        plan(&doc, &PlanOptions::default()).unwrap().lines
    };
    vec![
        ("svg paths", svg_job("svg/paths.svg")),
        ("svg colours", svg_job("svg/colors.svg")),
        ("dxf lines and arcs", dxf_job("dxf/lines_arcs.dxf")),
        ("dxf curves", dxf_job("dxf/curves.dxf")),
        ("image line by line", image_job(RasterTool::LineToLine, None)),
        ("image dithering", image_job(RasterTool::Dithering, None)),
        (
            "image vectorize, hatch",
            image_job(RasterTool::Vectorize, Some(VectorFill::Hatch(FillPattern::Cross))),
        ),
    ]
}

#[test]
fn imported_jobs_stream_to_the_emulator() {
    let emu = freekerf_transport::pty::serve_emulator(freekerf_emulator::DeviceConfig::default()).unwrap();
    let s = Service::in_memory().unwrap();
    let user = Caller::user();
    // no custom footer (the default one sends the head home): the test checks that
    // the head stops where the program ends. Host options are taken at connect.
    s.dispatch(&user, "settings.set", json!({"key": "job.footer", "value": ""}))
        .unwrap();
    s.dispatch(&user, "machine.connect", json!({"port": emu.path()}))
        .unwrap();
    wait(&s, |st| {
        st["machine"]["ready"] == true && st["machine"]["status"] == "Idle"
    });
    let dir = tempfile::tempdir().unwrap();
    for (name, lines) in jobs() {
        assert!(lines.len() > 3, "{name}: {lines:?}");
        let file = dir.path().join("job.nc");
        std::fs::write(&file, lines.join("\n")).unwrap();
        let events = s.subscribe();
        s.dispatch(&user, "job.load", json!({"path": file.to_string_lossy()}))
            .unwrap();
        let loaded = s.dispatch(&user, "state.get", Value::Null).unwrap();
        let program_lines = loaded["program"]["lines"].as_u64().unwrap();
        s.dispatch(&user, "job.run", Value::Null).unwrap();
        let end = Instant::now() + Duration::from_secs(60);
        let mut done = None;
        while done.is_none() && Instant::now() < end {
            if let Ok(e @ ServiceEvent::Machine(_)) = events.recv_timeout(Duration::from_millis(100))
                && let Some(j) = e.to_json()["data"].get("JobEnded")
            {
                done = Some(j.clone());
            }
        }
        let done = done.unwrap_or_else(|| panic!("{name}: the job did not end"));
        assert_eq!(done["errors"], 0, "{name}");
        assert_eq!(done["lines"].as_u64(), Some(program_lines), "{name}");
        // status reports come every few hundred ms: wait for one after the job
        let (x, y) = end_position(&lines);
        let at_end = |st: &Value| {
            let wp = &st["machine"]["work_position"];
            let (wx, wy) = (
                wp["x"].as_f64().unwrap_or(f64::NAN),
                wp["y"].as_f64().unwrap_or(f64::NAN),
            );
            st["machine"]["status"] == "Idle" && (wx - x).abs() < 0.01 && (wy - y).abs() < 0.01
        };
        let end = Instant::now() + Duration::from_secs(10);
        let mut st = s.dispatch(&user, "state.get", Value::Null).unwrap();
        while !at_end(&st) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
            st = s.dispatch(&user, "state.get", Value::Null).unwrap();
        }
        assert!(
            at_end(&st),
            "{name}: program ends at ({x}, {y}), machine {}",
            st["machine"]
        );
    }
    s.dispatch(&user, "machine.disconnect", Value::Null).unwrap();
    emu.stop();
}
