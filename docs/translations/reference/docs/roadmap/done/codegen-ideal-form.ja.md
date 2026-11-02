説明：宣言的 dispatch と明示的 symbol 解決へ向けた WASM codegen 再設計
歴史的完了メタデータ：2026-04-19。
<a id="codegen-ideal-form"></a>

# Codegen の理想形

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](codegen-ideal-form.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](codegen-ideal-form.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](codegen-ideal-form.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/codegen-ideal-form.md)

> 原文上の注意：これは 4 月 13–19 日の歴史的な設計・完了記録であり、現在の実装監査ではない。完了には後続マイルストーンへ分割した作業も含まれ、理想形の全目標を実装した意味ではない。初期の無害な default という説明と残存するレイアウトリスク、VarTable の範囲変更、延期と完了の各段階、異なる件数見積もりは別々の歴史層として保持する。理想のシンボル解決・エラー設計と、記録された Named 型・panic の実装は異なる。原文の到達不能・証明済みという表現は記録された有限テストの根拠を述べており、独立に確立された普遍的証明ではない。後続のディスパッチ・定義表統合は完了作業と区別する。

> **作業状態（2026-04-19）：CLOSED。** 全7項目は取り込み済み、または独立作業へ分割済み。各項目の取り込みと意図的に対象外とした残る後続作業は、下の「最終状態」を参照。

v0.13 シリーズで継ぎ足した codegen を、**理想形**に向けて段階的にリファクタする。今回の nn/WASM 対応で表面化したバグ群 (型解決の散逸、test関数の name衝突、lifted closure の var_table 混線、stdlib emit の `list_elem_ty` 20箇所バグ) は、すべて**設計レベルの腐り**が根本原因。

<a id="動機"></a>

## 動機

今日 nn の WASM 対応で踏んだ地雷を列挙すると、全て「**場当たり修正が正しい場所で行われていない**」が共通項:

| 症状 | 真の原因 | 対症療法 |
|---|---|---|
| nested lambda の型が未解決 | Closure Conversion が型推論を内包 | `has_deep_unresolved`, VarTable 優先 |
| `p.0 * p.1` が `MulInt` | op lowering が BinOp を固定、emit で補正 | `fix_binop_type` |
| `list.slice(row, ...)` が i32 として要素コピー | `args[0].ty` が stale | 20箇所を `resolve_list_elem` に置換 |
| `t.broadcast_add` が test関数にジャンプ | test関数の name prefix が module 登録で漏れる | `is_test` 判定追加 |
| lifted closure の VarId 壊れる | program.var_table と module.var_table の境界管理 | module の lifted は module.functions に留置 |
| 関数未解決 → 実行時 `unreachable` | resolve の責務がコンパイル時に閉じていない | (未解決) |

どれも「**もう一箇所直せば出る**」状態。次の同じバグは時間の問題。

<a id="ターゲット-理想形"></a>

## ターゲット: 理想形

<a id="1-関数解決を独立パスに-最優先"></a>

### 1. 関数解決を独立パスに (最優先)

**現状**: WASM emit で `CallTarget::Module { module, func }` を見て、`almide_rt_{module}_{func}` を `func_map` から引く。見つからなければ bare name fallback → それでもダメなら `emit_stub_call` が **実行時 `unreachable`** を吐く。つまり「型チェック通った、build通った、WASM validation通った、実行時 trap」という最悪のUX。

**理想**:

- `pass_resolve_calls.rs` (新規 nanopass、Rust/WASM 両方に適用)
- 全 `CallTarget::Module` / `CallTarget::Named` を walk
- module 名を canonical 化 (alias → 実モジュール、nn.tensor vs tensor 正規化)
- 解決先の `IrFunction` への参照 (`Sym`) に書き換え → `CallTarget::Resolved { func_ref: Sym }` のような新バリアント
- 解決不能な呼び出しは**コンパイル時エラー** (diagnostics で「Did you mean...?」付き)
- emit は既知の symbol を call するだけ、`emit_stub_call` は**削除**

事後条件： `CallTarget::Module` / bare `CallTarget::Named` は全て `Resolved` に書き換わっている。

<a id="2-stdlib-関数の宣言駆動化"></a>

### 2. Stdlib 関数の宣言駆動化

