//! Language support for moss.
//!
//! This crate provides the `Language` trait and implementations for
//! various programming languages. Each language struct IS its support implementation.
//!
//! # Features
//!
//! - `all-languages` (default): Enable all supported languages
//! - `tier1`: Enable most common languages (Python, Rust, JS, TS, Go, Java, C++)
//! - `lang-python`, `lang-rust`, etc.: Enable individual languages
//!
//! # Example
//!
//! ```ignore
//! use moss_languages::{Python, Language, support_for_path};
//! use std::path::Path;
//!
//! // Static usage (compile-time known language):
//! println!("Python function kinds: {:?}", Python.function_kinds());
//!
//! // Dynamic lookup (from file path):
//! if let Some(support) = support_for_path(Path::new("foo.py")) {
//!     println!("Language: {}", support.name());
//! }
//! ```

mod registry;
mod traits;
pub mod ecmascript;
#[cfg(any(feature = "lang-c", feature = "lang-cpp"))]
pub mod c_cpp;
pub mod external_packages;

// Language implementations
#[cfg(feature = "lang-python")]
pub mod python;

#[cfg(feature = "lang-rust")]
pub mod rust;

#[cfg(feature = "lang-javascript")]
pub mod javascript;

#[cfg(feature = "lang-typescript")]
pub mod typescript;

#[cfg(feature = "lang-go")]
pub mod go;

#[cfg(feature = "lang-java")]
pub mod java;

#[cfg(feature = "lang-kotlin")]
pub mod kotlin;

#[cfg(feature = "lang-c-sharp")]
pub mod csharp;

#[cfg(feature = "lang-swift")]
pub mod swift;

#[cfg(feature = "lang-php")]
pub mod php;

#[cfg(feature = "lang-dockerfile")]
pub mod dockerfile;

#[cfg(feature = "lang-c")]
pub mod c;

#[cfg(feature = "lang-cpp")]
pub mod cpp;

#[cfg(feature = "lang-ruby")]
pub mod ruby;

#[cfg(feature = "lang-scala")]
pub mod scala;

#[cfg(feature = "lang-vue")]
pub mod vue;

#[cfg(feature = "lang-markdown")]
pub mod markdown;

#[cfg(feature = "lang-json")]
pub mod json;

#[cfg(feature = "lang-yaml")]
pub mod yaml;

#[cfg(feature = "lang-toml")]
pub mod toml;

#[cfg(feature = "lang-html")]
pub mod html;

#[cfg(feature = "lang-css")]
pub mod css;

#[cfg(feature = "lang-bash")]
pub mod bash;

#[cfg(feature = "lang-lua")]
pub mod lua;

#[cfg(feature = "lang-zig")]
pub mod zig;

#[cfg(feature = "lang-elixir")]
pub mod elixir;

#[cfg(feature = "lang-erlang")]
pub mod erlang;

#[cfg(feature = "lang-dart")]
pub mod dart;

#[cfg(feature = "lang-fsharp")]
pub mod fsharp;

#[cfg(feature = "lang-sql")]
pub mod sql;

#[cfg(feature = "lang-graphql")]
pub mod graphql;

#[cfg(feature = "lang-hcl")]
pub mod hcl;

#[cfg(feature = "lang-scss")]
pub mod scss;

#[cfg(feature = "lang-svelte")]
pub mod svelte;

#[cfg(feature = "lang-xml")]
pub mod xml;

#[cfg(feature = "lang-clojure")]
pub mod clojure;

#[cfg(feature = "lang-haskell")]
pub mod haskell;

#[cfg(feature = "lang-ocaml")]
pub mod ocaml;

#[cfg(feature = "lang-nix")]
pub mod nix;

#[cfg(feature = "lang-perl")]
pub mod perl;

#[cfg(feature = "lang-r")]
pub mod r;

#[cfg(feature = "lang-julia")]
pub mod julia;

#[cfg(feature = "lang-elm")]
pub mod elm;

#[cfg(feature = "lang-cmake")]
pub mod cmake;

#[cfg(feature = "lang-vim")]
pub mod vim;

#[cfg(feature = "lang-awk")]
pub mod awk;

#[cfg(feature = "lang-fish")]
pub mod fish;

#[cfg(feature = "lang-jq")]
pub mod jq;

#[cfg(feature = "lang-powershell")]
pub mod powershell;

#[cfg(feature = "lang-zsh")]
pub mod zsh;

#[cfg(feature = "lang-groovy")]
pub mod groovy;

#[cfg(feature = "lang-glsl")]
pub mod glsl;

