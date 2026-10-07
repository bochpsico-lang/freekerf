# 0005. Settings in TOML with a generated schema

- Status: accepted (M1; decision recorded in PLAN §14)
- Date: 2026-10-07

## Decision

- `settings.toml` (user-editable, readable in diffs), modelled by
  `freekerf_settings::Settings` with `version = 1`.
- Older versions are migrated when loaded; newer versions are refused.
- The JSON schema is generated from the model into `doc/schema/settings.schema.json`
  (checked in CI).
- What the app learns about the last machine (firmware version, `$$`, laser life
  counters) is not a user setting: it goes to `machine-cache.json` next to the data.
- `automation.*` (MCP) can only be changed by the user (`freekerf config set`, the
  app settings page), never by actions, scripts or agents.
