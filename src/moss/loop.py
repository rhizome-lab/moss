"""Silent Loop: Draft → Validate → Fix → Commit orchestration."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum, auto
from pathlib import Path
from typing import Any

from moss.events import EventBus, EventType
from moss.patches import Patch, apply_patch
from moss.shadow_git import CommitHandle, ShadowBranch, ShadowGit
from moss.validators import ValidationResult, ValidatorChain


class LoopStatus(Enum):
    """Status of the validation loop."""

    PENDING = auto()
    RUNNING = auto()
    SUCCESS = auto()
    FAILED = auto()
    STALLED = auto()  # No progress being made
    OSCILLATING = auto()  # Going back and forth


@dataclass
class LoopIteration:
    """Record of a single loop iteration."""

    iteration: int
    timestamp: datetime
    patch_applied: bool
    validation_result: ValidationResult | None
    commit: CommitHandle | None
    error_count: int
    duration_ms: int


@dataclass
class VelocityMetrics:
    """Metrics for tracking loop progress."""

    iterations: int = 0
    errors_fixed: int = 0
    errors_introduced: int = 0
    total_errors: int = 0
    stall_count: int = 0  # Consecutive iterations with no change
    oscillation_count: int = 0  # Back-and-forth error count changes
    last_error_counts: list[int] = field(default_factory=list)

    def record_iteration(self, error_count: int) -> None:
        """Record an iteration's error count."""
        self.iterations += 1
        prev = self.total_errors

        if error_count < prev:
            self.errors_fixed += prev - error_count
            self.stall_count = 0
        elif error_count > prev:
            self.errors_introduced += error_count - prev
            self.stall_count = 0
        else:
            self.stall_count += 1

        # Track oscillation (alternating increase/decrease)
        self.last_error_counts.append(error_count)
        if len(self.last_error_counts) > 4:
            self.last_error_counts.pop(0)

        if len(self.last_error_counts) >= 4:
            # Check for pattern like [5, 3, 5, 3] or [3, 5, 3, 5]
            diffs = [
                self.last_error_counts[i + 1] - self.last_error_counts[i]
                for i in range(len(self.last_error_counts) - 1)
            ]
            if all(d != 0 for d in diffs):
                signs = [d > 0 for d in diffs]
                if signs == [True, False, True] or signs == [False, True, False]:
                    self.oscillation_count += 1

        self.total_errors = error_count

    @property
    def is_stalled(self) -> bool:
        """Check if loop is stalled (no progress for 3+ iterations)."""
        return self.stall_count >= 3

    @property
    def is_oscillating(self) -> bool:
        """Check if loop is oscillating."""
        return self.oscillation_count >= 2

    @property
    def progress_ratio(self) -> float:
        """Ratio of errors fixed to total changes."""
        total = self.errors_fixed + self.errors_introduced
        if total == 0:
            return 1.0
        return self.errors_fixed / total


@dataclass
class LoopConfig:
    """Configuration for the validation loop."""

    max_iterations: int = 10
    stall_threshold: int = 3  # Max iterations without progress
    oscillation_threshold: int = 2  # Max oscillation patterns
    timeout_seconds: int = 300  # Overall timeout
    auto_commit: bool = True  # Commit on success


@dataclass
class LoopResult:
    """Result of running the validation loop."""

    status: LoopStatus
    iterations: list[LoopIteration]
    final_validation: ValidationResult | None
    final_commit: CommitHandle | None
    metrics: VelocityMetrics
    error: str | None = None

    @property
    def success(self) -> bool:
        return self.status == LoopStatus.SUCCESS


