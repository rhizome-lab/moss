"""HTTP route generator from MossAPI introspection.

This module generates FastAPI routes from the MossAPI structure.
Each sub-API becomes a router, and each method becomes an endpoint.

Example generated structure:
    POST /skeleton/extract
    POST /skeleton/format
    POST /anchor/find
    GET /health/check
    GET /health/summarize

Usage:
    # Generate OpenAPI spec
    from moss_orchestration.gen.http import generate_openapi
    spec = generate_openapi()

    # Execute API method via HTTP-style call
    from moss_orchestration.gen.http import HTTPExecutor
    executor = HTTPExecutor()
    result = executor.execute("skeleton.extract", {"file_path": "src/main.py"})

    # Generate FastAPI app
    from moss_orchestration.gen.http import HTTPGenerator
    generator = HTTPGenerator()
    app = generator.generate_app()
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from moss_orchestration.gen.base import LazyAPIExecutor
from moss_orchestration.gen.introspect import APIMethod, APIParameter, SubAPI, introspect_api
from moss_orchestration.gen.serialize import serialize

# Import Request at module level for type resolution with PEP 563 annotations.
# FastAPI uses get_type_hints() which looks in module globals.
try:
    from starlette.requests import Request
except ImportError:
    Request = None  # type: ignore[misc,assignment]


@dataclass
class HTTPEndpoint:
    """A generated HTTP endpoint.

    Attributes:
        path: URL path (e.g., "/skeleton/extract")
        method: HTTP method (GET, POST, etc.)
        description: OpenAPI description
        request_body: Request body schema (for POST)
        response_model: Response model name
        api_path: Path to API method (e.g., "skeleton.extract")
        parameters: Query/path parameters
    """

    path: str
    method: str = "POST"
    description: str = ""
    request_body: dict[str, Any] | None = None
    response_model: str = "Any"
    api_path: str = ""
    parameters: list[HTTPParameter] = field(default_factory=list)


@dataclass
class HTTPParameter:
    """A parameter for an HTTP endpoint.

    Attributes:
        name: Parameter name
        type: Python type string
        required: Whether required
        default: Default value
        description: OpenAPI description
        in_: Where the parameter appears (query, path, header)
    """

    name: str
    type: str = "str"
    required: bool = True
    default: Any = None
    description: str = ""
    in_: str = "query"


@dataclass
class HTTPRouter:
    """A router containing related endpoints.

    Attributes:
        prefix: URL prefix (e.g., "/skeleton")
        tag: OpenAPI tag
        description: Router description
        endpoints: List of endpoints
    """

    prefix: str
    tag: str
    description: str = ""
    endpoints: list[HTTPEndpoint] = field(default_factory=list)


class HTTPExecutor(LazyAPIExecutor):
    """Executor for HTTP API calls.

    Provides a clean interface for executing MossAPI methods via HTTP-style
    calls. Handles parameter passing, error handling, and result serialization.

    Usage:
        executor = HTTPExecutor()
        result = executor.execute("skeleton.extract", {"file_path": "src/main.py"})
    """

    def execute(
        self,
        api_path: str,
        arguments: dict[str, Any] | None = None,
        *,
        root: str | Path | None = None,
    ) -> Any:
        """Execute an API method and return serialized result.

        Args:
            api_path: Path to API method (e.g., "skeleton.extract")
            arguments: Method arguments as dict
            root: Override project root for this call

        Returns:
            Serialized result (JSON-compatible dict/list/primitive)

        Raises:
            ValueError: If api_path is invalid
            FileNotFoundError: If referenced file doesn't exist
        """
        parts = api_path.split(".")
        if len(parts) != 2:
            raise ValueError(f"Invalid API path: {api_path}. Expected 'subapi.method'")

        subapi_name, method_name = parts
        arguments = arguments or {}

        # Handle root override
        if root is not None:
            from moss import MossAPI

            api = MossAPI.for_project(Path(root).resolve())
        else:
            api = self.api

        # Get sub-API
        subapi = getattr(api, subapi_name, None)
        if subapi is None:
            raise ValueError(f"Unknown sub-API: {subapi_name}")

        # Get method
        method = getattr(subapi, method_name, None)
        if method is None:
            raise ValueError(f"Unknown method: {subapi_name}.{method_name}")

        # Execute and serialize result
        result = method(**arguments)
        return serialize(result)

    def execute_raw(
        self,
        api_path: str,
        arguments: dict[str, Any] | None = None,
        *,
        root: str | Path | None = None,
    ) -> Any:
        """Execute an API method and return raw (unserialized) result.

        Args:
            api_path: Path to API method (e.g., "skeleton.extract")
            arguments: Method arguments as dict
            root: Override project root for this call

        Returns:
            Raw result from API method
        """
        parts = api_path.split(".")
        if len(parts) != 2:
            raise ValueError(f"Invalid API path: {api_path}. Expected 'subapi.method'")

        subapi_name, method_name = parts
        arguments = arguments or {}

        if root is not None:
            from moss import MossAPI

            api = MossAPI.for_project(Path(root).resolve())
        else:
            api = self.api

        subapi = getattr(api, subapi_name, None)
        if subapi is None:
            raise ValueError(f"Unknown sub-API: {subapi_name}")

        method = getattr(subapi, method_name, None)
        if method is None:
            raise ValueError(f"Unknown method: {subapi_name}.{method_name}")

        return method(**arguments)


def _method_to_http_method(method: APIMethod) -> str:
    """Determine HTTP method based on API method characteristics.

    - Methods starting with "get", "check", "list", "analyze" -> GET
    - Everything else -> POST
    """
    read_prefixes = ("get", "check", "list", "analyze", "find", "resolve", "summarize")
    if method.name.startswith(read_prefixes):
        return "GET"
    return "POST"


def _has_complex_params(method: APIMethod) -> bool:
    """Check if method has parameters that need request body."""
    for param in method.parameters:
        # Path types typically go in request body
        if "Path" in param.type_hint:
            return True
        # List types go in request body
        if param.type_hint.startswith("list["):
            return True
        # Complex objects go in request body
        if param.type_hint not in ("str", "int", "float", "bool"):
            return True
    return False


def _param_to_http(param: APIParameter, use_body: bool = False) -> HTTPParameter:
    """Convert API parameter to HTTP parameter."""
    return HTTPParameter(
        name=param.name,
        type=param.type_hint,
        required=param.required,
        default=param.default,
        description=param.description,
        in_="body" if use_body else "query",
    )


def method_to_endpoint(method: APIMethod, prefix: str) -> HTTPEndpoint:
    """Convert an API method to an HTTP endpoint.

    Args:
        method: The API method to convert
        prefix: URL prefix (e.g., "/skeleton")

    Returns:
        HTTPEndpoint representing the method
    """
    http_method = _method_to_http_method(method)
    use_body = http_method == "POST" or _has_complex_params(method)

    parameters = []
    request_body: dict[str, Any] | None = None

    if use_body and method.parameters:
        # Build request body schema
        properties: dict[str, dict[str, Any]] = {}
        required_fields: list[str] = []

        for param in method.parameters:
            prop_type = "string"
            if param.type_hint == "int":
                prop_type = "integer"
            elif param.type_hint == "float":
                prop_type = "number"
            elif param.type_hint == "bool":
                prop_type = "boolean"
            elif param.type_hint.startswith("list["):
                prop_type = "array"

            properties[param.name] = {
                "type": prop_type,
                "description": param.description,
            }
            if param.default is not None:
                properties[param.name]["default"] = param.default
            if param.required:
                required_fields.append(param.name)

        request_body = {
            "type": "object",
            "properties": properties,
            "required": required_fields,
        }
    else:
        # Use query parameters
        for param in method.parameters:
            parameters.append(_param_to_http(param))

    api_name = prefix.strip("/")
    path = f"{prefix}/{method.name.replace('_', '-')}"

    return HTTPEndpoint(
        path=path,
        method=http_method,
        description=method.description,
        request_body=request_body,
        response_model=method.return_type,
        api_path=f"{api_name}.{method.name}",
        parameters=parameters,
    )


def subapi_to_router(subapi: SubAPI) -> HTTPRouter:
    """Convert a sub-API to an HTTP router.

    Args:
        subapi: The sub-API to convert

    Returns:
        HTTPRouter containing all methods as endpoints
    """
    prefix = f"/{subapi.name}"
    endpoints = [method_to_endpoint(m, prefix) for m in subapi.methods]

    return HTTPRouter(
        prefix=prefix,
        tag=subapi.name,
        description=subapi.description,
        endpoints=endpoints,
    )


class HTTPGenerator:
    """Generator for FastAPI routes from MossAPI.

    Usage:
        generator = HTTPGenerator()

        # Get route structure for custom handling
        routers = generator.generate_routers()

        # Generate FastAPI app (requires fastapi)
        app = generator.generate_app()
    """

    def __init__(self):
        """Initialize the generator."""
        self._routers: list[HTTPRouter] | None = None

    def generate_routers(self) -> list[HTTPRouter]:
        """Generate HTTP routers from MossAPI introspection.

        Returns:
            List of HTTPRouter objects
        """
        if self._routers is None:
            sub_apis = introspect_api()
            self._routers = [subapi_to_router(api) for api in sub_apis]
        return self._routers

    def generate_openapi_paths(self) -> dict[str, Any]:
        """Generate OpenAPI paths specification.

        Returns:
            Dict suitable for OpenAPI spec paths section
        """
        paths: dict[str, Any] = {}

        for router in self.generate_routers():
            for endpoint in router.endpoints:
                path_spec: dict[str, Any] = {
                    "summary": endpoint.description.split(".")[0] if endpoint.description else "",
                    "description": endpoint.description,
                    "tags": [router.tag],
                    "responses": {
                        "200": {
                            "description": "Successful response",
                        }
                    },
                }

                if endpoint.request_body:
                    path_spec["requestBody"] = {
                        "required": True,
                        "content": {
                            "application/json": {
                                "schema": endpoint.request_body,
                            }
                        },
                    }

                if endpoint.parameters:
                    path_spec["parameters"] = [
                        {
                            "name": p.name,
                            "in": p.in_,
                            "required": p.required,
                            "description": p.description,
                            "schema": {"type": "string"},
                        }
                        for p in endpoint.parameters
                    ]

                method_key = endpoint.method.lower()
                if endpoint.path not in paths:
                    paths[endpoint.path] = {}
                paths[endpoint.path][method_key] = path_spec

        return paths

    def generate_openapi_spec(self) -> dict[str, Any]:
        """Generate full OpenAPI specification.

        Returns:
            Complete OpenAPI spec dict
        """
        return {
            "openapi": "3.0.3",
            "info": {
                "title": "Moss API",
                "description": "Headless agent orchestration layer",
                "version": "0.1.0",
            },
            "paths": self.generate_openapi_paths(),
            "tags": [
                {"name": router.tag, "description": router.description}
                for router in self.generate_routers()
            ],
        }

    def generate_app(self, root: str | Path = ".") -> Any:
        """Generate a FastAPI application with all API routes.

        Creates a fully-functional FastAPI app with routes for all MossAPI
        methods. Each route extracts parameters from query strings (GET) or
        JSON body (POST) and returns serialized results.

        Args:
            root: Default project root directory

        Returns:
            FastAPI app instance

        Raises:
            ImportError: If FastAPI is not installed
        """
        try:
            from fastapi import FastAPI
        except ImportError as e:
            raise ImportError("FastAPI is required. Install with: pip install fastapi") from e

        root_path = Path(root).resolve()
        executor = HTTPExecutor(root_path)

        app = FastAPI(
            title="Moss API",
            description="Headless agent orchestration layer",
            version="0.1.0",
        )

        # Health check endpoint
        @app.get("/")
        async def root_endpoint():
            return {
                "name": "moss",
                "version": "0.1.0",
                "root": str(root_path),
                "status": "running",
            }

        @app.get("/health")
        async def health_check():
            return {"status": "healthy", "root": str(root_path)}

        # Generate routes for each endpoint
        for router_spec in self.generate_routers():
            for endpoint in router_spec.endpoints:
                self._register_endpoint(app, endpoint, executor)

        return app

    def _register_endpoint(self, app: Any, endpoint: HTTPEndpoint, executor: HTTPExecutor) -> None:
        """Register an endpoint on the FastAPI app.

        Creates a route handler that:
        1. Extracts parameters from request (query params or JSON body)
        2. Calls the API method via executor
        3. Returns serialized result or appropriate HTTP error
        """
        from fastapi import HTTPException

        tag = endpoint.path.split("/")[1]
        api_path = endpoint.api_path

        if endpoint.method == "GET":

            async def get_handler(request: Request) -> Any:  # type: ignore[valid-type]
                args = dict(request.query_params)
                root_override = args.pop("root", None)
                try:
                    return executor.execute(api_path, args, root=root_override)
                except FileNotFoundError as e:
                    raise HTTPException(status_code=404, detail=str(e)) from None
                except ValueError as e:
                    raise HTTPException(status_code=400, detail=str(e)) from None
                except Exception as e:
                    raise HTTPException(status_code=500, detail=str(e)) from None

            app.get(endpoint.path, summary=endpoint.description, tags=[tag])(get_handler)
        else:

            async def post_handler(request: Request) -> Any:  # type: ignore[valid-type]
                try:
                    body = await request.json()
                except json.JSONDecodeError:
                    body = {}
                args = {**body, **dict(request.query_params)}
                root_override = args.pop("root", None)
                try:
                    return executor.execute(api_path, args, root=root_override)
                except FileNotFoundError as e:
                    raise HTTPException(status_code=404, detail=str(e)) from None
                except ValueError as e:
                    raise HTTPException(status_code=400, detail=str(e)) from None
                except Exception as e:
                    raise HTTPException(status_code=500, detail=str(e)) from None

            app.post(endpoint.path, summary=endpoint.description, tags=[tag])(post_handler)


def generate_http() -> list[HTTPRouter]:
    """Generate HTTP routers from MossAPI.

    Convenience function that creates an HTTPGenerator and returns routers.

    Returns:
        List of HTTPRouter objects
    """
    generator = HTTPGenerator()
    return generator.generate_routers()


def generate_openapi() -> dict[str, Any]:
    """Generate OpenAPI specification from MossAPI.

    Convenience function that creates an HTTPGenerator and returns OpenAPI spec.

    Returns:
        OpenAPI specification dict
    """
    generator = HTTPGenerator()
    return generator.generate_openapi_spec()


__all__ = [
    "HTTPEndpoint",
    "HTTPExecutor",
    "HTTPGenerator",
    "HTTPParameter",
    "HTTPRouter",
    "generate_http",
    "generate_openapi",
    "method_to_endpoint",
    "subapi_to_router",
]
