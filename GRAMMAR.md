# tsn Grammar — Specification v0.1 (draft)

> Companion to `ABI.md`, `TYPES.md`, `PLAN.md`.
> Status: **draft.**
> This is the target rejection policy; the current compiler implements only a
> subset. Track actual support in `IMPLEMENTATION_STATUS.md`.

---

## 0. The core decision

**tsn adds zero new syntax.**

The grammar is exactly the ECMAScript + TypeScript grammar as parsed by
[oxc]. tsn is defined by **rejection**: a policy pass walks the oxc AST and
rejects specific productions, with stable error codes.

Everything that looks like a language extension is expressed with constructs
that already parse:

| Need | Expressed as | Not as |
|---|---|---|
| Fixed-width numerics | type names `i32`, `u64`, `f32` | new literals |
| Unsafe operations | deferred; no raw-pointer operations in the current subset | no new syntax |
| Heap vs arena | `new` vs literals | a `heap` keyword |
| Host-import escape hatch | file pragma `// tsn:allow-host-calls` | a decorator |
| Stdlib | `import … from "tsn:net"` | new keywords |
| npm | `import … from "npm:foo"` | bare specifiers |

**Why this matters:** the parser is fully reused, editor tooling works
unmodified, and there is no syntax to maintain. The cost is that a few
constructs are coarser than they could be (the escape hatch is per-file, not
per-function). That trade is worth it.

**Rule:** if oxc cannot parse it, tsn cannot have it.

---

## 1. Layers of rejection

Rejections happen at three layers. Each has a distinct error-code range.

| Layer | Examples | Range |
|---|---|---|
| **Lexical** | legacy octal `0123`, lone surrogates, HTML comments | `TSN1xxx` |
| **Syntactic** | `var`, `==`, `with`, `yield`, `namespace` | `TSN1xxx` |
| **Semantic** | `this` inside `function`, arena escape, `any` | `TSN2xxx` |

All three are compile errors. None are warnings.

---

## 2. Lexical grammar

### 2.1 Source

- **Encoding:** UTF-8. A byte-order mark is a compile error (it hides in diffs).
- **Line terminators:** LF, CRLF, CR, LS (U+2028), PS (U+2029). All are
  terminators for ASI purposes.
- **Strict mode is always on.** `"use strict"` is accepted and ignored.
- Sloppy-mode-only productions are rejected (see §2.3, §2.9).

### 2.2 Whitespace and comments

Accepted: space, tab, VT, FF, NBSP, ZWNBSP, Unicode `Zs`.
Accepted comments: `//` line, `/* */` block. Block comments nest **no** (per spec).

Rejected:

| Construct | Code | Why |
|---|---|---|
| `<!--` HTML open comment | TSN1101 | sloppy mode only |
| `-->` HTML close comment | TSN1102 | sloppy mode only |
| `#!` hashbang | TSN1103 | not a script we run |
| BOM at file start | TSN1104 | hides in diffs |

### 2.3 Identifiers and keywords

- Start: `ID_Start`, `$`, `_`. Continue: `ID_Continue`, `$`, `_`, ZWNJ, ZWJ.
- Unicode escapes `\uXXXX` and `\u{…}` are accepted in identifiers.
- **Reserved words** (full ES list) are rejected as identifiers.
- **Contextual keywords** keep their contextual behavior: `let`, `static`,
  `yield`, `await`, `async`, `of`, `as`, `satisfies`, `type`, `interface`,
  `enum`, `implements`, `private`, `public`, `protected`, `readonly`,
  `abstract`, `declare`, `namespace`, `module`, `keyof`, `infer`, `is`,
  `asserts`, `unique`, `override`, `accessor`, `using`.

Because strict mode is always on, `let`, `static`, `yield`, `implements`,
`interface`, `package`, `private`, `protected`, `public` are **reserved**, not
contextual. This removes a whole class of parser ambiguity.

### 2.4 Numeric literals

| Form | Accepted | Notes |
|---|---|---|
| `42`, `4.2`, `.5`, `5.`, `1e10` | yes | |
| `0x1F` | yes | |
| `0o17` | yes | |
| `0b1010` | yes | |
| `1_000_000` | yes | separators |
| `123n` | yes | `bigint` → `i64`/`u64` (TYPES §5) |
| `0123`, `08`, `09` | **no** | TSN1105 — use `0o123` |

`Infinity` and `NaN` are globals, not literals.

### 2.5 String literals

`'…'` and `"…"` with escapes `\n \t \r \b \f \v \0 \\ \' \" \xNN \uNNNN \u{…}`
and line continuations.

