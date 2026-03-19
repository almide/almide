# Trait System — HKT 基盤上の Protocol/Interface

**優先度:** 2.x
**前提:** HKT Foundation Phase 1-4 完了、User Generics Phase 2 (Built-in Bounds)
**構文:** 別途設計判断 — ユーザーに見せるかは未定

## 概要

`Ty::Applied(TypeConstructorId, Vec<Ty>)` の統一表現の上に Trait/Protocol を構築する。
コンパイラ内部で型コンストラクタの代数的性質を表現でき、
ユーザー定義型にも自動的に map 等の操作が適用可能になる。

## 現状

- `TypeConstructorRegistry` に代数法則テーブルがある (AlgebraicLaw)
- Stream Fusion が法則テーブルを参照して最適化
- しかし「List は Functor」という知識はハードコード

## 目標 (コンパイラ内部)

```rust
// コンパイラが内部で持つ trait 定義
trait Functor for TypeConstructor where Kind = * -> * {
    fn map[A, B](self: F[A], f: fn(A) -> B) -> F[B]
}

// 自動実装: List, Option は Functor
// ユーザー定義型: type Tree[T] も Kind: * -> * なら自動で Functor
```

## ユーザーへの公開 (検討中)

### Option A: 見せない (Go 方式)
- コンパイラ内部で最適化に使うだけ
- ユーザーは `list.map`, `option.map` を個別に使う
- LLM にとって最もシンプル

### Option B: 最小限の trait (Gleam 方式)
```almide
trait Stringify {
  fn to_str(self) -> String
}
```
- No associated types, no default methods
- No trait objects / dynamic dispatch

### Option C: Built-in bounds のみ (現在の方針)
- `Eq`, `Ord`, `Hash`, `Repr` だけ
- ユーザー定義 trait なし
- → [active/user-generics-and-traits.md](../active/user-generics-and-traits.md)

## 依存関係

```
user-generics-and-traits.md Phase 2 (Built-in Bounds)
  → この trait-system.md (ユーザー定義 trait の検討)
    → user-generics-and-traits.md Phase 3 (もしやるなら)
```