**現状**: `emit_wasm/calls.rs` (1000行+), `calls_list.rs` (1000行+), `calls_list_closure.rs`, `calls_list_closure2.rs`, `calls_list_helpers.rs`, `calls_map.rs`, `calls_option.rs`, `calls_set.rs` — match 文の山。同じ `elem_ty = self.list_elem_ty(&args[0].ty)` パターンを 20+ 箇所で書いて、1 箇所バグ修正で 20 箇所直す羽目になった。

**理想**:

- Rust ターゲットは既に TOML 駆動 (`stdlib/defs/*.toml`)。WASM も同じ方式。
- 各 stdlib 関数 = 宣言的記述:
  ```toml
  [list.slice]
  params = ["list", "int", "int"]
  ret = "list_of[0]"  # 型式
  elem_source = "arg[0]"  # 要素型をどこから取るか
  emit = """
  # alloc: 4 + new_len * elem_size
  # loop: copy [start..end] from src to dst
  ...
  """
  ```

原 emit sketch の英語注釈訳：4 + new_len × elem_size を確保し、src の [start..end] を dst へ copy する loop。
- Emitter は宣言から WASM を生成する**汎用エンジン**。stdlib ごとの個別コードは消える。
- 「要素型をどこから取るか」は宣言の一部 → `list_elem_ty` vs `resolve_list_elem` の選択ミスが起こらない。

効果: emit_wasm の calls*.rs がごっそり消える (推定 -4000 行)。

<a id="3-irmutvisitor-trait"></a>

### 3. `IrMutVisitor` trait

**現状**: `rewrite_var_ids` は 200行の手動 match。新 IrExprKind バリアント追加のたびに壊れる。同じパターンが `pass_*.rs` の多くに散在。

**理想**:

- `almide-ir` に `IrMutVisitor` trait を追加:
  ```rust
  pub trait IrMutVisitor {
      fn visit_expr_mut(&mut self, expr: &mut IrExpr) { walk_expr_mut(self, expr); }
      fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) { walk_stmt_mut(self, stmt); }
      fn visit_pattern_mut(&mut self, p: &mut IrPattern) { walk_pattern_mut(self, p); }
  }
  pub fn walk_expr_mut<V: IrMutVisitor + ?Sized>(v: &mut V, expr: &mut IrExpr) { /* recurse */ }
  ```

原コード注釈 recurse は再帰走査を表す。
- `rewrite_var_ids` は 30 行になる:
  ```rust
  impl IrMutVisitor for VarIdRewriter {
      fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
          if let IrExprKind::Var { id } = &mut expr.kind {
              if let Some(new) = self.mapping.get(id) { *id = *new; }
          }
          walk_expr_mut(self, expr);
      }
  }
  ```
- 既存の `IrVisitor` (read-only) と対称的な設計。

<a id="4-型解決の一元化"></a>

### 4. 型解決の一元化

**現状**: 型解決が複数レイヤーに散らばる:
- `LambdaTypeResolve` pass (top-down)
- `propagation.rs` (mono)
- `emit_wasm/closures.rs::resolve_expr_ty` (emit 時)
- `emit_wasm/collections.rs::emit_tuple_index` (VarTable vs expr.ty 優先順序判定)
- 各 `calls_*.rs` の `resolve_list_elem`

同じ「`p.0` の型は何か」を 3 箇所で解く。整合性が取れずバグる。

**理想**:

- `LambdaTypeResolve` の後に **`TypeConcretization` pass** を走らせ、全 IrExpr の `.ty` を VarTable と整合させる。
- 事後条件： 実行可能パス上に `Ty::Unknown` / `Ty::TypeVar` は一切残らない。
- Emit 側は `expr.ty` を**信頼するだけ**。VarTable を引く必要がない。
- `resolve_expr_ty` / `resolve_list_elem` のような emit 時 type lookup ヘルパーは削除可能。

これが達成できれば、今回の `is_unresolved_structural()` vs `has_deep_unresolved()` vs `is_unresolved()` の地獄が解消する。

**残 gap (v0.14.7 時点、21 件 on WASM `ALMIDE_CHECK_IR=1` audit)**:

`ALMIDE_CHECK_IR=1` で spec/ を WASM 走らせると 15-21 件が audit 違反で panic。内訳:

