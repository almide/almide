<p align="center">
  <img src="./assets/almide.png" alt="Almide" width="280">
</p>

<p align="center">A programming language designed for LLM code generation — optimized for AI proliferation.</p>

Almide is a language for AI to write code directly and efficiently. The design goal is that LLMs can express intent in the fewest tokens and with the least thinking, by providing the right abstractions and eliminating boilerplate.

The core thesis: **if AI can write a language reliably, code proliferates → training data grows → AI writes it even better → modules multiply**. Almide is designed to start this flywheel.

## Key Design Principles

- **Direct** — The right abstraction for each task, so AI writes intent, not workarounds
- **Predictable** — One unambiguous way to express each concept, reducing token branching
- **Local** — Understanding any piece of code requires only nearby context
- **Repairable** — Compiler diagnostics guide toward a unique fix, not multiple possibilities
- **Compact** — High semantic density, low syntactic noise

## Design Philosophy

Almide optimizes for **minimal thinking tokens** — the less an LLM has to reason about workarounds, syntax alternatives, or missing abstractions, the faster and cheaper code generation becomes. This means both removing ambiguity *and* providing the right tools so the AI never has to improvise.

### Syntax Ambiguity Removed

| Ambiguity source | Other languages | Almide | Token branching impact |
|---|---|---|---|
| Null handling | `null`, `nil`, `None`, `undefined` | `Option[T]` only | Eliminates null-check hallucination |
| Error handling | `throw`, `try/catch`, `panic`, error codes | `Result[T, E]` only | Error path always visible in types |
| Generics | `<T>` (ambiguous with `<` `>`) | `[T]` | No parser ambiguity with comparisons |
| Loops | `while`, `for`, `loop`, `forEach`, recursion | `for x in xs { }` + `do { guard ... }` | Iteration for collections, guard for dynamic conditions |
| Early exit | `return`, `break`, `continue`, `throw` | Last expression + `guard ... else` | No early-return confusion |
| Lambdas | `=>`, `->`, `lambda`, `fn`, `\x ->`, blocks | `fn(x) => expr` only | One syntax, zero alternatives |
| Statement termination | `;`, optional `;`, ASI rules | Newline-separated | No insertion ambiguity |
| Conditionals | `if` with optional `else`, ternary `?:` | `if/then/else` (else mandatory) | No dangling-else |
| Side effects | Implicit anywhere | `effect fn` annotation required | Restricts callable set at each point |
| Operator meaning | Overloading, implicit coercion | Fixed meaning, no overloading | Operators always resolve identically |
| Type conversions | Implicit widening, coercion | Explicit only | No hidden type changes |

### Semantic Ambiguity Removed

| Ambiguity source | What Almide does | Why it matters for LLMs |
|---|---|---|
| Name resolution | Core modules (`int`, `string`, `list`, `map`, `env`) are auto-imported; only `fs` requires explicit `import` | LLM never guesses at available names; core operations always work |
| Type inference | Local only — annotations required on function signatures | No inference across distant definitions |
| Overloading | None — each function name has exactly one definition | No ad-hoc dispatch resolution |
| Implicit conversions | None — `int.to_string(n)`, never auto-coerce | Every conversion visible in source |
| Trait/interface lookup | No traits, no implicit instances | No global instance search |
| Method resolution | UFCS with canonical function form (`module.fn(args)`) | Module prefix makes resolution local |
| Declaration order | Functions can reference each other freely | No forward-declaration confusion |
| Import style | `import module` only — no `from`, no `*`, no aliasing. Core modules (`int`, `string`, `list`, `map`, `env`) are auto-imported; only `fs` needs explicit import | One import form, zero variation |

### The `effect` System as Generation Space Reducer

`effect fn` is not primarily a safety feature — it is a **search space reducer for code generation**.

- A pure function can only call other pure functions → the set of valid completions shrinks dramatically
- An `effect fn` explicitly marks I/O boundaries → the LLM knows exactly where side effects are legal
- Effect mismatch is caught at compile time → wrong calls are rejected before execution
- Function signatures alone tell the LLM what is callable at each point, without reading function bodies

