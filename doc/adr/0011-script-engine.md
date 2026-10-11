# 0011. Script engine: QuickJS through rquickjs

- Status: accepted (M2)
- Date: 2026-10-10

## Context

User macros, `freekerf run <script.js>` and the MCP `run_script` tool need one
embedded scripting language (PLAN §9.2) that can be sandboxed by construction,
limited (time, memory, instructions), cancelled by STOP, and shipped on every
platform, tablets included. CPython is ruled out (it cannot be sandboxed, it is
heavy to ship); Python users get the external client. The candidates were:

1. **QuickJS** through `rquickjs` (MIT): a small C engine, ES2023, with memory and
   stack limits and an interrupt handler built in. Compiled from source by `cc`; the
   `unsafe` stays inside `rquickjs`.
2. **Boa** (MIT/Unlicense): pure Rust, slower, with less complete limits.

## Decision

QuickJS through `rquickjs` 0.14 (the maintainer's choice, 2026-10-10), with its
optional features off (no module loader, no `std`/`os` modules).

- The script sees the ECMAScript built-ins, `console.log` and a frozen `app` object
  generated from the action registry (`app.document.selectAll()` dispatches
  `document.select_all`); the native dispatcher is captured in a closure and deleted
  from the global object before user code runs.
- Limits per run (`freekerf_script::Limits`): 30 s wall clock, 64 MiB heap, 512 KiB
  stack, 2 million interrupt checks; the interrupt handler also checks the cancel
  flag that STOP sets.
- One `Runtime` per run: no state survives between scripts.

Measured with `cargo run --release -p freekerf-service --example script_bench`
(Linux, x86-64, release build):

| Case | Result |
| --- | --- |
| startup: engine, context, generated `app` object (57 actions), run `1` | 0.44 ms |
| build a 10,000-rectangle SVG in JavaScript and import it | 36 ms |
| translate 10,000 objects, one action each, in one transaction | 85 ms (8.5 µs per action) |
| translate the same 10,000 objects in one action | 57 ms |
| peak resident memory of the whole process (document, undo history, engine) | 41 MiB |

Boa was not measured: the choice was made before the benchmark, which shows QuickJS
is fast enough (dispatch dominates; the engine is not the bottleneck).

## Consequences

- C code in the build (QuickJS through `cc`), no `unsafe` in FreeKerf's crates.
  `freekerf-script` is a runtime crate; the core keeps building for `wasm32`. A web
  front end would need another engine or the browser's own JavaScript.
- The API is generated, so new actions appear in scripts, in `api.d.ts`
  (`doc/scripting/api.d.ts`, checked in CI) and in MCP without engine changes.
- Sandbox escape tests (`freekerf-script/tests/sandbox.rs`) cover the global object,
  the hidden dispatcher, tampering with `app` and the built-ins, `Function`, modules,
  timers, the limits and cancellation.
