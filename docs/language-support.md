# Language Support

Skeleton extraction support status. Arborium provides 70+ tree-sitter grammars.

## Currently Supported (20)

| Language | Skeleton | Notes |
|----------|----------|-------|
| Python | ✅ | class, function, async, docstrings |
| Rust | ✅ | struct, enum, trait, impl, fn |
| JavaScript | ✅ | class, function, method |
| TypeScript | ✅ | + interface, type, enum |
| TSX | ✅ | uses JS extractor |
| Go | ✅ | struct, interface, func, method |
| Java | ✅ | class, interface, enum, method |
| C | ✅ | struct, enum, function |
| C++ | ✅ | + class |
| Ruby | ✅ | class, module, method |
| Scala | ✅ | class, object, trait, def |
| Vue | ✅ | script section functions |
| Markdown | ✅ | headings (nested) |
| JSON | ✅ | top-level keys |
| YAML | ✅ | top-level keys |
| TOML | ✅ | tables, keys |
| HTML | ⚠️ | parse only, no skeleton |
| CSS | ⚠️ | parse only, no skeleton |
| Bash | ⚠️ | parse only, no skeleton |

## High Priority

Mobile/cross-platform:
- Kotlin (`lang-kotlin`) - Android, multiplatform
- Swift (`lang-swift`) - iOS, macOS
- Dart (`lang-dart`) - Flutter

.NET ecosystem:
- C# (`lang-c-sharp`) - Unity, .NET
- F# (`lang-fsharp`) - functional .NET

Web backends:
- PHP (`lang-php`) - WordPress, Laravel
- Elixir (`lang-elixir`) - Phoenix
- Erlang (`lang-erlang`) - OTP

Modern systems:
- Zig (`lang-zig`) - systems programming
- Lua (`lang-lua`) - embedding, gamedev

Data/API:
- SQL (`lang-sql`) - queries
- GraphQL (`lang-graphql`) - API schemas

Infrastructure:
- Dockerfile (`lang-dockerfile`)
- HCL (`lang-hcl`) - Terraform

Frontend:
- Svelte (`lang-svelte`) - components
- SCSS (`lang-scss`) - stylesheets

## Medium Priority

Functional languages:
- Haskell (`lang-haskell`)
- OCaml (`lang-ocaml`)
- Elm (`lang-elm`)
- Clojure (`lang-clojure`)
- Scheme (`lang-scheme`)
- Common Lisp (`lang-commonlisp`)
- Gleam (`lang-gleam`)
- ReScript (`lang-rescript`)

Data science/scripting:
- R (`lang-r`)
- Julia (`lang-julia`)
- MATLAB (`lang-matlab`)
- Perl (`lang-perl`)

Build systems:
- Nix (`lang-nix`)
- CMake (`lang-cmake`)
- Meson (`lang-meson`)
- Starlark (`lang-starlark`)

Config formats:
- INI (`lang-ini`)
- XML (`lang-xml`)
- Nginx (`lang-nginx`)
- SSH Config (`lang-ssh-config`)

## Low Priority (Niche)

Hardware/embedded:
- Ada (`lang-ada`)
- VHDL (`lang-vhdl`)
- Verilog (`lang-verilog`)
- Device Tree (`lang-devicetree`)

Shaders:
- GLSL (`lang-glsl`)
- HLSL (`lang-hlsl`)

Theorem provers:
- Lean (`lang-lean`)
- Agda (`lang-agda`)
- Idris (`lang-idris`)
- Prolog (`lang-prolog`)

Documentation:
- AsciiDoc (`lang-asciidoc`)
- Typst (`lang-typst`)

Query languages:
- jq (`lang-jq`)
- SPARQL (`lang-sparql`)

Shells:
- Fish (`lang-fish`)
- Zsh (`lang-zsh`)
- PowerShell (`lang-powershell`)
- Batch (`lang-batch`)

Other:
- Objective-C (`lang-objc`)
- D (`lang-d`)
- Groovy (`lang-groovy`)
- Nim (not in arborium)
- Visual Basic (`lang-vb`)
- Vim script (`lang-vim`)
- Elisp (`lang-elisp`)
- Diff (`lang-diff`)
- DOT (`lang-dot`)
- Cap'n Proto (`lang-capnp`)
- Thrift (`lang-thrift`)
- TextProto (`lang-textproto`)
- RON (`lang-ron`)
- KDL (`lang-kdl`)
- Jinja2 (`lang-jinja2`)
- Caddy (`lang-caddy`)
- Ninja (`lang-ninja`)
- TLA+ (`lang-tlaplus`)
- Wit (`lang-wit`)
- Uiua (`lang-uiua`)
- Yuri (`lang-yuri`)

## Architecture: `moss-languages` Crate

### Directory Structure

