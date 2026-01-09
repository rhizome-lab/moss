//! OpenAPI client code generation.
//!
//! Trait-based design allows multiple implementations per language/framework.
//!
//! # Extensibility
//!
//! Users can register custom generators via [`register()`]:
//!
//! ```ignore
//! use rhizome_moss_openapi::{OpenApiClientGenerator, register};
//! use serde_json::Value;
//!
//! struct MyGenerator;
//!
//! impl OpenApiClientGenerator for MyGenerator {
//!     fn language(&self) -> &'static str { "mylang" }
//!     fn variant(&self) -> &'static str { "myvariant" }
//!     fn generate(&self, spec: &Value) -> String { /* ... */ }
//! }
//!
//! // Register before first use
//! register(&MyGenerator);
//! ```

use serde_json::Value;
use std::sync::{OnceLock, RwLock};

/// A code generator for a specific language/framework.
pub trait OpenApiClientGenerator: Send + Sync {
    /// Language name (e.g., "typescript", "python")
    fn language(&self) -> &'static str;

    /// Framework/variant name (e.g., "fetch", "axios", "urllib")
    fn variant(&self) -> &'static str;

    /// Generate client code from OpenAPI JSON.
    fn generate(&self, spec: &Value) -> String;
}

/// Global registry of generator plugins.
static GENERATORS: RwLock<Vec<&'static dyn OpenApiClientGenerator>> = RwLock::new(Vec::new());
static INITIALIZED: OnceLock<()> = OnceLock::new();

/// Register a custom generator plugin.
///
/// Call this before any generation operations to add custom generators.
/// Built-in generators are registered automatically on first use.
pub fn register(generator: &'static dyn OpenApiClientGenerator) {
    GENERATORS.write().unwrap().push(generator);
}

/// Initialize built-in generators (called automatically on first use).
fn init_builtin() {
    INITIALIZED.get_or_init(|| {
        let mut generators = GENERATORS.write().unwrap();
        static TS: TypeScriptFetch = TypeScriptFetch;
        static PY: PythonUrllib = PythonUrllib;
        static RS: RustUreq = RustUreq;
        generators.push(&TS);
        generators.push(&PY);
        generators.push(&RS);
    });
}

/// Get a generator by language from the global registry (returns first match).
pub fn get_generator(lang: &str) -> Option<&'static dyn OpenApiClientGenerator> {
    init_builtin();
    let lang_lower = lang.to_lowercase();
    GENERATORS
        .read()
        .unwrap()
        .iter()
        .find(|g| {
            g.language() == lang_lower
                || (lang_lower == "ts" && g.language() == "typescript")
                || (lang_lower == "py" && g.language() == "python")
                || (lang_lower == "rs" && g.language() == "rust")
        })
        .copied()
}

/// List all available generators (language, variant) from the global registry.
pub fn list_generators() -> Vec<(&'static str, &'static str)> {
    init_builtin();
    GENERATORS
        .read()
        .unwrap()
        .iter()
        .map(|g| (g.language(), g.variant()))
        .collect()
}

// Backwards-compatible aliases
/// Find a generator by language (alias for get_generator, returns Box for compatibility).
pub fn find_generator(lang: &str) -> Option<Box<dyn OpenApiClientGenerator>> {
    get_generator(lang).map(|g| Box::new(GeneratorWrapper(g)) as Box<dyn OpenApiClientGenerator>)
}

struct GeneratorWrapper(&'static dyn OpenApiClientGenerator);

impl OpenApiClientGenerator for GeneratorWrapper {
    fn language(&self) -> &'static str {
        self.0.language()
    }

    fn variant(&self) -> &'static str {
        self.0.variant()
    }

    fn generate(&self, spec: &Value) -> String {
        self.0.generate(spec)
    }
}

/// Registry of available generators (returns boxed generators for compatibility).
pub fn generators() -> Vec<Box<dyn OpenApiClientGenerator>> {
    init_builtin();
    GENERATORS
        .read()
        .unwrap()
        .iter()
        .map(|g| Box::new(GeneratorWrapper(*g)) as Box<dyn OpenApiClientGenerator>)
        .collect()
}

// --- TypeScript (fetch) ---

struct TypeScriptFetch;

