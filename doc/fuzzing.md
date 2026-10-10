# Fuzzing

`fuzz/` holds [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets for
the code that reads files from users. It is its own workspace (nightly only), outside
the main one.

| Target | Input | What runs |
| --- | --- | --- |
| `gcode` | any bytes as a G-code file | parse, analysis (time, ranges), report |
| `svg` | any bytes as an SVG file | import, then plan with line, fill and cut layers |
| `dxf` | any bytes as a DXF file | import, then plan |
| `nrbf` | any bytes as a LaserGRBL data file | the `BinaryFormatter` reader, settings and laser counters |

```bash
rustup toolchain install nightly --profile minimal && cargo install cargo-fuzz
cd fuzz && cargo +nightly fuzz run svg corpus/svg -- -max_total_time=300 -rss_limit_mb=2048
```

Seed the corpora from the test fixtures (see `.github/workflows/fuzz.yml`). The
workflow runs every target for ten minutes every Monday and keeps the failing inputs
as artifacts; a failure there is a bug: reproduce it with
`cargo +nightly fuzz run <target> <artifact>`, fix it and add a regression test next
to the code.

## What it found (M2)

| Input | Problem | Fix |
| --- | --- | --- |
| DXF block inserting itself several times | 2^32 copies (memory) | at most a million drawn entities and two million segments per import (DIV-109) |
| BinaryFormatter null runs, huge array counts, nested arrays | allocations of gigabytes, quadratic copies | runs capped to the slots left, a budget of 262 144 values, records stored once, reservations bounded by the bytes left |
| SVG curves with non-finite or huge control points, or a negative size | flattening to 2^20 pieces per curve (seconds, gigabytes) | such curves are not subdivided; tolerance taken as positive (DIV-109) |
| SVG shapes with coordinates near the `i64` limit | integer overflow in the endpoint index of the filling | saturating arithmetic |
| A filled shape spanning 10^73 mm | an astronomically large hatch grid | fillings needing more than 20 million pattern elements are refused (DIV-109) |
