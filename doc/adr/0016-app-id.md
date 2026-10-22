# 0016. Application id on the GitHub domain

- Status: accepted (M5)
- Date: 2026-10-21

## Context

The desktop app needs a reverse-DNS application id: Tauri's `identifier`, the
Linux desktop entry and AppStream metainfo, the Flatpak app id, the Windows and
macOS bundle ids. The id was `org.freekerf.FreeKerf`, but the project does not own
`freekerf.org`. Flathub requires the id to belong to a domain the publisher
controls and verifies it: for `io.github.<org>.<App>` it checks the GitHub
organisation; for any other domain it checks a token served from that domain.

## Decision

- The application id is `io.github.freekerf.FreeKerf`, everywhere: Tauri's
  `identifier`, `packaging/linux/io.github.freekerf.FreeKerf.{desktop,metainfo.xml}`
  and the Flatpak manifest `packaging/flatpak/io.github.freekerf.FreeKerf.yml`.
- FreeKerf's own folders do not follow it: `freekerf-platform` keeps
  `ProjectDirs("org", "FreeKerf", "freekerf")` (`~/.config/freekerf` and so on), so
  the app id can change without moving users' settings.
- Nothing was released with the old id, so there is no migration.

## Consequences

- Flathub verifies the id through the `freekerf` GitHub organisation; no domain is
  needed.
- Moving to an own domain later (for example `org.freekerf.FreeKerf`) is possible
  but costs a Flathub rename (`end-of-life-rebase` in the old app's manifest) and
  breaks pinned shortcuts and desktop integrations that use the old id.
