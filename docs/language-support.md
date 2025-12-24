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

## Adding a New Language

1. Add to `Cargo.toml` features: `"lang-xxx"`
2. Add variant to `Language` enum in `language.rs`
3. Add extension mapping in `from_extension()`
4. Add arborium name in `Parsers::arborium_name()`
5. Add `extract_xxx()` method in `skeleton.rs`
6. Add test in `skeleton.rs`
