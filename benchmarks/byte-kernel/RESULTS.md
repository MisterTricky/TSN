# Preliminary standalone byte-kernel results

All three standalone programs produced checksum `1564648077`. The workload is
five kernel calls with slightly different input bounds (to prevent dead-code
elimination), timed as one batch.

## Pinned WSL2 run

| Runner | Median time per 5-call batch (10 samples) |
|---|---:|
| tsn standalone ELF | 83.23 ms |
| C standalone reference | 83.92 ms |
| Bun TypeScript | 104.47 ms |

In this run, tsn was about **1.26× faster than Bun** and approximately at parity
with C. This is preliminary and workload-specific. It does **not** establish
the revised M1 gate: the planned 100 MB record-stream workload, RSS comparison,
and 10-million-iteration leak test have not been run.

## Environment and method

- WSL2 Ubuntu, Linux x86_64; AMD Ryzen 7 7735HS
- Bun 1.4.5; Ubuntu Clang 21.1.8
- Both C reference and tsn C intermediate used Clang `-O3 -march=native -fwrapv`
  to emit LLVM IR, then Clang `-O0 -march=native -fwrapv -static` for native
  codegen/link.
  This gives both the same single LLVM optimization pass.
- Runs were pinned to CPU 2 and interleaved in rotating order to reduce
  scheduling/frequency bias.
- Bun gets five discarded processes; each measured process warms the kernel
  three times before timing its five-call batch.

The tsn program has a source-defined `main`, is linked as a static ELF, and runs
without Bun or a C harness. The compiler currently uses an AST-based C
intermediate and exposes temporary benchmark builtins `clockMs()` and
`printBenchmark()`; replacing these with ordinary stdlib imports and a direct
LLVM backend remains future work.
