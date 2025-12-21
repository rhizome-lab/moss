"""Template-based code generator.

Generates code from user-configurable templates. Templates are loaded from:
1. Project templates/ directory
2. User ~/.moss/templates/ directory
3. Built-in templates

Templates use simple string interpolation with placeholders:
- ${name} - specification description (sanitized)
- ${type} - type signature
- ${constraints} - constraints as comments
- ${examples} - input/output examples as comments
"""

from __future__ import annotations

import re
import string
from pathlib import Path
from typing import TYPE_CHECKING

from moss.synthesis.plugins.protocols import (
    CodeGenerator,
    GenerationCost,
    GenerationHints,
    GenerationResult,
    GeneratorMetadata,
    GeneratorType,
)

if TYPE_CHECKING:
    from moss.synthesis.types import Context, Specification


# =============================================================================
# Template Patterns
# =============================================================================

# Built-in templates for common patterns
BUILTIN_TEMPLATES: dict[str, dict[str, str]] = {
    # CRUD patterns
    "crud/create": '''def create_${name}(data: dict) -> ${type}:
    """Create a new ${name}.

    Args:
        data: The data to create

    Returns:
        The created ${name}
    """
    # ${constraints}
    raise NotImplementedError("TODO: implement create_${name}")
''',
    "crud/read": '''def get_${name}(id: str) -> ${type} | None:
    """Get ${name} by ID.

    Args:
        id: The ${name} ID

    Returns:
        The ${name} if found, None otherwise
    """
    raise NotImplementedError("TODO: implement get_${name}")
''',
    "crud/update": '''def update_${name}(id: str, data: dict) -> ${type} | None:
    """Update an existing ${name}.

    Args:
        id: The ${name} ID
        data: The update data

    Returns:
        The updated ${name} if found, None otherwise
    """
    raise NotImplementedError("TODO: implement update_${name}")
''',
    "crud/delete": '''def delete_${name}(id: str) -> bool:
    """Delete ${name} by ID.

    Args:
        id: The ${name} ID

    Returns:
        True if deleted, False if not found
    """
    raise NotImplementedError("TODO: implement delete_${name}")
''',
    # Validation patterns
    "validation/required": '''def validate_${name}(value: Any) -> bool:
    """Validate that ${name} is present and valid.

    Args:
        value: The value to validate

    Returns:
        True if valid, False otherwise
    """
    if value is None:
        return False
    # ${constraints}
    return True
''',
    "validation/range": '''def validate_${name}_range(
    value: ${type}, min_val: ${type}, max_val: ${type}
) -> bool:
    """Validate that ${name} is within range.

    Args:
        value: The value to validate
        min_val: Minimum allowed value
        max_val: Maximum allowed value

    Returns:
        True if in range, False otherwise
    """
    return min_val <= value <= max_val
''',
    # Transformation patterns
    "transform/map": '''def transform_${name}(items: list[${type}]) -> list[${type}]:
    """Transform a list of ${name}.

    Args:
        items: The items to transform

    Returns:
        The transformed items
    """
    # ${examples}
    return [item for item in items]  # TODO: implement transformation
''',
    "transform/filter": '''def filter_${name}(items: list[${type}], predicate) -> list[${type}]:
    """Filter a list of ${name}.

    Args:
        items: The items to filter
        predicate: Filter function

    Returns:
        The filtered items
    """
    return [item for item in items if predicate(item)]
''',
    # Function patterns
    "function/pure": '''def ${name}(${params}) -> ${type}:
    """${description}

    Args:
        ${params_doc}

    Returns:
        ${type}
    """
    # ${constraints}
    # ${examples}
    raise NotImplementedError("TODO: implement ${name}")
''',
    "function/async": '''async def ${name}(${params}) -> ${type}:
    """${description}

    Args:
        ${params_doc}

    Returns:
        ${type}
    """
    raise NotImplementedError("TODO: implement ${name}")
''',
}

