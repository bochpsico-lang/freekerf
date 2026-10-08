//! `cargo xtask gen-ts`: TypeScript types for the web front end, generated from the
//! JSON schemas of the Rust types (ADR 0009). Nothing in `ui/web` duplicates a Rust
//! type by hand: the action catalogue (parameters, results, tiers), the event
//! messages and the settings all come from here, and CI checks committed ==
//! generated.
//!
//! The emitter covers the subset of JSON Schema that `schemars` produces: objects,
//! `$ref` to `$defs`, `oneOf`/`anyOf`/`allOf`, `const`/`enum`, nullable type lists,
//! arrays (fixed-length arrays become tuples) and maps.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use anyhow::{Result, bail};
use serde_json::{Map, Value};

/// One named root schema to emit as `export type <name> = ...`.
pub struct Root<'a> {
    /// TypeScript name.
    pub name: &'a str,
    /// The schema (with its `$defs`).
    pub schema: &'a Value,
}

/// An action of the catalogue.
pub struct Action<'a> {
    /// `namespace.name`.
    pub name: &'a str,
    /// Tier (`safety`...`laser`).
    pub tier: &'a str,
    /// Description (first line is the summary).
    pub description: &'a str,
    /// Parameters schema.
    pub params: &'a Value,
    /// Result schema, if described.
    pub result: Option<&'a Value>,
}

/// Collected named definitions.
#[derive(Default)]
struct Defs {
    defs: BTreeMap<String, Value>,
}

impl Defs {
    /// Adds the `$defs` of a root schema; the same name must mean the same schema.
    fn absorb(&mut self, schema: &Value) -> Result<()> {
        let Some(defs) = schema.get("$defs").and_then(Value::as_object) else {
            return Ok(());
        };
        for (name, def) in defs {
            match self.defs.get(name) {
                Some(existing) if existing != def => {
                    bail!("two different Rust types are both named `{name}`; rename one of them")
                }
                Some(_) => {}
                None => {
                    self.defs.insert(name.clone(), def.clone());
                }
            }
        }
        Ok(())
    }

    /// Absorbs a root and returns its TypeScript type (a name when it is named).
    fn root(&mut self, schema: &Value, indent: &str) -> Result<String> {
        self.absorb(schema)?;
        let Some(name) = named(schema) else {
            return Ok(ty(schema, indent));
        };
        let def = bare(schema);
        match self.defs.get(name) {
            Some(existing) if *existing != def => {
                bail!("two different Rust types are both named `{name}`; rename one of them")
            }
            Some(_) => {}
            None => {
                self.defs.insert(name.to_owned(), def);
            }
        }
        Ok(name.to_owned())
    }
}

/// The Rust name of a root schema that stands for a named struct or enum
/// (`State`, `ConnectParams`); `None` for generic wrappers (`Array_of_X`,
/// `Nullable_X`, `null`), which are inlined.
fn named(schema: &Value) -> Option<&str> {
    let title = schema.get("title")?.as_str()?;
    let shaped = ["properties", "oneOf", "enum"]
        .iter()
        .any(|k| schema.get(*k).is_some())
        || schema.get("type") == Some(&Value::String("object".into()));
    (shaped && ident_ok(title) && !title.contains('_') && title.starts_with(|c: char| c.is_ascii_uppercase()))
        .then_some(title)
}

/// The root without its `$defs`, `$schema` and `title`.
fn bare(schema: &Value) -> Value {
    let mut v = schema.clone();
    if let Some(o) = v.as_object_mut() {
        for k in ["$defs", "$schema", "title"] {
            o.remove(k);
        }
    }
    v
}

fn ident_ok(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        && !name.starts_with(|c: char| c.is_ascii_digit())
}

fn key(name: &str) -> String {
    if ident_ok(name) {
        name.to_owned()
    } else {
        serde_json::to_string(name).unwrap_or_default()
    }
}

fn doc(out: &mut String, indent: &str, text: Option<&str>) {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return;
    };
    let text = text.replace("*/", "*\\/");
    if text.contains('\n') {
        let _ = writeln!(out, "{indent}/**");
        for line in text.lines() {
            let _ = writeln!(out, "{indent} * {line}").map(|()| ());
        }
        let _ = writeln!(out, "{indent} */");
    } else {
        let _ = writeln!(out, "{indent}/** {text} */");
    }
}

fn literal(v: &Value) -> String {
    match v {
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null => v.to_string(),
        _ => "unknown".into(),
    }
}

fn union(parts: Vec<String>) -> String {
    let mut seen = Vec::new();
    for p in parts {
        if !seen.contains(&p) {
            seen.push(p);
        }
    }
    match seen.len() {
        0 => "never".into(),
        1 => seen.remove(0),
        _ => seen.join(" | "),
    }
}

