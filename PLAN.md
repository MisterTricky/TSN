# tsn — Build Plan v0.1 (M0 → M5)

> Companion to `ABI.md` and `TYPES.md`.
> Scope: linux/amd64, standalone-first, npm as a leaf.
> Status: **draft.** Timelines assume a focused team of 3–4.

---

## 1. Purpose

This document turns the design into an ordered set of milestones, each with a
**measurable acceptance criterion** and a **kill gate** where the project can be
stopped cheaply.

The ordering is chosen to front-load risk:

1. Does the toolchain produce a real binary? (M0)
2. Is the codegen actually fast? (M1)
3. Does the npm boundary eat the win? (M2)
4. Can we run a server? (M3–M4)
5. Does the whole thing beat Bun on a realistic service? (M5)

If any gate fails, stop and re-plan rather than pushing forward.

---

## 2. Assumptions

**Standard-library product goal:** emulate Go's comprehensive, batteries-included
stdlib for server development. Common server capabilities should be cohesive,
well-documented built-in packages rather than mandatory third-party packages.
This is a long-term product bar; M0–M5 build only the minimum needed to prove
the compiler and performance thesis.

**Systems-programming goal:** tsn is intended to support native server/system
software, not only application scripting: fixed-width numerics, explicit memory
control, predictable layout, native C ABI/FFI, threads/atomics, and a runtime
that can be built and shipped as a native Linux binary. Raw pointer operations
and lower-level OS/embedded use need a deliberate safety model and are not part
of the current M0/M1 benchmark slice.

**Pointer support is deferred.** Do not block compiler or server-library work on
pointer syntax, raw pointers, or unsafe operations. When revisited, the desired
safe-pointer experience is Go-like, but it depends on resolving the tracing-GC
versus arena/refcount lifetime-model question first.

| Item | Decision |
|---|---|
| Compiler language | **Rust** |
| Runtime language | **Rust** (exported via `extern "C"`) |
| Parser | **oxc** (reuse; do not write one) |
| IR | LLVM IR via Clang for this bootstrap slice; direct LLVM API is a later backend improvement |
| LLVM | current stable, pinned in CI |
| Target | `x86_64-unknown-linux-musl`, static |
| Adapters | `napi-rs` for N-API; Node for the sidecar |
| Bootstrap | Rust only. Self-hosting is a post-v0 fantasy. |
| Team | 3–4 engineers |
| Timeline | ~14–16 weeks to M5 |

Why Rust for the compiler: oxc is Rust, LLVM bindings are mature, and the
language's own memory model (no GC, explicit ownership) maps cleanly onto
implementing one. Why Rust for the runtime: memory safety in the allocator and
event loop is worth the friction, and `extern "C"` exports are trivial.

---

## 3. Repository layout

```
tsn/
├── Cargo.toml                    # workspace
├── rust-toolchain.toml           # pinned
├── include/
│   └── tsn.h                     # the ABI header (single source of truth)
├── crates/
│   ├── tsn-driver/               # CLI: build, check, emit, bench
│   ├── tsn-frontend/             # oxc wrapper → AST → HIR
│   ├── tsn-types/                # checker, inference, soundness rules
│   ├── tsn-hir/                  # typed HIR (pre-lowering)
│   ├── tsn-mir/                  # SSA IR, escape analysis, lowering
│   ├── tsn-codegen/              # LLVM IR emission
│   ├── tsn-abi/                  # header/manifest generation, layout
│   └── tsn-runtime/              # cdylib + staticlib
├── runtime/
│   ├── core/                     # arena, refcount, strings, arrays, maps
│   ├── async/                    # tasks, timers, IO reactor
│   ├── net/                      # tcp, dns
│   └── platform/linux/           # epoll (v0.2)
├── adapters/
│   ├── napi/                     # Rust, napi-rs
│   └── sidecar/                  # TypeScript, Node
├── tests/
│   ├── conformance/              # C harness against tsn.h
│   ├── golden/                   # IR snapshots (insta)
│   ├── differential/             # same program under Bun and tsn
│   └── benches/                  # hyperfine + custom harnesses
└── docs/
    ├── ABI.md
    ├── TYPES.md
    └── PLAN.md
```

**Rule:** `include/tsn.h` is hand-written and authoritative. The `tsn-abi` crate
*reads* it to generate layout assertions; it never regenerates it. One source of
truth, checked by tests.

---

## 4. Milestone ladder