#[cfg(feature = "lang-hlsl")]
pub mod hlsl;

// Re-exports from registry
pub use registry::{register, support_for_extension, support_for_grammar, support_for_path, supported_languages, validate_unused_kinds_audit};

// Re-exports from traits
pub use traits::{
    EmbeddedBlock, Export, Import, Language, PackageSource, PackageSourceKind, Symbol, SymbolKind,
    Visibility, VisibilityMechanism, skip_dotfiles, has_extension,
};

// Re-export language structs
#[cfg(feature = "lang-python")]
pub use python::Python;

#[cfg(feature = "lang-rust")]
pub use rust::Rust;

#[cfg(feature = "lang-javascript")]
pub use javascript::JavaScript;

#[cfg(feature = "lang-typescript")]
pub use typescript::{TypeScript, Tsx};

#[cfg(feature = "lang-go")]
pub use go::Go;

#[cfg(feature = "lang-java")]
pub use java::Java;

#[cfg(feature = "lang-kotlin")]
pub use kotlin::Kotlin;

#[cfg(feature = "lang-c-sharp")]
pub use csharp::CSharp;

#[cfg(feature = "lang-swift")]
pub use swift::Swift;

#[cfg(feature = "lang-php")]
pub use php::Php;

#[cfg(feature = "lang-dockerfile")]
pub use dockerfile::Dockerfile;

#[cfg(feature = "lang-c")]
pub use c::C;

#[cfg(feature = "lang-cpp")]
pub use cpp::Cpp;

#[cfg(feature = "lang-ruby")]
pub use ruby::Ruby;

#[cfg(feature = "lang-scala")]
pub use scala::Scala;

#[cfg(feature = "lang-vue")]
pub use vue::Vue;

#[cfg(feature = "lang-markdown")]
pub use markdown::Markdown;

#[cfg(feature = "lang-json")]
pub use json::Json;

#[cfg(feature = "lang-yaml")]
pub use yaml::Yaml;

#[cfg(feature = "lang-toml")]
pub use toml::Toml;

#[cfg(feature = "lang-html")]
pub use html::Html;

#[cfg(feature = "lang-css")]
pub use css::Css;

#[cfg(feature = "lang-bash")]
pub use bash::Bash;

#[cfg(feature = "lang-lua")]
pub use lua::Lua;

#[cfg(feature = "lang-zig")]
pub use zig::Zig;

#[cfg(feature = "lang-elixir")]
pub use elixir::Elixir;

#[cfg(feature = "lang-erlang")]
pub use erlang::Erlang;

#[cfg(feature = "lang-dart")]
pub use dart::Dart;

#[cfg(feature = "lang-fsharp")]
pub use fsharp::FSharp;

#[cfg(feature = "lang-sql")]
pub use sql::Sql;

#[cfg(feature = "lang-graphql")]
pub use graphql::GraphQL;

#[cfg(feature = "lang-hcl")]
pub use hcl::Hcl;

#[cfg(feature = "lang-scss")]
pub use scss::Scss;

#[cfg(feature = "lang-svelte")]
pub use svelte::Svelte;

#[cfg(feature = "lang-xml")]
pub use xml::Xml;

#[cfg(feature = "lang-clojure")]
pub use clojure::Clojure;

#[cfg(feature = "lang-haskell")]
pub use haskell::Haskell;

#[cfg(feature = "lang-ocaml")]
pub use ocaml::OCaml;

#[cfg(feature = "lang-nix")]
pub use nix::Nix;

#[cfg(feature = "lang-perl")]
pub use perl::Perl;

#[cfg(feature = "lang-r")]
pub use r::R;

#[cfg(feature = "lang-julia")]
pub use julia::Julia;

#[cfg(feature = "lang-elm")]
pub use elm::Elm;

#[cfg(feature = "lang-cmake")]
pub use cmake::CMake;

#[cfg(feature = "lang-vim")]
pub use vim::Vim;

#[cfg(feature = "lang-awk")]
pub use awk::Awk;

#[cfg(feature = "lang-fish")]
pub use fish::Fish;

#[cfg(feature = "lang-jq")]
pub use jq::Jq;

#[cfg(feature = "lang-powershell")]
pub use powershell::PowerShell;

#[cfg(feature = "lang-zsh")]
pub use zsh::Zsh;

#[cfg(feature = "lang-groovy")]
pub use groovy::Groovy;

#[cfg(feature = "lang-glsl")]
pub use glsl::Glsl;

#[cfg(feature = "lang-hlsl")]
pub use hlsl::Hlsl;