/// The TypeScript type of a schema node.
fn ty(schema: &Value, indent: &str) -> String {
    let obj = match schema {
        Value::Bool(false) => return "never".into(),
        Value::Object(o) => o,
        _ => return "unknown".into(),
    };
    if let Some(r) = obj.get("$ref").and_then(Value::as_str) {
        return r.rsplit('/').next().unwrap_or("unknown").to_owned();
    }
    if let Some(c) = obj.get("const") {
        return literal(c);
    }
    if let Some(e) = obj.get("enum").and_then(Value::as_array) {
        return union(e.iter().map(literal).collect());
    }
    for k in ["oneOf", "anyOf"] {
        if let Some(alts) = obj.get(k).and_then(Value::as_array) {
            return union(alts.iter().map(|a| paren(&ty(a, indent))).collect());
        }
    }
    if let Some(all) = obj.get("allOf").and_then(Value::as_array) {
        return all
            .iter()
            .map(|a| paren(&ty(a, indent)))
            .collect::<Vec<_>>()
            .join(" & ");
    }
    let types: Vec<&str> = match obj.get("type") {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ if obj.contains_key("properties") => vec!["object"],
        _ => return "unknown".into(),
    };
    union(types.into_iter().map(|t| simple(t, obj, indent)).collect())
}

/// Wraps a union or intersection in parentheses (only operators at depth 0 count).
fn paren(t: &str) -> String {
    let mut depth = 0i32;
    let mut top = false;
    let bytes = t.as_bytes();
    for (i, c) in t.char_indices() {
        match c {
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth -= 1,
            '|' | '&' if depth == 0 && i > 0 && bytes.get(i - 1) == Some(&b' ') => top = true,
            _ => {}
        }
    }
    if top { format!("({t})") } else { t.to_owned() }
}

fn simple(t: &str, obj: &Map<String, Value>, indent: &str) -> String {
    match t {
        "string" => "string".into(),
        "integer" | "number" => "number".into(),
        "boolean" => "boolean".into(),
        "null" => "null".into(),
        "array" => array(obj, indent),
        "object" => object(obj, indent),
        _ => "unknown".into(),
    }
}

fn array(obj: &Map<String, Value>, indent: &str) -> String {
    if let Some(items) = obj.get("prefixItems").and_then(Value::as_array) {
        let parts: Vec<String> = items.iter().map(|i| ty(i, indent)).collect();
        return format!("[{}]", parts.join(", "));
    }
    let item = obj
        .get("items")
        .map_or_else(|| "unknown".into(), |i| ty(i, indent));
    let min = obj.get("minItems").and_then(Value::as_u64);
    let max = obj.get("maxItems").and_then(Value::as_u64);
    match (min, max) {
        (Some(n), Some(m)) if n == m && n <= 8 => {
            let n = usize::try_from(n).unwrap_or(0);
            format!("[{}]", vec![item; n].join(", "))
        }
        _ => format!("{}[]", paren(&item)),
    }
}

fn object(obj: &Map<String, Value>, indent: &str) -> String {
    let props = obj.get("properties").and_then(Value::as_object);
    let extra = match obj.get("additionalProperties") {
        Some(Value::Bool(false)) | None => None,
        Some(Value::Bool(true)) => Some("unknown".to_owned()),
        Some(s) => Some(ty(s, indent)),
    };
    let props = match props {
        Some(p) if !p.is_empty() => p,
        _ => {
            return match extra {
                Some(v) => format!("Record<string, {v}>"),
                // `additionalProperties: false` and nothing else: an empty object
                None if obj.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    "Record<string, never>".into()
                }
                None => "Record<string, unknown>".into(),
            };
        }
    };
    let required: Vec<&str> = obj
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let inner = format!("{indent}  ");
    let mut s = String::from("{\n");
    for (name, p) in props {
        doc(&mut s, &inner, p.get("description").and_then(Value::as_str));
        let opt = if required.contains(&name.as_str()) {
            ""
        } else {
            "?"
        };
        let _ = writeln!(s, "{inner}{}{opt}: {};", key(name), ty(p, &inner));
    }
    if let Some(v) = extra {
        let _ = writeln!(s, "{inner}[key: string]: {v};");
    }
    s.push_str(indent);
    s.push('}');
    s
}

fn header() -> String {
    "// Generated by `cargo xtask gen-ts` from the Rust types. Do not edit.\n\
     // Changing a Rust type? Run `cargo xtask gen-ts` (CI checks committed == generated).\n\
     /* eslint-disable */\n\n"
        .to_owned()
}