| # | Milestone | Weeks | Gate |
|---|---|---|---|
| M0 | Toolchain proof | 1 | static binary runs |
| M1 | Codegen quality | 2–3 | **standalone tsn program; within one C-run standard deviation on the byte-kernel workload** |
| M2 | npm boundary | 2 | **boundary < 10% of call** |
| M3 | Async + HTTP | 3–4 | serves 1k req/s |
| M4 | Scopes, recovery, typed errors | 2–3 | panic ≠ process death |
| M5 | Realistic service benchmark | 2 | **≥2× on a real API** |

Total: ~14–16 weeks. A team of 2 should roughly double it.

---

## 5. M0 — Toolchain proof (1 week)

**Goal:** prove the whole pipeline exists, end to end, with the least possible
language surface.

**Scope**

- Parse a `.ts` file with oxc into an AST.
- Support exactly: `function`, `const`, numeric literals, `+`, `return`, and a
  single `print(str)` builtin.
- Emit LLVM IR for `main()` that calls `write(2)`.
- Link statically against musl.
- CLI: `tsn build hello.ts -o hello`.

**Day-by-day**

| Day | Task |
|---|---|
| 1 | Workspace, oxc parse, AST dump |
| 2 | Minimal HIR + type check (numbers only) |
| 3 | LLVM IR emission via inkwell; `main` + `write` |
| 4 | musl static link; `tsn build` wired up |
| 5 | Golden IR test, CI, `hello` binary |

**Acceptance**

- `./hello` prints and exits 0.
- `file hello` → `ELF 64-bit LSB executable, statically linked`.
- `ldd hello` → `not a dynamic executable`.
- `tsn build` is reproducible: two builds produce byte-identical output.
- Golden IR snapshot test in CI.

**Explicitly out of scope:** types beyond `number`, strings as values, control
flow, structs, error messages beyond "parse failed."

---

## 6. M1 — Codegen quality (2–3 weeks) · GATE 1

**Goal:** answer *"is the generated code actually fast?"* before building
anything else.

**Language surface needed**

- `i32`/`i64`/`u32`/`u64`/`f64` and `number`
- `string` (UTF-8), `Uint8Array`, indexing, slicing
- `interface` (fixed layout), arrays, `for`/`while`, `if`
- functions, simple closures (no captures across scopes yet)
- integer arithmetic with defined wrap/trap
- `print`, `readStdin`, `writeStdout` builtins

**Workload:** a byte-oriented kernel, not a web app.

```
read a large buffer from stdin
  → parse a simple length-prefixed record stream
  → compute a rolling hash + aggregate stats
  → write results to stdout
```

Write the *same* program in TypeScript and run it under Bun.

**Why a byte kernel:** it isolates codegen quality from language-surface risk.
No JSON, no networking, no allocation-heavy patterns. If the codegen is good,
this shows it cleanly.

**Acceptance**

- The benchmark is a tsn-authored program with its own `main`; compilation emits
  a standalone linux/amd64 executable that runs without Bun or a C test runner.
- Correct output on 100 MB of input.
- On the same machine and 100 MB input, tsn median elapsed time must be no slower than the C median plus one sample standard deviation of the C runs, with equivalent timed work and optimization settings.
- Peak RSS ≤ 1.2× equivalent C. Report Bun throughput/RSS as an informational comparison, not a pass/fail threshold.
- No leaks under a 10M-iteration loop (Valgrind, heaptrack, or AddressSanitizer/LeakSanitizer).

**If it fails:** the thesis is in trouble. Options in order: check whether the
kernel is memory-bound (re-measure with a compute-bound variant), inspect the
IR against Clang's output for the equivalent C, and only then consider whether
LLVM is being driven correctly. Do **not** proceed to M2 hoping it improves.

**Benchmark rule:** a compiled object called by a C harness is useful for
isolated kernel debugging, but it does not satisfy the M1 comparison gate. The
gate uses a tsn-authored `main` and the minimum tsn runtime facilities required
for timing/output. Bun remains a separate baseline process, never a dependency
of the tsn executable.

### Follow-up benchmark: Sieve of Eratosthenes

After the standalone integer-kernel benchmark, the next workload is a sieve
that counts primes up to a fixed upper bound. It should exercise typed mutable
buffers, indexed reads/writes, loops, branches, integer arithmetic, and bounds
check elimination—not just scalar arithmetic.

**Fairness requirements:**

- Implement the same sieve and storage representation in tsn, Bun/TypeScript,
  and C (start with one byte per candidate; test a packed bitset separately).
- Use the same upper bound, return/print the same prime count checksum, and
  include allocation/setup consistently in the timed region.
- Run all three standalone programs on the same Linux/amd64 WSL environment;
  warm Bun in-process and rotate runner order.
- Report throughput and peak RSS. Do not fold sieve results into the byte
  kernel's headline number.

