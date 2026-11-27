# tsn C ABI — Specification v0.1 (draft, rev 3)

> "tsn" is a placeholder prefix. Rename once the language has a name.
> Status: **draft, not frozen.**
> Rev 2: per-request recovery, typed domain errors, scope API, host-import enforcement.
> Rev 3: embedded error manifest, ambient cancellation, explicit by-value exception.
> Sections marked ⚠ remain open.

---

## 0. Scope

This document defines the binary contract between:

1. **Compiled tsn code** and **the tsn runtime**,
2. **Compiled tsn code** and **hosts** (Node/Bun via N-API, a sidecar process, an embedded JS engine, or plain C),
3. **Compiled tsn modules** and **each other**.

It does **not** define the language grammar, type system (see `TYPES.md`), or memory model internals.

**Platform:** linux/amd64 only. SysV AMD64 psABI. C99 ABI. No C++ ABI, no varargs.

---

## 1. Non-goals

- Windows / macOS / arm64 (deferred; see §17).
- Cross-language generic instantiation (generics are monomorphized; only concrete types cross).
- Passing closures, Promises, JS objects, or language-level class instances.
- A stable ABI for *internal* runtime functions. Only the symbols listed here are contractual.
- Wasm. Explicitly out of scope.

---

## 2. Design principles

| # | Principle | Consequence |
|---|---|---|
| P1 | **No hidden allocation at the boundary.** | Every allocation is explicit and owned by exactly one side. |
| P2 | **No unwinding across the C ABI.** | Unwinding is internal and caught at scope boundaries. |
| P3 | **Compound values pass by pointer, never by value.** | Sole exception: the fixed word-pair view types (`tsn_str`, `tsn_bytes`, `tsn_mut_bytes`, `tsn_type_tag`), which every psABI classifies identically. |
| P4 | **Borrowed ≠ owned.** | Views are valid only for the call. Persistence requires an owned handle. |
| P5 | **The core knows nothing about JavaScript.** | All JS-shaped concepts live in adapters. |
| P6 | **Fast path is allocation-free.** | Errors and rare cases may allocate; hot paths may not. |
| P7 | **Versioned and append-only.** | Structs grow via `struct_size`; enums never change meaning. |
| P8 | **One runtime per process.** | No runtime handle threading through every signature. |
| P9 | **Failure is scoped, not fatal.** | Every request runs in a scope that recovers and tears down cleanly. |
| P10 | **Cancellation is ambient and structured.** | Scopes own tokens; children die with parents; no manual plumbing. |

---

## 3. Versioning and initialization

```
tsn_abi_version() -> (MAJOR << 16) | MINOR
```

- **MAJOR** changes only on breaking ABI changes.
- **MINOR** increments on additive changes.
- Every extensible struct begins with `uint32_t struct_size`. Callers set `struct_size = sizeof(T)`; callees MUST reject sizes below their minimum.
- Never reorder or remove fields. Never change an enum value. To change a signature, add `_v2`.

Initialization is **once per process**:

```c
tsn_status tsn_runtime_init(const tsn_runtime_config* cfg);
void       tsn_runtime_shutdown(void);
tsn_status tsn_runtime_cancel_all(uint32_t grace_ms);
```

`tsn_runtime_init` MUST be called before any other export. Calling it twice returns `TSN_EINVAL`. `tsn_runtime_shutdown` is idempotent and must not run concurrently with in-flight tasks.

---

## 4. Type mapping

| Language type | C type | Notes |
|---|---|---|
| `bool` | `uint8_t` | MUST be 0 or 1 |
| `i8`/`i16`/`i32`/`i64` | `int8_t`…`int64_t` | two's complement |
| `u8`/`u16`/`u32`/`u64` | `uint8_t`…`uint64_t` | |
| `f32`/`f64` | `float`/`double` | IEEE-754 |
| `void` | `void` | only as return |
| `string` (borrowed) | `tsn_str` | UTF-8, not NUL-terminated |
| `bytes` (borrowed) | `tsn_bytes` / `tsn_mut_bytes` | |
| `string`/`bytes` (owned) | `tsn_obj*` | refcounted |
| `T[]` (borrowed) | `{ const T* ptr; size_t len; }` | generated per `T` |
| `T[]` (owned) | `tsn_obj*` | |
| `interface` / struct | generated `#[repr(C)]` struct | by pointer only |
| C-like `enum` | `uint32_t` | explicit values; no negatives |
| tagged union | `struct { uint32_t tag; union { … }; }` | tag first |
| `T \| null` | `struct { uint8_t present; T value; }` | explicit; no sentinels |
| `Result<T,E>` | `int32_t` status + out-params | see §7 |

