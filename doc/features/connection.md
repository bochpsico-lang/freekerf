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

## Permissions on Linux

Serial devices belong to a group: `dialout` on Debian, Ubuntu and Fedora, `uucp`
on Arch and openSUSE. A user who is not in it cannot open the port, and the app
says so before and when they try:

- `machine.list_ports` returns `{path, access, group}` per port. On Linux `access`
  is `denied` when `access(2)` refuses read and write; `group` is the name of the
  device's group (from its gid). The port is **never opened** to find out: opening
  one toggles DTR, which resets many boards. Other systems always say `ok` and the
  group is not known.
- `machine.connect` on a port the system refuses (`EACCES`) fails with
  `{"kind": "failed", "message": {"reason": "...", "cause": {"code":
  "port_permission_denied", "port": "...", "group": "dialout"}}}`.
- The Machine panel shows, under the port picker, *You don't have permission to use
  <port>. Add yourself to the <group> group, then log out and back in*, with the
  command to copy: `sudo usermod -aG <group> $USER` (`dialout` when the group is not
  known). Being in the group takes effect at the next login.

In a Flatpak the sandbox has `--device=all` (the ports) and `/run/udev`;
`serialport` is built without libudev and lists ports from `/sys/class/tty` (no
ports, instead of its panic, where `/sys` is missing). The kernel still checks the user's real groups, so `access` is right; but
the device's group is outside the sandbox's user namespace and shows as the
overflow gid 65534 (`nogroup`), which is reported as unknown: the warning then
says `dialout`.

## API

- Actions: `machine.list_ports`, `machine.connect`, `machine.disconnect`,
  `state.get` (fields `machine.connected`, `machine.ready`, `machine.status`,
  `firmware_version`).
- Rust: `freekerf_transport::serial::{available_ports, port_access}`,
  `TransportError::PermissionDenied`;
  `freekerf_machine::Machine::{connect, disconnect, snapshot, subscribe}`;
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
`crates/runtime/freekerf-machine/tests/driver.rs` (hang-up on a PTY),
`freekerf-transport` `serial::tests` and `freekerf-service`
`a_denied_port_fails_with_its_cause` (temporary files with mode 000/600 stand for
devices), `ui/web/src/produce/produce.test.ts` (the warning), conformance
`tests/protocol` (`make test-rust`).

## Divergences

DIV-028, DIV-030, DIV-102 ([divergences.md](../divergences.md)).
