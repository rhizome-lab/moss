//! Language support registry with extension-based lookup.

use crate::Language;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

/// Global language registry.
static LANGUAGES: RwLock<Vec<&'static dyn Language>> = RwLock::new(Vec::new());
static INITIALIZED: OnceLock<()> = OnceLock::new();

/// Cached extension → language lookup table.
static EXTENSION_MAP: OnceLock<HashMap<&'static str, &'static dyn Language>> = OnceLock::new();

/// Cached grammar_name → language lookup table.
static GRAMMAR_MAP: OnceLock<HashMap<&'static str, &'static dyn Language>> = OnceLock::new();

/// Register a language in the global registry.
/// Called internally by language modules.
pub fn register(lang: &'static dyn Language) {
    LANGUAGES.write().unwrap().push(lang);
}

/// Initialize built-in languages (called once).
fn init_builtin() {
    INITIALIZED.get_or_init(|| {
        #[cfg(feature = "lang-python")]
        register(&crate::python::Python);

        #[cfg(feature = "lang-rust")]
        register(&crate::rust::Rust);

        #[cfg(feature = "lang-javascript")]
        register(&crate::javascript::JavaScript);

        #[cfg(feature = "lang-typescript")]
        {
            register(&crate::typescript::TypeScript);
            register(&crate::typescript::Tsx);
        }

        #[cfg(feature = "lang-go")]
        register(&crate::go::Go);

        #[cfg(feature = "lang-java")]
        register(&crate::java::Java);

        #[cfg(feature = "lang-kotlin")]
        register(&crate::kotlin::Kotlin);

        #[cfg(feature = "lang-c-sharp")]
        register(&crate::csharp::CSharp);

        #[cfg(feature = "lang-swift")]
        register(&crate::swift::Swift);

        #[cfg(feature = "lang-php")]
        register(&crate::php::Php);

        #[cfg(feature = "lang-dockerfile")]
        register(&crate::dockerfile::Dockerfile);

        #[cfg(feature = "lang-c")]
        register(&crate::c::C);

        #[cfg(feature = "lang-cpp")]
        register(&crate::cpp::Cpp);

        #[cfg(feature = "lang-ruby")]
        register(&crate::ruby::Ruby);

        #[cfg(feature = "lang-scala")]
        register(&crate::scala::Scala);

        #[cfg(feature = "lang-vue")]
        register(&crate::vue::Vue);

        #[cfg(feature = "lang-markdown")]
        register(&crate::markdown::Markdown);

        #[cfg(feature = "lang-json")]
        register(&crate::json::Json);

        #[cfg(feature = "lang-yaml")]
        register(&crate::yaml::Yaml);

        #[cfg(feature = "lang-toml")]
        register(&crate::toml::Toml);

        #[cfg(feature = "lang-html")]
        register(&crate::html::Html);

        #[cfg(feature = "lang-css")]
        register(&crate::css::Css);

        #[cfg(feature = "lang-bash")]
        register(&crate::bash::Bash);

        #[cfg(feature = "lang-lua")]
        register(&crate::lua::Lua);

        #[cfg(feature = "lang-zig")]
        register(&crate::zig::Zig);

        #[cfg(feature = "lang-elixir")]
        register(&crate::elixir::Elixir);

        #[cfg(feature = "lang-erlang")]
        register(&crate::erlang::Erlang);

        #[cfg(feature = "lang-dart")]
        register(&crate::dart::Dart);

        #[cfg(feature = "lang-fsharp")]
        register(&crate::fsharp::FSharp);

        #[cfg(feature = "lang-sql")]
        register(&crate::sql::Sql);

        #[cfg(feature = "lang-graphql")]
        register(&crate::graphql::GraphQL);

        #[cfg(feature = "lang-hcl")]
        register(&crate::hcl::Hcl);

        #[cfg(feature = "lang-scss")]
        register(&crate::scss::Scss);

        #[cfg(feature = "lang-svelte")]
        register(&crate::svelte::Svelte);

        #[cfg(feature = "lang-xml")]
        register(&crate::xml::Xml);

        #[cfg(feature = "lang-clojure")]
        register(&crate::clojure::Clojure);

        #[cfg(feature = "lang-haskell")]
        register(&crate::haskell::Haskell);

        #[cfg(feature = "lang-ocaml")]
        register(&crate::ocaml::OCaml);

        #[cfg(feature = "lang-nix")]
        register(&crate::nix::Nix);

        #[cfg(feature = "lang-perl")]
        register(&crate::perl::Perl);

        #[cfg(feature = "lang-r")]
        register(&crate::r::R);

        #[cfg(feature = "lang-julia")]
        register(&crate::julia::Julia);

        #[cfg(feature = "lang-elm")]
        register(&crate::elm::Elm);

        #[cfg(feature = "lang-cmake")]
        register(&crate::cmake::CMake);

        #[cfg(feature = "lang-vim")]
        register(&crate::vim::Vim);

        #[cfg(feature = "lang-awk")]
        register(&crate::awk::Awk);

        #[cfg(feature = "lang-fish")]
        register(&crate::fish::Fish);

        #[cfg(feature = "lang-jq")]
        register(&crate::jq::Jq);

        #[cfg(feature = "lang-powershell")]
        register(&crate::powershell::PowerShell);

        #[cfg(feature = "lang-zsh")]
        register(&crate::zsh::Zsh);

        #[cfg(feature = "lang-groovy")]
        register(&crate::groovy::Groovy);

        #[cfg(feature = "lang-glsl")]
        register(&crate::glsl::Glsl);

        #[cfg(feature = "lang-hlsl")]
        register(&crate::hlsl::Hlsl);

        #[cfg(feature = "lang-commonlisp")]
        register(&crate::commonlisp::CommonLisp);

        #[cfg(feature = "lang-elisp")]
        register(&crate::elisp::Elisp);

        #[cfg(feature = "lang-gleam")]
        register(&crate::gleam::Gleam);

        #[cfg(feature = "lang-scheme")]
        register(&crate::scheme::Scheme);

        #[cfg(feature = "lang-ini")]
        register(&crate::ini::Ini);

        #[cfg(feature = "lang-diff")]
        register(&crate::diff::Diff);

        #[cfg(feature = "lang-dot")]
        register(&crate::dot::Dot);

        #[cfg(feature = "lang-kdl")]
        register(&crate::kdl::Kdl);

        #[cfg(feature = "lang-ada")]
        register(&crate::ada::Ada);

        #[cfg(feature = "lang-agda")]
        register(&crate::agda::Agda);

        #[cfg(feature = "lang-d")]
        register(&crate::d::D);

        #[cfg(feature = "lang-matlab")]
        register(&crate::matlab::Matlab);

        #[cfg(feature = "lang-meson")]
        register(&crate::meson::Meson);

        #[cfg(feature = "lang-nginx")]
        register(&crate::nginx::Nginx);

        #[cfg(feature = "lang-prolog")]
        register(&crate::prolog::Prolog);

        #[cfg(feature = "lang-batch")]
        register(&crate::batch::Batch);
    });
}

