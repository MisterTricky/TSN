#!/usr/bin/env bash
set -euo pipefail

export PATH="/snap/bin:$PATH"
root="$(cd "$(dirname "$0")/../.." && pwd)"
bench="$root/benchmarks/m1-record-stream"
target="$root/target"
linux_target="$target/linux-cargo"
cd "$root"
mkdir -p "$target"
bun=""
if command -v bun >/dev/null 2>&1; then
  bun="bun"
elif [[ -x "$target/bun/bin/bun" ]]; then
  bun="target/bun/bin/bun"
fi

CARGO_TARGET_DIR="$linux_target" cargo build --manifest-path "$root/Cargo.toml" -p tsn

clang -O3 -march=native -fwrapv -Wall -Wextra -Werror -Wconversion \
  -Werror=conversion -Werror=sign-conversion -S -emit-llvm \
  "$bench/record-stream.c" -o "$target/record-stream.ll"
clang -O0 -march=native -fwrapv -static "$target/record-stream.ll" \
  -o "$target/record-stream-c"
"$linux_target/debug/tsn" compile "$bench/record-stream.tsn.ts" \
  -o "$target/m1-record-stream"

if [[ -n "$bun" ]]; then
  python3 "$bench/run.py" --c target/record-stream-c \
    --tsn target/m1-record-stream \
    --bun "$bun run benchmarks/m1-record-stream/record-stream.bun.ts"
else
  python3 "$bench/run.py" --c target/record-stream-c \
    --tsn target/m1-record-stream
fi

cp "$bench/input.bin" /tmp/m1-record-stream.bin
if [[ -n "$bun" ]]; then
  python3 "$bench/measure.py" --c target/record-stream-c \
    --tsn target/m1-record-stream --bun "$bun run benchmarks/m1-record-stream/record-stream.bun.ts" \
    --input /tmp/m1-record-stream.bin --runs 10 --raw-out "$bench/RESULTS-RAW.json"
else
  python3 "$bench/measure.py" --c target/record-stream-c \
    --tsn target/m1-record-stream --input /tmp/m1-record-stream.bin \
    --runs 10 --raw-out "$bench/RESULTS-RAW.json"
fi

TSN_BIN="$linux_target/debug/tsn" bash "$root/tests/m1-buffer-cleanup-linux.sh"
