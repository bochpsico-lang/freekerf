# 0001. Threads and channels, not an async runtime

- Status: accepted (M0)
- Date: 2026-10-07

## Context

The machine side of FreeKerf talks to one serial port (or socket) per machine,
with latency requirements in the tens of milliseconds (status polling, STOP) and a
few hundred lines per second at most. LaserGRBL used dedicated TX/RX threads. The
options were an async runtime (`tokio`) or plain threads with channels.

## Decision

Plain OS threads and `crossbeam-channel`:

- the protocol is **sans-IO** (ADR 0003), so the runtime choice touches only
  `freekerf-machine` and the transports;
- `freekerf-machine` runs one I/O thread per machine that `select!`s on commands,
  bytes from a reader thread and the next protocol deadline;
- blocking serial reads (`serialport`) run on the reader thread with a short
  timeout; STOP has a dedicated channel drained first.

## Consequences

- No async coloring in the libraries, simple stack traces, `serialport` used as is.
- Two threads per connected machine plus one for the service events: negligible.
- If a front end (the remote WebSocket server, M6) needs async, it bridges with
  channels at its edge; the core does not change.
