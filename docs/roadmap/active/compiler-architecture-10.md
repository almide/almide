# Compiler Architecture: All 10s [ACTIVE]

**目標**: コンパイラアーキテクチャ全項目 10/10
**現状**: 80/100 (7つの領域に改善余地)
**スコープ**: WASM codegen 以外の全コンパイラ基盤

---

## スコアカード

| 領域 | 現在 | 目標 | 主要改善 |
|------|------|------|----------|
| パイプライン設計 | 9 | 10 | パス依存宣言、BoxDeref のパイプライン統合 |
| パーサー | 9 | 10 | (維持 — fuzzing で補強) |
| 型チェッカー | 7 | 10 | mod.rs 分割、string interning、call resolution 整理 |
| IR 設計 | 9 | 10 | (維持 — verification の条件付き実行) |
| Nanopass | 8 | 10 | stream fusion 分割、walker 分割、スナップショットテスト |
| モノモーフィゼーション | 7 | 10 | ファイル分割、COW 特殊化、増分発見、収束検出 |
| エラー診断 | 9 | 10 | E003 の --explain 追加、エラーコードレジストリ |
| コード品質 | 7 | 10 | string interning、clone 削減、巨大ファイル分割 |
| テスト | 8 | 10 | nanopass テスト、スナップショット、fuzzing、ベンチマーク |
| ビルドシステム | 7 | 10 | build.rs 分割、型パーサー AST 化、生成コード検証 |

---

## Phase 1: 基盤整備 (Quick Wins)

即座に効果があり、後続フェーズの前提になるもの。

### 1.1 パス依存宣言の追加

**ファイル**: `src/codegen/pass.rs`
**変更**: NanoPass trait に `depends_on()` メソッドを追加

```
NanoPass trait:
  fn name(&self) -> &str;
  fn targets(&self) -> Option<Vec<Target>>;
  fn depends_on(&self) -> Vec<&'static str>;  // NEW
  fn run(&self, program: &mut IrProgram, target: Target);
```

Pipeline::run() でパス実行前に依存順序を検証。違反時は panic で即座に検出。

**暗黙の依存関係 (現在コメントのみ)**:
- StreamFusion → BorrowInsertion/CloneInsertion の前 (decorator がパターン検出を壊す)
- EffectInference → StdlibLowering の前 (Module call が消える前に effect 解析)
- StdlibLowering → ResultPropagation の前 (Named call が必要)
- ResultPropagation → BuiltinLowering の前 (Try wrapping が先)
- MatchLowering → ResultErasure の前 (TS/JS: パターン情報が消える前に)
- ShadowResolve → 全 lowering の後 (スコープが確定してから)

**工数**: S (2-3時間)

### 1.2 E003 の --explain 追加 + エラーコードレジストリ

**ファイル**: `src/main.rs` (print_error_explanation)
**問題**: E003 (undefined variable) が --explain に登録されていない

**追加作業**: `src/errors.rs` にエラーコードレジストリを新設し、散在する E001-E010 の定義を一元化。

**工数**: S (半日)

### 1.3 BoxDeref のパイプライン統合

**ファイル**: `src/codegen/mod.rs`, `src/codegen/target.rs`
**問題**: BoxDeref ロジックが Pipeline の外にハードコードされている (mod.rs:75-130)
**修正**: BoxDerefPass を NanoPass として実装し、Rust パイプラインの先頭に配置

**工数**: S (1-2時間)

---

## Phase 2: 型チェッカー改善

### 2.1 mod.rs の分割 (850行 → 3モジュール)

**現状**: 制約解消、登録、診断、宣言チェック、型解決が1ファイルに混在

**分割先**:

| モジュール | 責務 | 移動元 |
|-----------|------|--------|
| `registration.rs` | register_fn_sig, register_type_decl, register_protocol_decl, validate_protocol_impls, bounds collection | mod.rs:376-628 |
| `solving.rs` | solve_constraints, unify_infer, unify_structural | mod.rs:258-374 |
| `diagnostics.rs` | suggest_conversion, hint_with_conversion, エラーコード定義 | mod.rs:285-305 |

validate_protocol_impls の最適化: 全 type_protocols を clone するスナップショットを廃止し、直接イテレーションに変更。

**工数**: L (3-4日)

### 2.2 calls.rs の分割 (588行 → 4モジュール)

**現状**: UFCS 解決、ビルトイン呼び出し、静的ディスパッチ、ジェネリクス解決が混在

**分割先**:

| モジュール | 責務 | 行数 |
|-----------|------|------|
| `calls.rs` | check_call_with_type_args, check_constructor_args, unify_call_arg | 350 |
| `builtin_calls.rs` | ok/err/some/println/assert 等のビルトイン | 100 |
| `static_dispatch.rs` | resolve_static_member (fan.*, codec, module 解決) | 160 |
| `generic_resolution.rs` | instantiate_type_generics (キャッシュ付き), resolve_type_name | 80 |

resolve_type_name を O(n) 線形探索から TypeNameIndex キャッシュで O(1) に。

**工数**: M (3-4日)

---

## Phase 3: モノモーフィゼーション改善

### 3.1 ファイル分割 (1,290行 → 6モジュール)

**分割先**:

| モジュール | 責務 | 行数 |
|-----------|------|------|
| `mod.rs` | エントリポイント (固定点ループ) | 100 |
| `discovery.rs` | discover_instances, discover_in_expr/stmt | 200 |
| `specialization.rs` | specialize_function, substitute_* | 250 |
| `rewrite.rs` | rewrite_calls, rewrite_expr/stmt_calls | 200 |
| `propagation.rs` | propagate_concrete_types, propagate_expr/stmt | 250 |
| `utils.rs` | mangle_suffix, ty_to_name, has_typevar | 150 |

**工数**: M (1-2日)

### 3.2 COW 特殊化 (clone 80% 削減)

**問題**: specialize_function() が IrFunction 全体を clone (mono.rs:481)
**修正**: 変更されたフィールドのみ新規構築。body は Cow で、型が変わったノードだけ新規生成。

**工数**: M (2-3日)

### 3.3 増分インスタンス発見

**問題**: 毎ラウンド全関数をスキャン (O(N × total_functions))
**修正**: 前ラウンドで新規作成された関数のみスキャン (O(N × new_functions))。フロンティア追跡。

**工数**: M (1-2日)

### 3.4 収束検出 (max_iterations 撤廃)

**問題**: 固定点ループが max_iterations=10 でハードコード
**修正**: 新規インスタンス数=0 で終了。爆発検出 (1000+ で警告)。

**工数**: S (数時間)

---

## Phase 4: Nanopass + Walker 分割

### 4.1 pass_stream_fusion.rs の分割 (1,192行 → 5モジュール)

| モジュール | 責務 | 行数 |
|-----------|------|------|
| `pass_stream_fusion.rs` | NanoPass impl (エントリ) | 150 |
| `pass_stream_fusion/chain_detection.rs` | パイプチェーン検出 | 220 |
| `pass_stream_fusion/fusion_rules.rs` | try_fuse_* (6ルール) | 280 |
| `pass_stream_fusion/lambda_composition.rs` | compose_lambdas, compose_predicates | 200 |
| `pass_stream_fusion/ir_transform.rs` | recursive_transform, transform_children | 150 |

**工数**: M (4-6時間)

### 4.2 walker.rs の分割 (1,662行 → 8モジュール)

| モジュール | 責務 | 行数 |
|-----------|------|------|
| `walker.rs` | render_program, render_function | 200 |
| `walker/expressions.rs` | render_expr (50+ IR 式) | 620 |
| `walker/statements.rs` | render_stmt, render_pattern | 180 |
| `walker/types.rs` | render_type + ヘルパー | 200 |
| `walker/declarations.rs` | render_type_decl, render_function | 150 |
| `walker/templates.rs` | template_or, render_with ラッパー | 120 |
| `walker/builtins.rs` | render_method_call_full, enum ctor | 100 |
| `walker/annotations.rs` | record collection, type var handling | 120 |

**工数**: L (8-10時間)

---

## Phase 5: コード品質

### 5.1 String Interning

**設計**:
```
ModuleId(u8)  — 22 stdlib モジュール用 (静的配列)
FuncId(u16)   — モジュール内関数 ID
SymId(u32)    — 汎用識別子 (型チェッカー用)
```

**影響範囲**:
- codegen/emit_wasm/calls.rs: 9箇所の module== 比較 → ModuleId ディスパッチ
- pass_stream_fusion.rs: 7箇所の分類関数 → FuncId enum
- stdlib.rs:resolve_ufcs_candidates: 150+ のハードコード文字列マッチ → 静的レジストリ
- check/ 全体: define_var/lookup_var の 80+ 箇所 → SymId

**期待効果**: 文字列比較 O(n) → O(1)、clone 200-250 個削減、コンパイル速度 3-8% 向上

**工数**: M (1-2週間、段階的に適用)

### 5.2 Clone 削減 (Phase 3 の COW と合わせて)