# Pattern keywords that help match specs to templates
PATTERN_KEYWORDS: dict[str, list[str]] = {
    "crud/create": ["create", "add", "insert", "new", "make"],
    "crud/read": ["get", "read", "fetch", "find", "retrieve", "lookup"],
    "crud/update": ["update", "modify", "change", "edit", "patch"],
    "crud/delete": ["delete", "remove", "destroy", "drop"],
    "validation/required": ["required", "validate", "check", "must have", "mandatory"],
    "validation/range": ["range", "between", "min", "max", "bounds"],
    "transform/map": ["transform", "convert", "map", "process"],
    "transform/filter": ["filter", "select", "where", "matching"],
    "function/async": ["async", "await", "concurrent", "parallel"],
}


# =============================================================================
# Template Generator
# =============================================================================


class TemplateGenerator:
    """Generator that uses templates for common patterns.

    Templates are matched to specifications based on:
    1. Pattern keywords in the description
    2. Type signature hints
    3. Explicit template hints

    Template directories are scanned in order:
    1. hints.preferred_style (if specified)
    2. Project templates/ directory
    3. User ~/.moss/templates/ directory
    4. Built-in templates
    """

    def __init__(
        self,
        template_dirs: list[Path] | None = None,
        custom_templates: dict[str, str] | None = None,
    ) -> None:
        """Initialize template generator.

        Args:
            template_dirs: Additional directories to search for templates
            custom_templates: Additional templates to register
        """
        self._template_dirs = template_dirs or []
        self._custom_templates = custom_templates or {}
        self._loaded_templates: dict[str, str] = {}
        self._templates_loaded = False

        self._metadata = GeneratorMetadata(
            name="template",
            generator_type=GeneratorType.TEMPLATE,
            priority=10,  # Higher priority than placeholder
            description="Template-based code generation for common patterns",
        )

    @property
    def metadata(self) -> GeneratorMetadata:
        """Return generator metadata."""
        return self._metadata

    def _load_templates(self) -> None:
        """Load templates from all sources."""
        if self._templates_loaded:
            return

        # Start with builtins
        self._loaded_templates = dict(BUILTIN_TEMPLATES)

        # Load from directories
        for template_dir in self._template_dirs:
            if template_dir.is_dir():
                self._load_from_directory(template_dir)

        # Load user templates
        user_dir = Path.home() / ".moss" / "templates"
        if user_dir.is_dir():
            self._load_from_directory(user_dir)

        # Add custom templates (highest priority)
        self._loaded_templates.update(self._custom_templates)

        self._templates_loaded = True

    def _load_from_directory(self, directory: Path) -> None:
        """Load templates from a directory.

        Templates are named by relative path: crud/create.py.tmpl -> crud/create
        """
        for template_file in directory.rglob("*.tmpl"):
            # Get relative path without extension
            rel_path = template_file.relative_to(directory)
            name = str(rel_path.with_suffix("").with_suffix(""))

            try:
                content = template_file.read_text()
                self._loaded_templates[name] = content
            except (OSError, UnicodeDecodeError):
                pass  # Skip unreadable templates

    def can_generate(self, spec: Specification, context: Context) -> bool:
        """Check if a template matches this specification."""
        self._load_templates()

        # Always can generate if we have any templates
        # The generate method will fall back to generic if no match
        return len(self._loaded_templates) > 0

    def _match_template(
        self,
        spec: Specification,
        context: Context,
        hints: GenerationHints | None,
    ) -> tuple[str | None, float]:
        """Find the best matching template for a specification.

        Returns:
            (template_name, confidence) or (None, 0.0)
        """
        self._load_templates()

        description_lower = spec.description.lower()
        best_match: str | None = None
        best_score = 0.0

        # Check for explicit hint
        if hints and hints.preferred_style:
            if hints.preferred_style in self._loaded_templates:
                return hints.preferred_style, 0.9

        # Score each template based on keyword matches
        for template_name in self._loaded_templates:
            keywords = PATTERN_KEYWORDS.get(template_name, [])
            if not keywords:
                continue

            # Count keyword matches
            matches = sum(1 for kw in keywords if kw in description_lower)
            if matches > 0:
                score = matches / len(keywords)

                # Boost score if type signature matches
                if spec.type_signature:
                    if "list" in spec.type_signature.lower() and "transform" in template_name:
                        score += 0.2
                    if "bool" in spec.type_signature.lower() and "validation" in template_name:
                        score += 0.2

                if score > best_score:
                    best_score = score
                    best_match = template_name

        return best_match, best_score

    def _extract_name(self, spec: Specification) -> str:
        """Extract a function/variable name from specification."""
        # Try to extract a clean name from description
        desc = spec.description.lower()

        # Remove common prefixes
        for prefix in ["create a ", "make a ", "get the ", "return ", "calculate "]:
            if desc.startswith(prefix):
                desc = desc[len(prefix) :]
                break

        # Take first few words, make valid identifier
        words = re.findall(r"\w+", desc)[:3]
        name = "_".join(words)

        # Ensure valid Python identifier
        if name and name[0].isdigit():
            name = "_" + name

        return name or "func"

    def _render_template(
        self,
        template: str,
        spec: Specification,
        context: Context,
    ) -> str:
        """Render a template with specification values."""
        name = self._extract_name(spec)

        # Build substitution dict
        subs = {
            "name": name,
            "description": spec.description,
            "type": spec.type_signature or "Any",
            "params": "",
            "params_doc": "",
            "constraints": "",
            "examples": "",
        }

        # Format constraints
        if spec.constraints:
            subs["constraints"] = " | ".join(spec.constraints)

        # Format examples
        if spec.examples:
            example_lines = [f"{inp!r} -> {out!r}" for inp, out in spec.examples[:3]]
            subs["examples"] = " ; ".join(example_lines)

        # Try to parse type signature for params
        if spec.type_signature:
            # Simple extraction: (a, b) -> c
            match = re.match(r"\(([^)]*)\)\s*->\s*(\S+)", spec.type_signature)
            if match:
                subs["params"] = match.group(1)
                subs["type"] = match.group(2)

        # Render template
        try:
            template_obj = string.Template(template)
            return template_obj.safe_substitute(subs)
        except (ValueError, KeyError):
            # Fallback to simple replacement
            result = template
            for key, value in subs.items():
                result = result.replace(f"${{{key}}}", str(value))
            return result

    async def generate(
        self,
        spec: Specification,
        context: Context,
        hints: GenerationHints | None = None,
    ) -> GenerationResult:
        """Generate code using templates.

        Args:
            spec: The specification
            context: Available resources
            hints: Optional hints

        Returns:
            GenerationResult with templated code
        """
        self._load_templates()

        # Find matching template
        template_name, confidence = self._match_template(spec, context, hints)

        if template_name is None:
            # Fall back to generic function template
            template_name = "function/pure"
            confidence = 0.3

        template = self._loaded_templates.get(template_name, "")
        if not template:
            return GenerationResult(
                success=False,
                error=f"Template '{template_name}' not found",
            )

        # Render template
        code = self._render_template(template, spec, context)

        return GenerationResult(
            success=True,
            code=code,
            confidence=confidence,
            metadata={
                "source": "template",
                "template": template_name,
            },
        )

    def estimate_cost(self, spec: Specification, context: Context) -> GenerationCost:
        """Template generation is very fast."""
        return GenerationCost(
            time_estimate_ms=5,
            token_estimate=0,
            complexity_score=1,
        )

    def add_template(self, name: str, template: str) -> None:
        """Add a custom template.

        Args:
            name: Template name (e.g., "crud/create")
            template: Template content
        """
        self._custom_templates[name] = template
        self._loaded_templates[name] = template

    def get_available_templates(self) -> list[str]:
        """Get list of available template names."""
        self._load_templates()
        return list(self._loaded_templates.keys())


# Protocol compliance check
assert isinstance(TemplateGenerator(), CodeGenerator)
