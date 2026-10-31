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
- 最適化: borrow inference (`pass_borrow_inference*.rs`) が clone を削減する
  （既存コードの動作は変わらない）。v1 spine 側の別名解析は
  `crates/almide-mir/src/alias_safety.rs`

---

## 2. エラー伝播 — **隠さない**（auto-`?` は廃止済み）

かつてここには auto-`?` 挿入（`effect fn` 内の可謬呼び出しに `?` が暗黙に付く）が
記載されていた。**ADR-0008 で廃止済み**。伝搬は綴り一本 — 後置 `!` だけが `?` に落ちる。

```almide
effect fn load() -> String = {
  let text = fs.read_text("file.txt")   // E041 — Result 値のまま、伝搬しない
  let text = fs.read_text("file.txt")!  // これが `?` に落ちる
  text
}
```

`!` を書かない可謬呼び出しは Result **値**であって制御フローではない:

- 型注釈のない束縛 → **E041**（`let x = f()`）
- 文の位置で捨てる → **E042**（must-use。`f()!` か `let _ = f()`）
- `list.try_*` の綴り → **E043**（コールバックの `!` が戦略）

### ファイル

- `crates/almide-frontend/src/lower/auto_try.rs` — 名前は履歴的。現在は
  `!` マーカー駆動の `Try` 挿入で、暗黙挿入は行わない
- `crates/almide-codegen/src/pass_result_propagation.rs` — 署名 lift と
  呼び出し位置の `Try` 反映
- 詳細仕様: [specs/effect-fn-call-semantics.md](../specs/effect-fn-call-semantics.md)、
  [ADR-0008](../adr/0008-explicit-propagation-only.md)

---

## 3. Runtime 埋め込み

### 何が起きるか

生成コードに必要な Almide ランタイムが埋め込まれる。使用機能によっては
外部 Rust crate やホスト機能も必要になる。

### Rust target

`runtime/rs/src/*.rs` の内容が生成 `.rs` ファイルに埋め込まれる
（`crates/almide-codegen/build.rs` が `generated/rust_runtime.rs` を生成）。

含まれるもの:
- `almide_eq!` / `almide_ne!` マクロ (深い等値比較)
- `AlmideConcat` trait (String + List 連結)
- `@intrinsic` 宣言された stdlib ランタイム関数

### WASM target

stdlib の self-hosted な Almide 実装はユーザコードとともにリンクされ、
`almide-wasm` の structural emitter が直接 Wasm バイトを生成する。
旧 MIR→WAT renderer と手書き WAT preamble 経路は廃止済み。
新しい self-hosted body は `@implements` で宣言し、生成 registry へ反映する。
I/O などのホスト操作には選択した実行ルートの import 実装が必要。
詳細: [WASM-OUTPUT.md](../wasm/WASM-OUTPUT.md)。

---

## 4. Perceus RC 挿入 (native MIR / structural Wasm)

### 何が起きるか

v1 MIR (`crates/almide-mir`) は Perceus 方式で参照カウント操作
（dup / drop）と、in-place 変異のための一意性検査（MakeUnique）を自動挿入する。
この MIR は native renderer と MIR witness producer の経路であり、
現行 Wasm は別の structural emitter が RC/layout を扱う。

### ユーザーへの影響

- 言語上の意味を保つことが要件（native / wasm / interp の差分ゲートが対象 corpus で検査）
- 性能特性のみが対象。MakeUnique elision 等の最適化はリリースごとに強化される
- 証明: `crates/almide-perceus-belt/`（Lean 4）が RC 規律を検証する

---

## 5. fan の並行化 (Rust target)

### 何が起きるか

native の並行化対象 `fan { a(), b() }` は `std::thread::scope` + `spawn`
で実行される。`@ordered` 操作を含む block などは arm 順に実行されるため、
すべての `fan` が必ず thread-per-arm になるわけではない。以下は並行 route の概略。

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
- 実行基盤と並行化可否は compiler/host が決める。普通の `fan` は全 sibling を join し、arm 順の最初のエラーを返す

### ファイル

- `crates/almide-codegen/src/pass_fan_lowering.rs`、`pass_rust_lowering_fan.rs` — fan lowering と native parallel routing

---

## 隠さない操作

| 操作 | 言語での表現 | 隠さない理由 |
|------|-------------|-------------|
| I/O | `effect fn` | 型シグネチャで明示 |
| エラー伝播 | `Result[T, E]` | 型で可視 |
| 可変性 | `var` vs `let` | キーワードで明示 |
| 並行化 | `fan { }` | 構文で明示 |
