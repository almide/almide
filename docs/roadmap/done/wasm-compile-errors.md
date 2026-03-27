<!-- description: WASM compile error elimination (type mismatches, lambda issues) -->
# WASM Compile Error Elimination Roadmap

## Status: 10件 (109/182 passed)

## エラー分類と対策

### Category A: i32/i64 type mismatch (6件)

**症状**: `expected i32, found i64` or `expected i64, found i32`
**影響ファイル**: typed-api-client, almide-grep, codegen_variant_record_test, hash_protocol_test, set_extra_test, (grade_report — 別カテゴリに移行済)

**根本原因**: lambda/closure の call_indirect 型構築時に、lambda の Fn 型（`args[N].ty`）から param/ret の ValType を導出しているが、型推論が不完全なケースで Unknown/TypeVar が残り、`ty_to_valtype` の catch-all が i32 を返す。実際の値が i64 (Int) や f64 (Float) の場合に不整合。

**既に修正済み**:
- list.map: out_elem_ty を call-site return type から導出
- list.fold: acc_ty と elem_ty を concrete 引数型から導出
- list.filter: elem_ty を list 型から導出、ret を i32 (Bool) 固定

**残りの対策**:
1. **全 closure 関数を concrete type 導出に統一** — calls_list_closure.rs, calls_list_closure2.rs の残り関数
2. **共通 helper `emit_closure_call_indirect`** を作成:
   ```rust
   fn emit_closure_call_indirect(&mut self, param_types: &[&Ty], ret_ty: &Ty)
   ```
   各関数の call_indirect 構築を1行に集約。型の Unknown fallback ロジックを一箇所で管理。
3. **map closure 関数** (calls_map_closure.rs) にも同じ対策適用
4. **open record generic 残り**: mono で VarTable 更新が不完全なケースの追加修正

### Category B: nothing on stack (2件)

**症状**: `expected a type but nothing on stack`
**影響ファイル**: data_types_test, map_higher_order_test

**根本原因**: コードパスが値を返すべきブロックで値を生成しない。候補:
- match arm が Unit を返すのに外側が値を期待
- do-block の tail expression が欠落
- effect fn 内の Result unwrap で値が消える

**対策**:
1. 最小再現ケースを data_types_test, map_higher_order_test から抽出
2. WASM validator の offset から問題の関数を特定
3. emit_expr の該当パスを修正（Block/If/Match のスタック整合性）

### Category C: local index out of bounds (2件)

**症状**: `unknown local N: local index out of bounds`
**影響ファイル**: list_completion_test (local 14), grade_report (local 22)

**根本原因**: `count_scratch_depth` が関数内の最大 scratch local 使用数を過少カウント。実際の emit で使われる local index がアロケート数を超える。

**対策**:
1. `count_scratch_depth` (statements.rs) の漏れパターンを特定
2. 関数の IR をダンプして実際に必要な scratch 数を確認
3. 該当パターンの depth を増やす
4. 特に nested closure call + multiple scratch 使用の組み合わせ

### Category D: values remaining on stack (1件)

**症状**: `values remaining on stack at end of block`
**影響ファイル**: config_merger

**根本原因**: ブロック末尾で余分な値がスタックに残る。Block/If/Match の分岐で一方が値を返し他方が返さない、など。

**対策**:
1. 最小再現ケースを config_merger から抽出
2. WASM validator の offset から問題の block を特定
3. 該当する emit パスのスタック balance を修正

## 実装順序

1. **Category A helper 化** (最大効果: 6件) — `emit_closure_call_indirect` helper 作成、全 closure 関数に適用
2. **Category C scratch depth** (2件) — count_scratch_depth の漏れ修正
3. **Category B nothing on stack** (2件) — 最小再現 → 修正
4. **Category D values remaining** (1件) — 最小再現 → 修正

## 関連ファイル

- `src/codegen/emit_wasm/calls_list_closure.rs` — closure list 関数 (find, any, all, etc.)
- `src/codegen/emit_wasm/calls_list_closure2.rs` — closure list 関数 (take_while, fold, map, etc.)
- `src/codegen/emit_wasm/calls_map_closure.rs` — closure map 関数
- `src/codegen/emit_wasm/statements.rs` — count_scratch_depth
- `src/mono.rs` — VarTable 更新 (open record generic)
