//! LaserGRBL files written by LaserGRBL's own code (`tests/fixtures/LaserGrblFiles.cs`).

#![allow(clippy::unwrap_used)]

use freekerf_migrate::nrbf::Value;
use freekerf_migrate::{laser_life, settings};
use freekerf_settings::{Firmware, Settings, Streaming};

const SETTINGS: &[u8] = include_bytes!("fixtures/LaserGRBL.Settings.bin");
const LIFE: &[u8] = include_bytes!("fixtures/LaserLifeCounter.bin");

#[test]
fn settings_are_read_and_applied() {
    let s = settings::read(SETTINGS).unwrap();
    assert_eq!(s.values["Serial Speed"], Value::Int(250_000));
    assert_eq!(s.values["Jog Step"], Value::Decimal("2.5".into()));
    assert_eq!(s.values["Ratio"], Value::Float(0.25));
    assert_eq!(s.values["Big"], Value::Int(1_234_567_890_123));
    assert_eq!(s.values["Letter"], Value::Char('x'));
    assert_eq!(s.values["Last GrblVersion known"], Value::Null);
    assert_eq!(s.values["Same string twice B"], Value::String("shared".into()));
    assert_eq!(s.values["GrayScaleConversion.Gcode.Offset.X"], Value::Float(1.5));
    assert_eq!(
        s.values["Mainform Size and Position"]
            .member("width")
            .and_then(Value::as_i64),
        Some(800)
    );
    let version = &s.values["Current LaserGRBL Version"];
    assert_eq!(version.member("_Major").and_then(Value::as_i64), Some(7));

    let mut fk = Settings::default();
    let report = s.apply(&mut fk);
    assert_eq!(fk.machine.firmware, Firmware::Smoothie);
    assert_eq!(fk.machine.baud, 250_000);
    assert_eq!(fk.machine.streaming_mode, Streaming::RepeatOnError);
    assert!(!fk.machine.reset_on_connect && !fk.machine.query_machine_info && !fk.machine.support_pwm);
    assert!(fk.machine.continuous_jog);
    assert!(fk.machine.auto_cooling.enabled);
    assert_eq!(
        (fk.machine.auto_cooling.on_s, fk.machine.auto_cooling.off_s),
        (420, 90)
    );
    assert_eq!(fk.job.header, "G90 (absolute)\nM3 S[0]");
    assert_eq!(fk.job.footer, "G0 X0 Y0 Z0\nM5");
    assert_eq!(fk.job.passes, ";pass\nG4 P1");
    assert!((fk.jog.step_mm - 2.5).abs() < 1e-12);
    assert!((fk.jog.speed_mm_min - 1500.0).abs() < 1e-12);
    assert_eq!(report.applied.len(), 15);
    assert!(
        report
            .skipped
            .iter()
            .any(|(k, r)| k == "Color Schema" && r.contains("equivalent"))
    );
    // the result is a valid settings file
    assert_eq!(Settings::from_toml(&fk.to_toml().unwrap()).unwrap(), fk);
}

#[test]
fn values_of_another_type_are_skipped() {
    let mut s = settings::read(SETTINGS).unwrap();
    for k in [
        "Serial Speed",
        "Firmware Type",
        "Streaming Mode",
        "Jog Step",
        "Jog Speed",
        "AutoCooling TOn",
        "AutoCooling",
    ] {
        s.values.insert(k.into(), Value::String("x".into()));
    }
    let mut fk = Settings::default();
    let report = s.apply(&mut fk);
    assert_eq!(fk.machine.baud, Settings::default().machine.baud);
    assert_eq!(
        report.skipped.iter().filter(|(_, r)| r.contains("type")).count(),
        7
    );
    s.values.insert("Firmware Type".into(), Value::Int(9));
    s.values.insert("Streaming Mode".into(), Value::Int(9));
    let r = s.apply(&mut Settings::default());
    assert!(r.skipped.iter().any(|(k, _)| k == "Firmware Type"));
    for (fw, want) in [
        (0, Firmware::Grbl),
        (2, Firmware::Marlin),
        (3, Firmware::VigoWork),
    ] {
        s.values.insert("Firmware Type".into(), Value::Int(fw));
        let mut fk = Settings::default();
        let _ = s.apply(&mut fk);
        assert_eq!(fk.machine.firmware, want);
    }
    for (sm, want) in [(0, Streaming::Buffered), (1, Streaming::Synchronous)] {
        s.values.insert("Streaming Mode".into(), Value::Int(sm));
        let mut fk = Settings::default();
        let _ = s.apply(&mut fk);
        assert_eq!(fk.machine.streaming_mode, want);
    }
    assert!(settings::read(&LIFE[laser_life_offset()..]).is_err());
    assert!(settings::read(b"nope").is_err());
}

fn laser_life_offset() -> usize {
    LIFE.len() - laser_life::object_data(LIFE).unwrap().len()
}

#[test]
fn laser_life_counters() {
    let c = laser_life::read(LIFE).unwrap();
    assert_eq!(c.len(), 2);
    let d = &c[0];
    assert_eq!(
        (d.id.as_str(), d.name.as_str()),
        ("0b1f6f2e-6f7c-4c1e-9f55-1d2c3b4a5e6f", "Default")
    );
    assert_eq!((d.brand.as_str(), d.model.as_str()), ("Unknown", "Unknown"));
    assert_eq!(d.monitoring_date.as_deref(), Some("2023-03-04"));
    assert_eq!(d.last_usage.as_deref(), Some("2024-05-06"));
    assert_eq!(d.run_ms, 45_000_000);
    assert_eq!(d.normalized_power_ms, 14_400_000);
    assert_eq!(d.non_zero_ms, 21_600_000);
    assert_eq!(d.classes_ms[3], 540_000);
    assert_eq!(d.optical_power_w, None);
    let s = &c[1];
    assert_eq!(
        (s.name.as_str(), s.brand.as_str(), s.model.as_str()),
        ("Diode 10 W", "Acme", "LD-10")
    );
    assert_eq!(s.optical_power_w, Some(10.0));
    assert_eq!(s.purchase_date.as_deref(), Some("2022-01-02"));
    assert_eq!(s.death_date.as_deref(), Some("2024-12-31"));
    assert_eq!(s.run_ms, 5_700_000);
    // not a counter file
    assert!(laser_life::read(b"plain").is_err());
    let mut enc = b"<!-- Encription: Enabled -->\r\n".to_vec();
    enc.extend_from_slice(b"<!-- OBJECT DATA BEGIN HERE-->\r\n");
    assert!(laser_life::object_data(&enc).is_err());
    let mut wrong = b"<!-- OBJECT DATA BEGIN HERE-->\r\n".to_vec();
    wrong.extend_from_slice(SETTINGS);
    assert!(laser_life::read(&wrong).is_err());
}