The initial typed-buffer/control-flow slice now compiles this source without
recognizing sieve-specific syntax and matches the Bun/C checksum. It is still
preliminary: complete expression typing, cleanup on all exits, recoverable
bounds traps, and bounds-check optimization remain before this can be a v0
correctness/performance gate. See `benchmarks/sieve/RESULTS.md`.

---

## 7. M2 — npm boundary (2 weeks) · GATE 2

**Goal:** answer *"does marshalling eat the win?"*

**Scope**

- `tsn build --target=napi` emitting a `.node` addon.
- `tsn_host.resolve` wired to `napi-rs`.
- Host imports limited to `(bytes) -> bytes` and `(str) -> str`.
- Generated `Error` subclasses from the embedded manifest (minimal version).
- Adapter reads `tsn_manifest_json()` at init.

**Workload:** a realistic leaf module.

```ts
// validate.ts — compiled to native
export async function processBatch(payload: Buffer): Promise<Buffer> {
  // parse, validate, transform, hash, re-serialize
}
```

Called from Bun and from Node with 1 KB, 64 KB, and 1 MB payloads.

**Acceptance**

- Correctness: byte-identical output vs the pure-TS implementation.
- **Boundary overhead < 10%** of total call time at 64 KB.
- **≥2× end-to-end** vs the pure-TS version at 64 KB.
- No leaks across 1M calls.
- Addon loads in both Bun and Node without warnings.

**If it fails:** the leaf model is wrong for this workload. Re-scope to
coarser calls (whole-request instead of per-item), or accept that the win only
exists for larger payloads and document the threshold.

---

## 8. M3 — Async + HTTP (3–4 weeks)

**Goal:** serve requests.

**Scope**

- `async`/`await` lowering to state machines (or, if time is short, blocking
  calls on a thread pool — see the note below).
- Tasks: `spawn`, `on_done`, `poll`, `take_result`.
- Blocking TCP + a thread-per-connection server.
- HTTP/1.1: request line, headers, `Content-Length`, keep-alive, chunked
  *response* only.
- A minimal router and JSON encode/decode.

**Decision: blocking first, epoll later.** Thread-per-connection with a bounded
pool is dramatically simpler and fine for the demo. epoll lands in v0.2. This
is a deliberate, documented limitation, not an oversight.

**Acceptance**

- Serves 1,000 req/s on a trivial JSON endpoint (single core).
- Survives `ab`/`wrk` for 10 minutes without leaks or crashes.
- p99 latency within 2× of Bun's on the same endpoint.
- Graceful shutdown works: in-flight requests complete.

**If async/await lowering is the schedule risk:** ship `await` as a blocking
call on a thread pool for M3, and note it. Do not let the state-machine work
block the HTTP milestone.

---

## 9. M4 — Scopes, recovery, typed errors (2–3 weeks)

**Goal:** make failures boring.

**Scope**

- `tsn_scope_run` fully implemented: arena + finalizers + recovery.
- Itanium-ABI unwinding, caught at the scope boundary.
- Typed domain errors end-to-end: `throw` → `domain_id` + CBOR payload →
  JS `Error` subclass with fields.
- `Result<T,E>` lowering.
- Cancellation: ambient token, `AbortError`, structured teardown.
- `tsn_runtime_cancel_all` graceful shutdown.

**Acceptance**

- A handler that throws at depth 20 returns a correct typed error; the next
  request succeeds.
- **10,000 consecutive panicking requests** with flat RSS.
- Cancellation at every `await` leaves zero orphaned tasks (asserted in tests).
- Error round-trip: every declared error type survives tsn → JS → tsn with
  fields intact.
- `tsn.errors.json` and the embedded manifest are byte-identical.

---

## 10. M5 — Realistic service (2 weeks) · GATE 3

**Goal:** the demo that justifies the project.

**Service:** a JSON API with one CPU-bound endpoint — JWT verification +
schema validation + templated response. Something every backend team writes.

Implement it twice: once in Bun/TypeScript, once in tsn. Same payloads, same
machine.

**Acceptance**

- **≥2× throughput** on the CPU-bound endpoint.
- **p99 latency ≤ Bun's** (this is where the no-GC story should show).
- Cold start < 10 ms.
- Static binary ≤ 10 MB.
- No leaks over a 1-hour soak at 500 req/s.

**If it fails:** the honest conclusion is that the win is workload-specific.
Pivot to the module target as the primary product (it already works from M2)
and position the standalone binary as an internal tool.

---

## 11. Measurement methodology

JIT benchmarks lie in both directions. The gate numbers are only meaningful if
the method is fixed **before** the first run.