**Rules**

- Strings are **UTF-8**. `len` is bytes, not codepoints. Interior NULs are legal.
- No NUL terminator is guaranteed.
- Structs use natural alignment, declaration order, no packing. Max alignment 16.
- Padding bytes MUST be zeroed before a struct crosses the boundary.
- Bitfields are forbidden.
- Arrays of structs are packed C arrays (stride = `sizeof(T)`).

---

## 5. Passing rules

**P3: no struct by value, with one narrow exception.**

```c
/* YES */
tsn_status foo(const tsn_config* in, tsn_obj** out, tsn_error* err);

/* NO */
tsn_result foo(tsn_config in);        /* struct return by value — forbidden */
```

SysV AMD64 classifies small structs into INTEGER/SSE registers, and those rules are easy to get wrong across compilers, languages, and FFI layers. Passing by pointer makes the ABI trivially stable; the cost is one indirection.

**The exception:** `tsn_str`, `tsn_bytes`, `tsn_mut_bytes`, and `tsn_type_tag` MAY be passed and returned by value. They are 1–2 word pairs of `(pointer, length)` or `(bytes[16])`, classified `INTEGER,INTEGER` by every psABI-conforming compiler on every architecture. There is no classification ambiguity to get wrong, and making `tsn_obj_data()` return an out-param would be needless friction.

Everything else — every generated struct, every tuple, every tagged union — crosses by pointer.

- **Primitives** pass by value.
- **Returns** are a primitive, a pointer, one of the four view types, or `tsn_status` with out-params.
- **No variadic functions.**
- **Calling convention** is standard SysV AMD64 psABI.
- **Stack alignment** at call sites is 16 bytes. Do not rely on the red zone.

---

## 6. Ownership and lifetimes

| Kind | Created by | Freed by | Valid until |
|---|---|---|---|
| `tsn_str` / `tsn_bytes` (borrowed) | callee or caller | nobody | the call returns |
| `tsn_obj*` (owned) | `tsn_obj_new`, or out-param | `tsn_obj_release` | refcount hits 0 |
| `tsn_error.payload` | callee on error | `tsn_error_dispose` | dispose |
| `tsn_arena*` | `tsn_arena_create` | `tsn_arena_destroy` | destroy |
| `tsn_task*` | `tsn_task_spawn` | `tsn_task_release` | release |
| scope arena memory | `tsn_scope_alloc` | scope teardown | scope ends |

**Rules**

1. A borrowed view MUST NOT outlive the call that produced it. Holding one is UB.
2. An owned object is transferred to the caller unless documented "borrowed."
3. `tsn_obj_retain` increments; `tsn_obj_release` decrements and frees at zero.
4. Refcounts are **non-atomic**. Objects are thread-affine (§11).
5. Out-params are written only on success unless documented otherwise.
6. **Anything that must outlive a scope MUST be heap-allocated (`tsn_obj`), never arena-allocated.** The runtime enforces this at `tsn_scope_run`.

---

## 7. Typed errors

Two channels, deliberately separate:

- **`code`** — transport-level outcome (`TSN_EINVAL`, `TSN_EIO`, `TSN_EPANIC`, …).
- **`domain_id` + `payload`** — language-level error identity and structure.

```c
typedef struct {
    uint32_t  struct_size;
    tsn_status code;
    uint64_t  domain_id;   /* 0 = untyped */
    tsn_obj*  payload;     /* owned, nullable; CBOR */
} tsn_error;
```

### Identity

`domain_id` is **FNV-1a 64 over the UTF-8 string `"module:TypeName"`**. It is computed at compile time, embedded in generated code, recorded in the error manifest, and reproducible from C via `tsn_domain_id()`.

`0` is reserved for "untyped." Collisions MUST be detected at build time (the compiler errors if two types hash equal).

### Payload

CBOR-encoded map of the error's declared fields. The manifest carries the schema. **Schema evolution is add-only:** readers ignore unknown keys; writers never remove keys; a field name never changes type.

### Language surface

```ts
class NotFoundError extends Error {
  readonly id: string;
}

try {
  const user = await db.find(id);
} catch (e) {
  if (e instanceof NotFoundError) {      // compile-time narrowing
    return response(404, { id: e.id });
  }
  throw e;
}
```

`instanceof` is a compile-time type guard lowered to a `domain_id` comparison — no runtime class walk.

### `Result<T, E>`

For hot paths where unwind tables are unwanted:

```ts
const r: Result<User, NotFoundError> = db.find(id);
if (!r.ok) { … }
```

