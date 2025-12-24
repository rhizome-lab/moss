//! Language detection and metadata.

use std::path::Path;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Python,
    Rust,
    JavaScript,
    TypeScript,
    Tsx,
    Markdown,
    Json,
    Yaml,
    Html,
    Css,
    Go,
    C,
    Cpp,
    Java,
    Ruby,
    Bash,
    Toml,
    Scala,
    Vue,
}

impl Language {
    /// Detect language from file extension
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "py" | "pyi" | "pyw" => Some(Language::Python),
            "rs" => Some(Language::Rust),
            "js" | "mjs" | "cjs" => Some(Language::JavaScript),
            "ts" | "mts" | "cts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "jsx" => Some(Language::JavaScript), // JSX uses JS parser
            "md" | "markdown" => Some(Language::Markdown),
            "json" | "jsonc" => Some(Language::Json),
            "yaml" | "yml" => Some(Language::Yaml),
            "html" | "htm" => Some(Language::Html),
            "css" | "scss" => Some(Language::Css),
            "go" => Some(Language::Go),
            "c" | "h" => Some(Language::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Some(Language::Cpp),
            "java" => Some(Language::Java),
            "rb" | "ruby" => Some(Language::Ruby),
            "sh" | "bash" | "zsh" => Some(Language::Bash),
            "toml" => Some(Language::Toml),
            "scala" | "sc" => Some(Language::Scala),
            "vue" => Some(Language::Vue),
            _ => None,
        }
    }

    /// Detect language from file path
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(Self::from_extension)
    }

    /// Get the common file extensions for this language
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Language::Python => &["py", "pyi", "pyw"],
            Language::Rust => &["rs"],
            Language::JavaScript => &["js", "mjs", "cjs", "jsx"],
            Language::TypeScript => &["ts", "mts", "cts"],
            Language::Tsx => &["tsx"],
            Language::Markdown => &["md", "markdown"],
            Language::Json => &["json", "jsonc"],
            Language::Yaml => &["yaml", "yml"],
            Language::Html => &["html", "htm"],
            Language::Css => &["css", "scss"],
            Language::Go => &["go"],
            Language::C => &["c", "h"],
            Language::Cpp => &["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
            Language::Java => &["java"],
            Language::Ruby => &["rb"],
            Language::Bash => &["sh", "bash", "zsh"],
            Language::Toml => &["toml"],
            Language::Scala => &["scala", "sc"],
            Language::Vue => &["vue"],
        }
    }

    /// Language name for display
    pub fn name(&self) -> &'static str {
        match self {
            Language::Python => "Python",
            Language::Rust => "Rust",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::Tsx => "TSX",
            Language::Markdown => "Markdown",
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Go => "Go",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::Java => "Java",
            Language::Ruby => "Ruby",
            Language::Bash => "Bash",
            Language::Toml => "TOML",
            Language::Scala => "Scala",
            Language::Vue => "Vue",
        }
    }

    /// Whether this language typically has symbols (functions, classes, etc.)
    pub fn has_symbols(&self) -> bool {
        match self {
            Language::Json | Language::Yaml | Language::Markdown | Language::Toml => false,
            _ => true,
        }
    }
}
