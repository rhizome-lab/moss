//! Analyze command arguments with subcommands

use clap::{Args, Subcommand};
use std::path::PathBuf;

/// Analyze command arguments.
#[derive(Args, Debug)]
pub struct AnalyzeArgs {
    #[command(subcommand)]
    pub command: Option<AnalyzeCommand>,

    /// Root directory (defaults to current directory)
    #[arg(short, long, global = true)]
    pub root: Option<PathBuf>,

    /// Output as JSON
    #[arg(long, global = true)]
    pub json: bool,

    /// Filter JSON output with jq expression (implies --json)
    #[arg(long, global = true)]
    pub jq: Option<String>,

    /// Human-friendly output with colors and formatting
    #[arg(long, global = true)]
    pub pretty: bool,

    /// Exclude paths matching pattern or @alias
    #[arg(long, value_name = "PATTERN", global = true)]
    pub exclude: Vec<String>,

    /// Include only paths matching pattern or @alias
    #[arg(long, value_name = "PATTERN", global = true)]
    pub only: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum AnalyzeCommand {
    /// Run health analysis (file counts, complexity stats, large file warnings)
    Health {
        /// Target file or directory
        target: Option<String>,
    },

    /// Show comprehensive project overview (compact summary with grade)
    Overview {
        /// Compact one-line output
        #[arg(short, long)]
        compact: bool,
    },

    /// Run complexity analysis
    Complexity {
        /// Target file or directory
        target: Option<String>,

        /// Only show functions above this threshold
        #[arg(short, long)]
        threshold: Option<usize>,

        /// Filter by symbol kind: function, method
        #[arg(long)]
        kind: Option<String>,
    },

    /// Run function length analysis
    Length {
        /// Target file or directory
        target: Option<String>,
    },

    /// Run security analysis
    Security {
        /// Target file or directory
        target: Option<String>,
    },

    /// Analyze documentation coverage
    Docs {
        /// Number of worst-covered files to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },

    /// Show longest files in codebase
    Files {
        /// Number of files to show
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },

    /// Trace value provenance for a symbol
    Trace {
        /// Symbol to trace (format: symbol or file:line or file/symbol)
        symbol: String,

        /// Target file to search in
        #[arg(long)]
        target: Option<String>,

        /// Maximum trace depth
        #[arg(long, default_value = "10")]
        max_depth: usize,

        /// Trace into called functions (show what they return)
        #[arg(long)]
        recursive: bool,
    },

    /// Show what functions call a symbol
    Callers {
        /// Symbol to find callers for
        symbol: String,
    },

    /// Show what functions a symbol calls
    Callees {
        /// Symbol to find callees for
        symbol: String,
    },

    /// Run configured linters
    Lint {
        /// Target file or directory
        target: Option<String>,
    },

    /// Show git history hotspots (frequently changed files)
    Hotspots,

    /// Check documentation references for broken links
    CheckRefs,

    /// Find documentation with stale code references
    StaleDocs,

    /// Check example references in documentation
    CheckExamples,

    /// Detect duplicate functions (code clones)
    DuplicateFunctions {
        /// Elide identifier names when comparing (default: true)
        #[arg(long, default_value = "true")]
        elide_identifiers: bool,

        /// Elide literal values when comparing
        #[arg(long)]
        elide_literals: bool,

        /// Show source code for detected duplicates
        #[arg(long)]
        show_source: bool,

        /// Minimum lines for a function to be considered
        #[arg(long, default_value = "1")]
        min_lines: usize,

        /// Allow a duplicate function group (add to .moss/duplicate-functions-allow)
        #[arg(long, value_name = "LOCATION")]
        allow: Option<String>,

        /// Reason for allowing
        #[arg(long)]
        reason: Option<String>,
    },

    /// Detect duplicate type definitions
    DuplicateTypes {
        /// Target directory to scan
        target: Option<String>,

        /// Minimum field overlap percentage (default: 70)
        #[arg(long, default_value = "70")]
        min_overlap: usize,

        /// Allow a duplicate type pair (add to .moss/duplicate-types-allow)
        #[arg(long, num_args = 2, value_names = ["TYPE1", "TYPE2"])]
        allow: Option<Vec<String>>,

        /// Reason for allowing
        #[arg(long)]
        reason: Option<String>,
    },

    /// Run all analysis passes
    All {
        /// Target file or directory
        target: Option<String>,
    },
}
