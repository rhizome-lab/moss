"""SWE-bench evaluation harness for moss.

This module provides infrastructure for evaluating moss on SWE-bench,
a benchmark for real-world GitHub issue resolution.

Usage:
    from moss_orchestration.eval.swebench import SWEBenchHarness

    harness = SWEBenchHarness()
    results = harness.run(subset="lite", limit=10)
    print(results.summary())

The harness supports multiple agent strategies:
- "moss": Uses moss structural tools (skeleton, deps, anchors, etc.)
- "bash": Minimal bash-only approach (like mini-swe-agent)
- "hybrid": Moss for context, bash for execution

See also:
- https://www.swebench.com/
- https://github.com/princeton-nlp/SWE-bench
"""

from __future__ import annotations

import json
import subprocess
import tempfile
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from pathlib import Path
from typing import Any

# Optional imports - gracefully degrade if not installed
try:
    from datasets import load_dataset

    HAS_DATASETS = True
except ImportError:
    HAS_DATASETS = False

try:
    import swebench.harness.run_evaluation  # noqa: F401

    HAS_SWEBENCH = True
except ImportError:
    HAS_SWEBENCH = False


class Subset(str, Enum):
    """Available SWE-bench dataset subsets."""

    FULL = "princeton-nlp/SWE-bench"
    LITE = "princeton-nlp/SWE-bench_Lite"
    VERIFIED = "princeton-nlp/SWE-bench_Verified"


class AgentStrategy(str, Enum):
    """Agent strategies for solving instances."""

    MOSS = "moss"  # Use moss structural tools
    BASH = "bash"  # Minimal bash-only (like mini-swe-agent)
    HYBRID = "hybrid"  # Moss for context, bash for execution


@dataclass
class SWEBenchInstance:
    """A single SWE-bench instance (GitHub issue + patch)."""

    instance_id: str
    repo: str
    base_commit: str
    problem_statement: str
    hints_text: str
    patch: str  # Gold patch (for reference)
    test_patch: str  # Tests that validate the fix
    fail_to_pass: list[str]  # Tests that should go from fail to pass
    pass_to_pass: list[str]  # Tests that should stay passing

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SWEBenchInstance:
        """Create instance from HuggingFace dataset row."""
        return cls(
            instance_id=data["instance_id"],
            repo=data["repo"],
            base_commit=data["base_commit"],
            problem_statement=data["problem_statement"],
            hints_text=data.get("hints_text", ""),
            patch=data["patch"],
            test_patch=data["test_patch"],
            fail_to_pass=json.loads(data.get("FAIL_TO_PASS", "[]")),
            pass_to_pass=json.loads(data.get("PASS_TO_PASS", "[]")),
        )


@dataclass
class SWEBenchResult:
    """Result of running moss on a SWE-bench instance."""

    instance_id: str
    success: bool
    generated_patch: str | None
    error: str | None = None
    execution_time_seconds: float = 0.0
    token_count: int = 0
    tool_calls: int = 0

    # Detailed metrics
    tests_passed: int = 0
    tests_failed: int = 0
    tests_error: int = 0


@dataclass
class EvaluationRun:
    """Results from an evaluation run."""

    run_id: str
    subset: Subset
    strategy: AgentStrategy
    started_at: datetime
    completed_at: datetime | None = None
    results: list[SWEBenchResult] = field(default_factory=list)

    @property
    def total(self) -> int:
        return len(self.results)

    @property
    def passed(self) -> int:
        return sum(1 for r in self.results if r.success)

    @property
    def failed(self) -> int:
        return self.total - self.passed

    @property
    def pass_rate(self) -> float:
        return self.passed / self.total if self.total > 0 else 0.0

    def summary(self) -> str:
        """Generate human-readable summary."""
        lines = [
            f"SWE-bench Evaluation: {self.run_id}",
            f"Subset: {self.subset.name}",
            f"Strategy: {self.strategy.value}",
            "",
            f"Results: {self.passed}/{self.total} passed ({self.pass_rate:.1%})",
            "",
        ]

        if self.results:
            # Show failed instances
            failed = [r for r in self.results if not r.success]
            if failed:
                lines.append("Failed instances:")
                for r in failed[:10]:  # Show first 10
                    error_preview = (r.error or "unknown")[:50]
                    lines.append(f"  - {r.instance_id}: {error_preview}")
                if len(failed) > 10:
                    lines.append(f"  ... and {len(failed) - 10} more")

        return "\n".join(lines)

    def to_json(self) -> dict[str, Any]:
        """Export to JSON-serializable dict."""
        return {
            "run_id": self.run_id,
            "subset": self.subset.value,
            "strategy": self.strategy.value,
            "started_at": self.started_at.isoformat(),
            "completed_at": self.completed_at.isoformat() if self.completed_at else None,
            "total": self.total,
            "passed": self.passed,
            "pass_rate": self.pass_rate,
            "results": [
                {
                    "instance_id": r.instance_id,
                    "success": r.success,
                    "error": r.error,
                    "execution_time_seconds": r.execution_time_seconds,
                    "token_count": r.token_count,
                    "tool_calls": r.tool_calls,
                }
                for r in self.results
            ],
        }