This means the LLM can generate code by looking only at the current function's signature and its imports — no global analysis required.

### UFCS: Why Two Forms is Acceptable

`f(x, y)` and `x.f(y)` are equivalent, which superficially adds a synonym. We accept this because:

- **Canonical form is function style**: `module.fn(args)` — the module prefix makes resolution unambiguous
- **Method form is syntactic sugar for chaining only**: `x.f(y).g(z)` reads left-to-right
- The compiler does not need method lookup — it rewrites `x.f(y)` to `f(x, y)` at parse time
- A future formatter will normalize to canonical form, eliminating style drift

### Iteration: `for...in` + `do { guard }`

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
| Module prefix | Always explicit: `module.function()` | `string.len(s)`, `list.get(xs, i)`, `map.get(m, k)` (core modules auto-imported) |
| Predicate suffix | `?` for boolean-returning functions | `fs.exists?(path)`, `string.contains?(s, sub)` |
| Return type consistency | Fallible lookups return `Option`, I/O returns `Result` | `list.get() -> Option`, `fs.read_text() -> String` (effect fn) |
| No synonyms | One name per operation, no aliases | `len` not `length`/`size`/`count` |
| Symmetric pairs | Matching names for inverse operations | `read_text`/`write`, `split`/`join`, `to_string`/`to_int` |
| No method overloading | Same name never appears in two modules with different semantics | `string.len` and `list.len` both mean "count elements" |

## What Almide Sacrifices

These are intentional trade-offs — things we gave up to make LLM generation reliable:

| Sacrificed | Why |
|---|---|
| Raw expressiveness | Each concept has one idiomatic way to write it. Almide provides the right abstraction (e.g., `map`, `for...in`) but not multiple ways to achieve the same thing. |
| Operator overloading | `+` always means integer addition or is not valid. No custom operators. |
| Metaprogramming | No macros, no reflection, no code generation. The language surface is fixed. |
| Ad-hoc polymorphism | No traits, no typeclasses. Functions are monomorphic. Generics are limited to built-in containers. |
| Named/default arguments | All arguments are positional. No optionality, no reordering. |
| Multiple return styles | No `return` keyword. The last expression is always the value. No exceptions. |
| Syntax sugar variety | One way to write each construct. No shorthand forms, no alternative spellings. |
| DSL capabilities | No operator definition, no custom syntax. Almide code always looks like Almide. |

These are not missing features — they are **intentional constraints that keep the generation space focused**. The goal is not minimalism for its own sake, but ensuring each abstraction has one clear path.

## Quick Example

```
module app

import fs

type AppError =
  | NotFound(String)
  | Io(IoError)
  deriving From

effect fn greet(name: String) -> Result[Unit, AppError] = {
  guard string.len(name) > 0 else err(NotFound("empty name"))
  println("Hello, ${name}!")
  ok(())
}

effect fn main(args: List[String]) -> Result[Unit, AppError] = {
  let name = match list.get(args, 1) {
    some(n) => n,
    none => "world",
  }
  greet(name)
}

test "greet succeeds" {
  assert_eq(string.len("hello"), 5)
}
```

## File Extension

`.almd`

## Playground

Try Almide in your browser — no installation required:

