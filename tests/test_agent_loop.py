"""Tests for Composable Agent Loops (agent_loop.py)."""

import pytest

from moss.agent_loop import (
    AgentLoop,
    AgentLoopRunner,
    BenchmarkTask,
    ErrorAction,
    LLMConfig,
    LLMToolExecutor,
    LoopBenchmark,
    LoopContext,
    LoopMetrics,
    LoopStatus,
    LoopStep,
    StepType,
    analysis_loop,
    critic_loop,
    docstring_loop,
    incremental_loop,
    simple_loop,
)


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


class TestLoopTemplates:
    """Tests for pre-built loop templates."""

    def test_simple_loop_structure(self):
        loop = simple_loop()
        assert loop.name == "simple"
        assert len(loop.steps) == 3
        assert loop.steps[0].name == "understand"
        assert loop.steps[1].name == "act"
        assert loop.steps[2].name == "validate"
        assert "validate.success" in loop.exit_conditions

    def test_critic_loop_structure(self):
        loop = critic_loop()
        assert loop.name == "critic"
        assert len(loop.steps) == 4
        assert loop.steps[1].name == "review"
        assert loop.steps[1].step_type == StepType.LLM

    def test_incremental_loop_structure(self):
        loop = incremental_loop()
        assert loop.name == "incremental"
        assert any(s.name == "decide" for s in loop.steps)

    def test_analysis_loop_structure(self):
        loop = analysis_loop()
        assert loop.name == "analysis"
        assert len(loop.steps) == 2
        assert loop.steps[0].name == "skeleton"
        assert loop.steps[1].name == "analyze"
        assert loop.steps[1].step_type == StepType.LLM
        assert "analyze.success" in loop.exit_conditions

    def test_docstring_loop_structure(self):
        loop = docstring_loop()
        assert loop.name == "docstring"
        assert len(loop.steps) == 2
        assert loop.steps[0].name == "skeleton"
        assert loop.steps[1].name == "identify"
        assert loop.steps[1].step_type == StepType.LLM


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
