//! The reader never panics: random bytes and damaged LaserGRBL files give errors
//! or values, never a crash.

use freekerf_migrate::{laser_life, nrbf, settings};
use proptest::prelude::*;

const SETTINGS: &[u8] = include_bytes!("fixtures/LaserGRBL.Settings.bin");
const LIFE: &[u8] = include_bytes!("fixtures/LaserLifeCounter.bin");

proptest! {
    #[test]
    fn random_bytes(data in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = nrbf::read(&data);
        let mut with_header = vec![0u8];
        with_header.extend_from_slice(&data);
        let _ = nrbf::read(&with_header);
    }

    #[test]
    fn damaged_files(pos in 0usize..4000, byte in any::<u8>(), cut in 0usize..4000) {
        for file in [SETTINGS, LIFE] {
            let mut d = file.to_vec();
            let p = pos % d.len();
            d[p] = byte;
            let _ = settings::read(&d);
            let _ = laser_life::read(&d);
            let _ = nrbf::read(&file[..cut.min(file.len())]);
        }
    }
}
