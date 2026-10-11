//! `api.d.ts`: TypeScript declarations of the script API, generated from the
//! registry (names, parameters from the JSON schemas, descriptions, tiers): one
//! interface per namespace, so action names that are reserved words (`delete`,
//! `new`) stay valid methods. It is
//! committed (`doc/scripting/api.d.ts`) and checked in CI, published as an MCP
//! resource and used for completion in the macro editor (M4).

use std::collections::BTreeMap;
use std::fmt::Write;

use freekerf_session::ActionSpec;
use serde_json::Value;

use crate::camel;

fn resolve<'a>(schema: &'a Value, root: &'a Value) -> &'a Value {
    let mut s = schema;
    for _ in 0..16 {
        let Some(r) = s.get("$ref").and_then(Value::as_str) else {
            break;
        };
        let name = r.rsplit('/').next().unwrap_or("");
        let defs = root.get("$defs").or_else(|| root.get("definitions"));
        match defs.and_then(|d| d.get(name)) {
            Some(next) => s = next,
            None => break,
        }
    }
    s
}

/// The TypeScript type of a JSON schema (depth-limited; `unknown` beyond it).
fn ts_type(schema: &Value, root: &Value, depth: usize) -> String {
    if depth > 6 {
        return "unknown".into();
    }
    let s = resolve(schema, root);
    if let Some(values) = s.get("enum").and_then(Value::as_array) {
        let parts: Vec<String> = values.iter().map(ToString::to_string).collect();
        return parts.join(" | ");
    }
    if let Some(c) = s.get("const") {
        return c.to_string();
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(options) = s.get(key).and_then(Value::as_array) {
            let mut parts: Vec<String> = options.iter().map(|o| ts_type(o, root, depth + 1)).collect();
            parts.dedup();
            return parts.join(" | ");
        }
    }
    if let Some(all) = s.get("allOf").and_then(Value::as_array) {
        let parts: Vec<String> = all.iter().map(|o| ts_type(o, root, depth + 1)).collect();
        return parts.join(" & ");
    }
    let one = |t: &str| -> String {
        match t {
            "string" => "string".into(),
            "integer" | "number" => "number".into(),
            "boolean" => "boolean".into(),
            "null" => "null".into(),
            "array" => {
                let item = s
                    .get("items")
                    .map_or_else(|| "unknown".into(), |i| ts_type(i, root, depth + 1));
                if item.contains(' ') {
                    format!("({item})[]")
                } else {
                    format!("{item}[]")
                }
            }
            "object" => object_type(s, root, depth),
            _ => "unknown".into(),
        }
    };
    match s.get("type") {
        Some(Value::String(t)) => one(t),
        Some(Value::Array(ts)) => {
            let parts: Vec<String> = ts.iter().filter_map(Value::as_str).map(one).collect();
            parts.join(" | ")
        }
        _ if s.get("properties").is_some() => object_type(s, root, depth),
        _ => "unknown".into(),
    }
}

fn object_type(s: &Value, root: &Value, depth: usize) -> String {
    let Some(props) = s.get("properties").and_then(Value::as_object) else {
        return "Record<string, unknown>".into();
    };
    if props.is_empty() {
        return "Record<string, never>".into();
    }
    let required: Vec<&str> = s
        .get("required")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let fields: Vec<String> = props
        .iter()
        .map(|(k, v)| {
            let opt = if required.contains(&k.as_str()) { "" } else { "?" };
            format!("{k}{opt}: {}", ts_type(v, root, depth + 1))
        })
        .collect();
    format!("{{ {} }}", fields.join("; "))
}

fn comment(text: &str, indent: &str) -> String {
    let mut out = format!("{indent}/**\n");
    for line in text.lines() {
        if line.is_empty() {
            let _ = writeln!(out, "{indent} *");
        } else {
            let _ = writeln!(out, "{indent} * {}", line.replace("*/", "*\\/"));
        }
    }
    out.push_str(indent);
    out.push_str(" */\n");
    out
}

