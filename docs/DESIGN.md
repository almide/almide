# Design Philosophy

Almide optimizes for **minimal thinking tokens**: the less an LLM has to branch over syntax, semantics, repair strategies, or missing abstractions, the faster, cheaper, and more reliable code generation becomes. This means reducing branching during generation, completion, and repair, while providing the right tools so the AI does not need to improvise around missing abstractions.

## Surface Ambiguity Removed

| Ambiguity source | Other languages | Almide | Token branching impact |
|---|---|---|---|
| Null handling | `null`, `nil`, `None`, `undefined` | `Option[T]` only | Eliminates null-check hallucination |
| Error handling | `throw`, `try/catch`, `panic`, error codes | `Result[T, E]` only | Error path always visible in types |
| Generics | `<T>` (ambiguous with `<` `>`) | `[T]` | No parser ambiguity with comparisons |
| Loops | `while`, `for`, `loop`, `forEach`, recursion | `for x in xs { }` for collection iteration, `while cond { ... }` for condition-based loops, `do { guard ... }` for structured break with values | Each form has one purpose |
| Early exit | `return`, `break`, `continue`, `throw` | Last expression only; `guard ... else` is the canonical structured escape hatch | No early-return confusion |
| Lambdas | `=>`, `->`, `lambda`, `fn`, `\x ->`, blocks | `(x) => expr` only | One syntax, zero alternatives |
| Statement termination | `;`, optional `;`, ASI rules | Newline-separated | No insertion ambiguity |
| Conditionals | `if` with optional `else`, ternary `?:` | `if/then/else` (else mandatory) | No dangling-else |
| Side effects | Implicit anywhere | `effect fn` annotation required | Restricts callable set at each point |
| Operator meaning | Overloading, implicit coercion | Operators have fixed built-in meanings only. No user-defined overloading, no implicit coercion | Operators always resolve identically |
| Type conversions | Implicit widening, coercion | Explicit only | No hidden type changes |

## Semantic Ambiguity Removed

| Ambiguity source | What Almide does | Why it matters for LLMs |
|---|---|---|
| Name resolution | Core modules (`int`, `string`, `list`, `map`, `env`) are auto-imported; only `fs` requires explicit `import` | LLM never guesses at available names; core operations always work |
| Type inference | Local only — annotations required on function signatures | No inference across distant definitions |
| Overloading | None — names do not participate in ad-hoc overload resolution | No dispatch ambiguity |
| Implicit conversions | None — `int.to_string(n)`, never auto-coerce | Every conversion visible in source |
| Trait/interface lookup | Traits exist but all `impl` is explicit — no implicit instance resolution | No global instance search |
| Method resolution | Canonical resolution is module-qualified function form; UFCS is parse-time sugar for chaining | Resolution is always local — no method lookup tables |
| Declaration order | Functions can reference each other freely | No forward-declaration confusion |
| Import style | `import module` or `import self as alias` — one form per purpose; aliases only for self-imports and submodules; core modules are auto-imported | Two import forms, no `from`, no `*` |

## The `effect` System as Generation Space Reducer

`effect fn` is not primarily a safety feature — it is a **search space reducer for code generation**.

- A pure function can only call other pure functions → the set of valid completions shrinks dramatically
- An `effect fn` explicitly marks I/O boundaries → the LLM knows exactly where side effects are legal
- Effect mismatch is caught at compile time → wrong calls are rejected before execution
- Function signatures alone tell the LLM what is callable at each point, without reading function bodies

This means the LLM can generate code by looking only at the current function's signature and its imports — no global analysis required.

## Concurrency: `fan` — Boring on Purpose

Almide keeps concurrency boring on purpose: explicit fork, explicit join, automatic cancellation, and the same fail-fast semantics as `do`. There is no `async`/`await` — `effect fn` is the async boundary, and the compiler handles the rest.

One keyword, three forms:

- **`fan { a(); b() }`** — run expressions concurrently, wait for all, return results as a tuple
- **`fan.map(xs, fn)`** — parallel map over a collection, return results as a list
- **`fan.race(thunks)`** — run thunks concurrently, return the first to complete, cancel the rest

The rules are minimal:

- `fan { }` is only valid inside `effect fn` — pure functions cannot fork
- Result auto-unwrap: if any expression returns `Err`, the entire `fan` fails and siblings are cancelled
- No `var` capture inside `fan` — only `let` bindings from outer scope are readable (prevents data races)
- No unstructured `spawn` — all concurrency is scoped

This mirrors `do` exactly: `do` exits on the first `Result` error; `fan` exits on the first failed task. Sequential and concurrent code follow the same fail-fast rule.

## UFCS: Why Two Forms is Acceptable