```
crates/
  moss-core/                    # Unchanged
    src/
      lib.rs
      language.rs               # Language enum, extensions
      parsers.rs                # Parsers (arborium wrapper)
      paths.rs

  moss-languages/               # NEW CRATE
    Cargo.toml
    src/
      lib.rs                    # Trait + registry + re-exports
      traits.rs                 # LanguageSupport trait definition
      registry.rs               # Feature-gated registration
      nodes.rs                  # Common node kind constants

      # One file per language (~50-100 lines each)
      python.rs
      rust.rs
      javascript.rs
      typescript.rs
      go.rs
      java.rs
      scala.rs
      kotlin.rs
      swift.rs
      # ... 60+ more

  moss-cli/                     # Consumes moss-languages
    src/
      skeleton.rs               # Thin wrapper calling trait methods
      deps.rs                   # Thin wrapper
      ...
```

### Cargo.toml

```toml
[package]
name = "moss-languages"
version = "0.1.0"
edition = "2021"

[dependencies]
moss-core = { path = "../moss-core" }
arborium = { workspace = true }

[features]
default = ["common"]

# Tier 1: Core languages (~5MB) - universally useful, small grammars
tier1 = [
    "lang-python", "lang-rust", "lang-javascript", "lang-typescript",
    "lang-go", "lang-java", "lang-c",
    "lang-markdown", "lang-json", "lang-yaml", "lang-toml",
    "lang-html", "lang-css", "lang-bash",
]

# Tier 2: Common but larger (~25MB)
tier2 = [
    "lang-cpp", "lang-ruby", "lang-kotlin", "lang-c-sharp",
    "lang-swift", "lang-php", "lang-scala", "lang-tsx", "lang-vue",
    # ... plus lua, zig, elixir, dart, dockerfile, graphql, hcl, scss
]

# common = tier1 + tier2 (default, ~30MB)
# niche = large/specialized grammars (opt-in, ~60MB)
# all-languages = common + niche (~90MB)

# Individual language features (each enables arborium grammar)
lang-python = ["arborium/lang-python"]
lang-rust = ["arborium/lang-rust"]
lang-javascript = ["arborium/lang-javascript"]
lang-typescript = ["arborium/lang-typescript"]
lang-go = ["arborium/lang-go"]
lang-java = ["arborium/lang-java"]
lang-kotlin = ["arborium/lang-kotlin"]
lang-swift = ["arborium/lang-swift"]
lang-dart = ["arborium/lang-dart"]
lang-csharp = ["arborium/lang-c-sharp"]
lang-scala = ["arborium/lang-scala"]
# ... 60+ more
```

### Core Trait

```rust
// moss-languages/src/traits.rs

use rhizome_moss_core::{tree_sitter::Node, Language};

/// Unified language support trait
pub trait LanguageSupport: Send + Sync {
    /// Which Language enum variant this implements
    fn language(&self) -> Language;

    /// Grammar name for arborium (e.g., "python", "rust")
    fn grammar_name(&self) -> &'static str;

    // === Node Classification ===

    /// Container nodes that can hold methods (class, impl, module)
    fn container_kinds(&self) -> &'static [&'static str] { &[] }

    /// Function/method definition nodes
    fn function_kinds(&self) -> &'static [&'static str] { &[] }

    /// Type definition nodes (struct, enum, interface, type alias)
    fn type_kinds(&self) -> &'static [&'static str] { &[] }

    /// Import statement nodes
    fn import_kinds(&self) -> &'static [&'static str] { &[] }

    /// Export statement nodes
    fn export_kinds(&self) -> &'static [&'static str] { &[] }

    // === Symbol Extraction ===

    /// Extract symbol from a function/method node
    fn extract_function(&self, node: &Node, content: &str) -> Option<Symbol>;

    /// Extract symbol from a container node (class, impl, module)
    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol>;

    /// Extract symbol from a type definition node
    fn extract_type(&self, node: &Node, content: &str) -> Option<Symbol>;

    /// Extract docstring/doc comment for a node
    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> { None }

    // === Import/Export ===

    /// Extract import from an import node
    fn extract_import(&self, node: &Node, content: &str) -> Option<Import>;

    /// Extract export from an export node
    fn extract_export(&self, node: &Node, content: &str) -> Option<Export>;

    // === Complexity ===

    /// Nodes that increase cyclomatic complexity
    fn complexity_nodes(&self) -> &'static [&'static str] { &[] }

    /// Nodes that indicate nesting depth
    fn nesting_nodes(&self) -> &'static [&'static str] { &[] }

    // === Visibility ===

    /// Check if a node is public/exported
    fn is_public(&self, node: &Node, content: &str) -> bool { true }

    /// Get visibility modifier text if present
    fn visibility_modifier(&self, node: &Node, content: &str) -> Option<&str> { None }

    // === Edit Support ===

    /// Find the body node of a container (for prepend/append)
    fn container_body(&self, node: &Node) -> Option<Node> { None }

    /// Detect if first child of body is a docstring
    fn body_has_docstring(&self, body: &Node, content: &str) -> bool { false }
}

/// Common symbol representation
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub signature: String,
    pub docstring: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub visibility: Visibility,
}

pub enum SymbolKind {
    Function, Method, Class, Struct, Enum, Trait,
    Interface, Module, Type, Constant, Variable,
}

pub enum Visibility { Public, Private, Protected, Internal }

pub struct Import {
    pub module: String,
    pub names: Vec<String>,
    pub is_wildcard: bool,
    pub line: usize,
}

pub struct Export {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
}
```

