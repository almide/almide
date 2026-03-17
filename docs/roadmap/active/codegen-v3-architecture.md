# Codegen v3: 三層アーキテクチャ

**優先度:** High — 1.0 後の target 拡張（Go, Python）の前提条件
**見積り:** 3–4 週間（段階的移行）
**ブランチ:** develop

## 動機

現行 codegen は ~4000 LOC の手続きコード。Rust と TS の lowering が ~400 行の並列コピペで、Option/Result の意味差が暗黙的に散在し、特殊ケースが各所にハードコードされている。

3 つ目の target を追加すると、この重複が 3 倍になる。その前にアーキテクチャを刷新する。

## 調査した先行事例

| コンパイラ | アプローチ | 学び |
|---|---|---|
| **Gleam** | target ごとに独立 codegen + 共通 interface | シンプルで 2 target なら十分。ランタイムなし JS 生成 |
| **Haxe/Reflaxe** | Plugin trait: `compileExprImpl()` を実装するだけ | target 追加コスト最小。3 メソッドで新 target が作れる |
| **Kotlin K2** | 統一 IR + target backend | 共通最適化が可能。ただし規模が巨大 |
| **MLIR** | 多段 IR + progressive lowering | 高→中→低の段階的変換。各レベルで最適化可能 |

## 設計: 三層パイプライン

```
IrProgram (型付き IR — 現行そのまま)
    ↓
Layer 1: Core IR（target 無関係な正規化）
    ↓
Layer 2: Semantic Rewrite（target 固有の意味変換 — Plugin）
    ↓
Layer 3: Template Renderer（構文出力 — TOML 駆動）
    ↓
Rust / TypeScript / Go / Python ソースコード
```

### Layer 1: Core IR

現行 `IrProgram` をそのまま使う。追加の IR は作らない。

**やること:**
- target 無関係な正規化パスを追加（定数畳み込み、dead code 除去など）
- 現行 `lower.rs` の AST → IR 変換はそのまま

**やらないこと:**
- 新しい IR 型の定義（現行 `IrExpr`, `IrStmt` で十分）

### Layer 2: Semantic Rewrite（Plugin）

target 固有の **意味変換** をここに集約。trait ベースで実装。

```rust
trait SemanticRewrite {
    /// Option[T] の表現を target に合わせて変換
    fn rewrite_option_some(&self, inner: &IrExpr) -> RewrittenExpr;
    fn rewrite_option_none(&self, ty: &Ty) -> RewrittenExpr;

    /// Result[T, E] の表現を変換
    fn rewrite_result_ok(&self, inner: &IrExpr) -> RewrittenExpr;
    fn rewrite_result_err(&self, inner: &IrExpr) -> RewrittenExpr;

    /// auto-? 伝播: effect fn 内の Result 呼び出し
    fn rewrite_effect_call(&self, call: &IrExpr) -> RewrittenExpr;

    /// 所有権・借用（Rust のみ、他 target は no-op）
    fn rewrite_ownership(&self, expr: &IrExpr, ctx: &OwnershipCtx) -> RewrittenExpr;

    /// 並行処理: fan { } の変換
    fn rewrite_fan(&self, exprs: &[IrExpr]) -> RewrittenExpr;

    /// 型の表現: 再帰型の Box 化、anonymous record の具象化
    fn rewrite_type(&self, ty: &Ty) -> TargetType;
}
```

**Rust plugin が担当するもの (~20-30%):**
- Borrow analysis + clone 挿入
- `Result` → `?` 伝播
- `None` → `None::<T>` 型注釈
- 再帰型の `Box` 化
- `fan` → `std::thread::scope`
- top-level `let` → `LazyLock`
- string match → `.as_str()`
- power 演算 → `.pow()` / `.powf()`

**TS plugin が担当するもの (~10-15%):**
- `Option` erasure: `some(x)` → `x`, `none` → `null`
- `Result` のラッパー処理
- `fan` → `Promise.all`
- async/await 変換

**Go plugin（将来）:**
- `Option` → `(T, bool)` タプル
- `Result` → `(T, error)` タプル
- goroutine / channel 変換

### Layer 3: Template Renderer（TOML 駆動）

構文の **見た目だけ** を TOML で定義。意味変換は一切行わない。

```toml
# codegen/templates/rust.toml

[if_expr]
template = "if {cond} {{ {then} }} else {{ {else} }}"

[match_expr]
template = "match {subject} {{ {arms} }}"

[match_arm]
template = "{pattern} => {{ {body} }}"

[fn_decl]
template = "fn {name}({params}) -> {return_type} {{ {body} }}"

[fn_param]
template = "{name}: {type}"

[record_literal]
template = "{type_name} {{ {fields} }}"

[record_field]
template = "{name}: {value}"

[let_binding]
template = "let {name}: {type} = {value};"

[var_binding]
template = "let mut {name}: {type} = {value};"

[pipe]
# pipe は展開済み（Semantic Rewrite で関数呼び出しに変換）
template = "{callee}({args})"

[call]
template = "{callee}({args})"

[binary_op]
template = "({left} {op} {right})"

[string_interpolation]
template = "format!(\"{format_str}\", {args})"

[list_literal]
template = "vec![{elements}]"

[some]
template = "Some({inner})"

[none]
template = "None"
```

