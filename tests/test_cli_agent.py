import asyncio
from argparse import Namespace
from unittest.mock import AsyncMock, MagicMock, patch

from moss.cli import cmd_agent


def test_cmd_agent_dry_run_vanilla():
    """Test agent --dry-run --vanilla command."""
    args = Namespace(
        task="Test task",
        model=None,
        max_turns=50,
        verbose=False,
        dry_run=True,
        vanilla=True,
        root=".",
        directory=".",
        quiet=False,
        debug=False,
        json=False,
        compact=False,
        no_color=False,
        jq=None,
    )

    with patch("moss.cli.setup_output") as mock_setup:
        mock_output = MagicMock()
        mock_setup.return_value = mock_output

        result = cmd_agent(args)

        assert result == 0
        # Verify it mentioned Vanilla in output
        found_vanilla = False
        for call in mock_output.info.call_args_list:
            if "Vanilla" in call[0][0]:
                found_vanilla = True
                break
        assert found_vanilla


def test_cmd_agent_run_vanilla():
    """Test agent --vanilla actual run (mocked)."""
    args = Namespace(
        task="Test task",
        model="test-model",
        max_turns=5,
        verbose=True,
        dry_run=False,
        vanilla=True,
        root=".",
        directory=".",
        quiet=False,
        debug=False,
        json=False,
        compact=False,
        no_color=False,
        jq=None,
    )

    with patch("moss.cli.setup_output") as mock_setup:
        mock_output = MagicMock()
        mock_setup.return_value = mock_output

        with patch("moss.vanilla_loop.VanillaAgentLoop.run", new_callable=AsyncMock) as mock_run:
            from moss.vanilla_loop import VanillaLoopResult, VanillaLoopState

            mock_run.return_value = VanillaLoopResult(
                state=VanillaLoopState.DONE, turns=[], final_output="Success", total_duration_ms=100
            )

            # cmd_agent uses asyncio.run
            # In a non-async test, we can just let it run or mock asyncio.run
            with patch(
                "asyncio.run",
                side_effect=lambda coro: asyncio.new_event_loop().run_until_complete(coro),
            ):
                result = cmd_agent(args)

            assert result == 0
            mock_run.assert_called_once_with("Test task")
            mock_output.success.assert_called()
