//! Any bytes as a LaserGRBL data file (BinaryFormatter): settings and laser counters.
#![no_main]

use freekerf_migrate::{laser_life, nrbf, settings};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = nrbf::read(data);
    if let Ok(s) = settings::read(data) {
        let _ = s.apply(&mut freekerf_settings::Settings::default());
    }
    let _ = laser_life::read(data);
});
