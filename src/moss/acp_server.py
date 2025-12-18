"""ACP server for Moss - Agent Client Protocol for IDE integration.

This module implements the Agent Client Protocol (ACP), enabling Moss to work
as an AI coding agent inside editors like Zed and JetBrains IDEs.

ACP is the inverse of MCP:
- MCP: LLM connects to moss as a tool provider
- ACP: Editor connects to moss as an AI coding agent

Protocol: JSON-RPC 2.0 over stdio (stdin/stdout)

Spec: https://agentclientprotocol.com
Repo: https://github.com/zed-industries/agent-client-protocol

Usage:
    # Run the server (editor will spawn this process)
    python -m moss.acp_server

    # Or via CLI
    moss acp-server

    # In Zed's settings.json:
    "agent_servers": {
        "moss": {
            "command": "moss",
            "args": ["acp-server"]
        }
    }
"""

from __future__ import annotations

import asyncio
import json
import logging
import sys
import uuid
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from moss import MossAPI

logger = logging.getLogger(__name__)

# Protocol version
ACP_PROTOCOL_VERSION = "0.1.0"


# =============================================================================
# Data Types
# =============================================================================


@dataclass
class AgentInfo:
    """Information about this agent."""

    name: str = "moss"
    version: str = "0.1.0"
    description: str = "Moss - Structural code analysis and AI coding assistant"


@dataclass
class AgentCapabilities:
    """Capabilities this agent supports."""

    streaming: bool = True
    tool_calls: bool = True
    multi_file_edit: bool = True
    terminal: bool = True


@dataclass
class Session:
    """An active coding session."""

    id: str
    working_directory: Path
    mcp_servers: list[dict[str, Any]] = field(default_factory=list)
    mode: str = "default"


@dataclass
class JsonRpcRequest:
    """A JSON-RPC 2.0 request."""

    method: str
    params: dict[str, Any] | None = None
    id: str | int | None = None
    jsonrpc: str = "2.0"


@dataclass
class JsonRpcResponse:
    """A JSON-RPC 2.0 response."""

    id: str | int | None
    result: Any = None
    error: dict[str, Any] | None = None
    jsonrpc: str = "2.0"

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        d: dict[str, Any] = {"jsonrpc": self.jsonrpc, "id": self.id}
        if self.error is not None:
            d["error"] = self.error
        else:
            d["result"] = self.result
        return d


