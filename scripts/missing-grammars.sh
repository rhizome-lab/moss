#!/usr/bin/env bash
# Find arborium grammars not yet implemented in moss-languages
# Usage: ./scripts/missing-grammars.sh

set -euo pipefail

# Get all lang-* features from arborium via crates.io API
arborium_langs=$(curl -sL "https://crates.io/api/v1/crates/arborium/2.4.5" | \
    jq -r '.version.features | keys[]' | \
    grep '^lang-' | cut -d- -f2- | sort -u)

# Get implemented languages from moss-languages Cargo.toml (exclude comments)
moss_langs=$(grep -v '^#' crates/moss-languages/Cargo.toml | \
    grep -oE 'lang-[a-z0-9-]+' | cut -d- -f2- | sort -u)

arborium_count=$(echo "$arborium_langs" | wc -l)
moss_count=$(echo "$moss_langs" | wc -l)

echo "=== moss-languages: $moss_count implemented ==="
echo "$moss_langs" | tr '\n' ' '
echo -e "\n"

echo "=== arborium: $arborium_count available ==="
missing=$(comm -23 <(echo "$arborium_langs") <(echo "$moss_langs"))

if [[ -z "$missing" ]]; then
    echo "=== All grammars implemented! ==="
else
    missing_count=$(echo "$missing" | wc -l)
    echo "=== Missing: $missing_count ==="
    echo "$missing"
fi
