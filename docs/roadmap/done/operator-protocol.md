<!-- description: Convention-based operator dispatch (==, repr, sort, hash) -->
# Operator Protocol [ACTIVE]

Convention 宣言に基づく演算子・言語機能のディスパッチ。
Derive Conventions Phase 1-2 (convention 宣言 + method resolution) の上に構築。

## Scope

| 状況 | 変換 | 前提 |
|------|------|------|
| `a == b` where a: Dog | `Dog.eq(a, b)` | `type Dog: Eq` |
| `"${d}"` where d: Dog | `Dog.repr(d)` | `type Dog: Repr` |
| `list.sort(dogs)` | `Dog.ord` を comparator に | `type Dog: Ord` |
| `map[dog]` | `Dog.hash(dog)` をキーに | `type Dog: Hash` |

## Implementation

### `==` / `!=` dispatch
- checker: `a == b` で `a` の型が `deriving Eq` を持つとき、`Dog.eq(a, b)` が存在すれば使用
- 現状 `almide_eq!` マクロで全型に `==` が動くので、**カスタム eq が定義されている場合のみディスパッチ**
- codegen: `almide_eq!(a, b)` → `Dog_eq(a.clone(), b.clone())` に切り替え

### String interpolation dispatch
- lower: `"${d}"` の string interp で `d` の型が `deriving Repr` → `Dog.repr(d)` を挿入
- 現状 `format!("{:?}", d)` (Debug) で出力 → custom repr があれば `format!("{}", Dog_repr(d))` に

### Sort dispatch
- stdlib `list.sort` の comparator 引数に `Dog.ord` を自動挿入
- codegen で `dogs.sort_by(|a, b| Dog_ord(a, b))` を生成

## Priority
String interpolation > `==` dispatch > sort。auto-derive (下記) が先に必要かもしれない。

---

# Auto Derive

Convention 関数が未定義の場合、コンパイラが自動生成。

| Convention | Auto-derive 内容 |
|-----------|-----------------|
| `Eq` | 全フィールドの `==` で比較 |
| `Repr` | `"TypeName { field1: value1, ... }"` 形式 |
| `Ord` | フィールド順に辞書順比較 |
| `Hash` | 全フィールドの hash を combine |

## Implementation
- IR lowering パスで、`deriving Eq` だが `Dog.eq` が未定義の場合に `IrFunction` を自動生成
- field 一覧は `IrTypeDecl` から取得
- Rust codegen は既に `#[derive(PartialEq)]` を出しているので、auto-derive は Rust ターゲットでは不要
- TS/IR interpreter では必要

## Files
```
src/lower.rs       — auto-derive function generation
src/check/mod.rs   — operator dispatch resolution
src/optimize.rs    — string interp rewrite (optional)
```
