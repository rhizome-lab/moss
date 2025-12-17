"""FastAPI server example for Moss.

This server demonstrates how to expose Moss functionality via a REST API.
Run with: uvicorn examples.server.main:app --reload
"""

from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

from moss.anchors import Anchor, AnchorType, find_anchors
from moss.cfg import build_cfg
from moss.elided_literals import ElisionConfig, elide_literals
from moss.events import EventBus, EventType
from moss.patches import Patch, PatchType, apply_patch, apply_text_patch
from moss.skeleton import extract_python_skeleton, format_skeleton
from moss.validators import SyntaxValidator

# Global event bus for the application
event_bus = EventBus()


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan handler."""
    # Startup
    await event_bus.emit(EventType.PLAN_GENERATED, {"status": "server_started"})
    yield
    # Shutdown
    await event_bus.emit(EventType.PLAN_GENERATED, {"status": "server_stopped"})


app = FastAPI(
    title="Moss API",
    description="Headless agent orchestration layer for AI engineering",
    version="0.1.0",
    lifespan=lifespan,
)

# CORS middleware for development
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


# ============================================================================
# Request/Response Models
# ============================================================================


class SkeletonRequest(BaseModel):
    """Request for skeleton extraction."""

    source: str = Field(..., description="Python source code")


class SkeletonResponse(BaseModel):
    """Response with extracted skeleton."""

    skeleton: str = Field(..., description="Formatted skeleton")
    symbol_count: int = Field(..., description="Number of top-level symbols")


class AnchorRequest(BaseModel):
    """Request for anchor resolution."""

    source: str = Field(..., description="Python source code")
    anchor_type: str = Field(..., description="Type: function, class, method")
    name: str = Field(..., description="Name of the symbol to find")
    context: str | None = Field(None, description="Parent context (e.g., class name)")


class AnchorMatch(BaseModel):
    """A matched anchor location."""

    name: str
    lineno: int
    end_lineno: int
    col_offset: int
    end_col_offset: int


class AnchorResponse(BaseModel):
    """Response with anchor matches."""

    matches: list[AnchorMatch]
    count: int


class PatchRequest(BaseModel):
    """Request for code patching."""

    source: str = Field(..., description="Original source code")
    anchor_type: str = Field(..., description="Type: function, class, method")
    anchor_name: str = Field(..., description="Name of the symbol to patch")
    anchor_context: str | None = Field(None, description="Parent context")
    patch_type: str = Field("replace", description="Type: replace, insert, delete")
    content: str | None = Field(None, description="New content for replace/insert")


class PatchResponse(BaseModel):
    """Response with patched code."""

    success: bool
    patched: str | None = None
    error: str | None = None


class TextPatchRequest(BaseModel):
    """Request for text-based patching."""

    source: str = Field(..., description="Original source code")
    old_text: str = Field(..., description="Text to find and replace")
    new_text: str = Field(..., description="Replacement text")


class CFGRequest(BaseModel):
    """Request for CFG building."""

    source: str = Field(..., description="Python source code")
    function_name: str | None = Field(None, description="Specific function to analyze")
    output_format: str = Field("text", description="Output format: text or dot")


class CFGResponse(BaseModel):
    """Response with CFG representation."""

    cfgs: list[dict[str, Any]]
    count: int


class ElideRequest(BaseModel):
    """Request for literal elision."""

    source: str = Field(..., description="Python source code")
    preserve_docstrings: bool = Field(True, description="Keep docstrings")
    preserve_small_ints: bool = Field(True, description="Keep small integers")


class ElideResponse(BaseModel):
    """Response with elided code."""

    elided: str
    stats: dict[str, int]


class ValidateRequest(BaseModel):
    """Request for code validation."""

    source: str = Field(..., description="Python source code")


class ValidateResponse(BaseModel):
    """Response with validation results."""

    success: bool
    issues: list[dict[str, Any]]


# ============================================================================
# API Endpoints
# ============================================================================


@app.get("/")
async def root():
    """API root endpoint."""
    return {
        "name": "Moss API",
        "version": "0.1.0",
        "docs": "/docs",
    }


@app.get("/health")
async def health():
    """Health check endpoint."""
    return {"status": "healthy"}


@app.post("/skeleton", response_model=SkeletonResponse)
async def extract_skeleton(request: SkeletonRequest):
    """Extract Python skeleton from source code."""
    try:
        symbols = extract_python_skeleton(request.source)
        skeleton = format_skeleton(symbols)
        return SkeletonResponse(skeleton=skeleton, symbol_count=len(symbols))
    except SyntaxError as e:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Syntax error in source: {e}",
        ) from None


@app.post("/anchors", response_model=AnchorResponse)
async def find_anchor(request: AnchorRequest):
    """Find anchors in source code."""
    try:
        anchor_type_map = {
            "function": AnchorType.FUNCTION,
            "class": AnchorType.CLASS,
            "method": AnchorType.METHOD,
        }
        anchor_type = anchor_type_map.get(request.anchor_type.lower())
        if not anchor_type:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail=f"Invalid anchor type: {request.anchor_type}",
            )

        anchor = Anchor(type=anchor_type, name=request.name, context=request.context)
        matches = find_anchors(request.source, anchor)

        return AnchorResponse(
            matches=[
                AnchorMatch(
                    name=m.anchor.name,
                    lineno=m.lineno,
                    end_lineno=m.end_lineno,
                    col_offset=m.col_offset,
                    end_col_offset=m.end_col_offset,
                )
                for m in matches
            ],
            count=len(matches),
        )
    except SyntaxError as e:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST, detail=f"Syntax error: {e}"
        ) from None


@app.post("/patch", response_model=PatchResponse)
async def patch_code(request: PatchRequest):
    """Apply a patch to source code."""
    try:
        anchor_type_map = {
            "function": AnchorType.FUNCTION,
            "class": AnchorType.CLASS,
            "method": AnchorType.METHOD,
        }
        anchor_type = anchor_type_map.get(request.anchor_type.lower())
        if not anchor_type:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail=f"Invalid anchor type: {request.anchor_type}",
            )

        patch_type_map = {
            "replace": PatchType.REPLACE,
            "insert": PatchType.INSERT_AFTER,
            "delete": PatchType.DELETE,
        }
        patch_type = patch_type_map.get(request.patch_type.lower())
        if not patch_type:
            raise HTTPException(
                status_code=status.HTTP_400_BAD_REQUEST,
                detail=f"Invalid patch type: {request.patch_type}",
            )

        anchor = Anchor(
            type=anchor_type,
            name=request.anchor_name,
            context=request.anchor_context,
        )
        patch = Patch(anchor=anchor, patch_type=patch_type, content=request.content or "")
        result = apply_patch(request.source, patch)

        return PatchResponse(success=result.success, patched=result.patched, error=result.error)
    except SyntaxError as e:
        return PatchResponse(success=False, error=f"Syntax error: {e}")


@app.post("/patch/text", response_model=PatchResponse)
async def text_patch(request: TextPatchRequest):
    """Apply a text-based patch to source code."""
    result = apply_text_patch(request.source, request.old_text, request.new_text)
    return PatchResponse(success=result.success, patched=result.patched, error=result.error)


@app.post("/cfg", response_model=CFGResponse)
async def build_control_flow_graph(request: CFGRequest):
    """Build control flow graph from source code."""
    try:
        cfgs = build_cfg(request.source, function_name=request.function_name)

        result = []
        for cfg in cfgs:
            if request.output_format == "dot":
                result.append({"name": cfg.name, "dot": cfg.to_dot()})
            else:
                result.append({"name": cfg.name, "text": cfg.to_text()})

        return CFGResponse(cfgs=result, count=len(cfgs))
    except SyntaxError as e:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST, detail=f"Syntax error: {e}"
        ) from None


@app.post("/elide", response_model=ElideResponse)
async def elide_code_literals(request: ElideRequest):
    """Elide literals from source code to reduce token count."""
    try:
        config = ElisionConfig(
            preserve_docstrings=request.preserve_docstrings,
            preserve_small_ints=request.preserve_small_ints,
        )
        elided, stats = elide_literals(request.source, config)

        return ElideResponse(
            elided=elided,
            stats={
                "strings": stats.strings,
                "numbers": stats.numbers,
                "lists": stats.lists,
                "dicts": stats.dicts,
                "f_strings": stats.f_strings,
                "total": stats.total,
            },
        )
    except Exception as e:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=f"Error: {e}") from None


@app.post("/validate", response_model=ValidateResponse)
async def validate_code(request: ValidateRequest):
    """Validate Python source code syntax."""
    # Write to temp file for validation
    import tempfile

    with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
        f.write(request.source)
        temp_path = Path(f.name)

    try:
        validator = SyntaxValidator()
        result = await validator.validate(temp_path)

        return ValidateResponse(
            success=result.success,
            issues=[
                {
                    "line": issue.line,
                    "column": issue.column,
                    "message": issue.message,
                    "severity": issue.severity.value,
                }
                for issue in result.issues
            ],
        )
    finally:
        temp_path.unlink()


# ============================================================================
# Main entry point
# ============================================================================

if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8000)
