# M1 record-stream acceptance fixture

This directory defines the revised M1 streaming workload and equivalent C,
Bun, and tsn programs. The tsn implementation contains the validation and hash
algorithm in ordinary source; it uses only the documented `tsn:io` input/output
primitives at the host boundary. This fixture makes no pass claim until all M1
acceptance checks, including leak instrumentation, have run.

## Exact workload and algorithm

Input is exactly **100,000,000 bytes** (100 MB decimal): 100,000 records of
exactly 1,000 bytes each. Every record starts with its payload length encoded
as an unsigned 4-byte little-endian integer (`996`), followed by 996 binary
payload bytes. For record index `r` and payload byte index `i` (both zero
based), the payload byte is `(r * 17 + i * 31 + 0x5a) & 0xff`.

Each program reads the stream as bytes, validates every prefix and payload
byte, and rejects truncated data, invalid records, or trailing bytes. It
computes 32-bit FNV-1a over payload bytes only, in record/byte order: initialize
`hash = 0x811c9dc5`; for each byte `b`, set
`hash = ((hash XOR b) * 0x01000193) modulo 2^32`. Output is three decimal lines:
record count, byte count, checksum. The C, Bun, and tsn programs implement this
same contract. Bun consumes `Bun.stdin.stream()` and uses a `DataView` with the
little-endian flag; C explicitly reconstructs the 4-byte prefix, so neither
reference decodes the input as text.

## Generate and check

Requirements: Python 3 and a C compiler. Bun is optional as an informational
baseline; the tsn compiler is needed to compare tsn. From the workspace root:

```sh
cc -O3 -DNDEBUG benchmarks/m1-record-stream/record-stream.c -o benchmarks/m1-record-stream/record-stream-c
cargo run -p tsn -- compile benchmarks/m1-record-stream/record-stream.tsn.ts -o target/m1-record-stream
python3 benchmarks/m1-record-stream/run.py --c ./benchmarks/m1-record-stream/record-stream-c --bun "bun run benchmarks/m1-record-stream/record-stream.bun.ts" --tsn ./target/m1-record-stream
```

If Bun is unavailable, omit `--bun`; C-vs-tsn correctness can still be checked.

`run.py` deterministically creates `input.bin` beside itself (ignored source
artifact; exactly 100,000,000 bytes), pipes the binary file to each supplied
command, and compares their complete output lines. To use a different input
location, pass `--input PATH`. The generated file is not checked in; delete it
after use. No timing or memory benchmark is run by this script.

## Timing/RSS protocol (for a future acceptance run)

Use one Linux/amd64 host and identical input bytes, CPU affinity, and machine
load for all implementations. Build the C reference with `cc -O3 -DNDEBUG`;
record compiler/version/flags, Bun version, CPU model, and OS. Generate input
once before measurements. Run each implementation with the same input redirected
to stdin; verify its output against the exact line above before accepting any
sample. Do not regenerate input in a timed process.

For each implementation, collect 10 independent process runs, rotating runner
order between rounds. Record wall-clock elapsed time and peak RSS for each run
(on Linux, `/usr/bin/time -f '%e %M' command < input.bin`, where `%M` is peak
RSS in KiB); exclude compile and input-generation time. Report raw samples,
median elapsed time, and maximum observed peak RSS. Use the same measurement
tool and conditions throughout. These instructions specify a protocol only;
no samples, performance results, or pass/fail conclusion are included here.