**[almide.github.io/playground](https://almide.github.io/playground/)**

Write `.almd` code, compile to JavaScript via WebAssembly, and run it instantly. Includes AI code generation with Anthropic, OpenAI, and Gemini APIs.

## Documentation

- [docs/GRAMMAR.md](./docs/GRAMMAR.md) — EBNF grammar + stdlib reference (compact, for AI consumption)
- [CHEATSHEET.md](./CHEATSHEET.md) — Quick reference for AI code generation
- [SPEC.md](./SPEC.md) — Full language specification

## How It Works

Almide source (`.almd`) is compiled by a pure-Rust compiler to Rust or TypeScript, then executed natively.

```
.almd → Lexer → Parser → AST → CodeGen → .rs (Rust) or .ts (Deno)
```

### Usage

```bash
# Run directly (compile + execute in one step)
almide run app.almd

# Run with arguments
almide run app.almd -- arg1 arg2

# Build a standalone binary
almide build app.almd -o app

# Emit Rust source
almide app.almd --target rust

# Emit TypeScript source
almide app.almd --target ts
```

### Install

```bash
cargo build --release
cp target/release/almide ~/.local/bin/
```

## Benchmark

<p align="center">
  <img src="./assets/benchmark.png" alt="MiniGit Benchmark: Almide vs 15 languages" width="720">
</p>

Tested with the [MiniGit benchmark](https://github.com/almide/benchmark) — a task where Claude Code implements a mini version control system from a spec (v1: basic commands, v2: advanced features), with 10 trials per language.

### Results Summary (v1+v2 combined, 10 trials each)

| Language | Total Time | Avg Cost | v1 Tests | v2 Tests | Avg LOC |
|----------|-----------|----------|----------|----------|---------|
| Ruby | 73.1s±4.2s | $0.36 | 20/20 | 20/20 | 107+219 |
| Python | 74.6s±4.5s | $0.38 | 20/20 | 20/20 | 113+235 |
| JavaScript | 81.1s±5.0s | $0.39 | 20/20 | 20/20 | 123+248 |
| Go | 101.6s±37.0s | $0.50 | 20/20 | 20/20 | 143+324 |
| Rust | 113.7s±54.8s | $0.54 | 19/20 | 19/20 | 139+303 |
| Java | 115.4s±34.4s | $0.50 | 20/20 | 20/20 | 152+303 |
| Python/mypy | 125.3s±19.0s | $0.57 | 20/20 | 20/20 | 171+326 |
| OCaml | 128.1s±28.9s | $0.58 | 20/20 | 20/20 | 111+216 |
| Perl | 130.2s±44.2s | $0.55 | 20/20 | 20/20 | 173+315 |
| Scheme | 130.6s±39.9s | $0.60 | 20/20 | 20/20 | 171+310 |
| TypeScript | 133.0s±29.4s | $0.62 | 20/20 | 20/20 | 149+310 |
| Lua | 143.6s±43.0s | $0.58 | 20/20 | 20/20 | 226+398 |
| C | 155.8s±40.9s | $0.74 | 20/20 | 20/20 | 276+517 |
| Haskell | 174.0s±44.2s | $0.74 | 19/20 | 20/20 | 119+224 |
| Ruby/Steep | 186.6s±69.7s | $0.84 | 20/20 | 20/20 | 150+304 |
| **Almide** | **376.1s±9.4s** | **$0.89** | **6/6** | **6/6** | **115+293** |

### Key Findings

1. **Dynamic languages are fastest** — Ruby, Python, JavaScript lead in speed and cost, likely due to abundant training data and no compilation overhead
2. **Type systems add overhead** — Python/mypy vs Python (+68%), Ruby/Steep vs Ruby (+155%), TypeScript vs JavaScript (+64%) — the "type tax" is real
3. **All languages achieve near-perfect pass rates** — Most hit 20/20 on both v1 and v2 tests
4. **Cost correlates with time** — Faster languages use fewer tokens and cost less
5. **LOC varies 2-5x** — C needs ~5x more lines than Ruby for the same task

### Why This Matters for Almide

Almide targets the **TypeScript-to-Rust tier** (~100-150s) with key advantages:

- **Zero training data** — Unlike established languages with massive corpora, Almide has none. The time gap reflects data availability, not language quality
- **100% test pass rate** in earlier 5-trial benchmarks — Almide's constrained syntax eliminates the ambiguity that causes failures
- **Each successful generation adds to the corpus** — narrowing the gap over time
- **Constrained syntax = clean training signal** — Generated code is uniform, making future training more effective

## License

MIT