**Lone surrogates are a compile error** (TSN1106). A `\uD800` cannot be encoded
in UTF-8, and silently substituting U+FFFD would be a quiet semantic change.
This is the "make it loud" rule applied at the lexer.

### 2.6 Template literals

Accepted: `` `text ${expr} text` `` with nesting.

Rejected:

| Construct | Code | Replacement |
|---|---|---|
| Tagged template `` tag`…` `` | TSN1107 | call the function explicitly |

### 2.7 Regular expression literals

`/…/flags` **parses** but is rejected by policy in v0 (TSN1108), because no
regex engine ships yet.

This is deliberate: parse-accept, semantic-reject. When `tsn:regex` lands, only
the policy entry is removed — no parser change, no grammar change.

### 2.8 Punctuators

The full ES set. Nothing added, nothing removed.

### 2.9 Automatic semicolon insertion

**Inherited from oxc.** We do not implement ASI; we rely on a conformant parser
and a torture-test corpus (§10).

The restricted productions are therefore inherited too:

- `return` / `throw` / `break` / `continue` / `yield` followed by a newline.
- Postfix `++` / `--` before a newline.
- `=>` must be on the same line as the parameter list.
- `async` followed by a newline is an identifier, not a modifier.
- `let` followed by `[` is ambiguous and resolved per spec.

**Policy note:** we do not ban multi-line expressions. We *do* require
formatters to never emit a line break in a restricted production, and the
conformance suite covers every case in the spec's ASI examples.

---

## 3. Statements

### Accepted

`{ }`, `;`, expression statements, `if`/`else`, `while`, `do…while`, `for`,
`for…in`, `for…of`, `for await…of`, `break`/`continue` (with labels),
`return`, `throw`, `try`/`catch`/`finally`, `switch`, labelled statements,
`function` declarations, `class` declarations, `import`/`export`.

### Rejected

| Construct | Code | Replacement |
|---|---|---|
| `var x = …` | TSN1201 | `let` / `const` |
| `with (o) { … }` | TSN1202 | explicit property access |
| `debugger;` | TSN1203 | remove it |
| `function*` / `yield` | TSN1204 | `async`/`await` |
| `async function*` | TSN1205 | `async`/`await` |
| `label: for(;;) { break label; }` | accepted | — |

### Notes

- `for…in` is accepted but linted at `warn`: it enumerates string keys of a
  fixed-layout object, which is almost never what you want. `for…of` over
  `Object.keys(o)` is the idiomatic form.
- `switch` requires `default` (TSN1206). Missing `default` is a bug in 99% of
  cases, and the check is free.

---

## 4. Expressions and operators

### Accepted

Literals, identifiers, `this`, `super`, arrays, objects, functions, arrows,
classes, template literals, `new`, member access, calls, optional chaining
`?.`, nullish coalescing `??`, `typeof`, `instanceof`, `in`, `void`-less unary
`! ~ + -`, `await`, spread, destructuring, assignment (including `&&=`, `||=`,
`??=`), conditional `?:`, comma-free sequences.

### Rejected

| Construct | Code | Replacement |
|---|---|---|
| `a == b`, `a != b` | TSN1301 | `===` / `!==` |
| `delete o.x` | TSN1302 | design without deletion |
| `eval(…)`, `new Function(…)` | TSN1303 | nothing; AOT forbids it |
| `arguments` | TSN1304 | rest parameters |
| `undefined` | TSN1305 | `null` |
| `void expr` | TSN1306 | remove it |
| `a, b` (comma operator) | TSN1307 | separate statements |
| `import(…)` | TSN1308 | static import |
| `new.target` | TSN1309 | — |
| `o.#priv` | TSN1310 | `private` (compile-time) |
| `Proxy`, `Reflect` | TSN1311 | plain objects |
| getters / setters | TSN1312 | methods |
| `delete`, `Object.defineProperty` | TSN1313 | — |
| prototype mutation | TSN1314 | — |

### `this`

`this` is accepted **only** inside class methods and arrow functions nested
within them.

| Context | Result |
|---|---|
| method body | ok |
| arrow inside method | ok (lexical) |
| `function` declaration/expression | **TSN1315** |
| top level | **TSN1315** |

This removes JS's four binding rules entirely (TYPES §11).

### Truthiness

`if (x)`, `while (x)`, `!x`, `x && y`, `x || y` require `x` to be `boolean` or
`T | null`.

| Operand type | Result |
|---|---|
| `boolean` | ok |
| `T \| null` | ok (means "not null") |
| `number`, `string`, object | **TSN1316** — write the comparison |

### `+` and coercion

