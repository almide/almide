# Hidden Operations

> Zig の原則: "no hidden control flow, no hidden memory allocations"
> Almide は意図的にいくつかの操作を隠す。ここにその全てを文書化する。

---

## 1. Clone 自動挿入 (Rust target)

### 何が起きるか

非 Copy 型 (String, List, Map, Record, Variant) の変数が複数回使われると、コンパイラが `.clone()` を自動挿入する。

### 条件

- `use_count > 1` — IR の use-count 分析で変数の使用回数をカウント
- `!is_copy` — Int, Float, Bool 等の Copy 型には挿入しない
- 最後の使用では clone しない（move で渡す）

### ファイル

- `src/emit_rust/lower_rust_expr.rs` — Var 参照時の clone 判定
- `src/ir/use_count.rs` — use-count 分析

### ユーザーへの影響

- パフォーマンス: 不要な clone が挿入される可能性（正確性優先の設計）
- 最適化: 各 1.x リリースで clone 削減を改善（既存コードの動作は変わらない）

---

## 2. Auto-`?` 挿入 (Rust target)

### 何が起きるか

`effect fn` 内で `Result[T, E]` を返す関数を呼ぶと、生成 Rust コードに自動で `?` が付与される。

```almide
effect fn load() -> Result[String, String] = {
  let text = fs.read_text("file.txt")  // auto-? ここで挿入
  ok(text)
}
```

生成 Rust:
```rust
fn load() -> Result<String, String> {
    let text = almide_rt_fs_read_text("file.txt")?;  // ? が自動挿入
    Ok(text)
}
```

### 条件

- `auto_try == true` — `effect fn` 内かつ test block でない場合
- 呼び出し先の関数が `is_effect == true` または `Result` を返す

### 条件外

- `test` block 内 — `auto_try = false` で `.unwrap()` に変換
- `fan { }` 内 — spawn closure は `?` なし、`join().unwrap()?` で外側に伝播
- `pure fn` 内 — そもそも effect fn を呼べない (E006)

### ファイル

- `src/emit_rust/lower_rust.rs:113` — `auto_try` フラグの設定
- `src/emit_rust/lower_rust_expr.rs:329-586` — Call 式の `?` 挿入判定

---

## 3. Result 消去 (TS target)

### 何が起きるか

TS ターゲットでは `Result[T, E]` が消去される:
- `ok(x)` → `x` (値をそのまま返す)
- `err(e)` → `throw new Error(e)` (例外を投げる)

```almide
effect fn parse(s: String) -> Result[Int, String] = ...
```

生成 TS:
```typescript
function parse(s: string): number {
    // ok(42) → return 42
    // err("bad") → throw new Error("bad")
}
```

### 理由

JavaScript/TypeScript のネイティブなエラー処理は例外。Result 型を保持すると TS エコシステムとの相互運用性が下がる。

### ファイル

- `src/emit_ts/lower_ts.rs` — Result 消去ロジック

---

## 4. Runtime 埋め込み

### 何が起きるか

生成コードに Almide ランタイムが自動埋め込まれる。外部 crate/npm パッケージは不要。

### Rust target

`src/emit_rust/core_runtime.txt` + `runtime/rs/src/*.rs` の内容が生成 `.rs` ファイルに `include_str!` で埋め込まれる。

含まれるもの:
- `almide_eq!` / `almide_ne!` マクロ (深い等値比較)
- `AlmideConcat` trait (String + List 連結)
- 各 stdlib モジュールのランタイム関数 (22 モジュール)

### TS target

`runtime/ts/*.ts` の内容が生成 `.ts` ファイルの先頭に埋め込まれる。

含まれるもの:
- `__deep_eq` (深い等値比較)
- `__concat` (String + List 連結)
- 各 stdlib モジュールの TS 実装

### ファイル

- `src/generated/rust_runtime.rs` — Rust ランタイム埋め込み (build.rs で生成)
- `src/emit_ts/mod.rs` — TS ランタイム埋め込み

---

## 5. fan の並行化 (Rust target)

### 何が起きるか

`fan { a(); b() }` は `std::thread::scope` + `spawn` に変換される。各式が OS スレッドで実行される。

```rust
std::thread::scope(|__s| -> Result<_, String> {
    let __fan_h0 = __s.spawn(move || { a() });
    let __fan_h1 = __s.spawn(move || { b() });
    Ok((__fan_h0.join().unwrap()?, __fan_h1.join().unwrap()?))
})?
```

### ユーザーへの影響

- 外部変数は `move` でキャプチャされる（clone される可能性）
- `var` のキャプチャはコンパイルエラー (E008)
- スレッド数 = fan 内の式の数（制限なし）

### ファイル

- `src/emit_rust/lower_rust_expr.rs` — `lower_fan`, `lower_fan_call`

---

## 隠さない操作

| 操作 | 言語での表現 | 隠さない理由 |
|------|-------------|-------------|
| I/O | `effect fn` | 型シグネチャで明示 |
| エラー伝播 | `Result[T, E]` | 型で可視 |
| 可変性 | `var` vs `let` | キーワードで明示 |
| 並行化 | `fan { }` | 構文で明示 |
