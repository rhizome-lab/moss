"""gRPC generator from MossAPI introspection.

This module generates gRPC protocol buffer definitions and servicer
implementations from the MossAPI structure.

Usage:
    from moss.gen.grpc import GRPCGenerator

    # Generate proto file
    generator = GRPCGenerator()
    proto_content = generator.generate_proto()

    # Save proto file
    generator.save_proto("moss_api.proto")

    # After compiling proto (grpc_tools.protoc), create servicer
    servicer = generator.create_servicer()

The generated proto defines:
- MossService with all API methods as RPCs
- Request/Response messages for each method
- Common types (Symbol, AnchorMatch, etc.)

To compile the proto:
    python -m grpc_tools.protoc -I. --python_out=. --grpc_python_out=. moss_api.proto
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from moss.gen.introspect import APIMethod, introspect_api
from moss.gen.serialize import serialize

# Proto type mapping from Python types
PROTO_TYPE_MAP = {
    "str": "string",
    "int": "int64",
    "float": "double",
    "bool": "bool",
    "Path": "string",
    "list[str]": "repeated string",
    "list[int]": "repeated int64",
    "dict": "google.protobuf.Struct",
    "Any": "google.protobuf.Value",
}


@dataclass
class ProtoMessage:
    """A Protocol Buffer message definition."""

    name: str
    fields: list[ProtoField] = field(default_factory=list)


@dataclass
class ProtoField:
    """A field in a Protocol Buffer message."""

    name: str
    proto_type: str
    number: int
    repeated: bool = False
    optional: bool = False


@dataclass
class ProtoRPC:
    """An RPC method in a gRPC service."""

    name: str
    request_type: str
    response_type: str
    description: str = ""


def python_type_to_proto(type_hint: str) -> str:
    """Convert Python type hint to proto type."""
    # Handle common types
    if type_hint in PROTO_TYPE_MAP:
        return PROTO_TYPE_MAP[type_hint]

    # Handle Path variants
    if "Path" in type_hint:
        return "string"

    # Handle list types
    if type_hint.startswith("list["):
        inner = type_hint[5:-1]
        inner_proto = python_type_to_proto(inner)
        return f"repeated {inner_proto}"

    # Handle optional types
    if type_hint.startswith("Optional[") or " | None" in type_hint:
        clean = type_hint.replace("Optional[", "").replace("]", "").replace(" | None", "")
        return python_type_to_proto(clean)

    # Default to string for complex types
    return "string"


def method_to_rpc(method: APIMethod, api_name: str) -> ProtoRPC:
    """Convert an API method to a gRPC RPC definition."""
    # Create PascalCase names
    rpc_name = "".join(word.title() for word in f"{api_name}_{method.name}".split("_"))
    request_type = f"{rpc_name}Request"
    response_type = f"{rpc_name}Response"

    return ProtoRPC(
        name=rpc_name,
        request_type=request_type,
        response_type=response_type,
        description=method.description,
    )


def method_to_messages(method: APIMethod, api_name: str) -> tuple[ProtoMessage, ProtoMessage]:
    """Generate request and response messages for a method."""
    rpc_name = "".join(word.title() for word in f"{api_name}_{method.name}".split("_"))

    # Request message with method parameters
    request_fields = []
    for i, param in enumerate(method.parameters):
        proto_type = python_type_to_proto(param.type_hint)
        repeated = proto_type.startswith("repeated ")
        if repeated:
            proto_type = proto_type[9:]  # Strip "repeated "

        request_fields.append(
            ProtoField(
                name=param.name,
                proto_type=proto_type,
                number=i + 1,
                repeated=repeated,
                optional=not param.required,
            )
        )

    request = ProtoMessage(name=f"{rpc_name}Request", fields=request_fields)

    # Response message - generic JSON result
    response = ProtoMessage(
        name=f"{rpc_name}Response",
        fields=[
            ProtoField(name="success", proto_type="bool", number=1),
            ProtoField(name="result", proto_type="string", number=2),  # JSON-encoded
            ProtoField(name="error", proto_type="string", number=3, optional=True),
        ],
    )

    return request, response


class GRPCGenerator:
    """Generator for gRPC definitions from MossAPI.

    Usage:
        generator = GRPCGenerator()

        # Generate proto content
        proto = generator.generate_proto()

        # Save to file
        generator.save_proto("moss_api.proto")

        # Get RPC definitions
        rpcs = generator.generate_rpcs()
    """

    def __init__(self, root: str | Path = "."):
        """Initialize the generator."""
        self._root = Path(root).resolve()
        self._rpcs: list[ProtoRPC] | None = None
        self._messages: list[ProtoMessage] | None = None

    def generate_rpcs(self) -> list[ProtoRPC]:
        """Generate RPC definitions from MossAPI."""
        if self._rpcs is None:
            self._generate()
        return self._rpcs or []

    def generate_messages(self) -> list[ProtoMessage]:
        """Generate message definitions from MossAPI."""
        if self._messages is None:
            self._generate()
        return self._messages or []

    def _generate(self) -> None:
        """Generate all RPC and message definitions."""
        sub_apis = introspect_api()
        self._rpcs = []
        self._messages = []

        for api in sub_apis:
            for method in api.methods:
                rpc = method_to_rpc(method, api.name)
                self._rpcs.append(rpc)

                request, response = method_to_messages(method, api.name)
                self._messages.append(request)
                self._messages.append(response)

    def generate_proto(self) -> str:
        """Generate Protocol Buffer definition file content."""
        lines = [
            'syntax = "proto3";',
            "",
            "package moss;",
            "",
            "option python_generic_services = true;",
            "",
            "// Generated from MossAPI introspection",
            "// Do not edit manually - regenerate with: moss gen --target=grpc",
            "",
        ]

        # Add service definition
        lines.append("service MossService {")
        for rpc in self.generate_rpcs():
            if rpc.description:
                lines.append(f"  // {rpc.description}")
            lines.append(f"  rpc {rpc.name}({rpc.request_type}) returns ({rpc.response_type});")
        lines.append("}")
        lines.append("")

        # Add message definitions
        for msg in self.generate_messages():
            lines.append(f"message {msg.name} {{")
            for fld in msg.fields:
                optional = "optional " if fld.optional else ""
                repeated = "repeated " if fld.repeated else ""
                lines.append(f"  {optional}{repeated}{fld.proto_type} {fld.name} = {fld.number};")
            lines.append("}")
            lines.append("")

        return "\n".join(lines)

    def save_proto(self, path: str | Path) -> Path:
        """Save proto file to disk.

        Args:
            path: Output path for .proto file

        Returns:
            Absolute path to saved file
        """
        path = Path(path)
        path.write_text(self.generate_proto())
        return path.resolve()

    def generate_servicer_code(self) -> str:
        """Generate Python servicer implementation code.

        This generates a servicer class that can be used with grpcio
        after compiling the proto file.
        """
        lines = [
            '"""Generated MossService servicer implementation.',
            "",
            "This servicer delegates to MossAPI methods.",
            '"""',
            "",
            "from __future__ import annotations",
            "",
            "import json",
            "from pathlib import Path",
            "from typing import Any",
            "",
            "from moss import MossAPI",
            "from moss.gen.serialize import serialize",
            "",
            "# Import generated protobuf modules",
            "# These are created by running:",
            "#   python -m grpc_tools.protoc -I. --python_out=. --grpc_python_out=. moss_api.proto",
            "try:",
            "    import moss_api_pb2 as pb2",
            "    import moss_api_pb2_grpc as pb2_grpc",
            "except ImportError:",
            "    pb2 = None",
            "    pb2_grpc = None",
            "",
            "",
            "class MossServiceServicer:",
            '    """gRPC servicer that delegates to MossAPI."""',
            "",
            "    def __init__(self, root: str | Path = '.'):",
            "        self._root = Path(root).resolve()",
            "        self._api = None",
            "",
            "    @property",
            "    def api(self):",
            '        """Lazy-initialize MossAPI."""',
            "        if self._api is None:",
            "            self._api = MossAPI.for_project(self._root)",
            "        return self._api",
            "",
            "    def _make_response(self, response_class, result=None, error=None):",
            '        """Create a response message."""',
            "        if error:",
            "            return response_class(success=False, error=str(error))",
            "        return response_class(",
            "            success=True,",
            "            result=json.dumps(serialize(result)),",
            "        )",
            "",
        ]

        # Generate method implementations
        sub_apis = introspect_api()
        for api in sub_apis:
            for method in api.methods:
                rpc_name = "".join(word.title() for word in f"{api.name}_{method.name}".split("_"))
                response_class = f"pb2.{rpc_name}Response"

                lines.append(f"    def {rpc_name}(self, request, context):")
                lines.append(f'        """Handle {rpc_name} RPC."""')
                lines.append("        try:")
                lines.append(f"            subapi = self.api.{api.name}")
                lines.append(f"            method = subapi.{method.name}")

                # Build kwargs from request
                if method.parameters:
                    lines.append("            kwargs = {}")
                    for param in method.parameters:
                        lines.append(
                            f"            if request.HasField('{param.name}') "
                            f"if hasattr(request, 'HasField') else request.{param.name}:"
                        )
                        lines.append(
                            f"                kwargs['{param.name}'] = request.{param.name}"
                        )
                    lines.append("            result = method(**kwargs)")
                else:
                    lines.append("            result = method()")

                lines.append(
                    f"            return self._make_response({response_class}, result=result)"
                )
                lines.append("        except Exception as e:")
                lines.append(f"            return self._make_response({response_class}, error=e)")
                lines.append("")

        return "\n".join(lines)


