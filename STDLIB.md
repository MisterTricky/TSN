# tsn Standard Library — Inventory and Roadmap

## Purpose and status

tsn's long-term standard-library goal is Go-inspired: a cohesive, batteries-included set of packages for Linux server and systems programming, so routine application work does not require third-party packages. This is a product direction, **not a claim that these packages or APIs are implemented**. The compiler is experimental, and its current accepted subset is recorded in `IMPLEMENTATION_STATUS.md`; `README.md`, `PLAN.md`, and `TYPES.md` remain authoritative for project scope and language semantics.

This document distinguishes:

- **Implemented now** — only the narrow compiler/runtime surface explicitly listed below.
- **Proposed v0** — a staged target, contingent on language, runtime, and milestone acceptance criteria. Names and signatures are design sketches, not promises of support.
- **Deferred** — explicitly outside the current path or v0 scope unless the plan is revised.

## Source-implemented stdlib requirement

The standard library is to be implemented in tsn source, not accumulated as
compiler-recognized built-ins. A package counts as implemented only when its
public algorithms, parsing, formatting, validation, and protocol logic are
ordinary `.ts` modules compiled by tsn and covered by executable tests. The
compiler/runtime may expose a small, documented platform layer (allocation,
clock, process I/O, sockets, entropy, and thread/task primitives) where crossing
the OS boundary requires it; those primitives must not hide package-level
behavior such as HTTP parsing, JSON logic, or JWT verification.

The current compiler is not yet expressive enough to implement the proposed
stdlib. Before claiming the source-stdlib gate, it must support at least the
types, control flow, string/byte construction, ownership/scopes, errors, module
exports, and test/build workflow needed by the actual library sources. Compiler
intrinsics added to bypass a missing source-language capability do not satisfy
this requirement.

All proposed imports use the existing `tsn:<package>` built-in import convention. Current local-module support is limited to relative named imports of exported functions. Do not infer support for npm, arbitrary ESM forms, dynamic imports, namespace/default imports, or package discovery from the sketches here.

## Implemented now

The current compiler has initial named imports from:

| Import | Implemented API | Limits |
|---|---|---|
| `tsn:time` | `monotonicMs()` | Monotonic elapsed-time value; not wall-clock time, calendars, timers, or a complete time package. |
| `tsn:io` | `printString(value: string): void`, `printU32(value: u32): void`, `printI32(value: i32): void`, `printF64(value: f64): void`, `readStdin(buffer: Uint8Array): u32` | Basic scalar output and bounded input into caller-owned buffers; `readStdin` returns bytes read, `0` at EOF, `u32::MAX` on I/O error. Not general reader/writer abstractions or file/network I/O. |
| `tsn:net` | `listenTcp(port: u16, loopback: bool): i32`, `acceptTcp(listener: i32): i32`, `readTcp(fd: i32, buffer: Uint8Array, offset: u32): u32`, `writeTcp(fd: i32, buffer: Uint8Array, offset: u32): u32`, `writeTcpString(fd: i32, value: string, offset: u32): u32`, `closeTcp(fd: i32): i32` | Minimal blocking Linux socket boundary only; `loopback` selects loopback vs all interfaces; descriptors are explicit integers, `-1` signals listen/accept/close errors, `u32::MAX` signals read/write errors, `0` signals EOF/no progress. No typed errors, concurrency, DNS, or HTTP implementation. The HTTP/protocol logic must live in tsn source. |
| `tsn:os` | `shutdownRequested(): bool`, `spawnThread(worker: (i32) => void, argument: i32): i32` | SIGINT/SIGTERM request flag and bounded (128 active threads) worker-spawn primitive. This is a host execution primitive, not a task runtime; no process/environment/file APIs or cancellation tokens yet. |
| `tsn:strings` | `sliceUtf8(value: string, start: u32, end: u32): string`, `stringByteAt(value: string, index: u32): u8` | Bounds-checked byte primitives only; higher-level `contains` search is implemented in `stdlib/strings.ts`. Indices are UTF-8 bytes, not Unicode scalar positions. |
| `tsn:bench` | `printBenchmark(...)` (temporary) | Benchmark-only intrinsic; not a general application API. See migration below. |

