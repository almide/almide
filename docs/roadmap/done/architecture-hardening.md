<!-- description: Fix structural weaknesses in compiler architecture -->
<!-- done: 2026-03-15 -->
# Architecture Hardening

コンパイラの構造的脆弱性の修正。言語の成長・新機能追加に伴い必ず踏む地雷を事前に除去する。

## P1: IrProgram の clone 除去

**問題:** `emit_with_options()` で IrProgram と全モジュール IR をディープコピーしている。

```rust
// emit_rust/mod.rs:145-146
emitter.ir_program = Some(ir.clone());
emitter.module_irs = module_irs.clone();
```

大規模プロジェクトでメガバイト単位の無駄なアロケーション。

**修正:**
- [ ] `Emitter` にライフタイムパラメータ追加: `Emitter<'a>`
- [ ] `ir_program: Option<&'a IrProgram>`、`module_irs: &'a HashMap<String, IrProgram>` に変更
- [ ] `emit_with_options` の呼び出し元でライフタイムが十分であることを確認

**影響:** emit_rust/mod.rs, program.rs, ir_expressions.rs, ir_blocks.rs

## P1: Emitter の状態管理リファクタ

**問題:** 25+ フィールド、RefCell/Cell による interior mutability、状態フラグ（`in_effect`, `in_do_block`, `skip_auto_q`）の独立管理で整合性リスク。

```rust
pub(crate) in_do_block: std::cell::Cell<bool>,
pub(crate) skip_auto_q: std::cell::Cell<bool>,
pub(crate) anon_record_structs: std::cell::RefCell<HashMap<...>>,
```

**修正:**
- [ ] コンテキスト状態を `CodegenContext` 構造体に分離（`in_effect`, `in_do_block`, `skip_auto_q`, `in_test`）
- [ ] `anon_record_structs` と `anon_record_counter` を事前収集パスに移動（codegen 中の mutation を除去）
- [ ] RefCell → 事前計算テーブル、Cell → 明示的なスタック管理

## P1: fixpoint 反復の収束保証 ✅

**修正済み.** 上限を `max(fn_count, 20)` に変更。未収束時に warning を出力。

## P1: モジュール循環参照の検出 ✅

**既に実装済み.** `resolve.rs` に `loading: HashSet<String>` による循環検出あり。`circular import detected: ...` エラーを出す。

**修正:**
- [ ] `resolve.rs` で import グラフを構築し、DAG であることを検証
- [ ] 循環検出時にエラー: `"circular import: A → B → A"`
- [ ] テスト: 循環 import のテストケース追加

## P2: build.rs テンプレートの検証

**問題:** stdlib TOML の `rust:` テンプレートに未知のプレースホルダがあっても黙ってリテラル出力。生成コードが壊れる。

```rust
// build.rs — {unknown_param} がそのまま Rust コードに出力される
```

**修正:**
- [ ] テンプレートスキャン時に全 `{placeholder}` が既知パラメータに含まれることを検証
- [ ] 未知プレースホルダでビルドエラー
- [ ] クロージャ型のアリティ解析をネストブラケット対応に修正

## P2: scope の push/pop バランス検証

**問題:** `LowerCtx` の scope スタックが push/pop 不整合で壊れる可能性。エラーパスで pop が呼ばれないケース。

```rust
fn pop_scope(&mut self) { self.scopes.pop(); }  // 空なら panic
```

**修正:**
- [ ] debug_assert で push/pop バランスを検証
- [ ] RAII ガードパターン: `let _guard = ctx.push_scope()` で Drop 時に自動 pop
- [ ] 空スタック pop を graceful error に変更

## P2: parser と precedence.toml の整合性

**問題:** `grammar/precedence.toml` はドキュメント用にのみ使われ、実際のパーサーの優先順位はハードコード。乖離する可能性。

**修正:**
- [ ] cargo test で precedence.toml とパーサーの優先順位が一致することを検証するテスト追加
- [ ] 将来: precedence.toml からパーサーコードを生成

## P2: unsafe indexing の安全性

**問題:** `--fast` モードで `get_unchecked` を使うが、インデックスの範囲検証なし。`as usize` で負数が巨大正数に化ける。

```rust
format!("unsafe {{ *{}.get_unchecked({} as usize) }}", obj, idx)
```

**修正:**
- [ ] codegen 時に `debug_assert!(idx >= 0 && (idx as usize) < {}.len())` を `unsafe` ブロック前に挿入
- [ ] または: `as usize` の前に負数チェック

## P2: パーサー再帰深度制限 ✅

**修正済み.** `Parser` に `depth: usize` フィールド追加。`parse_expr` と `parse_type_expr` の入り口で `enter_depth()` → `MAX_DEPTH(500)` 超過でエラー。

## P3: VarId の u32 オーバーフロー ✅

**修正済み.** `debug_assert!(self.entries.len() < u32::MAX as usize)` を追加。

**修正:**
- [ ] `assert!(self.entries.len() < u32::MAX as usize, "too many variables")` 追加
