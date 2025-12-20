"""Workflow loading with TOML parsing and @reference resolution.

Workflows are loaded from:
1. .moss/workflows/{name}.toml (user override)
2. src/moss/workflows/{name}.toml (built-in)

Reference syntax:
- @prompts/name -> loads prompt from prompts directory
- @workflows/name -> loads workflow definition
"""

from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from moss.agent_loop import AgentLoop, LLMConfig, LoopResult

try:
    import tomllib
except ImportError:
    import tomli as tomllib  # type: ignore[import-not-found]

from moss.prompts import load_prompt

# Package directory for built-in workflows
_BUILTIN_DIR = Path(__file__).parent


@dataclass
class WorkflowStep:
    """Single step in a workflow."""

    name: str
    tool: str
    type: str = "tool"  # "tool" or "llm"
    input_from: str | None = None
    prompt: str | None = None  # Resolved from @reference if present
    on_error: str | dict[str, Any] | None = None
    max_retries: int = 0

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "name": self.name,
            "tool": self.tool,
            "type": self.type,
            "input_from": self.input_from,
            "prompt": self.prompt,
            "on_error": self.on_error,
            "max_retries": self.max_retries,
        }


@dataclass
class WorkflowLimits:
    """Resource limits for workflow execution."""

    max_steps: int = 10
    token_budget: int = 50000
    timeout_seconds: int = 300

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "max_steps": self.max_steps,
            "token_budget": self.token_budget,
            "timeout_seconds": self.timeout_seconds,
        }


@dataclass
class WorkflowLLMConfig:
    """LLM configuration for workflow."""

    model: str = "gemini/gemini-3-flash-preview"
    temperature: float = 0.0
    system_prompt: str | None = None  # Resolved from @reference if present

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "model": self.model,
            "temperature": self.temperature,
            "system_prompt": self.system_prompt,
        }


