#!/usr/bin/env python3
"""Example click CLI for testing help output parsing."""

import click


@click.group()
@click.option("-v", "--verbose", is_flag=True, help="Enable verbose output")
@click.option("-c", "--config", type=click.Path(), metavar="FILE", help="Config file path")
@click.option("-p", "--port", default=8080, help="Port number")
@click.version_option(version="1.0.0")
def cli(verbose, config, port):
    """An example CLI tool for testing."""
    pass


@cli.command()
@click.option("-r", "--release", is_flag=True, help="Build in release mode")
@click.option("-t", "--target", metavar="DIR", help="Target directory")
def build(release, target):
    """Build the project."""
    click.echo("Building...")


@cli.command()
@click.argument("args", nargs=-1)
def run(args):
    """Run the project."""
    click.echo(f"Running with args: {args}")


@cli.command()
def clean():
    """Clean build artifacts."""
    click.echo("Cleaning...")


if __name__ == "__main__":
    cli()
