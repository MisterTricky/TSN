# tsn

An experimental TS-shaped ahead-of-time language for Linux/amd64 servers.

**Status:** early compiler, not v0 complete. The Rust `tsn` executable uses
oxc, type-checks a small explicit subset (fixed-width numbers, UTF-8 literal
views, control flow, and checked `Uint8Array` buffers), and emits standalone
Linux/amd64 ELFs via a C intermediate and Clang/LLVM. Relative named imports
of exported functions and selected `tsn:*` built-ins work; npm and other ESM
forms remain unimplemented. `build` remains a
separate hello-literal bootstrap path. Rust is the temporary compiler
implementation language; self-hosting is a later goal.

## Build and try it (Ubuntu/WSL)

```sh
CARGO_TARGET_DIR=/tmp/tsn-cargo-target cargo run -p tsn -- build examples/hello.ts -o target/tsn-hello
CARGO_TARGET_DIR=/tmp/tsn-cargo-target cargo run -p tsn -- check benchmarks/sieve/sieve.ts
CARGO_TARGET_DIR=/tmp/tsn-cargo-target cargo run -p tsn -- compile benchmarks/byte-kernel/kernel.ts -o target/tsn-bench
./target/tsn-bench
./target/tsn-hello
```

In WSL, keep Cargo's Linux build cache separate from Windows Cargo's `target/`
directory. For smoke tests, set `TSN_BIN=/tmp/tsn-cargo-target/debug/tsn`.

To remove only this workspace's Rust build artifacts: `./scripts/clean.ps1`
on PowerShell, or `CARGO_TARGET_DIR=/tmp/tsn-cargo-target cargo clean --manifest-path Cargo.toml`
in WSL/Linux. These do not remove source files or `Cargo.lock`.

The build command invokes `clang -O2 -static` to link the emitted LLVM IR.
Linux builds must be produced and run inside Linux; Windows and Linux binaries
are not interchangeable.

## Current benchmark

`benchmarks/byte-kernel` contains matching Bun TypeScript, optimized C, and
standalone tsn runners. Bun is only the comparison baseline; the tsn executable
does not depend on Bun or a C runner. All runs must use the same OS and
machine; see
`benchmarks/byte-kernel/RESULTS.md`.

`benchmarks/sieve` now exercises normal typed-buffer allocation, indexed
mutation/reads, bounds checks, branches, and cleanup. It compiles and produces
the same checksum as Bun and C; its preliminary timing is in
`benchmarks/sieve/RESULTS.md`.

On Linux/WSL, `CARGO_TARGET_DIR=/tmp/tsn-cargo-target cargo run -p tsn -- version`
builds the Linux compiler separately from Windows artifacts. Then run
`TSN_BIN=/tmp/tsn-cargo-target/debug/tsn bash tests/compiler-smoke-linux.sh` to
compile/run the sieve, local-module, string, and signed-number examples and
check type diagnostics and bounds-trap cleanup.
The Linux Bun installation was already present at `~/.bun/bin/bun` in WSL.

## Design notes

- `ABI.md` — C ABI draft
- `TYPES.md` — type-system draft
- `GRAMMAR.md` — parser policy draft
- `PLAN.md` — milestone plan
- `IMPLEMENTATION_STATUS.md` — implemented subset and next correctness gates

## Standard library direction

The target is a comprehensive, batteries-included standard library, using Go's
stdlib as the benchmark—particularly for server-side development. Prefer
cohesive, well-documented built-in packages for common needs over requiring
third-party dependencies for basic server functionality. This is a product
goal, not a claim that the library already exists.
