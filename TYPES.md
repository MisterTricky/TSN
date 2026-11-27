# tsn Type System — Specification v0.1 (draft)

> Companion to `ABI.md`. "tsn" is a placeholder name.
> Status: **draft.** Everything here is negotiable until M2.
> This is the intended language contract, not a statement that every rule is
> implemented. See `IMPLEMENTATION_STATUS.md` for the live compiler subset.

---

## 0. Scope

This document defines the **static semantics**: which programs are accepted, what they mean, and how types lower to the C ABI in `ABI.md` §4.

It does not define the full grammar (see `GRAMMAR.md`, TODO) or runtime internals.

---

## 1. Goals

1. A TypeScript developer reads and writes it without a new mental model for the common case.
2. **No silent semantic changes.** Every divergence from JS/TS is either a compile error or a loud lint. Never a quiet behavior difference.
3. Soundness where it costs nothing at runtime; explicit opt-in where it doesn't.
4. Predictable performance: types determine layout, layout determines speed.

## 2. Non-goals

- Full TypeScript compatibility.
- Structural typing for everything.
- Runtime type reflection.
- `any`.
- Arbitrary-precision arithmetic as a primitive.

## 2.1 Systems-programming intent

The language is intended to support native server and systems software, not
only application scripting. The planned capability set includes fixed-width
numeric types, explicit allocation/layout, a native C ABI and FFI, threads and
atomics, and native Linux binaries. Raw-pointer/unsafe operations require an
explicit safety design; they are not yet specified and are outside the current
compiler spike. Kernel/embedded and freestanding use are not promised by v0.

### Pointers — deferred

Pointer types, explicit indirection, raw pointers, and unsafe pointer
operations are **out of scope for the current implementation phase**. They are
not prerequisites for scalar compilation or typed-buffer support. Do not add
pointer syntax or expose pointer APIs yet.

When revisited, the intended safe-code experience is Go-like: no pointer
arithmetic, arbitrary integer/address casts, or manual `free`; pointer validity
and lifetime are language/runtime managed. The memory model must be settled
first because Go's guarantees rely on tracing GC, while the current tsn proposal
uses arenas and refcounting.

---

## 3. Relationship to TypeScript

**Subset in syntax, stricter in semantics.** Most valid tsn is valid TS; the reverse is not true. The intended compiler is a TS parser plus a sound checker plus a lowering pass.

The governing principle for every divergence:

> **Make it loud.** If the semantics differ from JS, the program must fail to compile (or lint at `error`), never quietly do something else.

---

## 4. Soundness contract (target invariant)

These are requirements for accepted v0 programs. The current compiler enforces
only a subset; each missing check is tracked in `IMPLEMENTATION_STATUS.md` and
must be closed before claiming v0 type safety.

An accepted program has none of the following, without an explicit `unsafe` block:

| Property | Enforced by |
|---|---|
| No null dereference | `strictNullChecks`, always on |
| No out-of-bounds access | bounds checks (elided when provable) |
| No use of uninitialized values | definite assignment analysis |
| No unchecked dynamic escape | `any` banned; `unknown` requires narrowing |
| No data races | thread-affine objects; transfer only |
| No use-after-scope | arena escape analysis (§14) |
| No integer UB | defined wrap/trap semantics (§5) |

---

## 5. Numeric types

| Type | Width | Notes |
|---|---|---|
| `number` | 64 | **f64**, exactly JS semantics |
| `i8`/`i16`/`i32`/`i64` | signed | two's complement |
| `u8`/`u16`/`u32`/`u64` | unsigned | |
| `f32`/`f64` | IEEE-754 | `f64` ≡ `number` |
| `bool` | 1 byte | not interchangeable with numbers |

**Rules**

- **No implicit narrowing.** `i32` → `i8` requires `as`, which traps in debug builds if lossy.
- **Literals** adapt to context: `42` is `number` by default, `i32` in an `i32` context. `123n` is `i64` unless annotated.
- **Overflow:** wrapping in release, trapping in debug. Defined, not UB.
- **`/` always yields `f64`** (JS semantics). Integer division is `Math.idiv(a, b)`; remainder is `Math.imod(a, b)`. No new syntax.
- **Bitwise operators require integer types.** `a | 0` on a `number` is a compile error — use `Math.toI32(a)`.
- **`bigint`** maps to `i64`/`u64`, **not** arbitrary precision. Arbitrary precision lives in the stdlib `BigInt` type. This is a divergence and is linted.
- **`NaN`/`Infinity`** exist only on `f32`/`f64`.

---

## 6. Strings and text

