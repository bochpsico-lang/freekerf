//! Load and analyze a synthetic raster of ~1M lines (see `doc/benchmarks.md`).
#![allow(missing_docs)]

use criterion::{Criterion, criterion_group, criterion_main};
use freekerf_gcode::{AnalysisOptions, Program};

fn raster(lines: usize) -> String {
    let mut s = String::with_capacity(lines * 16);
    s.push_str("G90\nM4\n");
    let mut i = 0;
    while i < lines {
        let row = i / 500;
        s.push_str(&format!("G0 X0 Y{}.{}\n", row / 10, row % 10));
        for c in 0..499 {
            s.push_str(&format!("G1 X{}.{} S{}\n", c / 10, c % 10, (c * 7 + row) % 256));
        }
        i += 500;
    }
    s.push_str("M5\n");
    s
}

fn bench(c: &mut Criterion) {
    let text = raster(1_000_000);
    let mut g = c.benchmark_group("raster_1m");
    g.sample_size(10);
    g.bench_function("parse_and_analyze", |b| {
        b.iter(|| Program::parse(&text, &AnalysisOptions::default()));
    });
    g.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
