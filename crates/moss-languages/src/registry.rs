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

        #[cfg(feature = "lang-csharp")]
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

/// Get language support for a file extension.
///
/// Returns `None` if the extension is not recognized or the feature is not enabled.
pub fn support_for_extension(ext: &str) -> Option<&'static dyn Language> {
    extension_map()
        .get(ext)
        .or_else(|| extension_map().get(ext.to_lowercase().as_str()))
        .copied()
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