### Example Language Implementation

```rust
// moss-languages/src/python.rs

use crate::traits::*;
use rhizome_moss_core::{tree_sitter::Node, Language};

pub struct PythonSupport;

impl LanguageSupport for PythonSupport {
    fn language(&self) -> Language { Language::Python }
    fn grammar_name(&self) -> &'static str { "python" }

    fn container_kinds(&self) -> &'static [&'static str] {
        &["class_definition"]
    }

    fn function_kinds(&self) -> &'static [&'static str] {
        &["function_definition", "async_function_definition"]
    }

    fn type_kinds(&self) -> &'static [&'static str] {
        &["class_definition"]  // Python classes are types
    }

    fn import_kinds(&self) -> &'static [&'static str] {
        &["import_statement", "import_from_statement"]
    }

    fn extract_function(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = node.child_by_field_name("name")?;
        let name_str = content[name.byte_range()].to_string();

        let params = node.child_by_field_name("parameters")
            .map(|p| &content[p.byte_range()])
            .unwrap_or("()");

        let ret = node.child_by_field_name("return_type")
            .map(|r| format!(" -> {}", &content[r.byte_range()]))
            .unwrap_or_default();

        let is_async = node.kind() == "async_function_definition";
        let prefix = if is_async { "async def" } else { "def" };

        Some(Symbol {
            name: name_str.clone(),
            kind: SymbolKind::Function,
            signature: format!("{} {}{}{}", prefix, name_str, params, ret),
            docstring: self.extract_docstring(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: if name_str.starts_with('_') {
                Visibility::Private
            } else {
                Visibility::Public
            },
        })
    }

    fn extract_container(&self, node: &Node, content: &str) -> Option<Symbol> {
        let name = node.child_by_field_name("name")?;
        let name_str = content[name.byte_range()].to_string();

        let bases = node.child_by_field_name("superclasses")
            .map(|b| &content[b.byte_range()])
            .unwrap_or("");

        let sig = if bases.is_empty() {
            format!("class {}", name_str)
        } else {
            format!("class {}{}", name_str, bases)
        };

        Some(Symbol {
            name: name_str,
            kind: SymbolKind::Class,
            signature: sig,
            docstring: self.extract_docstring(node, content),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            visibility: Visibility::Public,
        })
    }

    fn extract_docstring(&self, node: &Node, content: &str) -> Option<String> {
        let body = node.child_by_field_name("body")?;
        let first = body.child(0)?;

        // Handle both grammar versions
        let string_node = match first.kind() {
            "string" => Some(first),
            "expression_statement" => first.child(0).filter(|n| n.kind() == "string"),
            _ => None,
        }?;

        // Try string_content child (arborium style)
        let mut cursor = string_node.walk();
        for child in string_node.children(&mut cursor) {
            if child.kind() == "string_content" {
                let doc = content[child.byte_range()].trim();
                if !doc.is_empty() {
                    return Some(doc.to_string());
                }
            }
        }
        None
    }

    fn extract_import(&self, node: &Node, content: &str) -> Option<Import> {
        // ... import parsing logic
        todo!()
    }

    fn extract_export(&self, node: &Node, content: &str) -> Option<Export> {
        None  // Python doesn't have explicit exports
    }

    fn complexity_nodes(&self) -> &'static [&'static str] {
        &["if_statement", "for_statement", "while_statement",
          "try_statement", "except_clause", "with_statement",
          "match_statement", "case_clause", "and", "or"]
    }

    fn is_public(&self, node: &Node, content: &str) -> bool {
        if let Some(name) = node.child_by_field_name("name") {
            let name_str = &content[name.byte_range()];
            !name_str.starts_with('_') || name_str.starts_with("__")
        } else {
            true
        }
    }

    fn container_body(&self, node: &Node) -> Option<Node> {
        node.child_by_field_name("body")
    }

    fn body_has_docstring(&self, body: &Node, content: &str) -> bool {
        body.child(0).map(|c| {
            c.kind() == "string" ||
            (c.kind() == "expression_statement" &&
             c.child(0).map(|n| n.kind() == "string").unwrap_or(false))
        }).unwrap_or(false)
    }
}
```