fn extension_map() -> &'static HashMap<&'static str, &'static dyn Language> {
    init_builtin();
    EXTENSION_MAP.get_or_init(|| {
        let mut map = HashMap::new();
        let langs = LANGUAGES.read().unwrap();
        for lang in langs.iter() {
            for ext in lang.extensions() {
                map.insert(*ext, *lang);
            }
        }
        map
    })
}

fn grammar_map() -> &'static HashMap<&'static str, &'static dyn Language> {
    init_builtin();
    GRAMMAR_MAP.get_or_init(|| {
        let mut map = HashMap::new();
        let langs = LANGUAGES.read().unwrap();
        for lang in langs.iter() {
            map.insert(lang.grammar_name(), *lang);
        }
        map
    })
}

/// Get language support for a file extension.
///
/// Returns `None` if the extension is not recognized or the feature is not enabled.
pub fn support_for_extension(ext: &str) -> Option<&'static dyn Language> {
    extension_map()
        .get(ext)
        .or_else(|| extension_map().get(ext.to_lowercase().as_str()))
        .copied()
}

/// Get language support by grammar name.
///
/// Returns `None` if the grammar is not recognized or the feature is not enabled.
pub fn support_for_grammar(grammar: &str) -> Option<&'static dyn Language> {
    grammar_map().get(grammar).copied()
}

/// Get language support from a file path.
///
/// Returns `None` if the file has no extension, the extension is not recognized,
/// or the feature is not enabled.
pub fn support_for_path(path: &Path) -> Option<&'static dyn Language> {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(support_for_extension)
}

