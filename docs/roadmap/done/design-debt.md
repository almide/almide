<!-- description: Design debt including partial TOML dispatch and anonymous records -->
<!-- done: 2026-03-18 -->
# Design Debt

## 1. gen_generated_call の部分接続

**状態:** TOML テンプレート dispatch が list/string/map/int/float/math/result/option のみ接続。残りのモジュール (fs, http, json, etc.) は直接 `almide_rt_*` 呼び出し
**問題:** 新モジュール追加時に TOML の `&`/`&*`/`.to_vec()` convention と runtime 関数シグネチャの不一致が再発する
**修正方針:** `lower_stdlib_call` の `use_template` を全モジュールに拡張。各モジュールの runtime シグネチャを TOML convention に合わせる
**見積り:** 2日（モジュールごとに runtime シグネチャを確認・修正）

## 2. Anonymous record の設計

**状態:** `AlmdRec0<T0, T1>` — フィールド名のソート順で generics を割り当て。今回修正済みだが根本設計が fragile
**問題:** フィールド数が同じで名前が違う anonymous record が同じ `AlmdRec` 名を共有しない（正しい）が、generics パラメータの対応が暗黙的
**修正方針:** anonymous record を `struct AlmdRec_age_name { age: i64, name: String }` のように concrete 型にする（generics なし）。各フィールド組み合わせが 1 つの concrete struct
**見積り:** 1日

## 3. Borrow analysis の深化

**状態:** 接続済みだが効果が限定的。関数パラメータの borrow 判定のみ
**問題:** ループ内の変数、field access、nested call の clone が過剰
**修正方針:**
- Phase 1: inter-procedural escape analysis（callee の borrow 情報を利用） — 既に fixpoint loop のコードあり
- Phase 2: loop-aware clone（ループ外で 1 回 clone、ループ内は参照）
- Phase 3: field-level borrow（`obj.field` が non-heap なら clone 不要）
**見積り:** Phase 1: 2日、Phase 2: 3日、Phase 3: 2日
