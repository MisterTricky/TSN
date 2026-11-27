#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
compiler="${TSN_BIN:-$root/target/debug/tsn}"
out="$root/target/compiler-smoke"
mkdir -p "$out"

"$compiler" compile "$root/benchmarks/sieve/sieve.ts" -o "$out/sieve"
"$compiler" check "$root/benchmarks/sieve/sieve.ts"
result="$("$out/sieve")"
[[ "$result" == *'"checksum":392492'* ]] || {
  echo "unexpected sieve result: $result" >&2
  exit 1
}

"$compiler" compile "$root/examples/string.ts" -o "$out/string"
string_output="$("$out/string")"
if [[ "$string_output" != "héllo, tsn" ]]; then
  echo "unexpected string output: $string_output" >&2
  exit 1
fi

"$compiler" compile "$root/examples/string-concat.ts" -o "$out/string-concat"
concat_output=$("$out/string-concat")
if [[ "$concat_output" != "héllo, tsn" ]]; then
  echo "unexpected string concatenation output: $concat_output" >&2
  exit 1
fi

"$compiler" compile "$root/examples/stdin.ts" -o "$out/stdin"
printf 'ABCD' | "$out/stdin"

"$compiler" compile "$root/examples/tcp-echo-once.ts" -o "$out/tcp-echo-once"
python3 "$root/tests/tcp-smoke-linux.py" "$out/tcp-echo-once"

"$compiler" compile "$root/examples/thread-smoke.ts" -o "$out/thread-smoke"
thread_output=$("$out/thread-smoke")
if [[ "$thread_output" != "7" ]]; then
  echo "unexpected thread smoke output: $thread_output" >&2
  exit 1
fi

"$compiler" compile "$root/examples/http-once.ts" -o "$out/http-once"
python3 "$root/tests/http-smoke-linux.py" "$out/http-once"

"$compiler" compile "$root/examples/http-parser.ts" -o "$out/http-parser"
"$out/http-parser"

"$compiler" compile "$root/examples/http-keepalive.ts" -o "$out/http-keepalive"
python3 "$root/tests/http-load-linux.py" "$out/http-keepalive"

"$compiler" compile "$root/examples/stdlib.ts" -o "$out/stdlib"
"$compiler" check "$root/examples/stdlib.ts"
stdlib_output=$("$out/stdlib")
if [[ "$stdlib_output" != "tsn stdlib imports are linked" ]]; then
  echo "unexpected stdlib output: $stdlib_output" >&2
  exit 1
fi

"$compiler" compile "$root/examples/print-u32.ts" -o "$out/print-u32"
print_u32_output=$("$out/print-u32")
if [[ "$print_u32_output" != "42" ]]; then
  echo "unexpected printU32 output: $print_u32_output" >&2
  exit 1
fi

"$compiler" compile "$root/examples/print-numbers.ts" -o "$out/print-numbers"
print_numbers_output=$("$out/print-numbers")
if [[ "$print_numbers_output" != $'-7\n3.5' ]]; then
  echo "unexpected numeric io output: $print_numbers_output" >&2
  exit 1
fi

"$compiler" compile "$root/examples/division.ts" -o "$out/division"
division_output=$("$out/division")
if [[ "$division_output" != $'3.5\n-3.5' ]]; then
  echo "unexpected division output: $division_output" >&2
  exit 1
fi

"$compiler" compile "$root/examples/stdlib-bytes.ts" -o "$out/stdlib-bytes"
stdlib_bytes_output=$("$out/stdlib-bytes")
if [[ "$stdlib_bytes_output" != "3289118412" ]]; then
  echo "unexpected source-stdlib output: $stdlib_bytes_output" >&2
  exit 1
fi

"$compiler" compile "$root/examples/byte-cast.ts" -o "$out/byte-cast"
"$out/byte-cast"

"$compiler" compile "$root/examples/word-array.ts" -o "$out/word-array"
"$out/word-array"

