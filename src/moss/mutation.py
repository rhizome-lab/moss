"""Mutation testing analyzer.

Runs mutation testing to find undertested code paths. Surviving mutations
indicate places where tests don't verify behavior - a single character change
like `!a` → `a` or `<` → `<=` can completely change behavior without being caught.

Uses mutmut as the underlying mutation testing tool.
"""

from __future__ import annotations

import asyncio
import shutil
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


@dataclass
class SurvivingMutant:
    """A mutation that survived (tests didn't catch it)."""

    file: Path
    line: int
    original: str
    mutated: str
    mutation_type: str  # e.g., "comparison", "arithmetic", "return", "keyword"
    mutant_id: int | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "file": str(self.file),
            "line": self.line,
            "original": self.original,
            "mutated": self.mutated,
            "mutation_type": self.mutation_type,
            "mutant_id": self.mutant_id,
        }


@dataclass
class MutationResult:
    """Result of mutation testing run."""

    total_mutants: int = 0
    killed: int = 0
    survived: int = 0
    timeout: int = 0
    suspicious: int = 0
    skipped: int = 0
    survivors: list[SurvivingMutant] = field(default_factory=list)
    execution_time_seconds: float = 0.0
    tested_files: list[Path] = field(default_factory=list)

    @property
    def mutation_score(self) -> float:
        """Percentage of mutants killed (higher is better).

        Returns 1.0 if no mutants were generated.
        """
        testable = self.total_mutants - self.skipped
        if testable == 0:
            return 1.0
        return self.killed / testable

    @property
    def has_survivors(self) -> bool:
        return self.survived > 0

    def to_dict(self) -> dict[str, Any]:
        return {
            "stats": {
                "total_mutants": self.total_mutants,
                "killed": self.killed,
                "survived": self.survived,
                "timeout": self.timeout,
                "suspicious": self.suspicious,
                "skipped": self.skipped,
                "mutation_score": self.mutation_score,
                "execution_time_seconds": self.execution_time_seconds,
            },
            "survivors": [s.to_dict() for s in self.survivors],
            "tested_files": [str(f) for f in self.tested_files],
        }

    def to_compact(self) -> str:
        """Format as compact single-line summary (token-efficient).

        Example: mutate: 85% score | 17/20 killed | 3 survived
        """
        score_pct = f"{self.mutation_score:.0%}"
        testable = self.total_mutants - self.skipped
        parts = [f"mutate: {score_pct} score"]
        parts.append(f"{self.killed}/{testable} killed")
        if self.survived:
            parts.append(f"{self.survived} survived")
        return " | ".join(parts)

    def to_markdown(self) -> str:
        lines = ["# Mutation Testing Results", ""]

        # Summary stats
        score_pct = f"{self.mutation_score:.0%}"
        testable = self.total_mutants - self.skipped
        lines.append(f"**Mutation Score:** {score_pct} ({self.killed}/{testable} mutants killed)")
        lines.append(f"**Execution Time:** {self._format_time(self.execution_time_seconds)}")
        lines.append("")

        # Breakdown
        lines.append("## Statistics")
        lines.append("")
        lines.append("| Status | Count |")
        lines.append("|--------|-------|")
        lines.append(f"| Killed | {self.killed} |")
        lines.append(f"| Survived | {self.survived} |")
        lines.append(f"| Timeout | {self.timeout} |")
        lines.append(f"| Suspicious | {self.suspicious} |")
        lines.append(f"| Skipped | {self.skipped} |")
        lines.append("")

        if not self.survivors:
            lines.append("All mutants were killed - great test coverage!")
            return "\n".join(lines)

        # Group survivors by file
        lines.append("## Surviving Mutants (Undertested Code)")
        lines.append("")
        lines.append("These mutations survived - your tests don't verify this behavior:")
        lines.append("")

        by_file: dict[Path, list[SurvivingMutant]] = {}
        for survivor in self.survivors:
            by_file.setdefault(survivor.file, []).append(survivor)

        for file_path, mutants in sorted(by_file.items()):
            lines.append(f"### {file_path}")
            lines.append("")
            for m in sorted(mutants, key=lambda x: x.line):
                lines.append(f"- **Line {m.line}**: `{m.original}` → `{m.mutated}`")
                lines.append(f"  - Type: {m.mutation_type}")
            lines.append("")

        return "\n".join(lines)

    @staticmethod
    def _format_time(seconds: float) -> str:
        if seconds < 60:
            return f"{seconds:.1f}s"
        minutes = int(seconds // 60)
        secs = seconds % 60
        return f"{minutes}m {secs:.0f}s"


class MutationAnalyzer:
    """Runs mutation testing via mutmut."""

    def __init__(
        self,
        root: Path,
        *,
        paths_to_mutate: list[Path] | None = None,
        test_command: str | None = None,
    ):
        """Initialize mutation analyzer.

        Args:
            root: Project root directory
            paths_to_mutate: Specific paths to mutate (default: auto-detect src/)
            test_command: Custom test command (default: pytest)
        """
        self.root = root.resolve()
        self.paths_to_mutate = paths_to_mutate
        self.test_command = test_command

    def is_available(self) -> bool:
        """Check if mutmut is installed."""
        return shutil.which("mutmut") is not None

    def get_version(self) -> str | None:
        """Get mutmut version."""
        if not self.is_available():
            return None
        try:
            result = subprocess.run(
                ["mutmut", "version"],
                capture_output=True,
                text=True,
                timeout=10,
            )
            return result.stdout.strip() if result.returncode == 0 else None
        except (OSError, subprocess.SubprocessError):
            return None

    async def run(
        self,
        *,
        quick_check: bool = False,
        paths: list[Path] | None = None,
        since: str | None = None,
    ) -> MutationResult:
        """Run mutation testing.

        Args:
            quick_check: Only test a sample of mutants (faster)
            paths: Specific paths to mutate (overrides init paths)
            since: Only mutate files changed since this git commit

        Returns:
            MutationResult with statistics and surviving mutants
        """
        import time

        start_time = time.time()

        # Determine paths to mutate
        mutate_paths = paths or self.paths_to_mutate
        if mutate_paths is None:
            mutate_paths = self._auto_detect_paths()

        if since:
            mutate_paths = self._filter_changed_since(mutate_paths, since)

        if not mutate_paths:
            return MutationResult(execution_time_seconds=time.time() - start_time)

        # Create temporary setup.cfg with mutmut config
        # (mutmut v3 uses config file instead of CLI flags)
        setup_cfg = self.root / "setup.cfg"
        setup_cfg_backup = None
        if setup_cfg.exists():
            setup_cfg_backup = setup_cfg.read_text()

        try:
            # Write mutmut configuration (v3 uses config file)
            config_lines = ["[mutmut]"]
            paths_str = ",".join(str(p) for p in mutate_paths)
            config_lines.append(f"paths_to_mutate={paths_str}")
            if self.test_command:
                config_lines.append(f"runner={self.test_command}")
            config_lines.append("tests_dir=tests")
            # v3 needs src dir copied for imports to work
            config_lines.append("also_copy=src/")
            config_lines.append("")

            if setup_cfg_backup:
                # Append to existing config
                full_config = setup_cfg_backup + "\n" + "\n".join(config_lines)
            else:
                full_config = "\n".join(config_lines)

            setup_cfg.write_text(full_config)

            # Build mutmut command (v3 simplified)
            cmd = ["mutmut", "run"]

            # Run mutmut
            process = await asyncio.create_subprocess_exec(
                *cmd,
                cwd=self.root,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            await process.communicate()
        except (OSError, asyncio.SubprocessError):
            # Return empty result on error
            return MutationResult(
                execution_time_seconds=time.time() - start_time,
                tested_files=[p for p in mutate_paths],
            )
        finally:
            # Restore original setup.cfg
            if setup_cfg_backup is not None:
                setup_cfg.write_text(setup_cfg_backup)
            elif setup_cfg.exists():
                setup_cfg.unlink()
            # Clean up mutmut artifacts
            import shutil

            mutants_dir = self.root / "mutants"
            if mutants_dir.exists():
                shutil.rmtree(mutants_dir, ignore_errors=True)

        # Parse results
        result = self._parse_results()
        result.execution_time_seconds = time.time() - start_time
        result.tested_files = list(mutate_paths)

        # In quick mode, limit survivors reported
        if quick_check and len(result.survivors) > 10:
            result.survivors = result.survivors[:10]

        return result

    def _auto_detect_paths(self) -> list[Path]:
        """Auto-detect source paths to mutate."""
        candidates = [
            self.root / "src",
            self.root / "lib",
            self.root / self.root.name,  # Package with same name as project
        ]
        paths = []
        for candidate in candidates:
            if candidate.is_dir():
                paths.append(candidate)
        return paths if paths else [self.root]

    def _filter_changed_since(self, paths: list[Path], since: str) -> list[Path]:
        """Filter to only files changed since a git commit."""
        try:
            result = subprocess.run(
                ["git", "diff", "--name-only", since, "HEAD"],
                cwd=self.root,
                capture_output=True,
                text=True,
                timeout=30,
            )
            if result.returncode != 0:
                return paths

            changed_files = set(result.stdout.strip().split("\n"))
            filtered = []
            for path in paths:
                if path.is_file():
                    rel = path.relative_to(self.root)
                    if str(rel) in changed_files:
                        filtered.append(path)
                elif path.is_dir():
                    # Check if any file in the dir changed
                    for changed in changed_files:
                        if changed.startswith(str(path.relative_to(self.root))):
                            filtered.append(path)
                            break
            return filtered if filtered else paths
        except (OSError, subprocess.SubprocessError, ValueError):
            return paths

    def _parse_results(self) -> MutationResult:
        """Parse mutmut results from its cache."""
        result = MutationResult()

        # Try to get results from mutmut results command
        try:
            proc = subprocess.run(
                ["mutmut", "results"],
                cwd=self.root,
                capture_output=True,
                text=True,
                timeout=60,
            )
            if proc.returncode == 0:
                result = self._parse_results_output(proc.stdout)
        except (OSError, subprocess.SubprocessError):
            pass

        # Try to get detailed survivor info
        try:
            proc = subprocess.run(
                ["mutmut", "show", "survived"],
                cwd=self.root,
                capture_output=True,
                text=True,
                timeout=60,
            )
            if proc.returncode == 0:
                result.survivors = self._parse_survivors(proc.stdout)
        except (OSError, subprocess.SubprocessError):
            pass

        return result

    def _parse_results_output(self, output: str) -> MutationResult:
        """Parse the mutmut results summary output."""
        result = MutationResult()

        # Parse lines like:
        # Killed: 45
        # Survived: 3
        # etc.
        for line in output.strip().split("\n"):
            line = line.strip()
            if ":" in line:
                key, value = line.split(":", 1)
                key = key.strip().lower()
                try:
                    count = int(value.strip())
                    if key == "killed":
                        result.killed = count
                    elif key == "survived":
                        result.survived = count
                    elif key == "timeout":
                        result.timeout = count
                    elif key == "suspicious":
                        result.suspicious = count
                    elif key == "skipped":
                        result.skipped = count
                except ValueError:
                    pass

        result.total_mutants = (
            result.killed + result.survived + result.timeout + result.suspicious + result.skipped
        )
        return result

    def _parse_survivors(self, output: str) -> list[SurvivingMutant]:
        """Parse the mutmut show survived output."""
        survivors = []

        # mutmut show output format varies, try to extract what we can
        # Typically shows diffs or mutation descriptions
        current_file: Path | None = None
        current_line: int = 0
        original: str = ""

        for line in output.strip().split("\n"):
            # Look for file paths
            if line.startswith("--- ") or "Mutant" in line:
                # Try to extract file info
                for part in line.split():
                    if part.endswith(".py"):
                        try:
                            current_file = Path(part.lstrip("a/").lstrip("b/"))
                        except (ValueError, TypeError):
                            pass
            # Look for line numbers
            if line.startswith("@@"):
                # Diff hunk header: @@ -10,5 +10,5 @@
                try:
                    parts = line.split()
                    for part in parts:
                        if part.startswith("+") and "," in part:
                            current_line = int(part[1:].split(",")[0])
                except (ValueError, IndexError):
                    pass
            # Look for actual mutations (- old / + new lines)
            if line.startswith("-") and not line.startswith("---"):
                original = line[1:].strip()
                # The next + line should be the mutation
                continue
            if line.startswith("+") and not line.startswith("+++"):
                mutated = line[1:].strip()
                if current_file:
                    mutation_type = self._classify_mutation(original, mutated)
                    survivors.append(
                        SurvivingMutant(
                            file=current_file,
                            line=current_line,
                            original=original if original else "?",
                            mutated=mutated,
                            mutation_type=mutation_type,
                        )
                    )
                    original = ""  # Reset for next mutation

        return survivors

    def _classify_mutation(self, original: str, mutated: str) -> str:
        """Classify what type of mutation occurred."""
        # Simple heuristics
        true_to_false = "True" in original and "False" in mutated
        false_to_true = "False" in original and "True" in mutated
        if true_to_false or false_to_true:
            return "boolean"
        if any(op in original for op in ["==", "!=", "<", ">", "<=", ">="]):
            return "comparison"
        if any(op in original for op in ["+", "-", "*", "/", "%"]):
            return "arithmetic"
        if "return" in original or "return" in mutated:
            return "return"
        if "not " in original or "not " in mutated:
            return "negation"
        if "and" in original or "or" in original:
            return "logical"
        return "other"


def create_mutation_analyzer(
    root: Path | None = None,
    **kwargs: Any,
) -> MutationAnalyzer:
    """Factory function to create a MutationAnalyzer.

    Args:
        root: Project root (default: current directory)
        **kwargs: Additional arguments passed to MutationAnalyzer

    Returns:
        Configured MutationAnalyzer instance
    """
    if root is None:
        root = Path.cwd()
    return MutationAnalyzer(root, **kwargs)
