<!-- description: Direct IR execution for instant REPL, playground, and fast test runs -->
# IR Interpreter [ON HOLD]

IR を直接実行するインタプリタ。codegen → rustc を経由せずに即時実行。

## Why

現在の `almide run` は `.almd → IR → .rs → rustc → binary → execute` で、rustc が 1-3 秒かかる。IR インタプリタがあれば:

| 用途 | 効果 |
|------|------|
| **REPL** | 式を入力 → 即座に結果。rustc 不要 |
| **Playground** | ブラウザで即時実行 (WASM 上でインタプリタ動作) |
| **テスト高速化** | `almide test` で小さいテストは interpret、大きいものは compile |
| **スクリプティング** | 短いスクリプトの起動時間ゼロ |

## Architecture

```
Source → Lexer → Parser → AST → Checker → Lowering → IrProgram
                                                         │
                                              ┌──────────┴──────────┐
                                              ▼                     ▼
                                         Interpreter            Codegen
                                         (即時実行)            (Rust/TS/JS)
```

IR の全ノードが `Ty` を持っているため、型安全にインタプリトできる。

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
- 算術、文字列、リスト、レコード、match、if/else、lambda
- `almide eval "1 + 2"` → `3`

### Phase 2: Control flow + stdlib
- for/while/do、guard、break/continue
- stdlib (string, list, map, int, float, math)

### Phase 3: Effect functions + I/O
- fs, env, process, path — ネイティブ呼び出し
- Result/Option 伝播

### Phase 4: REPL integration
- `almide repl` — 状態を保持して対話的に実行
- 変数束縛の蓄積、:type コマンド

## Unlocked by

IR Redesign Phase 5 完了。IrExpr が全ノードに Ty を保持しており、型情報を参照しながら安全に evaluate できる。codegen と同じ `&IrProgram` を入力とするため、interpret と compile の結果が一致することを保証しやすい。

## Affected files

| File | Change |
|------|--------|
| `src/interpret.rs` (new) | インタプリタ本体 |
| `src/cli.rs` | `almide eval`, `almide repl` コマンド |
| `src/main.rs` | パイプライン分岐 |
