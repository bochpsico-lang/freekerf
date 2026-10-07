# Host protocol (`freekerf host --stdio`)

## Purpose

Drive FreeKerf from another process: the conformance suite (`RustHost`), scripts
and tools. Every request is an action of the registry ([actions.md](actions.md)).

## Protocol

JSON Lines on stdin/stdout. One request per line, one response per line, in order.

Request:

```json
{"id": 1, "action": "machine.connect", "params": {"port": "/dev/pts/3", "baud": 115200}}
```

`id` is echoed (any JSON value); `params` may be omitted for actions without
parameters.

Responses:

```json
{"id": 1, "ok": true, "result": null}
{"id": 2, "ok": false, "error": {"kind": "refused", "message": "not allowed now: the machine is not idle"}}
```

| `error.kind` | Meaning |
| --- | --- |
| `invalid_request` | the line is not JSON or has no `action` |
| `unknown` | no such action |
| `invalid_params` | parameters do not match the schema |
| `permission_denied` | tier too high for the caller (not for `host --stdio`, which has every tier) |
| `refused` | not possible in the current state (not connected, not idle, busy) |
| `failed` | the operation failed (port cannot open, the board rejected a setting) |

Settings live in memory unless `--persistent` is given, so each process starts
from the defaults. Closing stdin disconnects the machine and exits.

## Mapping of conformance's `HostAdapter`

| HostAdapter | Action |
| --- | --- |
| `connect(port, baud)` / `disconnect()` | `machine.connect` / `machine.disconnect` |
| `set_option(name, value)` | `settings.set` (`machine.streaming_mode`, `machine.reset_on_connect`, `machine.query_machine_info`, `machine.continuous_jog`, `machine.support_pwm`, `machine.firmware`, `machine.auto_cooling.*`, `job.header`, `job.footer`, `job.passes`, `jog.step_mm`, `jog.speed_mm_min`) |
| `load_gcode(path)` / `run_job(homing, passes)` / `resume_job(position, homing, set_wco)` / `abort_job()` | `job.load` / `job.run` / `job.resume` / `job.abort` |
| `send_command(line)` | `machine.send` |
| `feed_hold()` / `resume()` / `soft_reset()` | `machine.feed_hold` / `machine.resume` / `machine.stop` |
| `unlock()` / `home()` / `jog(...)` | `machine.unlock` / `machine.home` / `machine.jog` |
| `set_override_targets(...)` | `machine.overrides` |
| `write_settings(dict)` / `refresh_settings()` | `machine.write_settings` / `machine.read_settings` |
| `custom_code(code)` / `set_zero()` | `machine.custom_code` / `machine.set_zero` |
| `jog_to(x, y, feed)` / `jog_abort()` | `machine.jog_to` / `machine.jog_stop` |
| `status`, `connected`, `ready`, `job_running`, `issues`, `firmware_version`, `job_errors`, `firmware_vendor`, `machine_position`, `work_offset`, `overrides`, `buffer_size`, `detected_ip`, `last_issue` | `state.get` (`machine.status`, `machine.connected`, `machine.ready`, `machine.in_program`, `issues`, `firmware_version`, `machine.job_errors`, `machine.version.vendor_info`, `machine.machine_position`, `machine.work_offset`, `machine.overrides`, `machine.buffer_size`, `machine.detected_ip`, `machine.last_issue`) |

`RustHost` treats a `refused` error (and a "not supported" one) of `machine.feed_hold`,
`machine.resume`, `machine.unlock`, `machine.home`, `machine.jog`, `machine.jog_to`,
`machine.jog_stop`, `machine.set_zero`, `machine.safety_door`, `job.run`, `job.resume`
and `job.abort` as "ignored", which is what LaserGRBL does silently (DIV-103).

## Tests

`apps/cli/src/host.rs` unit test, `apps/cli/tests/cli.rs::host_stdio_drives_the_emulator_on_a_pty`,
conformance `make test-rust` (protocol tests over real PTYs and the golden G-code cases),
conformance `make test-diff` (both hosts on generated programs and scenarios).

A change to this protocol updates this page and `RustHost` in conformance.