`Result` lowers to status + out-params, allocates nothing, and produces the same `tsn_error` at the ABI. **`throw` is the default; `Result` is the opt-in.**

### C-side matching

```c
if (err.code == TSN_EUSER && err.domain_id == TSN_DOMAIN_app_NotFoundError) { … }
```

### Manifest

The compiler emits `tsn.errors.json` **and embeds the same document in the binary.** The embedded copy is the source of truth; the file is for tooling.

- `tsn_manifest_json()` returns a borrowed UTF-8 view, valid for process life.
- The manifest carries `manifest_version`, `abi_major`, `abi_minor`, and the error table.
- Adapters read it at init: N-API generates `Error` subclasses; the sidecar receives it during handshake; QuickJS gets it injected.
- **One artifact, no desync, no "missing manifest" failure mode.**

### Fast path

- `domain_id == 0` and `payload == NULL` means "no detail" — zero allocation.
- Payload is allocated only when an error type declares fields.
- Expected validation failures SHOULD use `Result` with a payload-free variant.

---

## 8. Exceptions, traps, and request recovery

**Uncaught exceptions do not kill the process. They kill the request.**

- `throw` unwinds using the **Itanium C++ ABI** (`_Unwind_RaiseException`, `.eh_frame`, `.gcc_except_table`, generated landing pads).
- Runtime traps — bounds violations, null dereference, checked-integer overflow — take the same path.
- The nearest `catch` handles it. If none, the nearest **request scope** handles it (§9).
- The scope runs finalizers LIFO, frees its arena, and reports `TSN_EPANIC` with the thrown error as payload. The runtime stays healthy; the next request runs normally.
- **Unwinding never crosses the C ABI.** The boundary is `noexcept`; the catch-all lives inside the runtime.

**Non-recoverable conditions still abort:** OOM, allocator corruption, stack overflow, and any exception escaping every scope (a bug). These log a diagnostic before `abort()`.

**Tasks:** each task body runs in its own scope. An uncaught exception surfaces as `TSN_EPANIC` from `tsn_task_take_result`; the worker survives.

**Cost:** unwind tables cost code size. Modules that avoid `throw` in hot paths and use `Result` pay nothing. ⚠ The overhead has not been measured; see §16.

---

## 9. Async, tasks, scopes, and cancellation

### Scopes

A scope is simultaneously an **arena**, a **cleanup stack**, a **recovery point**, and a **cancellation node**.

```c
typedef tsn_status (*tsn_handler_fn)(void* ctx, tsn_obj** out);

tsn_status tsn_scope_run(tsn_handler_fn fn, void* ctx, tsn_obj** out, tsn_error* err);
void*      tsn_scope_alloc(size_t n, size_t align);          /* current scope */
void       tsn_scope_on_cleanup(tsn_finalizer_fn fn, void* ctx);
void       tsn_scope_cancel(void);
int32_t    tsn_scope_cancelled(void);
```

- Scopes nest; the innermost catches first.
- `tsn_scope_alloc` bumps the current scope's arena. Freed wholesale at teardown.
- `tsn_scope_on_cleanup` registers a finalizer for resources that are *not* arena memory (fds, pooled handles, retained objects). Finalizers run LIFO on normal, panic, and cancellation exits.
- `*out` MUST be a heap `tsn_obj`. The runtime rejects arena pointers returned as results.

**Typical adapter use:** the HTTP server wraps each request in `tsn_scope_run`. Request parsing allocates in the arena; the response is heap-allocated; the arena dies at scope end. A panic costs one arena teardown, not a process.

### Tasks

`tsn_task` is a **completion handle**, not a Promise and not a future.

- `spawn` starts work on the runtime's pool and returns immediately.
- `on_done` fires **on the runtime's completion thread**, exactly once, before `release`.
- `take_result` transfers ownership; calling it twice returns `TSN_EINVAL`.
- **`wait` on the host event loop deadlocks or stalls the host.** Adapters MUST NOT do this.
- After `release` the pointer is invalid.

Adapters bridge tasks; they never expose them. A JS caller sees a Promise; a C caller sees a task.

### Cancellation

Cancellation is **ambient, cooperative, and structured.** It is modelled on `AbortSignal` because that is the idiom TS developers already know, but propagation is implicit so nobody threads a `signal` parameter by hand.

