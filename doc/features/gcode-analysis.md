# G-code analysis

## Purpose

Understand a program before running it: normalized commands, estimated time,
where it moves and burns, which powers it uses.

## Behaviour

- Lines are trimmed, blank lines skipped, letters upper-cased; comments (`;` to the
  end, `(...)`) removed; runs of spaces collapse. Parsing stops silently at a
  malformed number or a repeated letter (`G90 G1 X3`), keeping the words before
  it (DIV-001, as LaserGRBL).
- Modal state: motion (G0-G3...), distance (G90/G91), coordinate system, plane,
  feed mode, spindle (M3/M4/M5), coolant; G92 offsets.
- Time: rapids at `$110`; G1/G2/G3 at `min(F, $110)`; dwell `G4 P`/`S` seconds;
  each command rounded to whole milliseconds (as LaserGRBL). No acceleration.
- Arcs: `I/J` centers; `R` arcs use the chord midpoint (DIV-012, kept).
- Ranges: *moving* (origin included) and *drawing* (where the laser burns: M3 with
  power, or M4 while not rapid; with hardware PWM `S0` does not burn), power range;
  quadrant of the drawing.
- Large files: lines are parsed on all cores, then analyzed in order; 1M lines in
  about 0.7 s ([benchmarks.md](../benchmarks.md)).

## API

- `freekerf analyze FILE [--json] [--max-rate MM_MIN]`. `--json` prints the report
  of `conformance`'s golden files: `commands`, `wire`, `offsets_s`,
  `estimated_time_s`, `drawing_range`, `moving_range`, `spindle_range`, `quadrant`,
  and `stream` (the lines a Grbl device receives for a whole job, from a simulated
  run of the real host against the emulator).
- Actions `job.load`, `job.load_text`, `job.info`.
- Rust: `freekerf_gcode::{Program, Command, MachineState}`,
  `freekerf_service::analysis::analyze`.

## Tests

`crates/core/freekerf-gcode/tests/golden.rs` and
`crates/runtime/freekerf-service/tests/golden_stream.rs` (every conformance golden
file), `tests/properties.rs` (parser never panics, normalization stable), unit tests;
conformance `tests/golden/test_golden.py` with `LASERGRBL_HOST=rust`.

## Divergences

DIV-001, DIV-004, DIV-012, DIV-017, DIV-018 (kept); DIV-052 (letters upper-cased
without a locale) and DIV-053 (an overflowing move counts 0 ms, the analysis goes on)
(fixed).