| pattern | 例 | 件数 |
|---|---|---|
| empty list/collection literal の element 型逆推論失敗 | `fn fan.map empty list: List[Unknown]` / `fn empty for: List[Unknown]` | 10+ |
| Codec auto-derive の empty field | `fn decode_container: List[Unknown]` / `fn derive Codec: empty list field` | 3-4 |
| Result/Option ok/some() で inner 型未決 | `fn safe_div: Result[Unknown, String]` / `fn validate_age` | 2-3 |
| OpenRecord → concrete record 解決未 | `fn chain_b__Unknown: OpenRecord { name: String }` | 1 |
| 深い generic context | `fn is_balanced: Option[List[Unknown]]` | 1 |
| Event/Tuple empty payload | `fn extract_click_positions: List[Tuple[Unknown, Unknown]]` | 1 |

default (no env var) では WASM emit 側の defensive fallback
(`resolve_list_elem` の 4 段 fallback、`Unknown` → i32 assumption) が
吸収するので 203/206 pass。潜在 risk: empty list が i32 element
alloc されて後から i64 要素 push で layout 不整合、等。spec test に
該当 pattern が無いだけで user code で踏み得る。

**Fix の分解 (0.14.8 以降の sub-phase 候補)**:

- (a) Empty literal の consumer 逆推論: `fn f(xs: List[Int]) = f([])`
  で `[]: List[Int]` を引き当てる。checker 拡張、半日〜1日
- (b) `ok(x)` / `some(x)` wrapper の inner 型推論: wrapper の引数型
  を wrapper の expected ret_ty から逆引き。半日
- (c) Codec auto-derive の empty field: `derive.rs` で empty list
  field の element type を default に固定。1-2時間
- (d) OpenRecord → concrete resolve: checker の open → closed 解決
  path 整理。数日
- (e) Event/Tuple empty payload: variant constructor の payload
  types を variant sig から伝播。半日

(a) + (b) + (c) で 17/21 、(d) + (e) で残 4。total 1 週間 scope。

**当面のスタンス**: default では無害なので release blocker ではない。
dojo measurement で user code に顕在化するパターンが出たら priority
上げて潰す。stdlib-declarative-unification arc の合間に進めるのが
自然 (両者とも WASM emit の精度向上という共通方向)。

<a id="5-vartable-の責務明確化"></a>

### 5. VarTable の責務明確化

**現状**:
- `program.var_table` と `module.var_table` が**別物**
- ClosureConversion の lifted closure は「どこに置くか」で VarId 参照先が壊れる (今日修正)
- テストや WASM emit 中で「この関数の var_table はどれ?」を毎回判断

**理想** (2択):

A) **VarId に region 情報**: `VarId(ModuleId, u32)` にして、lookup は `program.lookup(var_id)` 一本化。
   - 利点: 引数の受け渡しがシンプル
   - 欠点: VarId のサイズが増える (u32 → u64 相当)

B) **関数ごとの var_table**: `IrFunction.var_table` を持たせ、関数スコープに閉じる。
   - 利点: VarId の局所性が明確
   - 欠点: 関数境界をまたぐ VarId 参照 (closure capture) の書き換えが必要

どちらかを選んで統一。B のほうが「関数 = 閉じた単位」という原則に沿う。

<a id="6-test-namespace-完全分離"></a>

### 6. Test namespace 完全分離

**現状**: `fn broadcast_add` と `test "broadcast_add"` が同じ `func.name` を持つ → 登録時に衝突。今日は `is_test` チェックを3箇所追加して対応。

**理想**:
- Parser / lowering の段階で test の名前は `__test::{name}` のような常に衝突しない形に正規化。
- `IrFunction.is_test` は残すが、**名前は既に unique**。下流の全パスで衝突判定が不要。

<a id="7-emit_stub_call-廃止"></a>

### 7. `emit_stub_call` 廃止

**現状**: 解決不能な呼び出しを `drop args; unreachable` で trap させる。
- validation 通る → 実行時 trap
- どの call が未解決かデバッグ困難
- 型・関数のレイヤーが分離していない証拠

**理想**: 上記 #1 で関数解決が独立パスになれば、**emit 時点で未解決は存在しない**。`emit_stub_call` 自体を削除。

<a id="実装順-推奨"></a>

## 実装順 (推奨)

1. **#1 関数解決パス** — 最大の価値 (実行時 trap → compile error)、独立性高い、他の改修の基盤
2. **#3 IrMutVisitor** — 機械的作業、他パスのリファクタを楽にする
3. **#4 TypeConcretization** — #1, #3 ができれば書きやすい
4. **#5 VarTable 統合** — #4 と相互依存、一緒にやる
5. **#6 Test namespace** — 単発、いつでも可
6. **#2 Stdlib 宣言駆動** — 最大の行数削減だが、慎重に段階的に
7. **#7 emit_stub_call 廃止** — #1 の副産物

