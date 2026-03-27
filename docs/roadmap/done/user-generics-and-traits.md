<!-- description: User-defined generics and protocol system for custom types -->
# User Generics & Protocol System

**優先度:** 1.x
**前提:** Generics Phase 1 完了済み
**ブランチ:** `feature/protocol`

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

## Protocol System — 完了

**キーワード: `protocol`** (Swift/Python の語彙)

Convention system をユーザー定義に開放する。組み込み convention (Eq, Repr, Codec 等) も protocol の特殊ケースとして統一。

### 構文

```almide
// protocol 定義
protocol Action {
  fn name(a: Self) -> String
  fn execute(a: Self, ctx: Context) -> Result[String, String]
}

// 型が protocol を満たすことを宣言（既存の convention 構文と同じ）
type GreetAction: Action = { greeting: String }

// convention methods で実装（既存の仕組み、変更なし）
fn GreetAction.name(a: GreetAction) -> String = "greet"
fn GreetAction.execute(a: GreetAction, ctx: Context) -> Result[String, String] =
  ok(a.greeting)

// generic bounds で使用
fn run_action[T: Action](action: T, ctx: Context) -> Result[String, String] =
  action.execute(ctx)

// impl ブロックで実装（convention methods の代替構文）
impl Action for GreetAction {
  fn name(a: GreetAction) -> String = "greet"
  fn execute(a: GreetAction, ctx: Context) -> Result[String, String] =
    ok(a.greeting)
}

// derive との共存
type User: Codec = { name: String, age: Int } derive(Codec)
```

### 設計方針

- `Self` は実装型のプレースホルダー（型、キーワードではない）
- 満足は **明示的** — `type Foo: Protocol` の宣言、または `impl Protocol for Foo { ... }` が必要
- **2つの実装スタイル**: convention methods (`fn Foo.method`) と `impl` ブロック。両方等価
- モノモーフィゼーションで解決 — 動的ディスパッチなし
- 組み込み convention は protocol として登録（後方互換維持）

### 実装進捗

| Phase | 内容 | 状態 |
|-------|------|------|
| Phase 1 | AST + Parser (protocol キーワード, ProtocolMethod 強型化, `impl` ブロック) | ✅ 完了 |
| Phase 2 | 型システムインフラ (ProtocolDef, TypeEnv 拡張, impl_validated) | ✅ 完了 |
| Phase 3 | チェッカー (protocol 登録, 満足検証, impl ブロック統合, シグネチャ検証) | ✅ 完了 |
| Phase 4 | Generic bounds (`fn f[T: Action](x: T)`, `[T: P1 + P2]`) | ✅ 完了 |
| Phase 5 | Lowerer (generic protocol メソッド呼び出し解決, モノモーフ書き換え) | ✅ 完了 |
| Phase 6 | 後方互換性 (既存 derive/convention との統合) | ✅ 完了 |
| Phase 7 | テスト (92+ テスト, 12 テストファイル) | ✅ 完了 |

### NOT in scope

- default methods
- associated types
- dynamic dispatch / protocol objects
- orphan rules
- `derive(UserProtocol)` — 組み込み convention のみ auto-derive 可能
