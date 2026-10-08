# FreeKerf: maintenance guide

For people and coding agents working on this repository. `CLAUDE.md` only includes
this file. The plan (what to build, milestones) is [`PLAN.md`](PLAN.md); this
guide is how to work on what exists.

## Overview

FreeKerf designs jobs and drives laser machines (Grbl, grblHAL, Smoothie, Marlin,
VigoWork). It is the GPL-3.0-or-later successor of LaserGRBL, in Rust. Done so far:
M0 (workspace, CI, docs) and M1 (headless machine core and the `freekerf` CLI); M4a
(desktop app shell in Tauri 2 + Svelte 5) is in progress.
The behaviour of LaserGRBL's core is specified by
[`freekerf/conformance`](https://github.com/freekerf/conformance); FreeKerf passes
its protocol and golden (G-code) layers (`make test-rust` there).

## Layout

```
crates/core/       pure logic, no I/O, builds for wasm32
  freekerf-gcode       commands, modal state, arcs, analysis, time estimate
  freekerf-protocol    sans-IO host state machine (port of GrblCore) + firmware variants
  freekerf-emulator    sans-IO Grbl 1.1 device (simulator, tests)
  freekerf-session     action registry: names, JSON schemas, permission tiers
crates/config/     persistent data
  freekerf-settings    settings.toml model, migrations, JSON schema
crates/native/     operating system and hardware
  freekerf-transport   Transport trait: serial, emulator, emulator on a PTY
  freekerf-platform    directories, atomic writes
crates/runtime/    orchestration
  freekerf-machine     I/O thread: host + link + clock, priority STOP
  freekerf-service     the facade: every action of every front end
crates/testing/
  freekerf-testkit     simulated clock, host+device bench, fixtures (dev-dependency only)
apps/cli/          the `freekerf` binary (thin shell over the service)
apps/desktop/      the Tauri 2 app (`src-tauri` = crate freekerf-desktop, thin shell over the service)
ui/web/            web front end (Svelte 5 + TypeScript + Vite): src/ipc (Tauri), src/state,
                   src/gen (generated: api.ts, icons.ts), src/dev (fake service, never shipped)
ui/tokens/         fk-tokens.css generated from freekerf/design (commit in design.lock)
ui/i18n/           Fluent translations (<locale>/freekerf.ftl): en is the reference
tests/             cross-crate integration tests (crate freekerf-it)
tools/xtask/       cargo xtask check-layers | gen-docs | gen-ts | check-docs | sync-tokens
doc/               documentation (below)
```

## Commands

```bash
cargo test --workspace                       # all tests (no hardware, no network)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo xtask check-layers                     # layer rules
cargo xtask gen-docs                         # regenerate doc/features/actions.md, doc/schema/*, ui/web/src/gen/api.ts
cargo xtask check-docs                       # CI: generated docs and TypeScript types are committed
cargo xtask sync-tokens                      # tokens, icons, fonts from ../design at ui/tokens/design.lock
cargo llvm-cov --workspace --exclude xtask --exclude freekerf-testkit --exclude freekerf-desktop --summary-only
cargo bench -p freekerf-gcode                # 1M-line analysis (doc/benchmarks.md)
cargo build --target wasm32-unknown-unknown -p freekerf-gcode -p freekerf-protocol -p freekerf-emulator -p freekerf-session
cargo deny check

cargo run -p freekerf-cli -- analyze file.nc --json
cargo run -p freekerf-cli -- send file.nc --emulator
cargo run -p freekerf-cli -- emulate --pty   # prints a /dev/pts path any program can open
```

Desktop app (Linux needs WebKitGTK 4.1: `libwebkit2gtk-4.1-dev`, see
`.github/actions/tauri-deps`; `freekerf-desktop` needs Rust ≥ 1.90):

```bash
cd ui/web && npm ci                          # once
npm run dev                                  # Vite on :5173 (dev.html = UI on the fake service, in a browser)
cargo run -p freekerf-desktop                # the app, loading the Vite dev server
npm run build && cargo run -p freekerf-desktop --features custom-protocol   # embedded front end
npm run lint && npm run check && npm test    # ESLint (architecture rules), svelte-check, Vitest
npm run coverage                             # Vitest + V8 coverage (report in ui/web/coverage)
npm run format                               # Prettier
npm run check:bundle                         # after build: the fake service is not in dist/
npm run licenses                             # npm license allow-list (GPL-3.0 compatible)
```

Conformance (in a checkout of `freekerf/conformance`):
`FREEKERF_BIN=/path/to/target/debug/freekerf make test-rust`.

## Architecture on one screen

```
 front ends:  CLI · host --stdio · desktop (Tauri: dispatch/catalog/subscribe) · (scripts, MCP later)
                   │ dispatch(action, json)          ▲ ServiceEvent
 runtime:     freekerf-service ── Registry<Inner> (tier check on every call)
                   │ Machine::call(|host, now| ...)   ▲ MachineEvent
              freekerf-machine ── I/O thread: select(commands, rx bytes, deadline); STOP channel first
                   │ bytes                             ▲ bytes
 native:      freekerf-transport ── serial (serialport) · emulator thread · PTY
 core:        freekerf-protocol::Host (sans-IO): receive(bytes, now) / tick(now) → Output + Event
              freekerf-gcode (Program, MachineState) · freekerf-emulator (Device) · freekerf-session (Registry)
```

Time is always injected (`now` in milliseconds, `Clock` trait): the protocol is
tested step by step with a simulated clock (`freekerf_testkit::Bench`). Details:
[`doc/architecture.md`](doc/architecture.md).

## SOLID, as applied here

- **S**: one crate per concern; apps only wire the service to a front end.
- **O**: new firmware = new `impl Firmware` (`freekerf-protocol/src/firmware.rs`);
  new transport = new `impl Transport`; new action = one `register` call in
  `freekerf-service`. Existing implementations are not edited for a new case.
- **L**: any `Transport`, `Firmware` or `Clock` can replace another: no panics on
  valid input, errors as values.
- **I**: small traits (`Transport::open`, `LinkReader::read`, `LinkWriter::write_all`,
  `Clock::now_ms`, `SettingsStore::load/save`). Add a trait rather than optional methods.
- **D**: the host takes time as a parameter; the machine takes a `Clock`; the
  service takes stores. Tests use `SimClock`, `Bench`, `MemoryStore`.

## Invariant rules

**Safety** (PLAN §11)
- No automated test opens a real serial device. Use the emulator, a PTY
  (`freekerf_transport::pty::serve_emulator`) or the conformance fake Grbl.
- STOP (`machine.stop`, soft reset) is tier `safety`, always allowed, and goes
  through the machine's priority channel; it also works while connecting.
- Anything that can turn the laser on is tier `laser` (`job.run`, `job.resume`,
  `machine.send`, `machine.custom_code`).
- `automation.*` settings are user-only: actions (and later scripts and agents)
  cannot change them.

**Layers** (checked by `cargo xtask check-layers`)

| Layer | May depend on |
| --- | --- |
| core | core, pure libraries (no serial, nix, libc, directories, channels, tokio) |
| config | core |
| native | core |
| runtime | core, config, native |
| apps | runtime (`freekerf-service`), config |
| ui/web | only the `AppService` contract (`src/service.ts`); Tauri only in `src/ipc/`; `src/dev/` only in tests and `dev.html`; app state read-only outside `src/state/`, changed only by `dispatch` (ESLint `freekerf/no-app-state-mutation`) |
| testing | anything, but only as a dev-dependency of others |

The core builds for `wasm32-unknown-unknown` (CI). `unsafe_code` is forbidden in
the whole workspace (no crate needs it today).

**Licenses**: GPL-3.0-or-later; `deny.toml` is an allow-list (`cargo deny check`);
npm packages pass `ui/web/scripts/check-licenses.mjs` (`npm run licenses`).
Porting LaserGRBL code is allowed (GPLv3, credit in `NOTICE`).

**Errors**: libraries return errors (`thiserror`), `anyhow` only in apps; no
`unwrap`/`expect`/`panic!` outside tests (clippy enforces it).

## Living documentation

A change is done when its tests and docs are updated in the same PR.

| Changed… | Update |
| --- | --- |
| a public API (any crate) | rustdoc (`missing_docs` is on), the feature page |
| protocol / streaming / firmware behaviour | `doc/features/<feature>.md`, `doc/divergences.md` if it differs from LaserGRBL |
| the `host --stdio` protocol | `doc/features/host-protocol.md` and conformance's `RustHost` |
| an action (added, renamed, new parameter, new tier) | `cargo xtask gen-docs` (`doc/features/actions.md`, checked in CI) |
| settings format | `doc/features/settings.md`, a migration in `freekerf-settings` and its test, `cargo xtask gen-docs` |
| crates, traits, data flow, layers | `doc/architecture.md`, this file, `tools/xtask/src/layers.rs` |
| a type crossing to the UI (result, event, settings) | `cargo xtask gen-ts` (`ui/web/src/gen/api.ts`, checked in CI) |
| the design commit (tokens, icons, fonts) | `ui/tokens/design.lock`, `cargo xtask sync-tokens` |
| a decision | a new ADR in `doc/adr/` |
| something postponed | `doc/roadmap.md` |
| a milestone item | §13 of `PLAN.md` |

## Testing rules

- Unit tests next to the code; integration tests in `crates/<crate>/tests/`;
  cross-crate in `tests/it/`; CLI end to end in `apps/cli/tests/`.
- Protocol behaviour: drive `freekerf_testkit::Bench` (host + emulator in memory,
  simulated clock). Never sleep for time to pass: advance the simulated clock.
  Tests on real threads wait on conditions with a timeout.
- Golden files (`crates/core/freekerf-gcode/tests/fixtures/`) come from
  `conformance`; when they change there, copy them again.
- Property tests (`proptest`) for parsers and invariants.
- Coverage gates: 90 % lines for the workspace, 95 % for `freekerf-gcode` and
  `freekerf-protocol` ([`doc/coverage.md`](doc/coverage.md)).
- A flaky test is a bug: fix it, never retry blindly.

## Divergences from LaserGRBL

LaserGRBL bugs listed in `conformance/FINDINGS.md` do not have to be reproduced;
each one has an entry `DIV-NNN` (the number of the finding) in
[`doc/divergences.md`](doc/divergences.md): kept, fixed or not applicable. Code
comments cite the id. When a conformance test fails because of an intentional
divergence, mark it `@pytest.mark.rust_divergence("DIV-NNN")` in `conformance`;
never delete it or loosen it for the C# host.

## Conventions

- Crates are `freekerf-<name>`; all inherit `license = "GPL-3.0-or-later"`.
- Units in names where ambiguous: `speed_mm_min`, `step_mm`, `*_ms`.
- Deterministic outputs: the same input and settings give the same G-code and report.
- Logging with `tracing`; no `println!` in libraries.
- No telemetry.
- Documentation, commit messages and PR descriptions in English; UI text goes
  through i18n: `t('message-id')` with the id in `ui/i18n/en/freekerf.ftl` and
  every other locale (a test checks the ids match).
