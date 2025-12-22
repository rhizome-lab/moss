"""Tests for Composable Agent Loops (agent_loop.py)."""

import pytest

from moss.agent_loop import (
    AgentLoop,
    AgentLoopRunner,
    BenchmarkTask,
    CompositeToolExecutor,
    ErrorAction,
    LLMConfig,
    LLMToolExecutor,
    LoopBenchmark,
    LoopContext,
    LoopMetrics,
    LoopStatus,
    LoopStep,
    MCPServerConfig,
    MCPToolExecutor,
    MossToolExecutor,
    StepType,
)

# NOTE: Predefined loops (simple_loop, critic_loop, etc.) removed.
# Use DWIMLoop or TOML workflows instead. See docs/philosophy.md.


class TestLoopStep:
    """Tests for LoopStep dataclass."""

    def test_create_basic_step(self):
        step = LoopStep(name="test", tool="skeleton.format")
        assert step.name == "test"
        assert step.tool == "skeleton.format"
        assert step.step_type == StepType.TOOL
        assert step.on_error == ErrorAction.ABORT

    def test_create_llm_step(self):
        step = LoopStep(
            name="generate",
            tool="llm.generate",
            step_type=StepType.LLM,
            input_from="context",
        )
        assert step.step_type == StepType.LLM
        assert step.input_from == "context"

    def test_goto_requires_target(self):
        with pytest.raises(ValueError, match="GOTO action requires goto_target"):
            LoopStep(name="test", tool="test", on_error=ErrorAction.GOTO)

    def test_goto_with_target(self):
        step = LoopStep(
            name="test", tool="test", on_error=ErrorAction.GOTO, goto_target="retry_step"
        )
        assert step.goto_target == "retry_step"


class TestAgentLoop:
    """Tests for AgentLoop dataclass."""

    def test_create_basic_loop(self):
        loop = AgentLoop(
            name="test",
            steps=[LoopStep(name="step1", tool="test")],
        )
        assert loop.name == "test"
        assert loop.entry == "step1"  # Defaults to first step
        assert loop.max_steps == 10

    def test_loop_requires_steps(self):
        with pytest.raises(ValueError, match="must have at least one step"):
            AgentLoop(name="empty", steps=[])

    def test_loop_validates_entry(self):
        with pytest.raises(ValueError, match="Entry step 'nonexistent' not found"):
            AgentLoop(
                name="test",
                steps=[LoopStep(name="step1", tool="test")],
                entry="nonexistent",
            )

    def test_loop_validates_goto_targets(self):
        with pytest.raises(ValueError, match="GOTO target 'nonexistent' not found"):
            AgentLoop(
                name="test",
                steps=[
                    LoopStep(
                        name="step1",
                        tool="test",
                        on_error=ErrorAction.GOTO,
                        goto_target="nonexistent",
                    )
                ],
            )

    def test_step_names_must_be_unique(self):
        with pytest.raises(ValueError, match="Step names must be unique"):
            AgentLoop(
                name="test",
                steps=[
                    LoopStep(name="dupe", tool="test1"),
                    LoopStep(name="dupe", tool="test2"),
                ],
            )


class TestLoopContext:
    """Tests for LoopContext dataclass."""

    def test_initial_context(self):
        ctx = LoopContext(input="initial data")
        assert ctx.input == "initial data"
        assert ctx.steps == {}
        assert ctx.last is None

    def test_with_step(self):
        ctx = LoopContext(input="initial")
        ctx2 = ctx.with_step("step1", "output1")

        # Original unchanged
        assert ctx.steps == {}
        assert ctx.last is None

        # New context has step
        assert ctx2.steps == {"step1": "output1"}
        assert ctx2.last == "output1"
        assert ctx2.input == "initial"

    def test_chained_steps(self):
        ctx = LoopContext(input="initial")
        ctx = ctx.with_step("step1", "out1")
        ctx = ctx.with_step("step2", "out2")

        assert ctx.steps == {"step1": "out1", "step2": "out2"}
        assert ctx.last == "out2"

    def test_get_step(self):
        ctx = LoopContext(input="initial", steps={"a": 1, "b": 2})
        assert ctx.get("a") == 1
        assert ctx.get("c") is None
        assert ctx.get("c", "default") == "default"

    def test_with_step_eviction(self):
        """Test that max_steps limits context history."""
        ctx = LoopContext(input="initial")
        # Add 5 steps
        for i in range(5):
            ctx = ctx.with_step(f"step{i}", f"out{i}")

        assert len(ctx.steps) == 5

        # Add another step with max_steps=3 - should evict oldest
        ctx = ctx.with_step("step5", "out5", max_steps=3)
        assert len(ctx.steps) == 3
        # Should keep most recent 3: step3, step4, step5
        assert "step0" not in ctx.steps
        assert "step1" not in ctx.steps
        assert "step2" not in ctx.steps
        assert ctx.steps == {"step3": "out3", "step4": "out4", "step5": "out5"}

    def test_with_step_no_eviction_when_under_limit(self):
        """Test that max_steps doesn't evict when under limit."""
        ctx = LoopContext(input="initial")
        ctx = ctx.with_step("step1", "out1", max_steps=10)
        ctx = ctx.with_step("step2", "out2", max_steps=10)
        assert len(ctx.steps) == 2
        assert ctx.steps == {"step1": "out1", "step2": "out2"}


