<p align="center">
  <img src="./assets/almide.png" alt="Almide" width="280">
</p>

<p align="center">A programming language designed for LLM code generation — optimized for AI proliferation.</p>

Almide is not a language for humans to write freely — it is a language for AI to converge correctly. Every design decision minimizes the set of valid next tokens at each generation step, reducing hallucination and syntax errors.

The core thesis: **if AI can write a language reliably, code proliferates → training data grows → AI writes it even better → modules multiply**. Almide is designed to start this flywheel.

## Key Design Principles

- **Predictable** — At each point in code generation, the set of valid continuations is small
- **Local** — Understanding any piece of code requires only nearby context
- **Repairable** — Compiler diagnostics guide toward a unique fix
- **Compact** — High semantic density, low syntactic noise

## What Almide Eliminates

Most design decisions are **subtractive** — reducing the space of valid programs so the AI converges faster.

| Removed | Almide alternative | Why |
|---------|-------------------|-----|
| `null` / `nil` | `Option[T]` | Eliminates null-check hallucination |
| Exceptions / `throw` / `try-catch` | `Result[T, E]` | Error path is always visible in types |
| `<>` generics | `[]` generics | No ambiguity with comparison operators |
| `while` / `for` / `loop` | `do { guard ... else ... }` | Single loop construct, break condition is explicit |
| `return` | Last expression is the value | No early-return confusion |
| Multiple lambda forms | `fn(x) => expr` only | One syntax, no alternatives |
| Implicit side effects | `effect fn` | Callability is restricted by effect, narrowing completions |
| Semicolons | Newline-separated | No semicolon insertion ambiguity |
| Operator overloading | None | Operators have fixed meaning |
| Implicit conversions | None | Every conversion is explicit |
| `if` without `else` | `guard ... else` | Forces exhaustive handling, eliminates dangling-else |

The compiler actively **rejects** common patterns from other languages (`while`, `return`, `print`, `class`, `null`) with targeted hints directing toward the Almide equivalent.

## Features

- `Result[T, E]` / `Option[T]` — No exceptions, no null
- `effect fn` — Side effects narrow the set of callable functions at each point
- `[]` for generics — No `<>` ambiguity with comparison operators
- `do` blocks — Automatic error propagation and loop construct
- UFCS — `f(x, y)` and `x.f(y)` are equivalent; canonical form is function style
- `guard ... else` — Flat early returns instead of nested if-else
- `_` holes and `todo()` — Type-checked incomplete code
- Single lambda syntax — `fn(x) => expr`, no alternatives

## Quick Example

```
module app

import fs
import string
import list

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

## Compiler Diagnostics

The compiler rejects common patterns from other languages with targeted hints, reducing AI fix-loops:

```
'!' is not valid in Almide
  Hint: Use 'not x' for boolean negation, not '!x'.

'while' is not valid in Almide
  Hint: Use 'do { guard condition else break_expr }' for loops.

'return' is not valid in Almide
  Hint: Use the last expression as the return value, or 'guard ... else' for early exit.
```

## Benchmark

Tested with the [MiniGit benchmark](https://github.com/mizchi/ai-coding-lang-bench) — a task where Claude Code implements a mini version control system from a spec, with zero prior knowledge of the language.

| Trial | Time | Turns | Tests | LOC |
|-------|------|-------|-------|-----|
| 1 | 187s | 7 | 11/11 | 118 |
| 2 | 115s | 11 | 11/11 | 129 |
| 3 | 131s | 6 | 11/11 | 113 |
| 4 | 139s | 7 | 11/11 | 124 |
| 5 | 135s | 6 | 11/11 | 112 |

**Pass rate: 5/5 (100%)** — The AI converges to correct code every time, given only a ~60-line EBNF grammar reference.

### Proliferation Potential

The 100% pass rate across all trials demonstrates Almide's core value proposition: **reliability breeds proliferation**.

- AI generates correct Almide code consistently → generated code becomes training data
- Constrained syntax means generated code is uniform → training signal is clean
- New modules can be AI-generated with the same reliability → ecosystem grows organically
- Fewer syntax choices = faster convergence = lower cost per generation

The generation time gap vs established languages (Python ~40s, Almide ~140s) reflects zero training data, not language quality. Each successful generation adds to the corpus, narrowing this gap over time.

## License

MIT
