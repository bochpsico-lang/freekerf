# Scripting

FreeKerf runs JavaScript scripts against the app: user macros, `freekerf run
<script.js>`, and (with the MCP server) agents. A script reaches the app only through
the **action registry**, with the caller's tier checked on every call (PLAN §9.2,
[ADR 0011](../adr/0011-script-engine.md)).

```js
// make.js: import a generated drawing, move it, save the project
const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm">'
  + '<rect x="1" y="1" width="5" height="5" stroke="red" fill="none"/></svg>';
const { objects } = app.document.importData({ format: "svg", text: svg });
app.document.translate({ dx: 10, dy: 0, ids: objects });
app.document.save({ path: "out.fkproj" });
console.log(`moved ${objects.length} objects`);
app.session.snapshot().document.objects.length   // the script's value
```

## The API

- `app.<namespace>.<action>(params)` dispatches the action `<namespace>.<action>`
  with `params` (snake_case action names become camelCase:
  `document.select_all` → `app.document.selectAll()`). It returns the action's JSON
  result, or throws an `Error` with the action's message.
- `console.log(...)` writes to the run's log.
- Nothing else: no file system, network, processes, modules, timers or raw serial
  access. Files go through actions (`document.import`, `document.save`), which apply
  their own rules.
- Declarations: [`doc/scripting/api.d.ts`](../scripting/api.d.ts), generated from
  the registry by `cargo xtask gen-docs` and checked in CI. Parameters and results:
  [actions.md](actions.md).

## Runs

- **One undo step.** A run's document, selection and view changes form one undo
  step labelled `script <name>`. One live script runs at a time.
- **Rollback.** On an uncaught error (including a failing action the script does not
  catch, a limit or STOP) every document, selection and view change of the run is
  undone. Machine actions cannot be undone and are never retried.
- **Dry run.** The script runs against a copy of the session: document and view
  actions change the copy, `read` actions outside the session read the real state,
  and every other action (machine, laser, files, settings, jobs) is simulated: it
  returns `{"simulated": true}` and is listed in the report. Nothing is applied and
  no change event is published.
- **Limits** (`freekerf_script::Limits`): 30 s wall clock, 64 MiB heap, 512 KiB
  stack, an instruction budget.
- **Report** (`freekerf_service::script::ScriptReport`, the JSON `freekerf run`
  prints): `value` (the completion value as JSON), `log`, `actions` (every dispatched
  action with its parameters and error), `error`, `elapsed_ms`, `dry_run`,
  `simulated`, `rolled_back`.

## Safety

- **Tiers.** The caller's tier limits every call (`permission denied: ... needs
  tier ...`), in dry runs too. `automation.*` settings stay user-only.
- **Laser.** An action of tier `laser` runs only when a person confirms it
  (`ScriptRequest::confirm_laser`, given by the desktop app from M4a). Without a
  confirmation it is refused, so headless runs, the command line and agents cannot
  turn the laser on.
- **STOP** (`machine.stop`, `Service::stop`, Ctrl-C in the CLI) cancels every
  running script before it stops the machine.

## Command line

```bash
freekerf run make.js
```

`freekerf run <file.js> [--dry-run] [--tier read|ui|edit|machine] [--timeout SECONDS]`
prints the report and exits with 1 when the script failed. The default tier is
`machine`; `laser` is accepted only with `--dry-run` (laser actions are then
simulated, never run). A file name ending in `.js` selects a script;
anything else is an action name (`freekerf run state.get`).

## Tests

`freekerf-script/tests/sandbox.rs` (escape attempts, limits, cancel),
`freekerf-script/src/{lib,dts}.rs`, `freekerf-service/tests/scripts.rs` (undo step,
rollback, dry run, laser refusal, tiers, STOP, one script at a time),
`apps/cli/tests/cli.rs::run_a_script`. Benchmark:
`freekerf-service/examples/script_bench.rs`.