class TestLoopMetrics:
    """Tests for LoopMetrics dataclass."""

    def test_initial_metrics(self):
        metrics = LoopMetrics()
        assert metrics.llm_calls == 0
        assert metrics.tool_calls == 0
        assert metrics.iterations == 0

    def test_record_tool_step(self):
        metrics = LoopMetrics()
        metrics.record_step("step1", StepType.TOOL, duration=1.5)

        assert metrics.tool_calls == 1
        assert metrics.llm_calls == 0
        assert metrics.step_times["step1"] == 1.5

    def test_record_llm_step(self):
        metrics = LoopMetrics()
        metrics.record_step("step1", StepType.LLM, duration=2.0, tokens_in=100, tokens_out=50)

        assert metrics.llm_calls == 1
        assert metrics.llm_tokens_in == 100
        assert metrics.llm_tokens_out == 50
        assert metrics.tool_calls == 0

    def test_record_hybrid_step_with_tokens(self):
        metrics = LoopMetrics()
        metrics.record_step("step1", StepType.HYBRID, duration=1.0, tokens_in=10, tokens_out=5)

        assert metrics.tool_calls == 1
        assert metrics.llm_calls == 1
        assert metrics.llm_tokens_in == 10

    def test_record_hybrid_step_without_tokens(self):
        metrics = LoopMetrics()
        metrics.record_step("step1", StepType.HYBRID, duration=1.0)

        assert metrics.tool_calls == 1
        assert metrics.llm_calls == 0

    def test_to_compact(self):
        metrics = LoopMetrics()
        metrics.llm_calls = 2
        metrics.llm_tokens_in = 100
        metrics.llm_tokens_out = 50
        metrics.tool_calls = 5
        metrics.wall_time_seconds = 3.5
        metrics.iterations = 3
        metrics.retries = 1

        compact = metrics.to_compact()
        assert "LLM: 2 calls" in compact
        assert "150 tokens" in compact
        assert "Tools: 5 calls" in compact


class MockExecutor:
    """Mock executor for testing."""

    def __init__(self, responses: dict[str, tuple] | None = None):
        self.responses = responses or {}
        self.calls: list[tuple[str, LoopContext, LoopStep]] = []

    async def execute(
        self, tool_name: str, context: LoopContext, step: LoopStep
    ) -> tuple[str, int, int]:
        self.calls.append((tool_name, context, step))

        if tool_name in self.responses:
            resp = self.responses[tool_name]
            if isinstance(resp, Exception):
                raise resp
            return resp

        # Default response
        return f"output:{tool_name}", 0, 0


