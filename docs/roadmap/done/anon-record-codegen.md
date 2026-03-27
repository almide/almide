<!-- description: Fix Rust codegen emitting invalid type for anonymous records -->
<!-- done: 2026-03-18 -->
# Anonymous Record Codegen Fix

**優先度:** High — Grammar Lab 実験で全タスクの 30% が影響
**見積り:** 1–2日
**ブランチ:** develop

## 問題

空リストリテラル `[]` に anonymous record 型の型注釈がつくと、Rust codegen が `AnonRecord` という存在しない型名を出力する。

```almide
let ps: List[Product] = []
// Product = { name: String, price: Int, category: String }
```

生成される Rust:
```rust
let ps: Vec<Product> = Vec::<AnonRecord>::new();  // ← AnonRecord が未定義
```

正しくは:
```rust
let ps: Vec<Product> = Vec::<Product>::new();
// または
let ps: Vec<Product> = vec![];
```

## 影響

- Grammar Lab `optional-handling` 実験で 10 タスク中 3 タスク (t07, t08, t10) がこのバグで fail
- LLM の出力は正しいのに compile が通らない = **LLM survival rate を人為的に下げている**
- テストで `let xs: List[T] = []` を書くたびに踏む。頻出パターン

## 再現

```almide
type Item = { name: String, value: Int }

test "empty list of records" {
  let items: List[Item] = []
  assert_eq(list.len(items), 0)
}
```

```
$ almide test repro.almd
error[E0412]: cannot find type `AnonRecord` in this scope
```

## 原因 (推定)

`emit_rust/` の空リスト codegen で、型パラメータを解決する際に anonymous record 型のマッピングが行われていない。`Ty::Record(fields)` → Rust 構造体名 (`AlmdRec_*` or type alias) の変換が空リスト文脈で欠けている。

## 修正方針

1. `emit_rust/` で空リスト `[]` の codegen を特定
2. 型注釈から element type を取得する箇所を確認
3. `Ty::Record` → concrete Rust 型名の変換を空リストにも適用
4. テスト: `let xs: List[{a: Int}] = []` が compile + pass することを確認

## 確認タスク

- [ ] `src/emit_rust/` で `AnonRecord` を grep → 生成箇所を特定
- [ ] 空リスト以外にも同じ問題がないか確認 (`none` の型推論など)
- [ ] 修正後、Grammar Lab の t07/t08/t10 が PASS に変わることを確認

## 関連

- [design-debt.md #2](../on-hold/design-debt.md) — Anonymous record の根本設計
- Grammar Lab [optional-handling REPORT](../../../research/grammar-lab/experiments/optional-handling/REPORT.md)
