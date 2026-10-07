# Importer golden cases (copied from conformance)

A subset of `freekerf/conformance` (`fixtures/importers/{cases,inputs}` and
`fixtures/golden/importers`), so `tests/import_cases.rs` checks the importers and the
planner in this repository too. The goldens were recorded with LaserGRBL (C#, Mono);
when they change in conformance, copy them again. The full set runs in the CI job
`conformance` (`make test-rust`).

`inputs/resources/LaserGRBL.Generator.SVG.LaserGRBL-accuracy-test-file.svg` is
LaserGRBL's accuracy test file (embedded in LaserGRBL.exe), © Diego Settimi, GPL-3.0.
