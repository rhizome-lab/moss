"""Tests for workflow loading with TOML parsing and @reference resolution."""

from pathlib import Path

import pytest

from moss.workflows import (
    AgentDefinition,
    Workflow,
    WorkflowLimits,
    WorkflowStep,
    _resolve_reference,
    list_workflows,
    load_workflow,
)


class TestWorkflowStep:
    """Tests for WorkflowStep dataclass."""

    def test_to_dict(self):
        step = WorkflowStep(name="validate", tool="validator.run")
        d = step.to_dict()
        assert d["name"] == "validate"
        assert d["tool"] == "validator.run"
        assert d["type"] == "tool"

    def test_with_all_fields(self):
        step = WorkflowStep(
            name="analyze",
            tool="llm.analyze",
            type="llm",
            input_from="validate",
            prompt="Fix the errors",
            on_error={"action": "goto", "target": "validate"},
            max_retries=3,
        )
        d = step.to_dict()
        assert d["type"] == "llm"
        assert d["input_from"] == "validate"
        assert d["max_retries"] == 3


class TestWorkflowLimits:
    """Tests for WorkflowLimits dataclass."""

    def test_defaults(self):
        limits = WorkflowLimits()
        assert limits.max_steps == 10
        assert limits.token_budget == 50000
        assert limits.timeout_seconds == 300

    def test_to_dict(self):
        limits = WorkflowLimits(max_steps=5, token_budget=10000)
        d = limits.to_dict()
        assert d["max_steps"] == 5
        assert d["token_budget"] == 10000


class TestWorkflow:
    """Tests for Workflow dataclass."""

    def test_minimal(self):
        wf = Workflow(name="test")
        assert wf.name == "test"
        assert wf.steps == []

    def test_to_dict(self):
        wf = Workflow(
            name="test",
            description="Test workflow",
            steps=[WorkflowStep(name="step1", tool="tool1")],
        )
        d = wf.to_dict()
        assert d["name"] == "test"
        assert len(d["steps"]) == 1


class TestLoadWorkflow:
    """Tests for load_workflow function."""

    def test_load_builtin_workflow(self):
        """Load the validate-fix workflow from built-in directory."""
        wf = load_workflow("validate-fix")
        assert wf.name == "validate-fix"
        assert len(wf.steps) == 3
        assert wf.steps[0].name == "validate"
        assert wf.steps[1].name == "analyze"
        assert wf.steps[2].name == "fix"

    def test_resolves_prompt_references(self):
        """@prompts/name references are resolved to prompt content."""
        wf = load_workflow("validate-fix")
        # system_prompt should be resolved from @prompts/terse
        assert wf.llm.system_prompt is not None
        assert "terse" in wf.llm.system_prompt.lower() or "preamble" in wf.llm.system_prompt.lower()
        # step prompt should be resolved from @prompts/repair-engine
        analyze_step = wf.steps[1]
        assert analyze_step.prompt is not None
        assert "REPAIR" in analyze_step.prompt

    def test_workflow_limits(self):
        """Workflow limits are parsed correctly."""
        wf = load_workflow("validate-fix")
        assert wf.limits.max_steps == 10
        assert wf.limits.token_budget == 50000
        assert wf.limits.timeout_seconds == 300

    def test_workflow_not_found(self):
        """FileNotFoundError for missing workflow."""
        with pytest.raises(FileNotFoundError) as exc_info:
            load_workflow("nonexistent-workflow")
        assert "nonexistent-workflow" in str(exc_info.value)

    def test_user_override(self, tmp_path: Path):
        """User workflow in .moss/ takes precedence over built-in."""
        # Create user workflow
        user_dir = tmp_path / ".moss" / "workflows"
        user_dir.mkdir(parents=True)
        user_wf = user_dir / "test-wf.toml"
        user_wf.write_text("""
[workflow]
name = "user-test"
description = "User override"
version = "2.0"

[[workflow.steps]]
name = "user-step"
tool = "user.tool"
""")
        wf = load_workflow("test-wf", project_root=tmp_path)
        assert wf.name == "user-test"
        assert wf.version == "2.0"


