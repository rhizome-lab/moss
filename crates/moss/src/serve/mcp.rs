//! MCP (Model Context Protocol) server for moss.
//!
//! Exposes moss CLI as an MCP tool for LLM integration.
//! Compile with `--features mcp` to enable.
//!
//! Two modes (matching Python implementation):
//! 1. Single-tool mode (default): One "moss" tool that accepts CLI-style commands
//!    - Lower token overhead for tool definitions (~50 vs ~8K tokens)
//!    - Better for LLMs that handle CLI-style inputs well
//!
//! 2. Multi-tool mode (--full): Each command as a separate tool
//!    - Better discoverability for IDEs
//!    - Explicit parameter schemas

#[cfg(feature = "mcp")]
mod implementation {
    use std::process::Command;
    use std::sync::Arc;

    use rmcp::handler::server::router::tool::ToolRouter;
    use rmcp::handler::server::wrapper::Parameters;
    use rmcp::model::*;
    use rmcp::transport::stdio;
    use rmcp::{ErrorData as McpError, ServiceExt, tool, tool_handler, tool_router};
    use schemars::JsonSchema;
    use serde::Deserialize;

    /// Request for the moss tool.
    #[derive(Debug, Deserialize, JsonSchema)]
    pub struct MossRequest {
        /// view <path> [--deps|--focus|--full] | edit <path> --delete|--replace|--before|--after|--prepend|--append | analyze [--health|--complexity] | grep <pattern>
        pub command: String,
    }

    /// MCP server that wraps moss CLI.
    #[derive(Clone)]
    pub struct MossServer {
        root: Arc<String>,
        tool_router: ToolRouter<Self>,
    }

    #[tool_router]
    impl MossServer {
        /// Create a new MCP server for the given root directory.
        pub fn new(root: &str) -> Self {
            Self {
                root: Arc::new(root.to_string()),
                tool_router: Self::tool_router(),
            }
        }

        /// Code intelligence primitives.
        #[tool(description = "Code intelligence: view, analyze, grep")]
        async fn moss(
            &self,
            Parameters(req): Parameters<MossRequest>,
        ) -> Result<CallToolResult, McpError> {
            let root = self.root.clone();
            let command = req.command;
            let result = tokio::task::spawn_blocking(move || execute_moss_command(&command, &root))
                .await
                .unwrap_or_else(|e| CommandResult {
                    output: format!("Task panicked: {}", e),
                    exit_code: 1,
                });

            let content = if result.exit_code == 0 {
                Content::text(&result.output)
            } else {
                Content::text(&format!(
                    "Error (exit {}): {}",
                    result.exit_code, result.output
                ))
            };

            Ok(CallToolResult::success(vec![content]))
        }
    }

    #[tool_handler]
    impl rmcp::ServerHandler for MossServer {
        fn get_info(&self) -> ServerInfo {
            ServerInfo {
                instructions: Some(
                    "Use the 'moss' tool to query code intelligence for the codebase.".into(),
                ),
                capabilities: ServerCapabilities::builder().enable_tools().build(),
                ..Default::default()
            }
        }
    }

    /// Result of executing a moss CLI command.
    struct CommandResult {
        output: String,
        exit_code: i32,
    }

    /// Execute a moss CLI command.
    fn execute_moss_command(command: &str, root: &str) -> CommandResult {
        let current_exe = match std::env::current_exe() {
            Ok(exe) => exe,
            Err(e) => {
                return CommandResult {
                    output: format!("Failed to get current executable: {}", e),
                    exit_code: 1,
                };
            }
        };

        let args: Vec<&str> = command.split_whitespace().collect();
        if args.is_empty() {
            return CommandResult {
                output: "Empty command".to_string(),
                exit_code: 1,
            };
        }

        let output = match Command::new(&current_exe)
            .args(&args)
            .current_dir(root)
            .output()
        {
            Ok(out) => out,
            Err(e) => {
                return CommandResult {
                    output: format!("Failed to execute command: {}", e),
                    exit_code: 1,
                };
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(1);

        if exit_code == 0 {
            CommandResult {
                output: stdout,
                exit_code: 0,
            }
        } else if stderr.is_empty() {
            CommandResult {
                output: stdout,
                exit_code,
            }
        } else {
            CommandResult {
                output: format!("{}\nError: {}", stdout, stderr),
                exit_code,
            }
        }
    }

    /// Run the MCP server.
    pub async fn run_server(_root: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let server = MossServer::new(_root);
        let service = server.serve(stdio()).await?;
        service.waiting().await?;
        Ok(())
    }
}

/// Command handler for `moss serve mcp`.
pub fn cmd_serve_mcp(root: Option<&std::path::Path>, _json: bool) -> i32 {
    #[cfg(feature = "mcp")]
    {
        let root = root
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                eprintln!("Failed to create runtime: {}", e);
                return 1;
            }
        };

        match rt.block_on(implementation::run_server(&root)) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("MCP server error: {}", e);
                1
            }
        }
    }

    #[cfg(not(feature = "mcp"))]
    {
        let _ = root;
        eprintln!("MCP server requires the 'mcp' feature.");
        eprintln!("Rebuild with: cargo build --features mcp");
        1
    }
}