class TestAgentLoopRunner:
    """Tests for AgentLoopRunner."""

    @pytest.fixture
    def mock_executor(self):
        return MockExecutor()

    @pytest.fixture
    def runner(self, mock_executor):
        return AgentLoopRunner(mock_executor)

    async def test_run_simple_loop(self, runner, mock_executor):
        loop = AgentLoop(
            name="test",
            steps=[
                LoopStep(name="step1", tool="tool1"),
                LoopStep(name="step2", tool="tool2"),
            ],
        )

        result = await runner.run(loop, initial_input="input")

        assert result.success
        assert result.status == LoopStatus.SUCCESS
        assert len(mock_executor.calls) == 2
        assert result.final_output == "output:tool2"

    async def test_context_passed_through(self, runner, mock_executor):
        loop = AgentLoop(
            name="test",
            steps=[
                LoopStep(name="step1", tool="tool1"),
                LoopStep(name="step2", tool="tool2", input_from="step1"),
            ],
        )

        await runner.run(loop, initial_input="my_input")

        # First call gets initial input
        _, ctx1, _ = mock_executor.calls[0]
        assert ctx1.input == "my_input"
        assert ctx1.steps == {}

        # Second call has step1 output available
        _, ctx2, _ = mock_executor.calls[1]
        assert ctx2.input == "my_input"
        assert "step1" in ctx2.steps

    async def test_exit_condition(self, runner, mock_executor):
        loop = AgentLoop(
            name="test",
            steps=[
                LoopStep(name="step1", tool="tool1"),
                LoopStep(name="step2", tool="tool2"),
            ],
            exit_conditions=["step1.success"],
        )

        result = await runner.run(loop)

        assert result.success
        # Should exit after step1, not run step2
        assert len(mock_executor.calls) == 1

    async def test_max_steps_limit(self, runner, mock_executor):
        loop = AgentLoop(
            name="test",
            steps=[LoopStep(name="step1", tool="tool1")],
            exit_conditions=["never.exits"],  # Forces loop to continue
            max_steps=3,
        )

        result = await runner.run(loop)

        assert result.status == LoopStatus.MAX_ITERATIONS
        assert len(mock_executor.calls) == 3

    async def test_error_abort(self, runner):
        mock = MockExecutor(responses={"tool1": ValueError("test error")})
        runner = AgentLoopRunner(mock)

        loop = AgentLoop(
            name="test",
            steps=[LoopStep(name="step1", tool="tool1", on_error=ErrorAction.ABORT)],
        )

        result = await runner.run(loop)

        assert result.status == LoopStatus.FAILED
        assert "test error" in result.error

    async def test_error_skip(self, runner):
        mock = MockExecutor(
            responses={
                "tool1": ValueError("skip this"),
                "tool2": ("success", 0, 0),
            }
        )
        runner = AgentLoopRunner(mock)

        loop = AgentLoop(
            name="test",
            steps=[
                LoopStep(name="step1", tool="tool1", on_error=ErrorAction.SKIP),
                LoopStep(name="step2", tool="tool2"),
            ],
        )

        result = await runner.run(loop)

        assert result.success
        assert result.final_output == "success"

    async def test_error_retry(self, runner):
        call_count = 0

        class RetryExecutor:
            async def execute(self, tool_name, context, step):
                nonlocal call_count
                call_count += 1
                if call_count < 3:
                    raise ValueError("retry me")
                return "success", 0, 0

        runner = AgentLoopRunner(RetryExecutor())

        loop = AgentLoop(
            name="test",
            steps=[LoopStep(name="step1", tool="tool1", on_error=ErrorAction.RETRY, max_retries=5)],
        )

        result = await runner.run(loop)

        assert result.success
        assert call_count == 3

    async def test_error_goto(self, runner):
        call_sequence = []

        class GotoExecutor:
            async def execute(self, tool_name, context, step):
                call_sequence.append(step.name)
                if step.name == "step1":
                    raise ValueError("goto recovery")
                return f"output:{step.name}", 0, 0

        runner = AgentLoopRunner(GotoExecutor())

        loop = AgentLoop(
            name="test",
            steps=[
                LoopStep(
                    name="step1",
                    tool="tool1",
                    on_error=ErrorAction.GOTO,
                    goto_target="recovery",
                ),
                LoopStep(name="step2", tool="tool2"),
                LoopStep(name="recovery", tool="recover"),
            ],
            max_steps=5,
        )

        await runner.run(loop)

        assert "step1" in call_sequence
        assert "recovery" in call_sequence

    async def test_metrics_tracking(self, runner):
        mock = MockExecutor(
            responses={
                "tool1": ("out1", 0, 0),
                "llm.gen": ("out2", 100, 50),
            }
        )
        runner = AgentLoopRunner(mock)

        loop = AgentLoop(
            name="test",
            steps=[
                LoopStep(name="step1", tool="tool1", step_type=StepType.TOOL),
                LoopStep(name="step2", tool="llm.gen", step_type=StepType.LLM),
            ],
        )

        result = await runner.run(loop)

        assert result.metrics.tool_calls == 1
        assert result.metrics.llm_calls == 1
        assert result.metrics.llm_tokens_in == 100
        assert result.metrics.llm_tokens_out == 50

    async def test_token_budget(self, runner):
        mock = MockExecutor(responses={"llm.gen": ("out", 100, 100)})
        runner = AgentLoopRunner(mock)

        loop = AgentLoop(
            name="test",
            steps=[LoopStep(name="step1", tool="llm.gen", step_type=StepType.LLM)],
            exit_conditions=["never"],
            token_budget=150,
            max_steps=10,
        )

        result = await runner.run(loop)

        assert result.status == LoopStatus.BUDGET_EXCEEDED


class TestLLMConfig:
    """Tests for LLMConfig."""

    def test_default_config(self):
        config = LLMConfig()
        assert "gemini" in config.model
        assert config.temperature == 0.0
        assert config.mock is False

    def test_custom_config(self):
        config = LLMConfig(model="gpt-4o", temperature=0.7, mock=True)
        assert config.model == "gpt-4o"
        assert config.temperature == 0.7
        assert config.mock is True

    def test_rotation_config(self):
        config = LLMConfig(
            models=["gemini/gemini-3-flash-preview", "gpt-4o"],
            rotation="round_robin",
        )
        assert len(config.models) == 2
        assert config.rotation == "round_robin"


class TestLLMRotation:
    """Tests for multi-LLM rotation."""

    def test_no_rotation_uses_primary(self, tmp_path):
        config = LLMConfig(model="primary-model", mock=True)
        executor = LLMToolExecutor(config=config, root=tmp_path, load_env=False)

        model = executor._get_model()
        assert model == "primary-model"

    def test_round_robin_rotation(self, tmp_path):
        config = LLMConfig(
            models=["model-a", "model-b", "model-c"],
            rotation="round_robin",
            mock=True,
        )
        executor = LLMToolExecutor(config=config, root=tmp_path, load_env=False)

        # Should cycle through models
        assert executor._get_model() == "model-a"
        assert executor._get_model() == "model-b"
        assert executor._get_model() == "model-c"
        assert executor._get_model() == "model-a"  # Wraps around

    def test_random_rotation(self, tmp_path):
        config = LLMConfig(
            models=["model-a", "model-b"],
            rotation="random",
            mock=True,
        )
        executor = LLMToolExecutor(config=config, root=tmp_path, load_env=False)

        # Should return a model from the pool
        models_seen = set()
        for _ in range(20):
            models_seen.add(executor._get_model())

        # With 20 tries, we should see both models (probabilistically)
        assert "model-a" in models_seen or "model-b" in models_seen

    def test_empty_models_uses_primary(self, tmp_path):
        config = LLMConfig(
            model="primary",
            models=[],
            rotation="round_robin",
            mock=True,
        )
        executor = LLMToolExecutor(config=config, root=tmp_path, load_env=False)

        assert executor._get_model() == "primary"


