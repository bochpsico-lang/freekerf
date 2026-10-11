# 0015. TypeScript types generated from the Rust JSON schemas

- Status: accepted (M4a)
- Date: 2026-10-08

## Context

Commands, events, state and the action catalogue must reach TypeScript
**generated** from Rust and checked in CI; no hand-written type may duplicate a Rust
type. Candidates: `specta` + `tauri-specta`, `ts-rs`, or the JSON schemas the
project already generates with `schemars` (ADR 0004: action parameters; ADR 0005:
settings).

## Decision

- Every type that crosses the boundary derives `schemars::JsonSchema` (parameters
  already did; results, `HostSnapshot`, `Event`, `ConsoleRow`... now do too).
- The registry records the **result** schema of each action
  (`Registry::describe_result`, additive: `register` is unchanged). Actions without
  one are typed `unknown`; a service test lists them.
- Service events have a typed wire shape, `EventMessage` (`{"event": "..."}`),
  used by `ServiceEvent::to_json`.
- `cargo xtask gen-ts` turns the schemas into `ui/web/src/gen/api.ts`: every named
  type, an `Actions` map (`params`, `result`, `tier` per action), `ActionName` and
  `ACTION_TIERS`. The emitter (`tools/xtask/src/ts.rs`) covers the subset of JSON
  Schema that `schemars` produces and fails when two different Rust types share a
  name.
- `api.ts` is part of the generated docs: `cargo xtask check-docs` (CI) fails when
  it is stale.

## Why not `specta` / `tauri-specta` or `ts-rs`

- The project already has one schema system (`schemars`) for actions and settings;
  a second derive on the same types would be two sources of truth.
- The Tauri surface is three commands (`dispatch`, `catalog`, `subscribe`) plus raw
  payloads; typing commands is not where the value is. The value is in the action
  catalogue, which lives in the registry, not in Tauri commands.
- The same schemas serve the script API (`api.d.ts`, M2) and MCP, so the UI, scripts
  and agents see identical types.
- `tauri-specta` 2 is still a release candidate.

## Consequences

- Changing a Rust type that crosses the boundary means running
  `cargo xtask gen-ts` (or `gen-docs`) and committing `api.ts`.
- `freekerf-protocol` depends on `schemars` (a pure library; the core still builds
  for wasm32).
- Bulk data (G-code preview geometry, head positions at 60 Hz, images) does not go
  through these types: it uses raw binary payloads with a layout documented next to
  the encoder (ADR 0006).
