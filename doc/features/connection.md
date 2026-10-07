# Connection

## Purpose

Open a link to the machine, identify its firmware, read its settings, and keep a
fresh view of its state.

## Behaviour

Transports (`freekerf-transport`):

| Transport | Use | `machine.connect` |
| --- | --- | --- |
| serial | USB-serial boards (`/dev/ttyUSB0`, `COM3`) | `{"port": "...", "baud": 115200}` |
| emulator | built-in Grbl 1.1 emulator in the process | `{"emulator": true}` |
| PTY (Unix) | the emulator on a pseudo terminal for other programs | `freekerf emulate --pty` |

Connect sequence (Grbl; the same as LaserGRBL):

1. the port opens; state **Connecting**;
2. soft reset (ctrl-x) unless `machine.reset_on_connect = false`, then a status
   query `?`; the board prints its banner (`Grbl 1.1f ['$' for help]`), which sets
   the firmware version and resets the host state;
3. the first status report moves to **Idle** (or whatever the board reports);
4. 500 ms later `$$` (settings, when Idle or Alarm) and `$I` (unless
   `machine.query_machine_info = false`) are sent. `[OPT:...,128]` (or bigger)
   in `$I` sets the RX buffer size;
5. when both are answered the host is **ready**.

Then `?` is sent every `machine.status_query_ms` (500 ms). A connection that
stays in Connecting for 10 s is closed. A hang-up (cable pulled, PTY closed) is
detected by the reader and closes the link (`UnexpectedDisconnect` if a job ran).

`$$`/`$I` replies are collected until the expected number of rows arrived, 500 ms
after the last one, or 5 s after the `ok`; boards that send `ok` first are
handled. A query without answer fails after 10 s (and stays pending, DIV-028).

## API

- Actions: `machine.list_ports`, `machine.connect`, `machine.disconnect`,
  `state.get` (fields `machine.connected`, `machine.ready`, `machine.status`,
  `firmware_version`).
- Rust: `freekerf_machine::Machine::{connect, disconnect, snapshot, subscribe}`;
  `freekerf_protocol::Host::{open, receive, tick, close}`.

## Configuration

`machine.port`, `machine.baud`, `machine.firmware`, `machine.reset_on_connect`,
`machine.query_machine_info`, `machine.status_query_ms` (see [settings.md](settings.md)).

## Limits

Telnet/WebSocket transports and network discovery are M5. Android USB host is M6.

## Safety

Automated tests never open real serial devices. STOP works while connecting
(DIV-102).

## Tests

`crates/core/freekerf-protocol/tests/protocol.rs` (connect scenarios),
`host_behaviour.rs` (timeouts, banners), `crates/native/freekerf-transport/tests/serial_pty.rs`,
`crates/runtime/freekerf-machine/tests/driver.rs` (hang-up on a PTY), conformance
`tests/protocol` (`make test-rust`).

## Divergences

DIV-028, DIV-030, DIV-102 ([divergences.md](../divergences.md)).
