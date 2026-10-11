# LaserGRBL migration

`freekerf migrate lasergrbl [DIR] [--apply] [--json]` reads LaserGRBL's data folder
(`%APPDATA%\LaserGRBL` by default on Windows) and shows what it would take; with
`--apply` it writes the result into FreeKerf's settings file, machine cache and
macro folder.
LaserGRBL's files are **only read**. The desktop app's welcome flow uses the same
code through two actions:

| Action | Tier | What |
| --- | --- | --- |
| `migrate.detect {path?}` | read | what the folder holds, by item: `connection` (port, baud rate, firmware, streaming mode, reset on connect), `preferences` (the other settings), `counters`, `buttons`, `materials` (found only) |
| `migrate.run {items, path?}` | edit | imports the chosen items; the service reads its settings again |

The folder defaults to LaserGRBL's on this computer (`$FREEKERF_LASERGRBL_DIR`
overrides it, for tests). User materials are reported but not kept yet (they come
with the material library UI).

## What is taken

| LaserGRBL file | FreeKerf |
| --- | --- |
| `LaserGRBL.Settings.bin` | settings with an equivalent: firmware, baud rate, streaming mode, reset and `$I` on connect, hardware PWM, continuous jog, auto cooling (on/off times), custom header/passes/footer (CRLF → LF), jog step and speed. Every other key is listed as not taken |
| `LaserLifeCounter.bin` | the laser module counters (name, brand, model, power, dates, run time, normalized and non-zero emission time, the ten power classes) into `machine-cache.json`; a counter with the same id is replaced |
| `UserMaterials.psh` | detected; `freekerf_materials::MaterialLibrary::import_lasergrbl` converts it with the library's importer (blocked materials skipped); keeping user materials comes with the material library UI |
| `UsageStats.bin` | not taken: it is LaserGRBL's anonymous usage telemetry, FreeKerf has none (DIV-027) |
| `CustomButtons.bin` | the custom buttons, as scripts in the macro folder (below) |

A value stored with another type than LaserGRBL expects is skipped, as LaserGRBL
itself ignores it (`Settings.GetObject` returns the default).

## Custom buttons

`freekerf migrate lasergrbl-buttons FILE [--apply]` migrates a button export
(`.zbn`, such as LaserGRBL's `StandardButtons.zbn`) the same way.

Every button action becomes a script that runs its G-code with
`app.machine.customCode`, which evaluates the same `[expr]` expressions (`[left]`,
`[width/2]`, `[$30*3/100]`, the jog settings...) when it runs
([custom-code.md](custom-code.md)), so every button migrates:

```js
// Migrated from the LaserGRBL button "Frame" (click).
// `[expr]` parts are evaluated when the code runs, as in LaserGRBL.
app.machine.customCode({ code: "G0 X[left] Y[bottom]\nM3 S[$30*3/100] F1000\n..." });
```

The macro folder (`<data>/macros`) gets one `.js` per action (`frame.js`; two-state
buttons also `air.off.js` for the second click, push buttons `hold.release.js` for
the release), the icon as `.png`, and `buttons.json`: the list of buttons with their
name, tooltip, kind (`button`, `two_state`, `push`), when they are enabled
(`always`, `connected`, `idle`, `run`, `idle_with_program`) and their files
(format version 1, read by the desktop app's buttons and macro editor from M4).
`machine.custom_code` is tier `laser`, so running a migrated button always needs a
person's confirmation; `freekerf run --dry-run --tier laser frame.js` shows what it
would send.

## How the files are read

LaserGRBL saves these files with .NET's `BinaryFormatter` (the laser counters and
buttons behind a short ASCII header written by `Tools.Serializer`; `.zbn` exports are
also deflate-compressed). `freekerf_migrate::nrbf` reads
the [MS-NRBF] format into a tree of values and resolves references; nothing is
instantiated or executed, so a crafted file can only fail to read. Property tests
feed it random and damaged files.

[MS-NRBF]: https://learn.microsoft.com/openspecs/windows_protocols/ms-nrbf/

## Tests

`crates/config/freekerf-migrate/tests/lasergrbl_files.rs` on files written by
LaserGRBL's own code (`tests/fixtures/LaserGrblFiles.cs`, run with Mono against
`LaserGRBL.exe`) and on LaserGRBL's own `StandardButtons.zbn`; `tests/properties.rs`;
`freekerf-service/src/migrate.rs` (report and apply, chosen items);
`apps/cli/tests/cli.rs::migrate_lasergrbl`.