/// Get all supported languages.
pub fn supported_languages() -> Vec<&'static dyn Language> {
    init_builtin();
    LANGUAGES.read().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use moss_core::arborium::GrammarStore;

    /// Dump all valid node kinds for a grammar (useful for fixing invalid kinds).
    /// Run with: cargo test -p moss-languages dump_node_kinds -- --nocapture
    #[test]
    #[ignore]
    fn dump_node_kinds() {
        let store = GrammarStore::new();
        // Change this to the grammar you want to inspect
        let grammar_name = std::env::var("DUMP_GRAMMAR").unwrap_or_else(|_| "python".to_string());

        let grammar = store.get(&grammar_name).expect("grammar not found");
        let ts_lang = grammar.language();

        println!("\n=== Valid node kinds for '{}' ===\n", grammar_name);
        let count = ts_lang.node_kind_count();
        for id in 0..count as u16 {
            if let Some(kind) = ts_lang.node_kind_for_id(id) {
                let named = ts_lang.node_kind_is_named(id);
                if named && !kind.starts_with('_') {
                    println!("{}", kind);
                }
            }
        }
    }

    /// Validate that all node kinds returned by Language trait methods
    /// actually exist in the tree-sitter grammar.
    #[test]
    fn validate_node_kinds() {
        let store = GrammarStore::new();
        let mut errors: Vec<String> = Vec::new();

        for lang in supported_languages() {
            let grammar_name = lang.grammar_name();
            let grammar = match store.get(grammar_name) {
                Some(g) => g,
                None => {
                    // Grammar not available (feature not enabled)
                    continue;
                }
            };
            let ts_lang = grammar.language();

            // Collect all node kinds from trait methods
            let all_kinds: Vec<(&str, &[&str])> = vec![
                ("container_kinds", lang.container_kinds()),
                ("function_kinds", lang.function_kinds()),
                ("type_kinds", lang.type_kinds()),
                ("import_kinds", lang.import_kinds()),
                ("public_symbol_kinds", lang.public_symbol_kinds()),
                ("scope_creating_kinds", lang.scope_creating_kinds()),
                ("control_flow_kinds", lang.control_flow_kinds()),
                ("complexity_nodes", lang.complexity_nodes()),
                ("nesting_nodes", lang.nesting_nodes()),
            ];

            for (method, kinds) in all_kinds {
                for kind in kinds {
                    // id_for_node_kind returns 0 if the kind doesn't exist
                    let id = ts_lang.id_for_node_kind(kind, true);
                    if id == 0 {
                        // Also check unnamed nodes (like operators)
                        let unnamed_id = ts_lang.id_for_node_kind(kind, false);
                        if unnamed_id == 0 {
                            errors.push(format!(
                                "{}: {}() contains invalid node kind '{}'",
                                lang.name(), method, kind
                            ));
                        }
                    }
                }
            }
        }

        if !errors.is_empty() {
            panic!(
                "Found {} invalid node kinds:\n{}",
                errors.len(),
                errors.join("\n")
            );
        }
    }

    /// Cross-check grammar node kinds against Language implementations.
    /// Finds potentially useful kinds that exist in the grammar but aren't used.
    /// Run with: cargo test -p moss-languages cross_check_node_kinds -- --nocapture --ignored
    #[test]
    #[ignore]
    fn cross_check_node_kinds() {
        use std::collections::HashSet;

        let store = GrammarStore::new();

        // Keywords that suggest a node kind might be useful
        let interesting_patterns = [
            "statement", "expression", "definition", "declaration",
            "clause", "block", "body", "import", "export", "function",
            "method", "class", "struct", "enum", "interface", "trait",
            "module", "type", "return", "if", "else", "for", "while",
            "loop", "match", "case", "try", "catch", "except", "throw",
            "raise", "with", "async", "await", "yield", "lambda",
            "comprehension", "generator", "operator",
        ];

        for lang in supported_languages() {
            let grammar_name = lang.grammar_name();
            let grammar = match store.get(grammar_name) {
                Some(g) => g,
                None => continue,
            };
            let ts_lang = grammar.language();

            // Collect all kinds currently used by the language
            let mut used_kinds: HashSet<&str> = HashSet::new();
            for kind in lang.container_kinds() { used_kinds.insert(kind); }
            for kind in lang.function_kinds() { used_kinds.insert(kind); }
            for kind in lang.type_kinds() { used_kinds.insert(kind); }
            for kind in lang.import_kinds() { used_kinds.insert(kind); }
            for kind in lang.public_symbol_kinds() { used_kinds.insert(kind); }
            for kind in lang.scope_creating_kinds() { used_kinds.insert(kind); }
            for kind in lang.control_flow_kinds() { used_kinds.insert(kind); }
            for kind in lang.complexity_nodes() { used_kinds.insert(kind); }
            for kind in lang.nesting_nodes() { used_kinds.insert(kind); }

            // Get all valid named node kinds from grammar
            let mut all_kinds: Vec<&str> = Vec::new();
            let count = ts_lang.node_kind_count();
            for id in 0..count as u16 {
                if let Some(kind) = ts_lang.node_kind_for_id(id) {
                    let named = ts_lang.node_kind_is_named(id);
                    if named && !kind.starts_with('_') {
                        all_kinds.push(kind);
                    }
                }
            }

            // Find unused but potentially interesting kinds
            let mut unused_interesting: Vec<&str> = all_kinds
                .into_iter()
                .filter(|kind| !used_kinds.contains(*kind))
                .filter(|kind| {
                    let lower = kind.to_lowercase();
                    interesting_patterns.iter().any(|p| lower.contains(p))
                })
                .collect();

            unused_interesting.sort();

            if !unused_interesting.is_empty() {
                println!("\n=== {} ({}) - {} potentially useful unused kinds ===",
                    lang.name(), grammar_name, unused_interesting.len());
                for kind in &unused_interesting {
                    println!("  {}", kind);
                }
            }
        }
    }
}

