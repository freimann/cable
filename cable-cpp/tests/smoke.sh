#!/bin/sh
# Smoke test for the C++ port.  Builds two small deterministic files, runs the
# non-interactive benchmark and checks the offsets it reports.

set -eu

bin=${1:-./cable}

tmp=$(mktemp -d "${TMPDIR:-/tmp}/cable-smoke.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

# 64 KiB of 'A', then the search marker near the end.
dd if=/dev/zero of="$tmp/zero.bin" bs=65536 count=1 2>/dev/null
tr '\0' 'A' < "$tmp/zero.bin" > "$tmp/a.bin"
printf 'CABLE-NEEDLE-1234' >> "$tmp/a.bin"

# The second file differs at a known offset near the end.
cp "$tmp/a.bin" "$tmp/b.bin"
printf 'B' | dd of="$tmp/b.bin" bs=1 seek=65000 conv=notrunc 2>/dev/null

"$bin" --version | grep -q '^cable 0\.2\.4'
"$bin" --help | grep -q -- '--bench'
"$bin" --bench "$tmp/a.bin" "$tmp/b.bin" > "$tmp/out.csv"

[ "$(wc -l < "$tmp/out.csv" | tr -d ' ')" -eq 6 ]
grep -Eq '^open,[0-9]+\.[0-9]{3},65553,65553$' "$tmp/out.csv"
grep -Eq '^scan,[0-9]+\.[0-9]{3},65553,[0-9]+$' "$tmp/out.csv"
grep -Eq '^search,[0-9]+\.[0-9]{3},65553,65536$' "$tmp/out.csv"
grep -Eq '^compare,[0-9]+\.[0-9]{3},65553,65000$' "$tmp/out.csv"
grep -Eq '^save,[0-9]+\.[0-9]{3},65553,65553$' "$tmp/out.csv"

# The save phase rewrites a temporary copy and must not touch the inputs.
[ "$(wc -c < "$tmp/a.bin" | tr -d ' ')" -eq 65553 ]

echo "smoke: ok"
