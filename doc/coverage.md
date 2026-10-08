# Coverage

Measured with `cargo llvm-cov` (lines). CI fails below the thresholds.

| Scope | Threshold | Last measured |
| --- | --- | --- |
| workspace (minus exclusions) | 90 % | 96.8 % |
| `freekerf-gcode` | 95 % | 99.6 % |
| `freekerf-protocol` | 95 % | 97.7 % |

```bash
cargo llvm-cov --workspace --exclude xtask --exclude freekerf-testkit --exclude freekerf-desktop --summary-only
cargo llvm-cov -p freekerf-gcode --summary-only
cargo llvm-cov -p freekerf-protocol --summary-only
```

## Exclusions

| Excluded | Why |
| --- | --- |
| `tools/xtask` | repository tooling; its layer rules have their own unit tests |
| `crates/testing/freekerf-testkit` | test support, exercised by every test that uses it |
| `apps/desktop/src-tauri` (`freekerf-desktop`) | window and Tauri wiring that needs a display; its logic (`bridge.rs`) has unit tests in the `desktop` CI job, and the app is exercised end to end by the UI tests |

## Front end (`ui/web`)

Measured with Vitest (`npm run coverage`, V8). The report is uploaded by the CI
job `ui` (artifact `ui-coverage`). Generated code (`src/gen/`) and the entry
points (`src/main.ts`, `src/dev/main.ts`) are excluded.

Lines not covered on purpose:

- `freekerf-transport`: OS error paths of PTYs and serial ports that cannot be
  produced in tests without real hardware (permission errors, a device vanishing
  mid-write).
- `freekerf-cli`: the Ctrl-C handler body and the `emulate` command on non-Unix
  systems.
