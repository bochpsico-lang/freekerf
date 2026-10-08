# Credits of the UI assets

Everything here is copied from [`freekerf/design`](https://github.com/freekerf/design)
at the commit pinned in [`tokens/design.lock`](tokens/design.lock) by
`cargo xtask sync-tokens`; see `design/CREDITS.md` for the full record.

| Asset | Where | Origin | License |
| --- | --- | --- | --- |
| Inter (WOFF2 subsets) | `web/public/fonts/Inter-*.woff2` | The Inter Project Authors (Rasmus Andersson) | SIL OFL-1.1 (`web/public/fonts/OFL.txt`) |
| JetBrains Mono (WOFF2 subsets) | `web/public/fonts/JetBrainsMono-*.woff2` | The JetBrains Mono Project Authors | SIL OFL-1.1 |
| Space Grotesk Bold (WOFF2 subset) | `web/public/fonts/SpaceGrotesk-Bold.woff2` | The Space Grotesk Project Authors | SIL OFL-1.1 |
| Logo, symbol, favicon, app icons | `web/public/favicon.svg`, `web/public/brand/`, `../apps/desktop/src-tauri/icons/` | original FreeKerf design | GPL-3.0-or-later |
| UI icons (~80) | `web/src/gen/icons.ts` (from `prototypes/app/icons.js`) | original FreeKerf design | GPL-3.0-or-later |
| Design tokens | `tokens/fk-tokens.css` (from `tokens.json`) | original FreeKerf design | GPL-3.0-or-later |

The fonts were subset with fontTools by the `design` repository; no glyph was changed.
