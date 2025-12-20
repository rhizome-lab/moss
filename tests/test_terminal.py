"""Tests for terminal module - persistent shell sessions."""

from __future__ import annotations

import pytest

from moss.terminal import (
    PersistentShell,
    ShellConfig,
    ShellResult,
    TerminalSubagent,
)


class TestShellResult:
    """Tests for ShellResult dataclass."""

    def test_success_property(self):
        result = ShellResult(
            command="echo hello",
            stdout="hello",
            stderr="",
            returncode=0,
            cwd="/tmp",
        )
        assert result.success is True

    def test_failure_property(self):
        result = ShellResult(
            command="false",
            stdout="",
            stderr="",
            returncode=1,
            cwd="/tmp",
        )
        assert result.success is False

    def test_to_dict(self):
        result = ShellResult(
            command="pwd",
            stdout="/home/user",
            stderr="",
            returncode=0,
            cwd="/home/user",
            duration_ms=50,
        )
        d = result.to_dict()
        assert d["command"] == "pwd"
        assert d["returncode"] == 0
        assert d["success"] is True

    def test_to_compact(self):
        result = ShellResult(
            command="echo test",
            stdout="test",
            stderr="",
            returncode=0,
            cwd="/tmp",
        )
        compact = result.to_compact()
        assert "✓" in compact
        assert "echo test" in compact
        assert "test" in compact


class TestPersistentShell:
    """Tests for PersistentShell class."""

    @pytest.mark.asyncio
    async def test_basic_command(self):
        """Test running a simple command."""
        async with PersistentShell() as shell:
            result = await shell.run("echo hello")
            assert result.success
            assert "hello" in result.stdout

    @pytest.mark.asyncio
    async def test_persistent_cwd(self):
        """Test that working directory persists between commands."""
        async with PersistentShell() as shell:
            await shell.run("cd /tmp")
            result = await shell.run("pwd")
            assert result.success
            # The shell.cwd should be updated
            assert "/tmp" in shell.cwd

    @pytest.mark.asyncio
    async def test_persistent_env(self):
        """Test that environment variables persist."""
        async with PersistentShell() as shell:
            await shell.run("export MY_TEST_VAR=hello123")
            result = await shell.run("echo $MY_TEST_VAR")
            assert result.success
            assert "hello123" in result.stdout

    @pytest.mark.asyncio
    async def test_command_failure(self):
        """Test handling of failed commands."""
        async with PersistentShell() as shell:
            # Use a command that fails but doesn't exit the shell
            result = await shell.run("false")
            # This should fail but not crash
            assert result.returncode != 0 or not result.success

    @pytest.mark.asyncio
    async def test_history(self):
        """Test command history tracking."""
        async with PersistentShell() as shell:
            await shell.run("echo one")
            await shell.run("echo two")
            await shell.run("echo three")

            history = shell.history
            assert len(history) == 3
            assert "one" in history[0].command
            assert "two" in history[1].command
            assert "three" in history[2].command

    @pytest.mark.asyncio
    async def test_timeout(self):
        """Test command timeout."""
        config = ShellConfig(timeout_seconds=0.5)
        async with PersistentShell(config) as shell:
            result = await shell.run("sleep 10", timeout=0.3)
            # Should timeout and return error
            assert not result.success or "timeout" in result.stderr.lower()

    @pytest.mark.asyncio
    async def test_multiline_output(self):
        """Test handling multi-line output."""
        async with PersistentShell() as shell:
            result = await shell.run("echo -e 'line1\\nline2\\nline3'")
            assert result.success
            lines = result.stdout.strip().split("\n")
            assert len(lines) >= 3

    @pytest.mark.asyncio
    async def test_stderr(self):
        """Test stderr capture."""
        async with PersistentShell() as shell:
            result = await shell.run("echo error >&2")
            # stderr should contain the error message
            assert "error" in result.stderr or "error" in result.stdout

    @pytest.mark.asyncio
    async def test_is_running(self):
        """Test is_running property."""
        shell = PersistentShell()
        assert not shell.is_running

        await shell.start()
        assert shell.is_running

        await shell.close()
        assert not shell.is_running


class TestTerminalSubagent:
    """Tests for TerminalSubagent class."""

    @pytest.mark.asyncio
    async def test_execute(self):
        """Test basic command execution."""
        async with TerminalSubagent() as agent:
            result = await agent.execute("echo hello")
            assert result.success
            assert "hello" in result.stdout

    @pytest.mark.asyncio
    async def test_execute_check(self):
        """Test check mode raises on failure."""
        async with TerminalSubagent() as agent:
            with pytest.raises(RuntimeError):
                # Use 'false' which fails but doesn't exit the shell
                await agent.execute("false", check=True)

    @pytest.mark.asyncio
    async def test_cd_and_pwd(self):
        """Test directory navigation."""
        async with TerminalSubagent() as agent:
            await agent.cd("/tmp")
            pwd = await agent.pwd()
            assert "/tmp" in pwd

    @pytest.mark.asyncio
    async def test_ls(self):
        """Test directory listing."""
        async with TerminalSubagent() as agent:
            files = await agent.ls("/")
            assert len(files) > 0
            # Root should have common directories
            assert any(d in files for d in ["bin", "usr", "etc", "tmp", "home"])

    @pytest.mark.asyncio
    async def test_cat(self):
        """Test file reading."""
        async with TerminalSubagent() as agent:
            content = await agent.cat("/etc/hostname")
            # Should return something (hostname)
            assert content is not None

    @pytest.mark.asyncio
    async def test_run_script(self):
        """Test multi-line script execution."""
        script = """
        # This is a comment
        echo line1
        echo line2
        echo line3
        """
        async with TerminalSubagent() as agent:
            results = await agent.run_script(script)
            assert len(results) == 3
            assert all(r.success for r in results)

    @pytest.mark.asyncio
    async def test_run_script_check(self):
        """Test script execution with check mode."""
        script = """
        echo before
        false
        echo after
        """
        async with TerminalSubagent() as agent:
            results = await agent.run_script(script, check=True)
            # Should stop at the failing command (false)
            assert len(results) == 2
            assert results[0].success
            assert not results[1].success
