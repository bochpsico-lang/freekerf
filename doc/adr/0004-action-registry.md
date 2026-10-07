# 0004. One action registry for every front end

- Status: accepted (M0)
- Date: 2026-10-07

## Context

PLAN §9: the UI, the CLI, scripts and AI agents (MCP) must be able to do the same
things, with the same permission checks, and new actions must not need changes in
every front end. LaserGRBL wires UI events to core methods directly.

## Decision

- `freekerf-session` provides `Registry<C>`: actions registered with a
  `namespace.snake_case` name, a description, a permission **tier** and typed
  parameters whose JSON schema is generated (`schemars`).
- Tiers, in order: `safety` (STOP, feed hold, abort: always allowed), `read`, `ui`,
  `edit`, `machine`, `laser` (anything that can turn the laser on).
- `Registry::dispatch(ctx, caller, name, json)` checks the caller's maximum tier on
  **every** call.
- `freekerf-service` registers every machine, job, settings and state action;
  `freekerf host --stdio`, the CLI and the tests use only `dispatch`.
- The catalogue `doc/features/actions.md` is generated from the registry and
  checked in CI.

## Consequences

- A new action is one registration; it appears in the catalogue, in `host --stdio`
  and (later) in the script API and MCP without more code.
- Permission grants and human confirmation of `laser` actions (PLAN §9.5) build on
  the tier carried by every action (M2/M4).