impl OpenApiClientGenerator for TypeScriptFetch {
    fn language(&self) -> &'static str {
        "typescript"
    }
    fn variant(&self) -> &'static str {
        "fetch"
    }

    fn generate(&self, spec: &Value) -> String {
        let mut out = String::new();
        out.push_str("// Auto-generated from OpenAPI spec\n");
        out.push_str("// Uses fetch (built-in)\n\n");

        // Generate interfaces from schemas
        if let Some(schemas) = spec
            .pointer("/components/schemas")
            .and_then(|s| s.as_object())
        {
            for (name, schema) in schemas {
                out.push_str(&format!("export interface {} {{\n", name));
                if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                    let required: Vec<&str> = schema
                        .get("required")
                        .and_then(|r| r.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                        .unwrap_or_default();
                    for (prop_name, prop) in props {
                        let ts_type = json_schema_to_ts(prop);
                        let opt = if required.contains(&prop_name.as_str()) {
                            ""
                        } else {
                            "?"
                        };
                        out.push_str(&format!("  {}{}: {};\n", prop_name, opt, ts_type));
                    }
                }
                out.push_str("}\n\n");
            }
        }

        // Generate client class
        out.push_str("export class ApiClient {\n");
        out.push_str("  constructor(private baseUrl = 'http://localhost:8080') {}\n\n");
        out.push_str("  private async request<T>(path: string, params?: Record<string, string | number | undefined>): Promise<T> {\n");
        out.push_str("    const url = new URL(path, this.baseUrl);\n");
        out.push_str("    if (params) {\n");
        out.push_str("      for (const [k, v] of Object.entries(params)) {\n");
        out.push_str("        if (v !== undefined) url.searchParams.set(k, String(v));\n");
        out.push_str("      }\n");
        out.push_str("    }\n");
        out.push_str("    const res = await fetch(url.toString());\n");
        out.push_str("    if (!res.ok) throw new Error(`HTTP ${res.status}`);\n");
        out.push_str("    return await res.json() as T;\n");
        out.push_str("  }\n\n");

        // Generate methods from paths
        if let Some(paths) = spec.get("paths").and_then(|p| p.as_object()) {
            for (path, methods) in paths {
                if let Some(op) = methods.get("get").and_then(|g| g.as_object()) {
                    let op_id = op
                        .get("operationId")
                        .and_then(|id| id.as_str())
                        .unwrap_or("unknown");
                    let params = op
                        .get("parameters")
                        .and_then(|p| p.as_array())
                        .map(|a| a.as_slice())
                        .unwrap_or(&[]);

                    let path_params: Vec<&str> = params
                        .iter()
                        .filter(|p| p.get("in").and_then(|i| i.as_str()) == Some("path"))
                        .filter_map(|p| p.get("name").and_then(|n| n.as_str()))
                        .collect();
                    let query_params: Vec<&str> = params
                        .iter()
                        .filter(|p| p.get("in").and_then(|i| i.as_str()) == Some("query"))
                        .filter_map(|p| p.get("name").and_then(|n| n.as_str()))
                        .collect();

                    // Response type from nested path
                    let op_value = Value::Object(op.clone());
                    let resp_type = op_value
                        .pointer("/responses/200/content/application~1json/schema")
                        .map(json_schema_to_ts)
                        .unwrap_or_else(|| "void".to_string());

                    let mut args = Vec::new();
                    for p in &path_params {
                        args.push(format!("{}: string", p));
                    }
                    if !query_params.is_empty() {
                        let opts: Vec<String> = query_params
                            .iter()
                            .map(|p| format!("{}?: string | number", p))
                            .collect();
                        args.push(format!("options?: {{ {} }}", opts.join("; ")));
                    }

                    let url_template = path.replace('{', "${");
                    let call_params = if query_params.is_empty() {
                        ""
                    } else {
                        ", options"
                    };

                    out.push_str(&format!(
                        "  async {}({}): Promise<{}> {{\n",
                        op_id,
                        args.join(", "),
                        resp_type
                    ));
                    out.push_str(&format!(
                        "    return this.request<{}>(`{}`{});\n",
                        resp_type, url_template, call_params
                    ));
                    out.push_str("  }\n\n");
                }
            }
        }

        out.push_str("}\n");
        out
    }
}

// --- Python (urllib) ---

struct PythonUrllib;

