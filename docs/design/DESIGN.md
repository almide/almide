# Design Philosophy

Almide optimizes for **minimal thinking tokens**: the less an LLM has to branch over syntax, semantics, repair strategies, or missing abstractions, the faster, cheaper, and more reliable code generation becomes. This means reducing branching during generation, completion, and repair, while providing the right tools so the AI does not need to improvise around missing abstractions.

## Surface Ambiguity Removed

| Ambiguity source | Other languages | Almide | Token branching impact |
|---|---|---|---|
| Null handling | `null`, `nil`, `None`, `undefined` | `Option[T]` only | Eliminates null-check hallucination |
| Error handling | `throw`, `try/catch`, `panic`, error codes | `Result[T, E]` only | Error path always visible in types |
| Generics | `<T>` (ambiguous with `<` `>`) | `[T]` | No parser ambiguity with comparisons |
| Loops | `while`, `for`, `loop`, `forEach`, recursion | `for x in xs { }` for collection iteration, `while cond { ... }` for condition-based loops | Each form has one purpose |
| Early exit | `return`, `break`, `continue`, `throw` | Last-expression function results; `guard ... else` for structured escape, `break`/`continue` inside loops | No early-return confusion |
| Lambdas | `=>`, `->`, `lambda`, `fn`, `\x ->`, blocks | `(x) => expr` only | One syntax, zero alternatives |
| Statement termination | `;`, optional `;`, ASI rules | Newline-separated | No insertion ambiguity |
| Conditionals | `if` with optional `else`, ternary `?:` | `if/then/else`; an omitted `else` yields Unit | No dangling-else |
| Side effects | Implicit anywhere | `effect fn` marks effect calls; output/abort builtins are also admissible in plain `fn` | Restricts callable set at each point |
| Operator meaning | Custom operator syntax, implicit coercion | Fixed operator vocabulary; declared protocol conformance can dispatch operators to requirement methods. No implicit coercion | Resolution follows the operand type and protocol requirements |
| Type conversions | Implicit widening, coercion | Explicit only | No hidden type changes |

## Semantic Ambiguity Removed

| Ambiguity source | What Almide does | Why it matters for LLMs |
|---|---|---|
| Name resolution | Core modules (`int`, `string`, `list`, `map`, `math`, `datetime`, ...) are auto-imported; effectful platform modules (`fs`, `env`, `io`, `json`, `random`, ...) require explicit `import` | LLM never guesses at available names; core operations always work |
| Type inference | Local only — annotations required on function signatures | No inference across distant definitions |
| Overloading | None — names do not participate in ad-hoc overload resolution | No dispatch ambiguity |
| Implicit conversions | None — `int.to_string(n)`, never auto-coerce | Every conversion visible in source |
| Protocol lookup | Types declare protocol satisfaction; convention methods (`fn Type.method`) provide implementations — no global implicit instance resolution | No global instance search |
| Method resolution | Canonical calls are module-qualified; receiver-aware UFCS is resolved by the checker | Resolution follows explicit receiver/scope rules |
| Declaration order | Functions can reference each other freely | No forward-declaration confusion |
| Import style | `import module` or `import self as alias` — one form per purpose; aliases only for self-imports and submodules; core modules are auto-imported | Two import forms, no `from`, no `*` |

## The `effect` System as Generation Space Reducer

`effect fn` is not primarily a safety feature — it is a **search space reducer for code generation**.

- A plain `fn` rejects direct effect calls; output/abort builtins and transparent callback parameters have the documented exceptions
- An `effect fn` explicitly marks I/O boundaries → the LLM knows exactly where side effects are legal
- Effect mismatch is caught at compile time → wrong calls are rejected before execution
- Function signatures alone tell the LLM what is callable at each point, without reading function bodies

This means the LLM can generate code by looking only at the current function's signature and its imports — no global analysis required.

## Concurrency: `fan` — Boring on Purpose

Almide uses explicit fork/join without exposing `async`/`await`. Ordinary
`fan` joins every sibling and reports the first error in source/list order,
not the first failure in wall-clock time.

Two common forms:

- **`fan { a(), b() }`** — run expressions as one scoped unit, wait for all, return results as a tuple in declaration order; the compiler and host choose the execution substrate
- **`fan.map(xs, f)`** — run every element and collect results in list order; parallel execution depends on the callback and target

The rules are minimal:

- `fan { }` is only valid inside `effect fn` — pure functions cannot fork
- If an arm returns `Err`, the block returns the first `Err` in arm order after joining all siblings
- No `var` capture inside `fan` — only `let` bindings from outer scope are readable (prevents data races)
- No unstructured `spawn` — all concurrency is scoped

