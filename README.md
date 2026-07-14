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
- **Standard library** — 834 functions across 39 modules (string, list, map, json, http, fs, etc.)
- **Built-in testing** — `test "name" { assert_eq(a, b) }` with `almide test`
- **Actionable diagnostics** — Every error includes file:line, context, and a concrete fix suggestion

## The Equivalence Claim — Byte-Identical Across Targets

**Every program that compiles for both targets produces byte-identical observable output — stdout, stderr, exit code — whether it runs as a native binary or as WebAssembly.** Native is the oracle; `native == wasm` is a hard invariant, not a "target difference" to be documented around.

"Byte-identical" means the *execution output*, not the compiled artifacts — a native binary and a `.wasm` module are different bytes by construction; what must not differ is anything the program lets you observe.

This claim is not prose. Every observable promise is a named contract in the [behavior-contract ledger](docs/contracts/), each traceable to executable evidence, and the numbers below are regenerated from the ledger (`scripts/gen-claims.sh`, enforced by `scripts/check-contracts.sh` in CI) so this section cannot drift from what the gates actually verify:

<!-- claims:generated:start — derived from docs/contracts/contracts.toml by scripts/gen-claims.sh; DO NOT EDIT between the markers -->
> **Ledger: 132 contracts — 131 active, 1 flagged-for-revision.**
>
> **Exceptions (1)** — contracts flagged for revision; the ratchet says this list may only shrink:
>
> - [C-006 — fan.timeout is the SOLE documented wall-clock divergence (wasm warns)](docs/contracts/C-006-fan-timeout-divergence.md)
<!-- claims:generated:end -->

| Evidence layer | What it locks |
|---|---|
| [Contract ledger](docs/contracts/) | every promise is a named `C-NNN`; an `active` contract must carry evidence of class ≥ `fixture` |
| [Cross-target fixture gate](tests/wasm_runtime_test.rs) | every `spec/wasm_cross/*.almd` fixture runs on both targets; outputs byte-compared (`wasm_cross_target_spec`) |
| [Differential fuzz](tests/regex_fuzz_test.rs) | randomized programs and inputs, native vs wasm outputs compared |
| Emit-time Σ-probes | wasm Unicode/case tables exhaustively probed against Rust `std` over the full scalar domain at emit time |
| [Lean 4 belt](crates/almide-perceus-belt/) | RC-insertion correctness machine-checked by the Lean kernel |
| [Org byte-verify sweep](scripts/org-byte-verify.sh) | every runnable repo in the almide org executed on both targets, stdout + exit byte-compared |

## Memory Safety — Formally Verified

You write no ownership annotations, no lifetimes, no `free` — memory management is fully automatic, garbage-collector-free, pause-free. The mechanism is per-target today:

- **WebAssembly** — the compiler inserts [Perceus](https://www.microsoft.com/en-us/research/publication/perceus-garbage-free-reference-counting-with-reuse/) reference counting: precise, compiler-placed RC with no GC. This is the path the Lean proofs below certify.
- **Native (Rust)** — the compiler emits ownership-idiomatic Rust, inserting borrows and clones for you; every heap value is freed by Rust's own scope-end drops.

Making Perceus the *single* memory model on both targets — native rendered from the same IR discipline, with the Drop-erasure machine-checked — is tracked in [#764](https://github.com/almide/almide/issues/764).

Where Rust gives you *zero-cost* abstraction (paid for in ownership annotations), Almide gives you **zero-annotation** abstraction: you write none, and every heap free is machine-proven — *write none, prove all.*

The correctness of the RC insertion pass is **mathematically proven** in Lean 4:

```lean
theorem perceus_all_heap_freed (fb : FnBody) :
    allHeapFreed (perceusTransform fb)
```

**For any program, the compiler produces code where every heap allocation is freed on all execution paths** — by Rust's own drop semantics on native, and by the proven Perceus transform on wasm. 22 theorems, 0 sorry — verified by the Lean 4 kernel.

This is connected to the actual compiler (not a separate paper proof):

- `perceus_verified.rs` runs in the compiler's verify pipeline
- 19 property-based tests validate Lean/Rust algorithm consistency
- CI blocks any unproven theorem (`sorry`) from merging

Details: [`crates/almide-perceus-belt/`](./crates/almide-perceus-belt/) — [Specification](./docs/specs/perceus.md)

## What's Next — v1: The Trust Spine

> In active development on the `develop` branch (the **v1** line). This is a ground-up redesign of the compiler's *trust model*, not a feature on top of v0.

The Perceus proof above proves one compiler pass, once. v1 generalizes that principle to the **whole pipeline** — but instead of proving every pass, it proves a tiny *checker* and makes the compiler re-verify itself on every build.

The v0 compiler (everything described above) takes the shortest path: `AST → IR → codegen`. It's fast, and it's correct *as far as the tests can tell*. v1 asks a harder question: **not "do the tests pass?" but "can a machine prove the output is correct?"**

### The idea

You don't make a compiler trustworthy by making it perfect — a correct 100k-line compiler is a proof obligation no one can discharge. Instead:

> **Don't prove the compiler. Prove a tiny checker — and have the compiler emit a certificate on every build that the checker re-verifies.**

This rests on an asymmetry the whole field stands on: **building is hard, checking is cheap.** Solving a sudoku is work; verifying a solved one is a glance. So the compiler is *allowed* to have bugs — if it emits a wrong artifact, the attached certificate won't check out and the checker rejects it. The only thing that must be proven correct is the checker, and the only theorem is:

> *If the checker accepts, the artifact has the property* — and this theorem never mentions the compiler's internals.

That single move collapses the **trusted base from ~100,000 lines to a few hundred.** The big compiler becomes *untrusted* — free to be as large and buggy as it likes, because nothing trusts it.

### The pipeline (proof-carrying code)

```
        ALS — normative semantics (Coq; the single source of truth for meaning)
         │ refine                                                    │ refine
 ┌───────┴───────────────────────────────┐                          │
 │ UNTRUSTED — any size, bugs allowed     │                          │
 │ .almd → check → lower → MIR → emit     │                          │
 └───────┬───────────────────────────────┘                          │
         │                                                           │
   ( wasm bytes a , certificate bundle c )                           │
         │                                                           │
 ┌───────┴───────────────────────────────────────────────────────────────┐
 │ TRUSTED — a few hundred lines, machine-proven sound in Coq              │
 │   K  property checker      K(c, a) accepts ⟹ a satisfies property P    │
 │   V  translation checker   V(a, ALS) accepts ⟹ a refines ALS(s)       │
 └───────────────────────────────────────────────────────────────────────┘
```

- **K (property checker)** verifies the certificate: memory safety, name totality, capability upper bound, stack balance, termination behavior.
- **V (translation checker)** verifies — *on every build* — that the emitted wasm actually refines the language semantics. This is the answer to the reviewer's killer question: *"You proved a model — but does the thing that actually runs match it?"*
- **ALS** (Almide Language Specification) is the normative semantics, in Coq. The compiler and both backends don't define meaning; they *refine* ALS. So byte-for-byte agreement between targets isn't an afterthought — it falls out of the design.

The **trusted base is a single Coq kernel** (plus CompCert/CertiCoq, the hardware, and the assumption that ALS says what we intend). Everything else is either proven against it or untrusted. There is no third category.

### Receipts — verify it yourself

Each build folds its certificates into claims, each with a published refutation procedure:

| Receipt | Claim |
|---|---|
| **C-SAFE** | Capability-bounded, no undefined behavior — checkable from the artifact alone |
| **C-REPRO** | Same source → byte-identical output on any host |
| **C-FAITHFUL** | Observable behavior refines the language semantics |
| **C-PROVEN** | Kernel-checked universal properties (RC balance, stack balance, …) |

Run `make verify` and you re-derive every claim **on your own machine.** CI is a courtesy pre-run, deliberately *outside* the trusted base — you never have to trust our infrastructure to trust the artifact.

### Why it's slower — on purpose

v0 is fast because it stops at "the tests pass." v1 is slower because every unit of work runs the full verification gauntlet: the property checker (the *corpus-wall*) re-verifies ownership / name / capability certificates for every function; an output-parity gate byte-compares against v0 as an oracle; and where needed `coqc` plus an independent `coqchk` kernel re-check confirm the proofs introduce no stray axioms (`Print Assumptions ⊆ standard`). A single change can trigger minutes of checking.

That cost isn't inefficiency. It's the price of replacing **"it should be correct" (trust the tests) with "a machine has verified that it is" (trust the proof).** v0 is quick but hopeful; v1 is slow but ships only what the checker has accepted.

Where it stands today: the architecture is proven on a language subset, and the current work is taking it end-to-end over real `.almd` programs, on the road to byte-reproducibility and qualification-grade hardening. See [`docs/roadmap/active/v1-proof-architecture.md`](./docs/roadmap/active/v1-proof-architecture.md) and [`v1-system-map.md`](./docs/roadmap/active/v1-system-map.md).

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

Almide emits WASM bytecode directly (no LLVM, no Cranelift). Each binary is self-contained — allocator, string handling, and runtime are all included. No external GC or host runtime dependency. Aggressive DCE strips unused runtime functions and data automatically.

| Program | Size |
|---------|-----:|
| Hello World | **467 B** |
| FizzBuzz | **809 B** |
| Fibonacci (recursive) | **682 B** |
| Closure + call_indirect | **812 B** |
| Variant (match + float) | **1,105 B** |

These are raw `almide build --target wasm` output — no post-processing. `wasm-opt -O3` saves only 1–5 more bytes because the compiler's built-in dead code and dead data elimination already strips everything unused.

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
| Stdlib | 834 functions across 39 modules |
| Tests | 240 test files pass (Rust), 232 pass (WASM) |
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
- [docs/stdlib/](./docs/stdlib/) — Standard library reference, per module (834 functions across 39 modules)
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