/// Validate that a language's unused node kinds audit is complete and accurate.
///
/// This function checks:
/// 1. All kinds in `documented_unused` actually exist in the grammar
/// 2. All potentially useful kinds from the grammar are either used or documented
///
/// Call this from each language's `unused_node_kinds_audit` test.
pub fn validate_unused_kinds_audit(
    lang: &dyn Language,
    documented_unused: &[&str],
) -> Result<(), String> {
    use std::collections::HashSet;
    use moss_core::arborium::GrammarStore;

    let store = GrammarStore::new();
    let grammar = store.get(lang.grammar_name())
        .ok_or_else(|| format!("Grammar '{}' not found", lang.grammar_name()))?;
    let ts_lang = grammar.language();

    // Keywords that suggest a node kind might be useful (same as cross_check_node_kinds)
    let interesting_patterns = [
        "statement", "expression", "definition", "declaration",
        "clause", "block", "body", "import", "export", "function",
        "method", "class", "struct", "enum", "interface", "trait",
        "module", "type", "return", "if", "else", "for", "while",
        "loop", "match", "case", "try", "catch", "except", "throw",
        "raise", "with", "async", "await", "yield", "lambda",
        "comprehension", "generator", "operator",
    ];

    // Collect all kinds used by Language trait methods
    let mut used_kinds: HashSet<&str> = HashSet::new();
    for kind in lang.container_kinds() { used_kinds.insert(kind); }
    for kind in lang.function_kinds() { used_kinds.insert(kind); }
    for kind in lang.type_kinds() { used_kinds.insert(kind); }
    for kind in lang.import_kinds() { used_kinds.insert(kind); }
    for kind in lang.public_symbol_kinds() { used_kinds.insert(kind); }
    for kind in lang.scope_creating_kinds() { used_kinds.insert(kind); }
    for kind in lang.control_flow_kinds() { used_kinds.insert(kind); }
    for kind in lang.complexity_nodes() { used_kinds.insert(kind); }
    for kind in lang.nesting_nodes() { used_kinds.insert(kind); }

    let documented_set: HashSet<&str> = documented_unused.iter().copied().collect();

    // Get all valid named node kinds from grammar
    let mut grammar_kinds: HashSet<&str> = HashSet::new();
    let count = ts_lang.node_kind_count();
    for id in 0..count as u16 {
        if let Some(kind) = ts_lang.node_kind_for_id(id) {
            let named = ts_lang.node_kind_is_named(id);
            if named && !kind.starts_with('_') {
                grammar_kinds.insert(kind);
            }
        }
    }

    let mut errors: Vec<String> = Vec::new();

    // Check 1: All documented unused kinds must exist in grammar
    for kind in documented_unused {
        if !grammar_kinds.contains(*kind) {
            errors.push(format!("Documented kind '{}' doesn't exist in grammar", kind));
        }
        // Also check it's not actually being used
        if used_kinds.contains(*kind) {
            errors.push(format!("Documented kind '{}' is actually used in trait methods", kind));
        }
    }

    // Check 2: All potentially useful grammar kinds must be used or documented
    for kind in &grammar_kinds {
        let lower = kind.to_lowercase();
        let is_interesting = interesting_patterns.iter().any(|p| lower.contains(p));

        if is_interesting && !used_kinds.contains(*kind) && !documented_set.contains(*kind) {
            errors.push(format!("Potentially useful kind '{}' is neither used nor documented", kind));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} validation errors:\n  - {}", errors.len(), errors.join("\n  - ")))
    }
}
