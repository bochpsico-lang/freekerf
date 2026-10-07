//! The layer rules (PLAN §3), checked on `cargo metadata`.

use serde::Deserialize;

/// The part of `cargo metadata` we need.
#[derive(Debug, Deserialize)]
pub struct Metadata {
    /// Workspace packages.
    pub packages: Vec<Package>,
}

/// A workspace package.
#[derive(Debug, Deserialize)]
pub struct Package {
    /// Name.
    pub name: String,
    /// Path of its `Cargo.toml`.
    pub manifest_path: String,
    /// Declared dependencies.
    pub dependencies: Vec<Dependency>,
}

/// A declared dependency.
#[derive(Debug, Deserialize)]
pub struct Dependency {
    /// Crate name.
    pub name: String,
    /// `null` (normal), `"dev"` or `"build"`.
    pub kind: Option<String>,
}

/// Layer of a workspace crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Pure logic.
    Core,
    /// Persistent data.
    Config,
    /// OS and hardware.
    Native,
    /// Orchestration.
    Runtime,
    /// Binaries.
    Apps,
    /// Test support.
    Testing,
    /// Cross-crate integration tests.
    IntegrationTests,
    /// Repository tools.
    Tools,
}

/// The layer of a package from its location.
#[must_use]
pub fn layer_of(manifest_path: &str) -> Option<Layer> {
    let p = manifest_path.replace('\\', "/");
    [
        ("/crates/core/", Layer::Core),
        ("/crates/config/", Layer::Config),
        ("/crates/native/", Layer::Native),
        ("/crates/runtime/", Layer::Runtime),
        ("/crates/testing/", Layer::Testing),
        ("/apps/", Layer::Apps),
        ("/tools/", Layer::Tools),
        ("/tests/Cargo.toml", Layer::IntegrationTests),
    ]
    .into_iter()
    .find(|(part, _)| p.contains(part))
    .map(|(_, l)| l)
}

/// May a crate of layer `from` depend on a crate of layer `to` (normal dependency)?
#[must_use]
pub const fn allowed(from: Layer, to: Layer) -> bool {
    use Layer::{Apps, Config, Core, IntegrationTests, Native, Runtime, Testing, Tools};
    match from {
        Core => matches!(to, Core),
        Config => matches!(to, Core | Config),
        Native => matches!(to, Core | Native),
        Runtime => matches!(to, Core | Config | Native | Runtime),
        // apps go through the service (runtime), never protocol/transport directly
        Apps => matches!(to, Runtime | Config),
        Testing | IntegrationTests | Tools => true,
    }
}

/// Third-party crates the core must not use (I/O, OS, threads of their own).
pub const CORE_FORBIDDEN: &[&str] = &[
    "serialport",
    "nix",
    "libc",
    "directories",
    "crossbeam-channel",
    "tokio",
    "ctrlc",
    "tracing-subscriber",
];

/// Every violation, as text.
#[must_use]
pub fn check(meta: &Metadata) -> Vec<String> {
    let layer = |name: &str| {
        meta.packages
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| layer_of(&p.manifest_path))
    };
    let mut out = Vec::new();
    for p in &meta.packages {
        let Some(from) = layer_of(&p.manifest_path) else {
            out.push(format!("{}: not in a known layer directory", p.name));
            continue;
        };
        for d in &p.dependencies {
            let dev = d.kind.as_deref() == Some("dev");
            match layer(&d.name) {
                Some(Layer::Testing)
                    if !dev
                        && from != Layer::Testing
                        && from != Layer::IntegrationTests
                        && from != Layer::Tools =>
                {
                    out.push(format!(
                        "{} -> {}: test support may only be a dev-dependency",
                        p.name, d.name
                    ));
                }
                Some(_) if dev => {} // tests may use anything
                Some(to) if !allowed(from, to) => {
                    out.push(format!(
                        "{} ({from:?}) -> {} ({to:?}): not allowed",
                        p.name, d.name
                    ));
                }
                None if from == Layer::Core && !dev && CORE_FORBIDDEN.contains(&d.name.as_str()) => {
                    out.push(format!("{} (Core) -> {}: I/O crate in the core", p.name, d.name));
                }
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, dir: &str, deps: &[(&str, Option<&str>)]) -> Package {
        Package {
            name: name.into(),
            manifest_path: format!("/r/{dir}/{name}/Cargo.toml"),
            dependencies: deps
                .iter()
                .map(|(n, k)| Dependency {
                    name: (*n).into(),
                    kind: k.map(Into::into),
                })
                .collect(),
        }
    }

    fn meta(extra: Vec<Package>) -> Metadata {
        let mut packages = vec![
            pkg("freekerf-gcode", "crates/core", &[]),
            pkg(
                "freekerf-protocol",
                "crates/core",
                &[("freekerf-gcode", None), ("freekerf-testkit", Some("dev"))],
            ),
            pkg(
                "freekerf-settings",
                "crates/config",
                &[("freekerf-protocol", None)],
            ),
            pkg("freekerf-transport", "crates/native", &[("serialport", None)]),
            pkg(
                "freekerf-service",
                "crates/runtime",
                &[("freekerf-transport", None), ("freekerf-settings", None)],
            ),
            pkg(
                "freekerf-testkit",
                "crates/testing",
                &[("freekerf-protocol", None)],
            ),
            pkg("freekerf-cli", "apps", &[("freekerf-service", None)]),
            Package {
                name: "freekerf-it".into(),
                manifest_path: "/r/tests/Cargo.toml".into(),
                dependencies: vec![],
            },
        ];
        packages.extend(extra);
        Metadata { packages }
    }

    #[test]
    fn the_real_shape_passes() {
        assert!(check(&meta(vec![])).is_empty());
    }

    #[test]
    fn a_broken_rule_fails() {
        let bad = check(&meta(vec![
            pkg(
                "freekerf-bad-core",
                "crates/core",
                &[("freekerf-transport", None), ("serialport", None)],
            ),
            pkg("freekerf-bad-app", "apps", &[("freekerf-protocol", None)]),
            pkg("freekerf-bad-cfg", "crates/config", &[("freekerf-service", None)]),
            pkg("freekerf-bad-rt", "crates/runtime", &[("freekerf-testkit", None)]),
            pkg("freekerf-lost", "elsewhere", &[]),
        ]));
        assert_eq!(bad.len(), 6, "{bad:#?}");
        assert!(bad.iter().any(|v| v.contains("I/O crate in the core")));
        assert!(bad.iter().any(|v| v.contains("dev-dependency")));
        assert!(bad.iter().any(|v| v.contains("not in a known layer")));
    }

    #[test]
    fn rules_table() {
        assert!(allowed(Layer::Runtime, Layer::Native));
        assert!(!allowed(Layer::Native, Layer::Config));
        assert!(!allowed(Layer::Apps, Layer::Core));
        assert!(allowed(Layer::Tools, Layer::Apps));
        assert_eq!(layer_of("C:\\r\\crates\\core\\x\\Cargo.toml"), Some(Layer::Core));
    }
}
