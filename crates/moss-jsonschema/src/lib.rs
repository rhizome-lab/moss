//! JSON Schema type generation.
//!
//! Generates type definitions from JSON Schema for multiple languages.
//!
//! # Extensibility
//!
//! Users can register custom generators via [`register()`]:
//!
//! ```ignore
//! use rhizome_moss_jsonschema::{JsonSchemaGenerator, register};
//! use serde_json::Value;
//!
//! struct MyGenerator;
//!
//! impl JsonSchemaGenerator for MyGenerator {
//!     fn language(&self) -> &'static str { "mylang" }
//!     fn generate(&self, schema: &Value, root_name: &str) -> String { /* ... */ }
//! }
//!
//! // Register before first use
//! register(&MyGenerator);
//! ```

use serde_json::Value;
use std::sync::{OnceLock, RwLock};

/// A type generator for a specific language.
pub trait JsonSchemaGenerator: Send + Sync {
    /// Language name (e.g., "typescript", "python", "rust")
    fn language(&self) -> &'static str;

    /// Generate type definitions from JSON Schema.
    fn generate(&self, schema: &Value, root_name: &str) -> String;
}

/// Global registry of generator plugins.
static GENERATORS: RwLock<Vec<&'static dyn JsonSchemaGenerator>> = RwLock::new(Vec::new());
static INITIALIZED: OnceLock<()> = OnceLock::new();

/// Register a custom generator plugin.
///
/// Call this before any generation operations to add custom generators.
/// Built-in generators are registered automatically on first use.
pub fn register(generator: &'static dyn JsonSchemaGenerator) {
    GENERATORS.write().unwrap().push(generator);
}

/// Initialize built-in generators (called automatically on first use).
fn init_builtin() {
    INITIALIZED.get_or_init(|| {
        let mut generators = GENERATORS.write().unwrap();
        static TS: TypeScriptGenerator = TypeScriptGenerator;
        static PY: PythonGenerator = PythonGenerator;
        static RS: RustGenerator = RustGenerator;
        generators.push(&TS);
        generators.push(&PY);
        generators.push(&RS);
    });
}

/// Get a generator by language from the global registry.
pub fn get_generator(lang: &str) -> Option<&'static dyn JsonSchemaGenerator> {
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

/// List all available generator language names from the global registry.
pub fn list_generators() -> Vec<&'static str> {
    init_builtin();
    GENERATORS
        .read()
        .unwrap()
        .iter()
        .map(|g| g.language())
        .collect()
}

// Backwards-compatible aliases
/// Find a generator by language (alias for get_generator, returns Box for compatibility).
pub fn find_generator(lang: &str) -> Option<Box<dyn JsonSchemaGenerator>> {
    get_generator(lang).map(|g| Box::new(GeneratorWrapper(g)) as Box<dyn JsonSchemaGenerator>)
}

struct GeneratorWrapper(&'static dyn JsonSchemaGenerator);

impl JsonSchemaGenerator for GeneratorWrapper {
    fn language(&self) -> &'static str {
        self.0.language()
    }

    fn generate(&self, schema: &Value, root_name: &str) -> String {
        self.0.generate(schema, root_name)
    }
}

/// Registry of available generators (returns boxed generators for compatibility).
pub fn generators() -> Vec<Box<dyn JsonSchemaGenerator>> {
    init_builtin();
    GENERATORS
        .read()
        .unwrap()
        .iter()
        .map(|g| Box::new(GeneratorWrapper(*g)) as Box<dyn JsonSchemaGenerator>)
        .collect()
}

// --- TypeScript ---

struct TypeScriptGenerator;

impl JsonSchemaGenerator for TypeScriptGenerator {
    fn language(&self) -> &'static str {
        "typescript"
    }

    fn generate(&self, schema: &Value, root_name: &str) -> String {
        let mut out = String::new();
        out.push_str("// Auto-generated from JSON Schema\n\n");

        // Handle definitions/$defs first
        if let Some(defs) = schema
            .get("definitions")
            .or_else(|| schema.get("$defs"))
            .and_then(|d| d.as_object())
        {
            for (name, def_schema) in defs {
                out.push_str(&generate_ts_type(name, def_schema, 0));
                out.push('\n');
            }
        }

        // Generate root type
        out.push_str(&generate_ts_type(root_name, schema, 0));
        out
    }
}

