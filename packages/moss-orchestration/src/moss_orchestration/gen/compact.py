"""Compact tool encoding for token-efficient LLM integration.

This module generates minimal tool signatures for LLM consumption,
bypassing JSON Schema overhead. Use when you control both sides
(e.g., moss agent loop) and don't need full schema validation.

Token savings: ~42KB (10K tokens) → ~2KB (500 tokens) for 85 tools.
"""

from __future__ import annotations

from moss_orchestration.gen.introspect import SubAPI, introspect_api


def _format_param(name: str, type_hint: str, required: bool) -> str:
    """Format a parameter as compact string."""
    # Normalize path-like types
    if "Path" in type_hint or "pathlib" in type_hint:
        type_hint = "path"

    # Simplify common types
    simple_types = {
        "str": "",  # str is default, no annotation needed
        "int": "int",
        "bool": "bool",
        "float": "float",
        "path": "",  # file paths are obvious from name
        "str | path": "",
        "list[str]": "list",
        "dict[str, Any]": "dict",
        "Any": "",
    }

    type_str = simple_types.get(type_hint, type_hint)

    if not required:
        return f"{name}?"
    elif type_str:
        return f"{name}:{type_str}"
    else:
        return name


def _format_return(return_type: str) -> str:
    """Format return type as compact string."""
    # Skip common/uninformative return types
    if return_type in ("None", "Any", "dict[str, Any]"):
        return ""

    # Simplify common types
    simple = {
        "list[str]": "list",
        "list[dict[str, Any]]": "list",
        "str | None": "str?",
    }

    return simple.get(return_type, return_type)


def format_tool_compact(api_name: str, method_name: str, params: list, return_type: str) -> str:
    """Format a single tool as compact signature.

    Format: api.method(param1, param2?) -> ReturnType
    """
    param_strs = []
    for p in params:
        param_strs.append(_format_param(p.name, p.type_hint, p.required))

    sig = f"{api_name}.{method_name}({', '.join(param_strs)})"

    ret = _format_return(return_type)
    if ret:
        sig += f" -> {ret}"

    return sig


def format_subapi_compact(subapi: SubAPI) -> list[str]:
    """Format all methods in a sub-API as compact signatures."""
    lines = []
    for method in subapi.methods:
        sig = format_tool_compact(subapi.name, method.name, method.parameters, method.return_type)
        lines.append(sig)
    return lines


def generate_compact_tools(include_descriptions: bool = False, minimal: bool = False) -> str:
    """Generate compact tool listing for LLM consumption.

    Args:
        include_descriptions: If True, add one-line description after signature
        minimal: If True, use ultra-minimal format (just tool names with params)

    Returns:
        Compact text listing of all tools
    """
    apis = introspect_api()
    lines = []

    for api in apis:
        for method in api.methods:
            if minimal:
                # Ultra-minimal: skeleton.format(file) instead of full signature
                req_params = [p.name.split("_")[0] for p in method.parameters if p.required]
                opt_count = sum(1 for p in method.parameters if not p.required)
                params_str = ", ".join(req_params)
                if opt_count:
                    params_str += f", ...{opt_count}" if req_params else f"...{opt_count}"
                sig = f"{api.name}.{method.name}({params_str})"
            else:
                sig = format_tool_compact(
                    api.name, method.name, method.parameters, method.return_type
                )

            if include_descriptions and method.description:
                # Take first sentence only
                desc = method.description.split(".")[0].strip()
                if desc:
                    sig += f"  # {desc}"

            lines.append(sig)

    return "\n".join(lines)


def generate_compact_by_category() -> dict[str, list[str]]:
    """Generate compact tools grouped by API category.

    Returns:
        Dict mapping API name to list of compact method signatures
    """
    apis = introspect_api()
    result = {}

    for api in apis:
        sigs = []
        for method in api.methods:
            sig = format_tool_compact(api.name, method.name, method.parameters, method.return_type)
            sigs.append(sig)
        if sigs:
            result[api.name] = sigs

    return result


def estimate_tokens(text: str) -> int:
    """Rough token estimate (chars/4)."""
    return len(text) // 4


if __name__ == "__main__":
    # Demo the savings
    import json

    from moss_orchestration.gen.mcp import generate_mcp_definitions

    # Full MCP schema
    full_defs = generate_mcp_definitions()
    full_json = json.dumps(full_defs)

    # Compact formats
    compact = generate_compact_tools(include_descriptions=True)
    compact_no_desc = generate_compact_tools(include_descriptions=False)
    minimal = generate_compact_tools(minimal=True)

    def fmt(name: str, text: str, baseline: int) -> str:
        chars = len(text)
        tokens = estimate_tokens(text)
        pct = 100 * (1 - chars / baseline)
        return f"{name:<20} {chars:>6,} chars (~{tokens:,} tokens) - {pct:.0f}% smaller"

    print("=== Token Comparison ===")
    base = len(full_json)
    print(f"{'Full MCP schema:':<20} {base:>6,} chars (~{estimate_tokens(full_json):,} tokens)")
    print(fmt("Compact + desc:", compact, base))
    print(fmt("Compact (no desc):", compact_no_desc, base))
    print(fmt("Minimal:", minimal, base))
    print()
    print("=== Minimal Format (first 20) ===")
    for line in minimal.split("\n")[:20]:
        print(line)
    print()
    print("=== Compact Format (first 15) ===")
    for line in compact.split("\n")[:15]:
        print(line)
