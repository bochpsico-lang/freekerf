# Coverage

Measured with `cargo llvm-cov` (lines). CI fails below the thresholds.

| Scope | Threshold | Last measured |
| --- | --- | --- |
| workspace (minus exclusions) | 90 % | 96.8 % |
| `freekerf-gcode` | 95 % | 99.6 % |
| `freekerf-protocol` | 95 % | 97.7 % |

```bash
cargo llvm-cov --workspace --exclude xtask --exclude freekerf-testkit --summary-only
cargo llvm-cov -p freekerf-gcode --summary-only
cargo llvm-cov -p freekerf-protocol --summary-only
```

## Exclusions

| Excluded | Why |
| --- | --- |
| `tools/xtask` | repository tooling; its layer rules have their own unit tests |
| `crates/testing/freekerf-testkit` | test support, exercised by every test that uses it |

Lines not covered on purpose:

- `freekerf-transport`: OS error paths of PTYs and serial ports that cannot be
  produced in tests without real hardware (permission errors, a device vanishing
  mid-write).
- `freekerf-cli`: the Ctrl-C handler body and the `emulate` command on non-Unix
  systems.
