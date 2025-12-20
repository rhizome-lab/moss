#!/usr/bin/env bash
# Benchmark suite for moss CLI commands
# Usage: ./bench.sh [--quick]
#
# Runs each command multiple times and reports average time.
# Use --quick for single run (faster but less accurate).

set -e

RUNS=5
if [[ "$1" == "--quick" ]]; then
    RUNS=1
fi

# Build release first
echo "Building release binary..."
cargo build --release -p moss-cli 2>/dev/null

MOSS="./target/release/moss"

# Ensure index exists
echo "Ensuring index is up to date..."
$MOSS reindex >/dev/null 2>&1

# Function to benchmark a command (uses built-in TIMEFORMAT)
bench() {
    local name="$1"
    shift
    local total=0

    for ((i=1; i<=RUNS; i++)); do
        # Use bash's time and parse real time
        local output
        output=$( { time $MOSS "$@" >/dev/null 2>&1; } 2>&1 )
        # Extract real time (format: "real 0m0.003s")
        local real_time=$(echo "$output" | grep real | sed 's/real[[:space:]]*//' | sed 's/m/\*60+/' | sed 's/s//')
        # Convert to milliseconds using awk
        local ms=$(echo "$real_time" | awk -F'+' '{print int(($1 + $2) * 1000)}')
        total=$((total + ms))
    done

    local avg=$((total / RUNS))
    printf "%-25s %6d ms\n" "$name" "$avg"
}

echo ""
echo "=== Moss CLI Benchmark ==="
echo "Runs per command: $RUNS"
echo ""

# File resolution benchmarks
echo "--- File Resolution ---"
bench "path (exact)" path cli.py
bench "path (fuzzy)" path "dwim"
bench "path (deep)" path "moss_api"

# Symbol extraction benchmarks
echo ""
echo "--- Symbol Extraction ---"
bench "symbols" symbols src/moss/cli.py
bench "skeleton" skeleton src/moss/cli.py
bench "expand" expand main

# Call graph benchmarks
echo ""
echo "--- Call Graph ---"
bench "callers (indexed)" callers serialize
bench "callees" callees serialize --file src/moss/gen/serialize.py

# Analysis benchmarks
echo ""
echo "--- Analysis ---"
bench "complexity" complexity src/moss/cli.py
bench "deps" deps src/moss/cli.py
bench "anchors" anchors src/moss/cli.py

# Tree benchmarks
echo ""
echo "--- Directory Tree ---"
bench "tree (shallow)" tree --depth 2
bench "tree (full)" tree

# Index benchmarks
echo ""
echo "--- Index Operations ---"
bench "search-tree" search-tree cli

# Health/summary
echo ""
echo "--- Overview ---"
bench "summarize" summarize src/moss/cli.py
bench "health" health

echo ""
echo "Done."
