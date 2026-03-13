# Design Philosophy

Almide optimizes for **minimal thinking tokens**: the less an LLM has to branch over syntax, semantics, repair strategies, or missing abstractions, the faster, cheaper, and more reliable code generation becomes. This means both removing ambiguity *and* providing the right tools so the AI does not need to improvise around missing abstractions.

## Syntax Ambiguity Removed

| Ambiguity source | Other languages | Almide | Token branching impact |
|---|---|---|---|
| Null handling | `null`, `nil`, `None`, `undefined` | `Option[T]` only | Eliminates null-check hallucination |
| Error handling | `throw`, `try/catch`, `panic`, error codes | `Result[T, E]` only | Error path always visible in types |
| Generics | `<T>` (ambiguous with `<` `>`) | `[T]` | No parser ambiguity with comparisons |
| Loops | `while`, `for`, `loop`, `forEach`, recursion | `for x in xs { }` for collection iteration, `do { guard ... }` for condition-driven repetition | Each form has one purpose |
| Early exit | `return`, `break`, `continue`, `throw` | Last expression only; `guard ... else` is the canonical structured escape hatch | No early-return confusion |
| Lambdas | `=>`, `->`, `lambda`, `fn`, `\x ->`, blocks | `fn(x) => expr` only | One syntax, zero alternatives |
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
| Overloading | None — each function name has exactly one definition | No ad-hoc dispatch resolution |
| Implicit conversions | None — `int.to_string(n)`, never auto-coerce | Every conversion visible in source |
| Trait/interface lookup | No traits, no implicit instances | No global instance search |
| Method resolution | Canonical resolution is module-qualified function form; UFCS is parse-time sugar for chaining | Resolution is always local — no method lookup tables |
| Declaration order | Functions can reference each other freely | No forward-declaration confusion |
| Import style | `import module` only — no `from`, no `*`, no aliasing. Core modules (`int`, `string`, `list`, `map`, `env`) are auto-imported; only `fs` needs explicit import | One import form, zero variation |

## The `effect` System as Generation Space Reducer

`effect fn` is not primarily a safety feature — it is a **search space reducer for code generation**.

- A pure function can only call other pure functions → the set of valid completions shrinks dramatically
- An `effect fn` explicitly marks I/O boundaries → the LLM knows exactly where side effects are legal
- Effect mismatch is caught at compile time → wrong calls are rejected before execution
- Function signatures alone tell the LLM what is callable at each point, without reading function bodies

This means the LLM can generate code by looking only at the current function's signature and its imports — no global analysis required.

## UFCS: Why Two Forms is Acceptable

`f(x, y)` and `x.f(y)` are equivalent, which superficially adds a synonym. We accept this because:

- **Canonical resolution is module-qualified function form**: `module.fn(args)` — the module prefix makes resolution unambiguous
- **Surface syntax may omit the module** for auto-imported core modules (e.g., `len(s)` instead of `string.len(s)`)
- **Method form is syntactic sugar for chaining only**: `x.f(y).g(z)` reads left-to-right
- The compiler does not need method lookup — it rewrites `x.f(y)` to `f(x, y)` at parse time
- A future formatter will normalize to canonical form, eliminating style drift

## Iteration: `for...in` + `do { guard }`

Two loop constructs, each with a clear purpose:

- **`for x in xs { ... }`** — iterate over a collection. The natural choice for lists and map keys. Effect-compatible (I/O inside the loop body is fine).
- **`do { guard ... else ... }`** — loop with dynamic break conditions (e.g., linked-list traversal, reading until EOF). `guard condition else break_expr` is the only way to exit.

Benchmark data showed that forcing all iteration through `do { guard }` caused LLMs to write 5-8 extra lines of index management boilerplate. `for...in` eliminates this entirely.

## Compiler Diagnostics: Single Likely Fix

Almide's diagnostics are designed so that **each error points to exactly one repair**. This is critical for LLM fix-loops:

- Rejected syntax (`!`, `while`, `return`, `class`, `null`) includes a hint naming the exact Almide equivalent
- Expected tokens at each parse position are kept to a small, enumerable set
- Parser recovery does not guess — it fails fast with a precise location and expectation
- `_` holes and `todo()` let LLMs generate incomplete but type-valid code, then fill incrementally

```
'!' is not valid in Almide at line 5:12
  Hint: Use 'not x' for boolean negation, not '!x'.

'while' is not valid in Almide at line 8:3
  Hint: Use 'do { guard condition else break_expr }' for loops.

'return' is not valid in Almide at line 12:5
  Hint: Use the last expression as the return value, or 'guard ... else' for early exit.
```

## Stdlib Naming Conventions

The standard library follows strict naming rules to minimize LLM guessing:

| Convention | Rule | Example |
|---|---|---|
| Module prefix | Canonical form is `module.function()`; core modules may omit the prefix in surface syntax | `string.len(s)`, `list.get(xs, i)`, `map.get(m, k)` (core modules auto-imported) |
| Predicate suffix | `?` for boolean-returning functions | `fs.exists?(path)`, `string.contains?(s, sub)` |
| Return type consistency | Fallible lookups return `Option`, fallible I/O returns `Result`, infallible pure conversions return plain values | `list.get() -> Option[T]`, `fs.read_text() -> Result[String, FsError]` (effect fn) |
| No synonyms | One name per operation, no aliases | `len` not `length`/`size`/`count` |
| Symmetric pairs | Matching names for inverse operations | `read_text`/`write`, `split`/`join`, `to_string`/`to_int` |
| No method overloading | Same operation names are reused only when the semantics match across modules | `string.len` and `list.len` both mean "count elements" |

## What Almide Sacrifices

These are intentional trade-offs — things we gave up to make LLM generation reliable:

| Sacrificed | Why |
|---|---|
| Raw expressiveness | Each concept has one idiomatic way to write it. Almide provides the right abstraction (e.g., `map`, `for...in`) but not multiple ways to achieve the same thing. |
| Operator overloading | Operators have fixed built-in meanings only. No user-defined overloading, no implicit coercion. |
| Metaprogramming | No macros, no reflection, no code generation. The language surface is fixed. |
| Ad-hoc polymorphism | No traits, no typeclasses, and no ad-hoc polymorphism. Parametric generics exist, but constraints are structural (`T: { field: Type, .. }`), not resolved through global instances. |
| Named/default arguments | All arguments are positional. No optionality, no reordering. |
| Multiple return styles | No `return` keyword. The last expression is always the value. No exceptions. |
| Syntax sugar variety | One way to write each construct. No shorthand forms, no alternative spellings. |
| DSL capabilities | No operator definition, no custom syntax. Almide code always looks like Almide. |

These are not missing features — they are **intentional constraints that keep the generation space focused**. The goal is not minimalism for its own sake, but ensuring each abstraction has one clear path.