class SilentLoop:
    """Orchestrates the draft → validate → fix → commit loop."""

    def __init__(
        self,
        shadow_git: ShadowGit,
        validators: ValidatorChain,
        event_bus: EventBus | None = None,
        config: LoopConfig | None = None,
    ):
        self.shadow_git = shadow_git
        self.validators = validators
        self.event_bus = event_bus
        self.config = config or LoopConfig()

    async def _emit(self, event_type: EventType, payload: dict[str, Any]) -> None:
        """Emit an event if event bus is configured."""
        if self.event_bus:
            await self.event_bus.emit(event_type, payload)

    async def run(
        self,
        branch: ShadowBranch,
        target_path: Path,
        patches: list[Patch],
    ) -> LoopResult:
        """Run the validation loop.

        Args:
            branch: Shadow branch to work on
            target_path: Path to validate
            patches: Initial patches to apply

        Returns:
            LoopResult with final status and history
        """
        iterations: list[LoopIteration] = []
        metrics = VelocityMetrics()
        current_patches = list(patches)

        await self._emit(EventType.TOOL_CALL, {"action": "loop_start", "patches": len(patches)})

        try:
            for i in range(self.config.max_iterations):
                start_time = datetime.now(UTC)

                # Apply patches
                patch_applied = False
                if current_patches:
                    source = target_path.read_text()
                    for patch in current_patches:
                        result = apply_patch(source, patch)
                        if result.success:
                            source = result.patched
                            patch_applied = True
                    if patch_applied:
                        target_path.write_text(source)

                # Commit the changes
                commit = None
                if patch_applied:
                    try:
                        commit = await self.shadow_git.commit(
                            branch,
                            f"Loop iteration {i + 1}",
                            allow_empty=False,
                        )
                        await self._emit(
                            EventType.SHADOW_COMMIT,
                            {"iteration": i + 1, "sha": commit.sha},
                        )
                    except Exception:
                        pass  # No changes to commit

                # Validate
                validation = await self.validators.validate(target_path)
                error_count = validation.error_count

                # Record metrics
                metrics.record_iteration(error_count)

                # Record iteration
                end_time = datetime.now(UTC)
                duration = int((end_time - start_time).total_seconds() * 1000)
                iteration = LoopIteration(
                    iteration=i + 1,
                    timestamp=start_time,
                    patch_applied=patch_applied,
                    validation_result=validation,
                    commit=commit,
                    error_count=error_count,
                    duration_ms=duration,
                )
                iterations.append(iteration)

                await self._emit(
                    EventType.TOOL_CALL,
                    {
                        "action": "loop_iteration",
                        "iteration": i + 1,
                        "errors": error_count,
                        "success": validation.success,
                    },
                )

                # Check for success
                if validation.success:
                    return LoopResult(
                        status=LoopStatus.SUCCESS,
                        iterations=iterations,
                        final_validation=validation,
                        final_commit=commit,
                        metrics=metrics,
                    )

                # Check for stall/oscillation
                if metrics.is_stalled:
                    await self._emit(
                        EventType.VALIDATION_FAILED,
                        {"reason": "stalled", "iterations": i + 1},
                    )
                    return LoopResult(
                        status=LoopStatus.STALLED,
                        iterations=iterations,
                        final_validation=validation,
                        final_commit=None,
                        metrics=metrics,
                        error="Loop stalled - no progress being made",
                    )

                if metrics.is_oscillating:
                    await self._emit(
                        EventType.VALIDATION_FAILED,
                        {"reason": "oscillating", "iterations": i + 1},
                    )
                    return LoopResult(
                        status=LoopStatus.OSCILLATING,
                        iterations=iterations,
                        final_validation=validation,
                        final_commit=None,
                        metrics=metrics,
                        error="Loop oscillating - fixes are being reverted",
                    )

                # Clear patches for next iteration (would need fix generation here)
                current_patches = []

            # Max iterations reached
            return LoopResult(
                status=LoopStatus.FAILED,
                iterations=iterations,
                final_validation=iterations[-1].validation_result if iterations else None,
                final_commit=None,
                metrics=metrics,
                error=f"Max iterations ({self.config.max_iterations}) reached",
            )

        except Exception as e:
            return LoopResult(
                status=LoopStatus.FAILED,
                iterations=iterations,
                final_validation=None,
                final_commit=None,
                metrics=metrics,
                error=str(e),
            )

    async def run_single(
        self,
        branch: ShadowBranch,
        target_path: Path,
        patch: Patch,
    ) -> LoopResult:
        """Run a single patch through validation."""
        return await self.run(branch, target_path, [patch])
