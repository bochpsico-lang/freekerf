# Changelog

All notable changes to FreeKerf are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions will follow
[Semantic Versioning](https://semver.org/) from the first release.

## [Unreleased]

### Added

- Workspace in layers (core, config, native, runtime, apps, testing) with
  `cargo xtask check-layers`, a CI that checks formatting, lints, tests with
  coverage thresholds, licenses, layers, generated docs, the wasm build of the
  core, Windows/macOS builds, the MSRV and the conformance suite (milestone M0).
- `freekerf-gcode`: G-code parsing, modal state, arcs, time estimate, ranges and
  quadrant, identical to LaserGRBL on every conformance golden file.
- `freekerf-protocol`: the LaserGRBL host as a sans-IO state machine: buffered,
  synchronous and repeat-on-error streaming with RX buffer accounting, status
  reports, firmware and vendor detection, `$$`/`$I`, settings writes, overrides,
  jog and continuous jog, jobs with passes, abort, STOP and resume, custom code
  with `[expr]`, issue detection, laser life counters; Grbl/grblHAL, Smoothie,
  Marlin and VigoWork.
- `freekerf-emulator`: a Grbl 1.1 device (simulator and test device).
- `freekerf-transport`: serial ports, the emulator in process and on a PTY.
- `freekerf-settings`: `settings.toml` with a JSON schema and migrations.
- `freekerf-session` and `freekerf-service`: the action registry with permission
  tiers; every front end dispatches actions.
- `freekerf-machine`: the I/O-thread driver with a priority STOP channel.
- `freekerf-geometry`: paths of lines, arcs and Bézier curves, Mono-compatible
  affine transforms, LaserGRBL's Bézier flattening and arc fitting, path ordering,
  polygon booleans and offsets (a port of Clipper 6.4.2) and the hatch filling
  patterns, identical to LaserGRBL's.
- `freekerf-document`: the project model (objects on colour layers with cut and
  raster settings) and the versioned `.fkproj` file with a generated JSON schema.
- `freekerf-planner`: documents to G-code the way LaserGRBL writes it (layer
  order, hatch fillings, pen and feed handling), the cutting, power/speed and
  shake test generators and Hershey text; .NET-compatible number formatting in
  `freekerf-gcode`.
- `freekerf-import`: SVG (colour layers, all path commands, shapes, transforms,
  units) and ASCII DXF (lines, true arcs, splines fitted with arcs, blocks,
  colours, units) import, identical to LaserGRBL except for documented fixes.
- `freekerf import-case`: runs `conformance` importer golden cases.
- `freekerf` CLI: `analyze`, `send`, `console`, `emulate --pty`, `host --stdio`,
  `config`, `actions`, `run`.
- Action `machine.safety_door` (tier `safety`): the Grbl 1.1 door command (0x84).
- Documentation: `AGENTS.md`, architecture, ADRs 0001-0005, feature pages,
  divergences from LaserGRBL, coverage, benchmarks, roadmap.

### Fixed

- Custom code: `[jogstep]` and `[jogspeed]` are evaluated (the jog settings), as in
  LaserGRBL; they were left unevaluated.