- **Storage: UTF-8.** Immutable.
- **`.length` is bytes.** A lint (`tsn/string-length-bytes`) fires on every use and suggests `.byteLength` or `.charCount`.
- **`s[i]` is a compile error.** Use `.bytes()` (yields `u8`) or `.chars()` (yields `char`).
- **`char`** is a `u32` Unicode scalar value. `String.fromCodePoint(c)` constructs.
- **Concatenation allocates.** Loops must use `StringBuilder`; a lint fires on `+=` inside a loop.
- **No implicit `toString` coercion.** `"" + x` is a compile error unless `x` is a `string`.

---

## 7. Nullability

- **`null` only.** `undefined` is a compile error. (`void` returns are `void`, not `undefined`.)
- **`strictNullChecks` always on.**
- `T | null` lowers to `{ uint8_t present; T value; }` for value types, or a tagged pointer for references (niche-optimized to a null sentinel).
- Optional parameters `x?: T` mean `T | null`.
- **`!` (non-null assertion) is allowed but emits a debug-mode trap** on null. It is an assertion, not a cast.
- `??` and `?.` behave as in TS.

---

## 8. Objects

### Interfaces — structural

- Fixed layout, computed from the declaration. Field order matters and is part of the layout.
- **No property addition or deletion.** `obj.x = 1` where `x` is not declared is a compile error.
- Two structurally identical interfaces (same fields, same order) share a canonical layout.
- Excess-property checks on literals.

### Classes — nominal

- Single inheritance + any number of interfaces.
- Always heap-allocated, refcounted.
- Virtual dispatch via vtable; `final` methods devirtualized.
- `private`/`protected` are **compile-time only** — no runtime enforcement. This is a systems language, not a security boundary.

### Banned in v0

- `Proxy`, `Reflect`, getters/setters, `Object.defineProperty`, prototype mutation.
- `delete`, `with`, `arguments`.
- Symbols (except a small builtin set, later).

---

## 9. Arrays and collections

| Type | Lowering | Notes |
|---|---|---|
| `T[]` | growable, arena by default | no holes, no sparse |
| `[A, B]` | generated struct | fixed tuple |
| `Record<K, V>` | hash map | monomorphized |
| `Map<K, V>` / `Set<T>` | hash map / hash set | monomorphized |
| `Uint8Array` etc. | raw buffer | zero-copy at the ABI boundary |

- Array literals allocate in the current scope arena (§14).
- `Array.prototype` methods that allocate are documented as such; hot loops should use index loops.
- **No sparse arrays.** `arr[100] = x` on a length-3 array is a compile error; use `arr.push`.

---

## 10. Generics

- **Monomorphized** at compile time. `Array<T>` has one instantiation per concrete `T`.
- Constraints via `extends`.
- Type inference at call sites (local, not global).
- **Not supported in v0:** conditional types, mapped types, template literal types, higher-kinded types, variadic tuples.
- Code-size control: `@noMonomorphize` marks a function for a single boxed instantiation when the type parameter is only used opaquely. ⚠ Not designed yet.

---

## 11. Functions and `this`

- **Arrow functions** capture `this` lexically.
- **`function` declarations have no `this`.** Writing `this` inside one is a compile error. This removes JS's four binding rules entirely.
- Methods have `this`, bound at call.
- **No `arguments`.** Use rest parameters.
- Default parameters, rest parameters, and destructuring are supported.
- **No overload signatures.** Use union parameters or separate names.

---

## 12. Errors

- `class X extends Error` with declared fields. See `ABI.md` §7.
- `throw` accepts only `Error` subtypes; throwing a string or number is a compile error.
- `catch (e)` binds `e: unknown`; narrow with `instanceof`.
- `instanceof` on error types is a **compile-time** narrowing to a `domain_id` comparison.
- `Result<T, E>` is available for hot paths and lowers to status + out-params with no allocation.

---

## 13. Async and cancellation

- `async` returns `Promise<T>`; `await` unwraps.
- `await` on a non-Promise is allowed but linted (JS compatibility).
- **Cancellation is ambient.** Every scope carries an `AbortSignal`-shaped token, inherited by children. `await` on cancellable operations checks it automatically. No manual plumbing.
- `AbortError` is a builtin typed error, so `instanceof` works.
- `withTimeout(ms, fn)` creates a child scope whose token fires on a timer.
- **Structured concurrency:** a scope's tasks die with the scope. No orphans.
- Cancellation is **cooperative**: observed at `await` and explicit `signal.throwIfAborted()`. A CPU-bound loop does not cancel until it yields. Documented, not hidden.

---

## 14. Memory typing — scope-bound vs heap

This is the mechanism that makes the perf claim real, so it is part of the type system, not an optimization detail.

