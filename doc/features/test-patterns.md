# Test patterns and Hershey text

Jobs generated without a drawing, to find the right power and speed for a material
or to check the mechanics. Port of LaserGRBL's `Generator` dialogs
(`freekerf_planner::generators`); labels use the Hershey single-stroke font of
LaserGRBL (`freekerf_planner::hershey`).

| Generator | What it engraves | Parameters |
| --- | --- | --- |
| Cutting test (`cutting_test`) | 10 mm squares cut with a speed per column and one more pass per row, labelled | columns, first/last speed, first/last passes, power, label speed and power, title, laser on command |
| Power/speed test (`greyscale_test`) | a grid of filled cells, power changing along X and speed along Y, with its grid and labels | rows, columns, speed and power ranges, size, filling lines/mm, grid speed and power, title, label speed and power, laser on command |
| Shake test (`shake_test`) | oscillations of growing amplitude along one axis between two crosses: backlash and ringing show as blurred crosses | axis, oscillation speed, axis length, cross power and speed |

The G-code is the same as LaserGRBL's, including two quirks kept on purpose (see
[divergences](../divergences.md#importers-and-generators)): the pass range of the
cutting test label reads `1 - F2 pass` (DIV-039), and Hershey coordinates are
written with 15 significant digits, floating point noise included (DIV-040).

**Hershey text** (`hershey::text(text, cx, cy, speed, power, horizontal, laser_on)`):
the text is centred on `(cx, cy)`, horizontal or vertical, 2 mm high, characters
0.3 mm apart; characters outside printable ASCII are skipped.

## Safety

Generators write laser-on commands: like every job, they run only through `job.run`
(tier `laser`), which needs the user's confirmation.

## Tests

Unit tests in `generators.rs` and `hershey.rs`; `conformance` goldens
`gen_cutting_*`, `gen_greyscale_*`, `gen_shake_*`, `hershey_*` (all identical).