<a id="進捗ログ"></a>

## 進捗ログ

- **2026-04-13**: #1 (Phase 1a) 着手
  - `pass_resolve_calls.rs` 追加。Rust/WASM 両パイプラインに組み込み
  - Postcondition `Custom(verify_all_calls_resolved)` で user module への未解決呼び出しを検出
  - Phase 1b (IR 書き換え) と 1c (`emit_stub_call` 削除) は今後
- **2026-04-13**: #3 着手
  - `crates/almide-ir/src/visit_mut.rs` 新規。`IrMutVisitor` trait + `walk_expr_mut` / `walk_stmt_mut` / `walk_pattern_mut`
  - `pass_closure_conversion.rs::rewrite_var_ids` を 160行 → 60行に短縮
  - 全 183 spec テスト + nn WASM 60 テスト 通過
- **2026-04-13**: #4 着手 (ConcretizeTypes pass)
  - `crates/almide-codegen/src/pass_concretize_types.rs` 新規。bottom-up で全 IrExpr.ty を concrete 化
  - BinOp 正規化: `AddInt` on Float operands → `AddFloat` を IR レベルで修正
  - 効果: `emit_wasm/expressions.rs::fix_binop_type` (20行の補正関数) を **完全削除**
  - `ALMIDE_CHECK_IR=1` で IR verify 通過 (以前は AddInt/Float 型不一致で破綻)

- **2026-04-13**: #4 拡張 — Call 戻り値解決 + canonical `has_unresolved_deep`
  - `Ty::has_unresolved_deep()` を `almide-types` に追加、場当たり 2コピーを削除
    - これまで `is_unresolved()` / `is_unresolved_structural()` / `has_deep_unresolved()` の3種類があって、誤用で今日3回バグった
  - ConcretizeTypes に **SymbolTable** を追加: `(module, func) → ret_ty` をビルド
    - ユーザーモジュール関数 (`tensor.broadcast_add` など) の戻り値を直接解決
    - stdlib `list.*` (map/filter/zip/fold/reduce ほか) も型式で解決
  - 効果:
    - `ALMIDE_AUDIT_TYPES=1` で nn 全モジュール中 6/7 が warn ゼロ (gguf の `Option[Unknown]` 6件のみ残、実害なし)
    - `resolve_list_elem` の 4段 fallback chain を **2段に簡素化** (ConcretizeTypes を信頼できるようになった)
  - 全 183 spec + nn WASM 60 テスト 通過

- **2026-04-13**: `resolve_expr_ty` 完全削除 + `emit_stub_call` 到達不能確認
  - `emit_wasm/closures::resolve_expr_ty` (**90+ 行**、emit 時の場当たり型解決 helper) を削除
    - 外部呼び出し2箇所 (`closures.rs:62`, `mod.rs:827`) は `expr.ty.clone()` に置換
    - ConcretizeTypes が信頼できるので emit は「型を信じるだけ」になった
  - `emit_stub_call` に `ALMIDE_WASM_STUB_PANIC=1` モード追加
    - 発火すれば compile-time panic — CI用
    - **全 183 spec + nn WASM 60 テスト** を panic モードで実行 → **1件も発火せず**
    - つまり stub_call は現行コードベースで既に到達不能。#7 (stub削除) は安全に進められる状態
  - #7 の Phase 1 相当完了: 不発弾が**実測で不発弾ではなかった**ことを証明

- **2026-04-13**: #2 着手 (stdlib 宣言駆動 Phase 1)
  - `emit_wasm/stdlib_dispatch.rs` 新規。`StdlibOp` enum で `Call1` / `Call2` / `Call3` / `FloatUnaryCall` パターンを表現
  - `emit_math_call` / `emit_float_call` / `emit_string_call` で宣言テーブルに移行:
    - math: sin/cos/tan/log/exp/log10/log2 (7個)
    - float: to_string
    - string: trim/trim_start/trim_end/reverse/len/contains/split/join/count/replace (10個)
  - パターン証明完了。同じ dispatcher を全モジュール (option/map/bytes/value/list など) に展開可能
  - 全 183 spec + nn WASM 60 テスト 通過

