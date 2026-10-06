# FreeKerf: monorepo plan

FreeKerf is free software (**GPL-3.0-or-later**) for designing jobs and driving laser machines (Grbl, grblHAL, Smoothie, Marlin…). It is the successor of LaserGRBL (© 2016 Diego Settimi), rewritten in Rust, cross-platform (Linux, Windows, macOS; tablets later), and designed for mouse, keyboard **and** touch.

> **Status: planning.** There is no code yet. This document defines **what to build, in which order, and under which rules** (architecture, tests, documentation). Once code lands, the maintenance guide is [`AGENTS.md`](AGENTS.md) (created in M0) and this file tracks progress in §12 *Where each item lives*.

Related repositories in the [`freekerf`](https://github.com/freekerf) organization:

| Repository | Role for this monorepo |
| --- | --- |
| [`conformance`](https://github.com/freekerf/conformance) | Executable specification of the current LaserGRBL core (Python). Its *protocol* and *golden* layers run against the Rust build with `LASERGRBL_HOST=rust`. |
| [`library`](https://github.com/freekerf/library) | Materials and machine profiles. Consumed by `freekerf-materials` as a git dependency on the `freekerf-library` crate (model + `read_archive`). |
| [`design`](https://github.com/freekerf/design) | UX research, brand, design system (`fk-` tokens), clickable prototype and the UI toolkit analysis (`tech-notes.md`). |
| [`website`](https://github.com/freekerf/website) | Public site and user documentation. |

---

## 1. Principles

1. **Layers are separate and machine-checked.** Business logic knows nothing about I/O, the operating system or the UI (§3).
2. **Sans-IO protocol.** The Grbl state machine takes bytes, lines and time, and returns bytes to send and events. It is tested without threads, real clocks or serial ports.
3. **One service API for every front end.** The CLI, the desktop app and the remote (tablet) client all use the same command/event API (`freekerf-service`).
4. **Observable behaviour is checked by the `conformance` suite.** LaserGRBL bugs do not have to be reproduced (§8), but every difference is documented.
5. **Operator safety over convenience.** STOP is always available, nothing fires the laser without confirmation, and machine states are explicit (§10).
6. **Code, tests and documentation ship together.** A change is not done until its tests and docs are updated in the same PR (§6, §7).

## 2. Repository layout

```
freekerf/
├── AGENTS.md                  maintenance guide for people and agents (M0)
├── CLAUDE.md                  contains only "@AGENTS.md"
├── PLAN.md                    this file
├── Cargo.toml                 virtual workspace (members per layer)
├── deny.toml                  license and source policy (already configured)
├── rust-toolchain.toml        pinned toolchain + MSRV
├── rustfmt.toml               max_width = 110 (same as `library`)
├── crates/
│   ├── core/                  ── CORE: pure logic, no I/O
│   │   ├── freekerf-gcode       G-code parser/model, modal state, arcs, analysis (ranges, quadrant), time estimate
│   │   ├── freekerf-geometry    paths, curves, flattening, booleans, offset, transforms
│   │   ├── freekerf-raster      image operations, 12+ dithers, line-to-line, vectorization preparation
│   │   ├── freekerf-document    project model: objects, colour layers, cut settings, .fkproj format
│   │   ├── freekerf-import      SVG / DXF / image / G-code → document (reads bytes, not files)
│   │   ├── freekerf-planner     document → job: ordering, path optimization, generators (material test, Hershey), G-code
│   │   ├── freekerf-protocol    sans-IO state machine: Grbl/grblHAL, Smoothie, Marlin, VigoWork
│   │   └── freekerf-emulator    model of a Grbl 1.1 device (tests and simulator mode)
│   ├── config/                ── CONFIG: persistent data and schemas
│   │   ├── freekerf-settings    app and machine settings (serde, versioned schema, migrations)
│   │   ├── freekerf-materials   material library access (wraps `library`'s `freekerf-library`)
│   │   └── freekerf-migrate     LaserGRBL import: Settings.bin, .zbn buttons, materials, usage counters
│   ├── native/                ── NATIVE: everything that depends on the OS or hardware
│   │   ├── freekerf-transport   Transport trait + serial, Telnet, WebSocket (ESP8266), emulator; Android USB later
│   │   ├── freekerf-discovery   machine discovery on the network (mDNS/scan)
│   │   └── freekerf-platform    directories, atomic writes, sounds, notifications, OS info
│   ├── runtime/               ── RUNTIME: orchestration
│   │   ├── freekerf-machine     driver: protocol + transport + clock on an I/O thread; job lifecycle
│   │   ├── freekerf-service     application facade: commands → events/snapshots; sessions; API used by every UI
│   │   └── freekerf-remote      WebSocket server for the remote (tablet) client — M6
│   └── testing/               ── TEST SUPPORT (dev-dependencies only, never shipped)
│       └── freekerf-testkit     fixtures, fake devices, simulated Clock, PTY helpers, golden/snapshot helpers
├── ui/                        ── UI (not Rust)
│   ├── web/                     front end (TypeScript) of the desktop/tablet app, grown from `design`'s prototype
│   ├── tokens/                  `fk-` tokens generated from `design/tokens.json`
│   └── i18n/                    Fluent (.ftl) translations shared by Rust and web
├── apps/                      ── APPS: final binaries (thin shells)
│   ├── cli/                     `freekerf` binary (headless)
│   └── desktop/                 Tauri 2 app (`src-tauri` = crate `freekerf-desktop`)
├── tests/                     cross-crate integration tests (workspace member `freekerf-it`)
├── tools/xtask/               automation: check-layers, resx2ftl, sync-tokens, release
└── doc/                       documentation (§7)
```

Crate names are always `freekerf-<name>`; every crate inherits `license = "GPL-3.0-or-later"` from `[workspace.package]`.

## 3. Layer dependency rules

| Layer | May depend on | Must not |
| --- | --- | --- |
| **core** | other core crates and pure libraries | `std::fs`, `std::net`, threads, the system clock, OS/serial/UI crates. Must build for `wasm32-unknown-unknown`. |
| **config** | core | native, runtime, apps (file I/O arrives through a trait / `platform`) |
| **native** | core | config, runtime, apps |
| **runtime** | core, config, native | apps, UI |
| **apps** | runtime (through `freekerf-service`), config, ui | `protocol`/`transport` directly |
| **testing** | anything | being a normal (non-dev) dependency of any crate |
| **ui/web** | only the API exposed by `freekerf-service` (Tauri IPC or remote WebSocket) | its own machine or G-code logic |

Automatic checks:
- `cargo xtask check-layers` reads `cargo metadata` and fails on any edge that breaks the table. It runs in CI.
- `cargo build -p <core crate> --target wasm32-unknown-unknown` in CI proves the core does no I/O.
- `unsafe_code = "forbid"` through workspace lints, except in `freekerf-transport` and `freekerf-platform` where each `unsafe` block has a documented reason.

## 4. Licensing (configured in `deny.toml`)

- Project: **GPL-3.0-or-later**; credit in `NOTICE` ("Derivado do LaserGRBL, Copyright (c) 2016 Diego Settimi"). Porting the C# code directly is allowed.
- `cargo deny check` runs in CI. `deny.toml` is an **allow-list**: GPL-2.0-only, AGPL, SSPL, BUSL, EPL, CDDL, NC/proprietary licenses and unlicensed crates are rejected. Third-party crates come only from crates.io; the `library` git dependency is the only allowed git source (add it to `allow-git` in M2).
- Potrace (GPL-2.0-or-later) is accepted: the combined work is GPL-3.0.
- Avoid `GPL-3.0-only` dependencies (they would pin the project to v3).
- Assets (fonts, icons) follow `design/CREDITS.md`.

## 5. Engineering practices

SOLID, applied to this codebase (same spirit as `library/AGENTS.md`):

- **S — single responsibility.** One crate per concern (§2); inside a crate, one module per concept. Apps only wire the service to a front end: no business logic in `apps/`.
- **O — open/closed.** New firmware = new `impl Firmware` in `freekerf-protocol`; new transport = new `impl Transport`; new importer = new `impl Importer`; new streaming mode = new `impl StreamingStrategy`. Registered in one place; existing implementations are not edited to fit a new case.
- **L — Liskov.** Any `Transport`, `Firmware`, `Importer` or `Clock` can replace another without surprises: no panics on valid input, errors as values.
- **I — interface segregation.** Small traits (one essential method). Do not add "optional" methods to an existing trait; add a new trait.
- **D — dependency inversion.** The protocol, machine and service receive `Clock`, `Transport`, `SettingsStore` and the like by injection; tests use the doubles from `freekerf-testkit`.

Other rules:
- Libraries return errors (`thiserror`); `anyhow` only in apps. No `unwrap`/`expect`/`panic!` on production paths.
- Logging with `tracing`; no `println!` in libraries.
- Deterministic outputs: stable ordering, reproducible G-code for the same input and settings.
- Units in names where ambiguous (`speed_mm_min`, `power_pct`), as in `library`.
- No telemetry (project decision, same as `library`).

## 6. Testing

New code comes with tests. CI fails when a gate below fails.

| Level | Where | What it proves | Tooling |
| --- | --- | --- | --- |
| **Unit** | next to the code (`#[cfg(test)] mod tests`) | one function/type in isolation; protocol driven step by step with a simulated `Clock` | `cargo test`, `freekerf-testkit` |
| **Property** | `tests/` of the crate | the parser never panics; parse → format → parse is stable; the RX buffer never exceeds its limit; geometry invariants | `proptest` |
| **Snapshot / golden** | `tests/` of the crate | G-code and analysis output for fixed inputs; reviewed diffs | `insta` |
| **Crate integration** | `crates/**/tests/*.rs` | the public API of one crate end to end (e.g. `freekerf-machine` streaming a file to `freekerf-emulator` through a PTY) | `cargo test`, PTY helpers |
| **Cross-crate integration** | `tests/` (`freekerf-it`) | real compositions: service + machine + transport + emulator; settings migration; import → plan → stream | `cargo test` |
| **CLI end to end** | `apps/cli/tests/` | the `freekerf` binary: `analyze --json`, `send`, `host --stdio`, `emulate --pty`, exit codes and errors | `assert_cmd`, `predicates` |
| **Conformance** | external repo `conformance` | observable behaviour vs. LaserGRBL (protocol + golden; importers from M2) with `LASERGRBL_HOST=rust` | `make test-rust` in CI (checkout of `conformance`) |
| **Fuzzing** | `fuzz/` | G-code, SVG, DXF and host-protocol parsers (from M2; scheduled CI job) | `cargo-fuzz` |
| **UI end to end** | `ui/web/tests/`, `apps/desktop/tests/` | the 8 flows of `design/ux.md` against the emulator (from M4) | Playwright + Tauri WebDriver |
| **Benchmarks** | `benches/` | load/analyze/plan large files; results in `doc/benchmarks.md` | `criterion` |

Rules:
- **Coverage** with `cargo llvm-cov`: CI fails below **90 % lines for the workspace** (same bar as `library`), and below **95 %** for `freekerf-gcode` and `freekerf-protocol`. Excluded code is listed in `doc/coverage.md` with a reason.
- **No real hardware** in automated tests (§10). Serial tests use PTYs, the emulator or the `conformance` fake Grbl.
- **No sleeps** in tests: waits on conditions with timeouts, or the simulated `Clock`. A flaky test is a bug: fix it or quarantine it with an issue link, never retry blindly.
- **Test data** lives in `crates/<crate>/tests/fixtures/` or in `freekerf-testkit`; large generated inputs are created at test time.

## 7. Documentation

All documentation is in **English**: this file, `README.md`, `AGENTS.md`, everything under `doc/`, rustdoc, commit messages and PR descriptions. Product UI text goes through i18n (`ui/i18n/`), with at least `en` and `pt-BR`.

Structure (created in M0, maintained afterwards):

| File | Content |
| --- | --- |
| `AGENTS.md` | Maintenance guide for people and agents: overview, layout, commands, architecture on one screen, SOLID as applied here, invariant rules (safety, layers, licenses), the living-documentation table, testing rules, conventions. `CLAUDE.md` contains only `@AGENTS.md`. |
| `doc/architecture.md` | Layers, crates, data flow (Mermaid diagrams), extension points table ("I want to… → do…"), key decisions. |
| `doc/features/` | **One page per feature** (e.g. `streaming.md`, `jog.md`, `overrides.md`, `firmware-variants.md`, `gcode-analysis.md`, `svg-import.md`, `material-library.md`, `host-protocol.md`), plus `README.md` as the index. Each page: purpose, user-visible behaviour, API/CLI entry points, configuration, limits, safety notes, tests that cover it, related divergences. |
| `doc/adr/NNNN-title.md` | Architecture decision records (threads vs. tokio, numeric representation, injectable `Clock`, UI toolkit, settings format…). |
| `doc/divergences.md` | Every intentional difference from LaserGRBL (§8). |
| `doc/coverage.md`, `doc/benchmarks.md` | Current numbers and exclusions. |
| `doc/roadmap.md` | What was postponed and open decisions. |
| `CHANGELOG.md` | Keep a Changelog format, from the first release. |

**Living documentation:** every behaviour change updates the docs **in the same PR**. `AGENTS.md` keeps this table up to date:

| Changed… | Update |
| --- | --- |
| a public API (any crate) | rustdoc (`missing_docs` is on), the feature page |
| protocol / streaming / firmware behaviour | `doc/features/<feature>.md`, `doc/divergences.md` if it differs from LaserGRBL |
| the `host --stdio` protocol | `doc/features/host-protocol.md` and `conformance`'s `RustHost` |
| settings format | `doc/features/settings.md`, a migration and its test |
| crates, traits, data flow, layers | `doc/architecture.md`, `AGENTS.md`, `xtask check-layers` |
| a decision | a new ADR |
| something postponed | `doc/roadmap.md` |
| a milestone item | §12 of this file |

Generated documentation (JSON schemas of settings and the host protocol, CLI `--help` captured in docs) is checked in CI: committed == generated.

## 8. Divergences from LaserGRBL

- LaserGRBL bugs listed in `conformance/FINDINGS.md` **do not have to be reproduced**. FreeKerf may behave differently, as long as it is **documented**.
- Every relevant FINDINGS item gets an entry `DIV-NNN` in `doc/divergences.md`: *kept* / *fixed* / *not applicable*, with the reason and the affected test.
- When a `conformance` test fails because of an intentional divergence, mark it **for the Rust host only** (`rust_divergence("DIV-NNN")` marker in `conformance`). Never delete the test or loosen the assertion for the C# host.

## 9. Milestones

Each milestone has objective acceptance criteria, **including tests and docs**. Do not start a milestone until the previous one is green in CI.

### M0 — Bootstrap (1–2 weeks)
- Virtual workspace, `rust-toolchain.toml` (current stable + pinned MSRV), `rustfmt.toml`, workspace lints (`missing_docs`, `unsafe_code = "forbid"`, selected `clippy::pedantic`).
- `freekerf-testkit` skeleton; `tools/xtask` with `check-layers`.
- CI (skeleton in `.github/workflows/ci.yml`): fmt, clippy `-D warnings`, tests with `cargo llvm-cov` and the thresholds of §6, `cargo deny check`, `check-layers`, wasm build of the core, `cargo check` for Windows (`x86_64-pc-windows-gnu`) and macOS (`aarch64-apple-darwin`). Tests run on Linux.
- Docs: `AGENTS.md`, `CLAUDE.md`, `doc/architecture.md`, `doc/features/README.md`, `doc/roadmap.md`, ADRs 0001–0003 (threads + `crossbeam-channel` vs. `tokio`; numeric representation — LaserGRBL uses `decimal`, so `rust_decimal` where wire formatting requires it; injectable `Clock`).

**Acceptance:** CI green with one minimal crate per layer; a test PR that breaks a layer rule fails `check-layers`; docs skeleton in place.

### M1 — Headless machine core (6–10 weeks)
Crates: `gcode`, `protocol`, `emulator`, `transport` (serial + emulator), `machine`, `settings`, `service`, `testkit`, `apps/cli`.

- Functional parity with the "core" tier of `conformance/SCOPE.md`: streaming modes, RX buffer accounting, `ok`/`error:N`/`ALARM`/`<status>`/`[MSG]`, `$$`/`$I`, real-time commands, overrides, jog and continuous jog, job lifecycle, resume, custom code with `[expr]`, firmware detection, laser life counters, emulator.
- `freekerf` CLI:
  - `analyze <file.nc> --json`: the same JSON as `conformance/src/lasergrbl_harness/golden.py`;
  - `send`, `console`;
  - `emulate --pty`: exposes the emulator on a PTY (also useful to test the old LaserGRBL);
  - `host --stdio`: JSON Lines protocol (`doc/features/host-protocol.md`) implementing `conformance`'s `HostAdapter`.
- In `conformance`: a PR adding `RustHost`, the Rust analyzer, the `rust_divergence` marker and a `make test-rust` target that needs neither Mono nor `LASERGRBL_REPO`.
- Large files: streaming load and parallel analysis (`rayon`); benchmark with a synthetic ~1M-line raster.

**Acceptance:**
- `conformance` *protocol* and *golden (G-code)* layers green with `LASERGRBL_HOST=rust`, 3 consecutive runs without flakiness (or the divergence recorded, §8);
- coverage thresholds of §6; property tests for the parser and the buffer accounting; integration tests streaming through a PTY to the emulator; CLI end-to-end tests;
- feature pages for every M1 feature, `doc/divergences.md`, `doc/benchmarks.md` (Rust vs. C#).

### M2 — Importers and planner (6–10 weeks)
Crates: `geometry`, `raster`, `document`, `import`, `planner`, `materials`, `migrate`.

- SVG (colour layers, fill/cut, per-layer settings), DXF (true arcs, splines → arcs within a tolerance), images (adjustments, dithers, line-to-line), vectorization (Potrace and centerline), generators (power × speed grid, shade test) and Hershey fonts.
- `freekerf-materials` on top of `library`'s `freekerf-library` (`read_archive` of a release package).
- `freekerf-migrate`: LaserGRBL Settings.bin (BinaryFormatter, read-only), `.zbn` buttons, materials and counters.
- Versioned `.fkproj` project format with a JSON schema generated from the model (same approach as `library`).

**Acceptance:** `conformance` *golden (importers)* green with `LASERGRBL_HOST=rust`; `platform_dependent` (GDI+) cases compared with a documented tolerance. Fuzz targets for SVG/DXF/G-code running in a scheduled CI job. Integration test import → plan → stream to the emulator. Feature pages for every importer and generator.

### M3 — UI spike (2–3 weeks)
Run the "What the spike must validate" table of `design/tech-notes.md` (canvas with 20,000 paths + an 8000×6000 image, gestures, 60 Hz geometry IPC, serial, remote client, accessibility, packaging) for **Tauri 2** and, for tests 1, 2 and 5, for **Flutter + flutter_rust_bridge**.

**Acceptance:** `doc/adr/000X-ui-toolkit.md` with the decision and the measured numbers; spike code kept in a branch, not in `main`. The current recommendation is Tauri 2; it changes only if WebKitGTK on Linux fails tests 1 or 2.

### M4 — Desktop MVP (10–16 weeks)
- `ui/web` grown from `design/prototypes/app`: Design/Produce modes, canvas (pan, zoom, pinch, selection, handles), colour layers, inspector, machine panel, console, fire confirmation, job recovery, welcome flow and LaserGRBL migration.
- All **MVP** items of the matrix in `design/research.md` §6.
- i18n: `cargo xtask resx2ftl` converts the LaserGRBL `.resx` translations that still apply.
- Automatic density by input type; light, dark and high-contrast themes from the `fk-` tokens.

**Acceptance:** the 8 flows of `design/ux.md` pass as UI end-to-end tests against the emulator in CI on Linux; front-end unit tests (component logic) with coverage reported; user-facing docs drafted for `website`.

### M5 — v1 and distribution (8–12 weeks)
- **v1** items of the matrix: node editing, booleans, offset, array, trace, material library UI, material test, `$$` editor, Smoothie/Marlin in the UI, Telnet/WebSocket + discovery, notifications.
- Packaging:
  - Linux: `.deb`, AppImage, Flatpak;
  - Windows: signed MSI;
  - macOS: signed and notarized DMG;
  - optional, opt-out auto-update.
- CI matrix on real runners (Linux, Windows, macOS): unit + integration tests on all three; UI E2E on Linux and Windows.

**Acceptance:** installers built by the release workflow; smoke test of each installer in CI; `CHANGELOG.md`; user docs published on `website`.

### M6 — Tablet and remote control (later)
`freekerf-remote` (LAN WebSocket, heartbeat for continuous jog, STOP < 50 ms) and the tablet client (Tauri mobile or web served by the core). Android with direct USB host; iPad only as a remote client.

**Acceptance:** latency tests (jog step < 100 ms, STOP < 50 ms) in an integration test over loopback and on a real LAN (manual, documented); the heartbeat stops continuous jog when the client disappears (integration test).

## 10. Safety

- No automated test opens real serial devices (`/dev/ttyUSB*`, `/dev/ttyACM*`, `/dev/ttyS*`, `COMx`). Use PTYs, the emulator or `conformance`'s fake Grbl. Real-machine tests are manual and documented in the PR.
- STOP (soft reset + laser off) bypasses every queue and works in every state; covered by unit and integration tests.
- No command that turns the laser on is sent without an explicit user action. Continuous jog by touch requires a heartbeat (dead-man switch).
- Materials flagged `block` by `library`'s hazard list cannot be used in a job; `warn` shows an alert.

## 11. How to start

1. Read `conformance/README.md`, `SCOPE.md` and `FINDINGS.md`; `design/README.md`, `ux.md` and `tech-notes.md`; and `library/AGENTS.md` as the reference for documentation and testing style.
2. Do M0 and open a PR with the skeleton.
3. M1 in small PRs, one crate at a time: `gcode` → `analyze --json` + golden → `protocol` → `transport`/`machine` → `host --stdio` + `RustHost` → variants, emulator, settings. Each PR brings its tests and its feature page.

## 12. Where each item lives

Updated by every PR that delivers a milestone item (empty until M0).

| Item | Implementation | Tests | Documentation |
| --- | --- | --- | --- |
| — | — | — | — |

## 13. Open questions

- Settings format: TOML (readable, consistent with `library`) or JSON (simpler for the web UI)?
- Auto-update: own channel or package managers only?
