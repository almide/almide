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

- `crates/almide-codegen/src/pass_clone.rs` — Clone 挿入パス
- `crates/almide-ir/src/use_count.rs` — use-count 分析

### ユーザーへの影響

- パフォーマンス: 不要な clone が挿入される可能性（正確性優先の設計）
- 最適化: borrow inference (`pass_borrow_inference*.rs`) と alias COW
  (`pass_alias_cow.rs`) が clone を削減する（既存コードの動作は変わらない）

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

- `crates/almide-codegen/src/pass_result_propagation.rs` — `Try { expr }` 挿入パス
- 詳細仕様: [specs/effect-fn-call-semantics.md](./specs/effect-fn-call-semantics.md)

---

## 3. Runtime 埋め込み

### 何が起きるか

生成コードに Almide ランタイムが自動埋め込まれる。外部 crate は不要。

### Rust target

`runtime/rs/src/*.rs` の内容が生成 `.rs` ファイルに埋め込まれる
（`crates/almide-codegen/build.rs` が `generated/rust_runtime.rs` を生成）。

含まれるもの:
- `almide_eq!` / `almide_ne!` マクロ (深い等値比較)
- `AlmideConcat` trait (String + List 連結)
- `@intrinsic` 宣言された stdlib ランタイム関数

### WASM target

外部ランタイムは存在しない。stdlib は self-hosted な純 Almide 実装
（`stdlib/*.almd` → `crates/almide-mir/src/render_wasm/registry.rs`）が
ユーザコードと一緒に WAT へコンパイルされ、少数の手書き WAT プリアンブルと
共にモジュールへ埋め込まれる。詳細: [WASM-OUTPUT.md](./WASM-OUTPUT.md)。

---

## 4. Perceus RC 挿入 (v1 trust-spine: WASM / native render)

### 何が起きるか

v1 MIR (`crates/almide-mir`) は Perceus 方式で参照カウント操作
（dup / drop）と、in-place 変異のための一意性検査（MakeUnique）を自動挿入する。
所有権とメモリレイアウトの単一真実源はこの MIR。

### ユーザーへの影響

- 言語上の意味は変わらない（native / wasm / interp の 3-way oracle で保証）
- 性能特性のみが対象。MakeUnique elision 等の最適化はリリースごとに強化される
- 証明: `crates/almide-perceus-belt/`（Lean 4）が RC 規律を検証する

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

- `crates/almide-codegen/src/pass_fan_lowering.rs` — fan lowering パス

---

## 隠さない操作

| 操作 | 言語での表現 | 隠さない理由 |
|------|-------------|-------------|
| I/O | `effect fn` | 型シグネチャで明示 |
| エラー伝播 | `Result[T, E]` | 型で可視 |
| 可変性 | `var` vs `let` | キーワードで明示 |
| 並行化 | `fan { }` | 構文で明示 |