class TestLLMToolExecutor:
    """Tests for LLMToolExecutor with mock mode."""

    @pytest.fixture
    def mock_llm_executor(self, tmp_path):
        config = LLMConfig(mock=True)
        return LLMToolExecutor(config=config, root=tmp_path, load_env=False)

    async def test_llm_tool_mock_mode(self, mock_llm_executor):
        step = LoopStep(name="gen", tool="llm.generate", step_type=StepType.LLM)
        context = LoopContext(input="test prompt")

        output, tokens_in, tokens_out = await mock_llm_executor.execute(
            "llm.generate", context, step
        )

        assert "[MOCK generate]" in output
        assert tokens_in > 0
        assert tokens_out > 0

    async def test_routes_to_moss_executor(self, mock_llm_executor, tmp_path):
        # Create a test file for skeleton
        test_file = tmp_path / "test.py"
        test_file.write_text("def hello(): pass")

        step = LoopStep(name="skel", tool="skeleton.format")
        context = LoopContext(input=str(test_file))

        _output, tokens_in, tokens_out = await mock_llm_executor.execute(
            "skeleton.format", context, step
        )

        # Should route to MossToolExecutor, not LLM
        assert tokens_in == 0
        assert tokens_out == 0


class TestLoopBenchmark:
    """Tests for LoopBenchmark."""

    async def test_benchmark_single_loop(self):
        mock = MockExecutor(
            responses={
                "tool1": ("out1", 0, 0),
                "llm.gen": ("out2", 50, 25),
            }
        )
        benchmark = LoopBenchmark(executor=mock)

        loop = AgentLoop(
            name="test",
            steps=[
                LoopStep(name="step1", tool="tool1", step_type=StepType.TOOL),
                LoopStep(name="step2", tool="llm.gen", step_type=StepType.LLM),
            ],
        )

        tasks = [
            BenchmarkTask(name="task1", input_data="input1"),
            BenchmarkTask(name="task2", input_data="input2"),
        ]

        result = await benchmark.run(loop, tasks)

        assert result.tasks_run == 2
        assert result.successes == 2
        assert result.total_llm_calls == 2
        assert result.total_tool_calls == 2

    async def test_benchmark_comparison(self):
        mock = MockExecutor()
        benchmark = LoopBenchmark(executor=mock)

        loop1 = AgentLoop(name="fast", steps=[LoopStep(name="s1", tool="t1")])
        loop2 = AgentLoop(name="slow", steps=[LoopStep(name="s1", tool="t1")])

        tasks = [BenchmarkTask(name="task1", input_data="input")]

        results = await benchmark.compare([loop1, loop2], tasks)

        assert len(results) == 2
        assert results[0].loop_name == "fast"
        assert results[1].loop_name == "slow"

    async def test_benchmark_result_formatting(self):
        mock = MockExecutor()
        benchmark = LoopBenchmark(executor=mock)

        loop = AgentLoop(name="test", steps=[LoopStep(name="s1", tool="t1")])
        tasks = [BenchmarkTask(name="t1", input_data="i1")]

        result = await benchmark.run(loop, tasks)

        compact = result.to_compact()
        assert "test" in compact
        assert "100%" in compact

        markdown = result.to_markdown()
        assert "# Benchmark" in markdown
        assert "Success rate" in markdown


class TestMossToolExecutor:
    """Tests for MossToolExecutor."""

    @pytest.fixture
    def executor(self, tmp_path):
        """Create a MossToolExecutor for testing."""
        return MossToolExecutor(root=tmp_path)

    @pytest.mark.asyncio
    async def test_parse_docstrings_basic(self, executor):
        """Test parsing basic FUNC:name|docstring format."""
        llm_output = """FUNC:foo|Does something useful
FUNC:bar|Processes the data"""
        context = LoopContext(input=llm_output)
        step = LoopStep(name="parse", tool="parse.docstrings")

        result, tokens_in, tokens_out = await executor.execute("parse.docstrings", context, step)

        assert len(result) == 2
        assert result[0]["function"] == "foo"
        assert result[0]["docstring"] == "Does something useful"
        assert result[1]["function"] == "bar"
        assert result[1]["docstring"] == "Processes the data"
        assert tokens_in == 0
        assert tokens_out == 0

    @pytest.mark.asyncio
    async def test_parse_docstrings_with_extra_lines(self, executor):
        """Test parsing ignores non-FUNC lines."""
        llm_output = """Here are the functions that need docstrings:

FUNC:calculate|Calculates the result

Some other text
FUNC:validate|Validates input data"""
        context = LoopContext(input=llm_output)
        step = LoopStep(name="parse", tool="parse.docstrings")

        result, _, _ = await executor.execute("parse.docstrings", context, step)

        assert len(result) == 2
        assert result[0]["function"] == "calculate"
        assert result[1]["function"] == "validate"

    @pytest.mark.asyncio
    async def test_parse_docstrings_empty_input(self, executor):
        """Test parsing empty input returns empty list."""
        context = LoopContext(input="")
        step = LoopStep(name="parse", tool="parse.docstrings")

        result, _, _ = await executor.execute("parse.docstrings", context, step)

        assert result == []

    @pytest.mark.asyncio
    async def test_parse_docstrings_malformed_lines(self, executor):
        """Test parsing skips malformed lines."""
        llm_output = """FUNC:valid|Valid docstring
FUNC:no_pipe_here
FUNC:|empty_name
FUNC:also_valid|Another valid one"""
        context = LoopContext(input=llm_output)
        step = LoopStep(name="parse", tool="parse.docstrings")

        result, _, _ = await executor.execute("parse.docstrings", context, step)

        assert len(result) == 2
        assert result[0]["function"] == "valid"
        assert result[1]["function"] == "also_valid"

    @pytest.mark.asyncio
    async def test_parse_docstrings_preserves_pipe_in_docstring(self, executor):
        """Test that pipes in docstring are preserved."""
        llm_output = "FUNC:filter|Filters items where x | y is true"
        context = LoopContext(input=llm_output)
        step = LoopStep(name="parse", tool="parse.docstrings")

        result, _, _ = await executor.execute("parse.docstrings", context, step)

        assert len(result) == 1
        assert result[0]["docstring"] == "Filters items where x | y is true"


