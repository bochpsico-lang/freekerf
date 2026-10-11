//! Repository automation, run with `cargo xtask <command>`:
//!
//! * `check-layers`: fails when a dependency between workspace crates (or a
//!   forbidden third-party crate in the core) breaks the layer rules of
//!   `PLAN.md` §3 / `AGENTS.md`;
//! * `gen-docs`: regenerates the generated documentation (action catalogue,
//!   schemas, the script API's `api.d.ts`);
//! * `check-docs`: fails when the committed generated documentation is stale.

#![allow(clippy::print_stdout, clippy::print_stderr)] // a command-line tool

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};

mod layers;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("check-layers") => check_layers(),
        Some("gen-docs") => gen_docs(false),
        Some("check-docs") => gen_docs(true),
        _ => {
            eprintln!("usage: cargo xtask <check-layers|gen-docs|check-docs>");
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
    ])
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
