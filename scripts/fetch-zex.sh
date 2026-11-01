#!/usr/bin/env bash
# Fetch the ZEXDOC/ZEXALL Z80 exercisers (GPL, by Frank Cringle) for the CPU tests.
set -euo pipefail
dir="$(dirname "$0")/../crates/u880/tests/zex"
mkdir -p "$dir"
base="https://raw.githubusercontent.com/cdio/caio/master/src/test/zilog_z80/zex"
for f in zexdoc.com zexall.com; do
  curl -fsSL "$base/$f" -o "$dir/$f"
done
sha1sum "$dir"/*.com
