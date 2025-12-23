"""Evaluation harnesses for benchmarking moss.

This module provides evaluation infrastructure for measuring moss's
performance on standard code intelligence benchmarks.

Available harnesses:
- SWE-bench: Real-world GitHub issue resolution

Usage:
    # List available instances
    moss eval swebench --list

    # Run on specific instance
    moss eval swebench --instance sympy__sympy-20590

    # Run on Lite subset
    moss eval swebench --subset lite --limit 10
"""

from moss_orchestration.eval.swebench import (
    SWEBenchHarness,
    SWEBenchInstance,
    SWEBenchResult,
)

__all__ = [
    "SWEBenchHarness",
    "SWEBenchInstance",
    "SWEBenchResult",
]
