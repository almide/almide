<p align="center">
  <img src="./docs/assets/almide.svg" alt="Almide" width="200">
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
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg" alt="License: MIT / Apache-2.0"></a>
  <a href="https://deepwiki.com/almide/almide"><img src="https://deepwiki.com/badge.svg" alt="Ask DeepWiki"></a>
</p>

## What is Almide?

Almide is a statically-typed language optimized for AI-generated code. It compiles to native binaries (via Rust) and WebAssembly.

The core metric is **modification survival rate** — how often code still compiles and passes tests after a series of AI-driven modifications. The language achieves this through unambiguous syntax, actionable compiler diagnostics, and a standard library that covers common patterns out of the box.

The flywheel: LLMs write Almide reliably → more code is produced → training data grows → LLMs write it better → the ecosystem expands.

### MSR Scorecard

Measured by [almide-dojo](https://github.com/almide/almide-dojo) across 30 tasks (basic / intermediate / advanced):

| Model | Pass Rate | 1-Shot Rate |
|---|---|---|
| Claude Sonnet 4.6 | **100%** (30/30) | 47% |
| Llama 3.3 70B | 61% (17/28) | 33% |

## Quick Start

**[Try it in your browser →](https://almide.github.io/playground/)** — No installation required.

### Install (macOS / Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/almide/almide/main/tools/install.sh | sh
```

### Install (Windows)

```powershell
irm https://raw.githubusercontent.com/almide/almide/main/tools/install.ps1 | iex
```

### Install from source

Requires [Rust](https://rustup.rs/) (stable, 1.89+):

```bash
git clone https://github.com/almide/almide.git
cd almide
cargo build --release
cp target/release/almide ~/.local/bin/
```

### Hello World

```almd
fn main() -> Unit = {
  println("Hello, world!")
}
```

```bash
almide run hello.almd
```

## Features

- **Multi-target** — Same source compiles to native binary (via Rust) or WebAssembly (direct emit)
- **Generics** — Functions (`fn id[T](x: T) -> T`), records, variant types, recursive variants with auto Box wrapping
- **Pattern matching** — Exhaustive match with variant destructuring
- **Effect functions** — `effect fn` for explicit error propagation (`Result` auto-wrapping)
- **Bidirectional type inference** — Type annotations flow into expressions (`let xs: List[Int] = []`)
- **Codec system** — `Type.decode(value)` / `Type.encode(value)` convention with auto-derive
- **Map literals** — `["key": value]` syntax with `m[key]` access and `for (k, v) in m` iteration
- **Fan concurrency** — `fan { a(); b() }`, `fan.map`, `fan.race`, `fan.any`, `fan.settle`
- **Top-level constants** — `let PI = 3.14` at module scope, compile-time evaluated
- **Pipeline operator** — `data |> transform |> output`
- **Module system** — Packages, sub-namespaces, visibility control, diamond dependency resolution
- **Standard library** — 430 functions across 23 modules (string, list, map, json, http, fs, etc.)
- **Built-in testing** — `test "name" { assert_eq(a, b) }` with `almide test`
- **Actionable diagnostics** — Every error includes file:line, context, and a concrete fix suggestion

## Why Almide?

- **Predictable** — One canonical way to express each concept, reducing token branching for LLMs
- **Local** — Understanding any piece of code requires only nearby context
- **Repairable** — Compiler diagnostics guide toward a specific fix, not multiple possibilities
- **Compact** — High semantic density, low syntactic noise

For the full design rationale, see [Design Philosophy](./docs/DESIGN.md).

## Example

```almd
let PI = 3.14159265358979323846
let SOLAR_MASS = 4.0 * PI * PI

type Tree[T] =
  | Leaf(T)
  | Node(Tree[T], Tree[T])

fn tree_sum(t: Tree[Int]) -> Int =
  match t {
    Leaf(v) => v
    Node(left, right) => tree_sum(left) + tree_sum(right)
  }

effect fn greet(name: String) -> Result[Unit, String] = {
  guard string.len(name) > 0 else err("empty name")
  println("Hello, ${name}!")
  ok(())
}

effect fn main() -> Result[Unit, String] = {
  greet("world")
}

test "greet succeeds" {
  assert_eq("hello".len(), 5)
}
```

## How It Works

Almide source (`.almd`) is compiled by a pure-Rust compiler through a three-layer codegen architecture:

```
.almd → Lexer → Parser → AST → Type Checker → Lowering → IR
                                                            ↓
                                              Nanopass Pipeline (semantic rewrites)
                                                            ↓
                                              Template Renderer (TOML-driven)
                                                            ↓
                                                    .rs / .wasm
```

The Nanopass pipeline applies target-specific transformations: `ResultPropagation` (Rust `?`), `CloneInsertion` (Rust borrow analysis), `LICM` (loop-invariant code motion). The Template Renderer is purely syntactic — all semantic decisions are already encoded in the IR.

```bash
almide run app.almd              # Compile + execute (Rust target)
almide run app.almd -- arg1      # With arguments
almide build app.almd -o app     # Build standalone binary
almide build app.almd --target wasm  # Build WebAssembly (WASI)
almide compile                   # Compile to .almdi (module interface + IR)
almide compile parser            # Compile a specific module
almide compile --json            # Output interface as JSON
almide test                      # Find and run all test blocks (recursive)
almide test spec/lang/           # Run tests in a directory
almide test --run "pattern"      # Filter tests by name
almide check app.almd            # Type check only
almide check app.almd --json     # Type check with JSON output
almide fmt app.almd              # Format source code
almide clean                     # Clear build + dependency cache
```

## WASM Binary Size

Almide emits WASM bytecode directly (no Rust/C intermediary). Each binary is self-contained — allocator, string handling, and runtime are all included. No external GC or host runtime dependency.

Sizes below are with `ALMIDE_WASM_OPT=1` (post-build `wasm-opt -O3`, opt-in for now).

| Program | Default | `wasm-opt -O3` |
|---------|--------:|---------------:|
| Hello World | 2,587 B | **889 B** |
| FizzBuzz | 2,880 B | **1,135 B** |
| Fibonacci | 2,942 B | **1,086 B** |
| Closure | 3,161 B | **1,193 B** |
| Variant | 3,628 B | **1,680 B** |

### vs Rust + wasm-bindgen

For trivial programs the two are tied. For numerical / stdlib-heavy code, Almide stays small while wasm-bindgen grows with API surface:

| Workload | Almide | Rust + wasm-bindgen | Ratio |
|---|--------:|--------:|:------:|
| Hello World (`println`) | **889 B** | 852 B | tie |
| Matmul + scale (matrix stdlib) | **1,492 B** | 10,701 B | **Almide 7.17× smaller** |
| Elementwise chain (scale + scale + add) | **2,108 B** | 10,701 B | **Almide 5.07× smaller** |

Why: wasm-bindgen's type-marshalling glue grows with each exported API; Almide's stdlib lives in a single coherent runtime that doesn't bloat as the call surface expands.

Bench source: [almide-wasm-bindgen/examples/bench](https://github.com/almide/almide-wasm-bindgen/tree/main/examples/bench).

## Native Performance

Almide compiles to Rust, which then compiles to native machine code. No runtime, no GC, no interpreter.

| Metric | Value |
|--------|-------|
| Binary size (minigit CLI) | **444 KB** (stripped) |
| Runtime (100 ops) | **1.1s** |
| Dependencies | **0** (single static binary) |
| WASM target | `almide build app.almd --target wasm` |

## Project Status

| Category | Status |
|----------|--------|
| Compiler | Pure Rust, single binary, 0 ICE |
| Targets | Rust (native), WASM (direct emit) |
| Codegen | v3 — Nanopass + TOML templates, fully target-agnostic walker |
| Stdlib | 430 functions across 23 modules |
| Tests | 177 test files pass (Rust), 278 pass (WASM) |
| MSR | 23/25 exercises pass (Sonnet 4.6, WASM, max 3 attempts) |
| MiniGit Bench | 41/41 tests pass, 100% success rate ([ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench)) |
| Artifacts | `.almdi` module interface files via `almide compile` |
| Playground | [Live](https://almide.github.io/playground/) — compiler runs as WASM in browser |

### AI Coding Language Benchmark

Comparison with 15 established languages using [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) (MiniGit implementation task).

![Execution Time](docs/figures/lang-bench-time.png?v=1775655978)
![Code Size](docs/figures/lang-bench-loc.png?v=1775655978)
![Pass Rate](docs/figures/lang-bench-pass-rate.png?v=1775655978)

> Almide uses Sonnet 4.6 (unknown language); all others use Opus 4.6 (known language). Almide achieves 100% pass rate with fewer lines of code than most languages, despite needing more time due to the model having no prior training data for the language.

## Ecosystem

### Grammar — [almide-grammar](https://github.com/almide/almide-grammar)

Single source of truth for Almide syntax — keywords, operators, precedence, and TextMate scopes. Written in Almide itself.

All tools that need to know Almide's syntax import this module rather than maintaining their own keyword lists:

```toml
# almide.toml
[dependencies]
almide-grammar = { git = "https://github.com/almide/almide-grammar", tag = "v0.1.0" }
```

```almide
import almide_grammar
almide_grammar.keyword_groups()    // 6 groups, 41 keywords
almide_grammar.precedence_table()  // 8 levels, pipe → unary
```

The compiler itself uses `almide-grammar`'s TOML files (`tokens.toml`, `precedence.toml`) at build time to generate its lexer keyword table — ensuring the compiler and all tooling stay in sync.

### Editor Support

- **VS Code** — [vscode-almide](https://github.com/almide/vscode-almide) — Syntax highlighting, bracket matching, comment toggling, code folding
- **Tree-sitter** — [tree-sitter-almide](https://github.com/almide/tree-sitter-almide) — Tree-sitter grammar for editors that support it (Neovim, Helix, Zed)

### Playground — [playground](https://github.com/almide/playground)

Browser-based compiler and runner. The Almide compiler runs as WASM — no server, no installation. Try it at [almide.github.io/playground](https://almide.github.io/playground/).

## Documentation

- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — Compiler pipeline, module map, design decisions
- [docs/SPEC.md](./docs/SPEC.md) — Full language specification
- [docs/GRAMMAR.md](./docs/GRAMMAR.md) — EBNF grammar + stdlib reference
- [docs/CHEATSHEET.md](./docs/CHEATSHEET.md) — Quick reference for AI code generation
- [docs/DESIGN.md](./docs/DESIGN.md) — Design philosophy and trade-offs
- [docs/STDLIB-SPEC.md](./docs/STDLIB-SPEC.md) — Standard library specification (381 functions)
- [docs/roadmap/](./docs/roadmap/README.md) — Language evolution plans

## Contributing

Contributions are welcome! Please open an issue or pull request on [GitHub](https://github.com/almide/almide).

After cloning, install the git hooks:

```bash
brew install lefthook  # macOS; see https://github.com/evilmartians/lefthook for other platforms
lefthook install
```

All commits must be in English (enforced by the commit-msg hook). See [CLAUDE.md](./CLAUDE.md) for project conventions.

## License

Licensed under either of [MIT](./LICENSE-MIT) or [Apache 2.0](./LICENSE-APACHE) at your option.
