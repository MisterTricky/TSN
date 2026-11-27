# Preliminary Sieve results

All implementations produced checksum `392492`, the sum of prime counts for
limits 1,000,000 through 1,000,004. Each timed batch includes five zeroed byte
buffer allocations, sieve passes, and frees.

## Pinned WSL2 run

| Runner | Median time per 5-sieve batch (10 samples) |
|---|---:|
| tsn standalone ELF | 12.06 ms |
| Bun TypeScript | 11.05 ms |
| C standalone reference | 9.13 ms |

This first result is a correctness milestone, not a performance win: the tsn
version is slower than both references. Its checked indexed access currently
calls bounds-checking helpers for reads and writes; the compiler does not yet
prove/eliminate checks. Any bounds-check optimization must preserve the
language's safety behavior and be justified by IR/benchmark evidence.

## Environment and method

- WSL2 Ubuntu, Linux x86_64; AMD Ryzen 7 7735HS
- Bun 1.4.5; Ubuntu Clang 21.1.8
- Rotating runner order, 10 samples; Bun warmed in-process
- All runners pinned to CPU 2 with `taskset`
- C and tsn used the same Clang/LLVM `-O3 -fwrapv` IR-optimization and `-O0`
  native-codegen split; tsn emits a static ELF from its own `main()`.
- Byte-per-candidate storage, allocation/setup/free included in the timed batch.

Treat these results as preliminary. The source-level feature works, but the
compiler still lacks complete expression typing, span-based diagnostics,
reusable type-safe buffer APIs, and bounds-check elimination.