fn generate_ts_type(name: &str, schema: &Value, depth: usize) -> String {
    let mut out = String::new();

    // Handle allOf (intersection)
    if let Some(all_of) = schema.get("allOf").and_then(|a| a.as_array()) {
        let types: Vec<String> = all_of.iter().map(|s| schema_to_ts(s)).collect();
        out.push_str(&format!("export type {} = {};\n", name, types.join(" & ")));
        return out;
    }

    // Handle oneOf/anyOf (union)
    if let Some(one_of) = schema
        .get("oneOf")
        .or_else(|| schema.get("anyOf"))
        .and_then(|a| a.as_array())
    {
        let types: Vec<String> = one_of.iter().map(|s| schema_to_ts(s)).collect();
        out.push_str(&format!("export type {} = {};\n", name, types.join(" | ")));
        return out;
    }

    // Handle enum
    if let Some(enum_vals) = schema.get("enum").and_then(|e| e.as_array()) {
        let variants: Vec<String> = enum_vals
            .iter()
            .map(|v| match v {
                Value::String(s) => format!("\"{}\"", s),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => "unknown".to_string(),
            })
            .collect();
        out.push_str(&format!(
            "export type {} = {};\n",
            name,
            variants.join(" | ")
        ));
        return out;
    }

    // Handle object type
    let type_str = schema.get("type").and_then(|t| t.as_str());
    if type_str == Some("object") || schema.get("properties").is_some() {
        out.push_str(&format!("export interface {} {{\n", name));
        if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
            let required: Vec<&str> = schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();

            for (prop_name, prop_schema) in props {
                let ts_type = schema_to_ts(prop_schema);
                let opt = if required.contains(&prop_name.as_str()) {
                    ""
                } else {
                    "?"
                };
                let indent = "  ".repeat(depth + 1);
                out.push_str(&format!("{}{}{}: {};\n", indent, prop_name, opt, ts_type));
            }
        }
        out.push_str("}\n");
        return out;
    }

    // Simple type alias
    let ts_type = schema_to_ts(schema);
    out.push_str(&format!("export type {} = {};\n", name, ts_type));
    out
}

fn schema_to_ts(schema: &Value) -> String {
    // Handle $ref
    if let Some(ref_path) = schema.get("$ref").and_then(|r| r.as_str()) {
        return ref_path.split('/').last().unwrap_or("unknown").to_string();
    }

    // Handle type array (nullable)
    if let Some(arr) = schema.get("type").and_then(|t| t.as_array()) {
        let types: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
        let non_null: Vec<_> = types.iter().filter(|t| **t != "null").collect();
        if non_null.len() == 1 {
            let base = type_to_ts(non_null[0]);
            return format!("{} | null", base);
        }
    }

    let type_str = schema.get("type").and_then(|t| t.as_str());

    // Handle array
    if type_str == Some("array") {
        if let Some(items) = schema.get("items") {
            return format!("{}[]", schema_to_ts(items));
        }
        return "unknown[]".to_string();
    }

    // Handle const
    if let Some(const_val) = schema.get("const") {
        return match const_val {
            Value::String(s) => format!("\"{}\"", s),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => "unknown".to_string(),
        };
    }

    type_str
        .map(type_to_ts)
        .unwrap_or_else(|| "unknown".to_string())
}

fn type_to_ts(t: &str) -> String {
    match t {
        "string" => "string".to_string(),
        "integer" | "number" => "number".to_string(),
        "boolean" => "boolean".to_string(),
        "object" => "Record<string, unknown>".to_string(),
        "null" => "null".to_string(),
        _ => "unknown".to_string(),
    }
}

// --- Python ---

struct PythonGenerator;

impl JsonSchemaGenerator for PythonGenerator {
    fn language(&self) -> &'static str {
        "python"
    }

    fn generate(&self, schema: &Value, root_name: &str) -> String {
        let mut out = String::new();
        out.push_str("# Auto-generated from JSON Schema\n\n");
        out.push_str("from dataclasses import dataclass\n");
        out.push_str("from typing import Any, Literal, Optional, Union\n\n");

        // Handle definitions/$defs first
        if let Some(defs) = schema
            .get("definitions")
            .or_else(|| schema.get("$defs"))
            .and_then(|d| d.as_object())
        {
            for (name, def_schema) in defs {
                out.push_str(&generate_py_type(name, def_schema));
                out.push('\n');
            }
        }

        // Generate root type
        out.push_str(&generate_py_type(root_name, schema));
        out
    }
}

