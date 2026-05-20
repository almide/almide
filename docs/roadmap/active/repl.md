<!-- description: Interactive REPL — `almide` with no args starts an interactive session -->
# REPL

> **Status: Active** — design started, implementation pending.

## Design

`almide` (no arguments) starts an interactive session:

```
$ almide
Almide REPL v0.18.x — type expressions to evaluate, :q to quit

>>> 1 + 2
3
>>> let name = "world"
>>> "Hello, " + name
"Hello, world"
>>> list.map([1, 2, 3], (x) => x * 2)
[2, 4, 6]
```

### Key Decisions

- **No subcommand**: `almide` alone → REPL (like Python, Node, Mojo)
- **Session state**: `let`/`var` bindings persist across lines
- **Expression result**: auto-printed with debug representation
- **Codegen strategy**: compile each eval to Rust, cargo run, capture output
  - Alternative: WASM eval (faster iteration, no cargo overhead)

### Challenges

- Top-level `let` codegen produces `LazyLock` statics — need to unwrap for printing
- Expression type detection: need to format Int/Float/String/Bool/List differently
- Incremental compilation: first eval is slow (cargo build), subsequent should be fast (incremental)
- Repr protocol: `Repr` derives exist for user types but not for primitives

### Commands

- `:q` / `:quit` — exit
- `:h` / `:help` — show help
- `:history` — show evaluation history
- `:clear` — clear session state

## References

- Python REPL, Node REPL, `iex` (Elixir), `ghci` (Haskell)
- Mojo REPL (`mojo` with no args)