class GRPCExecutor:
    """Executor for gRPC-style calls (for testing without full gRPC setup)."""

    def __init__(self, root: str | Path = "."):
        """Initialize the executor."""
        self._root = Path(root).resolve()
        self._api = None

    @property
    def api(self):
        """Lazy-initialize MossAPI."""
        if self._api is None:
            from moss import MossAPI

            self._api = MossAPI.for_project(self._root)
        return self._api

    def execute(self, rpc_name: str, request: dict[str, Any]) -> dict[str, Any]:
        """Execute an RPC-style call.

        Args:
            rpc_name: PascalCase RPC name (e.g., "SkeletonExtract")
            request: Request parameters as dict

        Returns:
            Response dict with success, result, error fields
        """
        # Parse RPC name to get api and method
        # SkeletonExtract -> skeleton.extract
        import re

        parts = re.findall("[A-Z][a-z]*", rpc_name)
        if len(parts) < 2:
            return {"success": False, "error": f"Invalid RPC name: {rpc_name}"}

        api_name = parts[0].lower()
        method_name = "_".join(p.lower() for p in parts[1:])

        try:
            subapi = getattr(self.api, api_name, None)
            if subapi is None:
                return {"success": False, "error": f"Unknown API: {api_name}"}

            method = getattr(subapi, method_name, None)
            if method is None:
                return {"success": False, "error": f"Unknown method: {method_name}"}

            result = method(**request)
            return {"success": True, "result": serialize(result)}
        except Exception as e:
            return {"success": False, "error": str(e)}


def generate_proto() -> str:
    """Generate proto file content from MossAPI.

    Convenience function.
    """
    generator = GRPCGenerator()
    return generator.generate_proto()


def generate_servicer_code() -> str:
    """Generate servicer implementation code.

    Convenience function.
    """
    generator = GRPCGenerator()
    return generator.generate_servicer_code()


__all__ = [
    "GRPCExecutor",
    "GRPCGenerator",
    "ProtoField",
    "ProtoMessage",
    "ProtoRPC",
    "generate_proto",
    "generate_servicer_code",
    "method_to_messages",
    "method_to_rpc",
    "python_type_to_proto",
]
