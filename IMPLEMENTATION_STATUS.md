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
| Expressions | Numeric/boolean literals, locals, `+ - *`, selected bitwise ops, `>>>`, relational/equality and logical conditions, declared function calls, `Math.imul`, assignment, `++` |
| Typed arrays | Zero-initialized `Uint8Array` and `Uint32Array`, checked get/set, `.length`, compiler-generated cleanup on block/function exits; borrowed parameters are supported but cannot be returned or retained; aliases/rebinding, returns, and `for`-initializer declarations remain rejected |
| General arrays / slices / generic collections | Not implemented |
| Strings | UTF-8 literal/views, byte-based `.length`, content equality, checked `sliceUtf8`/`stringByteAt`, `printString`, concatenation, and source-level `stdlib/strings.ts::contains`; concatenated bytes are process-owned and released at standalone exit/trap, not yet request-scoped; builders and broad string APIs are absent |
| Objects / classes / generics / unions | Not implemented |
| Errors / async / per-request scopes / cancellation | OOB traps recover at standalone `main` and release tracked buffers; typed language errors and per-request/server scopes are not implemented |
| Raw pointers / unsafe | Deferred by decision; not part of the current v0 implementation path |
| Standalone Linux executable | Implemented for the supported subset; static ELF with a source-defined `main()` |
| libc/target baseline | Currently statically linked against the WSL distro's libc with `-march=native`; musl and portable CPU baseline are not yet implemented |
| Backend | Oxc AST → typed-ish C intermediate → Clang/LLVM IR and native link; direct LLVM IR lowering is future work |
| Stdlib/platform packages | `tsn:time` / `monotonicMs`; `tsn:io` scalar output and `readStdin`; minimal `tsn:net` TCP primitives; source utilities `stdlib/bytes.ts` (FNV-1a/equality), `stdlib/strings.ts` (`contains`), `stdlib/base64.ts` (base64url decode), `stdlib/crypto.ts` (SHA-256/HMAC-SHA-256 test-vector prototypes), and `stdlib/http.ts` smoke parser. `tsn:bench` remains benchmark-only; general HTTP/JSON and thread pool are absent. |
| Runtime support | Typed-buffer allocator/check/free and standalone trap boundary; no reusable general-purpose runtime library yet |

Current diagnostic categories: `TSN1000` parser, `TSN1301` loose equality,
`TSN2001` spanned type error, `TSN2000` other subset/semantic rejection, and
`TSN900x` toolchain/I/O errors. These are initial stable categories;
fine-grained codes and fix-it suggestions remain on the DX backlog.

## Near-term completion order

1. **Complete source typechecking:** close remaining operator/conversion gaps;
   C diagnostics must remain a backend sanity check, not tsn's semantic
   definition.
2. **Buffer correctness and scopes:** checked indexing and lexical cleanup are
   present; a Linux smoke test verifies main-boundary recovery. Add leak tests,
   typed error propagation, and per-request scopes before server use.
3. **Core DX:** fine-grained grammar diagnostic codes, suggestions/fix-its,
   negative/differential tests, and generated-IR snapshots.
4. **Server stdlib:** replace benchmark-only `tsn:bench` hooks with ordinary
   `tsn:fmt`/`tsn:io`, then add networking packages, with Go's stdlib as the
   completeness/consistency reference.

Pointers remain explicitly deferred. This does not block the above work.