class TestParseDocstringOutput:
    """Direct tests for _parse_docstring_output method."""

    @pytest.fixture
    def executor(self, tmp_path):
        return MossToolExecutor(root=tmp_path)

    def test_parse_basic(self, executor):
        output = "FUNC:test|A test function"
        result = executor._parse_docstring_output(output)
        assert result == [{"function": "test", "docstring": "A test function"}]

    def test_parse_multiple(self, executor):
        output = "FUNC:a|First\nFUNC:b|Second"
        result = executor._parse_docstring_output(output)
        assert len(result) == 2

    def test_parse_strips_whitespace(self, executor):
        output = "FUNC:  spaced  |  has spaces  "
        result = executor._parse_docstring_output(output)
        assert result[0]["function"] == "spaced"
        assert result[0]["docstring"] == "has spaces"

    def test_parse_ignores_empty_lines(self, executor):
        output = "\n\nFUNC:test|value\n\n"
        result = executor._parse_docstring_output(output)
        assert len(result) == 1


class TestApplyDocstrings:
    """Tests for _apply_docstrings method."""

    @pytest.fixture
    def executor(self, tmp_path):
        return MossToolExecutor(root=tmp_path)

    def test_apply_docstrings_to_file(self, executor, tmp_path):
        """Test applying docstrings to a Python file."""
        # Create a test file with undocumented functions
        test_file = tmp_path / "test_module.py"
        test_file.write_text("""def foo():
    pass

def bar(x, y):
    return x + y
""")

        docstrings = [
            {"function": "foo", "docstring": "Do foo things."},
            {"function": "bar", "docstring": "Add two numbers."},
        ]

        result = executor._apply_docstrings(str(test_file), docstrings)

        assert "foo" in result["applied"]
        assert "bar" in result["applied"]
        assert result["errors"] == []

        # Verify the file was modified
        modified = test_file.read_text()
        assert '"""Do foo things."""' in modified
        assert '"""Add two numbers."""' in modified

    def test_apply_docstrings_function_not_found(self, executor, tmp_path):
        """Test handling of functions that don't exist."""
        test_file = tmp_path / "test_module.py"
        test_file.write_text("def foo():\n    pass\n")

        docstrings = [
            {"function": "nonexistent", "docstring": "Should not apply."},
        ]

        result = executor._apply_docstrings(str(test_file), docstrings)

        assert result["applied"] == []
        assert "Function not found: nonexistent" in result["errors"]

    def test_apply_docstrings_file_not_found(self, executor, tmp_path):
        """Test handling of missing files."""
        result = executor._apply_docstrings(
            str(tmp_path / "nonexistent.py"),
            [{"function": "foo", "docstring": "test"}],
        )

        assert result["applied"] == []
        assert any("not found" in e.lower() for e in result["errors"])

    def test_apply_docstrings_preserves_indentation(self, executor, tmp_path):
        """Test that docstrings are properly indented."""
        test_file = tmp_path / "test_module.py"
        test_file.write_text("""class MyClass:
    def method(self):
        pass
""")

        docstrings = [{"function": "method", "docstring": "A method."}]

        result = executor._apply_docstrings(str(test_file), docstrings)

        assert "method" in result["applied"]
        modified = test_file.read_text()
        # Verify the docstring is indented correctly (8 spaces for method body)
        assert '        """A method."""' in modified

    @pytest.mark.asyncio
    async def test_patch_docstrings_via_execute(self, executor, tmp_path):
        """Test patch.docstrings tool via executor.execute."""
        test_file = tmp_path / "test.py"
        test_file.write_text("def test_fn():\n    pass\n")

        docstrings = [{"function": "test_fn", "docstring": "Test function."}]
        context = LoopContext(input={"file_path": str(test_file)})
        step = LoopStep(
            name="apply",
            tool="patch.docstrings",
            input_from="parse",
        )
        # Set up context with parse output
        context = context.with_step("parse", docstrings)

        result, tokens_in, tokens_out = await executor.execute("patch.docstrings", context, step)

        assert "test_fn" in result["applied"]
        assert tokens_in == 0
        assert tokens_out == 0


