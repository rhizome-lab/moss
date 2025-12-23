"""CLI-based LLM provider.

Shells out to command-line tools like `llm`, `claude`, or `gemini`.
Zero dependencies - always available as a fallback.
"""

from __future__ import annotations

import shutil
import subprocess
from dataclasses import dataclass, field

from moss_orchestration.llm.protocol import LLMResponse, Message, Role


@dataclass
class CLIProvider:
    """LLM provider that shells out to CLI tools.

    Supports various CLI tools:
    - llm: Simon Willison's llm CLI (default)
    - claude: Anthropic's Claude CLI
    - gemini: Google's Gemini CLI

    Example:
        provider = CLIProvider(cmd="llm", model="gpt-4o")
        response = provider.complete("Hello!")

        # Or with Claude CLI
        provider = CLIProvider(cmd="claude", model="claude-sonnet-4-20250514")
    """

    cmd: str = "llm"
    model: str | None = None
    timeout: int = 300
    extra_args: list[str] = field(default_factory=list)

    def complete(
        self,
        prompt: str,
        *,
        system: str | None = None,
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a prompt using the CLI tool.

        Args:
            prompt: The user prompt
            system: Optional system prompt
            **kwargs: Additional options (ignored for CLI)

        Returns:
            LLMResponse with the completion
        """
        args = self._build_args(prompt, system=system)
        result = subprocess.run(
            args,
            capture_output=True,
            text=True,
            timeout=self.timeout,
            check=False,
        )

        if result.returncode != 0:
            raise RuntimeError(f"CLI command failed: {result.stderr}")

        return LLMResponse(
            content=result.stdout.strip(),
            model=self.model,
        )

    def chat(
        self,
        messages: list[Message],
        **kwargs: object,
    ) -> LLMResponse:
        """Complete a multi-turn conversation.

        For CLI providers, this concatenates messages into a single prompt.
        """
        # Extract system message if present
        system = None
        user_messages = []
        for msg in messages:
            if msg.role == Role.SYSTEM:
                system = msg.content
            else:
                user_messages.append(msg)

        # Build conversation prompt
        parts = []
        for msg in user_messages:
            prefix = "User: " if msg.role == Role.USER else "Assistant: "
            parts.append(f"{prefix}{msg.content}")

        prompt = "\n\n".join(parts)
        return self.complete(prompt, system=system, **kwargs)

    def _build_args(self, prompt: str, *, system: str | None = None) -> list[str]:
        """Build CLI arguments based on the command type."""
        if self.cmd == "claude" or self.cmd.endswith("/claude"):
            return self._build_claude_args(prompt, system=system)
        elif self.cmd == "gemini" or self.cmd.endswith("/gemini"):
            return self._build_gemini_args(prompt, system=system)
        else:
            # Default to llm CLI syntax
            return self._build_llm_args(prompt, system=system)

    def _build_llm_args(self, prompt: str, *, system: str | None = None) -> list[str]:
        """Build args for Simon Willison's llm CLI."""
        args = [self.cmd]
        if self.model:
            args.extend(["-m", self.model])
        if system:
            args.extend(["-s", system])
        args.extend(self.extra_args)
        args.append(prompt)
        return args

    def _build_claude_args(self, prompt: str, *, system: str | None = None) -> list[str]:
        """Build args for Claude CLI."""
        args = [self.cmd]
        if self.model:
            args.extend(["--model", self.model])
        if system:
            args.extend(["--system", system])
        args.extend(self.extra_args)
        args.extend(["--print", prompt])
        return args

    def _build_gemini_args(self, prompt: str, *, system: str | None = None) -> list[str]:
        """Build args for Gemini CLI."""
        args = [self.cmd]
        if self.model:
            args.extend(["--model", self.model])
        args.extend(self.extra_args)
        # Gemini CLI doesn't have a standard system prompt flag
        # Prepend system to prompt if provided
        if system:
            prompt = f"{system}\n\n{prompt}"
        args.append(prompt)
        return args

    @classmethod
    def is_available(cls) -> bool:
        """CLI provider is always available."""
        # Check if at least one CLI tool is available
        for cmd in ["llm", "claude", "gemini"]:
            if shutil.which(cmd):
                return True
        # Even if no CLI found, still "available" - will fail at runtime
        return True
