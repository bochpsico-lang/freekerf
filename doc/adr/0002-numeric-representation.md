# 0002. Numeric representation

- Status: accepted (M0)
- Date: 2026-10-07

## Context

LaserGRBL keeps coordinates, feeds and powers in .NET `decimal` and computes
geometry (arcs, distances) in `double`, converting between the two at specific
points (`(decimal)double` keeps 15 significant digits; `TimeSpan.FromMinutes`
rounds to milliseconds). The conformance golden files pin the results to the last
digit: positions, ranges, time offsets.

## Decision

- G-code numbers are `rust_decimal::Decimal` (scale kept: `X1.50` prints `X1.50`).
- Geometry is `f64`, with conversions exactly where LaserGRBL converts
  (`freekerf_gcode::number`: `decimal_from_f64` with 15 significant digits,
  `millis_from` rounding like `TimeSpan`, `round_half_even` like `Math.Round`).
- Positions reported by the board are `f32` (`Point3`), as LaserGRBL's `GPoint`.
- Durations are integer milliseconds (`i64`; negative dwell times exist).

## Consequences

- `freekerf analyze --json` reproduces every conformance golden file exactly.
- Code that needs plain floats converts at its boundary (`f64_from_decimal`).