impl OpenApiClientGenerator for PythonUrllib {
    fn language(&self) -> &'static str {
        "python"
    }
    fn variant(&self) -> &'static str {
        "urllib"
    }

    fn generate(&self, spec: &Value) -> String {
        let mut out = String::new();
        out.push_str("# Auto-generated from OpenAPI spec\n");
        out.push_str("# Uses urllib (stdlib)\n\n");
        out.push_str("from dataclasses import dataclass\n");
        out.push_str("from typing import Any, Optional\n");
        out.push_str("from urllib.parse import urlencode\n");
        out.push_str("from urllib.request import urlopen\n");
        out.push_str("import json\n\n\n");

        // Generate dataclasses from schemas
        if let Some(schemas) = spec
            .pointer("/components/schemas")
            .and_then(|s| s.as_object())
        {
            for (name, schema) in schemas {
                out.push_str("@dataclass\n");
                out.push_str(&format!("class {}:\n", name));
                if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                    let required: Vec<&str> = schema
                        .get("required")
                        .and_then(|r| r.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                        .unwrap_or_default();

                    // Required fields first
                    for (prop_name, prop) in props {
                        if required.contains(&prop_name.as_str()) {
                            let py_type = json_schema_to_py(prop);
                            out.push_str(&format!("    {}: {}\n", prop_name, py_type));
                        }
                    }
                    // Optional fields
                    for (prop_name, prop) in props {
                        if !required.contains(&prop_name.as_str()) {
                            let py_type = json_schema_to_py(prop);
                            out.push_str(&format!(
                                "    {}: Optional[{}] = None\n",
                                prop_name, py_type
                            ));
                        }
                    }
                    if props.is_empty() {
                        out.push_str("    pass\n");
                    }
                } else {
                    out.push_str("    pass\n");
                }
                out.push_str("\n\n");
            }
        }

        // Generate client class
        out.push_str("class ApiClient:\n");
        out.push_str("    def __init__(self, base_url: str = 'http://localhost:8080'):\n");
        out.push_str("        self.base_url = base_url.rstrip('/')\n\n");
        out.push_str("    def _request(self, path: str, params: Optional[dict] = None) -> dict:\n");
        out.push_str("        url = f'{self.base_url}{path}'\n");
        out.push_str("        if params:\n");
        out.push_str("            filtered = {k: v for k, v in params.items() if v is not None}\n");
        out.push_str("            if filtered:\n");
        out.push_str("                url = f'{url}?{urlencode(filtered)}'\n");
        out.push_str("        with urlopen(url) as response:\n");
        out.push_str("            return json.load(response)\n\n");

        // Generate methods from paths
        if let Some(paths) = spec.get("paths").and_then(|p| p.as_object()) {
            for (path, methods) in paths {
                if let Some(op) = methods.get("get").and_then(|g| g.as_object()) {
                    let op_id = op
                        .get("operationId")
                        .and_then(|id| id.as_str())
                        .unwrap_or("unknown");
                    let params = op
                        .get("parameters")
                        .and_then(|p| p.as_array())
                        .map(|a| a.as_slice())
                        .unwrap_or(&[]);

                    let path_params: Vec<&str> = params
                        .iter()
                        .filter(|p| p.get("in").and_then(|i| i.as_str()) == Some("path"))
                        .filter_map(|p| p.get("name").and_then(|n| n.as_str()))
                        .collect();
                    let query_params: Vec<&str> = params
                        .iter()
                        .filter(|p| p.get("in").and_then(|i| i.as_str()) == Some("query"))
                        .filter_map(|p| p.get("name").and_then(|n| n.as_str()))
                        .collect();

                    let op_value = Value::Object(op.clone());
                    let resp_type = op_value
                        .pointer("/responses/200/content/application~1json/schema")
                        .map(json_schema_to_py)
                        .unwrap_or_else(|| "dict".to_string());

                    let mut args = vec!["self".to_string()];
                    for p in &path_params {
                        args.push(format!("{}: str", p));
                    }
                    if !query_params.is_empty() {
                        args.push("*".to_string());
                        for p in &query_params {
                            args.push(format!("{}: Optional[str] = None", p));
                        }
                    }

                    let url_template = path.replace('{', "{");
                    let params_dict = if query_params.is_empty() {
                        String::new()
                    } else {
                        let kv: Vec<_> = query_params
                            .iter()
                            .map(|p| format!("'{}': {}", p, p))
                            .collect();
                        format!(", {{{}}}", kv.join(", "))
                    };

                    out.push_str(&format!(
                        "    def {}({}) -> {}:\n",
                        op_id,
                        args.join(", "),
                        resp_type
                    ));
                    out.push_str(&format!(
                        "        data = self._request(f'{}'{})\n",
                        url_template, params_dict
                    ));
                    out.push_str(&format!("        return {}(**data)\n\n", resp_type));
                }
            }
        }

        out
    }
}

// --- Rust (ureq) ---

struct RustUreq;

