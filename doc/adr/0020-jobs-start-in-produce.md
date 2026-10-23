# 0020. Jobs start only in the Produce mode, and an out-of-date job only when accepted

- Status: accepted (M5)
- Date: 2026-10-22

## Context

The Design mode had the machine and console panels in its dock, and F5 started
the loaded job from it. A job could be fired without its preview ever being
shown, and without its G-code having been made from the current design: the job
banner said the design changed only in Produce, and the fire confirmation did not
know it. While a job ran, the person could go back to the Design mode.

## Decision

- **Machine controls live in Produce, design ones in Design.** The machine and
  console panels are Produce tabs only, the layers and the inspector Design tabs
  only (the Produce canvas shows the job; a layer changed there would put it out
  of date). Opening a panel switches to its mode (in the view state, so scripts
  and agents get the same rule). F5 and F7 in Design switch to Produce and start
  nothing.
- **A laser request is confirmed in Produce.** `fire.request` switches the view to
  Produce before opening the confirmation.
- **A running job keeps Produce.** The service sets `view.produce_locked` from the
  machine's events while `in_program` (running or paused); `view.set_mode design`
  is refused then.
- **Out-of-date jobs need an explicit acceptance.** Every loaded job records the
  document revision when it was planned or loaded. A `job.run` whose job is older
  than the design (with objects) carries `summary.stale` (`design` or `file`); the
  dialog offers to update it from the design or asks to tick that the old job is
  burned as it is, and `Service::confirm_fire` refuses it without `accept_stale`.
  The check is in the service, not only in the window.
- `job.resume` is not checked: it continues the cut job.

## Consequences

- `ProgramInfo.document_revision` replaces `FromDocument.document_revision`;
  `Service::confirm_fire` takes `accept_stale`; the Tauri command takes
  `acceptStale` (default false).
- The default dock panel is the inspector (`object`); the narrow layout sets the
  view to Produce.
- A G-code file loaded while the design is empty is never out of date; editing the
  design after loading a file makes it so, since the person likely means the design.
