<p align="center">
  <img src="./almide.png" alt="Almide" width="280">
</p>

<p align="center">A programming language designed for LLM code generation.</p>

Almide is not a language for humans to write freely — it is a language for AI to converge correctly. Every design decision minimizes the set of valid next tokens at each generation step, reducing hallucination and syntax errors.

## Key Design Principles

- **Predictable** — At each point in code generation, the set of valid continuations is small
- **Local** — Understanding any piece of code requires only nearby context
- **Repairable** — Compiler diagnostics guide toward a unique fix
- **Compact** — High semantic density, low syntactic noise

## Features

- `Result[T, E]` / `Option[T]` — No exceptions, no null
- `effect fn` — Side effects are visible in function signatures
- `[]` for generics — No `<>` ambiguity with comparison operators
- `do` blocks — Automatic error propagation and loop construct
- UFCS — `f(x, y)` and `x.f(y)` are equivalent
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

- [CHEATSHEET.md](./CHEATSHEET.md) — Quick reference for AI code generation (~340 lines)
- [SPEC.md](./SPEC.md) — Full language specification

## How It Works

Almide source (`.almd`) is transpiled to TypeScript and runs on [Deno](https://deno.land/).

```
.almd → Lexer → Parser → AST → CodeGen → .ts (Deno)
```

### Usage

```bash
deno run --allow-read src/almide.ts input.almd > output.ts
deno run --allow-read --allow-write --allow-env output.ts
```

## Benchmark

Tested with the [MiniGit benchmark](https://github.com/mame/ai-coding-lang-bench) (11 tests, 10 trials).

An LLM with **zero prior knowledge** of Almide, given only the CHEATSHEET as reference, achieved:

| Metric | Result |
|--------|--------|
| Pass rate | **10/10 (100%)** |
| All tests passed | **11/11 per trial** |
| Avg LOC | ~170 |

For comparison, existing languages scored: Ruby 90%, TypeScript 80%, Go 50%.

## License

MIT