class TestMCPServerConfig:
    """Tests for MCPServerConfig dataclass."""

    def test_basic_config(self):
        config = MCPServerConfig(command="uv", args=["run", "moss-mcp"])
        assert config.command == "uv"
        assert config.args == ["run", "moss-mcp"]
        assert config.cwd is None
        assert config.env is None

    def test_config_with_all_options(self):
        config = MCPServerConfig(
            command="npx",
            args=["@anthropic/mcp-server-filesystem"],
            cwd="/tmp",
            env={"DEBUG": "1"},
        )
        assert config.command == "npx"
        assert config.cwd == "/tmp"
        assert config.env == {"DEBUG": "1"}


class TestMCPToolExecutor:
    """Tests for MCPToolExecutor."""

    def test_init(self):
        config = MCPServerConfig(command="test", args=["arg"])
        executor = MCPToolExecutor(config)
        assert executor.config == config
        assert executor._session is None
        assert executor._tools == {}

    def test_list_tools_empty_before_connect(self):
        config = MCPServerConfig(command="test", args=["arg"])
        executor = MCPToolExecutor(config)
        assert executor.list_tools() == []

    @pytest.mark.asyncio
    async def test_context_manager_protocol(self):
        """Test that MCPToolExecutor can be used as async context manager."""
        config = MCPServerConfig(command="test", args=["arg"])
        executor = MCPToolExecutor(config)
        # Just verify the methods exist
        assert hasattr(executor, "__aenter__")
        assert hasattr(executor, "__aexit__")


class TestCompositeToolExecutor:
    """Tests for CompositeToolExecutor."""

    @pytest.fixture
    def mock_executor(self):
        """Create a mock executor for testing."""

        class MockExecutor:
            async def execute(self, tool_name, context, step):
                return f"mock:{tool_name}", 0, 0

        return MockExecutor()

    def test_init(self, mock_executor):
        composite = CompositeToolExecutor({"test.": mock_executor})
        assert "test." in composite.executors
        assert composite.default is None

    def test_init_with_default(self, mock_executor):
        composite = CompositeToolExecutor({}, default=mock_executor)
        assert composite.default == mock_executor

    def test_get_executor_matches_prefix(self, mock_executor):
        composite = CompositeToolExecutor({"foo.": mock_executor})
        executor, stripped = composite._get_executor("foo.bar")
        assert executor == mock_executor
        assert stripped == "bar"

    def test_get_executor_first_match_wins(self, mock_executor):
        class OtherExecutor:
            pass

        other = OtherExecutor()
        # "foo.bar" matches both "foo." and "foo.b"
        # First matching prefix in dict order wins
        composite = CompositeToolExecutor({"foo.": mock_executor, "foo.b": other})
        executor, stripped = composite._get_executor("foo.bar")
        # "foo." matches first
        assert executor == mock_executor
        assert stripped == "bar"

    def test_get_executor_uses_default(self, mock_executor):
        composite = CompositeToolExecutor({}, default=mock_executor)
        executor, stripped = composite._get_executor("unknown.tool")
        assert executor == mock_executor
        assert stripped == "unknown.tool"

    def test_get_executor_no_match_raises(self):
        composite = CompositeToolExecutor({"foo.": None})
        with pytest.raises(ValueError, match="No executor found"):
            composite._get_executor("bar.tool")

    @pytest.mark.asyncio
    async def test_execute_routes_correctly(self, mock_executor):
        composite = CompositeToolExecutor({"test.": mock_executor})
        context = LoopContext()
        step = LoopStep(name="s", tool="test.hello")

        result, tokens_in, tokens_out = await composite.execute("test.hello", context, step)

        assert result == "mock:hello"
        assert tokens_in == 0
        assert tokens_out == 0

    @pytest.mark.asyncio
    async def test_execute_with_default(self, mock_executor):
        composite = CompositeToolExecutor({}, default=mock_executor)
        context = LoopContext()
        step = LoopStep(name="s", tool="any.tool")

        result, _, _ = await composite.execute("any.tool", context, step)

        assert result == "mock:any.tool"

    @pytest.mark.asyncio
    async def test_execute_with_multiple_executors(self, tmp_path):
        """Test routing to different executors."""

        class ExecutorA:
            async def execute(self, tool_name, context, step):
                return f"A:{tool_name}", 0, 0

        class ExecutorB:
            async def execute(self, tool_name, context, step):
                return f"B:{tool_name}", 0, 0

        composite = CompositeToolExecutor(
            {
                "a.": ExecutorA(),
                "b.": ExecutorB(),
            }
        )

        context = LoopContext()
        step_a = LoopStep(name="s", tool="a.foo")
        step_b = LoopStep(name="s", tool="b.bar")

        result_a, _, _ = await composite.execute("a.foo", context, step_a)
        result_b, _, _ = await composite.execute("b.bar", context, step_b)

        assert result_a == "A:foo"
        assert result_b == "B:bar"


