# 0007. Polygon clipping: a port of Clipper 6.4.2

- Status: accepted (M2)
- Date: 2026-10-07

## Context

`freekerf-geometry` needs polygon booleans (union, difference, intersection, xor)
and offsets for the design tools (M5: booleans, offset), and the importers need
hatch fillings (PLAN §10 M2). LaserGRBL fills SVG/DXF shapes and vectorized images
by clipping a grid of open lines against the shapes with **Clipper 6.4.2** (Angus
Johnson, C#, embedded in `LaserGRBL/Clipper/Clipper.cs`), then orders the lines
nearest-neighbour. The `conformance` goldens of every fill (`svg_fill_*`,
`dxf_fill_layers`, `potrace_vector_fill*`, `ip_vectorize_vector_fill`) pin the exact
vertices and their order.

Candidates on crates.io:

| Crate | License | Notes |
| --- | --- | --- |
| `clipper2` 0.6 | MIT/Apache | Clipper2 (a rewrite): different output ordering and vertex placement than 6.4.2; open-path clipping semantics changed |
| `i_overlay` 9 | MIT | fast, robust; its own algorithm and output order |
| `geo` 0.33 (`BooleanOps`) | MIT/Apache | uses `i_overlay`; heavy dependency tree for what we need |
| a port of Clipper 6.4.2 | Boost-1.0 | same algorithm and same output as LaserGRBL |

None of the crates reproduces LaserGRBL's fillings: the lines would be split,
ordered and sometimes placed differently (a vertex differs by one grid unit), so
every fill golden would become a divergence, and users comparing a job with
LaserGRBL would see different G-code for no gain.

## Decision

- Port Clipper 6.4.2 (C# version, `use_lines` on, 64-bit integers) to Rust in
  `freekerf_geometry::clipper`: `Clipper` (all clip and fill types, open paths,
  `PolyTree`, `StrictlySimple`, `ReverseSolution`) and `ClipperOffset`. Objects
  become arena indices; the algorithm, every rounding and every ordering are kept.
- The intersection list is sorted with a port of .NET's unstable introspective sort
  (`List<T>.Sort`), because the order of equal-height intersections changes the
  output paths.
- `freekerf_geometry::boolean` is the `f64` API for the rest of FreeKerf (scaled to
  an integer grid, non-zero rule); `freekerf_geometry::hatch` ports
  `PotraceClipper` (grid patterns, inset filling) on top of it.
- License: Boost Software License 1.0, which is in `deny.toml`'s allow-list and
  GPL-compatible; the copyright and license notice are kept in `NOTICE` and in the
  module documentation. No new third-party dependency.
- Exactness is tested by a differential fixture: seeded random inputs run through
  LaserGRBL's own `Clipper.cs` under Mono (`tests/fixtures/ClipperCases.cs`); the
  port must return the same paths, vertices and order for all 440 cases.

## Consequences

- No extra dependency, wasm-friendly (pure Rust, no `unsafe`), and fillings that
  match LaserGRBL vertex for vertex.
- Clipper 6.4.2 is unmaintained upstream (superseded by Clipper2); bugs found in it
  are ours to fix, and fixing one that changes fill output is a documented
  divergence (`doc/divergences.md`).
- If performance or robustness of 6.4.2 becomes a problem for the design tools
  (M5), `i_overlay` can be adopted behind `freekerf_geometry::boolean` without
  touching the importers, which keep the port for LaserGRBL-compatible fillings.