- **2026-04-13**: pass.rs dead code 削除 + emit_wasm/calls.rs 分割 (4609 → 1144 行)
  - `pass.rs` に stub 実装された `OptionErasurePass` / `CloneInsertionPass` (stub) / `TypeConcretizationPass` (stub) / `StreamFusionPass` (stub) / `ResultPropagationPass` (stub) / `default_pipeline()` (どこからも呼ばれない) を削除
    - 展示で致命的な「何もしないパスが pipeline に並ぶ」嘘を除去
    - `TypeConcretizationPass` (Rust stub) / `OptionErasurePass` (Python stub) を pipeline から除去
  - `emit_wasm/calls.rs` (225KB / 4609 行) を機能単位に分割:
    - `calls_env.rs` (68), `calls_random.rs` (219), `calls_datetime.rs` (481)
    - `calls_http.rs` (332), `calls_fs.rs` (1728), `calls_io.rs` (419), `calls_process.rs` (276)
  - calls.rs は 1144 行になり、責務は「メインディスパッチ + stub + assert_eq + fan + env/random/datetime/http/fs/io/process 以外の scalar ops」に集約
  - 全 183 spec + nn WASM 60 テスト 通過

<a id="非ゴール"></a>

## 非ゴール

- **Rust 側 TOML テンプレートの再設計**は対象外 (既に機能している)
- **新機能追加** (新 stdlib、新構文) はこのロードマップに含めない — 構造リファクタだけ
- **パフォーマンス最適化** — 正しさ優先

<a id="測定"></a>

## 測定

各段階で:
- **全 spec/ テスト通過** (回帰ゼロ)
- **nn 全モジュール Rust/WASM 両方で通過** (今日達成した基準を維持)
- **almide-dojo の MSR** 低下なし

<a id="関連"></a>

## 関連

- `docs/roadmap/active/whisper-almide.md` — この基盤の上で動く
- `docs/roadmap/active/stdlib-defs-runtime-consistency.md` — #2 と関連
- `crates/CLAUDE.md` — 現状の設計原則
- `crates/almide-codegen/CLAUDE.md` — 現状の三層アーキテクチャ

---

<a id="phase-3-arc-v0147--ideal-form-migration-done-2026-04-17"></a>

## Phase 3 作業（v0.14.7）— 理想形への移行［DONE 2026-04-17］

v0.14.7 として出荷。6つの出荷地点（S1 / S2 / S3 / S4 / B / A）で `done/bundled-almide-ideal-form.md` の全5項目を閉じた。patch 層の全監査と各段階の注記は CHANGELOG `[0.14.7]` を参照。

Phase 3 の焦点は**dispatch 層の重複排除**。定義層（TOML + `runtime/rs` + `emit_wasm/calls_*.rs`）は依然三重記述で、その範囲は Stdlib Declarative Unification 作業に属する。`active/stdlib-declarative-unification.md` 参照。さらに先の egg + MLIR 作業は `active/mlir-backend-adoption.md` が引き継ぐ。

<a id="historical-phase-3-plan-below"></a>

### 歴史的 Phase 3 計画（以下）

0.14.6 で bundled-Almide dispatch を ship した際、patch layer が複数溜まった
(option/result signature co-dependence、stub_call 残存、mono coverage の穴)。
`bundled-almide-ideal-form.md` で書き出した 5 debt のうち、(1)(2)(3) は
この codegen-ideal-form の #1 / #4 / #7 と重なる。(4)(5) が新規追加項目。
統合して 4-step arc として回収する。

<a id="step-s1--optionresult-signature-normalization-0147-phase31"></a>

### Step S1 — Option/Result signature 正規化 `0.14.7-phase3.1`

- TOML の `Fn[Unit] -> X` を `Fn[] -> X` に書き換え (option.toml 2 箇所、他 module
  にはなし、調査済)
- `stdlib_codegen.rs::parse_type` の `Fn[...]` 分岐で empty params を handle
  (現状 `fn()` 側のみ handle、`Fn[]` 側未対応)
- 各 target template が `Fn[] -> X` を正しく render するか確認
  (Rust: `impl Fn() -> X`、WASM: closure convention)
- `stdlib/option.almd` と `stdlib/result.almd` を削除 (signature override 不要に)
- `BUNDLED_MODULES` / `AUTO_IMPORT_BUNDLED` / `get_bundled_source` から option/result を削除
- `spec/stdlib/coverage_misc_test.almd` (`() => x` caller) が新 signature で通る確認
- 既存 spec 全通過 + 両 target smoke

