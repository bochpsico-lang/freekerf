//! Invariants of the raster pipeline on random images.

use freekerf_gcode::report::Report;
use freekerf_gcode::{AnalysisOptions, Program};
use freekerf_raster::dither::{DitherMode, dither};
use freekerf_raster::image::Image;
use freekerf_raster::l2l::{Direction, L2lConf, L2lOptions, load_image_l2l};
use freekerf_raster::ops::{self, Formula, Interpolation};
use proptest::prelude::*;

fn image() -> impl Strategy<Value = Image> {
    (1u32..12, 1u32..12).prop_flat_map(|(w, h)| {
        proptest::collection::vec(any::<u32>(), (w * h) as usize)
            .prop_map(move |px| Image::from_pixels(w, h, px).unwrap_or_else(|_| Image::new(1, 1, 0)))
    })
}

fn direction() -> impl Strategy<Value = Direction> {
    prop_oneof![
        Just(Direction::Horizontal),
        Just(Direction::Vertical),
        Just(Direction::Diagonal)
    ]
}

proptest! {
    #[test]
    fn operations_keep_the_size(img in image(), w in 1u32..20, h in 1u32..20, t in 0.0f32..1.0) {
        let size = (img.width(), img.height());
        for out in [
            ops::grayscale(&img, 1.0, 1.0, 1.0, 0.0, 1.0, Formula::WeightAverage),
            ops::invert(&img),
            ops::threshold(&img, t, true),
            ops::whitenize(&img, 128, false),
            ops::outline(&img, (0, 0)),
            ops::fill(&img, (0, 0), 0xFF00_0000, 10).0,
        ] {
            prop_assert_eq!((out.width(), out.height()), size);
        }
        for filter in [Interpolation::NearestNeighbor, Interpolation::Bilinear, Interpolation::Bicubic] {
            let r = ops::resize(&img, w, h, true, filter);
            prop_assert_eq!((r.width(), r.height()), (w, h));
        }
    }

    #[test]
    fn dithering_gives_black_and_white(img in image(), seed in any::<u64>()) {
        for mode in [DitherMode::FloydSteinberg, DitherMode::Stucki, DitherMode::Random] {
            let d = dither(&img, mode, seed);
            prop_assert!(d.pixels().iter().all(|&p| p & 0x00FF_FFFF == 0 || p & 0x00FF_FFFF == 0x00FF_FFFF));
        }
    }

    #[test]
    fn line_to_line_stays_on_the_image(img in image(), dir in direction(), res in 1.0f64..20.0, max in 1i32..1000) {
        let conf = L2lConf { res, direction: dir, max_power: max, ..L2lConf::default() };
        let lines = load_image_l2l(&img, &conf, &L2lOptions::default());
        let program = Program::parse(&lines.join("\n"), &AnalysisOptions::default());
        let w = f64::from(img.width()) / res;
        let h = f64::from(img.height()) / res;
        if let Some([x0, y0, x1, y1]) = Report::new("l2l", &program).drawing_range {
            let eps = 2.0 / res;
            prop_assert!(x0 >= -eps && y0 >= -eps && x1 <= w + eps && y1 <= h + eps, "{x0} {y0} {x1} {y1} {w} {h}");
        }
    }
}
