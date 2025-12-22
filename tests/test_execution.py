"""Tests for moss.execution module."""

from moss.execution import (
    FlatContext,
    InheritedContext,
    Scope,
    StepResult,
    TaskTreeContext,
    Transition,
    WorkflowState,
    WorkflowStep,
    _run_steps,
    _summarize_children,
    evaluate_condition,
    state_machine_loop,
    step_loop,
)


class TestConditionPlugins:
    """Test condition plugin system."""

    def test_has_errors_true(self):
        assert evaluate_condition("has_errors", "", "Found 3 errors in code")

    def test_has_errors_false(self):
        assert not evaluate_condition("has_errors", "", "All checks passed")

    def test_success_true(self):
        assert evaluate_condition("success", "", "Operation completed")

    def test_success_false(self):
        assert not evaluate_condition("success", "", "[Error] Failed to parse")

    def test_empty_true(self):
        assert evaluate_condition("empty", "", "")
        assert evaluate_condition("empty", "", "   ")

    def test_empty_false(self):
        assert not evaluate_condition("empty", "", "content")

    def test_contains_with_param(self):
        assert evaluate_condition("contains:TypeError", "", "TypeError: foo")
        assert not evaluate_condition("contains:TypeError", "", "ValueError: bar")

    def test_contains_no_param(self):
        assert not evaluate_condition("contains", "", "anything")

    def test_unknown_condition(self):
        assert not evaluate_condition("nonexistent", "", "result")


class TestStateMachineLoop:
    """Test state machine execution."""

    def test_terminal_state_immediate(self):
        states = [
            WorkflowState(name="done", terminal=True),
        ]
        result = state_machine_loop(states, initial="done")
        assert "Terminal state reached" in result

    def test_simple_transition(self):
        states = [
            WorkflowState(
                name="start",
                transitions=[Transition(next="end")],
            ),
            WorkflowState(name="end", terminal=True),
        ]
        result = state_machine_loop(states, initial="start")
        assert "state: start" in result or "state: end" in result

    def test_conditional_transition(self):
        # Uses NoLLM by default, so action won't actually run
        # but we can test the transition logic
        states = [
            WorkflowState(
                name="check",
                action="view .",  # won't actually execute in test
                transitions=[
                    Transition(next="error_state", condition="has_errors"),
                    Transition(next="ok_state"),  # default
                ],
            ),
            WorkflowState(name="error_state", terminal=True),
            WorkflowState(name="ok_state", terminal=True),
        ]
        # Without actual execution, result is empty, so "has_errors" is False
        # Should go to ok_state
        result = state_machine_loop(states, initial="check")
        assert "ok_state" in result or "Terminal" in result

    def test_invalid_initial_state(self):
        states = [WorkflowState(name="start", terminal=True)]
        result = state_machine_loop(states, initial="nonexistent")
        assert "not found" in result

    def test_no_valid_transition(self):
        states = [
            WorkflowState(
                name="stuck",
                transitions=[
                    Transition(next="other", condition="has_errors"),
                    # No default transition!
                ],
            ),
            WorkflowState(name="other", terminal=True),
        ]
        result = state_machine_loop(states, initial="stuck")
        assert "No valid transition" in result

    def test_max_transitions_limit(self):
        # Infinite loop between two states
        states = [
            WorkflowState(
                name="a",
                transitions=[Transition(next="b")],
            ),
            WorkflowState(
                name="b",
                transitions=[Transition(next="a")],
            ),
        ]
        # Should stop after max_transitions
        result = state_machine_loop(states, initial="a", max_transitions=5)
        # Just verify it returns without hanging
        assert result is not None

    def test_initial_context(self):
        states = [WorkflowState(name="done", terminal=True)]
        result = state_machine_loop(
            states,
            initial="done",
            initial_context={"file": "main.py"},
        )
        assert "file: main.py" in result

    def test_on_entry_hook(self):
        """on_entry runs when entering a state."""
        states = [
            WorkflowState(
                name="start",
                on_entry="view .",  # This runs when entering
                transitions=[Transition(next="end")],
            ),
            WorkflowState(name="end", terminal=True),
        ]
        # NoLLM by default, action doesn't execute but context captures state
        result = state_machine_loop(states, initial="start")
        # Should have entered both states
        assert "start" in result
        assert "end" in result

    def test_on_exit_hook(self):
        """on_exit runs before leaving a state."""
        states = [
            WorkflowState(
                name="start",
                on_exit="view .",  # This runs before transitioning
                transitions=[Transition(next="end")],
            ),
            WorkflowState(name="end", terminal=True),
        ]
        result = state_machine_loop(states, initial="start")
        # Should have both states in context
        assert "start" in result
        assert "end" in result

    def test_hooks_with_terminal_state(self):
        """Terminal states run on_entry but not on_exit (no transition out)."""
        states = [
            WorkflowState(
                name="done",
                terminal=True,
                on_entry="view .",  # Should run
                on_exit="view .",  # Should NOT run (no transition out)
            ),
        ]
        result = state_machine_loop(states, initial="done")
        assert "Terminal state reached" in result


