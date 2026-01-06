#!/usr/bin/env bash
# Regenerate all CLI --help fixtures.
# Run from within nix-shell for all dependencies.

set -euo pipefail

cd "$(dirname "$0")"

echo "=== Generating clap fixtures ==="
(cd clap && cargo build --release 2>/dev/null)
./clap/target/release/example --help > clap/example.help
./clap/target/release/example build --help > clap/example-build.help
./clap/target/release/example run --help > clap/example-run.help
echo "  clap/example.help"
echo "  clap/example-build.help"
echo "  clap/example-run.help"

echo "=== Generating argparse fixtures ==="
python argparse/example.py --help > argparse/example.help
echo "  argparse/example.help"

echo "=== Generating click fixtures ==="
python click/example.py --help > click/example.help
echo "  click/example.help"

echo "=== Generating commander fixtures ==="
(cd commander && npm install --silent 2>/dev/null)
node commander/example.js --help > commander/example.help
echo "  commander/example.help"

echo "=== Generating yargs fixtures ==="
(cd yargs && npm install --silent 2>/dev/null)
node yargs/example.js --help > yargs/example.help
echo "  yargs/example.help"

echo "=== Generating cobra fixtures ==="
(cd cobra && go build -o example 2>/dev/null)
./cobra/example --help > cobra/example.help
echo "  cobra/example.help"

echo ""
echo "Done! All fixtures regenerated."