Source-implemented pilots now include `stdlib/bytes.ts` (FNV-1a and byte
equality), `stdlib/strings.ts::contains`, `stdlib/base64.ts::decodeBase64Url`,
and `stdlib/crypto.ts::sha256`/`hmacSha256`. Their loops and algorithms are
ordinary tsn code; narrow `sliceUtf8`/`stringByteAt`, fixed-width arrays, and
integer casts supply representation operations. SHA-256 and HMAC are checked
against standard test vectors, but crypto is not yet security-audited and no
production constant-time claim is made. `stdlib/http.ts` currently validates
only the empty-GET smoke request, not the HTTP/1.1 specification. These are
bootstrap proofs, not complete packages or package-name imports; most proposed
library behavior is still absent.

`Uint8Array` currently supports zero-initialized allocation, checked indexing, `.length`, and compiler-generated cleanup within the supported ownership rules. Borrowed buffer parameters are supported for synchronous calls, but buffers cannot be returned, retained, or rebound. Strings are UTF-8 literal/views with byte-based `.length`, content equality, concatenation, and output through `printString`; concatenation allocations are currently retained until standalone exit/trap and are not request-scoped. Numeric output is provided by the explicitly typed `printU32`, `printI32`, and `printF64`. General formatting, parsing, and string utilities are not implemented. See `IMPLEMENTATION_STATUS.md` for the full live inventory.

## Proposed v0 package map

These cohesive package names and API shapes are a proposal. The subset of each API that can ship depends on compiler support and the ordered gates below. Prefer explicit, typed operations and `Result<T, E>` where failures are expected; do not imply Go's exact API or semantics.

| Package | Proposed responsibilities / example shape | Status |
|---|---|---|
| `tsn:io` | Basic output plus `Reader`/`Writer`-like interfaces or equivalent typed operations; typed scalar output and bounded stdin read are implemented. | Narrow output/input APIs implemented; general API proposed. |
| `tsn:bytes` | Mutable byte sequences and utilities over `Uint8Array`; e.g. `equal(a, b)`, `indexOf(buf, needle)`, `Buffer`/builder only after ownership and buffer rules permit it. | Proposed; not a promise of a buffer object or aliasing. |
| `tsn:strings` | UTF-8 operations such as `contains`, `indexOf`, `split`/iteration, `join`, and a builder for allocation-conscious construction. | `contains` source pilot implemented; remainder proposed. |
| `tsn:fmt` | Explicit formatting and conversion, e.g. `format(template, ...values)` only if supported, or typed `write*`/append functions; avoid implicit coercion. | Proposed; compiler tracker identifies replacing benchmark hooks with ordinary `tsn:fmt`/`tsn:io`. |
| `tsn:time` | `monotonicMs` seed; eventually `Instant`-like elapsed-time measurement and wall-clock `now`, duration operations, and timer/sleep APIs as runtime scheduling allows. | One API implemented; remainder proposed. |
| `tsn:os` | Environment, process arguments, exit status, and file operations with typed errors, subject to the Linux-only v0 target and security/API decisions. | Proposed; not currently exposed. |
| `tsn:net` | Blocking-first TCP listener/connection and address operations; e.g. `listen`, `accept`, `read`, `write`, `close`, with explicit errors and timeouts only when supported. | Low-level blocking primitives implemented; no DNS, typed errors, pool, or complete network package. |
| `tsn:json` | Typed JSON encode/decode for supported data shapes, with explicit parse/validation errors. | Proposed; M3 requires minimal JSON encode/decode, not a general reflection system. |
| `tsn:http` | HTTP/1.1 client/server primitives and a minimal router over `net`; v0 scope is deliberately bounded. | Only a source-level empty-GET smoke route is implemented; general request parsing, `Content-Length` bodies, chunked responses, router, and soak criteria remain open. |
| `tsn:log` | Structured/contextual levels and a configurable sink built on `io`; avoid global behavior until concurrency and lifecycle semantics are defined. | Proposed; not currently exposed. |
| `tsn:crypto` | Carefully selected standard algorithms and secure-random facilities through vetted implementations; API and dependency policy must be explicit. | Proposed direction only. Not current support; M5's JWT workload is not evidence of a crypto package. |