**見積**: 2-3h、1 commit。#7 (stub 廃止) の前にやる必要がある
(option/result で引っかかるため)。

<a id="step-s2--concretizetypes-hard-postcondition-0147-phase32-shipped"></a>

### Step S2 — ConcretizeTypes の強制事後条件 `0.14.7-phase3.2` *（出荷済み）*

codegen-ideal-form #4 の完成。phase3.1 で audit を常時実行 + `ALMIDE_CHECK_IR`
での panic escalate まで到達、phase3.2 で **flip to hard** を完了:

- spec/ WASM sweep の ConcretizeTypes audit: 3 class → 0 (PRs #194–#198)
  - list.zip 系 `Option[Unknown]` 含む 15-21 件の実ケースを
    empty-list elem ty / ResultErr Ok slot / fold acc back-prop / Match
    subject → pattern bindings / OpenRecord skip の組合せで closure
- `pass.rs` の `ALMIDE_CHECK_IR` / `ALMIDE_VERIFY_IR` gating 削除
- debug build で IR verify + Postcondition violation を常時 panic、
  release build では diagnostic (非 panic) として stderr に出す
- CHANGELOG に「`expr.ty` は契約により信頼可能になった」と記載

残 WASM lifted-lambda TypeVar は `ClosureConversion` 由来の pass boundary
issue で、S3 (pass_resolve_calls Phase 1b-c) で自然に解消する見込み。

<a id="step-s3--pass_resolve_calls-phase-1b-d--stub-廃止"></a>

### Step S3 — pass_resolve_calls Phase 1b-d + stub 廃止

codegen-ideal-form #1 / #7 を段階的に完遂するアーク。

- **Phase 1b** *（出荷済み、`0.14.7-phase3.5`）* — bundled-Almide stdlib fn の
  `CallTarget::Module` → `CallTarget::Named { almide_rt_<m>_<f> }` 書き換え。
- **Phase 1c** *（出荷済み、`0.14.7-phase3.2`）* — `emit_stub_call*` を
  compile-time panic 化 (runtime trap 廃止)。
- **Phase 1d** *（出荷済み、`0.14.7-phase3.3`）* — `emit_stub_call_named` /
  `emit_stub_call` helper を完全削除し、各 WASM dispatcher の `_` fallback を
  inline `panic!("[ICE] emit_wasm: no WASM dispatch for ...")` に置換。
  S2 flip 下で既に compile-time ICE だったので behavior 不変、診断文が
  module / dispatcher 固有になった。

残タスク (Phase 1e+):

- Rust + WASM 両 dispatch entry を `dispatch_module_call` 1 本に統合
  - priority: (a) IR に user/bundled fn → user-fn call / (b) TOML → inline emit
    / (c) どちらも無し → compile-time ICE
- WASM emit 側 `emit_list_call` / `emit_int_call` 等の match arm に fallback
  が不要になる (resolve パスが事前解決)

**見積**: Phase 1e は 4-6h、resolve pass 拡張 + Rust/WASM dispatch 統合。arc の主軸。

<a id="step-s5--test-namespace-normalization-shipped-2026-04-19"></a>

### Step S5 — Test namespace 正規化 *（2026-04-19 出荷済み）*

codegen-ideal-form #6 の完成。`lower_test` が `IrFunction.name` に
`__test_almd_` prefix を付けるようになり、下流全パス (walker /
emit_wasm / module emit) から `is_test` 名前衝突回避ロジックが消えた:

- `almide_ir::TEST_NAME_PREFIX` + `IrFunction::display_name()` を新規公開
- `lower/mod.rs::lower_test` が prefix 付きの `Sym` を allocate
- `walker/mod.rs` と `emit_wasm/mod.rs` の `if func.is_test { format!("__test_…") }`
  分岐を削除、`func.name` を直接利用
- 下流全 spec + WASM 全テスト green (219 + 213)

`is_test` フラグは semantic 用途 (mono 保存 / auto_unwrap / template 選択)
で残すが、**名前は upstream で unique**。

<a id="step-s6--vartable-consolidation-deferred-to-activevar-table-unificationmd"></a>

### Step S6 — VarTable 統合 *（`active/var-table-unification.md` へ延期）*

codegen-ideal-form #5 の残務。`program.var_table` / `module.var_table`
2 層構造そのものを消す作業。本アークでは core の pain point (lifted
closure が別 var_table に跨る) が `pass_closure_conversion` の
module-local 保持で既に解消済みのため、**構造統合は独立アークに切り出す**。
追跡: `active/var-table-unification.md`。

<a id="step-s4--monomorphize-coverage-拡張-0147-phase33"></a>

### Step S4 — Monomorphize coverage 拡張 `0.14.7-phase3.3`

bundled-almide-ideal-form #5。0.14.6 の `monomorphize_module_fns` は narrow
(Module call → bundled stdlib の generic のみ)。以下を広げる:

- `CallTarget::Method` + UFCS が lower 後 Module になる path
- user package 内 generic fn (非 stdlib、純ユーザ定義)
- cross-module generic chains (bundled A の generic が bundled B の generic を呼ぶ)

**見積**: 2-3h、1-2 commits。S3 が land した後の方がクリーン (resolved call
target に対して discover するだけになる)。

<a id="release-cadence"></a>

### リリース間隔

- `0.14.7-phase3.1`: S1 + S2
- `0.14.7-phase3.2`: S3
- `0.14.7-phase3.3`: S4
- `0.14.7`: full release (4 step 完了 + dojo regression sweep)

各 `.N` で dojo の v0.14.6 baseline + subset 測定、MSR 非回帰を確認してから次。

<a id="non-goals"></a>

### 非ゴール

- 新 stdlib fn 追加は Phase 3 では行わない (infra 完成まで待つ)
- 語彙追加 (UFCS / ? chain の拡張候補) は 0.15 以降に後送り
- dojo task bank 拡張は dojo チーム側で並行

---

<a id="final-status-2026-04-19"></a>

## 最終状態（2026-04-19）

| # | 項目 | 状態 | 取り込み |
|---|---|---|---|
| #1 | Resolve-calls 独立 pass | **完了** | v0.14.7-phase3.5（PR #202） |
| #2 | stdlib 宣言的 dispatch | **分割** | → `active/stdlib-declarative-unification.md` |
| #3 | `IrMutVisitor` | **完了** | phase 1a（2026-04-13） |
| #4 | ConcretizeTypes 強制事後条件 | **完了** | v0.14.7-phase3.2（PR #199） |
| #5 | VarTable 統合 | **完了** | commit `ba50fd7b` → `done/var-table-unification.md` |
| #6 | test namespace 正規化 | **完了** | commit `496c51e4` |
| #7 | `emit_stub_call` 削除 | **完了** | v0.14.7-phase3.3 |

`#2` のみ未完。dispatch 層の重複排除はここで完了したが、定義層（TOML + `runtime/rs` + `emit_wasm/calls_*.rs`）は三重記述のままで、独立作業へ範囲を切り直したため。

<a id="what-this-arc-actually-shipped"></a>

### この作業で実際に出荷したもの

- **関数解決は pass** であり emit 時の推測ではない。未解決 call は compile-time ICE。`emit_stub_call_*` helper は削除し、各 WASM dispatcher fallback は inline `panic!("[ICE] ...")` となった。spec sweep がその到達不能を証明した。
- **`expr.ty` は契約により信頼できる。** `ConcretizeTypes` は強制事後条件。emit 時の型再導出（`resolve_expr_ty` / `resolve_list_elem` の4段 fallback）は削除。debug build は残る `Unknown` で panic、release は診断を出す。
- **`IrMutVisitor`** により VarId 書換え pass は200行でなく30行となり、新 IR variant が下流 pass を黙って壊さない。
- **test block は上流から `TEST_NAME_PREFIX` を持つ**。walker / emit_wasm 各所の `if is_test { format!("__test_…") }` 名前衝突 guard は不要。
- **VarTable は統合済み**。最初の codegen pass が `IrModule.var_table` を `IrProgram.var_table` へ吸収する。名前 key の `top_let_globals_by_name` mirror は、主たる cross-module lookup でなく backup となった。

<a id="follow-ups-not-part-of-this-arc"></a>

### 後続（この作業に含まない）

- `active/stdlib-declarative-unification.md` — 定義層の三重記述除去。
- `active/mlir-backend-adoption.md` — egg + MLIR のコンパイラ世界の作業。
- `active/dispatch-unification-plan.md` — Rust と WASM の stdlib dispatch 入口統合。
