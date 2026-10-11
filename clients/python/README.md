# freekerf (Python client)

Drive a running [FreeKerf](https://github.com/freekerf/freekerf) from Python: the
same actions as the UI, scripts and AI agents, over the app's local endpoint
(Unix socket or Windows named pipe, with a per-run token). Standard library only,
Python 3.9+.

```python
import freekerf

with freekerf.connect(name="notebook") as app:
    app.document.import_file(path="logo.svg")        # action document.import
    app.document.select_all()
    app.document.translate(dx=10, dy=0)
    print(app.state("session.document.layers"))
    report = app.run_script("app.document.selectAll(); 1", dry_run=True)
```

## Running FreeKerf

The client attaches to a running instance: `freekerf serve --headless` today, the
desktop app later. `connect(home=...)` takes the instance's `--home` directory
(exactly as it was given to `freekerf`); otherwise `$FREEKERF_HOME`, else the
per-user directory. `connect(timeout=10)` waits for an instance that is starting.

## The API

- `app.<namespace>.<action>(params=None, **kwargs)` runs one action of the registry
  (`doc/features/actions.md`); the names are the registry's (`select_all`,
  `open_panel`), and `document.import` is `import_file`. Methods and their help
  come from the app when you connect (`dir(app.document)`,
  `help(app.document.translate)`).
- `app.dispatch(action, params)`: an action by name.
- `app.state(path=None)`: the app state (`session`, `machine`, `console`), or a part
  of it (`"session.view.mode"`).
- `app.run_script(code, dry_run=False)`: JavaScript in the app (one undo step,
  rolled back on error); returns the report (`value`, `log`, `actions`, `error`).
- `app.search_api(query)`, `app.api_dts()`: discover actions.
- `app.subscribe()` and `app.events(timeout)`: change events.
- `app.stop()`: STOP (scripts cancelled, machine soft-reset), on its own connection,
  so it works while another call is running.

Errors raise `freekerf.FreeKerfError` (`.kind`, `.message`); `PermissionDenied` and
`Refused` are subclasses.

## Permissions

The client runs with your own permissions outside the app, but the app still checks
every call: the Python client's tier is `edit`, or `machine` with
`freekerf config set automation.python.allow_machine true`. Actions that can turn the
laser on always need a person to confirm them in the app; a headless instance
refuses them.

## Tests

```bash
cargo build -p freekerf-cli                  # from the repository root
cd clients/python && uv run --with pytest pytest
```

`FREEKERF_BIN` points the tests at another `freekerf` binary.