### Registry

```rust
// moss-languages/src/registry.rs

use crate::traits::LanguageSupport;
use rhizome_moss_core::Language;
use std::collections::HashMap;
use std::sync::OnceLock;

static REGISTRY: OnceLock<HashMap<Language, Box<dyn LanguageSupport>>> = OnceLock::new();

pub fn get_support(lang: Language) -> Option<&'static dyn LanguageSupport> {
    REGISTRY.get_or_init(|| {
        let mut map: HashMap<Language, Box<dyn LanguageSupport>> = HashMap::new();

        #[cfg(feature = "lang-python")]
        map.insert(Language::Python, Box::new(crate::python::PythonSupport));

        #[cfg(feature = "lang-rust")]
        map.insert(Language::Rust, Box::new(crate::rust::RustSupport));

        #[cfg(feature = "lang-kotlin")]
        map.insert(Language::Kotlin, Box::new(crate::kotlin::KotlinSupport));

        // ... all other languages

        map
    }).get(&lang).map(|b| b.as_ref())
}

pub fn supported_languages() -> Vec<Language> {
    REGISTRY.get_or_init(|| HashMap::new()).keys().copied().collect()
}
```

### Usage in moss-cli

```rust
// moss-cli/src/skeleton.rs (after refactor)

use rhizome_moss_languages::{get_support, LanguageSupport, Symbol};

pub fn extract_skeleton(path: &Path, content: &str) -> Vec<Symbol> {
    let lang = Language::from_path(path)?;
    let support = get_support(lang)?;
    let tree = parsers.parse_lang(lang, content)?;

    let mut symbols = Vec::new();
    let mut cursor = tree.root_node().walk();

    collect_symbols(&mut cursor, content, support, &mut symbols, None);
    symbols
}

fn collect_symbols(
    cursor: &mut TreeCursor,
    content: &str,
    support: &dyn LanguageSupport,
    symbols: &mut Vec<Symbol>,
    parent: Option<&str>,
) {
    loop {
        let node = cursor.node();
        let kind = node.kind();

        // Generic dispatch based on node kinds
        if support.function_kinds().contains(&kind) {
            if let Some(sym) = support.extract_function(&node, content) {
                symbols.push(sym);
            }
        } else if support.container_kinds().contains(&kind) {
            if let Some(mut sym) = support.extract_container(&node, content) {
                // Recurse into container body
                if let Some(body) = support.container_body(&node) {
                    let mut body_cursor = body.walk();
                    collect_symbols(&mut body_cursor, content, support, &mut sym.children, Some(&sym.name));
                }
                symbols.push(sym);
                if cursor.goto_next_sibling() { continue; }
                break;
            }
        }

        if cursor.goto_first_child() {
            collect_symbols(cursor, content, support, symbols, parent);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() { break; }
    }
}
```

### Adding a New Language (Post-Refactor)

1. Add feature to `Cargo.toml`: `lang-kotlin = ["arborium/lang-kotlin"]`
2. Add to `all-languages` feature list
3. Create `src/kotlin.rs` implementing `LanguageSupport` (~50-100 lines)
4. Add to registry in `src/registry.rs`
5. Add `Kotlin` variant to `Language` enum in moss-core
6. Add tests

Total: ~60 lines of code vs ~200+ lines scattered across 8 files.

### Fixing Invalid Node Kinds

The `validate_node_kinds` test in `registry.rs` checks that all node kind strings returned by Language trait methods actually exist in the tree-sitter grammar. Currently 187 invalid kinds (test is `#[ignore]`).

**Workflow to fix a language:**

```bash
# 1. See which kinds are invalid for a language
cargo test -p moss-languages validate_node_kinds -- --ignored 2>&1 | grep "Python:"

# 2. Dump valid node kinds for that grammar
DUMP_GRAMMAR=python cargo test -p moss-languages dump_node_kinds -- --nocapture --ignored

# 3. Find the correct node kind name
# e.g., "async_function_definition" doesn't exist, but "function_definition" does
# Check if the grammar uses a different name or if the concept doesn't exist

# 4. Update the language file (e.g., python.rs)
# Replace invalid kind with correct one, or remove if not applicable

# 5. Re-run validation to confirm fix
cargo test -p moss-languages validate_node_kinds -- --ignored 2>&1 | grep "Python:"
```

**Common patterns:**
- `async_function_definition` → often just `function_definition` (async is an attribute)
- `switch_statement` → might be `expression_switch_statement` or `type_switch_statement`
- `*_declaration` vs `*_definition` - grammars vary
- Some concepts don't exist in all grammars (e.g., no `actor` in most languages)