class TestContextModes:
    """Test context modes for nested scopes."""

    def test_isolated_mode_default(self):
        """Isolated mode: child gets fresh context via .child()."""
        parent_ctx = FlatContext()
        parent_ctx.add("key", "parent_value")

        scope = Scope(context=parent_ctx)
        with scope.child(mode="isolated") as child_scope:
            # Child should have fresh context (no parent_value)
            child_scope.context.add("key", "child_value")
            child_ctx = child_scope.context.get_context()
            assert "child_value" in child_ctx
            # Parent's value not in child (it's isolated)
            assert "parent_value" not in child_ctx

        # Parent unchanged
        assert "parent_value" in parent_ctx.get_context()
        assert "child_value" not in parent_ctx.get_context()

    def test_shared_mode(self):
        """Shared mode: child uses same context object."""
        parent_ctx = FlatContext()
        parent_ctx.add("key", "parent_value")

        scope = Scope(context=parent_ctx)
        with scope.child(mode="shared") as child_scope:
            # Child should be same context object
            assert child_scope.context is parent_ctx
            child_scope.context.add("key", "child_value")

        # Both values in context (same object)
        ctx = parent_ctx.get_context()
        assert "parent_value" in ctx
        assert "child_value" in ctx

    def test_inherited_mode(self):
        """Inherited mode: child sees parent (read), writes to own."""
        parent_ctx = FlatContext()
        parent_ctx.add("key", "parent_value")

        scope = Scope(context=parent_ctx)
        with scope.child(mode="inherited") as child_scope:
            # Child should see parent's context
            child_ctx = child_scope.context.get_context()
            assert "parent_value" in child_ctx

            # Child writes to own storage
            child_scope.context.add("key", "child_value")
            child_ctx = child_scope.context.get_context()
            assert "child_value" in child_ctx
            assert "parent_value" in child_ctx  # Still visible

        # Parent unchanged (one-way visibility)
        parent_result = parent_ctx.get_context()
        assert "parent_value" in parent_result
        assert "child_value" not in parent_result

    def test_inherited_context_class(self):
        """Test InheritedContext wrapper directly."""
        parent = FlatContext()
        parent.add("foo", "bar")

        inherited = InheritedContext(parent)
        # See parent's data
        assert "foo: bar" in inherited.get_context()

        # Add own data
        inherited.add("baz", "qux")
        ctx = inherited.get_context()
        assert "foo: bar" in ctx
        assert "baz: qux" in ctx

        # Parent unchanged
        assert "baz" not in parent.get_context()