"$compiler" compile "$root/examples/sha256.ts" -o "$out/sha256"
"$out/sha256"

"$compiler" compile "$root/examples/sha256-multiblock.ts" -o "$out/sha256-multiblock"
"$out/sha256-multiblock"

"$compiler" compile "$root/examples/hmac-sha256.ts" -o "$out/hmac-sha256"
"$out/hmac-sha256"

"$compiler" compile "$root/examples/base64url.ts" -o "$out/base64url"
"$out/base64url"

"$compiler" compile "$root/examples/jwt-verify.ts" -o "$out/jwt-verify"
"$out/jwt-verify"

"$compiler" compile "$root/benchmarks/service-m5/service.tsn.ts" -o "$out/service-m5"
m5_output=$("$out/service-m5")
if [[ "$m5_output" != $'44540000\n'* ]]; then
  echo "unexpected M5 source-service checksum: $m5_output" >&2
  exit 1
fi

"$compiler" compile "$root/examples/json-body.ts" -o "$out/json-body"
"$out/json-body"

"$compiler" compile "$root/examples/stdlib-strings.ts" -o "$out/stdlib-strings"
"$out/stdlib-strings"

"$compiler" compile "$root/examples/numeric-widening.ts" -o "$out/widening"
"$out/widening"

"$compiler" compile "$root/examples/signed-min.ts" -o "$out/signed-min"
"$out/signed-min"

"$compiler" compile "$root/examples/signed-wrap.ts" -o "$out/signed-wrap"
"$out/signed-wrap"

"$compiler" compile "$root/examples/loop-cleanup.ts" -o "$out/loop-cleanup"
"$out/loop-cleanup"

"$compiler" compile "$root/examples/modules/main.ts" -o "$out/modules"
module_output=$("$out/modules")
if [[ "$module_output" != *'"checksum":42'* ]]; then
  echo "unexpected local-module output: $module_output" >&2
  exit 1
fi

set +e
module_diagnostic=$("$compiler" check "$root/examples/modules/bad-main.ts" 2>&1)
status=$?
set -e
if [[ $status -eq 0 ]] || [[ "$module_diagnostic" != *"error[TSN2001]"* ]] || [[ "$module_diagnostic" != *"bad-math.ts:2:"* ]]; then
  echo "expected module-local type diagnostic; got:" >&2
  echo "$module_diagnostic" >&2
  exit 1
fi

"$compiler" compile "$root/tests/fixtures/array-out-of-bounds.ts" -o "$out/oob"
set +e
trap_output=$( (ulimit -c 0; "$out/oob") 2>&1)
status=$?
set -e
if [[ $status -eq 0 ]] || [[ "$trap_output" != *"error[TSN3002]: Uint8Array index out of bounds"* ]]; then
  echo "out-of-bounds access did not produce the expected runtime trap:" >&2
  echo "$trap_output" >&2
  exit 1
fi

set +e
diagnostic=$("$compiler" check "$root/tests/fixtures/type-mismatch.ts" 2>&1)
status=$?
set -e
if [[ $status -eq 0 ]] || [[ "$diagnostic" != *"error[TSN2001]"* ]] || [[ "$diagnostic" != *"type-mismatch.ts:2:"* ]]; then
  echo "expected spanned TSN2001 type diagnostic; got:" >&2
  echo "$diagnostic" >&2
  exit 1
fi

set +e
diagnostic=$("$compiler" check "$root/tests/fixtures/loose-equality.ts" 2>&1)
status=$?
set -e
if [[ $status -eq 0 ]] || [[ "$diagnostic" != *"error[TSN1301]"* ]] || [[ "$diagnostic" != *'use `===` or `!==`'* ]]; then
  echo "expected spanned TSN1301 loose-equality diagnostic; got:" >&2
  echo "$diagnostic" >&2
  exit 1
fi

echo "compiler smoke tests passed (sieve, recovered bounds trap with cleanup, spanned type/grammar errors)"