**主要ターゲット**:
- ir/substitute.rs: Ty の深いクローンを参照に (150箇所)
- check/infer.rs: 関数シグネチャの clone を遅延評価に (39箇所)
- lower/derive_codec.rs: 最多 clone ファイル (79箇所) — フィールド名の &str 化

**工数**: S-M (各ファイル半日〜1日)

---

## Phase 6: テスト強化

### 6.1 Nanopass ユニットテスト (40-50テスト)

各パスに入力 IR → 出力 IR の変換テスト:
- ResultErasure: ok(x)→x, err(e)→throw (5テスト)
- ResultPropagation: Try 挿入 (3テスト)
- MatchLowering: match→if/else (3テスト)
- EffectInference: effect 推移検出 (4テスト)
- CloneInsertion: ヒープ型のみ clone (2テスト)
- ShadowResolve: let 再束縛→代入 (1テスト)
- StdlibLowering: Module→Named 変換 (4テスト)
- BuiltinLowering: Named→Macro 変換 (2テスト)

**工数**: M (4-5日)

### 6.2 スナップショットテスト (insta crate)

IR の before/after を JSON で golden file 比較。パスの silent breakage を検出。

**工数**: M (2-3日)

### 6.3 モノモーフィゼーションユニットテスト (25テスト)

- インスタンス発見: 単純/複数/推移的/深いチェーン (7テスト)
- 特殊化: セマンティクス保存/OpenRecord/VarTable 整合 (3テスト)
- 書き換え: call 名変換/全呼び出し元の書き換え (2テスト)
- 型伝搬: TypeVar 排除/戻り値型伝搬 (2テスト)

**工数**: M (2-3日)

### 6.4 Parser Fuzzing (proptest)

proptest で 100k+ 入力を生成し、パーサーがパニックしないことを検証。エラーリカバリの堅牢性確認。

**工数**: M (2-3日)

### 6.5 パフォーマンスベンチマーク (criterion)

- パーサー: 100行/200行/500行のファイルのパース速度
- 型チェッカー: ジェネリクス展開の制約解消速度
- モノモーフィゼーション: 推移チェーンの特殊化速度

CI で regression tracking。

**工数**: S (1-2日)

---

## Phase 7: ビルドシステム

### 7.1 build.rs の分割 (1,261行 → 5モジュール)

build.rs 単体を xtask クレートまたはサブモジュールに移行:

| モジュール | 責務 | 行数 |
|-----------|------|------|
| `parser/types.rs` | 型文字列のAST パース (bracket matching 含む) | 300 |
| `loader/stdlib.rs` | TOML 定義ファイルの読み込み + スキーマ検証 | 250 |
| `loader/runtime.rs` | ランタイムソースのスキャン (syn crate 使用) | 150 |
| `codegen/stdlib.rs` | sig + call 生成 | 200 |
| `validate/mod.rs` | 型検証、テンプレート検証、ランタイムカバレッジ検証 | 350 |

**主要改善**:
- 手書き bracket matching → AST ベースの型パーサー (TypeExpr enum)
- 文字列 regex でのランタイムスキャン → syn crate による Rust AST パース
- 生成コードの syntax 検証 (syn::parse_file で検証)
- .unwrap() パニック → 集約エラーレポート (file:line 付き)

**工数**: M (5-6日)

---

## 実行順序

```
Phase 1 (1-2日)     ← 即座に効く、後続の前提
  1.1 パス依存宣言
  1.2 E003 + エラーレジストリ
  1.3 BoxDeref パイプライン統合

Phase 2 (1-2週間)   ← 型チェッカーの品質基盤
  2.1 mod.rs 分割
  2.2 calls.rs 分割

Phase 3 (1-2週間)   ← モノモーフィゼーション
  3.1 ファイル分割
  3.2 COW 特殊化
  3.3 増分発見
  3.4 収束検出

Phase 4 (1週間)     ← Nanopass + Walker の可読性
  4.1 stream fusion 分割
  4.2 walker 分割

Phase 5 (1-2週間)   ← 全体のコード品質
  5.1 String Interning
  5.2 Clone 削減

Phase 6 (2-3週間)   ← Phase 1-5 と並行可能
  6.1 Nanopass テスト
  6.2 スナップショットテスト
  6.3 モノモーフィゼーションテスト
  6.4 Parser Fuzzing
  6.5 ベンチマーク

Phase 7 (1週間)     ← 最後 (他のフェーズに依存しない)
  7.1 build.rs 分割 + 検証レイヤー
```

**総工数見積もり**: 6-10 週間 (テストは並行実施)
**完了時スコア**: 100/100