- Every scope owns a cancellation token. `tsn_scope_run` creates a child of the current token; scopes form a tree.
- `await` on any cancellable operation (IO, timers, task joins) checks the token and throws a typed `AbortError`.
- Explicit surface for long computations: `signal.aborted`, `signal.reason`, `signal.onAbort(cb)`, `signal.throwIfAborted()`, and the C equivalents `tsn_scope_cancelled()` / `tsn_scope_cancel()`.
- **Structured concurrency:** when a scope ends — normally, by panic, or by cancellation — every task spawned inside it is cancelled. No orphans.
- `withTimeout(ms, fn)` is sugar: a child scope whose token fires on a timer.
- `signal.reason` carries a typed error (`AbortError`, `TimeoutError`, `ParentCancelled`, `Shutdown`), so `instanceof` narrowing works.
- **Cooperative means cooperative.** A CPU-bound loop with no `await` will not cancel until it yields. This is documented, not hidden. ⚠ See §16 for the backedge-check option.

**Host integration**

- The HTTP adapter ties the request token to client disconnect.
- `tsn_runtime_cancel_all(grace_ms)` implements graceful shutdown: every scope and task is cancelled, in-flight `tsn_scope_run` returns `TSN_ECANCELED`, and the runtime waits up to `grace_ms` before forcing teardown.

---

## 10. Host imports (npm leaf)

A declared npm import lowers to a **host import**, resolved once at module init.

```ts
import { verify } from "npm:some-jwt-lib";
```

```
resolve("npm:some-jwt-lib#verify", "(bytes,bytes)->bytes")
```

### Signature grammar

```
sig    := '(' params ')' '->' ret
params := type (',' type)* | ε
type   := 'void' | 'bool' | 'i8' | 'i16' | 'i32' | 'i64'
        | 'u8' | 'u16' | 'u32' | 'u64' | 'f32' | 'f64'
        | 'str' | 'bytes' | 'obj'
ret    := type
```

- `str`, `bytes`, and `obj` are the **only** aggregate types permitted at a host boundary.
- Fallibility is implicit: every host import returns `tsn_status` with out-params.
- `resolve` returning `NULL` means "unavailable" → `tsn_module_<name>_init` returns `TSN_EHOST`, unless the import was declared `optional`.

### Resolution modes

| Build | `resolve` backed by | Engine in process? | npm compat |
|---|---|---|---|
| Hosted (N-API) | adapter → JS function | No (host has it) | full |
| Standalone + sidecar | IPC client → Node | No | full |
| Standalone + embedded | QuickJS adapter | Yes (~1 MB) | pure-JS only |
| Standalone, no host | `NULL` table | No | none |

### Enforcement: leaf-only, by default

Every host call **copies** arguments into the host representation and results back. That is why imports are leaf-only.

**The type checker rejects a host import call unless all of these hold:**

1. The enclosing function is `async`.
2. The enclosing function is a module-boundary export.
3. The call is not lexically inside a loop, a `.map`/`.forEach`/`for await`, or a recursive cycle.

Violation is a **compile error**.

### The escape hatch

The escape hatch is **file-level**, because the leaf boundary is a module
concept. tsn adds no decorator syntax (GRAMMAR §0), so it is a pragma:

```ts
// tsn:allow-host-calls
import { verify } from "npm:some-jwt-lib";

export async function legacyReport(rows: Row[]): Promise<Buffer> {
  return renderWithNpm(rows);          // permitted; audited
}
```

- The pragma downgrades the error to a warning and emits a build-report entry.
- The build report lists every file carrying it, plus per-file host-call counts.
  Review it in CI.
- **Dev-mode runtime guard:** `host_call_warn_threshold` counts host calls per
  scope and logs when exceeded.
- The hatch does not change marshalling cost. It changes who is allowed to pay it.
- A file-level pragma is coarser than a function-level annotation. That is the
  accepted cost of zero new syntax.

---

## 11. Threading

- The runtime owns `thread_count` worker threads plus an IO thread.
- **Objects are thread-affine.** A `tsn_obj*` created on thread A MUST NOT be used on thread B unless explicitly transferred.
- **Transfer** is by channel/move, never by sharing. The ABI exposes no locks or atomics for user data.
- Functions are thread-safe unless documented otherwise. Runtime-global state is internally synchronized.
- `tsn_task_on_done` callbacks run on a runtime thread. Calling back into the host from a runtime thread requires the host's threading primitive (N-API: `napi_threadsafe_function`).
- Scopes are per-thread and do not migrate. A task that resumes on another thread starts a fresh scope, still a child of the spawning token.

---

## 12. Adapter: N-API (hosted mode)