fn generate_py_type(name: &str, schema: &Value) -> String {
    let mut out = String::new();

    // Handle allOf (intersection - use first as base, others as mixins)
    if let Some(all_of) = schema.get("allOf").and_then(|a| a.as_array()) {
        // Python doesn't have intersection types, merge properties
        let types: Vec<String> = all_of.iter().map(|s| schema_to_py(s)).collect();
        out.push_str(&format!("{} = {}\n", name, types.join(" | ")));
        return out;
    }

    // Handle oneOf/anyOf (union)
    if let Some(one_of) = schema
        .get("oneOf")
        .or_else(|| schema.get("anyOf"))
        .and_then(|a| a.as_array())
    {
        let types: Vec<String> = one_of.iter().map(|s| schema_to_py(s)).collect();
        out.push_str(&format!("{} = Union[{}]\n", name, types.join(", ")));
        return out;
    }

    // Handle enum
    if let Some(enum_vals) = schema.get("enum").and_then(|e| e.as_array()) {
        let variants: Vec<String> = enum_vals
            .iter()
            .map(|v| match v {
                Value::String(s) => format!("\"{}\"", s),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
                _ => "Any".to_string(),
            })
            .collect();
        out.push_str(&format!("{} = Literal[{}]\n", name, variants.join(", ")));
        return out;
    }

    // Handle object type
    let type_str = schema.get("type").and_then(|t| t.as_str());
    if type_str == Some("object") || schema.get("properties").is_some() {
        out.push_str("@dataclass\n");
        out.push_str(&format!("class {}:\n", name));
        if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
            let required: Vec<&str> = schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();

            // Required fields first
            for (prop_name, prop_schema) in props {
                if required.contains(&prop_name.as_str()) {
                    let py_type = schema_to_py(prop_schema);
                    out.push_str(&format!("    {}: {}\n", prop_name, py_type));
                }
            }
            // Optional fields
            for (prop_name, prop_schema) in props {
                if !required.contains(&prop_name.as_str()) {
                    let py_type = schema_to_py(prop_schema);
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
        return out;
    }

    // Simple type alias
    let py_type = schema_to_py(schema);
    out.push_str(&format!("{} = {}\n", name, py_type));
    out
}

fn schema_to_py(schema: &Value) -> String {
    // Handle $ref
    if let Some(ref_path) = schema.get("$ref").and_then(|r| r.as_str()) {
        return ref_path.split('/').last().unwrap_or("Any").to_string();
    }

    // Handle type array (nullable)
    if let Some(arr) = schema.get("type").and_then(|t| t.as_array()) {
        let types: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
        let non_null: Vec<_> = types.iter().filter(|t| **t != "null").collect();
        if non_null.len() == 1 {
            let base = type_to_py(non_null[0]);
            return format!("Optional[{}]", base);
        }
    }

    let type_str = schema.get("type").and_then(|t| t.as_str());

    // Handle array
    if type_str == Some("array") {
        if let Some(items) = schema.get("items") {
            return format!("list[{}]", schema_to_py(items));
        }
        return "list".to_string();
    }

    // Handle const
    if let Some(const_val) = schema.get("const") {
        return match const_val {
            Value::String(s) => format!("Literal[\"{}\"]", s),
            Value::Number(n) => format!("Literal[{}]", n),
            Value::Bool(b) => format!("Literal[{}]", if *b { "True" } else { "False" }),
            _ => "Any".to_string(),
        };
    }

    type_str
        .map(type_to_py)
        .unwrap_or_else(|| "Any".to_string())
}

fn type_to_py(t: &str) -> String {
    match t {
        "string" => "str".to_string(),
        "integer" => "int".to_string(),
        "number" => "float".to_string(),
        "boolean" => "bool".to_string(),
        "object" => "dict".to_string(),
        "null" => "None".to_string(),
        _ => "Any".to_string(),
    }
}

// --- Rust ---

struct RustGenerator;

impl JsonSchemaGenerator for RustGenerator {
    fn language(&self) -> &'static str {
        "rust"
    }

    fn generate(&self, schema: &Value, root_name: &str) -> String {
        let mut out = String::new();
        out.push_str("//! Auto-generated from JSON Schema\n\n");
        out.push_str("use serde::{Deserialize, Serialize};\n\n");

        // Handle definitions/$defs first
        if let Some(defs) = schema
            .get("definitions")
            .or_else(|| schema.get("$defs"))
            .and_then(|d| d.as_object())
        {
            for (name, def_schema) in defs {
                out.push_str(&generate_rust_type(name, def_schema));
                out.push('\n');
            }
        }

        // Generate root type
        out.push_str(&generate_rust_type(root_name, schema));
        out
    }
}

fn generate_rust_type(name: &str, schema: &Value) -> String {
    let mut out = String::new();

    // Handle enum with string values
    if let Some(enum_vals) = schema.get("enum").and_then(|e| e.as_array()) {
        let all_strings = enum_vals.iter().all(|v| v.is_string());
        if all_strings {
            out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
            out.push_str(&format!("pub enum {} {{\n", name));
            for val in enum_vals {
                if let Some(s) = val.as_str() {
                    let variant = to_pascal_case(s);
                    if variant != s {
                        out.push_str(&format!("    #[serde(rename = \"{}\")]\n", s));
                    }
                    out.push_str(&format!("    {},\n", variant));
                }
            }
            out.push_str("}\n");
            return out;
        }
    }

    // Handle oneOf/anyOf (tagged union)
    if let Some(one_of) = schema
        .get("oneOf")
        .or_else(|| schema.get("anyOf"))
        .and_then(|a| a.as_array())
    {
        out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
        out.push_str("#[serde(untagged)]\n");
        out.push_str(&format!("pub enum {} {{\n", name));
        for (i, variant_schema) in one_of.iter().enumerate() {
            let variant_type = schema_to_rust(variant_schema);
            out.push_str(&format!("    Variant{}({}),\n", i, variant_type));
        }
        out.push_str("}\n");
        return out;
    }

    // Handle object type
    let type_str = schema.get("type").and_then(|t| t.as_str());
    if type_str == Some("object") || schema.get("properties").is_some() {
        out.push_str("#[derive(Debug, Clone, Serialize, Deserialize)]\n");
        out.push_str(&format!("pub struct {} {{\n", name));
        if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
            let required: Vec<&str> = schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();

            for (prop_name, prop_schema) in props {
                let rust_type = schema_to_rust(prop_schema);
                let field_name = to_snake_case(prop_name);
                let field_type = if required.contains(&prop_name.as_str()) {
                    rust_type
                } else {
                    format!("Option<{}>", rust_type)
                };
                if field_name != *prop_name {
                    out.push_str(&format!("    #[serde(rename = \"{}\")]\n", prop_name));
                }
                out.push_str(&format!("    pub {}: {},\n", field_name, field_type));
            }
        }
        out.push_str("}\n");
        return out;
    }

    // Simple type alias
    let rust_type = schema_to_rust(schema);
    out.push_str(&format!("pub type {} = {};\n", name, rust_type));
    out
}

