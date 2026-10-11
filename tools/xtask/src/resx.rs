//! `cargo xtask resx2ftl --lasergrbl <checkout> [--check]`: reuses the LaserGRBL
//! translations that still apply (PLAN M4a). `ui/i18n/lasergrbl.toml` maps FreeKerf
//! message ids to `.resx` keys and pins the LaserGRBL commit; for every culture
//! LaserGRBL translates, the mapped strings become `ui/i18n/<locale>/lasergrbl.ftl`
//! (messages already in `<locale>/freekerf.ftl` are skipped: hand-written wins).
//! Files are read with `git show <commit>:<path>`, so the checkout may be sparse or
//! on another branch.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// The mapping file.
#[derive(Debug, serde::Deserialize)]
pub struct Mapping {
    /// LaserGRBL commit.
    pub commit: String,
    /// FreeKerf id → `path/File.resx:key`.
    pub messages: BTreeMap<String, String>,
}

/// String entries of a `.resx` (plain `data` with a `value`, no type).
///
/// # Errors
/// Not XML.
pub fn resx_strings(xml: &str) -> Result<BTreeMap<String, String>> {
    let doc = roxmltree::Document::parse(xml).context("resx")?;
    let mut out = BTreeMap::new();
    for d in doc.root_element().children().filter(|n| n.has_tag_name("data")) {
        if d.attribute("type").is_some() || d.attribute("mimetype").is_some() {
            continue;
        }
        let Some(name) = d.attribute("name") else { continue };
        let value = d
            .children()
            .find(|n| n.has_tag_name("value"))
            .and_then(|v| v.text())
            .unwrap_or("");
        out.insert(name.to_owned(), value.to_owned());
    }
    Ok(out)
}

/// A LaserGRBL UI string as FreeKerf text: no accelerator `&`, no trailing colon,
/// trimmed. `None` when nothing is left.
#[must_use]
pub fn clean(text: &str) -> Option<String> {
    let mut s = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                s.push('&');
                chars.next();
            }
            continue;
        }
        s.push(c);
    }
    let s = s.trim().trim_end_matches(':').trim().to_owned();
    (!s.is_empty()).then_some(s)
}

/// A Fluent message `id = value` (braces escaped, lines joined).
#[must_use]
pub fn ftl_message(id: &str, value: &str) -> String {
    let one_line = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let escaped: String = one_line
        .chars()
        .map(|c| match c {
            '{' => "{\"{\"}".to_owned(),
            '}' => "{\"}\"}".to_owned(),
            c => c.to_string(),
        })
        .collect();
    // a value starting with these characters would be read as Fluent syntax
    let escaped = if escaped.starts_with(['[', '*', '.']) {
        format!("{{\"{}\"}}{}", &escaped[..1], &escaped[1..])
    } else {
        escaped
    };
    format!("{id} = {escaped}\n")
}

fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .context("git")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

/// Message ids defined in a `.ftl` text.
fn ftl_ids(text: &str) -> BTreeSet<String> {
    text.lines()
        .filter_map(|l| {
            let (id, _) = l.split_once('=')?;
            let id = id.trim();
            (!id.is_empty()
                && !l.starts_with([' ', '#'])
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .then(|| id.to_owned())
        })
        .collect()
}

/// The generated files: (path relative to the repository, contents).
///
/// # Errors
/// Missing mapping, git or XML errors, or a mapped key missing in the neutral `.resx`.
pub fn outputs(root: &Path, lasergrbl: &Path) -> Result<Vec<(PathBuf, String)>> {
    let map: Mapping = toml::from_str(&std::fs::read_to_string(root.join("ui/i18n/lasergrbl.toml"))?)
        .context("ui/i18n/lasergrbl.toml")?;
    let commit = map.commit.as_str();
    let files = String::from_utf8(git(lasergrbl, &["ls-tree", "-r", "--name-only", commit])?)?;
    // neutral files used by the mapping, and their cultures
    let mut by_file: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (id, target) in &map.messages {
        let (file, key) = target
            .split_once(':')
            .with_context(|| format!("{id}: expected `path/File.resx:key`"))?;
        by_file.entry(file).or_default().push((id, key));
    }
    let mut per_locale: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (file, entries) in &by_file {
        let neutral = resx_strings(&String::from_utf8(git(
            lasergrbl,
            &["show", &format!("{commit}:{file}")],
        )?)?)?;
        for (id, key) in entries {
            if !neutral.contains_key(*key) {
                bail!("{id}: `{key}` is not in {file} at {commit}");
            }
        }
        let stem = file.trim_end_matches(".resx");
        for path in files.lines() {
            let Some(culture) = path
                .strip_prefix(stem)
                .and_then(|r| r.strip_prefix('.'))
                .and_then(|r| r.strip_suffix(".resx"))
            else {
                continue;
            };
            if culture.contains('/') || culture.is_empty() {
                continue;
            }
            let strings = resx_strings(&String::from_utf8(git(
                lasergrbl,
                &["show", &format!("{commit}:{path}")],
            )?)?)?;
            for (id, key) in entries {
                if let Some(text) = strings.get(*key).and_then(|t| clean(t)) {
                    per_locale
                        .entry(culture.to_owned())
                        .or_default()
                        .insert((*id).to_owned(), text);
                }
            }
        }
    }
    let mut out = Vec::new();
    for (locale, messages) in per_locale {
        let own =
            std::fs::read_to_string(root.join(format!("ui/i18n/{locale}/freekerf.ftl"))).unwrap_or_default();
        let own = ftl_ids(&own);
        let body: String = messages
            .iter()
            .filter(|(id, _)| !own.contains(*id))
            .map(|(id, text)| ftl_message(id, text))
            .collect();
        if body.is_empty() {
            continue;
        }
        let text = format!(
            "# Generated by `cargo xtask resx2ftl` from LaserGRBL {commit} (GPL-3.0,\n\
             # © Diego Settimi and the LaserGRBL translators). Do not edit: change\n\
             # ui/i18n/lasergrbl.toml or write the message in freekerf.ftl instead.\n\n{body}"
        );
        out.push((PathBuf::from(format!("ui/i18n/{locale}/lasergrbl.ftl")), text));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_resx_strings_only() {
        let xml = r#"<?xml version="1.0"?><root>
            <data name="A.Text" xml:space="preserve"><value>&amp;Connect</value></data>
            <data name="Img" type="System.Drawing.Bitmap"><value>AAAA</value></data>
            <data name="B"><value>Line #:</value></data>
        </root>"#;
        let s = resx_strings(xml).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(clean(&s["A.Text"]).unwrap(), "Connect");
        assert_eq!(clean(&s["B"]).unwrap(), "Line #");
        assert_eq!(clean("R&&D"), Some("R&D".into()));
        assert_eq!(clean(" : "), None);
    }

    #[test]
    fn writes_fluent_safely() {
        assert_eq!(ftl_message("a-b", "Hello\n  world"), "a-b = Hello world\n");
        assert_eq!(ftl_message("x", "{0} done"), "x = {\"{\"}0{\"}\"} done\n");
        assert_eq!(ftl_message("y", "[safe]"), "y = {\"[\"}safe]\n");
    }

    #[test]
    fn finds_ids() {
        let ids = ftl_ids("# c\nmode-design = Design\n\n## g\nx-y = a = b\n  .attr = z\n");
        assert_eq!(ids.into_iter().collect::<Vec<_>>(), ["mode-design", "x-y"]);
    }
}
