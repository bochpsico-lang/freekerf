# 0003. Injected time and a sans-IO protocol

- Status: accepted (M0)
- Date: 2026-10-07

## Context

LaserGRBL's core reads the clock and the serial port from inside its logic
(timers, `Thread.Sleep`, busy waits). Its behaviour depends on timing (500 ms
before reading `$$`, 10 s timeouts, hang detection) and its tests need real
threads and waits.

## Decision

- `freekerf_protocol::Host` takes the current time (`now`, ms) as a parameter of
  every call, never reads a clock, and owns no I/O: bytes in (`receive`), bytes out
  (`poll_output`), events out (`poll_event`), deadline out (`next_deadline`).
- The `Clock` trait (`now_ms`) is injected where real time is needed:
  `freekerf-machine` uses `SystemClock`; tests use `freekerf_testkit::SimClock` or
  drive the host with explicit times (`Bench`).
- The emulator (`freekerf-emulator`) follows the same model.

## Consequences

- Timeouts and periodic queries are tested deterministically, without sleeps.
- The conformance protocol scenarios run in memory (`crates/core/freekerf-protocol/tests/protocol.rs`)
  and also against real PTYs through `freekerf host --stdio`.
- The core builds for `wasm32-unknown-unknown` (no `std::time::Instant`).
