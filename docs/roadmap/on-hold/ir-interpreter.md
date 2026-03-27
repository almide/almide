<!-- description: Direct IR execution for instant REPL, playground, and fast test runs -->
# IR Interpreter

An interpreter that executes IR directly. Instant execution without going through codegen → rustc.

## Why

The current `almide run` pipeline is `.almd → IR → .rs → rustc → binary → execute`, where rustc takes 1-3 seconds. With an IR interpreter:

| Use case | Effect |
|----------|--------|
| **REPL** | Enter expression → instant result. No rustc needed |
| **Playground** | Instant execution in browser (interpreter running on WASM) |
| **Test speedup** | `almide test` interprets small tests, compiles large ones |
| **Scripting** | Zero startup time for short scripts |

## Architecture

```
Source → Lexer → Parser → AST → Checker → Lowering → IrProgram
                                                         │
                                              ┌──────────┴──────────┐
                                              ▼                     ▼
                                         Interpreter            Codegen
                                         (instant exec)        (Rust/TS/JS)
```

All IR nodes carry `Ty`, enabling type-safe interpretation.

## Design

```rust
// src/interpret.rs
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Unit,
    List(Vec<Value>),
    Map(BTreeMap<Value, Value>),
    Record(Vec<(String, Value)>),
    Variant { tag: String, payload: Vec<Value> },
    Fn(Closure),
    Option(Option<Box<Value>>),
    Result(std::result::Result<Box<Value>, Box<Value>>),
}

pub fn interpret(ir: &IrProgram) -> Result<Value, String> {
    let mut env = Env::new(&ir.var_table);
    // Execute top-level lets
    for tl in &ir.top_lets { ... }
    // Find and call main
    if let Some(main_fn) = ir.functions.iter().find(|f| f.name == "main") {
        eval_fn(main_fn, &[], &mut env)
    }
}
```

### Phase 1: Pure computation
- Arithmetic, strings, lists, records, match, if/else, lambda
- `almide eval "1 + 2"` → `3`

### Phase 2: Control flow + stdlib
- for/while/do, guard, break/continue
- stdlib (string, list, map, int, float, math)

### Phase 3: Effect functions + I/O
- fs, env, process, path — native calls
- Result/Option propagation

### Phase 4: REPL integration
- `almide repl` — interactive execution with persistent state
- Variable binding accumulation, :type command

## Unlocked by

IR Redesign Phase 5 complete. IrExpr carries Ty on all nodes, enabling safe evaluation while referencing type information. Since both interpret and compile take the same `&IrProgram` as input, it's easy to guarantee that their results match.

## Affected files

| File | Change |
|------|--------|
| `src/interpret.rs` (new) | Interpreter core |
| `src/cli.rs` | `almide eval`, `almide repl` commands |
| `src/main.rs` | Pipeline branching |
