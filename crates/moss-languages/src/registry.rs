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
}
