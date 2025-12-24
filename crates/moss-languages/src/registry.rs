//! Language support registry with feature-gated registration.

use crate::LanguageSupport;
use moss_core::Language;
use std::collections::HashMap;
use std::sync::OnceLock;

type LanguageMap = HashMap<Language, &'static dyn LanguageSupport>;

static REGISTRY: OnceLock<LanguageMap> = OnceLock::new();

/// Get language support for a given language.
///
/// Returns `None` if the language is not supported or the feature is not enabled.
pub fn get_support(lang: Language) -> Option<&'static dyn LanguageSupport> {
    REGISTRY.get_or_init(init_registry).get(&lang).copied()
}

/// Get all supported languages (based on enabled features).
pub fn supported_languages() -> Vec<Language> {
    REGISTRY
        .get_or_init(init_registry)
        .keys()
        .copied()
        .collect()
}

/// Check if a language is supported.
pub fn is_supported(lang: Language) -> bool {
    REGISTRY.get_or_init(init_registry).contains_key(&lang)
}

fn init_registry() -> LanguageMap {
    let mut map = HashMap::new();

    #[cfg(feature = "lang-python")]
    map.insert(Language::Python, &crate::python::PythonSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-rust")]
    map.insert(Language::Rust, &crate::rust::RustSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-javascript")]
    map.insert(Language::JavaScript, &crate::javascript::JavaScriptSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-typescript")]
    map.insert(Language::TypeScript, &crate::typescript::TypeScriptSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-go")]
    map.insert(Language::Go, &crate::go::GoSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-java")]
    map.insert(Language::Java, &crate::java::JavaSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-c")]
    map.insert(Language::C, &crate::c::CSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-cpp")]
    map.insert(Language::Cpp, &crate::cpp::CppSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-ruby")]
    map.insert(Language::Ruby, &crate::ruby::RubySupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-scala")]
    map.insert(Language::Scala, &crate::scala::ScalaSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-vue")]
    map.insert(Language::Vue, &crate::vue::VueSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-markdown")]
    map.insert(Language::Markdown, &crate::markdown::MarkdownSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-json")]
    map.insert(Language::Json, &crate::json::JsonSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-yaml")]
    map.insert(Language::Yaml, &crate::yaml::YamlSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-toml")]
    map.insert(Language::Toml, &crate::toml::TomlSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-html")]
    map.insert(Language::Html, &crate::html::HtmlSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-css")]
    map.insert(Language::Css, &crate::css::CssSupport as &dyn LanguageSupport);

    #[cfg(feature = "lang-bash")]
    map.insert(Language::Bash, &crate::bash::BashSupport as &dyn LanguageSupport);

    map
}
