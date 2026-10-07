# Job lifecycle

## Purpose

Run a loaded program on the machine: start, pause, stop, passes, recovery.

## Behaviour

- **Run** (`job.run`, tier `laser`): allowed when connected, ready, Idle or Check,
  with a program and an empty queue. Queues: optional `$H` (`homing`), the custom
  header, the program, and when the last line is answered, the footer. With
  `passes > 1` the passes code and the program are queued again after each pass.
- **Feed hold / resume** (`machine.feed_hold`, `machine.resume`): `!` / `~`. A hold
  the host did not request shows as `AutoHold`, a cooling hold as `Cooling`.
- **Safety door** (`machine.safety_door`, tier `safety`): the real-time byte 0x84;
  a Grbl 1.1 board holds as if its door opened (`Door`) and `machine.resume`
  continues. LaserGRBL sends `@` instead (Grbl 0.9; DIV-020).
- **Abort** (`job.abort`, tier `safety`): stops streaming, clears the queue, sends
  `M5`. Lines already in the board's buffer still run.
- **STOP** (`machine.stop`, tier `safety`): soft reset. Flushes every queue, ends
  the job; Grbl turns the laser off and stops at once. Bypasses every queue.
- **Resume** (`job.resume`, tier `laser`): from a position: optional `$H`, optional
  `G92` to restore the work offset of the interrupted job, `G90`, `M5 G0` to the
  computed position with its feed and power, the modal state (`G91 M3`...), then
  the program from that line (the first motion gets its motion mode). `state.get`
  offers a `resume` suggestion after an interrupted job.
- **Issues** (reported in `state.get.issues`): `MachineAlarm` (ALARM during a job),
  `UnexpectedReset` (banner during a job), `UnexpectedDisconnect`,
  `StopResponding` (no status report for 10 query periods and at least 5 s while
  running). User actions (abort, STOP, disconnect) are not issues.
- **Progress**: `machine.job_target`, `job_sent`, `job_executed`, `job_errors`,
  `job_time_ms`, `projected_ms` (from the real speed so far; pauses excluded).
- **Auto cooling** (`machine.auto_cooling`): with Grbl 1.1 in laser mode, hold
  `off_s` every `on_s` of job time.

The job "ends" when the board acknowledged the last line: a Grbl board still
executes up to 15 planned moves after that.

## Safety

Anything that starts a job is tier `laser`. STOP is always allowed. The desktop
app (M4) adds a human fire confirmation; agents never skip it (PLAN §9.5).

## Tests

`protocol.rs` (hold, reset, abort, errors, alarm), `host_behaviour.rs` (passes,
resume, issues, cooling, projection), `crates/runtime/freekerf-service/tests/service.rs`,
`tests/it/stream_over_pty.rs`.

## Divergences

DIV-019, DIV-030, DIV-105.
