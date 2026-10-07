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
| 2026-10-07 | Intel Core i5-1235U (10 cores, 12 threads), Linux, Rust 1.97.1 | 1M lines, parallel parsing (rayon) | 0.36 s |
| 2026-10-07 | same | 1M lines, sequential | 0.50 s |

## LaserGRBL (C#)

`conformance`'s `scripts/large_file.py` writes the same raster, times LaserGRBL's
`GrblFile.LoadFile` (load and analysis, in process; LaserGRBL bf15096 built with Mono)
and `freekerf analyze --json`, and checks that every field of the analysis is equal
(`stream` included). Details: `conformance/doc/large-files.md`.

| Date | Machine | Case | LaserGRBL | FreeKerf |
| --- | --- | --- | --- | --- |
| 2026-10-07 | i5-1235U, Mono 6.8 / Rust 1.97.1 release | 100k lines: load + analysis / `analyze --json` | 0.41 s | 0.24 s |
| 2026-10-07 | same | 1M lines: load + analysis / `analyze --json` | 3.96 s | 2.59 s |
| 2026-10-07 | same | 1M lines: load + analysis / parse + analysis (rayon) | 3.96 s | 0.36 s |

`analyze --json` also simulates the whole job run for its `stream` field, which is most
of its time. Mono on Linux is not .NET Framework on Windows: take the C# column as an
order of magnitude. Both give identical results on 100,003 lines.
