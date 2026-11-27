# Sieve benchmark (next workload)

This checks mutable typed buffers, indexing, bounds traps, branches, and
repeated allocation/free using the same byte-per-candidate algorithm in tsn,
Bun/TypeScript, and C. The timed batch includes five allocations, sieves, and
frees for limits 1,000,000 through 1,000,004. All programs sum the five prime
counts and must report the same checksum.

The tsn source is ordinary source code with a declared `main`; the standalone
binary must not require Bun or a C runner. Compile/run after the compiler slice
supports the program:

```sh
cargo run -p tsn -- compile benchmarks/sieve/sieve.ts -o target/tsn-sieve
bash benchmarks/sieve/run-linux.sh
```

Do not replace the sieve with a benchmark-specific source translator. Keep the
byte-per-candidate storage representation identical across all three
implementations; a packed-bitset variant can be a separate follow-up.
