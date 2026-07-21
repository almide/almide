<!-- description: Interactive REPL — `almide` with no args starts an interactive session -->
<!-- done: 2026-05-20 -->
# REPL

> **Status: Done** — shipped in v0.19.0.

## Implementation (v0.19.0)

`almide` (no arguments) starts an interactive session:

```
$ almide
Almide REPL v0.19.0 — type expressions to evaluate, :q to quit

>>> 1 + 2
3
>>> let name = "world"
>>> "Hello, " + name
"Hello, world"
>>> list.map([1, 2, 3], (x) => x * 2)
[2, 4, 6]
```

### Architecture

- Input classified as TopLevel (`fn`/`type`/`import`), Body (`let`/`var`/assignment), or Expression
- Builds a virtual `.almd` source from accumulated session state + new input
- Declarations: compile-only validation (no cargo build)
- Expressions: compile → cargo build (incremental) → execute → capture stdout
- Debug format (`{:?}`) for output — works for all types including List, records
- RcCow Debug made transparent so `[1, 2, 3]` prints instead of `RcCow([1, 2, 3])`
- Warnings suppressed during REPL compilation via `SUPPRESS_WARNINGS` atomic flag
- Persistent cargo cache at `~/.almide/repl/build/` for incremental builds

### Commands

- `:q` / `:quit` — exit
- `:h` / `:help` — show help
- `:history` — show evaluation history
- `:clear` — clear session state

### Future improvements

- WASM eval (faster iteration, no cargo overhead)
- Multi-line input (continuation prompt `...`)
- Tab completion
- Pretty-print for custom types (Repr protocol)
