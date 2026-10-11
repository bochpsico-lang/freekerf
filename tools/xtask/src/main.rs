//! Repository automation, run with `cargo xtask <command>`:
//!
//! * `check-layers`: fails when a dependency between workspace crates (or a
//!   forbidden third-party crate in the core) breaks the layer rules of
//!   `PLAN.md` §3 / `AGENTS.md`;
//! * `gen-docs`: regenerates the generated documentation (action catalogue,
//!   schemas, the script API's `api.d.ts`, the UI's TypeScript types);
//! * `check-docs`: fails when the committed generated documentation (and the
//!   generated TypeScript types) are stale;
//! * `sync-tokens [--design <dir>] [--check]`: tokens, icons, fonts and app icons
//!   from `freekerf/design` at the commit of `ui/tokens/design.lock` (default
//!   checkout: `$FREEKERF_DESIGN` or `../design`);
//! * `resx2ftl --lasergrbl <checkout> [--check]`: LaserGRBL translations that still
//!   apply, as `ui/i18n/<locale>/lasergrbl.ftl` (mapping in `ui/i18n/lasergrbl.toml`);
//! * `gen-ts`: same as `gen-docs` (the TypeScript types of `ui/web/src/gen/` are
//!   generated files too).

#![allow(clippy::print_stdout, clippy::print_stderr)] // a command-line tool

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use serde_json::Value;

mod design;
mod layers;
mod resx;
mod ts;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("check-layers") => check_layers(),
        Some("gen-docs" | "gen-ts") => gen_docs(false),
        Some("check-docs") => gen_docs(true),
        Some("sync-tokens") => sync_tokens(&args[1..]),
        Some("resx2ftl") => resx2ftl(&args[1..]),
        _ => {
            eprintln!("usage: cargo xtask <check-layers|gen-docs|gen-ts|check-docs|sync-tokens|resx2ftl>");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.ancestors().nth(2).unwrap_or(manifest).to_path_buf()
}

fn check_layers() -> Result<()> {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root())
        .output()
        .context("cargo metadata")?;
    if !out.status.success() {
        bail!("cargo metadata failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    let meta: layers::Metadata = serde_json::from_slice(&out.stdout)?;
    let violations = layers::check(&meta);
    if violations.is_empty() {
        println!("layers: ok ({} workspace crates)", meta.packages.len());
        Ok(())
    } else {
        for v in &violations {
            eprintln!("layer violation: {v}");
        }
        bail!(
            "{} layer violation(s); see AGENTS.md \"Layers\"",
            violations.len()
        )
    }
}

/// Generated files: (path, contents).
fn generated() -> Result<Vec<(PathBuf, String)>> {
    let service = freekerf_service::Service::in_memory()?;
    Ok(vec![
        (
            root().join("doc/features/actions.md"),
            service.registry().to_markdown(),
        ),
        (root().join("doc/scripting/api.d.ts"), service.api_dts()),
        (
            root().join("doc/schema/settings.schema.json"),
            freekerf_settings::json_schema(),
        ),
        (
            root().join("doc/schema/fkproj.schema.json"),
            freekerf_document::json_schema(),
        ),
        (root().join("ui/web/src/gen/api.ts"), typescript(&service)?),
    ])
}

/// `ui/web/src/gen/api.ts`.
fn typescript(service: &freekerf_service::Service) -> Result<String> {
    let value = |s: schemars::Schema| serde_json::to_value(s).unwrap_or(Value::Null);
    let settings: Value = serde_json::from_str(&freekerf_settings::json_schema())?;
    let events = value(schemars::schema_for!(freekerf_service::EventMessage));
    let error = value(schemars::schema_for!(freekerf_service::ActionError));
    let tier = value(schemars::schema_for!(freekerf_service::Tier));
    let spec = value(schemars::schema_for!(freekerf_service::ActionSpec));
    let roots = [
        ts::Root {
            name: "Tier",
            schema: &tier,
        },
        ts::Root {
            name: "ActionError",
            schema: &error,
        },
        ts::Root {
            name: "ActionSpec",
            schema: &spec,
        },
        ts::Root {
            name: "EventMessage",
            schema: &events,
        },
        ts::Root {
            name: "Settings",
            schema: &settings,
        },
    ];
    let specs = service.specs();
    let actions: Vec<ts::Action<'_>> = specs
        .iter()
        .map(|s| ts::Action {
            name: &s.name,
            tier: s.tier.as_str(),
            description: &s.description,
            params: &s.params,
            result: s.result.as_ref(),
        })
        .collect();
    ts::generate(&roots, &actions)
}

fn gen_docs(check: bool) -> Result<()> {
    let mut stale = Vec::new();
    for (path, contents) in generated()? {
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current == contents {
            continue;
        }
        if check {
            stale.push(path.display().to_string());
        } else {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, contents)?;
            println!("wrote {}", path.display());
        }
    }
    if stale.is_empty() {
        Ok(())
    } else {
        bail!(
            "generated docs are stale (run `cargo xtask gen-docs`): {}",
            stale.join(", ")
        )
    }
}

fn sync_tokens(args: &[String]) -> Result<()> {
    let check = args.iter().any(|a| a == "--check");
    let design = args
        .iter()
        .position(|a| a == "--design")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("FREEKERF_DESIGN").map(PathBuf::from))
        .unwrap_or_else(|| root().join("../design"));
    let commit = design::pinned(&root())?;
    let mut stale = Vec::new();
    for out in design::outputs(&design, &commit)? {
        let path = root().join(&out.path);
        if std::fs::read(&path).ok().as_deref() == Some(out.bytes.as_slice()) {
            continue;
        }
        if check {
            stale.push(out.path.display().to_string());
        } else {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, &out.bytes)?;
            println!("wrote {}", out.path.display());
        }
    }
    if stale.is_empty() {
        println!("design {commit}: in sync");
        Ok(())
    } else {
        bail!(
            "files differ from design {commit} (run `cargo xtask sync-tokens`): {}",
            stale.join(", ")
        )
    }
}

fn resx2ftl(args: &[String]) -> Result<()> {
    let check = args.iter().any(|a| a == "--check");
    let checkout = args
        .iter()
        .position(|a| a == "--lasergrbl")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LASERGRBL_REPO").map(PathBuf::from))
        .context("pass --lasergrbl <checkout of arkypita/LaserGRBL> (or set LASERGRBL_REPO)")?;
    let outputs = resx::outputs(&root(), &checkout)?;
    let mut stale = Vec::new();
    for (path, text) in &outputs {
        let full = root().join(path);
        if std::fs::read_to_string(&full).ok().as_deref() == Some(text.as_str()) {
            continue;
        }
        if check {
            stale.push(path.display().to_string());
        } else {
            if let Some(d) = full.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(&full, text)?;
            println!("wrote {}", path.display());
        }
    }
    // files no longer generated (every message is now hand-written in that locale)
    for dir in std::fs::read_dir(root().join("ui/i18n"))?.flatten() {
        let file = dir.path().join("lasergrbl.ftl");
        let rel = file.strip_prefix(root()).unwrap_or(&file).to_path_buf();
        if file.exists() && !outputs.iter().any(|(p, _)| *p == rel) {
            if check {
                stale.push(rel.display().to_string());
            } else {
                std::fs::remove_file(&file)?;
                println!("removed {}", rel.display());
            }
        }
    }
    if stale.is_empty() {
        println!("resx2ftl: {} locale files in sync", outputs.len());
        Ok(())
    } else {
        bail!("stale (run `cargo xtask resx2ftl`): {}", stale.join(", "))
    }
}
