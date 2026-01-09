//! Generate command - code generation from API specs and schemas.

use clap::{Args, Subcommand};
use std::path::PathBuf;

/// Generate command arguments
#[derive(Args)]
pub struct GenerateArgs {
    #[command(subcommand)]
    pub target: GenerateTarget,
}

#[derive(Subcommand)]
pub enum GenerateTarget {
    /// Generate API client from OpenAPI spec
    Client {
        /// OpenAPI spec JSON file
        spec: PathBuf,

        /// Target language: typescript, python, rust
        #[arg(short, long)]
        lang: String,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate types from JSON Schema
    Types {
        /// JSON Schema file
        schema: PathBuf,

        /// Root type name
        #[arg(short, long, default_value = "Root")]
        name: String,

        /// Target language: typescript, python, rust
        #[arg(short, long)]
        lang: String,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

/// Run the generate command
pub fn run(args: GenerateArgs) -> i32 {
    match args.target {
        GenerateTarget::Client { spec, lang, output } => {
            let Some(generator) = rhizome_moss_openapi::find_generator(&lang) else {
                eprintln!("Unknown language: {}. Available:", lang);
                for (lang, variant) in rhizome_moss_openapi::list_generators() {
                    eprintln!("  {} ({})", lang, variant);
                }
                return 1;
            };

            let content = match std::fs::read_to_string(&spec) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to read {}: {}", spec.display(), e);
                    return 1;
                }
            };
            let spec_json: serde_json::Value = match serde_json::from_str(&content) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("Failed to parse JSON: {}", e);
                    return 1;
                }
            };

            let code = generator.generate(&spec_json);

            if let Some(path) = output {
                if let Err(e) = std::fs::write(&path, &code) {
                    eprintln!("Failed to write {}: {}", path.display(), e);
                    return 1;
                }
                eprintln!("Generated {}", path.display());
            } else {
                print!("{}", code);
            }
            0
        }
        GenerateTarget::Types {
            schema,
            name,
            lang,
            output,
        } => {
            let Some(generator) = rhizome_moss_jsonschema::find_generator(&lang) else {
                eprintln!("Unknown language: {}. Available:", lang);
                for l in rhizome_moss_jsonschema::list_generators() {
                    eprintln!("  {}", l);
                }
                return 1;
            };

            let content = match std::fs::read_to_string(&schema) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to read {}: {}", schema.display(), e);
                    return 1;
                }
            };
            let schema_json: serde_json::Value = match serde_json::from_str(&content) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("Failed to parse JSON: {}", e);
                    return 1;
                }
            };

            let code = generator.generate(&schema_json, &name);

            if let Some(path) = output {
                if let Err(e) = std::fs::write(&path, &code) {
                    eprintln!("Failed to write {}: {}", path.display(), e);
                    return 1;
                }
                eprintln!("Generated {}", path.display());
            } else {
                print!("{}", code);
            }
            0
        }
    }
}