- Entry: `napi_register_module_v1` / `NAPI_MODULE`.
- `tsn_runtime_init` is called from module `init`, with a `tsn_host` whose `resolve` wraps the requested npm export.
- **Strings:** `napi_get_value_string_utf8` (two-call pattern) into the scope arena.
- **Large buffers:** prefer `napi_get_buffer_info` for a zero-copy `tsn_bytes` view. The `ArrayBuffer` MUST NOT be detached during the call.
- **Results:** `tsn_obj*` → `napi_create_buffer_copy`, or an external buffer with a finalizer calling `tsn_obj_release`.
- **Errors:** read `tsn_manifest_json()` at init, generate `Error` subclasses, map `domain_id` → class and payload → fields.
- **Async:** `napi_create_promise` + `napi_threadsafe_function`. `on_done` enqueues to the TSFN; the JS callback resolves.
- **Cancellation:** JS `AbortSignal` → `tsn_scope_cancel()` on the current scope.
- **Never** call `tsn_task_wait` on the JS thread.
- **Recovery:** a recovered panic becomes a rejected Promise / thrown JS Error, not a process exit.

---

## 13. Adapter: sidecar (standalone + full npm)

- One Unix-socket connection per process, multiplexed by request id.
- Framing: 4-byte little-endian length prefix + CBOR. (JSON is debug-only.)
- The manifest is exchanged during handshake so both sides agree on error ids.
- Each host import is a request/response pair. Async maps naturally: the client returns a pending task resolved on response.
- Bounded in-flight window (default 64); overflow returns `TSN_ETIMEOUT`.
- Latency budget: < 100 µs on loopback for small payloads. Measure; do not assume.
- Sidecar failure is a hard error (`TSN_EHOST`), never a silent fallback.

---

## 14. Adapter: embedded QuickJS (standalone + pure-JS npm)

- `JS_NewRuntime` + `JS_NewContext` at init; one context per worker thread.
- Only **pure-JS** packages load. Any `require('fs'|'net'|'http'|…)` fails at load with a clear diagnostic.
- Memory cap and interrupt handler enforce per-call time and heap limits.
- The manifest is injected so JS-side error objects reconstruct correctly.
- Calls are synchronous into QuickJS; wrap them in a task so runtime threads are not blocked.
- ⚠ Interop fidelity (Buffer, `process.env`, `TextEncoder`) is limited. Document loudly.

---

## 15. Symbols, visibility, build

- All exports prefixed `tsn_` (placeholder). Generated module exports use `tsn_<module>_<name>`.
- Default visibility **hidden**; only `TSN_EXPORT` symbols are public.
- No C++ mangling. `extern "C"` throughout.
- Static binary: link `musl` statically; no `libgcc`/`libstdc++` runtime dependency.
- Shared library / addon: `-fvisibility=hidden -fPIC`, `SONAME` pinned to `MAJOR`.
- Reproducible builds: `-ffile-prefix-map`, deterministic archives, no build timestamps.
- Unwind info (`.eh_frame`) is required in all builds — recovery depends on it. Do not strip it.

---

## 16. Conformance and open questions

**Conformance tests (must exist before M1 ships)**

- C harness linking the shared library, calling every export.
- JS harness loading the N-API addon, exercising every generated export.
- Layout assertions: `static_assert(offsetof(T, f) == N)` for every crossing struct.
- Boundary fuzzing: random bytes into every `tsn_str`/`tsn_bytes` entry point.
- Leak checks: refcount balance under a 10M-iteration loop.
- **Recovery tests:** throw at every depth; assert the runtime serves the next request.
- **Cancellation tests:** cancel at every `await`; assert structured teardown and no orphans.
- **Error round-trip tests:** every declared error type survives C → JS → C with fields intact.

**Open questions (⚠)**

1. **Shared (cross-thread) objects.** Currently forbidden. If needed, introduce a distinct `tsn_shared*` type with atomics rather than making `tsn_obj` thread-safe.
2. **CBOR schema evolution.** Add-only is stated, but tooling to detect incompatible changes across versions is not designed.
3. **Unwind cost.** Deferred by decision. Measure on realistic modules before freezing; if overhead exceeds ~15%, reconsider making `Result` the default.
4. **Nested scope depth.** Deeply nested scopes are cheap, but a pathological chain could exhaust the arena stack. Define a limit.
5. **Cancellation inside CPU-bound loops.** Cooperative cancellation cannot interrupt a tight loop. Consider backedge yield checks with a counter (the .NET / JSC approach) if this bites.

---

## 17. Deferred targets

| Target | Trigger | Notes |
|---|---|---|
| linux/arm64 | first user asks | same code; cross-compile |
| macOS | first user asks | kqueue, Mach-O, codesigning |
| Windows x64 | paid demand | IOCP is real work; last |

Adding a target MUST NOT change this ABI. If it would, the ABI was wrong.