`a + b` requires both operands to be `string` **or** both numeric.
Mixed and object operands are TSN1317. There is no implicit coercion anywhere.

---

## 5. Operator precedence

Identical to ECMAScript. Listed for reference; two entries are rejected.

| Level | Operators | Assoc |
|---|---|---|
| 1 | `()` grouping | — |
| 2 | `.` `?.` `[]` call `new X(…)` | left |
| 3 | `new X` (no args) | right |
| 4 | `++` `--` (postfix) | — |
| 5 | `!` `~` `+` `-` `typeof` `await` | right |
| 6 | `**` | right |
| 7 | `*` `/` `%` | left |
| 8 | `+` `-` | left |
| 9 | `<<` `>>` `>>>` | left |
| 10 | `<` `<=` `>` `>=` `in` `instanceof` | left |
| 11 | `===` `!==` ~~`==`~~ ~~`!=`~~ | left |
| 12 | `&` | left |
| 13 | `^` | left |
| 14 | `\|` | left |
| 15 | `&&` | left |
| 16 | `\|\|` | left |
| 17 | `??` | left |
| 18 | `?:` | right |
| 19 | `=` `+=` `-=` `*=` `/=` `%=` `**=` `<<=` `>>=` `>>>=` `&=` `^=` `\|=` `&&=` `\|\|=` `??=` | right |
| 20 | ~~`,`~~ | — |

Inherited constraints: `??` may not mix with `&&`/`||` without parentheses;
`**` may not have a unary left operand; `in` is restricted in `for` heads.

---

## 6. Declarations

### Accepted

```ts
let x = 1;
const y = 2;
function f(a: i32, b: string): f64 { … }
async function g(): Promise<void> { … }
class C extends B implements I { … }
interface I { a: i32; b?: string }
type Alias = i32 | null;
enum E { A, B = 5 }
import { x } from "./mod";
import type { T } from "./types";
export function h() {}
export default class {}
```

### Rejected

| Construct | Code | Replacement |
|---|---|---|
| `namespace N {}` / `module N {}` | TSN1401 | ES modules |
| `declare …` | TSN1402 | ship real declarations |
| `abstract class` | TSN1403 | `interface` + concrete class |
| `declare global` | TSN1404 | — |
| `export =` / `import =` | TSN1405 | ESM syntax |
| `require(…)` | TSN1406 | `import` |
| `module.exports` | TSN1407 | `export` |
| `__dirname` / `__filename` | TSN1408 | `import.meta.url` |
| decorators (any) | TSN1409 | pragmas or builtins |
| `accessor` fields | TSN1410 | plain fields |
| `using` declarations | TSN1411 | explicit cleanup |

### `enum`

Numeric and string enums are accepted. They lower to `u32` with explicit
discriminants (TYPES §4).

- Members without an initializer auto-increment from 0.
- `const enum` is accepted and inlined.
- Heterogeneous enums (mixing numeric and string) are **TSN1412**.
- Reverse mappings (`E[0] === "A"`) are not generated.

### Class member restrictions

| Member | Status |
|---|---|
| fields with type annotations | accepted |
| `private` / `protected` / `public` | accepted (compile-time only) |
| `readonly` | accepted (compile-time only) |
| `static` | accepted |
| `#private` | TSN1310 |
| getters / setters | TSN1312 |
| `abstract` members | TSN1403 |
| `override` | accepted (checked) |
| constructor parameter properties | accepted |
| index signatures | accepted → map |

---

## 7. Type syntax

### Accepted

```ts
i8 i16 i32 i64 u8 u16 u32 u64 f32 f64
bool char void null never unknown
string T[] [A, B] Record<K, V> Map<K, V> Set<T>
T | null
T & U                       // intersections of object types only
interface / type aliases
generics: Foo<T>, constraints `T extends U`
literal types: 42, "x", true
optional `?`, readonly modifiers
typeof x                    // in type position
as T                        // numeric casts and explicit widening only
satisfies T
!                           // non-null assertion (debug trap)
```

### Rejected

| Construct | Code | Why |
|---|---|---|
| `any` | TSN1501 | soundness |
| `@ts-ignore` / `@ts-expect-error` | TSN1502 | fix the type |
| conditional types `T extends U ? X : Y` | TSN1503 | checker complexity |
| mapped types `{ [K in T]: U }` | TSN1504 | checker complexity |
| template literal types `` `a${T}` `` | TSN1505 | checker complexity |
| `keyof T` | TSN1506 | not needed in v0 |
| `infer U` | TSN1507 | follows from 1503 |
| `T extends U ? … : …` (infer) | TSN1503 | — |
| `this` type | TSN1508 | — |
| variance annotations `in`/`out` | TSN1509 | — |
| `const` type parameters | TSN1510 | — |
| definite assignment `x!: T` | TSN1511 | defeats definite-assignment analysis |
| `as unknown as T` | TSN1512 | double cast is a lie |
| `declare` in type position | TSN1402 | — |
| index access types `T["k"]` | TSN1513 | not needed in v0 |
| `unique symbol` | TSN1514 | symbols deferred |
| `asserts x is T` | TSN1515 | — |