@dataclass
class Workflow:
    """Composable workflow definition loaded from TOML."""

    name: str
    description: str = ""
    version: str = "1.0"
    limits: WorkflowLimits = field(default_factory=WorkflowLimits)
    llm: WorkflowLLMConfig = field(default_factory=WorkflowLLMConfig)
    steps: list[WorkflowStep] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization."""
        return {
            "name": self.name,
            "description": self.description,
            "version": self.version,
            "limits": self.limits.to_dict(),
            "llm": self.llm.to_dict(),
            "steps": [s.to_dict() for s in self.steps],
        }


@dataclass
class AgentDefinition:
    """Agent definition composing workflow with tools."""

    name: str
    description: str = ""
    workflow: Workflow | None = None
    enabled_tools: list[str] = field(default_factory=list)
    disabled_tools: list[str] = field(default_factory=list)
    include_diagnostics: bool = True
    include_memory: bool = True
    peek_first: bool = True

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "name": self.name,
            "description": self.description,
            "workflow": self.workflow.to_dict() if self.workflow else None,
            "enabled_tools": self.enabled_tools,
            "disabled_tools": self.disabled_tools,
            "include_diagnostics": self.include_diagnostics,
            "include_memory": self.include_memory,
            "peek_first": self.peek_first,
        }


def _resolve_reference(ref: str, project_root: Path, loading_stack: set[str] | None = None) -> str:
    """Resolve an @reference to its content.

    Args:
        ref: Reference string like "@prompts/name" or "@workflows/name"
        project_root: Root directory for .moss/ lookup
        loading_stack: Set of references currently being loaded (cycle detection)

    Returns:
        Resolved content (prompt text or workflow TOML string)

    Raises:
        ValueError: If reference format is invalid or circular reference detected
        FileNotFoundError: If referenced file not found
    """
    if not ref.startswith("@"):
        return ref  # Not a reference, return as-is

    if loading_stack is None:
        loading_stack = set()

    if ref in loading_stack:
        raise ValueError(f"Circular reference detected: {ref}")

    loading_stack.add(ref)

    parts = ref[1:].split("/", 1)
    if len(parts) != 2:
        raise ValueError(f"Invalid reference format: {ref}. Expected @type/name")

    ref_type, name = parts

    if ref_type == "prompts":
        return load_prompt(name, project_root)
    elif ref_type == "workflows":
        # For workflow refs in strings, return a placeholder
        # The caller should handle workflow refs specially
        # (actual workflow loading happens in load_agent)
        return f"<workflow:{name}>"
    else:
        raise ValueError(f"Unknown reference type: {ref_type}")


def _resolve_value(value: Any, project_root: Path, loading_stack: set[str] | None = None) -> Any:
    """Recursively resolve @references in a value.

    Args:
        value: Any TOML value (string, dict, list)
        project_root: Root directory for .moss/ lookup
        loading_stack: Set of references currently being loaded

    Returns:
        Value with all @references resolved
    """
    if isinstance(value, str) and value.startswith("@"):
        return _resolve_reference(value, project_root, loading_stack)
    elif isinstance(value, dict):
        return {k: _resolve_value(v, project_root, loading_stack) for k, v in value.items()}
    elif isinstance(value, list):
        return [_resolve_value(item, project_root, loading_stack) for item in value]
    return value


def load_workflow(
    name: str | Path,
    project_root: Path | None = None,
    loading_stack: set[str] | None = None,
) -> Workflow:
    """Load a workflow by name or path.

    Args:
        name: Workflow name (without .toml) or explicit Path
        project_root: Project root for .moss/ lookup. Defaults to cwd.
        loading_stack: For internal cycle detection

    Returns:
        Parsed Workflow object with @references resolved.

    Raises:
        FileNotFoundError: If workflow not found
        ValueError: If workflow format is invalid
    """
    if project_root is None:
        project_root = Path.cwd()

    if loading_stack is None:
        loading_stack = set()

    # Find workflow file
    if isinstance(name, Path):
        workflow_path = name
    else:
        # Check user override first
        user_path = project_root / ".moss" / "workflows" / f"{name}.toml"
        if user_path.exists():
            workflow_path = user_path
        else:
            # Fall back to built-in
            builtin_path = _BUILTIN_DIR / f"{name}.toml"
            if builtin_path.exists():
                workflow_path = builtin_path
            else:
                raise FileNotFoundError(
                    f"Workflow '{name}' not found. Searched:\n  - {user_path}\n  - {builtin_path}"
                )

    # Parse TOML
    content = workflow_path.read_text()
    data = tomllib.loads(content)

    # Resolve @references
    data = _resolve_value(data, project_root, loading_stack)

    # Extract workflow section
    if "workflow" not in data:
        raise ValueError(f"Workflow file missing [workflow] section: {workflow_path}")

    wf_data = data["workflow"]

    # Parse limits
    limits = WorkflowLimits()
    if "limits" in wf_data:
        lim = wf_data["limits"]
        limits = WorkflowLimits(
            max_steps=lim.get("max_steps", 10),
            token_budget=lim.get("token_budget", 50000),
            timeout_seconds=lim.get("timeout_seconds", 300),
        )

    # Parse LLM config
    llm = WorkflowLLMConfig()
    if "llm" in wf_data:
        llm_data = wf_data["llm"]
        llm = WorkflowLLMConfig(
            model=llm_data.get("model", "gemini/gemini-3-flash-preview"),
            temperature=llm_data.get("temperature", 0.0),
            system_prompt=llm_data.get("system_prompt"),
        )

    # Parse steps
    steps = []
    for step_data in wf_data.get("steps", []):
        step = WorkflowStep(
            name=step_data["name"],
            tool=step_data["tool"],
            type=step_data.get("type", "tool"),
            input_from=step_data.get("input_from"),
            prompt=step_data.get("prompt"),
            on_error=step_data.get("on_error"),
            max_retries=step_data.get("max_retries", 0),
        )
        steps.append(step)

    return Workflow(
        name=wf_data.get("name", workflow_path.stem),
        description=wf_data.get("description", ""),
        version=wf_data.get("version", "1.0"),
        limits=limits,
        llm=llm,
        steps=steps,
    )


def load_agent(name: str | Path, project_root: Path | None = None) -> AgentDefinition:
    """Load an agent definition by name or path.

    Args:
        name: Agent name (without .toml) or explicit Path
        project_root: Project root for .moss/ lookup. Defaults to cwd.

    Returns:
        Parsed AgentDefinition with workflow resolved.

    Raises:
        FileNotFoundError: If agent definition not found
        ValueError: If agent format is invalid
    """
    if project_root is None:
        project_root = Path.cwd()

    # Find agent file
    if isinstance(name, Path):
        agent_path = name
    else:
        # Check user override first
        user_path = project_root / ".moss" / "agents" / f"{name}.toml"
        if user_path.exists():
            agent_path = user_path
        else:
            # Fall back to built-in
            builtin_path = _BUILTIN_DIR.parent / "agents" / f"{name}.toml"
            if builtin_path.exists():
                agent_path = builtin_path
            else:
                raise FileNotFoundError(
                    f"Agent '{name}' not found. Searched:\n  - {user_path}\n  - {builtin_path}"
                )

    # Parse TOML
    content = agent_path.read_text()
    data = tomllib.loads(content)

    if "agent" not in data:
        raise ValueError(f"Agent file missing [agent] section: {agent_path}")

    ag_data = data["agent"]

    # Load referenced workflow
    workflow = None
    workflow_ref = ag_data.get("workflow")
    if workflow_ref:
        if isinstance(workflow_ref, str) and workflow_ref.startswith("@workflows/"):
            workflow_name = workflow_ref.split("/", 1)[1]
            workflow = load_workflow(workflow_name, project_root)
        elif isinstance(workflow_ref, str):
            workflow = load_workflow(workflow_ref, project_root)

    # Parse tools config
    tools = ag_data.get("tools", {})
    enabled = tools.get("enabled", [])
    disabled = tools.get("disabled", [])

    # Parse context config
    context = ag_data.get("context", {})

    return AgentDefinition(
        name=ag_data.get("name", agent_path.stem),
        description=ag_data.get("description", ""),
        workflow=workflow,
        enabled_tools=enabled,
        disabled_tools=disabled,
        include_diagnostics=context.get("include_diagnostics", True),
        include_memory=context.get("include_memory", True),
        peek_first=context.get("peek_first", True),
    )


def list_workflows(project_root: Path | None = None) -> list[str]:
    """List available workflow names.

    Returns workflows from both user and built-in directories.
    """
    if project_root is None:
        project_root = Path.cwd()

    workflows: set[str] = set()

    # Built-in workflows
    for p in _BUILTIN_DIR.glob("*.toml"):
        workflows.add(p.stem)

    # User workflows
    user_dir = project_root / ".moss" / "workflows"
    if user_dir.exists():
        for p in user_dir.glob("*.toml"):
            workflows.add(p.stem)

    return sorted(workflows)


def list_agents(project_root: Path | None = None) -> list[str]:
    """List available agent definition names."""
    if project_root is None:
        project_root = Path.cwd()

    agents: set[str] = set()

    # Built-in agents
    agents_dir = _BUILTIN_DIR.parent / "agents"
    if agents_dir.exists():
        for p in agents_dir.glob("*.toml"):
            agents.add(p.stem)

    # User agents
    user_dir = project_root / ".moss" / "agents"
    if user_dir.exists():
        for p in user_dir.glob("*.toml"):
            agents.add(p.stem)

    return sorted(agents)


# ============================================================================
# Agent Loop Integration
# ============================================================================


def workflow_to_agent_loop(workflow: Workflow) -> "AgentLoop":
    """Convert a TOML Workflow to an executable AgentLoop.

    This bridges the declarative TOML format with the executable agent loop.

    Args:
        workflow: Workflow loaded from TOML

    Returns:
        AgentLoop ready for execution with AgentLoopRunner
    """
    from moss.agent_loop import AgentLoop, ErrorAction, LoopStep, StepType

    steps = []
    for wf_step in workflow.steps:
        # Convert step type
        if wf_step.type == "llm":
            step_type = StepType.LLM
        elif wf_step.type == "hybrid":
            step_type = StepType.HYBRID
        else:
            step_type = StepType.TOOL

        # Convert error action
        on_error = ErrorAction.ABORT
        goto_target = None
        if wf_step.on_error:
            if isinstance(wf_step.on_error, str):
                on_error = ErrorAction[wf_step.on_error.upper()]
            elif isinstance(wf_step.on_error, dict):
                action = wf_step.on_error.get("action", "abort")
                on_error = ErrorAction[action.upper()]
                goto_target = wf_step.on_error.get("target")

        steps.append(
            LoopStep(
                name=wf_step.name,
                tool=wf_step.tool,
                step_type=step_type,
                input_from=wf_step.input_from,
                on_error=on_error,
                goto_target=goto_target,
                max_retries=wf_step.max_retries,
            )
        )

    return AgentLoop(
        name=workflow.name,
        steps=steps,
        max_steps=workflow.limits.max_steps,
        token_budget=workflow.limits.token_budget,
        timeout_seconds=workflow.limits.timeout_seconds,
    )


def workflow_to_llm_config(workflow: Workflow) -> "LLMConfig":
    """Convert workflow LLM config to LLMConfig for executor.

    Args:
        workflow: Workflow with LLM configuration

    Returns:
        LLMConfig for use with LLMToolExecutor
    """
    from moss.agent_loop import LLMConfig

    return LLMConfig(
        model=workflow.llm.model,
        temperature=workflow.llm.temperature,
        system_prompt=workflow.llm.system_prompt,
    )


async def run_workflow(
    name: str | Path,
    initial_input: Any = None,
    project_root: Path | None = None,
) -> "LoopResult":
    """Load and run a workflow by name.

    Convenience function that:
    1. Loads workflow from TOML
    2. Converts to AgentLoop
    3. Creates appropriate executor
    4. Runs and returns result

    Args:
        name: Workflow name (without .toml) or explicit Path
        initial_input: Input data for the workflow
        project_root: Project root for .moss/ lookup

    Returns:
        LoopResult with execution status and metrics
    """
    from moss.agent_loop import AgentLoopRunner, LLMToolExecutor

    # Load workflow
    workflow = load_workflow(name, project_root)

    # Convert to executable loop
    loop = workflow_to_agent_loop(workflow)
    llm_config = workflow_to_llm_config(workflow)

    # Create executor and run
    executor = LLMToolExecutor(config=llm_config)
    runner = AgentLoopRunner(executor)

    return await runner.run(loop, initial_input)