class TestResolveReference:
    """Tests for @reference resolution."""

    def test_non_reference_passthrough(self, tmp_path: Path):
        """Non-@ strings pass through unchanged."""
        result = _resolve_reference("plain text", tmp_path)
        assert result == "plain text"

    def test_invalid_reference_format(self, tmp_path: Path):
        """Invalid reference format raises ValueError."""
        with pytest.raises(ValueError) as exc_info:
            _resolve_reference("@invalid", tmp_path)
        assert "Invalid reference format" in str(exc_info.value)

    def test_unknown_reference_type(self, tmp_path: Path):
        """Unknown reference type raises ValueError."""
        with pytest.raises(ValueError) as exc_info:
            _resolve_reference("@unknown/name", tmp_path)
        assert "Unknown reference type" in str(exc_info.value)

    def test_circular_reference_detection(self, tmp_path: Path):
        """Circular references are detected."""
        loading_stack = {"@prompts/test"}
        with pytest.raises(ValueError) as exc_info:
            _resolve_reference("@prompts/test", tmp_path, loading_stack)
        assert "Circular reference" in str(exc_info.value)


class TestListWorkflows:
    """Tests for list_workflows function."""

    def test_lists_builtin_workflows(self):
        """Built-in workflows are listed."""
        workflows = list_workflows()
        assert "validate-fix" in workflows

    def test_includes_user_workflows(self, tmp_path: Path):
        """User workflows are included in listing."""
        user_dir = tmp_path / ".moss" / "workflows"
        user_dir.mkdir(parents=True)
        (user_dir / "custom.toml").write_text("[workflow]\nname = 'custom'")

        workflows = list_workflows(project_root=tmp_path)
        assert "custom" in workflows


class TestAgentDefinition:
    """Tests for AgentDefinition dataclass."""

    def test_defaults(self):
        agent = AgentDefinition(name="test")
        assert agent.include_diagnostics is True
        assert agent.include_memory is True
        assert agent.peek_first is True

    def test_to_dict(self):
        agent = AgentDefinition(
            name="test",
            enabled_tools=["skeleton", "grep"],
            peek_first=False,
        )
        d = agent.to_dict()
        assert d["name"] == "test"
        assert d["enabled_tools"] == ["skeleton", "grep"]
        assert d["peek_first"] is False


class TestWorkflowToAgentLoop:
    """Tests for workflow -> agent loop conversion."""

    def test_converts_builtin_workflow(self):
        """Convert validate-fix workflow to AgentLoop."""
        from moss.workflows import workflow_to_agent_loop

        wf = load_workflow("validate-fix")
        loop = workflow_to_agent_loop(wf)

        assert loop.name == "validate-fix"
        assert len(loop.steps) == 3
        assert loop.max_steps == 10
        assert loop.token_budget == 50000

    def test_converts_step_types(self):
        """Step types are converted correctly."""
        from moss.agent_loop import StepType
        from moss.workflows import workflow_to_agent_loop

        wf = load_workflow("validate-fix")
        loop = workflow_to_agent_loop(wf)

        # validate step is tool type
        assert loop.steps[0].step_type == StepType.TOOL
        # analyze step is llm type
        assert loop.steps[1].step_type == StepType.LLM
        # fix step is tool type
        assert loop.steps[2].step_type == StepType.TOOL

    def test_converts_error_actions(self):
        """Error actions are converted correctly."""
        from moss.agent_loop import ErrorAction
        from moss.workflows import workflow_to_agent_loop

        wf = load_workflow("validate-fix")
        loop = workflow_to_agent_loop(wf)

        # validate step has on_error: skip
        assert loop.steps[0].on_error == ErrorAction.SKIP
        # fix step has no on_error, defaults to ABORT
        assert loop.steps[2].on_error == ErrorAction.ABORT

    def test_converts_goto_error_action(self):
        """GOTO error action with target is converted correctly."""
        from moss.agent_loop import ErrorAction
        from moss.workflows import WorkflowStep, workflow_to_agent_loop

        wf = Workflow(
            name="test",
            steps=[
                WorkflowStep(
                    name="step1",
                    tool="tool1",
                    on_error={"action": "goto", "target": "step1"},
                )
            ],
        )
        loop = workflow_to_agent_loop(wf)

        assert loop.steps[0].on_error == ErrorAction.GOTO
        assert loop.steps[0].goto_target == "step1"

    def test_converts_input_from(self):
        """input_from is preserved in conversion."""
        from moss.workflows import workflow_to_agent_loop

        wf = load_workflow("validate-fix")
        loop = workflow_to_agent_loop(wf)

        assert loop.steps[1].input_from == "validate"
        assert loop.steps[2].input_from == "analyze"


class TestWorkflowToLLMConfig:
    """Tests for workflow LLM config conversion."""

    def test_converts_llm_config(self):
        """LLM config is converted correctly."""
        from moss.workflows import workflow_to_llm_config

        wf = load_workflow("validate-fix")
        config = workflow_to_llm_config(wf)

        assert config.model == "gemini/gemini-3-flash-preview"
        assert config.temperature == 0.0
        assert config.system_prompt is not None
