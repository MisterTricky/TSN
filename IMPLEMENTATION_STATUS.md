# Implementation status (working tracker)

The design documents describe the intended language; this file records what
the compiler/runtime actually accepts today. A feature is not implemented just
because Oxc can parse its syntax.

## Compiler slice currently in the tree

| Capability | Status |
|---|---|
| TypeScript/ECMAScript parsing | Oxc parser; syntax diagnostics surfaced |
| Language policy pass | Not implemented |
| Diagnostics | Syntax errors and type errors have stable category codes; type errors include source spans. Unsupported-syntax/backend errors still use generic phase codes and lack fix-it suggestions |
| Top-level module syntax | Synchronous functions, recursive relative named imports of exported functions, and named imports from `tsn:*`; function names must be globally unique; npm, default/namespace, and type-only imports are unsupported |
| Function signatures | Explicit primitive annotations; `main(): i32` required |
| Primitive type names | `u8/u16/u32/u64`, `i8/i16/i32/i64`, `f32/f64`, `number` (= `f64`), `bool`, `string`, `void` accepted in signatures/locals |
| Type checking | Partial source-level inference/checking for supported numeric/logical expressions, calls with lexical shadowing, assignments, returns/definite returns, mutability, exact literal ranges, and buffer allocation/index bounds; strict C conversion diagnostics remain a backstop |
| Integer semantics | Unsigned arithmetic wraps by C definition; i8/i16 arithmetic explicitly wraps through unsigned bit representations; Clang uses `-fwrapv` for i32/i64 arithmetic; trapping debug overflow mode is not implemented; shifts are restricted to u32/u64 and mask the shift count |
| `let` / `const` | Initialized local bindings; const reassignment/increment rejected; block shadowing supported |
| Control flow | Lexical blocks, `if`/`else`, `for`, `while`, `do`/`while`, unlabeled `break`/`continue` with cleanup; labels and several productions remain unsupported |
| Expressions | Numeric/boolean/string literals, locals, `+ - * /` (division promotes both numeric operands to `f64`), conditional expressions with boolean tests and compatible non-array branches, selected bitwise ops, `>>>`, relational/equality and logical conditions, declared function calls, `Math.imul`, simple assignment, integer `++`/`--` with prefix/postfix values; remainder/exponentiation, compound assignment, and many documented expressions remain unsupported |
| Typed arrays | Zero-initialized `Uint8Array` and `Uint32Array`, checked get/set, `.length`, compiler-generated cleanup on block/function exits; borrowed parameters are supported but cannot be returned or retained; aliases/rebinding, returns, and `for`-initializer declarations remain rejected |
| General arrays / slices / generic collections | Not implemented |
| Strings | UTF-8 literal/views, byte-based `.length`, content equality, checked `sliceUtf8`/`stringByteAt`, `printString`, concatenation, and source-level `stdlib/strings.ts::contains`; concatenated bytes are process-owned and released at standalone exit/trap, not yet request-scoped; builders and broad string APIs are absent |
| Objects / classes / generics / unions | Not implemented |
| Errors / async / per-request scopes / cancellation | OOB traps recover at standalone `main` and release tracked buffers; SIGINT/SIGTERM flag and bounded thread spawn/wait are present, but typed errors, request arenas, cancellation tokens, and async/task APIs are absent |
| Raw pointers / unsafe | Deferred by decision; not part of the current v0 implementation path |
| Standalone Linux executable | Implemented for the supported subset; static ELF with a source-defined `main()` |
| libc/target baseline | Currently statically linked against the WSL distro's libc with `-march=native`; musl and portable CPU baseline are not yet implemented |
| Backend | Oxc AST → typed-ish C intermediate → Clang/LLVM IR and native link; direct LLVM IR lowering is future work |
| Stdlib/platform packages | `tsn:time` / `monotonicMs`; `tsn:io` scalar output and `readStdin`; minimal `tsn:net` TCP primitives; `tsn:os` signal flag and bounded thread spawn; source utilities in `stdlib/` cover bytes, strings, base64url, SHA-256/HMAC, fixed-schema JSON, service core, and a bounded HTTP parser. General HTTP/JSON, typed errors, and task runtime remain incomplete. `tsn:bench` remains benchmark-only. |
| Runtime support | Typed-buffer allocator/check/free and standalone trap boundary; no reusable general-purpose runtime library yet |

## Current milestone evidence

- M3 constrained HTTP fixture: a 10-minute, four-worker WSL2 run completed
  6,129,355 requests at 10,215.6 req/s with zero request errors; p99 was
  0.957 ms versus 0.944 ms for the matching Bun fixture. See
  `benchmarks/http-m3/RESULTS.md`. This does not establish general HTTP
  conformance, and the run did not signal during a deliberately in-flight
  request, so graceful in-flight completion remains unverified.
- M5 remains incomplete: the service-core fixture is not a networked API and
  its measured throughput was about 1.08x Bun, below the documented 2x target;
  cold-start, binary-size, p99, and one-hour soak gates remain unverified.

Current diagnostic categories: `TSN1000` parser, `TSN1301` loose equality,
`TSN2001` spanned type error, `TSN2000` other subset/semantic rejection, and
`TSN900x` toolchain/I/O errors. These are initial stable categories;
fine-grained codes and fix-it suggestions remain on the DX backlog.

## Near-term v0 compiler completion order

HTTP/M5 work is not the current priority. Close compiler gaps against
`TYPES.md`, `GRAMMAR.md`, and `PLAN.md` in dependency order:

1. **Semantic completeness:** introduce/finish an explicit source analysis
   result so every accepted AST expression and statement has a checked type and
   every unsupported production is rejected with a source span before C
   emission. C diagnostics must be a backend check, not the language's type
   checker.
2. **Core scalar semantics:** complete documented numeric operators and
   conversions, assignment/update rules, booleans, and function/call checking;
   add positive execution tests and negative diagnostics for each rule.
3. **Usable language structures:** implement the v0-supported records/objects,
   arrays, tuples/unions, and generic functions, or explicitly resolve any
   draft items that are outside the agreed v0 contract before claiming v0.
4. **Control flow and modules:** complete documented statements, definite
   return/assignment analysis, import/export validation, and source-mapped
   diagnostics.
5. **Memory and errors:** implement the agreed v0 ownership/scope and error
   semantics with escape and cleanup tests; then handle async only if it remains
   in the v0 acceptance contract.
6. **Conformance gate:** run a checked-in positive/negative language corpus,
   differential tests for shared TS behavior, workspace quality checks, and
   standalone Linux execution tests. Do not infer v0 completion from the M1/M3
   performance or server fixtures.

Pointers remain explicitly deferred. This does not block the above work.
