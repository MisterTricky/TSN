#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
tsn_bin="$root/target/tsn-bench"
c_bin="$root/benchmarks/byte-kernel/kernel-c"
bun_bin="${BUN_BIN:-$HOME/.bun/bin/bun}"
ts_file="$root/benchmarks/byte-kernel/bun-run.ts"
cpu_set="${TSN_CPU_SET:-2}"

for path in "$tsn_bin" "$c_bin" "$bun_bin"; do
  if [[ ! -x "$path" ]]; then echo "missing executable: $path" >&2; exit 2; fi
done
if [[ ! -f "$ts_file" ]]; then echo "missing Bun harness: $ts_file" >&2; exit 2; fi

clang -O3 -march=native -fwrapv -DNDEBUG -S -emit-llvm \
  "$root/benchmarks/byte-kernel/kernel.c" -o /tmp/tsn-kernel-c-reference.ll
clang -O0 -march=native -fwrapv -static /tmp/tsn-kernel-c-reference.ll -o "$c_bin"

echo "Pinning every runner to CPU set $cpu_set"
echo "Warm up Bun (5 discarded processes; each measured process also warms its kernel)"
for _ in {1..5}; do taskset -c "$cpu_set" "$bun_bin" run "$ts_file" >/dev/null; done

run_one() {
  case "$1" in
    tsn) taskset -c "$cpu_set" "$tsn_bin" ;;
    bun) taskset -c "$cpu_set" "$bun_bin" run "$ts_file" ;;
    c) taskset -c "$cpu_set" "$c_bin" ;;
  esac
}

# Rotate order to reduce systematic CPU-frequency/thermal bias.
for sample in {1..10}; do
  case $((sample % 3)) in
    0) order=(tsn bun c) ;;
    1) order=(bun c tsn) ;;
    2) order=(c tsn bun) ;;
  esac
  for name in "${order[@]}"; do
    printf '%s sample %02d: ' "$name" "$sample"
    run_one "$name"
  done
done
