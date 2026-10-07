# Settings

## Purpose

User settings in a readable file, with a versioned format.

## Behaviour

- File: `settings.toml` in the user's configuration directory
  (`freekerf config path`), or in `$FREEKERF_HOME` / `--home DIR`.
- Schema: [`doc/schema/settings.schema.json`](../schema/settings.schema.json),
  generated from `freekerf_settings::Settings` (CI checks it is current).
- `version = 1`. A file without `version` (drafts) is migrated (`[machine] streaming`
  → `streaming_mode`); a newer version is refused with a clear error. Unknown keys
  are refused. Missing keys take their defaults.
- What FreeKerf learns about the machine (firmware version, `$$`, laser life
  counters) is kept in `machine-cache.json` in the data directory, not here.

```toml
version = 1

[machine]
firmware = "Grbl"            # Grbl | Smoothie | Marlin | VigoWork
port = "/dev/ttyUSB0"
baud = 115200
streaming_mode = "Buffered"  # Buffered | Synchronous | RepeatOnError
reset_on_connect = true
query_machine_info = true
status_query_ms = 500
support_pwm = true
continuous_jog = false

[machine.auto_cooling]
enabled = false
on_s = 600
off_s = 60

[job]
header = "G90 (use absolute coordinates)"
passes = "..."
footer = "G0 X0 Y0 Z0 (move back to origin)"

[jog]
step_mm = 1.0
speed_mm_min = 1000.0

[automation.mcp]            # user-only (PLAN §9)
enabled = false
allow_machine = false
allow_laser = false
```

## API

- `freekerf config path | get [KEY] | set KEY VALUE | schema` (the user: every key).
- Actions `settings.get {key?}`, `settings.set {key, value}` (tier `edit`); keys under
  `automation` and `version` are refused there: only the user changes them.

## Tests

`freekerf-settings` unit tests, `tests/it/settings_files.rs`,
`service.rs::settings_are_saved_and_automation_is_user_only`, `cli.rs::config_get_set_path_schema`.
