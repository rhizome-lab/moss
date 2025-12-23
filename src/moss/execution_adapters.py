"""Adapter drivers that wrap existing execution loops.

These adapters allow existing synchronous loops to be used through the
driver API, enabling gradual migration to the unified driver model.

Migration path:
1. Keep existing agent_loop, step_loop, state_machine_loop working
2. These adapters wrap them for driver API compatibility
3. Gradually migrate callers to use driver API
4. Eventually deprecate direct loop functions
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from moss.drivers import Action, ActionResult, Context, DriverRegistry

if TYPE_CHECKING:
    from moss.session import Session


@dataclass
class LegacyAgentDriver:
    """Adapter driver wrapping the legacy agent_loop.

    Runs the synchronous agent_loop in a thread pool to maintain
    async compatibility.
    """

    name = "legacy_agent"

    # agent_loop configuration
    max_turns: int = 10
    model: str = "claude-sonnet-4-20250514"
    context_strategy: str = "flat"
    cache_strategy: str = "in_memory"
    retry_strategy: str = "none"

    # Internal state
    _result: str | None = field(default=None, init=False)
    _completed: bool = field(default=False, init=False)

    async def decide_next_step(
        self,
        task: Session,
        context: Context,
    ) -> Action | None:
        if self._completed:
            return None

        # Run entire loop in background (it's synchronous and self-contained)
        loop = asyncio.get_event_loop()
        self._result = await loop.run_in_executor(
            None,
            self._run_agent_loop,
            task.description or task.id,
        )
        self._completed = True

        # Return done action with result
        return Action(tool="done", parameters={"result": self._result})

    async def on_action_complete(
        self,
        task: Session,
        action: Action,
        result: ActionResult,
    ) -> None:
        pass

    def _run_agent_loop(self, task_description: str) -> str:
        """Run the synchronous agent_loop."""
        from moss.execution import (
            CACHE_STRATEGIES,
            CONTEXT_STRATEGIES,
            LLM_STRATEGIES,
            RETRY_STRATEGIES,
            agent_loop,
        )

        context_cls = CONTEXT_STRATEGIES.get(self.context_strategy)
        cache_cls = CACHE_STRATEGIES.get(self.cache_strategy)
        retry_cls = RETRY_STRATEGIES.get(self.retry_strategy)
        llm_cls = LLM_STRATEGIES.get("simple")

        return agent_loop(
            task=task_description,
            context=context_cls() if context_cls else None,
            cache=cache_cls() if cache_cls else None,
            retry=retry_cls() if retry_cls else None,
            llm=llm_cls(model=self.model) if llm_cls else None,
            max_turns=self.max_turns,
        )


@dataclass
class LegacyStepDriver:
    """Adapter driver wrapping the legacy step_loop.

    Takes workflow steps and runs them through the existing step_loop.
    """

    name = "legacy_step"

    # step_loop configuration
    steps: list[dict] = field(default_factory=list)
    context_strategy: str = "flat"
    cache_strategy: str = "in_memory"
    retry_strategy: str = "none"
    initial_context: dict[str, str] = field(default_factory=dict)

    # Internal state
    _result: str | None = field(default=None, init=False)
    _completed: bool = field(default=False, init=False)

    async def decide_next_step(
        self,
        task: Session,
        context: Context,
    ) -> Action | None:
        if self._completed:
            return None

        loop = asyncio.get_event_loop()
        self._result = await loop.run_in_executor(None, self._run_step_loop)
        self._completed = True

        return Action(tool="done", parameters={"result": self._result})

    async def on_action_complete(
        self,
        task: Session,
        action: Action,
        result: ActionResult,
    ) -> None:
        pass

    def _run_step_loop(self) -> str:
        """Run the synchronous step_loop."""
        from moss.execution import (
            CACHE_STRATEGIES,
            CONTEXT_STRATEGIES,
            RETRY_STRATEGIES,
            WorkflowStep,
            step_loop,
        )

        context_cls = CONTEXT_STRATEGIES.get(self.context_strategy)
        cache_cls = CACHE_STRATEGIES.get(self.cache_strategy)
        retry_cls = RETRY_STRATEGIES.get(self.retry_strategy)

        # Convert dict steps to WorkflowStep objects
        workflow_steps = []
        for step_dict in self.steps:
            workflow_steps.append(WorkflowStep(**step_dict))

        return step_loop(
            steps=workflow_steps,
            context=context_cls() if context_cls else None,
            cache=cache_cls() if cache_cls else None,
            retry=retry_cls() if retry_cls else None,
            initial_context=self.initial_context or None,
        )


@dataclass
class LegacyStateMachineDriver:
    """Adapter driver wrapping the legacy state_machine_loop.

    Takes state definitions and runs them through the existing loop.
    """

    name = "legacy_state_machine"

    # state_machine_loop configuration
    states: list[dict] = field(default_factory=list)
    initial: str = ""
    context_strategy: str = "flat"
    cache_strategy: str = "in_memory"
    retry_strategy: str = "none"
    max_transitions: int = 50
    initial_context: dict[str, str] = field(default_factory=dict)
    workflow_path: str | None = None

    # Internal state
    _result: str | None = field(default=None, init=False)
    _completed: bool = field(default=False, init=False)

    async def decide_next_step(
        self,
        task: Session,
        context: Context,
    ) -> Action | None:
        if self._completed:
            return None

        loop = asyncio.get_event_loop()
        self._result = await loop.run_in_executor(None, self._run_state_machine_loop)
        self._completed = True

        return Action(tool="done", parameters={"result": self._result})

    async def on_action_complete(
        self,
        task: Session,
        action: Action,
        result: ActionResult,
    ) -> None:
        pass

    def _run_state_machine_loop(self) -> str:
        """Run the synchronous state_machine_loop."""
        from moss.execution import (
            CACHE_STRATEGIES,
            CONTEXT_STRATEGIES,
            RETRY_STRATEGIES,
            WorkflowState,
            state_machine_loop,
        )

        context_cls = CONTEXT_STRATEGIES.get(self.context_strategy)
        cache_cls = CACHE_STRATEGIES.get(self.cache_strategy)
        retry_cls = RETRY_STRATEGIES.get(self.retry_strategy)

        # Convert dict states to WorkflowState objects
        workflow_states = []
        for state_dict in self.states:
            workflow_states.append(WorkflowState(**state_dict))

        return state_machine_loop(
            states=workflow_states,
            initial=self.initial,
            context=context_cls() if context_cls else None,
            cache=cache_cls() if cache_cls else None,
            retry=retry_cls() if retry_cls else None,
            max_transitions=self.max_transitions,
            initial_context=self.initial_context or None,
            workflow_path=self.workflow_path,
        )


@dataclass
class LegacyWorkflowDriver:
    """Adapter driver for loading and running TOML workflow files.

    Wraps run_workflow() to execute .toml workflow definitions.
    """

    name = "legacy_workflow"

    # Workflow configuration
    workflow_path: str = ""
    initial_context: dict[str, str] = field(default_factory=dict)

    # Internal state
    _result: str | None = field(default=None, init=False)
    _completed: bool = field(default=False, init=False)

    async def decide_next_step(
        self,
        task: Session,
        context: Context,
    ) -> Action | None:
        if self._completed:
            return None

        if not self.workflow_path:
            return None

        loop = asyncio.get_event_loop()
        self._result = await loop.run_in_executor(None, self._run_workflow)
        self._completed = True

        return Action(tool="done", parameters={"result": self._result})

    async def on_action_complete(
        self,
        task: Session,
        action: Action,
        result: ActionResult,
    ) -> None:
        pass

    def _run_workflow(self) -> str:
        """Run the workflow from file."""
        from moss.execution import run_workflow

        return run_workflow(self.workflow_path, initial_context=self.initial_context or None)


# Register adapter drivers
DriverRegistry.register(LegacyAgentDriver)
DriverRegistry.register(LegacyStepDriver)
DriverRegistry.register(LegacyStateMachineDriver)
DriverRegistry.register(LegacyWorkflowDriver)


__all__ = [
    "LegacyAgentDriver",
    "LegacyStateMachineDriver",
    "LegacyStepDriver",
    "LegacyWorkflowDriver",
]
