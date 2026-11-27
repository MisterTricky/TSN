# Byte-kernel benchmark

This is an early **compiler-thesis benchmark**. The C file establishes the
optimized native reference and the Bun harness establishes the Bun baseline.
The tsn program declares its own `main()` and compiles to a standalone native
executable. Bun is only the separate baseline; it is never a tsn dependency.

## Workload

Each program runs the same integer kernel five times, with bounds from
16,777,216 through 16,777,220, and must produce the same checksum.
The workload avoids IO, allocation, and network effects so it can reveal
code-generation quality. It is intentionally scalar: vectorization is not
currently part of the claim being tested.

## Run on Linux/amd64

Requirements: Bun and Clang on the same Linux environment.

```sh
clang -O3 -march=native -DNDEBUG kernel.c -o kernel-c
bun run kernel.ts
./kernel-c
```

Build and run the standalone tsn program:

```sh
cargo run -p tsn -- compile benchmarks/byte-kernel/kernel.ts -o target/tsn-bench
./target/tsn-bench
bash benchmarks/byte-kernel/run-linux.sh
```

Do not compare Windows Bun with a Linux binary. If Bun is not installed in the
Linux environment, install it there first or defer measurements; cross-OS
timings are not a valid comparison.

The script rotates runner order, warms Bun in-process, and takes 10 samples.
M1's pass/fail performance comparison is tsn versus equivalent C: target a tsn
median elapsed time no slower than the C median plus one sample standard
deviation, and no more than 1.2x C peak RSS on the planned 100 MB record-stream
workload. Bun remains an informational baseline. Pin all runners
to the same CPU set and record CPU model, Bun version, Clang version, compiler
flags, raw output, throughput, and RSS.

`kernel.ts` uses fixed-width arithmetic via `Math.imul` and bitwise operations
to make its integer behavior explicit. Its tsn-authored `main()` calls the M1
benchmark builtins `clockMs()` and `printBenchmark()`. `tsn compile` parses the
TypeScript AST, lowers the supported subset to a typed C intermediate, invokes
Clang/LLVM to emit IR, then links a static ELF. The C reference uses the same
optimization/codegen pass split to keep the backend comparison fair. The executable runs
independently of Bun and any C runner.
