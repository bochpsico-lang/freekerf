//! `cargo xtask sync-tokens`: brings the design system of
//! [`freekerf/design`](https://github.com/freekerf/design) into this repository at
//! the commit pinned in `ui/tokens/design.lock`:
//!
//! * `ui/tokens/fk-tokens.css`, generated from `tokens.json` (a port of
//!   `design/tokens/build_tokens.py`; the result must equal the CSS committed in
//!   `design` at that commit, which proves the port);
//! * the UI icons (`prototypes/app/icons.js`) as `ui/web/src/gen/icons.ts`;
//! * the OFL fonts, the favicon and logos, and the app icons of the Tauri bundle.
//!
//! Files are read with `git show <commit>:<path>`, so the state of the `design`
//! checkout does not matter as long as it contains the pinned commit. `--check`
//! fails when a committed file differs (CI).

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// A JSON value that keeps the order of object keys (the token order is the CSS order).
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// `null`, booleans and numbers, as written.
    Scalar(String),
    /// A string.
    Str(String),
    /// An array.
    List(Vec<Node>),
    /// An object, in file order.
    Map(Vec<(String, Node)>),
}

impl Node {
    fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Node;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON")
            }
            fn visit_unit<E>(self) -> Result<Node, E> {
                Ok(Node::Scalar("None".into()))
            }
            fn visit_bool<E>(self, v: bool) -> Result<Node, E> {
                Ok(Node::Scalar(if v { "True" } else { "False" }.into()))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Node, E> {
                Ok(Node::Scalar(v.to_string()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Node, E> {
                Ok(Node::Scalar(v.to_string()))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Node, E> {
                Ok(Node::Scalar(format!("{v:?}")))
            }
            fn visit_str<E>(self, v: &str) -> Result<Node, E> {
                Ok(Node::Str(v.to_owned()))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Node, A::Error> {
                let mut out = Vec::new();
                while let Some(n) = a.next_element()? {
                    out.push(n);
                }
                Ok(Node::List(out))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Node, A::Error> {
                let mut out = Vec::new();
                while let Some((k, v)) = a.next_entry::<String, Node>()? {
                    out.push((k, v));
                }
                Ok(Node::Map(out))
            }
        }
        d.deserialize_any(V)
    }
}

/// `fk-tokens.css` from `tokens.json` (same output as `build_tokens.py`).
///
/// # Errors
/// Malformed tokens or a reference to a missing token.
pub fn tokens_css(tokens_json: &str) -> Result<String> {
    let root: Node = serde_json::from_str(tokens_json).context("tokens.json")?;
    let data = root.get("fk").context("tokens.json: no `fk` group")?;
    let mut prims = Vec::new();
    for group in ["color", "font", "space", "radius", "elevation", "duration", "z"] {
        let node = data
            .get(group)
            .with_context(|| format!("tokens.json: no `{group}`"))?;
        leaves(data, node, &format!("fk-{group}"), &mut prims)?;
    }
    let renamed = |section: &str, name: &str, to: &str| -> Result<Vec<(String, String)>> {
        let node = data
            .get(section)
            .and_then(|s| s.get(name))
            .with_context(|| format!("tokens.json: no `{section}.{name}`"))?;
        let prefix = format!("fk-{section}-{name}");
        let mut out = Vec::new();
        leaves(data, node, &prefix, &mut out)?;
        Ok(out
            .into_iter()
            .map(|(n, v)| (n.replace(&format!("{prefix}-"), to), v))
            .collect())
    };
    let theme = |t: &str| renamed("theme", t, "fk-color-");
    let density = |d: &str| renamed("density", d, "fk-");
    let mut css =
        String::from("/* GERADO por tokens/build_tokens.py a partir de tokens.json — não edite à mão. */\n");
    let mut root_block = prims;
    root_block.extend(theme("dark")?);
    root_block.extend(density("compact")?);
    css.push_str(&block(":root", &root_block));
    css.push_str(&block("[data-theme=\"light\"]", &theme("light")?));
    css.push_str(&block("[data-contrast=\"high\"]", &theme("high-contrast")?));
    css.push_str(&block("[data-density=\"comfortable\"]", &density("comfortable")?));
    Ok(css)
}

fn block(selector: &str, pairs: &[(String, String)]) -> String {
    let mut s = format!("{selector} {{\n");
    for (n, v) in pairs {
        s.push_str(&format!("  --{n}: {v};\n"));
    }
    s.push_str("}\n");
    s
}

fn leaves(data: &Node, node: &Node, prefix: &str, out: &mut Vec<(String, String)>) -> Result<()> {
    let Node::Map(entries) = node else { return Ok(()) };
    for (k, v) in entries {
        if k.starts_with('$') {
            continue;
        }
        if let Some(value) = v.get("$value") {
            out.push((format!("{prefix}-{k}"), resolve(data, value, 0)?));
        } else if matches!(v, Node::Map(_)) {
            leaves(data, v, &format!("{prefix}-{k}"), out)?;
        }
    }
    Ok(())
}

fn resolve(data: &Node, v: &Node, depth: usize) -> Result<String> {
    if depth > 16 {
        bail!("tokens.json: reference loop");
    }
    Ok(match v {
        Node::Str(s) => match s.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
            Some(path) => {
                let mut node = data;
                for part in path.split('.').skip(1) {
                    node = node
                        .get(part)
                        .with_context(|| format!("tokens.json: unknown reference `{s}`"))?;
                }
                let value = node
                    .get("$value")
                    .with_context(|| format!("`{s}` is not a token"))?;
                resolve(data, value, depth + 1)?
            }
            None => s.clone(),
        },
        Node::List(items) => items
            .iter()
            .map(|i| match i {
                Node::Str(s) if s.contains(' ') => format!("\"{s}\""),
                Node::Str(s) | Node::Scalar(s) => s.clone(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join(", "),
        Node::Map(_) if v.get("offsetX").is_some() => {
            let f = |k: &str| match v.get(k) {
                Some(Node::Str(s) | Node::Scalar(s)) => s.clone(),
                _ => String::new(),
            };
            format!(
                "{} {} {} {} {}",
                f("offsetX"),
                f("offsetY"),
                f("blur"),
                f("spread"),
                f("color")
            )
        }
        Node::Scalar(s) => s.clone(),
        Node::Map(_) => String::new(),
    })
}

/// `icons.ts` from the prototype's `icons.js` (`name: '<svg body>',` entries).
///
/// # Errors
/// No icon found.
pub fn icons_ts(icons_js: &str) -> Result<String> {
    let mut out = String::from(
        "// Generated by `cargo xtask sync-tokens` from freekerf/design `prototypes/app/icons.js`.\n\
         // Original FreeKerf icons, GPL-3.0-or-later (design/CREDITS.md). Do not edit.\n\
         // Each entry is the body of an `<svg viewBox=\"0 0 24 24\">`; `class=\"f\"` elements are filled.\n\n\
         export const ICONS = {\n",
    );
    let mut n = 0;
    for line in icons_js.lines() {
        let t = line.trim();
        let Some((name, rest)) = t.split_once(": '") else {
            continue;
        };
        if !name.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let Some(body) = rest.strip_suffix("',").or_else(|| rest.strip_suffix('\'')) else {
            continue;
        };
        out.push_str(&format!("  {name}: {},\n", serde_json::to_string(body)?));
        n += 1;
    }
    if n == 0 {
        bail!("icons.js: no icons found");
    }
    out.push_str("} as const;\n\n/** Icon names. */\nexport type IconName = keyof typeof ICONS;\n");
    Ok(out)
}

/// A file to write: destination (relative to the repository) and contents.
pub struct Output {
    /// Destination.
    pub path: PathBuf,
    /// Bytes.
    pub bytes: Vec<u8>,
}

fn git(design: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(design)
        .args(args)
        .output()
        .context("git")?;
    if !out.status.success() {
        bail!(
            "git {} in {}: {}",
            args.join(" "),
            design.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

/// The pinned commit of `design`.
///
/// # Errors
/// Missing or malformed lock file.
pub fn pinned(root: &Path) -> Result<String> {
    let lock =
        std::fs::read_to_string(root.join("ui/tokens/design.lock")).context("ui/tokens/design.lock")?;
    let commit = lock
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .context("ui/tokens/design.lock: no commit")?;
    if commit.len() != 40 || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("ui/tokens/design.lock: `{commit}` is not a full commit hash");
    }
    Ok(commit.to_owned())
}

/// Every synced file, read from `design` at `commit`.
///
/// # Errors
/// The commit or a file is missing, or the generator disagrees with `design`.
pub fn outputs(design: &Path, commit: &str) -> Result<Vec<Output>> {
    let show = |path: &str| git(design, &["show", &format!("{commit}:{path}")]);
    let text = |path: &str| -> Result<String> { Ok(String::from_utf8(show(path)?)?) };
    let css = tokens_css(&text("tokens.json")?)?;
    if css != text("tokens/fk-tokens.css")? {
        bail!(
            "generated fk-tokens.css differs from design's tokens/fk-tokens.css at {commit}: update tools/xtask/src/design.rs to match build_tokens.py"
        );
    }
    let mut out = vec![
        Output {
            path: "ui/tokens/fk-tokens.css".into(),
            bytes: css.into_bytes(),
        },
        Output {
            path: "ui/web/src/gen/icons.ts".into(),
            bytes: icons_ts(&text("prototypes/app/icons.js")?)?.into_bytes(),
        },
    ];
    let fonts = String::from_utf8(git(design, &["ls-tree", "--name-only", commit, "assets/fonts/"])?)?;
    for f in fonts.lines() {
        let name = f.rsplit('/').next().unwrap_or(f);
        out.push(Output {
            path: PathBuf::from("ui/web/public/fonts").join(name),
            bytes: show(f)?,
        });
    }
    let copies = [
        ("brand/icon/favicon.svg", "ui/web/public/favicon.svg"),
        (
            "brand/logo/freekerf-symbol-dark.svg",
            "ui/web/public/brand/freekerf-symbol-dark.svg",
        ),
        (
            "brand/logo/freekerf-symbol-light.svg",
            "ui/web/public/brand/freekerf-symbol-light.svg",
        ),
        (
            "brand/icon/png/freekerf-32.png",
            "apps/desktop/src-tauri/icons/32x32.png",
        ),
        (
            "brand/icon/png/freekerf-128.png",
            "apps/desktop/src-tauri/icons/128x128.png",
        ),
        (
            "brand/icon/png/freekerf-256.png",
            "apps/desktop/src-tauri/icons/128x128@2x.png",
        ),
        (
            "brand/icon/png/freekerf-512.png",
            "apps/desktop/src-tauri/icons/icon.png",
        ),
        (
            "brand/icon/freekerf.icns",
            "apps/desktop/src-tauri/icons/icon.icns",
        ),
        ("brand/icon/freekerf.ico", "apps/desktop/src-tauri/icons/icon.ico"),
    ];
    for (from, to) in copies {
        out.push(Output {
            path: to.into(),
            bytes: show(from)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKENS: &str = r##"{"fk": {
        "color": {"$type": "color", "brand": {"amber-500": {"$value": "#F4A51C"}}, "neutral": {"900": {"$value": "#151A20"}}},
        "font": {"family": {"ui": {"$value": ["Inter", "Segoe UI"]}}, "weight": {"bold": {"$value": 700}}},
        "space": {"1": {"$value": "4px"}},
        "radius": {}, "duration": {}, "z": {"estop": {"$value": 100}},
        "elevation": {"1": {"$value": {"color": "#0004", "offsetX": "0px", "offsetY": "1px", "blur": "2px", "spread": "0px"}}},
        "theme": {
            "dark": {"surface-1": {"$value": "{fk.color.neutral.900}"}, "accent": {"$value": "{fk.color.brand.amber-500}"}},
            "light": {"accent": {"$value": "#B86E00"}},
            "high-contrast": {"text": {"$value": "#FFFFFF"}}
        },
        "density": {"compact": {"control-h": {"$value": "28px"}}, "comfortable": {"control-h": {"$value": "44px"}}}
    }}"##;

    #[test]
    fn css_matches_the_python_generator_shape() {
        let css = tokens_css(TOKENS).unwrap();
        let expected = "/* GERADO por tokens/build_tokens.py a partir de tokens.json — não edite à mão. */\n\
:root {\n  --fk-color-brand-amber-500: #F4A51C;\n  --fk-color-neutral-900: #151A20;\n  \
--fk-font-family-ui: Inter, \"Segoe UI\";\n  --fk-font-weight-bold: 700;\n  --fk-space-1: 4px;\n  \
--fk-elevation-1: 0px 1px 2px 0px #0004;\n  --fk-z-estop: 100;\n  --fk-color-surface-1: #151A20;\n  \
--fk-color-accent: #F4A51C;\n  --fk-control-h: 28px;\n}\n\
[data-theme=\"light\"] {\n  --fk-color-accent: #B86E00;\n}\n\
[data-contrast=\"high\"] {\n  --fk-color-text: #FFFFFF;\n}\n\
[data-density=\"comfortable\"] {\n  --fk-control-h: 44px;\n}\n";
        assert_eq!(css, expected);
    }

    #[test]
    fn bad_references_fail() {
        let bad = TOKENS.replace("{fk.color.neutral.900}", "{fk.color.neutral.999}");
        assert!(tokens_css(&bad).is_err());
    }

    #[test]
    fn icons_are_extracted() {
        let js = "FK.ICONS = {\n  select: '<path d=\"M5 3\"/>',\n  dot: '<circle r=\"4\"/>'\n};\nFK.icon = function (name, cls) {";
        let ts = icons_ts(js).unwrap();
        assert!(
            ts.contains("  select: \"<path d=\\\"M5 3\\\"/>\",\n  dot: \"<circle r=\\\"4\\\"/>\",\n"),
            "{ts}"
        );
        assert!(icons_ts("nothing").is_err());
    }
}
