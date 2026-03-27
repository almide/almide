<!-- description: Fix type instantiation for generic variant constructors like Nothing -->
# Generic Variant Type Instantiation

**テスト:** `spec/lang/type_system_test.almd`
**ステータス:** ✅ 解決済み (commit af22305)

## 問題

`type Maybe[T] = | Just(T) | Nothing` + `let y: Maybe[Int] = Nothing()` が以下を生成:

```rust
let y = Maybe::Nothing;  // エラー: type annotations needed for Maybe<_>
```

## 根本原因（3層の問題）

### 層1: コンストラクタが空の型引数を返していた

`check_named_call` のバリアントコンストラクタ処理が `Ty::Named(type_name, vec![])` を返していた。ジェネリック型なのに型引数が空。

**修正:** `instantiate_type_generics()` を追加。型定義の TypeVar 数を数え、それぞれに fresh な推論変数を生成。`Ty::Named("Maybe", [TypeVar("?5")])` を返すように変更。

### 層2: Named 型の引数が unify されていなかった

`unify_infer` の `Concrete ↔ Concrete` パスで `(Named(na, _), Named(nb, _)) if na == nb => true` — 名前が同じなら引数を**無視**していた。HM の unification は型コンストラクタの引数を再帰的に unify する。

**修正:** `Named` 同士の unification で引数を `from_ty` 経由で `InferTy` に変換し、再帰的に unify するように変更。

### 層3: resolve_inference_vars が Named の引数に到達しなかった（**真の根本原因**）

`resolve_inference_vars` の `resolve_inner` が `Ty::Named(name, args)` をキャッチオールの `_ => ty.clone()` で処理していた。結果、`Named("Maybe", [TypeVar("?5")])` の中の `?5` が `Int` に解決されず、IR に `TypeVar("?5")` が残り続けた。

**修正（1行）:**

```rust
// src/check/types.rs resolve_inner() に追加
Ty::Named(name, args) if !args.is_empty() => {
    Ty::Named(name.clone(), args.iter().map(|a| Self::resolve_inner(a, solutions, seen)).collect())
}
```

## 型理論との対応

HM (Hindley-Milner) の2つの原則:

1. **型コンストラクタの構造的 unification**: `Maybe(?5)` と `Maybe(Int)` の unification は `?5 = Int` を導出する
2. **型変数の再帰的解決**: 制約解決後、全ての型構造内の型変数を置換する（Named の引数含む）

Almide はこの2つ目が欠けていた。HM では当然やることだが、`resolve_inference_vars` の実装時に `Named` のケースが抜けていた。

## 変更ファイル

- `src/check/types.rs` — `resolve_inner` に `Ty::Named` の再帰処理追加 (+3行)
- `src/check/mod.rs` — `unify_infer` の Named 同士の unification を構造的に (+4行)
- `src/check/calls.rs` — `instantiate_type_generics` 追加、コンストラクタが fresh vars を生成 (+15行)
