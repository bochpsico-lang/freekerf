//! The material library (PLAN §10, M2): release packages of
//! [`freekerf/library`](https://github.com/freekerf/library) read with its
//! `freekerf-library` crate (`read_archive` checks every file's SHA-256), searched,
//! and their recipes applied to a document layer. LaserGRBL's user materials
//! (`UserMaterials.psh`) are imported with the library's own LaserGRBL importer.

use std::path::Path;

use freekerf_document::{Layer, LayerMode};
use freekerf_library::import::Importer;
use freekerf_library::import::lasergrbl::{KeywordCategorizer, LaserGrblImporter, ModelResolver};
use freekerf_library::model::{
    HazardList, LaserKind, Material, MaterialCategory, Operation, Recipe, Severity,
};
use freekerf_library::package::{HazardRef, Index, read_archive};

pub use freekerf_library::model;

/// Errors of the material library.
#[derive(Debug, thiserror::Error)]
pub enum MaterialsError {
    /// The package could not be read or verified.
    #[error("material package: {0}")]
    Package(#[from] freekerf_library::package::PackageError),
    /// A file of the package is not what its index says.
    #[error("material package content: {0}")]
    Content(String),
    /// A LaserGRBL material file could not be imported.
    #[error("LaserGRBL materials: {0}")]
    Import(String),
}

/// A loaded material library.
#[derive(Debug, Clone)]
pub struct MaterialLibrary {
    index: Index,
    materials: Vec<Material>,
    hazards: HazardList,
}

/// What to look for; empty fields match everything.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    /// Words that must all appear in the name or id (case-insensitive).
    pub text: Option<String>,
    /// Category.
    pub category: Option<MaterialCategory>,
    /// Some recipe does this operation.
    pub operation: Option<Operation>,
    /// Some recipe is for this laser kind.
    pub laser: Option<LaserKind>,
    /// Thickness within 0.05 mm.
    pub thickness_mm: Option<f64>,
}

impl MaterialLibrary {
    /// Reads a release package (`library-vX.Y.Z.tar.zst`).
    ///
    /// # Errors
    /// Unreadable or tampered package.
    pub fn open(path: &Path) -> Result<Self, MaterialsError> {
        let package = read_archive(path)?;
        let materials = package.materials()?;
        let hazards = package
            .files
            .get(&package.index.hazards.path)
            .ok_or_else(|| MaterialsError::Content("no hazard list".into()))?;
        let hazards: HazardList =
            serde_json::from_slice(hazards).map_err(|e| MaterialsError::Content(e.to_string()))?;
        Ok(Self {
            index: package.index,
            materials,
            hazards,
        })
    }

    /// The package version (`X.Y.Z`).
    #[must_use]
    pub fn version(&self) -> &str {
        &self.index.version
    }

    /// Every material, in index order (sorted by id).
    #[must_use]
    pub fn materials(&self) -> &[Material] {
        &self.materials
    }

    /// The hazard list of the package.
    #[must_use]
    pub const fn hazard_list(&self) -> &HazardList {
        &self.hazards
    }

    /// A material by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }

    /// The hazards the index lists for a material (warnings to show before
    /// processing it; blocked materials never reach a package).
    #[must_use]
    pub fn hazards(&self, id: &str) -> &[HazardRef] {
        self.index
            .materials
            .iter()
            .find(|e| e.id == id)
            .map_or(&[], |e| e.hazards.as_slice())
    }

    /// Whether a material needs a warning before use.
    #[must_use]
    pub fn needs_warning(&self, id: &str) -> bool {
        self.hazards(id).iter().any(|h| h.severity == Severity::Warn)
    }

    /// The materials matching `q`, in id order.
    #[must_use]
    pub fn search(&self, q: &Query) -> Vec<&Material> {
        let words: Vec<String> = q
            .text
            .as_deref()
            .unwrap_or("")
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        self.materials
            .iter()
            .filter(|m| {
                let hay = format!("{} {}", m.name.to_lowercase(), m.id);
                words.iter().all(|w| hay.contains(w.as_str()))
                    && q.category.is_none_or(|c| m.category == c)
                    && q.operation
                        .is_none_or(|o| m.recipes.iter().any(|r| r.operation == o))
                    && q.laser
                        .is_none_or(|k| m.recipes.iter().any(|r| r.laser.kind == k))
                    && q.thickness_mm
                        .is_none_or(|t| m.thickness_mm.is_some_and(|mt| (mt - t).abs() <= 0.05))
            })
            .collect()
    }

    /// Imports LaserGRBL's `UserMaterials.psh` (or `StandardMaterials.psh`) with the
    /// library's importer and this package's hazard list (blocked materials are
    /// skipped and reported).
    ///
    /// # Errors
    /// Not a LaserGRBL material file.
    pub fn import_lasergrbl(&self, psh: &str) -> Result<Vec<Material>, MaterialsError> {
        let resolver = ModelResolver::default();
        let importer =
            LaserGrblImporter::new(&resolver, &KeywordCategorizer, &self.hazards, "UserMaterials.psh");
        importer
            .import(psh)
            .map(|o| o.materials)
            .map_err(|e| MaterialsError::Import(e.to_string()))
    }
}

/// Applies a recipe to a layer: speed, passes, power (`S` = the recipe's maximum
/// power percentage of `max_s`, the machine's `$30`), mode (cut, vector engraving as
/// lines, raster engraving as a filling at the recipe's line interval).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn apply_recipe(recipe: &Recipe, layer: &mut Layer, max_s: u32) {
    layer.speed_mm_min = recipe.speed_mm_min;
    layer.passes = recipe.passes.max(1);
    layer.power = ((recipe.power_max_pct.clamp(0.0, 100.0) / 100.0) * f64::from(max_s)).round() as u32;
    layer.mode = match recipe.operation {
        Operation::Cut => LayerMode::Cut,
        Operation::EngraveVector => LayerMode::Line,
        Operation::EngraveRaster => LayerMode::Fill,
    };
    if let Some(interval) = recipe.line_interval_mm.filter(|i| *i > 0.0) {
        layer.fill.lines_per_mm = 1.0 / interval;
    }
}