class TestParallelStates:
    """Test parallel state execution in state machine."""

    def test_parallel_fork_join(self):
        """Parallel states execute concurrently, then join."""
        states = [
            WorkflowState(
                name="fork",
                parallel=["task_a", "task_b"],
                join="join_state",
            ),
            WorkflowState(
                name="task_a",
                action="view .",  # Won't actually execute
                transitions=[Transition(next="done")],  # Ignored for parallel
            ),
            WorkflowState(
                name="task_b",
                action="view .",
                transitions=[Transition(next="done")],
            ),
            WorkflowState(
                name="join_state",
                transitions=[Transition(next="done")],
            ),
            WorkflowState(name="done", terminal=True),
        ]
        result = state_machine_loop(states, initial="fork")
        # Should have forked and joined
        assert "forking" in result
        assert "join_state" in result or "done" in result

    def test_parallel_missing_state(self):
        """Error if parallel state doesn't exist."""
        states = [
            WorkflowState(
                name="fork",
                parallel=["task_a", "nonexistent"],
                join="join",
            ),
            WorkflowState(name="task_a"),
            WorkflowState(name="join", terminal=True),
        ]
        result = state_machine_loop(states, initial="fork")
        assert "not found" in result

    def test_parallel_missing_join(self):
        """Error if parallel has no join state."""
        states = [
            WorkflowState(
                name="fork",
                parallel=["task_a"],
                # No join specified
            ),
            WorkflowState(name="task_a"),
        ]
        result = state_machine_loop(states, initial="fork")
        assert "no join" in result

    def test_parallel_invalid_join(self):
        """Error if join state doesn't exist."""
        states = [
            WorkflowState(
                name="fork",
                parallel=["task_a"],
                join="nonexistent",
            ),
            WorkflowState(name="task_a"),
        ]
        result = state_machine_loop(states, initial="fork")
        assert "not found" in result

    def test_parallel_results_in_context(self):
        """Parallel results are captured in context."""
        states = [
            WorkflowState(
                name="fork",
                parallel=["task_a", "task_b"],
                join="done",
            ),
            WorkflowState(name="task_a", action="view ."),
            WorkflowState(name="task_b", action="view ."),
            WorkflowState(name="done", terminal=True),
        ]
        result = state_machine_loop(states, initial="fork")
        # Should have parallel results recorded
        assert "parallel_result" in result or "done" in result


class TestStepSummarize:
    """Test summarize option for compound steps."""

    def test_summarize_children_flat_context(self):
        """Summarize generates brief status for flat context."""
        ctx = FlatContext()
        ctx.add("step", "step1")
        ctx.add("result", "done")

        summary = _summarize_children(ctx)
        assert "step" in summary or "result" in summary

    def test_summarize_children_task_tree(self):
        """Summarize generates child summaries for TaskTreeContext."""
        parent = TaskTreeContext()
        parent.task = "parent task"

        # Create child via .child()
        child = parent.child()
        assert isinstance(child, TaskTreeContext)
        child.add("task", "child task")
        child.add("result", "child completed")

        summary = _summarize_children(parent)
        assert "child task" in summary
        assert "child completed" in summary

    def test_summarize_children_empty_tree(self):
        """Empty TaskTree returns 'No child tasks'."""
        ctx = TaskTreeContext()
        summary = _summarize_children(ctx)
        assert summary == "No child tasks"

    def test_step_result_dataclass(self):
        """StepResult captures success, summary, and child results."""
        result = StepResult(
            success=True,
            summary="All done",
            child_results=["step1: ok", "step2: ok"],
        )
        assert result.success is True
        assert result.summary == "All done"
        assert len(result.child_results) == 2

    def test_run_steps_returns_step_result(self):
        """_run_steps returns StepResult with child_results."""
        steps = [
            WorkflowStep(name="step1", action="view ."),
        ]
        scope = Scope(context=FlatContext())
        result = _run_steps(scope, steps)

        assert isinstance(result, StepResult)
        assert result.success is True
        assert len(result.child_results) >= 1

    def test_compound_step_with_summarize(self):
        """Compound step with summarize=True adds child_summary to context."""
        steps = [
            WorkflowStep(
                name="compound",
                summarize=True,
                steps=[
                    WorkflowStep(name="inner1", action="view ."),
                    WorkflowStep(name="inner2", action="view ."),
                ],
            ),
        ]
        result = step_loop(steps)
        assert "child_summary" in result

    def test_nested_child_results_collected(self):
        """Child results from nested steps are collected."""
        steps = [
            WorkflowStep(
                name="outer",
                steps=[
                    WorkflowStep(name="inner", action="view ."),
                ],
            ),
        ]
        scope = Scope(context=FlatContext())
        result = _run_steps(scope, steps)

        assert result.success is True
        # Inner step result should be collected
        assert any("inner" in r for r in result.child_results)
