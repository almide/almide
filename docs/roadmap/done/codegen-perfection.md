<!-- description: Make codegen bulletproof by learning from Gleam/Roc architecture patterns -->
<!-- done: 2026-04-01 -->
# Codegen Perfection

他言語コンパイラ (Elm, Gleam, Roc) の codegen アーキテクチャを分析し、Almide の codegen を「バグが構造的に発生しない」レベルに引き上げる。

---

## 現状の問題

今日のシナリオテストで発見された codegen バグ 5 件は全て同じ構造的原因を持つ:

1. **Record の Fn フィールド** → `impl Fn(...)` が struct に入る (Rust では不正)
2. **effect fn の二重アンラップ** → test codegen が Result の扱いを間違える (2件)
3. **result.collect! の型ミスマッチ** → pipe + unwrap の型伝搬が不完全

**共通原因**: walker (codegen) が IR ノードの型情報を信頼せず、場当たり的にコードを生成している。

---

## 他言語から学んだパターン

### Gleam: Typed IR + Snapshot Testing

- **全 IR ノードに `Arc<Type>`** — codegen は型を再計算しない
- **Snapshot テスト 98 件** — .gleam → .erl / .js の入出力を全ファイルで検証
- **`insta` crate** で snapshot を自動管理
- codegen 関数は `Document` を返す（失敗しない）

### Roc: Layout-Typed LIR + Nanopass

- **MIR → LIR** の 2 段階 IR — LIR は全ノードに `layout.Idx` を持つ
- **OwnershipNormalize** パス — 所有権を codegen の前に正規化
- **RC Insertion** パス — 参照カウントを codegen と分離
- 各パスが不変条件を検証してから次に渡す

### Elm: Decision Tree

- パターンマッチを **決定木** にコンパイル — codegen は分岐を生成するだけ
- 最適化フェーズで全てのパターンが解決済み

---

## Almide の現状との差分

| 観点 | Gleam | Roc | Almide (現在) |
|------|-------|-----|--------------|
| IR の型情報 | 全ノードに Arc\<Type\> | 全ノードに layout.Idx | 全ノードに Ty (✅ 同等) |
| Codegen テスト | Snapshot 98件 | Snapshot + hex dump | ❌ Snapshot なし |
| IR 検証パス | 暗黙的 (型が保証) | 明示的 (各パス後に検証) | verify_program あり (△ 部分的) |
| 所有権の扱い | N/A (GC言語) | 専用パスで正規化 | BorrowInsertion + CloneInsertion (✅) |
| Nanopass | 1-2 パス | 6+ パス | 16 パス (Rust target) (✅) |
| 型→コード変換 | 直接 (型見て emit) | layout 経由 | template + walker 混在 (△) |

**Almide が足りないもの**: Snapshot テスト、パス間の不変条件検証

---

## 計画

### Phase 1: Snapshot テスト導入

Gleam の `assert_erl!` マクロに相当するものを Almide に導入する。

```rust
// tests/codegen_snapshot_test.rs
macro_rules! assert_rust {
    ($src:expr) => {{
        let rs = compile_to_rust($src);
        insta::assert_snapshot!(rs);
    }};
}

#[test]
fn record_with_fn_field() {
    assert_rust!(r#"
type Handler = { run: (String) -> String, name: String }
fn make(n: String) -> Handler = { run: (x) => n + x, name: n }
    "#);
}

#[test]
fn effect_fn_match_in_test() {
    assert_rust!(r#"
effect fn parse(s: String) -> Result[Int, String] = int.parse(s)!
test "x" { match parse("42") { ok(n) => assert_eq(n, 42), err(_) => assert(false) } }
    "#);
}
```

**カバーすべきパターン** (今日のバグから):

| パターン | 今日のバグ |
|----------|-----------|
| Record with Fn field | `impl Fn` → `Rc<dyn Fn>` |
| effect fn in test block (direct match) | 二重アンラップ |
| effect fn in test block (let bind + match) | 二重アンラップ |
| pipe + unwrap (result.collect!) | 型ミスマッチ |
| multi-line variant definition | パーサー (codegen ではないが) |

**Effort**: S (insta クレートは Cargo.toml に追加するだけ)

### Phase 2: パス間の IR 検証強化

`verify_program` を各 nanopass の間に挿入する。

```rust
// src/codegen/pass.rs の Pipeline::run() を修正
pub fn run(&self, program: IrProgram, target: Target) -> IrProgram {
    let mut program = program;
    for pass in &self.passes {
        program = pass.run(program, target);
        #[cfg(debug_assertions)]
        verify_program(&program).unwrap_or_else(|e| {
            panic!("IR verification failed after pass '{}': {:?}", pass.name(), e);
        });
    }
    program
}
```

**検証項目**:
- TypeVar が残っていない (既存)
- 全 VarId が定義されている
- Fn 型のフィールドが正しく表現されている
- Result 型の unwrap が二重になっていない
- Call target が全て解決されている

**Effort**: M

### Phase 3: Walker の型駆動化

現在の walker は IR ノードの `kind` でパターンマッチして Rust コードを生成している。型情報を見ていない箇所がある。

**目標**: walker の全 `match` 分岐で、ノードの `.ty` を使ってコード生成を決定する。

具体例:
```rust
// 現在: kind だけ見る
IrExprKind::Record { fields } => {
    // fields をそのまま emit → Fn フィールドが impl Fn になるバグ
}

// 理想: ty も見る  
IrExprKind::Record { fields } => {
    for (name, value) in fields {
        let field_ty = record_field_type(&expr.ty, name);
        if field_ty.is_fn() {
            // Rc<dyn Fn(...)> で emit
        } else {
            // 通常 emit
        }
    }
}
```

**Effort**: L (全 walker 関数を走査して型駆動に書き換え)

### Phase 4: Codegen の型安全な出力表現

Gleam の `Document` 型のように、生成コードを型安全な中間表現として構築する。

```rust
// 現在: 文字列連結
format!("pub struct {} {{ {} }}", name, fields.join(", "))

// 理想: 構造化された出力
RustItem::Struct {
    name,
    derives: vec![Derive::Clone],
    fields: fields.iter().map(|f| RustField {
        name: f.name,
        ty: rust_type(&f.ty),  // ← 型変換が 1 箇所に集約
    }).collect(),
}
```

**メリット**: 型変換ロジックが `rust_type()` に集約され、`impl Fn` → `Rc<dyn Fn>` のような変換が 1 箇所で管理できる。

**Effort**: XL (大規模リファクタ)

---

## 優先順位

| Phase | 効果 | Effort | 依存 |
|-------|------|--------|------|
| 1. Snapshot テスト | バグの **検出** | S | なし |
| 2. パス間検証 | バグの **早期発見** | M | なし |
| 3. Walker 型駆動化 | バグの **構造的排除** | L | なし |
| 4. 型安全出力表現 | バグの **原理的排除** | XL | Phase 3 |

**Phase 1 が最も費用対効果が高い。** 今日発見した 5 件のバグは全て snapshot テストがあれば即座に検出できた。

---

## 参考実装

- **Gleam snapshot**: `_references/lang/gleam/compiler-core/src/erlang/tests.rs`
- **Gleam assert_erl!**: 同ファイルの `assert_erl!` マクロ
- **Roc IR 定義**: `_references/lang/roc/src/lir/LIR.zig`
- **Roc 所有権正規化**: `_references/lang/roc/src/lir/OwnershipNormalize.zig`
- **Elm 決定木**: `_references/lang/elm/compiler/src/Optimize/DecisionTree.hs`
