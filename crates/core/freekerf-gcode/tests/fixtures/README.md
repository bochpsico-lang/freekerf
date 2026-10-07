# Golden fixtures

Copied from [`freekerf/conformance`](https://github.com/freekerf/conformance)
(`fixtures/gcode/*.nc` and `fixtures/golden/*.json`, recorded with LaserGRBL).
`tests/golden.rs` checks every key but `stream`, which needs the protocol and is
checked by the cross-crate test `tests/it/golden_stream.rs`. When the conformance
suite updates its goldens, copy them again.