`f(x, y)` and `x.f(y)` are equivalent, which superficially adds a synonym. We accept this because:

- **Canonical resolution is module-qualified function form**: `module.fn(args)` — the module prefix makes resolution unambiguous
- **Surface syntax may omit the module** for auto-imported core modules (e.g., `len(s)` instead of `string.len(s)`)
- **Method form is syntactic sugar for chaining only**: `x.f(y).g(z)` reads left-to-right
- The compiler does not need method lookup — it rewrites `x.f(y)` to `f(x, y)` at parse time
- A future formatter will normalize to canonical form, eliminating style drift

## Iteration: `for...in` + `while` + `do { guard }`

Three loop constructs, each with a clear purpose:

- **`for x in xs { ... }`** — iterate over a collection. The natural choice for lists and map keys. Effect-compatible (I/O inside the loop body is fine).
- **`while cond { ... }`** — condition-based loop. Runs while the condition is true. The straightforward choice when you have a simple loop condition.
- **`do { guard ... else ... }`** — loop with structured break and return values (e.g., linked-list traversal, reading until EOF). `guard condition else break_expr` is the only way to exit, and the `else` branch produces the loop's value.

In our tests, forcing all iteration through `do { guard }` consistently caused extra index-management boilerplate. `for...in` and `while` eliminate this entirely.

## Compiler Diagnostics: Single Likely Fix

Almide's diagnostics are designed so that **each error points to exactly one repair**. This is critical for LLM fix-loops:

- Rejected syntax (`!`, `return`, `class`, `null`) includes a hint naming the exact Almide equivalent
- Expected tokens at each parse position are kept to a small, enumerable set
- Parser recovery does not guess — it fails fast with a precise location and expectation
- `_` holes and `todo()` let LLMs generate incomplete but type-valid code, then fill incrementally

```
'!' is not valid in Almide at line 5:12
  Hint: Use 'not x' for boolean negation, not '!x'.

'return' is not valid in Almide at line 12:5
  Hint: Use the last expression as the return value, or 'guard ... else' for early exit.
```

## Stdlib Naming Conventions

The standard library follows strict naming rules to minimize LLM guessing:

| Convention | Rule | Example |
|---|---|---|
| Module prefix | Canonical form is `module.function()`; core modules may omit the prefix in surface syntax | `string.len(s)`, `list.get(xs, i)`, `map.get(m, k)` (core modules auto-imported) |
| Predicate prefix | `is_` for boolean-returning functions | `string.is_empty(s)`, `list.is_empty(xs)`, `map.contains(m, key)` |
| Return type consistency | Fallible lookups return `Option`, fallible I/O returns `Result`, infallible pure conversions return plain values | `list.get() -> Option[T]`, `fs.read_text() -> Result[String, FsError]` (effect fn) |
| No synonyms | One name per operation, no aliases | `len` not `length`/`size`/`count` |
| Symmetric pairs | Matching names for inverse operations | `read_text`/`write`, `split`/`join`, `to_string`/`to_int` |
| No semantic name drift | Same operation names are reused only when the semantics match across modules | `string.len` and `list.len` both mean "count elements", not different operations hidden behind the same name |

## What Almide Sacrifices

These are intentional trade-offs — things we gave up to make LLM generation reliable:

| Sacrificed | Why |
|---|---|
| Raw expressiveness | Each concept has one idiomatic way to write it. Almide provides the right abstraction (e.g., `map`, `for...in`) but not multiple ways to achieve the same thing. |
| Operator overloading | Operators have fixed built-in meanings only. No user-defined overloading, no implicit coercion. |
| Metaprogramming | No macros, no reflection, no code generation. The language surface is fixed. |
| Ad-hoc polymorphism | `protocol` defines required convention methods. Types declare satisfaction explicitly (`type Dog: Eq, Serializable`). Implementation via flat convention methods (`fn Dog.serialize(...)`), no `impl` blocks. Built-in conventions (Eq, Repr, Ord, Hash, Codec) are protocols. Generic bounds: `fn f[T: Protocol](x: T)` — monomorphized, no dynamic dispatch. No implicit instance resolution, no orphan rules. |
| Named/default arguments | Default arguments supported (`fn f(x: Int, y: Int = 0)`); named arguments for clarity (`f(x: 1, y: 2)`). No variadic arguments. |
| Multiple return styles | No `return` keyword. The last expression is always the value. No exceptions. |
| Syntax sugar variety | One way to write each construct. No shorthand forms, no alternative spellings. |
| DSL capabilities | No operator definition, no custom syntax. Almide code always looks like Almide. |

These are not missing features — they are **intentional constraints that keep the generation space focused**. The goal is not minimalism for its own sake, but ensuring each abstraction has one clear path.