Ordinary `fan` does not cancel siblings. Budget/deadline forms have separate
cooperative-cut rules; see [SPEC §13](../SPEC.md#13-concurrency-fan).

## UFCS: Why Two Forms is Acceptable

When UFCS resolution selects `f`, `x.f(y)` passes `x` as its first argument.
This is not an unconditional parse-time rewrite:

- **Canonical resolution is module-qualified function form**: `module.fn(args)` — the module prefix makes resolution unambiguous
- **Auto-import makes a module available**, not every function name globally available; use `string.len(s)` or receiver syntax such as `s.len()`
- **Method form is syntactic sugar for chaining only**: `x.f(y).g(z)` reads left-to-right
- The checker resolves convention methods, local/imported functions, and builtin-module methods using receiver type and scope (`calls_ufcs.rs`)
- A future formatter will normalize to canonical form, eliminating style drift

## Iteration: `for...in` + `while`

Two loop constructs, each with a clear purpose:

- **`for x in xs { ... }`** — iterate over a collection. The natural choice for lists and map keys. Effect-compatible (I/O inside the loop body is fine).
- **`while cond { ... }`** — condition-based loop. Runs while the condition is true. The straightforward choice when you have a simple loop condition.

## Compiler Diagnostics: Single Likely Fix

Almide's diagnostics are designed so that **each error points to exactly one repair**. This is critical for LLM fix-loops:

- Rejected syntax (prefix `!x`, `return`, `class`, `null`) includes a hint naming the exact Almide equivalent
- Expected tokens at each parse position are kept to a small, enumerable set
- Parser recovery does not guess — it fails fast with a precise location and expectation
- `_` holes and `todo()` mark incomplete code; they are not a guarantee that an unfinished program will compile or run successfully

```
Prefix '!x' is not boolean negation in Almide
  Hint: Use 'not x' for boolean negation, not '!x'.

'return' is not valid in Almide at line 12:5
  Hint: Use the last expression as the return value, or 'guard ... else' for early exit.
```

## Stdlib Naming Conventions

The standard library follows strict naming rules to minimize LLM guessing:

| Convention | Rule | Example |
|---|---|---|
| Module prefix | Canonical form is `module.function()`; receiver syntax may replace a qualified call after UFCS resolution | `string.len(s)`, `list.get(xs, i)`, `map.get(m, k)` (core modules auto-imported) |
| Predicate prefix | `is_` for boolean-returning functions | `string.is_empty(s)`, `list.is_empty(xs)`, `map.contains(m, key)` |
| Return type consistency | Fallible lookups return `Option`, fallible I/O returns `Result`, infallible pure conversions return plain values | `list.get() -> Option[T]`, `fs.read_text() -> Result[String, String]` (effect fn) |
| No synonyms | One name per operation, no aliases | `len` not `length`/`size`/`count` |
| Symmetric pairs | Matching names for inverse operations | `read_text`/`write_text`, `split`/`join`, `to_string`/`to_int` |
| No semantic name drift | Same operation names are reused only when the semantics match across modules | `string.len` and `list.len` both mean "count elements", not different operations hidden behind the same name |

## What Almide Sacrifices

These are intentional trade-offs — things we gave up to make LLM generation reliable:

| Sacrificed | Why |
|---|---|
| Raw expressiveness | Each concept has one idiomatic way to write it. Almide provides the right abstraction (e.g., `map`, `for...in`) but not multiple ways to achieve the same thing. |
| Custom operators | No custom operator syntax. Existing operators can dispatch through declared protocols (`Eq`, `Numeric`, ordering requirements); no implicit coercion. |
| Metaprogramming | No macros, no reflection, no code generation. The language surface is fixed. |
| Ad-hoc polymorphism | `protocol` defines required convention methods. Types declare satisfaction explicitly (`type Dog: Eq, Serializable`). Implementation via flat convention methods (`fn Dog.serialize(...)`), no `impl` blocks. Built-in conventions (Eq, Repr, Ord, Hash, Codec) are protocols. Generic bounds: `fn f[T: Protocol](x: T)` — monomorphized, no dynamic dispatch. No implicit instance resolution, no orphan rules. |
| Named/default arguments | Default arguments supported (`fn f(x: Int, y: Int = 0)`); named arguments for clarity (`f(x: 1, y: 2)`). No variadic arguments. |
| Multiple return styles | No `return` keyword. The last expression is always the value. No exceptions. |
| Syntax sugar variety | A small canonical surface with documented sugar: for example `T?`, `T!E`, UFCS, and record-field shorthand. Concrete grammar takes precedence over the design slogan. |
| DSL capabilities | No operator definition, no custom syntax. Almide code always looks like Almide. |

These are not missing features — they are **intentional constraints that keep the generation space focused**. The goal is not minimalism for its own sake, but ensuring each abstraction has one clear path.

## The MSR rule (how features are added / rejected)

> **Design rule**: syntax that is beautiful to humans but a trap for LLMs is not adopted.

Every language change is evaluated against its **MSR delta** — the change in retry-success rate on [almide-dojo](https://github.com/almide/almide-dojo) across `llama-3.3-70b`, `llama-3.1-8b`, and (when available) `Claude Sonnet 4.6`, measured at `T=0` over 3 runs.

A change is **rejected** if:
- It drops median MSR by ≥ 3 percentage points on any model, OR
- It introduces a second canonical form for something that already has one (fragments LLM training data), OR
- It improves human aesthetics at the cost of generation predictability.

A change is **accepted** if:
- It measurably raises MSR, **or**
- It eliminates a class of errors that retry-with-diagnostic cannot repair (the ceiling effect).

Release notes from `v0.14.5` onward include an **MSR delta table** so every release owns its impact. New feature proposals in `docs/roadmap/active/` must cite the failure pattern they address and the expected MSR shift.

## Beauty axis vs MSR axis

Almide does not compete on:
- Pure-FP aesthetics (Elm, Haskell are stronger here — and they sacrifice LLM writability for that beauty).
- Dependent types / linear types / first-class effects-as-types (research languages win here — at the cost of a writability cliff).
- DSL expressiveness, macros, metaprogramming.

Almide competes on exactly one thing: **an LLM sits down and writes correct code on the first try**. Every trade-off above, and every future one, is chosen with that axis as the arbiter.
