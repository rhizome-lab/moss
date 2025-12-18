"""Interface generators for MossAPI.

This module provides automatic interface generation from the MossAPI:
- CLI: Generate argparse commands from API methods
- gRPC: Generate Protocol Buffers and servicer implementations
- HTTP: Generate FastAPI routes from API methods
- LSP: Generate workspace commands for Language Server Protocol
- MCP: Generate Model Context Protocol tools from API methods
- TUI: Generate Textual terminal UI from API methods
- OpenAPI: Generate OpenAPI specification from API
"""

from moss.gen.cli import CLIGenerator, generate_cli
from moss.gen.grpc import GRPCGenerator, generate_proto, generate_servicer_code
from moss.gen.http import HTTPGenerator, generate_http, generate_openapi
from moss.gen.introspect import (
    APIMethod,
    APIParameter,
    SubAPI,
    introspect_api,
)
from moss.gen.lsp import LSPGenerator, generate_lsp_commands, get_command_list
from moss.gen.mcp import MCPGenerator, generate_mcp, generate_mcp_definitions
from moss.gen.tui import TUIGenerator, generate_tui_groups, run_tui

__all__ = [
    "APIMethod",
    "APIParameter",
    "CLIGenerator",
    "GRPCGenerator",
    "HTTPGenerator",
    "LSPGenerator",
    "MCPGenerator",
    "SubAPI",
    "TUIGenerator",
    "generate_cli",
    "generate_http",
    "generate_lsp_commands",
    "generate_mcp",
    "generate_mcp_definitions",
    "generate_openapi",
    "generate_proto",
    "generate_servicer_code",
    "generate_tui_groups",
    "get_command_list",
    "introspect_api",
    "run_tui",
]
