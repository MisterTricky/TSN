#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
compiler="${TSN_BIN:-$root/target/linux-cargo/debug/tsn}"
out="$root/target/m3-soak"
duration="${M3_SOAK_SECONDS:-600}"
workers="${M3_SOAK_WORKERS:-4}"
mkdir -p "$out"

"$compiler" compile "$root/examples/http-keepalive.ts" -o "$out/http-keepalive"
python3 "$root/tests/http-load-m3-linux.py" \
  --server "$out/http-keepalive" \
  --duration-seconds "$duration" \
  --workers "$workers"
