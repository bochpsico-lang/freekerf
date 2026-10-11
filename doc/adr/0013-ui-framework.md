# 0013. Front-end framework: Svelte 5 + TypeScript + Vite

- Status: accepted (M4a)
- Date: 2026-10-08
- Context: ADR 0006 (Tauri 2). Numbers 0007-0010 are left to M2 (clipping, GDI+
  tolerance, script engine, MCP transport).

## Context

`ui/web` is the web front end of the Tauri app, grown from the HTML/CSS/JS
prototype of `freekerf/design` (`prototypes/app`). The criteria (M4 plan):

1. performance next to a heavy canvas (the canvas must not be re-rendered by the
   framework; UI updates at 60 Hz must be cheap);
2. small runtime (installer budget ~25 MB, start-up on old PCs);
3. TypeScript;
4. testability (component tests without a browser, coverage);
5. how much of the prototype is reused (`index.html` markup, `app.css`, i18n keys).

## Decision

**Svelte 5 (runes) + TypeScript + Vite**, tested with Vitest + Testing Library
(jsdom), linted with ESLint (`eslint-plugin-svelte`, `typescript-eslint`) and
`svelte-check`, formatted with Prettier.

| | Svelte 5 | SolidJS | Lit | plain TS components | React |
| --- | --- | --- | --- | --- | --- |
| canvas next to UI | compiled, fine-grained updates; the canvas is a plain element owned by our renderer | same | same | same | virtual DOM diffing on every state change; needs memo discipline |
| runtime | ~10 kB (compiled) | ~7 kB | ~5 kB | 0 | ~45 kB |
| TypeScript | yes (`lang="ts"`, `svelte-check`) | yes (JSX) | yes | yes | yes |
| tests | Vitest + Testing Library | same | web-test-runner or jsdom with shims | hand-made | very mature |
| prototype reuse | **high**: markup and CSS move almost unchanged into `.svelte` files | JSX rewrite of the markup | templates in tagged literals | high, but we would write a mini framework | JSX rewrite |
| reactivity model | runes (`$state`, `$derived`): close to the prototype's "state object + render" | signals | reactive properties | manual | hooks |

Svelte and Solid are close on performance and size; Svelte wins on reuse (HTML and
CSS stay HTML and CSS, scoped per component) and on readability for contributors
coming from the prototype. Lit and plain TS were rejected because the shell has many
small stateful components (dock, dialogs, inspector) where a compiler saves code.
React does not win any criterion that would justify its runtime.

Measured in this PR: the production bundle of the skeleton is 35 kB of JS (13.7 kB
gzipped) and 7 kB of CSS.

## Consequences

- `ui/web` is a Vite project; `index.html` is the only production entry. `dev.html`
  runs the same UI on the fake service (`src/dev/`) in a browser.
- App state is never owned by components: it comes from the service (snapshots and
  events) into `src/state/`, and changes only through `dispatch` (PLAN §9,
  principle 7; lint in `eslint.config.js`).
- The canvas renderer is framework-independent TypeScript (ADR 0014); Svelte only
  hosts its `<canvas>` element.
- Node is needed to build the app (CI uses the current LTS). The Rust workspace
  still builds and tests without Node; only `freekerf-desktop` release builds embed
  `ui/web/dist`.
