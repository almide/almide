# HKT Foundation — Internal Type Constructor Infrastructure

**優先度:** 1.x (generic fn と同時進行)
**前提:** Generics Phase 1 完了済み
**原則:** ユーザーに HKT syntax は見せない。コンパイラ内部の表現力を上げる。

> 「ユーザーにはシンプル、コンパイラは賢い」

---

## 動機

### なぜ内部 HKT 基盤が必要か

1. **Stream Fusion** — `map |> filter |> fold` の中間アロケーション消滅 (2-5x 高速化)
2. **Ty 統一** — ハードコードされた型コンストラクタを統一表現に → 型チェッカー簡素化
3. **ユーザー定義型の一級市民化** — `type Tree[T]` が `List[T]` と同じコードパスで処理される
4. **Effect 推論の代数的基盤** — ヒューリスティックではなく法則に基づく推論
5. **将来の Trait system が最初から強力** — 技術的負債ゼロで Trait を導入可能

### なぜユーザーに見せないか

| 観点 | 理由 |
|------|------|
| LLM 負荷 | `F[_]`, `where F: Functor` は thinking tokens を爆発させる |
| Canonicity | `list.map(f)` と `functor.map(list, f)` の二択が生まれる |
| 先例 | Go: HKT なし 15 年成功。Gleam: HKT なし 1.0。Rust GATs: 使用率 < 1% |

---

## 現状

```rust
// src/types.rs — 型コンストラクタがハードコード
enum Ty {
    Int, Float, String, Bool, Unit,
    List(Box<Ty>),                    // * -> *
    Option(Box<Ty>),                  // * -> *
    Result(Box<Ty>, Box<Ty>),         // * -> * -> *
    Map(Box<Ty>, Box<Ty>),            // * -> * -> *
    Tuple(Vec<Ty>),                   // *^n -> *
    Record { fields: Vec<(String, Ty)> },
    Fn(Vec<Ty>, Box<Ty>),
    Generic(String),                  // 型宣言のみ
    Unknown,
}
```

問題:
- 組み込み型とユーザー定義型が別コードパス
- 型チェッカーに `match Ty::List(...)`, `match Ty::Option(...)` が散在
- Stream Fusion がパターンマッチのヒューリスティックでしか書けない
- 新しい組み込み型を追加するたびに全 match を更新

---

## Phase 1: Ty 統一 (1.x — generic fn と同時)

### 目標

型コンストラクタを統一表現に。Kind 情報を付与。

### 設計

```rust
// --- 新しい Ty ---
enum Ty {
    // 基本型 (kind: *)
    Concrete(TypeId),                      // Int, String, Bool, Float, Unit

    // 型コンストラクタ適用 (kind: * — 適用後)
    Applied(TypeConstructorId, Vec<Ty>),   // List[Int], Result[A, B], Tree[Int]

    // 型変数 (推論用)
    Var(TypeVarId),

    // 関数型
    Fn(Vec<Ty>, Box<Ty>),

    // 構造型
    Tuple(Vec<Ty>),
    Record { name: Option<String>, fields: Vec<(String, Ty)> },
}

// --- 型コンストラクタ ---
struct TypeConstructor {
    id: TypeConstructorId,
    name: String,           // "List", "Option", "Result", "Tree"
    kind: Kind,             // * -> *, * -> * -> *, etc.
    origin: TypeOrigin,     // Builtin | UserDefined
}

// --- Kind ---
enum Kind {
    Star,                   // *        (具体型: Int, String)
    Arrow(Box<Kind>, Box<Kind>),  // * -> *  (List, Option)
}
// List  : * -> *
// Result: * -> * -> *
// Map   : * -> * -> *
// Tree  : * -> *
```

### マイグレーション

```
Ty::List(inner)           → Ty::Applied(TC_LIST, vec![inner])
Ty::Option(inner)         → Ty::Applied(TC_OPTION, vec![inner])
Ty::Result(ok, err)       → Ty::Applied(TC_RESULT, vec![ok, err])
Ty::Map(key, val)         → Ty::Applied(TC_MAP, vec![key, val])
Ty::UserDefined(name, args) → Ty::Applied(tc_id, args)
```

