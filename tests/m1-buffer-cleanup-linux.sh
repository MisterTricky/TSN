#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
compiler="${TSN_BIN:-$root/target/debug/tsn}"
out="$root/target/m1-leak"
mkdir -p "$out"

"$compiler" compile "$root/benchmarks/m1-record-stream/buffer-cleanup-10m.ts" -o "$out/buffer-cleanup"
result=$("$out/buffer-cleanup")
if [[ "$result" != "15000000" ]]; then
  echo "unexpected 10M-loop checksum: $result" >&2
  exit 1
fi

if command -v valgrind >/dev/null 2>&1; then
  valgrind --quiet --leak-check=full --error-exitcode=99 "$out/buffer-cleanup" >/dev/null
elif command -v clang >/dev/null 2>&1; then
  clang -O1 -g -fsanitize=address -fno-omit-frame-pointer \
    "$out/buffer-cleanup.c" -o "$out/buffer-cleanup-asan"
  ASAN_OPTIONS=detect_leaks=1:exitcode=99 "$out/buffer-cleanup-asan" >/dev/null
else
  rss_kib=$({ /usr/bin/time -f '%M' "$out/buffer-cleanup" >/dev/null; } 2>&1)
  if (( rss_kib > 16384 )); then
    echo "10M cleanup RSS exceeded fallback bound: ${rss_kib} KiB" >&2
    exit 1
  fi
  echo "Valgrind unavailable; 10M cleanup passed the 16MiB RSS-growth fallback (${rss_kib} KiB). This is not a substitute for a leak analyzer." >&2
fi