`as` is allowed only for:

1. numeric width changes (`i64 as i32`), with a debug trap if lossy,
2. widening to a supertype,
3. `T` → `T | null`.

Everything else is TSN1512.

---

## 8. Module specifiers and pragmas

### Specifiers

**Every import must be explicitly namespaced.** A bare specifier is TSN1601.

| Form | Meaning |
|---|---|
| `"./x"`, `"../x"` | local module |
| `"tsn:net"`, `"tsn:http"`, `"tsn:json"`, `"tsn:raw"`, … | stdlib |
| `"npm:foo"`, `"npm:@scope/foo"` | host import (leaf) |
| `"foo"` | **TSN1601** — ambiguous, forbidden |

This is deliberate: it makes the dependency graph readable at a glance and
prevents the "is this local or from node_modules?" ambiguity that plagues ESM.

### Pragmas

File-level, must appear before the first statement, one per line:

```ts
// tsn:allow-host-calls
// tsn:no-monomorphize
```

| Pragma | Effect |
|---|---|
| `tsn:allow-host-calls` | permits `npm:` imports to be called outside module boundaries (ABI §10). Emits a build-report entry. |
| `tsn:no-monomorphize` | (deferred) single boxed instantiation per generic |

Unknown pragmas are **TSN1602** — a typo'd pragma that silently does nothing is
exactly the kind of quiet failure this language is trying to avoid.

---

## 9. Policy pass — implementation

```
source ──> oxc parse ──> AST ──> tsn-frontend::policy ──> Diagnostics
                                        │
                                        └──> accepted AST ──> HIR
```

- `policy` is a pure visitor over `oxc_ast`. No I/O, no globals.
- It tracks scope, so it can answer "is this `this` inside a `function`?"
- It reads leading comments once per file for pragmas.
- Every rejection carries a stable code, a span, and a one-line replacement hint.
- The pass is table-driven: one entry per code, one test per entry.

Diagnostics render through `miette` with source spans and the replacement
suggestion inline:

```
error[TSN1301]: '==' is not supported
  --> src/handler.ts:12:9
   |
12 |   if (a == b) {
   |         ^^ help: use '===' instead
```

---

## 10. Testing

| Test | Method |
|---|---|
| Each rejection | one `.ts` fixture per code, asserts the exact code and span |
| Positive corpus | real TS programs that must parse and pass policy |
| ASI torture | every example from the spec's ASI section |
| Unicode | identifiers with escapes, ZWJ, astral plane |
| Numeric | all literal forms, separators, bigint |
| Differential | parse with oxc, run policy, assert no false positives on a large TS corpus |
| Fuzz | `cargo-fuzz` on the policy pass (must never panic) |

**False-positive rate is the key metric.** A policy that rejects valid programs
is worse than no policy. Run it against the TypeScript compiler's own source
tree and count rejections — every one should be a deliberate, documented
divergence.

---

## 11. Grammar evolution policy

- **Additive only** after v1.0. New builtins, new type names, new pragmas.
- Removing a rejection is a **minor** change (more programs compile).
- Adding a rejection is a **major** change (programs stop compiling).
- Adding syntax is **never** allowed. If it cannot be parsed by stock oxc, it
  is not a tsn feature.

---

## 12. Open questions

1. **Regex.** Parse-accept/reject is a clean v0 story, but the day `tsn:regex`
   lands, `/…/` becomes a literal whose flags and semantics must match ES. The
   engine choice (RE2 vs backtracking) changes behavior on catastrophic
   patterns. Decide before v0.2.
2. **`for…in`.** Currently warn. Should it be an error? It has no sane meaning
   over fixed-layout objects.
3. **`switch` default.** Requiring `default` is opinionated. Measure how often
   it is genuinely redundant before freezing.
4. **`satisfies`.** Accepted, but its interaction with inference and with
   `as`-restriction needs a precise rule.
5. **Pragma placement.** Before the first statement, or before the first
   *import*? The latter is friendlier but weirder.
6. **`char` literals.** `'a'` is a `string`. A real `char` needs `'a' as char`
   or a builtin. Not urgent, but the ergonomics are bad.
