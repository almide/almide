<!-- description: gguf ADT walls, the Matrix value model, and coverage pins — all closed; nn walls 0 -->
<!-- done: 2026-07-04 -->
# Crush-Pass Continuation — gguf + Matrix walls closed

> 2026-07-04 完了。このファイルはセッション引き継ぎ（残件の再開点）として書かれ、
> 全項目がクローズされたため done へ移動。副産物の既知課題は
> [active/v1-backlog.md](../active/v1-backlog.md) に再開点付きで分離。

## 最終結果

- **nn walls 22 → 0**（in-profile 288、(c) FORBIDDEN 0）。classify_corpus + WALL_NAMES で確認
- MIR テスト **518 → 525**（各修正 pin 同梱）/ spec 273 / parity **179**（MISMATCH 0、
  ctor 開通で +2 ratchet 済み）/ corpus-wall PCC ACCEPT / stdlib purity gate OK
- probe はすべて 3点検証（stdout+stderr+exit）、matrix 系 oracle は `almide run --target wasm`
  （v0-native matrix codegen はこのブランチで既存破損 — followups 参照）

## A. gguf（完了）

- 診断どおり `(ADT, Int)` tuple 返し tail の ctor call を `try_lower_variant_ctor` へ迂回
  （binds_p4 の Named-call arm に前置判定 — binds.rs の list 要素 arm と同型）
- 予告どおり ADT brick 5 が続いて出た: `(ValArray(items), p)` の **ctor List field**。
  `try_lower_variant_ctor` に `ctor_list_field_drop_freeable` gate を追加 —
  生成側（`generate_variant_drop_sources` の field loop）が free できる形
  （`List[scalar]` / `List[rich variant]`）**だけ**を admit し、構築と drop が乖離しない
- `read_array` / `parse_metadata_entries` とも実物で壁消滅。再帰 + 蓄積 + ネスト teardown を
  probe で byte-verify（`end:9` がカーソル前進の内容シグナル）

## B. Matrix 値モデル（完了 — 方針 (a)）

- `stdlib/matrix_core.almd` 新設: **Matrix = List[List[Float]]**（v0 AlmideMatrix と同じ
  row-major 行リスト）。from_lists/to_lists は rc_inc 共有コピー（`__mx_share_fill` は
  coown 許可リスト登録）。24 fns を v0 オラクル忠実転写で self-host:
  get/rows/cols/zeros/ones/add/scale/map/transpose/slice_rows/gather_rows/causal_mask_add/
  dot_row/mul（k昇順 = v0-wasm matmul_naive と同一順序）/linear_row(_no_bias)/
  rms_norm_rows/layer_norm_rows/from_bytes_f32_le/from_bytes_f16_le/split_cols_even/
  concat_cols(_many)
- 型ルーティング: Matrix 値 = List[String] 級（`heap_elem_lists` — 行は flat block）、
  List[Matrix] = List[List[String]] 級（`list_list_str_lists` — DropListListStr の 2 レベル
  掃引がそのまま正しい）
- **nn の 3 walls 開通**（各 3点 byte 一致）:
  - per_head_rms_norm — defunc map の result gate に Matrix 要素を admit（結果は
    list_list_str 級で登録）
  - repeat_kv — flat_map accumulator を Matrix 要素に拡張（acc/leaf/new の drop 級を
    型から導出）+ `list.repeat_rc`（rc_inc 版 self-host、`list_heap_call_name` が heap 要素の
    repeat を全てここへ）+ heap-result if の param passthrough は既存 Var arm
  - concat_rows — list literal 引数の `elem_list_flat` 分類（call 要素の
    List[List[List[scalar]]] / List[Matrix]、DropListListStr 登録）
- **load_weights も開通**（roadmap 外だが nn 最後の壁）— 原因は 2 つ:
  1. capturing closure の `list.find` が **invalid wasm を Ok として emit**（render wall
     をすり抜ける translation error）。→ unfaithful-closure wall を全 HOF に一般化
     （`is_higher_order && last_call_had_unlifted_closure` — 名前リスト廃止）+
     **defunc `list.find`**（early-exit ループ + len-as-tag Option 材料化、rich record
     payload は `optrec:` 経由）
  2. **cross-module 型名の不整合**: registry キーは修飾名（`types_mod.Lin`）だが use-site
     `Ty::Named` は素の名前。`canonical_record_key` / `canonical_name_in`
     （厳密一致 → 一意 `.name` suffix、曖昧なら None=wall）を
     `aggregate_field_tys` / `record_drop_type_name`（正準名を返す = drop 名の同一性）/
     `recursive_aggregate_name`（生成側）に配線
- HOF dispatch は構造化: 呼び出し側の名前ホワイトリストを廃止し、エンジン内
  `match func` を単一の真実に（重複リストのドリフトで find を一度取り逃した教訓）

## C. カバレッジ（継続）

pin テスト +7（518 → 525）: adt_int_tuple_return_ctor / variant_ctor_list_field_recursive_accumulator /
matrix_self_host_floor / matrix_per_head_repeat_kv_concat_rows / matrix_norms_and_bytes /
defunc_find_capturing_predicate / load_weights_record_return_shape。
既存 find テスト 2 本は期待反転（auto-link → inline、fold の前例に同じ）。
台帳更新は proofs/coverage.sh（手動 llvm-cov）。
