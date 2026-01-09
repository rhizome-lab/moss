//! Server commands for moss (MCP, HTTP, LSP).
//!
//! Servers expose moss functionality over various protocols.

use clap::{Args, Subcommand};
use serde::Deserialize;
use std::path::PathBuf;

use rhizome_moss_derive::Merge;

pub mod http;
pub mod lsp;
pub mod mcp;

/// Serve configuration from config.toml.
#[derive(Debug, Clone, Deserialize, Default, Merge)]
#[serde(default)]
pub struct ServeConfig {
    /// Default HTTP port (overridden by --port).
    pub http_port: Option<u16>,
    /// HTTP host to bind to.
    pub http_host: Option<String>,
}

impl ServeConfig {
    pub fn http_port(&self) -> u16 {
        self.http_port.unwrap_or(8080)
    }

    pub fn http_host(&self) -> &str {
        self.http_host.as_deref().unwrap_or("127.0.0.1")
    }
}

/// Serve command arguments
#[derive(Args)]
pub struct ServeArgs {
    #[command(subcommand)]
    pub protocol: ServeProtocol,

    /// Root directory (defaults to current directory)
    #[arg(short, long, global = true)]
    pub root: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum ServeProtocol {
    /// Start MCP server for LLM integration (stdio transport)
    Mcp,

    /// Start HTTP server (REST API)
    Http {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Output OpenAPI spec and exit (don't start server)
        #[arg(long)]
        openapi: bool,
    },

    /// Start LSP server for IDE integration
    Lsp,
}

/// Run the serve command
pub fn run(args: ServeArgs, json: bool) -> i32 {
    use crate::config::MossConfig;

    let root = args.root.clone().unwrap_or_else(|| PathBuf::from("."));
    let config = MossConfig::load(&root);

    match args.protocol {
        ServeProtocol::Mcp => mcp::cmd_serve_mcp(args.root.as_deref(), json),
        ServeProtocol::Http { port, openapi } => {
            if openapi {
                // Output OpenAPI spec and exit
                use http::ApiDoc;
                use utoipa::OpenApi;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&ApiDoc::openapi()).unwrap()
                );
                0
            } else {
                // CLI port overrides config
                let effective_port = if port != 8080 {
                    port // Explicit CLI value
                } else {
                    config.serve.http_port()
                };
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(http::run_http_server(&root, effective_port))
            }
        }
        ServeProtocol::Lsp => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(lsp::run_lsp_server(args.root.as_deref()))
        }
    }
}
