# 0017. Failures with a cause front ends recognise

- Status: accepted (M5)
- Date: 2026-10-21

## Context

`ActionError::Failed(String)` carried only text. The window needs to recognise
some failures to do better than a toast: a serial port the user may not open
(Linux `dialout`) should show what to do and the command to copy. Matching the
text is fragile (translations, wording changes), and a new `ActionError` variant
per case would grow the enum every front end, script and agent sees.

## Decision

- `ActionError::Failed` carries a `Failure { reason, cause }`: `reason` is the
  text (what `Display` shows, as before) and `cause` an optional
  `FailureCause`, a typed enum tagged by `code`
  (`{"code": "port_permission_denied", "port": ..., "group": ...}`).
- `ActionError::failed(reason)` builds a failure without a cause, which is every
  failure but the ones a front end acts on.
- A new recognisable failure is a new `FailureCause` variant (open/closed: the
  other variants and `ActionError` do not change); `cargo xtask gen-ts` gives the
  UI a union it can `switch` on.

## Consequences

- On the wire, `message` of a `failed` error is an object (`{reason, cause?}`)
  instead of a string; the UI's `describeError` reads `reason`.
- The `host --stdio` protocol, the local endpoint and scripts send the error's
  text (`Display`), so they do not change.
