# User Generics & Trait System

**優先度:** 1.x
**前提:** Generics Phase 1 完了済み

## 現状

### ユーザー定義 Generics ✅ 動作確認済み

```almide
fn identity[A](x: A) -> A = x
fn map_pair[A, B](p: (A, A), f: (A) -> B) -> (B, B) = (f(p.0), f(p.1))
fn first[A, B](p: (A, B)) -> A = p.0

type Stack[T] = { items: List[T], size: Int }
type Tree[T] = | Leaf(T) | Node(Tree[T], Tree[T])
```

全て動作する。checker + lower + codegen (Rust/TS) 対応済み。

### 既知の問題

1. **テスト名と関数名の衝突** — test "identity" + fn identity で名前衝突。テスト関数名のsanitizeが不十分
2. **Trait bounds なし** — `fn sort[T](xs: List[T])` は T に制約がないので Rust では `T: PartialOrd` が必要だが自動付与されない
3. **Trait/Impl なし** — ユーザー定義の型クラス/インターフェースがない

## Phase 2: Trait Bounds (1.x)

```almide
// 将来構文
fn sort[T: Ord](xs: List[T]) -> List[T] = ...
fn show[T: Repr](x: T) -> String = ...
```

### 設計判断

| 選択肢 | メリット | デメリット |
|--------|---------|-----------|
| A. Structural bounds (現在) | `[T: { name: String }]` で十分 | 複雑な制約が書けない |
| B. Built-in bounds のみ | `Eq`, `Ord`, `Repr`, `Hash` だけ | ユーザー拡張不可 |
| C. User-defined traits | 完全な抽象化 | 複雑性爆発、LLM精度低下 |

**推奨: B (Built-in bounds のみ)** — Almide の設計原則 "No user-defined traits" を維持。Go が interface なしで 12 年繁栄した教訓。

### Built-in Bounds 候補

| Bound | 意味 | 自動判定 |
|-------|------|---------|
| `Eq` | `==` / `!=` 可能 | ✅ 既存 (Float除く) |
| `Ord` | `<` / `>` / `<=` / `>=` 可能 | 型構造から自動 |
| `Hash` | Map key 可能 | ✅ 既存 (Float, Fn除く) |
| `Repr` | `show()` 可能 | 型構造から自動 (Fn除く) |

## Phase 3: Trait/Impl (2.x, 慎重に)

**現状の設計原則: "No user-defined traits"**

理由:
- 抽象化の深さが増す → LLM精度低下
- orphan rules, associated types, trait objects → 複雑性爆発
- Almide は "write it the obvious way" が理念

### もし導入するなら

```almide
// 最小限の trait
trait Stringify {
  fn to_str(self) -> String
}

impl Stringify for Color {
  fn to_str(self) -> String = match self {
    Red => "red"
    Green => "green"
    Blue => "blue"
  }
}
```

**制約:**
- No associated types
- No default methods
- No trait objects / dynamic dispatch
- No orphan rules (same module only)
- deriving で自動実装可能なものは deriving

## TODO

- [ ] テスト名/関数名衝突の修正 (sanitize改善)
- [ ] Built-in bounds (Ord, Repr) の実装
- [ ] Trait system の設計判断 (B vs C)
