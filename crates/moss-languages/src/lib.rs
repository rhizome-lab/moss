//! Language support for moss.
//!
//! This crate provides the `Language` trait and implementations for
//! various programming languages. Each language struct IS its support implementation.
//!
//! Grammars are loaded dynamically from shared libraries via `GrammarLoader`.
//! Build grammars with `cargo xtask build-grammars`.
//!
//! # Example
//!
//! ```ignore
//! use moss_languages::{Python, Language, support_for_path, GrammarLoader};
//! use std::path::Path;
//!
//! // Load grammars
//! let loader = GrammarLoader::new();
//! let python_grammar = loader.get("python").expect("grammar not found");
//!
//! // Static usage (compile-time known language):
//! println!("Python function kinds: {:?}", Python.function_kinds());
//!
//! // Dynamic lookup (from file path):
//! if let Some(support) = support_for_path(Path::new("foo.py")) {
//!     println!("Language: {}", support.name());
//! }
//! ```

pub mod c_cpp;
pub mod ecmascript;
pub mod external_packages;
pub mod ffi;
mod grammar_loader;
mod registry;
mod traits;

// Language implementations
pub mod ada;
pub mod agda;
pub mod asciidoc;
pub mod asm;
pub mod awk;
pub mod bash;
pub mod batch;
pub mod c;
pub mod caddy;
pub mod capnp;
pub mod clojure;
pub mod cmake;
pub mod commonlisp;
pub mod cpp;
pub mod csharp;
pub mod css;
pub mod d;
pub mod dart;
pub mod devicetree;
pub mod diff;
pub mod dockerfile;
pub mod dot;
pub mod elisp;
pub mod elixir;
pub mod elm;
pub mod erlang;
pub mod fish;
pub mod fsharp;
pub mod gleam;
pub mod glsl;
pub mod go;
pub mod graphql;
pub mod groovy;
pub mod haskell;
pub mod hcl;
pub mod hlsl;
pub mod html;
pub mod idris;
pub mod ini;
pub mod java;
pub mod javascript;
pub mod jinja2;
pub mod jq;
pub mod json;
pub mod julia;
pub mod kdl;
pub mod kotlin;
pub mod lean;
pub mod lua;
pub mod markdown;
pub mod matlab;
pub mod meson;
pub mod nginx;
pub mod ninja;
pub mod nix;
pub mod objc;
pub mod ocaml;
pub mod perl;
pub mod php;
pub mod postscript;
pub mod powershell;
pub mod prolog;
pub mod python;
pub mod query;
pub mod r;
pub mod rescript;
pub mod ron;
pub mod ruby;
pub mod rust;
pub mod scala;
pub mod scheme;
pub mod scss;
pub mod sparql;
pub mod sql;
pub mod sshconfig;
pub mod starlark;
pub mod svelte;
pub mod swift;
pub mod textproto;
pub mod thrift;
pub mod tlaplus;
pub mod toml;
pub mod typescript;
pub mod typst;
pub mod uiua;
pub mod vb;
pub mod verilog;
pub mod vhdl;
pub mod vim;
pub mod vue;
pub mod wit;
pub mod x86asm;
pub mod xml;
pub mod yaml;
pub mod yuri;
pub mod zig;
pub mod zsh;

// Re-exports
pub use grammar_loader::GrammarLoader;
pub use registry::{
    register, support_for_extension, support_for_grammar, support_for_path, supported_languages,
    validate_unused_kinds_audit,
};
pub use traits::{
    EmbeddedBlock, Export, Import, Language, PackageSource, PackageSourceKind, Symbol, SymbolKind,
    Visibility, VisibilityMechanism, has_extension, skip_dotfiles,
};

// Re-export language structs
pub use ada::Ada;
pub use agda::Agda;
pub use asciidoc::AsciiDoc;
pub use asm::Asm;
pub use awk::Awk;
pub use bash::Bash;
pub use batch::Batch;
pub use c::C;
pub use caddy::Caddy;
pub use capnp::Capnp;
pub use clojure::Clojure;
pub use cmake::CMake;
pub use commonlisp::CommonLisp;
pub use cpp::Cpp;
pub use csharp::CSharp;
pub use css::Css;
pub use d::D;
pub use dart::Dart;
pub use devicetree::DeviceTree;
pub use diff::Diff;
pub use dockerfile::Dockerfile;
pub use dot::Dot;
pub use elisp::Elisp;
pub use elixir::Elixir;
pub use elm::Elm;
pub use erlang::Erlang;
pub use fish::Fish;
pub use fsharp::FSharp;
pub use gleam::Gleam;
pub use glsl::Glsl;
pub use go::Go;
pub use graphql::GraphQL;
pub use groovy::Groovy;
pub use haskell::Haskell;
pub use hcl::Hcl;
pub use hlsl::Hlsl;
pub use html::Html;
pub use idris::Idris;
pub use ini::Ini;
pub use java::Java;
pub use javascript::JavaScript;
pub use jinja2::Jinja2;
pub use jq::Jq;
pub use json::Json;
pub use julia::Julia;
pub use kdl::Kdl;
pub use kotlin::Kotlin;
pub use lean::Lean;
pub use lua::Lua;
pub use markdown::Markdown;
pub use matlab::Matlab;
pub use meson::Meson;
pub use nginx::Nginx;
pub use ninja::Ninja;
pub use nix::Nix;
pub use objc::ObjC;
pub use ocaml::OCaml;
pub use perl::Perl;
pub use php::Php;
pub use postscript::PostScript;
pub use powershell::PowerShell;
pub use prolog::Prolog;
pub use python::Python;
pub use query::Query;
pub use r::R;
pub use rescript::ReScript;
pub use ron::Ron;
pub use ruby::Ruby;
pub use rust::Rust;
pub use scala::Scala;
pub use scheme::Scheme;
pub use scss::Scss;
pub use sparql::Sparql;
pub use sql::Sql;
pub use sshconfig::SshConfig;
pub use starlark::Starlark;
pub use svelte::Svelte;
pub use swift::Swift;
pub use textproto::TextProto;
pub use thrift::Thrift;
pub use tlaplus::TlaPlus;
pub use toml::Toml;
pub use typescript::{Tsx, TypeScript};
pub use typst::Typst;
pub use uiua::Uiua;
pub use vb::VB;
pub use verilog::Verilog;
pub use vhdl::Vhdl;
pub use vim::Vim;
pub use vue::Vue;
pub use wit::Wit;
pub use x86asm::X86Asm;
pub use xml::Xml;
pub use yaml::Yaml;
pub use yuri::Yuri;
pub use zig::Zig;
pub use zsh::Zsh;
