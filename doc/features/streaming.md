# Streaming

## Purpose

Send lines to the board as fast as it can take them, never overflowing its serial
RX buffer.

## Behaviour

| Mode (`machine.streaming_mode`) | Rule |
| --- | --- |
| `Buffered` (default) | character counting: a line is sent when `used + length + 1 <= buffer size`; each `ok`/`error` frees the bytes of the oldest pending line |
| `Synchronous` | one line in flight |
| `RepeatOnError` | synchronous; a job line answered `error:N` is sent again, up to 3 retries |

- Buffer size: 127 bytes until the board says more: `[OPT:V,15,N]` in `$I`
  (any N > 127, forced), or the first `Bf:..,N` status field with a known size
  (128, 254, 255, 256, 10240).
- `$N=V` settings writes are synchronous (EEPROM writes stall Grbl).
- Lines are sent without spaces (`G1 X10 F300` → `G1X10F300`); `$` commands are
  sent as written. Comments are removed; a comment-only line is sent as an empty
  line (DIV-017, kept).
- Buffer stuck recovery: while running, if lines are pending, the buffer is not
  empty, nothing moved or answered for 10 s and the board reports an empty buffer
  (`Bf`), the host acknowledges the pending lines itself (LaserGRBL's "missing ok"
  recovery); also available by hand (`Host::unlock_buffer_stuck`).
- Broken `ok` (`machine.accept_broken_ok`, default on): a line that is not a
  response, a status report or a banner but contains `ok` in any case (noise merged
  an `ok` into it, e.g. `<Idle|MPos:0,0,0|FS:0,0ok`) acknowledges the oldest pending
  line, as in LaserGRBL. Never a startup block report (`>G54G20:ok`, which the Grbl
  interface says is not counted) or a well-formed `[...]` message (`[MSG:Look up]`):
  LaserGRBL counts both (DIV-051). Off, only lines starting with `ok` or `error`
  count, as the Grbl interface specifies; turn it off if a board sends other lines
  containing "ok" and jobs end early or overflow the buffer.
- Smoothie and Marlin always stream synchronously.

## API

`job.run`, `machine.send` (one line), `state.get` (`machine.used_buffer`,
`machine.buffer_size`, `machine.pending`, `machine.queued`).

## Tests

`buffer_properties.rs` (property: never more bytes than the RX buffer, every line once and in order),
`protocol.rs::stream_*`, `synchronous_mode_keeps_one_line_in_flight`,
`error_mid_job_is_retried_in_repeat_on_error_mode`,
`host_uses_the_rx_buffer_size_reported_by_the_board`; `host_behaviour.rs::eeprom_writes_go_one_at_a_time_and_update_the_table`,
`buffer_stuck_*`, `ip_messages_alarms_and_broken_oks`, `broken_oks_skip_startup_reports_and_messages`,
`broken_oks_can_be_turned_off`; conformance `tests/protocol`.

## Divergences

DIV-017 (kept), DIV-051 (partly fixed).
