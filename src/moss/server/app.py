"""Moss Server application.

This module provides the FastAPI application for serving MossAPI
over HTTP with WebSocket support for streaming.
"""

from __future__ import annotations

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from moss.server.state import ServerState


@dataclass
class MossServer:
    """Moss HTTP/WebSocket server.

    Provides:
    - REST API endpoints for all MossAPI operations
    - WebSocket endpoint for streaming results
    - Cache management endpoints
    - Health check endpoint
    """

    root: Path
    state: ServerState | None = None

    def __post_init__(self):
        """Initialize the server."""
        self.root = Path(self.root).resolve()
        if self.state is None:
            self.state = ServerState(root=self.root)


def create_app(root: str | Path = ".") -> Any:
    """Create a FastAPI application for Moss.

    Args:
        root: Project root directory

    Returns:
        FastAPI application instance

    Raises:
        ImportError: If FastAPI is not installed
    """
    try:
        from fastapi import FastAPI, HTTPException, WebSocket, WebSocketDisconnect
        from fastapi.middleware.cors import CORSMiddleware
    except ImportError as e:
        raise ImportError(
            "FastAPI is required for the server. Install with: pip install 'moss[server]'"
        ) from e

    root_path = Path(root).resolve()
    state = ServerState(root=root_path)

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        """Manage application lifespan."""
        # Startup
        app.state.moss_state = state
        yield
        # Shutdown
        state.invalidate()

    app = FastAPI(
        title="Moss API Server",
        description="Headless agent orchestration layer",
        version="0.1.0",
        lifespan=lifespan,
    )

    # Add CORS middleware
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    # ==========================================================================
    # Health & Status Endpoints
    # ==========================================================================

    @app.get("/")
    async def root_endpoint():
        """Server status endpoint."""
        return {
            "name": "moss",
            "version": "0.1.0",
            "root": str(root_path),
            "status": "running",
        }

    @app.get("/health")
    async def health_check():
        """Health check endpoint."""
        return {"status": "healthy", "root": str(root_path)}

    @app.get("/cache/stats")
    async def cache_stats():
        """Get cache statistics."""
        return state.stats()

    @app.post("/cache/invalidate")
    async def invalidate_cache(pattern: str | None = None):
        """Invalidate cache entries."""
        count = state.invalidate(pattern)
        return {"invalidated": count}

    # ==========================================================================
    # Skeleton API Endpoints
    # ==========================================================================

    @app.post("/skeleton/extract")
    async def skeleton_extract(file_path: str):
        """Extract code skeleton from a file."""
        try:
            result = await state.execute_cached(
                "skeleton.extract",
                lambda: state.api.skeleton.extract(file_path),
                file_path=file_path,
            )
            # Convert Symbol objects to dicts for JSON
            return {"symbols": [_symbol_to_dict(s) for s in result]}
        except FileNotFoundError:
            raise HTTPException(status_code=404, detail=f"File not found: {file_path}") from None
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e)) from e

    @app.post("/skeleton/format")
    async def skeleton_format(file_path: str, show_bodies: bool = False):
        """Format skeleton as readable text."""
        try:
            result = await state.execute_cached(
                "skeleton.format",
                lambda: state.api.skeleton.format(file_path, show_bodies=show_bodies),
                file_path=file_path,
                show_bodies=show_bodies,
            )
            return {"content": result}
        except FileNotFoundError:
            raise HTTPException(status_code=404, detail=f"File not found: {file_path}") from None
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e)) from e

    # ==========================================================================
    # Anchor API Endpoints
    # ==========================================================================

    @app.post("/anchor/find")
    async def anchor_find(
        file_path: str,
        name: str,
        anchor_type: str = "function",
    ):
        """Find anchors in a file."""
        try:
            result = state.api.anchor.find(file_path, name, anchor_type)
            return {"matches": [_anchor_match_to_dict(m) for m in result]}
        except FileNotFoundError:
            raise HTTPException(status_code=404, detail=f"File not found: {file_path}") from None
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e)) from e

    @app.post("/anchor/resolve")
    async def anchor_resolve(
        file_path: str,
        name: str,
        anchor_type: str = "function",
    ):
        """Resolve a single anchor."""
        try:
            result = state.api.anchor.resolve(file_path, name, anchor_type)
            return _anchor_match_to_dict(result)
        except FileNotFoundError:
            raise HTTPException(status_code=404, detail=f"File not found: {file_path}") from None
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e)) from e

    # ==========================================================================
    # Dependencies API Endpoints
    # ==========================================================================

    @app.post("/dependencies/extract")
    async def dependencies_extract(file_path: str):
        """Extract dependencies from a file."""
        try:
            result = await state.execute_cached(
                "dependencies.extract",
                lambda: state.api.dependencies.extract(file_path),
                file_path=file_path,
            )
            return _dep_info_to_dict(result)
        except FileNotFoundError:
            raise HTTPException(status_code=404, detail=f"File not found: {file_path}") from None
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e)) from e

    # ==========================================================================
    # CFG API Endpoints
    # ==========================================================================

    @app.post("/cfg/build")
    async def cfg_build(file_path: str, function_name: str | None = None):
        """Build control flow graph."""
        try:
            result = await state.execute_cached(
                "cfg.build",
                lambda: state.api.cfg.build(file_path, function_name),
                file_path=file_path,
                function_name=function_name,
            )
            return {"cfgs": [_cfg_to_dict(c) for c in result]}
        except FileNotFoundError:
            raise HTTPException(status_code=404, detail=f"File not found: {file_path}") from None
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e)) from e

    # ==========================================================================
    # Health API Endpoints
    # ==========================================================================

    @app.get("/project/health")
    async def project_health():
        """Get project health status."""
        try:
            result = state.api.health.check()
            return _status_to_dict(result)
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e)) from e

    @app.get("/project/summary")
    async def project_summary():
        """Get project summary."""
        try:
            result = state.api.health.summarize()
            return _summary_to_dict(result)
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e)) from e

    # ==========================================================================
    # Validation API Endpoints
    # ==========================================================================

    @app.post("/validation/validate")
    async def validation_validate(file_path: str):
        """Validate a file."""
        try:
            result = state.api.validation.validate(file_path)
            return _validation_result_to_dict(result)
        except FileNotFoundError:
            raise HTTPException(status_code=404, detail=f"File not found: {file_path}") from None
        except Exception as e:
            raise HTTPException(status_code=500, detail=str(e)) from e

    # ==========================================================================
    # WebSocket Endpoint for Streaming
    # ==========================================================================

    @app.websocket("/ws")
    async def websocket_endpoint(websocket: WebSocket):
        """WebSocket endpoint for streaming operations."""
        await websocket.accept()
        try:
            while True:
                data = await websocket.receive_json()
                operation = data.get("operation")
                args = data.get("args", {})

                try:
                    result = await _execute_operation(state.api, operation, args)
                    await websocket.send_json(
                        {
                            "status": "success",
                            "operation": operation,
                            "result": result,
                        }
                    )
                except Exception as e:
                    await websocket.send_json(
                        {
                            "status": "error",
                            "operation": operation,
                            "error": str(e),
                        }
                    )
        except WebSocketDisconnect:
            pass

    return app