型チェッカーの `match` 分岐が大幅に減る:

```rust
// Before: 型ごとに個別処理
match ty {
    Ty::List(inner) => { /* List 用ロジック */ }
    Ty::Option(inner) => { /* Option 用ロジック */ }
    Ty::Result(ok, err) => { /* Result 用ロジック */ }
    // ... 増え続ける
}

// After: 統一処理
match ty {
    Ty::Applied(tc, args) => {
        // tc の Kind と algebraic properties で分岐
    }
}
```

### 影響範囲

| ファイル | 変更内容 |
|---------|---------|
| `src/types.rs` | Ty enum リファクタ、TypeConstructor 追加 |
| `src/check/*.rs` | match 分岐の統一 |
| `src/lower.rs` | Ty 生成の統一 |
| `src/ir.rs` | IR 型表現の更新 |
| `src/emit_rust/*.rs` | Ty → Rust 型名の変換更新 |
| `src/emit_ts/*.rs` | Ty → TS 型名の変換更新 |
| `src/generated/*.rs` | build.rs 生成コードの対応 |

---

## Phase 2: 代数法則テーブル (1.x)

### 目標

型コンストラクタに代数法則を持たせ、最適化パスが法則を参照できるようにする。

### 設計

```rust
// コンパイラ内部 — ユーザーには見えない
struct TypeConstructor {
    // ... Phase 1 のフィールド
    laws: Vec<AlgebraicLaw>,
}

enum AlgebraicLaw {
    /// map(f) >> map(g) = map(f >> g)
    FunctorComposition,
    /// map(id) = id
    FunctorIdentity,
    /// flat_map(f) >> flat_map(g) = flat_map(x => f(x).flat_map(g))
    MonadAssociativity,
    /// filter(p) >> filter(q) = filter(x => p(x) && q(x))
    FilterComposition,
    /// map(f) >> fold(init, g) = fold(init, (acc, x) => g(acc, f(x)))
    MapFoldFusion,
}

// 組み込み型の法則
fn register_builtin_laws(registry: &mut TypeRegistry) {
    registry.add_laws(TC_LIST, vec![
        FunctorComposition,    // map 融合
        FunctorIdentity,       // map(id) 消去
        FilterComposition,     // filter 融合
        MapFoldFusion,         // map+fold 融合
    ]);
    registry.add_laws(TC_OPTION, vec![
        FunctorComposition,    // option.map 融合
        FunctorIdentity,
        MonadAssociativity,    // flat_map チェーン融合
    ]);
    registry.add_laws(TC_RESULT, vec![
        FunctorComposition,    // result.map 融合
        FunctorIdentity,
    ]);
}
```

---

## Phase 3: Stream Fusion Nanopass (1.x-2.x)

### 目標

パイプチェーン (`|>`) の中間アロケーションを消滅させる IR 最適化パス。

### 実装

```rust
struct StreamFusionPass;

impl NanoPass for StreamFusionPass {
    fn name(&self) -> &str { "StreamFusion" }
    fn targets(&self) -> Option<Vec<Target>> { None } // 全ターゲット

    fn run(&self, program: &mut IrProgram, _target: Target) {
        for func in &mut program.functions {
            func.body = fuse_chains(func.body.clone());
        }
    }
}

fn fuse_chains(expr: IrExpr) -> IrExpr {
    // パイプチェーンを検出:
    //   list |> map(f) |> filter(p) |> fold(init, g)
    //
    // 代数法則テーブルを参照:
    //   MapFoldFusion: map(f) >> fold(init, g) = fold(init, (acc, x) => g(acc, f(x)))
    //   FilterComposition: filter(p) >> filter(q) = filter(x => p(x) && q(x))
    //
    // 融合後の IR:
    //   ForIn { list, body: [
    //     let v = f(x);
    //     if p(v) { acc = g(acc, v); }
    //   ]}
}
```

### 性能効果

| パターン | Before | After | 改善 |
|---------|--------|-------|------|
| `map \|> filter \|> fold` (1M) | ~12ms, 3 alloc | ~3ms, 0 alloc | **4x** |
| `map \|> map \|> map` | 3 alloc | 0 alloc | **メモリ 1/4** |
| `option.map \|> map \|> unwrap_or` | 3 enum ops | 1 match | **3x** |
| `result.map \|> map_err \|> unwrap` | 3 enum ops | 1 match | **3x** |

