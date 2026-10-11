//! Any bytes as a DXF file: import, then plan the document.
#![no_main]

use freekerf_import::dxf::{DxfOptions, import};
use freekerf_planner::{PlanOptions, plan};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(imp) = import(data, &DxfOptions::default()) {
        let _ = plan(&imp.into_document_with_defaults(1000, 1000, None), &PlanOptions::default());
    }
});
