# Benchmarks

```bash
cargo bench -p freekerf-gcode            # with rayon (default)
cargo bench -p freekerf-gcode --no-default-features
```

`crates/core/freekerf-gcode/benches/analysis.rs` generates a synthetic raster of
about 1,000,000 lines (rows of 500 `G1 X.. S..` moves) and measures parsing plus
analysis (time estimate, ranges).

| Date | Machine | Case | Time |
| --- | --- | --- | --- |
| 2026-10-07 | 4 vCPU Xeon @ 2.8 GHz (cloud container), Rust 1.97 | 1M lines, parallel parsing (rayon) | 0.67 s |
| 2026-10-07 | same | 1M lines, sequential | 0.93 s |

## LaserGRBL (C#)

Not measured yet: it needs LaserGRBL built with Mono (`conformance`'s `make build`)
and a harness timing `GrblFile.LoadFile` on the same file. To do with the next
benchmark update; tracked in [roadmap.md](roadmap.md).