| Expression | Allocation |
|---|---|
| object literal `{ ... }` | current scope arena |
| array literal `[ ... ]` | current scope arena |
| `new C()` | heap (classes are always heap) |
| `box(x)` | heap copy |
| `StringBuilder`, `Map`, `Set` | arena by default |

**Escape rule:** a scope-bound value may not

- be returned from `tsn_scope_run`,
- be stored in a heap object,
- be sent to another thread,
- outlive the scope in any way the compiler can see.

Violations are **compile errors**, not runtime checks. This is what prevents the classic arena leak.

**Heap objects are refcounted.** Reference cycles leak. `WeakRef` is deferred. This is documented prominently — it is the one memory footgun the model accepts.

---

## 15. Modules

- ESM syntax (`import`/`export`).
- **Static imports only in v0.** Dynamic `import()` is a compile error.
- `import type` is supported and erased.
- No `require`, no `module.exports`, no `__dirname` (use `import.meta`).

---

## 16. Escape hatches

| Hatch | Effect |
|---|---|
| `unknown` | checked dynamic value; requires narrowing before use |
| `// tsn:allow-host-calls` | permits host imports outside module boundaries (file-level, audited) |
| `!` | non-null assertion; debug trap |

**Banned outright:** `any`, `@ts-ignore`, `@ts-expect-error`, `as unknown as T`, `declare module` augmentation, `eval`, `new Function`.

Raw pointers and unsafe operations are deferred. Current host/FFI boundaries do
not imply arbitrary pointer access.

---

## 17. Inference

- **Bidirectional:** annotations at module boundaries and exported signatures; inference locally.
- Literal widening: `const x = 1` → `1`; `let x = 1` → `number`.
- No whole-program inference. It is slow and produces unreadable errors.
- Inference never crosses a module boundary.

---

## 18. Divergence table

Every row is a place where tsn deliberately differs from JS/TS. All are compile errors or `error`-level lints.

| TS / JS | tsn | Reason |
|---|---|---|
| `undefined` | compile error | redundant with `null` |
| `any` | compile error | soundness |
| `var` | compile error | hoisting footgun |
| `==` / `!=` | compile error | coercion |
| `+` with mixed types | compile error | no coercion |
| truthiness on number/string/object | compile error | bug class |
| `s[i]` | compile error | UTF-8 vs UTF-16 |
| `.length` on string | bytes (linted) | UTF-8 |
| `/` on integers | `f64` | JS semantics |
| bitwise on `number` | compile error | require int types |
| integer overflow | wraps (release) | systems semantics |
| `this` inside `function` | compile error | binding rules |
| `arguments` | compile error | |
| `eval` / `Function` / `with` | compile error | AOT |
| `Proxy` / getters / `Reflect` | compile error (v0) | devirtualization |
| property add/delete | compile error | fixed layout |
| prototype mutation | compile error | |
| `bigint` | `i64`/`u64` | 64-bit, not arbitrary |
| class fields | fixed at declaration | layout |
| overload signatures | compile error | use unions |
| dynamic `import()` | compile error (v0) | AOT |

---

## 19. Lowering summary

| tsn type | C / ABI type |
|---|---|
| `bool` | `uint8_t` |
| `iN` / `uN` | `intN_t` / `uintN_t` |
| `f32` / `f64` / `number` | `float` / `double` |
| `string` (borrowed) | `tsn_str` |
| `string` / `bytes` (owned) | `tsn_obj*` |
| `T[]` (borrowed) | `{ const T*; size_t }` |
| `T[]` (owned) | `tsn_obj*` |
| interface | `#[repr(C)]` struct, by pointer |
| class | opaque `tsn_obj`-like handle + vtable |
| `T \| null` | `{ uint8_t present; T }` or tagged pointer |
| `enum` | `uint32_t` |
| `Result<T,E>` | `tsn_status` + out-params |
| `Promise<T>` | `tsn_task*` at the boundary |

---

## 20. Open questions

1. **Conditional / mapped types.** Powerful, but they turn the checker into a small interpreter and make error messages awful. Deferred. Revisit if real code demands them.
2. **Structural layout canonicalization.** Field order is part of the layout. Should structurally equal interfaces with different field order be compatible? (Currently: no. Simpler and more predictable.)
3. **`char` ergonomics.** `s.chars()` yielding `u32` is correct but clunky. Consider a `Char` wrapper with `isLetter()` etc.
4. **Getters.** Could be allowed if devirtualized at the call site. Needs measurement.
5. **Cycle collection.** Refcount leaks cycles. Is `WeakRef` enough, or is a trial-deletion collector needed?
6. **`@noMonomorphize`.** How does a boxed instantiation interact with the escape rule?
7. **Union of function types.** Call signatures over unions are a known TS pain point. Support or ban?