async def _execute_operation(api: Any, operation: str, args: dict[str, Any]) -> Any:
    """Execute an API operation by name.

    Args:
        api: MossAPI instance
        operation: Operation path (e.g., "skeleton.extract")
        args: Operation arguments

    Returns:
        Operation result (converted to JSON-serializable form)
    """
    parts = operation.split(".")
    if len(parts) != 2:
        raise ValueError(f"Invalid operation: {operation}")

    subapi_name, method_name = parts
    subapi = getattr(api, subapi_name, None)
    if subapi is None:
        raise ValueError(f"Unknown API: {subapi_name}")

    method = getattr(subapi, method_name, None)
    if method is None:
        raise ValueError(f"Unknown method: {method_name}")

    result = method(**args)

    # Convert result to serializable form based on operation
    if operation.startswith("skeleton."):
        if isinstance(result, list):
            return [_symbol_to_dict(s) for s in result]
        return result
    elif operation.startswith("health."):
        if hasattr(result, "health_grade"):
            return _status_to_dict(result)
        return result

    return result


# =============================================================================
# Serialization Helpers
# =============================================================================


def _symbol_to_dict(symbol: Any) -> dict[str, Any]:
    """Convert Symbol to dict."""
    return {
        "name": symbol.name,
        "kind": symbol.kind.name if hasattr(symbol.kind, "name") else str(symbol.kind),
        "line": symbol.line,
        "docstring": symbol.docstring,
        "signature": symbol.signature,
        "children": [_symbol_to_dict(c) for c in (symbol.children or [])],
    }