class SWEBenchHarness:
    """Harness for running moss on SWE-bench instances.

    This harness loads SWE-bench instances, sets up test environments,
    runs the moss agent, and evaluates results.

    Example:
        harness = SWEBenchHarness()

        # List available instances
        instances = harness.list_instances(subset=Subset.LITE)
        print(f"Found {len(instances)} instances")

        # Run evaluation
        results = harness.run(subset=Subset.LITE, limit=10)
        print(results.summary())

        # Run single instance
        result = harness.run_instance("sympy__sympy-20590")
        print(f"Success: {result.success}")
    """

    def __init__(
        self,
        work_dir: Path | None = None,
        strategy: AgentStrategy = AgentStrategy.MOSS,
        model: str = "claude-sonnet-4-20250514",
        max_iterations: int = 10,
        timeout_seconds: int = 300,
    ) -> None:
        """Initialize the harness.

        Args:
            work_dir: Directory for cloning repos and storing results.
                     Defaults to a temp directory.
            strategy: Agent strategy to use for solving instances.
            model: LLM model to use.
            max_iterations: Maximum agent iterations per instance.
            timeout_seconds: Timeout for each instance.
        """
        self.work_dir = work_dir or Path(tempfile.mkdtemp(prefix="moss-swebench-"))
        self.strategy = strategy
        self.model = model
        self.max_iterations = max_iterations
        self.timeout_seconds = timeout_seconds

        self._instances_cache: dict[str, list[SWEBenchInstance]] = {}

    def _check_dependencies(self) -> None:
        """Check that required dependencies are installed."""
        if not HAS_DATASETS:
            raise ImportError(
                "datasets package not installed. "
                "Install with: pip install 'moss[eval]' or pip install datasets"
            )

    def list_instances(
        self,
        subset: Subset = Subset.LITE,
        limit: int | None = None,
    ) -> list[SWEBenchInstance]:
        """List available instances from a subset.

        Args:
            subset: Which SWE-bench subset to use.
            limit: Maximum number of instances to return.

        Returns:
            List of SWEBenchInstance objects.
        """
        self._check_dependencies()

        cache_key = subset.value
        if cache_key not in self._instances_cache:
            dataset = load_dataset(subset.value, split="test")
            instances = [SWEBenchInstance.from_dict(row) for row in dataset]
            self._instances_cache[cache_key] = instances

        instances = self._instances_cache[cache_key]
        if limit:
            instances = instances[:limit]
        return instances

    def get_instance(self, instance_id: str) -> SWEBenchInstance | None:
        """Get a specific instance by ID.

        Args:
            instance_id: The instance ID (e.g., "sympy__sympy-20590").

        Returns:
            The instance, or None if not found.
        """
        # Try all subsets
        for subset in Subset:
            try:
                instances = self.list_instances(subset)
                for inst in instances:
                    if inst.instance_id == instance_id:
                        return inst
            except Exception:
                continue
        return None

    def run(
        self,
        subset: Subset = Subset.LITE,
        limit: int | None = None,
        instance_ids: list[str] | None = None,
    ) -> EvaluationRun:
        """Run evaluation on a subset of instances.

        Args:
            subset: Which SWE-bench subset to use.
            limit: Maximum number of instances to evaluate.
            instance_ids: Specific instance IDs to evaluate.
                         If provided, subset and limit are ignored.

        Returns:
            EvaluationRun with all results.
        """
        run_id = f"moss-{datetime.now().strftime('%Y%m%d-%H%M%S')}"
        run = EvaluationRun(
            run_id=run_id,
            subset=subset,
            strategy=self.strategy,
            started_at=datetime.now(),
        )

        if instance_ids:
            instances = [
                inst for inst in self.list_instances(subset) if inst.instance_id in instance_ids
            ]
        else:
            instances = self.list_instances(subset, limit)

        for instance in instances:
            result = self.run_instance(instance)
            run.results.append(result)

        run.completed_at = datetime.now()
        return run

    def run_instance(
        self,
        instance: SWEBenchInstance | str,
    ) -> SWEBenchResult:
        """Run moss on a single instance.

        Args:
            instance: Either an SWEBenchInstance or instance ID string.

        Returns:
            SWEBenchResult with the outcome.
        """
        if isinstance(instance, str):
            inst = self.get_instance(instance)
            if inst is None:
                return SWEBenchResult(
                    instance_id=instance,
                    success=False,
                    generated_patch=None,
                    error=f"Instance not found: {instance}",
                )
            instance = inst

        start_time = datetime.now()

        try:
            # Set up the environment
            repo_dir = self._setup_repo(instance)

            # Run the agent
            if self.strategy == AgentStrategy.BASH:
                patch = self._run_bash_agent(instance, repo_dir)
            elif self.strategy == AgentStrategy.MOSS:
                patch = self._run_moss_agent(instance, repo_dir)
            else:  # HYBRID
                patch = self._run_hybrid_agent(instance, repo_dir)

            # Check if patch was generated
            if not patch:
                return SWEBenchResult(
                    instance_id=instance.instance_id,
                    success=False,
                    generated_patch=None,
                    error="No patch generated",
                    execution_time_seconds=(datetime.now() - start_time).total_seconds(),
                )

            # Validate the patch (basic check - apply and run tests)
            success = self._validate_patch(instance, repo_dir, patch)

            return SWEBenchResult(
                instance_id=instance.instance_id,
                success=success,
                generated_patch=patch,
                execution_time_seconds=(datetime.now() - start_time).total_seconds(),
            )

        except Exception as e:
            return SWEBenchResult(
                instance_id=instance.instance_id,
                success=False,
                generated_patch=None,
                error=str(e),
                execution_time_seconds=(datetime.now() - start_time).total_seconds(),
            )

    def _setup_repo(self, instance: SWEBenchInstance) -> Path:
        """Clone and set up the repository for an instance.

        Args:
            instance: The SWE-bench instance.

        Returns:
            Path to the cloned repository.
        """
        repo_dir = self.work_dir / instance.instance_id.replace("/", "__")

        if repo_dir.exists():
            # Reset to base commit
            subprocess.run(
                ["git", "checkout", "-f", instance.base_commit],
                cwd=repo_dir,
                capture_output=True,
                check=True,
            )
        else:
            # Clone the repo
            repo_url = f"https://github.com/{instance.repo}.git"
            subprocess.run(
                ["git", "clone", "--depth=100", repo_url, str(repo_dir)],
                capture_output=True,
                check=True,
            )
            # Checkout base commit
            subprocess.run(
                ["git", "checkout", instance.base_commit],
                cwd=repo_dir,
                capture_output=True,
                check=True,
            )

        return repo_dir

    def _run_bash_agent(
        self,
        instance: SWEBenchInstance,
        repo_dir: Path,
    ) -> str | None:
        """Run minimal bash-only agent (like mini-swe-agent).

        This is a simple implementation that:
        1. Builds a prompt with the problem statement
        2. Asks the LLM to generate a patch
        3. Extracts the patch from the response

        Args:
            instance: The SWE-bench instance.
            repo_dir: Path to the repository.

        Returns:
            Generated patch string, or None if failed.
        """
        # Build prompt
        prompt = self._build_bash_prompt(instance, repo_dir)

        # Call LLM (placeholder - would use moss.llm in real implementation)
        # For now, return None to indicate not implemented
        _ = prompt  # Suppress unused variable warning
        return None  # TODO: Implement LLM call

    def _run_moss_agent(
        self,
        instance: SWEBenchInstance,
        repo_dir: Path,
    ) -> str | None:
        """Run moss-based agent using structural tools.

        This agent uses moss tools to understand the codebase:
        1. `moss skeleton` to understand structure
        2. `moss deps` to find related code
        3. `moss anchors` for precise editing

        Args:
            instance: The SWE-bench instance.
            repo_dir: Path to the repository.

        Returns:
            Generated patch string, or None if failed.
        """
        # Build context using moss tools
        context = self._build_moss_context(instance, repo_dir)

        # Build prompt with structural context
        prompt = self._build_moss_prompt(instance, context)

        # Call LLM (placeholder)
        _ = prompt  # Suppress unused variable warning
        return None  # TODO: Implement LLM call

    def _run_hybrid_agent(
        self,
        instance: SWEBenchInstance,
        repo_dir: Path,
    ) -> str | None:
        """Run hybrid agent: moss for context, bash for execution.

        Args:
            instance: The SWE-bench instance.
            repo_dir: Path to the repository.

        Returns:
            Generated patch string, or None if failed.
        """
        # Use moss for context gathering
        context = self._build_moss_context(instance, repo_dir)

        # Then run bash-style agent with enriched context
        prompt = self._build_hybrid_prompt(instance, repo_dir, context)

        _ = prompt  # Suppress unused variable warning
        return None  # TODO: Implement LLM call

    def _build_bash_prompt(
        self,
        instance: SWEBenchInstance,
        repo_dir: Path,
    ) -> str:
        """Build prompt for bash-only agent."""
        return f"""You are a software engineer tasked with fixing a GitHub issue.

## Repository
{instance.repo} (cloned to {repo_dir})

## Issue
{instance.problem_statement}

{f"## Hints{chr(10)}{instance.hints_text}" if instance.hints_text else ""}

## Task
1. Explore the repository to understand the codebase
2. Locate the relevant code
3. Generate a patch that fixes the issue
4. Output the patch in unified diff format

You can run any bash commands to explore and understand the code.
When ready, output your patch between ```diff and ``` markers.
"""

    def _build_moss_context(
        self,
        instance: SWEBenchInstance,
        repo_dir: Path,
    ) -> dict[str, Any]:
        """Build context using moss structural tools."""
        context: dict[str, Any] = {}

        # Run moss skeleton on likely files
        # (In a real implementation, we'd parse the problem statement
        # to find relevant files, then run moss tools on them)

        try:
            result = subprocess.run(
                ["moss", "skeleton", str(repo_dir)],
                capture_output=True,
                text=True,
                timeout=30,
            )
            context["skeleton"] = result.stdout
        except (OSError, subprocess.SubprocessError, subprocess.TimeoutExpired):
            context["skeleton"] = None

        return context

    def _build_moss_prompt(
        self,
        instance: SWEBenchInstance,
        context: dict[str, Any],
    ) -> str:
        """Build prompt with moss structural context."""
        skeleton = context.get("skeleton", "")
        skeleton_section = f"## Codebase Structure\n{skeleton}\n" if skeleton else ""

        return f"""You are a software engineer using moss structural tools to fix a GitHub issue.

## Repository
{instance.repo}

{skeleton_section}

## Issue
{instance.problem_statement}

{f"## Hints{chr(10)}{instance.hints_text}" if instance.hints_text else ""}

## Task
Use the structural context above to understand the codebase, then generate a patch.
Output your patch in unified diff format between ```diff and ``` markers.
"""

    def _build_hybrid_prompt(
        self,
        instance: SWEBenchInstance,
        repo_dir: Path,
        context: dict[str, Any],
    ) -> str:
        """Build prompt combining moss context with bash capabilities."""
        base_prompt = self._build_moss_prompt(instance, context)
        return (
            base_prompt
            + f"""

## Additional Context
Repository is available at: {repo_dir}
You can run bash commands to explore further if needed.
"""
        )

    def _validate_patch(
        self,
        instance: SWEBenchInstance,
        repo_dir: Path,
        patch: str,
    ) -> bool:
        """Validate a generated patch by applying and running tests.

        Args:
            instance: The SWE-bench instance.
            repo_dir: Path to the repository.
            patch: The generated patch.

        Returns:
            True if the patch resolves the issue, False otherwise.
        """
        # Save patch to file
        patch_file = repo_dir / "generated.patch"
        patch_file.write_text(patch)

        try:
            # Apply the patch
            result = subprocess.run(
                ["git", "apply", str(patch_file)],
                cwd=repo_dir,
                capture_output=True,
                text=True,
            )
            if result.returncode != 0:
                return False

            # Run the fail_to_pass tests
            for test in instance.fail_to_pass:
                result = subprocess.run(
                    ["pytest", test, "-x"],
                    cwd=repo_dir,
                    capture_output=True,
                    timeout=60,
                )
                if result.returncode != 0:
                    return False

            return True

        except (OSError, subprocess.SubprocessError, subprocess.TimeoutExpired):
            return False
        finally:
            # Reset the repo
            subprocess.run(
                ["git", "checkout", "-f", "."],
                cwd=repo_dir,
                capture_output=True,
            )


def run_swebench_evaluation(
    predictions_path: Path,
    run_id: str,
    instance_ids: list[str] | None = None,
    max_workers: int = 4,
) -> dict[str, Any]:
    """Run official SWE-bench evaluation on predictions.

    This uses the official swebench harness for evaluation,
    which requires Docker for reproducible results.

    Args:
        predictions_path: Path to predictions file (JSONL format).
        run_id: Identifier for this evaluation run.
        instance_ids: Specific instances to evaluate.
        max_workers: Number of parallel workers.

    Returns:
        Evaluation results from swebench.
    """
    if not HAS_SWEBENCH:
        raise ImportError(
            "swebench package not installed. "
            "Install with: pip install 'moss[eval]' or pip install swebench"
        )

    # Build command
    cmd = [
        "python",
        "-m",
        "swebench.harness.run_evaluation",
        "--predictions_path",
        str(predictions_path),
        "--run_id",
        run_id,
        "--max_workers",
        str(max_workers),
    ]

    if instance_ids:
        cmd.extend(["--instance_ids", *instance_ids])

    # Run evaluation
    result = subprocess.run(cmd, capture_output=True, text=True)

    return {
        "returncode": result.returncode,
        "stdout": result.stdout,
        "stderr": result.stderr,
    }
