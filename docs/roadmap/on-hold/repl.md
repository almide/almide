# REPL [ON HOLD]

Interactive Read-Eval-Print Loop for Almide. Split from [tooling.md](../on-hold/tooling.md).

## Usage

```bash
almide repl

almide> 1 + 2
3

almide> let xs = [1, 2, 3]
almide> xs |> list.map(fn(x) => x * 2)
[2, 4, 6]

almide> type Point = { x: Int, y: Int }
almide> Point { x: 1, y: 2 }
Point { x: 1, y: 2 }
```

## Architecture

Each input line/block goes through the full pipeline:

```
input → parse as expr/stmt/decl → check → lower → emit Rust → compile → execute → print result
```

### State Accumulation

The REPL maintains accumulated state across inputs:
- **Type environment**: declared types persist
- **Variable bindings**: `let x = ...` persists for subsequent inputs
- **Function definitions**: `fn f(x) = ...` persists
- **Import state**: `import http` persists

Each new input is compiled in the context of all previous declarations.

### Implementation Strategy

**Option A: Compile-and-exec per line**
- Wrap each expression in a `fn __repl_eval() -> String { format!("{:?}", <expr>) }`
- Compile full accumulated program + new expression
- Execute and capture stdout
- Pro: Reuses existing pipeline exactly
- Con: Slow for each evaluation (~1s for rustc)

**Option B: Interpreter mode**
- Walk the IR directly without emitting Rust
- Pro: Instant evaluation
- Con: Large implementation effort, behavior divergence from compiled code

**Recommendation:** Option A for Phase 1. Combined with incremental compilation (Level 1 cache), recompilation is only needed when new code is added.

## Phase 1: Basic REPL

- [ ] `almide repl` subcommand
- [ ] Expression evaluation with result display
- [ ] `let` / `var` bindings persist across inputs
- [ ] `fn` / `type` declarations persist
- [ ] Multi-line input (detect incomplete expressions by trailing `{`, `=`, etc.)
- [ ] Error display (same format as compiler errors)

## Phase 2: Ergonomics

- [ ] Tab completion (function names, module functions, variable names)
- [ ] History (arrow keys, persistent across sessions via `~/.almide/repl_history`)
- [ ] `:type <expr>` — show type without evaluating
- [ ] `:reset` — clear accumulated state
- [ ] `:load <file>` — load declarations from a file

## Dependencies

- `rustyline` crate for line editing, history, and completion
- Incremental compilation cache (reduces per-input latency)

## Affected Files

| File | Change |
|------|--------|
| `src/cli.rs` | Add `repl` subcommand |
| `src/repl.rs` (new) | REPL loop, state accumulation |
| `Cargo.toml` | Add `rustyline` dependency |