fn pascal(name: &str) -> String {
    let c = camel(name);
    let mut chars = c.chars();
    chars
        .next()
        .map(|f| f.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn params_decl(spec: &ActionSpec) -> String {
    let params = ts_type(&spec.params, &spec.params, 0);
    let optional = spec
        .params
        .get("required")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty);
    format!("params{}: {params}", if optional { "?" } else { "" })
}

/// How a script calls one action: `app.document.translate(params: {...}): unknown`
/// (`search_api` of the MCP server).
#[must_use]
pub fn signature(spec: &ActionSpec) -> String {
    let (ns, name) = spec.name.split_once('.').unwrap_or(("", spec.name.as_str()));
    format!("app.{ns}.{}({}): unknown", camel(name), params_decl(spec))
}

/// The declarations of `specs`.
#[must_use]
pub fn api_dts(specs: &[ActionSpec]) -> String {
    let mut by_ns: BTreeMap<&str, Vec<&ActionSpec>> = BTreeMap::new();
    for s in specs {
        if let Some((ns, _)) = s.name.split_once('.') {
            by_ns.entry(ns).or_default().push(s);
        }
    }
    let mut out = String::from(
        "// FreeKerf script API. Generated from the action registry by `cargo xtask gen-docs`. Do not edit.\n\
         // Every method dispatches one action; the caller's tier is checked on every call.\n",
    );
    let mut members = Vec::new();
    for (ns, actions) in by_ns {
        let iface = format!("App{}", pascal(ns));
        let _ = writeln!(out, "\ninterface {iface} {{");
        for s in actions {
            let name = s.name.split_once('.').map_or("", |(_, n)| n);
            let doc = format!("{}\n\nAction `{}`, tier `{}`.", s.description, s.name, s.tier);
            out.push_str(&comment(&doc, "  "));
            // `new(...)` in an interface is a construct signature: quote it
            let method = camel(name);
            let method = if method == "new" { "\"new\"".into() } else { method };
            let _ = writeln!(out, "  {method}({}): unknown;", params_decl(s));
        }
        out.push_str("}\n");
        members.push(format!("  readonly {ns}: {iface};"));
    }
    let _ = write!(
        out,
        "\n/** The application: one namespace per action prefix. */\ndeclare const app: {{\n{}\n}};\n",
        members.join("\n")
    );
    out.push_str(
        "\n/** Writes a line to the run's log (the app console). */\ndeclare const console: { log(...values: unknown[]): void };\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use freekerf_session::Tier;
    use serde_json::json;

    use super::*;

    #[test]
    fn declarations() {
        let spec = |name: &str, params: Value| ActionSpec {
            name: name.into(),
            description: "Does it.\nSecond line */ end.".into(),
            tier: Tier::Edit,
            params,
        };
        let specs = [
            spec(
                "document.select",
                json!({"type": "object", "required": ["ids"], "properties": {
                    "ids": {"type": "array", "items": {"type": "integer"}},
                    "mode": {"$ref": "#/$defs/Mode"},
                    "both": {"type": ["string", "null"]},
                    "any": {"anyOf": [{"type": "string"}, {"type": "number"}]},
                    "all": {"allOf": [{"type": "object", "properties": {"a": {"const": 1}}}]},
                    "list": {"type": "array", "items": {"enum": ["x", "y"]}},
                    "free": {}
                }, "$defs": {"Mode": {"enum": ["replace", "add"]}}}),
            ),
            spec("view.get", json!({"type": "object", "properties": {}})),
            spec("session.snapshot", json!({"type": "object"})),
            spec("bad", json!({})),
            spec("document.new", json!({"type": "object"})),
        ];
        let d = api_dts(&specs);
        assert!(d.contains("interface AppDocument {"), "{d}");
        assert!(d.contains("  readonly document: AppDocument;"));
        assert!(d.contains("  select(params: {"), "{d}");
        assert!(d.contains("ids: number[]"));
        assert!(d.contains("mode?: \"replace\" | \"add\""));
        assert!(d.contains("  get(params?: Record<string, never>): unknown;"));
        assert!(d.contains("  snapshot(params?: Record<string, unknown>): unknown;"));
        assert!(!d.contains(" * \n"));
        assert_eq!(pascal("job_queue"), "JobQueue");
        assert_eq!(pascal(""), "");
        assert!(d.contains("*\\/ end."));
        assert!(!d.contains("bad"));
        assert_eq!(
            signature(&specs[1]),
            "app.view.get(params?: Record<string, never>): unknown"
        );
        assert!(signature(&specs[0]).starts_with("app.document.select(params: {"));
        assert!(d.contains("  \"new\"(params?: Record<string, unknown>): unknown;"));
        // depth limit
        let mut deep = json!({"type": "string"});
        for _ in 0..10 {
            deep = json!({"type": "array", "items": deep});
        }
        assert!(ts_type(&deep, &deep, 0).contains("unknown"));
        let r = json!({"$ref": "#/$defs/Missing"});
        assert_eq!(ts_type(&r, &r, 0), "unknown");
    }
}
