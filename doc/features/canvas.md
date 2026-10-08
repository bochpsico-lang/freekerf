# Canvas

The canvas shows the machine table: work area with grid and rulers, the G-code
preview (tool path, burned part, job frame), the head, and (with M2) the design.
Renderer decision: [ADR 0012](../adr/0012-canvas-renderer.md). Code:
`ui/web/src/canvas/`.

## Behaviour

| Action | Mouse / trackpad | Touch (best effort, ADR 0006) |
| --- | --- | --- |
| Pan | middle button drag, Space + drag, Shift + wheel (horizontal), two-finger scroll; in **Produce** also a left drag | one finger on empty space, or two fingers |
| Zoom | wheel at the cursor (×1.2 per notch), Ctrl + wheel / trackpad pinch | pinch (centre between the fingers) |
| Select (Design) | click; Shift/Ctrl + click adds | tap |
| Area selection (Design) | drag on empty space: left→right selects what is inside (solid), right→left what it touches (dashed) | — |
| Context menu | right click | long press (500 ms) |

- A second finger cancels the current drag and starts a pinch.
- A drag starts after 3 px with a mouse or pen, 8 px with a finger.
- Losing the pointer (capture lost, window blurred) drops the gesture.
- Pan and zoom only change the view; they never change the document.
- Rulers show mm (or inches) with 1-2-5 steps; the grid is drawn on the work area
  only, with minor and major lines.

## G-code preview

When a program is loaded, the window asks the app for its tool path (Tauri command
`preview`, raw bytes): every motion as straight segments (arcs flattened within
0.01 mm), with the power and the index of the command. The canvas draws:

- moves with the laser off, faint (can be hidden);
- burning moves in the accent colour, alpha by power (S / highest S);
- the commands already executed in the "burn" colour;
- the job frame (bounds of the tool path), dashed.

Format "FKTP" v1: `crates/runtime/freekerf-service/src/preview.rs` (encoder, the
reference) and `ui/web/src/canvas/toolpath.ts` (decoder). Both are tested against
the same file, `ui/web/tests/fixtures/toolpath-v1.bin`.

## Head

`subscribe_head` streams 24-byte frames on a raw Tauri channel, at most 60 per
second and only when something changed: work X/Y, power, commands executed and in
the job, running and connected flags. Between status reports from the board the
position does not change (the board reports a few times per second); interpolation
along the tool path is listed in `doc/roadmap.md`.

## Performance

The scene is cached in a texture bigger than the view, so panning is a single
textured quad until the view leaves it; segments are bucketed in a 16×16 grid and
only visible cells are drawn. The check of PLAN M4a runs inside the app:
`freekerf-desktop --bench-canvas` (numbers and method in
[`doc/benchmarks.md`](../benchmarks.md)).

## Limits

- Lines are 1 device pixel wide.
- WebGL2 is required; without it the canvas shows an error (the rest of the app
  works).
- The work area is 400×400 mm until the machine profile is wired (PR 5).

## Tests

`ui/web/src/canvas/*.test.ts`: camera maths, gestures (click, marquee window and
crossing, pan in Produce / middle button / Space, wheel zoom at the cursor,
pinch, long press, lost pointer), the tool path decoder (with the Rust fixture),
spatial buckets, synthetic data, benchmark options. Rust: `preview.rs` (lines, arcs
within tolerance, binary layout, head frames, shared fixture) and the desktop
bridge (`preview_and_head`, head frames only on change).