- **Warmup:** Bun gets ≥ 5 s of warmup and ≥ 3 discarded runs. JSC/V8 need to
  reach their top tier.
- **Repetitions:** 10 runs; report median, p99, and standard deviation.
- **Tooling:** `hyperfine` for wall-clock; a custom harness for throughput.
- **Inputs:** identical payloads, identical machine, no other load.
- **Pinning:** `taskset` to a fixed core set where possible.
- **Reported:** throughput, p50/p99 latency, peak RSS, binary size, cold start.
- **Artifacts:** raw numbers committed to `tests/benches/results/` so claims are
  reproducible.

**Rule:** no benchmark number goes in a README without a committed script that
reproduces it.

---

## 12. Testing strategy

| Layer | Method |
|---|---|
| Parser / checker | golden tests (`insta`) on diagnostics |
| Codegen | golden IR snapshots |
| Semantics | **differential testing** vs Bun on the same source |
| ABI | C conformance harness against `tsn.h` |
| Layout | `static_assert(offsetof(...))` for every crossing struct |
| Boundary | `cargo-fuzz` on `tsn_str`/`tsn_bytes` entry points |
| Memory | valgrind/heaptrack on every milestone |
| Recovery | throw/cancel at every depth and every `await` |
| Soak | 1-hour run at target load before each gate |

**Differential testing is the highest-value item.** Any program that both Bun
and tsn accept must produce identical output. This finds semantic bugs faster
than any unit test suite, and it directly tests the "no silent divergence"
promise.

---

## 13. Risk register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Codegen materially slower than equivalent C | Medium | High | M1 gate; inspect generated C/IR vs Clang early; Bun is an informational baseline |
| Boundary cost dominates | Medium | High | M2 gate; coarse-grained leaf API |
| async lowering blows the schedule | High | Medium | Blocking fallback in M3 |
| Stdlib scope creep | High | High | M3 scope is fixed; no TLS/HTTP2 until v0.2 |
| Unwind cost surprises | Low | Medium | Measure in M4; `Result` is the escape |
| oxc AST gaps | Low | Low | Patch upstream or shim |
| LLVM binding churn | Medium | Low | Pin LLVM; `inkwell` behind a thin trait |
| Team underestimates runtime | High | High | Blocking-first; buy TLS, don't build it |

---

## 14. Scope discipline — what NOT to build before M5

Write this on the wall.

- No TLS. No HTTP/2. No DNS beyond `/etc/hosts` + a resolver stub.
- No generics beyond what the M1 kernel needs.
- No classes or inheritance.
- No getters/setters, no `Proxy`, no symbols.
- No incremental compilation.
- No package manager.
- No LSP. (A `tsn check` with good diagnostics is enough.)
- No Windows or macOS.
- No sidecar. (M2 proves the boundary; the sidecar is a v0.2 concern.)
- No QuickJS embedding.
- No self-hosting.

Every item here is real, and every one of them can wait.

---

## 15. Definition of done (v0)

- M0–M5 acceptance criteria all met.
- `ABI.md` frozen at 1.0, with the conformance suite green.
- A 2,000-line real service runs in production-like conditions for a week.
- Benchmark scripts reproduce every published number.
- `TYPES.md` divergences all implemented as compile errors or `error` lints.
- One external team has built something non-trivial without hand-holding.

That last one is the real bar. If only the authors can use it, it isn't done.

---

## 16. Post-v0 backlog (ordered)

1. epoll event loop (replace thread-per-connection)
2. TLS via `rustls`/BoringSSL
3. HTTP/2
4. Sidecar adapter (full npm, standalone)
5. Generics completion + `@noMonomorphize`
6. Cycle collection (`WeakRef` or trial deletion)
7. Incremental compilation
8. LSP + `tsn fmt`
9. linux/arm64
10. Package story

---

## 17. First ten days

Concrete, so nobody has to ask "what now?"

| Day | Output |
|---|---|
| 1 | Workspace builds; oxc parses `hello.ts`; AST printed |
| 2 | HIR + numeric type check; `const` and `function` |
| 3 | inkwell wired; `main` emits IR; `clang` links |
| 4 | `write(2)` builtin; `hello` prints |
| 5 | musl static link; `tsn build` CLI; CI green |
| 6 | Golden IR test harness (`insta`) |
| 7 | `i32`/`f64` arithmetic; integer wrap/trap |
| 8 | `string` + `Uint8Array` + indexing |
| 9 | `if`/`while`/`for`; functions with params |
| 10 | First byte-kernel spike; baseline timing vs Bun |

By day 10 there should be a number, even a bad one. A bad number on day 10 is
worth more than a good design document on day 30.