class TestLoopSerialization:
    """Tests for loop serialization (JSON/YAML)."""

    def test_step_to_dict(self):
        from moss.agent_loop import ErrorAction, LoopStep, StepType

        step = LoopStep(
            name="test",
            tool="skeleton.format",
            step_type=StepType.LLM,
            input_from="prev",
            on_error=ErrorAction.RETRY,
            max_retries=5,
        )
        d = step.to_dict()
        assert d["name"] == "test"
        assert d["tool"] == "skeleton.format"
        assert d["step_type"] == "llm"
        assert d["input_from"] == "prev"
        assert d["on_error"] == "retry"
        assert d["max_retries"] == 5

    def test_step_from_dict(self):
        from moss.agent_loop import ErrorAction, LoopStep, StepType

        d = {
            "name": "test",
            "tool": "patch.apply",
            "step_type": "hybrid",
            "on_error": "skip",
        }
        step = LoopStep.from_dict(d)
        assert step.name == "test"
        assert step.tool == "patch.apply"
        assert step.step_type == StepType.HYBRID
        assert step.on_error == ErrorAction.SKIP
        assert step.max_retries == 3  # default

    def test_loop_to_dict(self):
        loop = AgentLoop(
            name="test",
            steps=[
                LoopStep(name="s1", tool="tool1"),
                LoopStep(name="s2", tool="tool2"),
            ],
            exit_conditions=["s2.success"],
        )
        d = loop.to_dict()
        assert d["name"] == "test"
        assert len(d["steps"]) == 2
        assert d["entry"] == "s1"
        assert "s2.success" in d["exit_conditions"]

    def test_loop_from_dict(self):
        d = {
            "name": "custom",
            "steps": [
                {"name": "s1", "tool": "tool1"},
                {"name": "s2", "tool": "tool2", "input_from": "s1"},
            ],
            "max_steps": 20,
        }
        loop = AgentLoop.from_dict(d)
        assert loop.name == "custom"
        assert len(loop.steps) == 2
        assert loop.max_steps == 20
        assert loop.entry == "s1"  # default to first step

    def test_loop_roundtrip(self):
        original = AgentLoop(
            name="roundtrip",
            steps=[
                LoopStep(name="s1", tool="tool1"),
                LoopStep(name="s2", tool="tool2"),
            ],
        )
        d = original.to_dict()
        loaded = AgentLoop.from_dict(d)

        assert original.name == loaded.name
        assert len(original.steps) == len(loaded.steps)
        assert original.entry == loaded.entry
        for orig_step, loaded_step in zip(original.steps, loaded.steps, strict=True):
            assert orig_step.name == loaded_step.name
            assert orig_step.tool == loaded_step.tool
            assert orig_step.step_type == loaded_step.step_type

    def test_dump_load_json(self):
        from moss.agent_loop import dump_loop_json, load_loop_json

        loop = AgentLoop(name="json_test", steps=[LoopStep(name="s1", tool="t1")])
        json_str = dump_loop_json(loop)
        loaded = load_loop_json(json_str)

        assert loop.name == loaded.name
        assert len(loop.steps) == len(loaded.steps)

    def test_dump_load_yaml(self):
        from moss.agent_loop import dump_loop_yaml, load_loop_yaml

        loop = AgentLoop(name="yaml_test", steps=[LoopStep(name="s1", tool="t1")])
        yaml_str = dump_loop_yaml(loop)
        loaded = load_loop_yaml(yaml_str)

        assert loop.name == loaded.name
        assert len(loop.steps) == len(loaded.steps)

    def test_dump_json_to_file(self, tmp_path):
        from moss.agent_loop import dump_loop_json, load_loop_json

        loop = AgentLoop(name="file_test", steps=[LoopStep(name="s1", tool="t1")])
        path = tmp_path / "loop.json"
        dump_loop_json(loop, path)

        assert path.exists()
        loaded = load_loop_json(path)
        assert loop.name == loaded.name

    def test_dump_yaml_to_file(self, tmp_path):
        from moss.agent_loop import dump_loop_yaml, load_loop_yaml

        loop = AgentLoop(name="yaml_file_test", steps=[LoopStep(name="s1", tool="t1")])
        path = tmp_path / "loop.yaml"
        dump_loop_yaml(loop, path)

        assert path.exists()
        loaded = load_loop_yaml(path)
        assert loop.name == loaded.name