---

## Phase 4: Effect 型統合 (2.x)

### 目標

Effect 推論を型レベル表現に昇格し、Trait system と統合。

### 設計

```rust
// Effect を型コンストラクタとして扱う (内部表現)
// ユーザーは引き続き `effect fn` だけ書く

enum Effect {
    IO,     // fs, path
    Net,    // http, url
    Env,    // env, process
    Time,   // time, datetime
    Rand,   // math.random
    Fan,    // fan (concurrency)
    Log,    // log
}

// 関数の型に effect 情報を付与 (内部的に)
struct FnType {
    params: Vec<Ty>,
    ret: Ty,
    effects: EffectSet,  // コンパイラが自動推論
}

// almide.toml での制限
// [dependencies.lib]
// allow = ["Net"]  ← IO は禁止
```

### almide check --effects

```
$ almide check --effects src/server.almd

src/server.almd:
  fn handle_request  → {Net, IO, Log}
  fn parse_config    → {IO}
  fn validate_input  → {} (pure)
```

---

## Phase 5: Trait 統合 (2.x)

### 目標

Phase 1-2 の基盤上に Trait system を構築。HKT 表現力が内部にあるため、Trait が最初から強力。

### 内部表現 (ユーザーに見せるかは別判断)

```rust
// コンパイラが内部で持つ trait 定義
trait Mappable for TypeConstructor where Kind = * -> * {
    fn map[A, B](self: F[A], f: fn(A) -> B) -> F[B]
}

// 自動実装: List, Option, Result は Mappable
// ユーザー定義型: type Tree[T] = ... も Kind: * -> * なら自動で Mappable

// ユーザーから見えるのは:
//   list.map(f)     — 今まで通り
//   option.map(f)   — 今まで通り
//   tree.map(f)     — 新規: ユーザー定義型でも map が使える！
```

---

## 全体タイムライン

```
Phase 1: Ty 統一                    ← 1.x (generic fn と同時)
  Ty::List → Ty::Applied(LIST, [..])
  Kind 情報付与
  型チェッカー簡素化

Phase 2: 代数法則テーブル           ← 1.x
  Functor/Foldable/Filterable 法則
  型コンストラクタに法則を持たせる

Phase 3: Stream Fusion Nanopass     ← 1.x-2.x
  パイプチェーンの中間 alloc 消滅
  法則に基づく正当な最適化

Phase 4: Effect 型統合              ← 2.x
  Effect set を型レベルに昇格
  almide.toml での capability 制限
  almide check --effects

Phase 5: Trait 統合                 ← 2.x
  内部 HKT 表現上に Trait を構築
  ユーザー定義型の自動 Mappable 等
```

---

## 先例

| 言語 | アプローチ | 結果 |
|------|-----------|------|
| Haskell | HKT + type class をユーザーに全公開 | 強力だが学習曲線が急 |
| Rust | GATs で限定的 HKT | 使用率 < 1%、大半のユーザーに不要 |
| Swift | Protocol + associated type (内部 HKT 相当) | ユーザーは `some Collection` だけ |
| Go | interface 内部最適化 (escape analysis) | ユーザーは interface だけ |
| **Almide** | **内部 HKT + 代数法則。ユーザーに見せない** | **シンプル × 高速** |

---

## 依存関係

```
user-generics-and-traits.md
  └── Phase 2: Built-in Bounds → HKT Foundation Phase 1 と同時進行
  └── Phase 3: Trait/Impl     → HKT Foundation Phase 5 に統合

security-model.md
  └── Layer 2: Capability     → HKT Foundation Phase 4 (Effect 型統合)

GRAND_PLAN.md
  └── Phase 3: Runtime Foundation → HKT Foundation Phase 4-5 が基盤
```

---

## 一文で

> 型コンストラクタを統一し代数法則を持たせることで、Stream Fusion・Effect 推論・Trait system の全てが数学的に正当な基盤の上に乗る。ユーザーのコードは一切変わらない。
