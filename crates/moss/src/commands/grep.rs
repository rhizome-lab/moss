//! Grep command - search file contents for a pattern.

use crate::commands::filter::detect_project_languages;
use crate::config::MossConfig;
use crate::filter::Filter;
use crate::grep;
use crate::merge::Merge;
use crate::output::{OutputFormat, OutputFormatter};
use clap::Args;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Grep command configuration.
#[derive(Debug, Clone, Deserialize, Default, Merge)]
#[serde(default)]
pub struct GrepConfig {
    /// Default maximum number of matches
    pub limit: Option<usize>,
    /// Case-insensitive search by default
    pub ignore_case: Option<bool>,
}

impl GrepConfig {
    pub fn limit(&self) -> usize {
        self.limit.unwrap_or(100)
    }

    pub fn ignore_case(&self) -> bool {
        self.ignore_case.unwrap_or(false)
    }
}

/// Grep command arguments.
#[derive(Args, Debug)]
pub struct GrepArgs {
    /// Regex pattern to search for
    pub pattern: String,

    /// Root directory (defaults to current directory)
    #[arg(short, long)]
    pub root: Option<PathBuf>,

    /// Maximum number of matches to return
    #[arg(short, long)]
    pub limit: Option<usize>,

    /// Case-insensitive search
    #[arg(short = 'i', long)]
    pub ignore_case: bool,

    /// Exclude files matching patterns or aliases
    #[arg(long, value_delimiter = ',')]
    pub exclude: Vec<String>,

    /// Only include files matching patterns or aliases
    #[arg(long, value_delimiter = ',')]
    pub only: Vec<String>,
}

/// Run grep command with args.
pub fn run(args: GrepArgs, format: crate::output::OutputFormat) -> i32 {
    let effective_root = args
        .root
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let config = MossConfig::load(&effective_root);

    cmd_grep(
        &args.pattern,
        args.root.as_deref(),
        args.limit.unwrap_or_else(|| config.grep.limit()),
        args.ignore_case || config.grep.ignore_case(),
        &format,
        &args.exclude,
        &args.only,
    )
}

/// Search file contents for a pattern
pub fn cmd_grep(
    pattern: &str,
    root: Option<&Path>,
    limit: usize,
    ignore_case: bool,
    format: &OutputFormat,
    exclude: &[String],
    only: &[String],
) -> i32 {
    let root = root
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap());

    // Build filter for --exclude and --only
    let filter = if !exclude.is_empty() || !only.is_empty() {
        let config = MossConfig::load(&root);
        let languages = detect_project_languages(&root);
        let lang_refs: Vec<&str> = languages.iter().map(|s| s.as_str()).collect();

        match Filter::new(exclude, only, &config.filter, &lang_refs) {
            Ok(f) => {
                for warning in f.warnings() {
                    eprintln!("warning: {}", warning);
                }
                Some(f)
            }
            Err(e) => {
                eprintln!("error: {}", e);
                return 1;
            }
        }
    } else {
        None
    };

    match grep::grep(pattern, &root, filter.as_ref(), limit, ignore_case) {
        Ok(result) => {
            if result.matches.is_empty() && !format.is_json() {
                eprintln!("No matches found for: {}", pattern);
                return 1;
            }
            result.print(format);
            0
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            1
        }
    }
}