/// The generated module: shared definitions, the named roots and the action map.
///
/// # Errors
/// Two different types with the same name.
pub fn generate(roots: &[Root<'_>], actions: &[Action<'_>]) -> Result<String> {
    let mut defs = Defs::default();
    for r in roots {
        defs.absorb(r.schema)?;
    }
    let mut rows = Vec::new();
    for a in actions {
        let params = defs.root(a.params, "    ")?;
        let result = match a.result {
            Some(r) => defs.root(r, "    ")?,
            None => "unknown".into(),
        };
        rows.push((a, params, result));
    }
    let mut out = header();
    let root_names: Vec<&str> = roots.iter().map(|r| r.name).collect();
    for (name, def) in &defs.defs {
        if root_names.contains(&name.as_str()) {
            continue;
        }
        doc(&mut out, "", def.get("description").and_then(Value::as_str));
        let _ = writeln!(out, "export type {name} = {};\n", ty(def, ""));
    }
    for r in roots {
        doc(&mut out, "", r.schema.get("description").and_then(Value::as_str));
        let _ = writeln!(out, "export type {} = {};\n", r.name, ty(r.schema, ""));
    }
    out.push_str("/** Every action: parameters, result and tier. */\nexport interface Actions {\n");
    for (a, params, result) in &rows {
        doc(&mut out, "  ", Some(a.description));
        let _ = writeln!(
            out,
            "  {}: {{\n    params: {params};\n    result: {result};\n    tier: {};\n  }};",
            key(a.name),
            literal(&Value::String(a.tier.into()))
        );
    }
    out.push_str("}\n\n/** Action names. */\nexport type ActionName = keyof Actions;\n\n");
    out.push_str("/** Tier of each action, for the front end (the Rust side checks it again). */\n");
    out.push_str("export const ACTION_TIERS = {\n");
    for a in actions {
        let _ = writeln!(
            out,
            "  {}: {},",
            key(a.name),
            literal(&Value::String(a.tier.into()))
        );
    }
    out.push_str("} as const satisfies Record<ActionName, Tier>;\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn primitives_and_nullables() {
        assert_eq!(ty(&json!({"type": "string"}), ""), "string");
        assert_eq!(ty(&json!({"type": ["integer", "null"]}), ""), "number | null");
        assert_eq!(ty(&json!(true), ""), "unknown");
        assert_eq!(ty(&json!({"type": "null"}), ""), "null");
        assert_eq!(ty(&json!({"const": "Grbl"}), ""), "\"Grbl\"");
        assert_eq!(ty(&json!({"enum": ["a", "b"]}), ""), "\"a\" | \"b\"");
    }

    #[test]
    fn arrays_and_tuples() {
        let fixed = json!({"type": "array", "items": {"type": "integer"}, "minItems": 3, "maxItems": 3});
        assert_eq!(ty(&fixed, ""), "[number, number, number]");
        let list = json!({"type": "array", "items": {"anyOf": [{"type": "string"}, {"type": "null"}]}});
        assert_eq!(ty(&list, ""), "(string | null)[]");
        assert_eq!(paren("{\n  a: string | null;\n}"), "{\n  a: string | null;\n}");
        let tuple = json!({"type": "array", "prefixItems": [{"type": "string"}, {"type": "boolean"}]});
        assert_eq!(ty(&tuple, ""), "[string, boolean]");
    }

    #[test]
    fn objects_refs_and_maps() {
        let o = json!({
            "type": "object",
            "properties": {
                "a": {"type": "string", "description": "The a."},
                "b-c": {"$ref": "#/$defs/Thing"}
            },
            "required": ["a"]
        });
        assert_eq!(
            ty(&o, ""),
            "{\n  /** The a. */\n  a: string;\n  \"b-c\"?: Thing;\n}"
        );
        let map = json!({"type": "object", "additionalProperties": {"type": "string"}});
        assert_eq!(ty(&map, ""), "Record<string, string>");
        let empty = json!({"type": "object", "additionalProperties": false});
        assert_eq!(ty(&empty, ""), "Record<string, never>");
    }

    #[test]
    fn externally_tagged_enums() {
        let e = json!({"oneOf": [
            {"const": "ConsoleCleared", "type": "string"},
            {"type": "object", "properties": {"JobStarted": {"type": "object",
                "properties": {"target": {"type": "integer"}}, "required": ["target"]}},
             "required": ["JobStarted"], "additionalProperties": false}
        ]});
        assert_eq!(
            ty(&e, ""),
            "\"ConsoleCleared\" | {\n  JobStarted: {\n    target: number;\n  };\n}"
        );
    }

    #[test]
    fn whole_module() {
        let state = json!({
            "title": "S", "description": "A state.", "type": "object",
            "properties": {"k": {"$ref": "#/$defs/Kind"}}, "required": ["k"],
            "$defs": {"Kind": {"description": "Kind.", "enum": ["x", "y"]}}
        });
        let params = json!({"type": "object", "additionalProperties": false});
        let out = generate(
            &[Root {
                name: "S",
                schema: &state,
            }],
            &[Action {
                name: "state.get",
                tier: "read",
                description: "Reads.",
                params: &params,
                result: Some(&state),
            }],
        )
        .unwrap();
        assert!(
            out.contains("/** Kind. */\nexport type Kind = \"x\" | \"y\";"),
            "{out}"
        );
        assert!(out.contains("export type S = {\n  k: Kind;\n};"), "{out}");
        assert!(
            out.contains("  \"state.get\": {\n    params: Record<string, never>;\n    result: S;"),
            "{out}"
        );
        assert!(out.contains("  \"state.get\": \"read\",\n"), "{out}");
    }

    #[test]
    fn name_clashes_fail() {
        let a = json!({"$defs": {"T": {"type": "string"}}});
        let b = json!({"$defs": {"T": {"type": "integer"}}});
        let r = generate(
            &[
                Root {
                    name: "A",
                    schema: &a,
                },
                Root {
                    name: "B",
                    schema: &b,
                },
            ],
            &[],
        );
        assert!(r.is_err());
    }
}