```toml
# codegen/templates/typescript.toml

[if_expr]
template = "{cond} ? {then} : {else}"
# multi-line variant
block_template = "if ({cond}) {{ {then} }} else {{ {else} }}"

[match_expr]
# TS has no native match — emit as if/else chain or switch
template = "(() => {{ {arms} }})()"

[fn_decl]
template = "function {name}({params}): {return_type} {{ {body} }}"

[let_binding]
template = "const {name}: {type} = {value};"

[var_binding]
template = "let {name}: {type} = {value};"

[call]
template = "{callee}({args})"

[binary_op]
template = "({left} {op} {right})"

[string_interpolation]
template = "`{template_str}`"

[list_literal]
template = "[{elements}]"

# some/none は Semantic Rewrite で消去済みなので template 不要
```

**stdlib 関数の TOML（現行そのまま活用）:**

```toml
# stdlib/defs/list.toml — 変更なし
[map]
params = [{ name = "xs", type = "List[A]" }, { name = "f", type = "Fn[A] -> B" }]
return = "List[B]"
rust = "almide_rt_list_map(({xs}).to_vec(), |{f.args}| {{ {f.body} }})"
ts = "__almd_list.map({xs}, ({f.args}) => {f.body})"
```

## 移行計画

### Phase 1: Template 抽出（1 週間）

現行 `render.rs` と `render_common.rs` のパターンマッチを TOML テンプレートに抽出。

1. `codegen/templates/rust.toml` を作成
2. `codegen/templates/typescript.toml` を作成
3. テンプレートエンジン実装（TOML → `format!` 展開）
4. 現行 render を template-driven に段階的に置き換え
5. 各ステップで `almide test` が pass することを確認

**成果:** render 層が宣言的になる。target 追加時にテンプレート TOML を書くだけ。

### Phase 2: Semantic Rewrite 分離（1 週間）

現行 `lower_rust_expr.rs` と `lower_ts.rs` から意味変換ロジックを抽出。

1. `SemanticRewrite` trait を定義
2. `RustRewrite` を実装: borrow, auto-?, boxing, fan
3. `TsRewrite` を実装: Option erasure, Result handling, async
4. 共通ロジック（BinOp, let, call, match の構造）を shared walker に移動
5. 現行 lower を trait 経由に段階的に置き換え

**成果:** lower の重複 ~400 LOC が解消。target 固有ロジックが明確に分離。

### Phase 3: 統合テスト + AnonRecord 修正（1 週間）

1. AnonRecord codegen バグを修正（空リストの型パラメータ）
2. Grammar Lab `optional-handling` 実験で全 30/30 PASS を確認
3. spec/lang/ + spec/stdlib/ の全テストが pass
4. target 間の意味差分をドキュメント化

**成果:** codegen の正確性が向上。Grammar Lab survival rate が上がる。

### Phase 4: 新 target のプロトタイプ（1 週間、optional）

1. `codegen/templates/go.toml` を作成
2. `GoRewrite` を実装（Option → tuple, Result → error）
3. 基本的な spec/lang テストが Go target で pass

**成果:** 三層アーキテクチャが実際に target 追加を簡単にすることを実証。

## ディレクトリ構成（移行後）

```
src/
├── ir.rs                    現行そのまま（Layer 1: Core IR）
├── lower.rs                 現行そのまま（AST → IR）
├── codegen/
│   ├── mod.rs               統一エントリポイント
│   ├── rewrite.rs           SemanticRewrite trait 定義
│   ├── rewrite_rust.rs      Rust 固有の意味変換 (Layer 2)
│   ├── rewrite_ts.rs        TS 固有の意味変換 (Layer 2)
│   ├── walker.rs            共通 IR 走査 + template 呼び出し
│   ├── template.rs          TOML テンプレートエンジン (Layer 3)
│   └── templates/
│       ├── rust.toml         Rust 構文テンプレート
│       ├── typescript.toml   TS 構文テンプレート
│       └── go.toml           Go 構文テンプレート (将来)
├── emit_rust/               段階的に codegen/ へ移行、最終的に削除
├── emit_ts/                 同上
├── generated/               stdlib TOML → dispatch（現行そのまま）
└── emit_common.rs           codegen/walker.rs に統合
```

## 成功基準

- [ ] `almide test` の全テストが pass（Rust + TS 両 target）
- [ ] render 層の LOC が 50% 以下に削減（~700 → ~350）
- [ ] lower の重複が解消（共通 walker + target trait）
- [ ] 新 target 追加に必要な LOC: TOML テンプレート (~100 行) + Semantic Plugin (~200 行)
- [ ] Grammar Lab survival rate が codegen バグで下がらない

## リスク

| リスク | 対策 |
|--------|------|
| テンプレートで表現できない構文パターンが多い | escape hatch: テンプレートに `custom` フィールドを追加し、Rust コードにフォールバック |
| Semantic Rewrite の trait が肥大化 | メソッドをカテゴリ別に分割: `OptionRewrite`, `OwnershipRewrite`, `ConcurrencyRewrite` |
| 段階的移行中にリグレッション | Phase ごとに全テスト実行。revert 可能な粒度でコミット |
| TOML テンプレートのパース性能 | build.rs でコンパイル時に Rust コードに変換（現行 stdlib TOML と同じ） |

## 参考

- [Gleam compiler](https://github.com/gleam-lang/gleam) — 独立 codegen + 共通 interface
- [Haxe Reflaxe](https://github.com/SomeRanDev/reflaxe) — Plugin trait で target 追加
- [MLIR](https://mlir.llvm.org/) — Progressive lowering / multi-level IR
- [Kotlin K2](https://blog.jetbrains.com/kotlin/2025/05/kotlinconf-2025-language-features-ai-powered-development-and-kotlin-multiplatform/) — 統一 IR + compiler plugin API