def _anchor_match_to_dict(match: Any) -> dict[str, Any]:
    """Convert AnchorMatch to dict."""
    return {
        "name": match.name,
        "line": match.line,
        "column": match.column,
        "end_line": match.end_line,
        "end_column": match.end_column,
        "confidence": match.confidence,
    }


def _dep_info_to_dict(info: Any) -> dict[str, Any]:
    """Convert DependencyInfo to dict."""
    return {
        "imports": [{"module": i.module, "name": i.name, "alias": i.alias} for i in info.imports],
        "exports": [{"name": e.name, "kind": e.kind} for e in info.exports],
    }


def _cfg_to_dict(cfg: Any) -> dict[str, Any]:
    """Convert ControlFlowGraph to dict."""
    return {
        "name": cfg.name,
        "entry_id": cfg.entry_id,
        "exit_id": cfg.exit_id,
        "nodes": [
            {
                "id": n.id,
                "type": n.type.name if hasattr(n.type, "name") else str(n.type),
                "label": n.label,
            }
            for n in cfg.nodes
        ],
        "edges": [
            {
                "from_id": e.from_id,
                "to_id": e.to_id,
                "type": e.type.name if hasattr(e.type, "name") else str(e.type),
            }
            for e in cfg.edges
        ],
    }


def _status_to_dict(status: Any) -> dict[str, Any]:
    """Convert ProjectStatus to dict."""
    return {
        "health_score": status.health_score,
        "health_grade": status.health_grade,
        "categories": {
            name: {
                "score": cat.score,
                "max_score": cat.max_score,
                "issues": cat.issues,
            }
            for name, cat in status.categories.items()
        },
    }


def _summary_to_dict(summary: Any) -> dict[str, Any]:
    """Convert ProjectSummary to dict."""
    return {
        "name": summary.name,
        "total_modules": summary.total_modules,
        "total_functions": summary.total_functions,
        "total_classes": summary.total_classes,
        "modules": [
            {
                "name": m.name,
                "path": str(m.path),
                "functions": len(m.functions),
                "classes": len(m.classes),
            }
            for m in summary.modules
        ],
    }


def _validation_result_to_dict(result: Any) -> dict[str, Any]:
    """Convert ValidationResult to dict."""
    return {
        "success": result.success,
        "issues": [
            {
                "message": i.message,
                "severity": i.severity.name,
                "file": str(i.file) if i.file else None,
                "line": i.line,
                "column": i.column,
                "code": i.code,
            }
            for i in result.issues
        ],
        "error_count": result.error_count,
        "warning_count": result.warning_count,
    }


def run_server(
    root: str | Path = ".",
    host: str = "127.0.0.1",
    port: int = 8000,
    **kwargs: Any,
) -> None:
    """Run the Moss server.

    Args:
        root: Project root directory
        host: Host to bind to
        port: Port to bind to
        **kwargs: Additional uvicorn arguments
    """
    try:
        import uvicorn
    except ImportError as e:
        raise ImportError(
            "Uvicorn is required for the server. Install with: pip install 'moss[server]'"
        ) from e

    app = create_app(root)
    uvicorn.run(app, host=host, port=port, **kwargs)


__all__ = [
    "MossServer",
    "create_app",
    "run_server",
]
