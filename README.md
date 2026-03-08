<p align="center">
  <img src="./assets/almide.svg" alt="Almide" width="200">
</p>

<h1 align="center">Almide</h1>

<p align="center">A programming language designed for LLM code generation.</p>

<p align="center">
  <a href="https://almide.github.io/playground/">Playground</a> ·
  <a href="./docs/SPEC.md">Specification</a> ·
  <a href="./docs/GRAMMAR.md">Grammar</a> ·
  <a href="./docs/CHEATSHEET.md">Cheatsheet</a> ·
  <a href="./docs/DESIGN.md">Design Philosophy</a>
</p>

<p align="center">
  <a href="https://github.com/almide/almide/actions/workflows/ci.yml"><img src="https://github.com/almide/almide/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
</p>

## What is Almide?

Almide is a language for AI to write code directly and efficiently. The design goal is that LLMs can express intent in the fewest tokens and with the least thinking, by providing the right abstractions and eliminating boilerplate.

The core thesis: **if AI can write a language reliably, code proliferates → training data grows → AI writes it even better → modules multiply**. Almide is designed to start this flywheel.

## Quick Start

**[Try it in your browser →](https://almide.github.io/playground/)** — No installation required.

### Install from source

```bash
git clone https://github.com/almide/almide.git
cd almide
cargo build --release
cp target/release/almide ~/.local/bin/
```

### Hello World

```
effect fn main(args: List[String]) -> Result[Unit, String] = {
  println("Hello, world!")
  ok(())
}
```

```bash
almide run hello.almd
```

## Why Almide?

- **Direct** — The right abstraction for each task, so AI writes intent, not workarounds
- **Predictable** — One unambiguous way to express each concept, reducing token branching
- **Local** — Understanding any piece of code requires only nearby context
- **Repairable** — Compiler diagnostics guide toward a unique fix, not multiple possibilities
- **Compact** — High semantic density, low syntactic noise

For the full design rationale, see [Design Philosophy](./docs/DESIGN.md).

## Example

```
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

## How It Works

Almide source (`.almd`) is compiled by a pure-Rust compiler to Rust, TypeScript, or WebAssembly.

```
.almd → Lexer → Parser → AST → CodeGen → .rs / .ts / .wasm
```

```bash
almide run app.almd              # Compile + execute
almide run app.almd -- arg1      # With arguments
almide build app.almd -o app     # Build standalone binary
almide build app.almd --target wasm  # Build WebAssembly (WASI)
almide test                      # Run tests/ directory
almide check app.almd            # Type check only (no compilation)
almide fmt app.almd              # Format source code
almide clean                     # Clear dependency cache
almide app.almd --target rust    # Emit Rust source
almide app.almd --target ts      # Emit TypeScript source
```

## Benchmark

<p align="center">
  <img src="./assets/benchmark.png" alt="MiniGit Benchmark: Almide vs 15 languages" width="720">
</p>

Tested with the [MiniGit benchmark](https://github.com/almide/benchmark) — Claude Code implements a mini version control system from a spec, with 10 trials per language.

| Language | Total Time | Avg Cost | Pass Rate |
|----------|-----------|----------|-----------|
| Ruby | 73.1s | $0.36 | 40/40 |
| Python | 74.6s | $0.38 | 40/40 |
| TypeScript | 133.0s | $0.62 | 40/40 |
| Rust | 113.7s | $0.54 | 38/40 |
| **Almide** | **206.3s** | **$0.59** | **8/8** |

Almide's current generation speed gap reflects zero training data, not language quality. Each successful generation adds to the corpus, narrowing the gap over time. See [full results](https://github.com/almide/benchmark) for all 16 languages.

## Edge Performance

AI-generated Almide code compiles to native binaries — no runtime, no GC, no interpreter.

| Metric | Almide |
|--------|--------|
| Binary size (minigit CLI) | **635 KB** (stripped) |
| Runtime (100 ops) | **1.6s** |
| Dependencies | **0** (single static binary) |
| WASM target | `almide build app.almd --target wasm` |

Almide compiles to Rust, then to native machine code. The generated binaries are smaller and faster than Go, with no runtime overhead. For edge computing and WebAssembly, this means:

- **Sub-MB binaries** that deploy instantly to CDN edge nodes
- **Microsecond cold starts** — no interpreter initialization
- **Zero dependencies** — single binary, no package manager needed at runtime
- **WASM-native** — compiles to `wasm32-wasip1` without GC or runtime shims

## Documentation

- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — Compiler pipeline, module map, design decisions
- [docs/SPEC.md](./docs/SPEC.md) — Full language specification
- [docs/GRAMMAR.md](./docs/GRAMMAR.md) — EBNF grammar + stdlib reference
- [docs/CHEATSHEET.md](./docs/CHEATSHEET.md) — Quick reference for AI code generation
- [docs/DESIGN.md](./docs/DESIGN.md) — Design philosophy and trade-offs
- [docs/ROADMAP.md](./docs/ROADMAP.md) — Language evolution plans

## Contributing

Contributions are welcome! Please open an issue or pull request on [GitHub](https://github.com/almide/almide).

## License

[MIT](./LICENSE)
