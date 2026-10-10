//! Any bytes as an SVG file: import, then plan the document (fillings included).
#![no_main]

use freekerf_document::LayerMode;
use freekerf_import::svg::{SvgOptions, import};
use freekerf_planner::{PlanOptions, plan};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(imp) = import(data, &SvgOptions::default()) else {
        return;
    };
    let mut doc = imp.into_document_with_defaults(1000, 1000, None);
    // alternate the layer modes so the fillings run too
    for (i, l) in doc.layers.iter_mut().enumerate() {
        l.mode = [LayerMode::Line, LayerMode::FillAndLine, LayerMode::Cut][i % 3];
    }
    let _ = plan(&doc, &PlanOptions::default());
});