fn schema_to_rust(schema: &Value) -> String {
    // Handle $ref
    if let Some(ref_path) = schema.get("$ref").and_then(|r| r.as_str()) {
        return ref_path
            .split('/')
            .last()
            .unwrap_or("serde_json::Value")
            .to_string();
    }

    // Handle type array (nullable)
    if let Some(arr) = schema.get("type").and_then(|t| t.as_array()) {
        let types: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
        let non_null: Vec<_> = types.iter().filter(|t| **t != "null").collect();
        if non_null.len() == 1 {
            let base = type_to_rust(non_null[0]);
            return format!("Option<{}>", base);
        }
    }

    let type_str = schema.get("type").and_then(|t| t.as_str());

    // Handle array
    if type_str == Some("array") {
        if let Some(items) = schema.get("items") {
            return format!("Vec<{}>", schema_to_rust(items));
        }
        return "Vec<serde_json::Value>".to_string();
    }

    // Handle const
    if schema.get("const").is_some() {
        // Rust doesn't have const types, use the base type
        return "serde_json::Value".to_string();
    }

    type_str
        .map(type_to_rust)
        .unwrap_or_else(|| "serde_json::Value".to_string())
}

fn type_to_rust(t: &str) -> String {
    match t {
        "string" => "String".to_string(),
        "integer" => "i64".to_string(),
        "number" => "f64".to_string(),
        "boolean" => "bool".to_string(),
        "object" => "serde_json::Value".to_string(),
        "null" => "()".to_string(),
        _ => "serde_json::Value".to_string(),
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
        } else if c == '-' {
            result.push('_');
        } else {
            result.push(c);
        }
    }
    result
}

fn to_pascal_case(s: &str) -> String {
    s.split(|c| c == '_' || c == '-' || c == ' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
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
    fn test_simple_object_ts() {
        let schema: Value = serde_json::from_str(
            r#"{
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            },
            "required": ["name"]
        }"#,
        )
        .unwrap();

        let output = TypeScriptGenerator.generate(&schema, "Person");
        assert!(output.contains("export interface Person"));
        assert!(output.contains("name: string"));
        assert!(output.contains("age?: number"));
    }

    #[test]
    fn test_enum_ts() {
        let schema: Value = serde_json::from_str(
            r#"{
            "enum": ["red", "green", "blue"]
        }"#,
        )
        .unwrap();

        let output = TypeScriptGenerator.generate(&schema, "Color");
        assert!(output.contains("export type Color = \"red\" | \"green\" | \"blue\""));
    }
}
