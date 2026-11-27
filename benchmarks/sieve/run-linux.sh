#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
tsn_bin="$root/target/tsn-sieve"
c_bin="$root/benchmarks/sieve/sieve-c"
bun_bin="${BUN_BIN:-$HOME/.bun/bin/bun}"
bun_file="$root/benchmarks/sieve/bun-run.ts"
cpu_set="${TSN_CPU_SET:-2}"

for executable in "$tsn_bin" "$bun_bin"; do
  if [[ ! -x "$executable" ]]; then echo "missing executable: $executable" >&2; exit 2; fi
done
clang -O3 -march=native -fwrapv -DNDEBUG -S -emit-llvm \
  "$root/benchmarks/sieve/sieve.c" -o /tmp/tsn-sieve-c-reference.ll
clang -O0 -march=native -fwrapv -static /tmp/tsn-sieve-c-reference.ll -o "$c_bin"

echo "Pinning every runner to CPU set $cpu_set"
echo "Warm up Bun (5 discarded processes; each sample warms internally)"
for _ in {1..5}; do taskset -c "$cpu_set" "$bun_bin" run "$bun_file" >/dev/null; done

run_one() {
  case "$1" in
    tsn) taskset -c "$cpu_set" "$tsn_bin" ;;
    bun) taskset -c "$cpu_set" "$bun_bin" run "$bun_file" ;;
    c) taskset -c "$cpu_set" "$c_bin" ;;
  esac
}

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
