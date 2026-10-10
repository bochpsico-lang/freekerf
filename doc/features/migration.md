# LaserGRBL migration

`freekerf migrate lasergrbl [DIR] [--apply] [--json]` reads LaserGRBL's data folder
(`%APPDATA%\LaserGRBL` by default on Windows) and shows what it would take; with
`--apply` it writes the result into FreeKerf's settings file and machine cache.
LaserGRBL's files are **only read**. The desktop app's welcome flow (M4) uses the
same function (`freekerf_service::migrate::lasergrbl`).

## What is taken

| LaserGRBL file | FreeKerf |
| --- | --- |
| `LaserGRBL.Settings.bin` | settings with an equivalent: firmware, baud rate, streaming mode, reset and `$I` on connect, hardware PWM, continuous jog, auto cooling (on/off times), custom header/passes/footer (CRLF → LF), jog step and speed. Every other key is listed as not taken |
| `LaserLifeCounter.bin` | the laser module counters (name, brand, model, power, dates, run time, normalized and non-zero emission time, the ten power classes) into `machine-cache.json`; a counter with the same id is replaced |
| `UserMaterials.psh` | detected; `freekerf_materials::MaterialLibrary::import_lasergrbl` converts it with the library's importer (blocked materials skipped); keeping user materials comes with the material library UI |
| `UsageStats.bin` | not taken: it is LaserGRBL's anonymous usage telemetry, FreeKerf has none (DIV-027) |
| `CustomButtons.bin` / `.zbn` | migrated to scripts (Track B) |

A value stored with another type than LaserGRBL expects is skipped, as LaserGRBL
itself ignores it (`Settings.GetObject` returns the default).

## How the files are read

LaserGRBL saves these files with .NET's `BinaryFormatter` (the laser counters behind
a short ASCII header written by `Tools.Serializer`). `freekerf_migrate::nrbf` reads
the [MS-NRBF] format into a tree of values and resolves references; nothing is
instantiated or executed, so a crafted file can only fail to read. Property tests
feed it random and damaged files.

[MS-NRBF]: https://learn.microsoft.com/openspecs/windows_protocols/ms-nrbf/

## Tests

`crates/config/freekerf-migrate/tests/lasergrbl_files.rs` on files written by
LaserGRBL's own code (`tests/fixtures/LaserGrblFiles.cs`, run with Mono against
`LaserGRBL.exe`); `tests/properties.rs`; `freekerf-service/src/migrate.rs`;
`apps/cli/tests/cli.rs::migrate_lasergrbl`.