Package boundaries should stay small and composable: `http` builds on `net`, `json` on core values/strings/bytes, and `log` on `io`/`fmt`. Avoid duplicate overlapping APIs (for example, byte manipulation in both `bytes` and `strings`) and do not expose raw pointers as a shortcut to implementation.

## Dependency-ordered acceptance criteria

The following sequence is a roadmap, not a promise that all items fit the fixed M0–M5 schedule. `PLAN.md` remains the milestone authority; its M3 scope is minimal server proof, while TLS, HTTP/2, and epoll are explicitly out of scope before M5 or deferred. Each stage must first have compiler/runtime support for the types, errors, allocation, and cleanup it needs.

1. **Core and migration — `io`, `fmt`, `bytes`, `strings`, `time`.** Close source typechecking and buffer-correctness gaps first. Define testable, typed APIs over supported `string`/`Uint8Array`; verify output, edge cases, bounds, cleanup, and differential behavior. Keep `monotonicMs` semantics explicit. Replace `tsn:bench` / `printBenchmark` use in ordinary examples with normal `tsn:fmt` and `tsn:io` calls as those APIs become usable. Benchmarks may retain harness-specific measurement outside the application stdlib; no application should need the benchmark intrinsic to print results. Remove/deprecate the intrinsic only after benchmark sources and smoke tests have migrated and the compiler no longer needs it.
2. **Host basics — `os`.** Specify Linux behavior and typed failure cases before adding APIs. Acceptance: tests cover normal and error paths, resource closure, and supported target behavior; no pointer surface is introduced.
3. **Transport — `net`.** Prerequisite: reliable buffer/reader-writer contracts, error propagation, and runtime cleanup. Acceptance: blocking TCP smoke tests for loopback listen/accept/read/write/close and failure paths; concurrency/load behavior is measured. This does not imply async/await, DNS completeness, or epoll.
4. **Data encoding — `json`.** Prerequisite: supported value shapes, string/byte APIs, and typed error model. Acceptance: encode/decode round trips and malformed-input tests for the promised subset, including cross-checks against a reference implementation. Do not claim arbitrary runtime reflection or all TypeScript object shapes.
5. **Protocol — `http`.** Prerequisites: `net`, `io`, `bytes`/`strings`, `json` where used, plus request-scope cleanup. Acceptance is limited to the M3 HTTP/1.1 features in `PLAN.md`, with correctness, keep-alive, bounded parsing, and soak tests; TLS and HTTP/2 remain excluded unless separately planned.
6. **Operations — `log`.** Prerequisite: stable formatting, I/O sink behavior, and defined concurrency/lifecycle rules. Acceptance: deterministic formatting tests, configurable sink tests, and no hidden process-global assumptions in documented behavior.
7. **Security primitives — `crypto`.** Select a narrow, auditable surface and vetted implementation strategy before specifying APIs. Acceptance: known-answer vectors, malformed-input tests, and security review. A server milestone or JWT example must not be treated as package acceptance.

For each package, acceptance also requires documentation of supported types, allocation/ownership behavior, errors, target limitations, and unsupported operations. A parsed import is not implemented support; tests must compile and run through the tsn compiler on the stated target.

## Explicitly deferred / not implied by this inventory

- **Pointers, raw pointers, and unsafe operations** are deferred by `TYPES.md`. No package API in this document authorizes pointer syntax or arbitrary address access. Settle the memory/lifetime model before revisiting.
- Full TypeScript/ESM/npm compatibility, dynamic imports, package-manager behavior, and arbitrary third-party dependencies are not implied.
- Async runtime completeness, epoll, broad DNS, TLS, HTTP/2, and non-Linux targets are not implied. Follow `PLAN.md`'s milestone and post-v0 boundaries.
- General reflection-based serialization, unconstrained object shapes, and unbounded API compatibility with Go are not promised.
- A comprehensive stdlib is the long-term bar; v0 should ship a small, documented, tested subset rather than package names with unavailable behavior.