class TestConfigSerialization:
    """Tests for config serialization."""

    def test_llm_config_to_dict(self):
        config = LLMConfig(
            model="test-model",
            temperature=0.5,
            models=["m1", "m2"],
            rotation="round_robin",
        )
        d = config.to_dict()
        assert d["model"] == "test-model"
        assert d["temperature"] == 0.5
        assert d["models"] == ["m1", "m2"]
        assert d["rotation"] == "round_robin"

    def test_llm_config_from_dict(self):
        d = {"model": "custom", "temperature": 0.7, "mock": True}
        config = LLMConfig.from_dict(d)
        assert config.model == "custom"
        assert config.temperature == 0.7
        assert config.mock is True
        assert config.models == []  # default

    def test_llm_config_roundtrip(self):
        original = LLMConfig(
            model="test",
            models=["a", "b"],
            rotation="random",
            temperature=0.3,
            max_tokens=100,
        )
        d = original.to_dict()
        loaded = LLMConfig.from_dict(d)

        assert original.model == loaded.model
        assert original.models == loaded.models
        assert original.rotation == loaded.rotation
        assert original.temperature == loaded.temperature
        assert original.max_tokens == loaded.max_tokens

    def test_mcp_config_to_dict(self):
        config = MCPServerConfig(
            command="uv",
            args=["run", "test"],
            cwd="/tmp",
            env={"KEY": "value"},
        )
        d = config.to_dict()
        assert d["command"] == "uv"
        assert d["args"] == ["run", "test"]
        assert d["cwd"] == "/tmp"
        assert d["env"] == {"KEY": "value"}

    def test_mcp_config_from_dict(self):
        d = {"command": "npx", "args": ["@test/server"]}
        config = MCPServerConfig.from_dict(d)
        assert config.command == "npx"
        assert config.args == ["@test/server"]
        assert config.cwd is None
        assert config.env is None

    def test_mcp_config_roundtrip(self):
        original = MCPServerConfig(
            command="python",
            args=["-m", "server"],
            cwd="/app",
            env={"DEBUG": "1"},
        )
        d = original.to_dict()
        loaded = MCPServerConfig.from_dict(d)

        assert original.command == loaded.command
        assert original.args == loaded.args
        assert original.cwd == loaded.cwd
        assert original.env == loaded.env


class TestEphemeralOutputCaching:
    """Tests for ephemeral output caching in LLMToolExecutor."""

    @pytest.fixture
    def executor(self):
        """Create an LLMToolExecutor with caching enabled."""
        return LLMToolExecutor(
            config=LLMConfig(mock=True),
            cache_large_outputs=True,
            large_output_threshold=100,  # Low threshold for testing
        )

    @pytest.fixture
    def executor_no_cache(self):
        """Create an LLMToolExecutor with caching disabled."""
        return LLMToolExecutor(
            config=LLMConfig(mock=True),
            cache_large_outputs=False,
        )

    def test_small_output_not_cached(self, executor):
        """Small outputs should pass through unchanged."""
        result = ("small output", 10, 5)
        cached = executor._maybe_cache_output(result)

        assert cached == result
        assert cached[0] == "small output"

    def test_large_output_cached(self, executor):
        """Large outputs should be cached with preview."""
        large_output = "x" * 200  # Above 100 char threshold
        result = (large_output, 10, 5)
        cached = executor._maybe_cache_output(result)

        # Output should contain preview + cache ID
        assert "Cache ID:" in cached[0]
        assert "x" in cached[0]  # Preview should contain some content
        assert cached[1] == 10  # tokens preserved
        assert cached[2] == 5

    def test_caching_disabled(self, executor_no_cache):
        """When caching disabled, large outputs pass through."""
        large_output = "x" * 200
        result = (large_output, 10, 5)
        cached = executor_no_cache._maybe_cache_output(result)

        assert cached == result
        assert "Cache ID:" not in cached[0]

    def test_dict_output_cached(self, executor):
        """Dict outputs should be cached based on string representation."""
        large_dict = {"data": "x" * 200}
        result = (large_dict, 10, 5)
        cached = executor._maybe_cache_output(result)

        # Dict gets converted to string for caching
        assert "Cache ID:" in cached[0]

    def test_non_string_non_dict_passes_through(self, executor):
        """Non-string, non-dict outputs should pass through."""
        result = (123, 10, 5)
        cached = executor._maybe_cache_output(result)

        assert cached == result

    @pytest.mark.asyncio
    async def test_cache_get_operation(self, executor):
        """Test cache.get retrieves cached content."""
        # First, manually cache something
        cache = executor._get_ephemeral_cache()
        cache_id = cache.store("full cached content")

        context = LoopContext(last=cache_id)
        step = LoopStep(name="retrieve", tool="cache.get")

        result, tokens_in, tokens_out = await executor._execute_cache("cache.get", context, step)

        assert result == "full cached content"
        assert tokens_in == 0
        assert tokens_out == 0

    @pytest.mark.asyncio
    async def test_cache_get_not_found(self, executor):
        """Test cache.get with invalid ID."""
        context = LoopContext(last="nonexistent_id")
        step = LoopStep(name="retrieve", tool="cache.get")

        result, _, _ = await executor._execute_cache("cache.get", context, step)

        assert "not found" in result.lower()

    @pytest.mark.asyncio
    async def test_cache_stats_operation(self, executor):
        """Test cache.stats returns statistics."""
        # Add something to cache
        cache = executor._get_ephemeral_cache()
        cache.store("test content")

        context = LoopContext()
        step = LoopStep(name="stats", tool="cache.stats")

        result, _, _ = await executor._execute_cache("cache.stats", context, step)

        assert "entries" in result
        assert "total_size" in result  # Stats include size info

    @pytest.mark.asyncio
    async def test_unknown_cache_operation(self, executor):
        """Test unknown cache operation returns error."""
        context = LoopContext()
        step = LoopStep(name="unknown", tool="cache.unknown")

        result, _, _ = await executor._execute_cache("cache.unknown", context, step)

        assert "Unknown cache operation" in result