@dataclass
class JsonRpcNotification:
    """A JSON-RPC 2.0 notification (no id, no response expected)."""

    method: str
    params: dict[str, Any] | None = None
    jsonrpc: str = "2.0"

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for JSON serialization."""
        d: dict[str, Any] = {"jsonrpc": self.jsonrpc, "method": self.method}
        if self.params:
            d["params"] = self.params
        return d


# =============================================================================
# ACP Server
# =============================================================================


class ACPServer:
    """Agent Client Protocol server for IDE integration.

    Handles JSON-RPC 2.0 messages over stdio, implementing the ACP protocol
    to work as an AI coding agent inside editors.
    """

    def __init__(self) -> None:
        self.sessions: dict[str, Session] = {}
        self.agent_info = AgentInfo()
        self.capabilities = AgentCapabilities()
        self._request_id = 0
        self._pending_requests: dict[int, asyncio.Future[Any]] = {}
        self._reader: asyncio.StreamReader | None = None
        self._writer: asyncio.StreamWriter | None = None

    # -------------------------------------------------------------------------
    # Protocol Methods (Agent → Client calls these on the client)
    # -------------------------------------------------------------------------

    async def read_file(self, session_id: str, path: str) -> str:
        """Read a file from the client's filesystem."""
        result = await self._call_client(
            "fs/read_text_file",
            {"session_id": session_id, "path": path},
        )
        return result.get("content", "")

    async def write_file(self, session_id: str, path: str, content: str) -> None:
        """Write a file to the client's filesystem."""
        await self._call_client(
            "fs/write_text_file",
            {"session_id": session_id, "path": path, "content": content},
        )

    async def request_permission(
        self,
        session_id: str,
        tool_name: str,
        description: str,
    ) -> bool:
        """Request permission from the user for an operation."""
        result = await self._call_client(
            "session/request_permission",
            {
                "session_id": session_id,
                "tool_call": {"name": tool_name, "description": description},
            },
        )
        return result.get("outcome") == "approved"

    async def create_terminal(
        self,
        session_id: str,
        command: str,
        args: list[str] | None = None,
    ) -> str:
        """Create a terminal and run a command."""
        result = await self._call_client(
            "terminal/create",
            {
                "session_id": session_id,
                "command": command,
                "args": args or [],
            },
        )
        return result.get("terminal_id", "")

    async def get_terminal_output(self, session_id: str, terminal_id: str) -> str:
        """Get output from a terminal."""
        result = await self._call_client(
            "terminal/output",
            {"session_id": session_id, "terminal_id": terminal_id},
        )
        return result.get("output", "")

    async def send_update(
        self,
        session_id: str,
        content: str | None = None,
        thought: str | None = None,
        tool_call: dict[str, Any] | None = None,
    ) -> None:
        """Send a session update notification to the client."""
        params: dict[str, Any] = {"session_id": session_id}

        if content:
            params["message_chunk"] = {"content": content}
        if thought:
            params["thought"] = {"content": thought}
        if tool_call:
            params["tool_call"] = tool_call

        await self._send_notification("session/update", params)

    # -------------------------------------------------------------------------
    # Handler Methods (Client → Agent, we implement these)
    # -------------------------------------------------------------------------

    async def handle_initialize(self, params: dict[str, Any]) -> dict[str, Any]:
        """Handle initialize request - establish connection."""
        logger.info("Initialize request from client: %s", params.get("client_info"))
        return {
            "protocol_version": ACP_PROTOCOL_VERSION,
            "agent_info": {
                "name": self.agent_info.name,
                "version": self.agent_info.version,
                "description": self.agent_info.description,
            },
            "capabilities": {
                "streaming": self.capabilities.streaming,
                "tool_calls": self.capabilities.tool_calls,
                "multi_file_edit": self.capabilities.multi_file_edit,
                "terminal": self.capabilities.terminal,
            },
            "authentication_methods": [],  # No auth required
        }

    async def handle_authenticate(self, params: dict[str, Any]) -> dict[str, Any]:
        """Handle authenticate request."""
        # No authentication required for now
        return {}

    async def handle_session_new(self, params: dict[str, Any]) -> dict[str, Any]:
        """Handle session/new - create a new conversation session."""
        session_id = str(uuid.uuid4())
        working_dir = Path(params.get("working_directory", ".")).resolve()
        mcp_servers = params.get("mcp_servers", [])

        session = Session(
            id=session_id,
            working_directory=working_dir,
            mcp_servers=mcp_servers,
        )
        self.sessions[session_id] = session

        logger.info("New session %s in %s", session_id, working_dir)
        return {
            "session_id": session_id,
            "mode_state": {"id": "default", "name": "Default"},
        }

    async def handle_session_load(self, params: dict[str, Any]) -> dict[str, Any]:
        """Handle session/load - resume an existing session."""
        session_id = params.get("session_id", "")
        if session_id not in self.sessions:
            raise ValueError(f"Session not found: {session_id}")

        return {"mode_state": {"id": "default", "name": "Default"}}

    async def handle_session_set_mode(self, params: dict[str, Any]) -> dict[str, Any]:
        """Handle session/set_mode - change session mode."""
        session_id = params.get("session_id", "")
        mode_id = params.get("mode_id", "default")

        if session_id in self.sessions:
            self.sessions[session_id].mode = mode_id

        return {}

    async def handle_session_prompt(self, params: dict[str, Any]) -> dict[str, Any]:
        """Handle session/prompt - process a user prompt.

        This is the main entry point for AI interactions. The user sends a prompt,
        and we process it, potentially making tool calls and streaming updates.
        """
        session_id = params.get("session_id", "")
        content_blocks = params.get("content", [])

        session = self.sessions.get(session_id)
        if not session:
            raise ValueError(f"Session not found: {session_id}")

        # Extract text from content blocks
        prompt_text = ""
        for block in content_blocks:
            if block.get("type") == "text":
                prompt_text += block.get("text", "")

        logger.info("Prompt in session %s: %s", session_id, prompt_text[:100])

        # Process the prompt using moss tools
        await self._process_prompt(session, prompt_text)

        return {"stop_reason": "end_turn"}

    async def _process_prompt(self, session: Session, prompt: str) -> None:
        """Process a prompt using moss capabilities.

        Uses semantic routing to match prompts to moss tools:
        - skeleton: Extract code structure
        - deps/dependencies: Analyze imports
        - complexity: Cyclomatic complexity
        - health: Project health check
        - patterns: Architectural patterns
        - weaknesses: Architectural gaps
        - security: Security analysis
        """
        import re

        from moss import MossAPI

        await self.send_update(
            session.id,
            thought=f"Analyzing request in {session.working_directory}...",
        )

        try:
            api = MossAPI(root=session.working_directory)
            prompt_lower = prompt.lower()

            # Extract file paths from prompt (e.g., "skeleton of src/main.py")
            file_pattern = r"(?:of|for|in)\s+([^\s\"']+\.py)"
            file_match = re.search(file_pattern, prompt)
            target_file = file_match.group(1) if file_match else None

            # Route based on prompt content
            if any(word in prompt_lower for word in ["overview", "structure", "describe"]):
                await self._handle_overview(session, api)

            elif "skeleton" in prompt_lower:
                await self._handle_skeleton(session, api, target_file)

            elif any(word in prompt_lower for word in ["depend", "import", "deps"]):
                await self._handle_dependencies(session, api, target_file)

            elif "complex" in prompt_lower:
                await self._handle_complexity(session, api, target_file)

            elif "health" in prompt_lower:
                await self._handle_health(session, api)

            elif "pattern" in prompt_lower:
                await self._handle_patterns(session, api)

            elif any(word in prompt_lower for word in ["weakness", "gap", "issue", "problem"]):
                await self._handle_weaknesses(session, api)

            elif any(word in prompt_lower for word in ["security", "vulnerab", "safe"]):
                await self._handle_security(session, api)

            else:
                # Use DWIM for semantic routing
                await self._handle_semantic(session, api, prompt)

        except Exception as e:
            logger.error("Error processing prompt: %s", e)
            await self.send_update(session.id, content=f"Error: {e}")

    async def _handle_overview(self, session: Session, api: MossAPI) -> None:
        """Handle overview/structure requests."""
        await self.send_update(session.id, thought="Getting project overview...")
        try:
            summary = api.health.summarize()
            content = "## Project Overview\n\n"
            content += f"**{summary.root.name}**\n\n"
            content += f"- Modules: {len(summary.modules)}\n"
            for mod in list(summary.modules.values())[:10]:
                content += f"  - `{mod.path}` ({len(mod.functions)} functions)\n"
            if len(summary.modules) > 10:
                content += f"  - ... and {len(summary.modules) - 10} more\n"
            await self.send_update(session.id, content=content)
        except Exception as e:
            await self.send_update(session.id, content=f"Overview unavailable: {e}")

    async def _handle_skeleton(
        self, session: Session, api: MossAPI, target_file: str | None
    ) -> None:
        """Handle skeleton extraction requests."""
        if not target_file:
            await self.send_update(
                session.id,
                content="Please specify a file, e.g., 'Show skeleton of src/main.py'",
            )
            return

        await self.send_update(session.id, thought=f"Extracting skeleton of {target_file}...")
        try:
            skeleton = api.skeleton.extract(target_file)
            content = f"## Skeleton: {target_file}\n\n```python\n{skeleton}\n```"
            await self.send_update(session.id, content=content)
        except Exception as e:
            await self.send_update(session.id, content=f"Skeleton failed: {e}")

    async def _handle_dependencies(
        self, session: Session, api: MossAPI, target_file: str | None
    ) -> None:
        """Handle dependency analysis requests."""
        await self.send_update(session.id, thought="Analyzing dependencies...")
        try:
            if target_file:
                deps = api.dependencies.analyze(target_file)
                content = f"## Dependencies: {target_file}\n\n"
                content += f"**Imports:** {', '.join(deps.imports[:10])}\n"
                if len(deps.imports) > 10:
                    content += f"  ... and {len(deps.imports) - 10} more\n"
            else:
                # Get external deps summary
                result = api.external_deps.list_direct()
                content = "## Dependencies\n\n"
                for dep in result[:15]:
                    content += f"- {dep.get('name', 'unknown')}: {dep.get('version', '?')}\n"
                if len(result) > 15:
                    content += f"\n... and {len(result) - 15} more"
            await self.send_update(session.id, content=content)
        except Exception as e:
            await self.send_update(session.id, content=f"Dependency analysis failed: {e}")

    async def _handle_complexity(
        self, session: Session, api: MossAPI, target_file: str | None
    ) -> None:
        """Handle complexity analysis requests."""
        await self.send_update(session.id, thought="Analyzing complexity...")
        try:
            if target_file:
                result = api.complexity.analyze_file(target_file)
            else:
                result = api.complexity.analyze_directory(".")

            content = "## Complexity Analysis\n\n"
            # Sort by complexity descending
            sorted_funcs = sorted(result.functions, key=lambda f: f.complexity, reverse=True)
            for func in sorted_funcs[:10]:
                content += f"- `{func.name}`: {func.complexity} (line {func.line})\n"
            if len(sorted_funcs) > 10:
                content += f"\n... and {len(sorted_funcs) - 10} more functions"
            await self.send_update(session.id, content=content)
        except Exception as e:
            await self.send_update(session.id, content=f"Complexity analysis failed: {e}")

    async def _handle_health(self, session: Session, api: MossAPI) -> None:
        """Handle health check requests."""
        await self.send_update(session.id, thought="Checking project health...")
        try:
            status = api.health.check()
            content = "## Project Health\n\n"
            content += f"**Grade:** {status.health_grade}\n"
            content += f"**Score:** {status.health_score}/100\n\n"
            content += f"- Python files: {status.file_count}\n"
            content += f"- Total lines: {status.line_count:,}\n"
            content += f"- Has tests: {'Yes' if status.has_tests else 'No'}\n"
            await self.send_update(session.id, content=content)
        except Exception as e:
            await self.send_update(session.id, content=f"Health check failed: {e}")

    async def _handle_patterns(self, session: Session, api: MossAPI) -> None:
        """Handle pattern detection requests."""
        from moss.patterns import analyze_patterns

        await self.send_update(session.id, thought="Detecting architectural patterns...")
        try:
            analysis = analyze_patterns(session.working_directory)
            content = "## Architectural Patterns\n\n"
            content += f"- Plugin systems: {len(analysis.plugin_systems)}\n"
            content += f"- Factories: {len(analysis.factories)}\n"
            content += f"- Strategies: {len(analysis.strategies)}\n"
            content += f"- Total: {len(analysis.patterns)}\n"
            await self.send_update(session.id, content=content)
        except Exception as e:
            await self.send_update(session.id, content=f"Pattern detection failed: {e}")

    async def _handle_weaknesses(self, session: Session, api: MossAPI) -> None:
        """Handle weakness detection requests."""
        await self.send_update(session.id, thought="Analyzing architectural weaknesses...")
        try:
            analysis = api.weaknesses.analyze()
            content = "## Architectural Weaknesses\n\n"
            content += f"**Total:** {len(analysis.weaknesses)}\n\n"
            by_sev = analysis.by_severity
            from moss.weaknesses import Severity

            content += f"- High: {len(by_sev.get(Severity.HIGH, []))}\n"
            content += f"- Medium: {len(by_sev.get(Severity.MEDIUM, []))}\n"
            content += f"- Low: {len(by_sev.get(Severity.LOW, []))}\n"
            await self.send_update(session.id, content=content)
        except Exception as e:
            await self.send_update(session.id, content=f"Weakness analysis failed: {e}")

    async def _handle_security(self, session: Session, api: MossAPI) -> None:
        """Handle security analysis requests."""
        await self.send_update(session.id, thought="Running security analysis...")
        try:
            analysis = api.security.analyze()
            content = "## Security Analysis\n\n"
            content += f"**Total findings:** {analysis.total_count}\n"
            content += f"- Critical: {analysis.critical_count}\n"
            content += f"- High: {analysis.high_count}\n"
            content += f"- Medium: {analysis.medium_count}\n"
            content += f"- Low: {analysis.low_count}\n"
            await self.send_update(session.id, content=content)
        except Exception as e:
            await self.send_update(session.id, content=f"Security analysis failed: {e}")

    async def _handle_semantic(self, session: Session, api: MossAPI, prompt: str) -> None:
        """Use DWIM for semantic tool routing."""
        from moss.dwim import analyze_intent

        await self.send_update(session.id, thought="Understanding your request...")

        try:
            matches = analyze_intent(prompt)
            if matches and matches[0].confidence > 0.5:
                best = matches[0]
                content = f"Based on your request, I suggest using `moss {best.tool}`.\n\n"
                content += f"Confidence: {best.confidence:.0%}\n"
                if best.message:
                    content += f"\n{best.message}"
            else:
                content = (
                    "I'm moss, a structural code analysis agent. I can help with:\n\n"
                    "- **overview** - Project structure and summary\n"
                    "- **skeleton <file>** - Extract code structure\n"
                    "- **dependencies** - Analyze imports\n"
                    "- **complexity** - Cyclomatic complexity\n"
                    "- **health** - Project health check\n"
                    "- **patterns** - Architectural patterns\n"
                    "- **weaknesses** - Architectural gaps\n"
                    "- **security** - Security analysis\n"
                )
            await self.send_update(session.id, content=content)
        except Exception as e:
            await self.send_update(session.id, content=f"Could not route request: {e}")

    # -------------------------------------------------------------------------
    # JSON-RPC Transport
    # -------------------------------------------------------------------------

    async def _call_client(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        """Make a JSON-RPC call to the client and wait for response."""
        self._request_id += 1
        request_id = self._request_id

        request = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": request_id,
        }

        future: asyncio.Future[Any] = asyncio.get_event_loop().create_future()
        self._pending_requests[request_id] = future

        await self._send_message(request)

        try:
            result = await asyncio.wait_for(future, timeout=30.0)
            return result
        except TimeoutError:
            del self._pending_requests[request_id]
            raise TimeoutError(f"Client call {method} timed out") from None

    async def _send_notification(self, method: str, params: dict[str, Any]) -> None:
        """Send a notification to the client (no response expected)."""
        notification = JsonRpcNotification(method=method, params=params)
        await self._send_message(notification.to_dict())

    async def _send_message(self, message: dict[str, Any]) -> None:
        """Send a JSON-RPC message to stdout."""
        if self._writer is None:
            # Fallback to sync stdout
            content = json.dumps(message)
            sys.stdout.write(content + "\n")
            sys.stdout.flush()
        else:
            content = json.dumps(message)
            self._writer.write((content + "\n").encode())
            await self._writer.drain()

    async def _handle_message(self, message: dict[str, Any]) -> dict[str, Any] | None:
        """Handle an incoming JSON-RPC message."""
        method = message.get("method")
        params = message.get("params", {})
        msg_id = message.get("id")

        # Check if this is a response to our request
        if "result" in message or "error" in message:
            req_id = message.get("id")
            if req_id in self._pending_requests:
                future = self._pending_requests.pop(req_id)
                if "error" in message:
                    err_msg = message["error"].get("message", "Unknown error")
                    future.set_exception(Exception(err_msg))
                else:
                    future.set_result(message.get("result", {}))
            return None

        # Route to handler
        handlers = {
            "initialize": self.handle_initialize,
            "authenticate": self.handle_authenticate,
            "session/new": self.handle_session_new,
            "session/load": self.handle_session_load,
            "session/set_mode": self.handle_session_set_mode,
            "session/prompt": self.handle_session_prompt,
        }

        handler = handlers.get(method)
        if handler is None:
            return JsonRpcResponse(
                id=msg_id,
                error={"code": -32601, "message": f"Method not found: {method}"},
            ).to_dict()

        try:
            result = await handler(params)
            return JsonRpcResponse(id=msg_id, result=result).to_dict()
        except Exception as e:
            logger.exception("Error handling %s", method)
            return JsonRpcResponse(
                id=msg_id,
                error={"code": -32000, "message": str(e)},
            ).to_dict()

    async def run(self) -> None:
        """Run the ACP server, reading from stdin and writing to stdout."""
        logger.info("Starting ACP server...")

        # Set up async stdin/stdout
        loop = asyncio.get_event_loop()
        self._reader = asyncio.StreamReader()
        protocol = asyncio.StreamReaderProtocol(self._reader)
        await loop.connect_read_pipe(lambda: protocol, sys.stdin)

        transport, _ = await loop.connect_write_pipe(
            asyncio.Protocol,
            sys.stdout,
        )
        self._writer = asyncio.StreamWriter(transport, protocol, self._reader, loop)

        logger.info("ACP server ready")

        # Read and process messages
        buffer = ""
        while True:
            try:
                chunk = await self._reader.read(4096)
                if not chunk:
                    break

                buffer += chunk.decode()

                # Process complete lines (each line is a JSON message)
                while "\n" in buffer:
                    line, buffer = buffer.split("\n", 1)
                    line = line.strip()
                    if not line:
                        continue

                    try:
                        message = json.loads(line)
                        response = await self._handle_message(message)
                        if response:
                            await self._send_message(response)
                    except json.JSONDecodeError as e:
                        logger.error("Invalid JSON: %s", e)

            except Exception as e:
                logger.exception("Error in message loop: %s", e)
                break

        logger.info("ACP server shutting down")


# =============================================================================
# Entry Point
# =============================================================================


def run_acp_server() -> None:
    """Run the ACP server (blocking)."""
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
        stream=sys.stderr,  # Log to stderr, not stdout (stdout is for JSON-RPC)
    )

    server = ACPServer()
    asyncio.run(server.run())


if __name__ == "__main__":
    run_acp_server()
