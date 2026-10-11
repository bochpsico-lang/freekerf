# Material library

`freekerf-materials` reads release packages of
[`freekerf/library`](https://github.com/freekerf/library) (`library-vX.Y.Z.tar.zst`)
with that repository's `freekerf-library` crate, a git dependency pinned by `rev`
in the workspace `Cargo.toml` (no release has been published yet; tests build a
package locally with the crate's own packaging function).

```rust
let lib = freekerf_materials::MaterialLibrary::open(path)?; // every file's SHA-256 checked
let found = lib.search(&Query { text: Some("plywood".into()), thickness_mm: Some(3.0), ..Query::default() });
freekerf_materials::apply_recipe(&found[0].recipes[0], &mut layer, 1000); // $30 = 1000
```

- **Search**: words in the name or id, category, operation, laser kind, thickness
  (±0.05 mm).
- **Hazards**: blocked materials never reach a package; `needs_warning(id)` tells
  when the app must warn before use (warn-level hazards of the index).
- **Recipes on layers**: speed, passes, power (`S` = maximum power % of the
  machine's `$30`), mode (cut → cut, vector engraving → line, raster engraving →
  fill at the recipe's line interval).
- **LaserGRBL materials**: `import_lasergrbl(psh)` converts `UserMaterials.psh` /
  `StandardMaterials.psh` with the library's importer and the package's hazard list.

Downloading packages, keeping user materials and the library UI come later (PLAN
§10, v1); see `doc/roadmap.md`.

## Tests

`crates/config/freekerf-materials/tests/package.rs`: a package built from TOML
data, read back, searched, applied; a tampered package refused; LaserGRBL materials
imported with a blocked one skipped.