impl OpenApiClientGenerator for RustUreq {
    fn language(&self) -> &'static str {
        "rust"
    }
    fn variant(&self) -> &'static str {
        "ureq"
    }

    fn generate(&self, spec: &Value) -> String {
        let mut out = String::new();
        out.push_str("//! Auto-generated from OpenAPI spec\n");
        out.push_str("//! Uses ureq (blocking HTTP)\n\n");
        out.push_str("use serde::{Deserialize, Serialize};\n\n");

        // Generate structs from schemas
        if let Some(schemas) = spec
            .pointer("/components/schemas")
            .and_then(|s| s.as_object())
        {
            for (name, schema) in schemas {
                out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
                out.push_str(&format!("pub struct {} {{\n", name));
                if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                    let required: Vec<&str> = schema
                        .get("required")
                        .and_then(|r| r.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                        .unwrap_or_default();
                    for (prop_name, prop) in props {
                        let rust_type = json_schema_to_rust(prop);
                        let field_type = if required.contains(&prop_name.as_str()) {
                            rust_type
                        } else {
                            format!("Option<{}>", rust_type)
                        };
                        out.push_str(&format!(
                            "    pub {}: {},\n",
                            to_snake_case(prop_name),
                            field_type
                        ));
                    }
                }
                out.push_str("}\n\n");
            }
        }

        // Generate client struct
        out.push_str("pub struct ApiClient {\n");
        out.push_str("    base_url: String,\n");
        out.push_str("}\n\n");

        out.push_str("impl ApiClient {\n");
        out.push_str("    pub fn new(base_url: impl Into<String>) -> Self {\n");
        out.push_str("        Self { base_url: base_url.into() }\n");
        out.push_str("    }\n\n");

        // Generate methods from paths
        if let Some(paths) = spec.get("paths").and_then(|p| p.as_object()) {
            for (path, methods) in paths {
                if let Some(op) = methods.get("get").and_then(|g| g.as_object()) {
                    let op_id = op
                        .get("operationId")
                        .and_then(|id| id.as_str())
                        .unwrap_or("unknown");
                    let params = op
                        .get("parameters")
                        .and_then(|p| p.as_array())
                        .map(|a| a.as_slice())
                        .unwrap_or(&[]);

                    let path_params: Vec<&str> = params
                        .iter()
                        .filter(|p| p.get("in").and_then(|i| i.as_str()) == Some("path"))
                        .filter_map(|p| p.get("name").and_then(|n| n.as_str()))
                        .collect();
                    let query_params: Vec<(&str, bool)> = params
                        .iter()
                        .filter(|p| p.get("in").and_then(|i| i.as_str()) == Some("query"))
                        .filter_map(|p| {
                            let name = p.get("name").and_then(|n| n.as_str())?;
                            let required =
                                p.get("required").and_then(|r| r.as_bool()).unwrap_or(false);
                            Some((name, required))
                        })
                        .collect();

                    let op_value = Value::Object(op.clone());
                    let resp_type = op_value
                        .pointer("/responses/200/content/application~1json/schema")
                        .map(json_schema_to_rust)
                        .unwrap_or_else(|| "()".to_string());

                    // Build function signature
                    let mut args = Vec::new();
                    args.push("&self".to_string());
                    for p in &path_params {
                        args.push(format!("{}: &str", to_snake_case(p)));
                    }
                    for (p, required) in &query_params {
                        let param_type = if *required {
                            "&str".to_string()
                        } else {
                            "Option<&str>".to_string()
                        };
                        args.push(format!("{}: {}", to_snake_case(p), param_type));
                    }

                    out.push_str(&format!(
                        "    pub fn {}({}) -> Result<{}, ureq::Error> {{\n",
                        to_snake_case(op_id),
                        args.join(", "),
                        resp_type
                    ));

                    // Build URL with path params
                    let url_expr = if path_params.is_empty() {
                        format!("format!(\"{{}}{}\"", path)
                    } else {
                        let rust_path = path_params.iter().fold(path.to_string(), |acc, p| {
                            acc.replace(&format!("{{{}}}", p), &format!("{{{}}}", to_snake_case(p)))
                        });
                        format!("format!(\"{{}}{}\", ", rust_path)
                    };
                    out.push_str(&format!("        let url = {}self.base_url);\n", url_expr));

                    // Build request
                    out.push_str("        let mut req = ureq::get(&url);\n");
                    for (p, required) in &query_params {
                        let snake = to_snake_case(p);
                        if *required {
                            out.push_str(&format!(
                                "        req = req.query(\"{}\", {});\n",
                                p, snake
                            ));
                        } else {
                            out.push_str(&format!(
                                "        if let Some(v) = {} {{ req = req.query(\"{}\", v); }}\n",
                                snake, p
                            ));
                        }
                    }

                    out.push_str("        let resp: ");
                    out.push_str(&resp_type);
                    out.push_str(" = req.call()?.into_json()?;\n");
                    out.push_str("        Ok(resp)\n");
                    out.push_str("    }\n\n");
                }
            }
        }

        out.push_str("}\n");
        out
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

fn json_schema_to_rust(schema: &Value) -> String {
    if let Some(ref_path) = schema.get("$ref").and_then(|r| r.as_str()) {
        return ref_path
            .split('/')
            .last()
            .unwrap_or("serde_json::Value")
            .to_string();
    }

    let type_val = schema.get("type");

    if let Some(arr) = type_val.and_then(|t| t.as_array()) {
        let types: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
        let non_null: Vec<_> = types.iter().filter(|t| **t != "null").collect();
        if non_null.len() == 1 {
            let base = type_str_to_rust(non_null[0]);
            return format!("Option<{}>", base);
        }
    }

    if let Some(type_str) = type_val.and_then(|t| t.as_str()) {
        if type_str == "array" {
            if let Some(items) = schema.get("items") {
                return format!("Vec<{}>", json_schema_to_rust(items));
            }
            return "Vec<serde_json::Value>".to_string();
        }
        return type_str_to_rust(type_str);
    }

    "serde_json::Value".to_string()
}

fn type_str_to_rust(t: &str) -> String {
    match t {
        "string" => "String".to_string(),
        "integer" => "i64".to_string(),
        "number" => "f64".to_string(),
        "boolean" => "bool".to_string(),
        "object" => "serde_json::Value".to_string(),
        _ => "serde_json::Value".to_string(),
    }
}

// --- Helpers ---

fn json_schema_to_ts(schema: &Value) -> String {
    if let Some(ref_path) = schema.get("$ref").and_then(|r| r.as_str()) {
        return ref_path.split('/').last().unwrap_or("unknown").to_string();
    }

    let type_val = schema.get("type");

    if let Some(arr) = type_val.and_then(|t| t.as_array()) {
        let types: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
        let non_null: Vec<_> = types.iter().filter(|t| **t != "null").collect();
        if non_null.len() == 1 {
            let base = type_str_to_ts(non_null[0]);
            return format!("{} | null", base);
        }
    }

    if let Some(type_str) = type_val.and_then(|t| t.as_str()) {
        if type_str == "array" {
            if let Some(items) = schema.get("items") {
                return format!("{}[]", json_schema_to_ts(items));
            }
            return "unknown[]".to_string();
        }
        return type_str_to_ts(type_str);
    }

    "unknown".to_string()
}

fn type_str_to_ts(t: &str) -> String {
    match t {
        "string" => "string".to_string(),
        "integer" | "number" => "number".to_string(),
        "boolean" => "boolean".to_string(),
        "object" => "Record<string, unknown>".to_string(),
        _ => "unknown".to_string(),
    }
}

fn json_schema_to_py(schema: &Value) -> String {
    if let Some(ref_path) = schema.get("$ref").and_then(|r| r.as_str()) {
        return ref_path.split('/').last().unwrap_or("Any").to_string();
    }

    let type_val = schema.get("type");

    if let Some(arr) = type_val.and_then(|t| t.as_array()) {
        let types: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
        let non_null: Vec<_> = types.iter().filter(|t| **t != "null").collect();
        if non_null.len() == 1 {
            let base = type_str_to_py(non_null[0]);
            return format!("Optional[{}]", base);
        }
    }

    if let Some(type_str) = type_val.and_then(|t| t.as_str()) {
        if type_str == "array" {
            if let Some(items) = schema.get("items") {
                return format!("list[{}]", json_schema_to_py(items));
            }
            return "list".to_string();
        }
        return type_str_to_py(type_str);
    }

    "Any".to_string()
}

fn type_str_to_py(t: &str) -> String {
    match t {
        "string" => "str".to_string(),
        "integer" => "int".to_string(),
        "number" => "float".to_string(),
        "boolean" => "bool".to_string(),
        "object" => "dict".to_string(),
        _ => "Any".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_generator() {
        assert!(find_generator("typescript").is_some());
        assert!(find_generator("ts").is_some());
        assert!(find_generator("python").is_some());
        assert!(find_generator("py").is_some());
        assert!(find_generator("rust").is_some());
        assert!(find_generator("rs").is_some());
        assert!(find_generator("unknown").is_none());
    }

    #[test]
    fn test_list_generators() {
        let gens = list_generators();
        assert!(gens.iter().any(|(l, _)| *l == "typescript"));
        assert!(gens.iter().any(|(l, _)| *l == "python"));
        assert!(gens.iter().any(|(l, _)| *l == "rust"));
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("getUserById"), "get_user_by_id");
        assert_eq!(to_snake_case("API"), "a_p_i");
        assert_eq!(to_snake_case("simple"), "simple");
    }
}
