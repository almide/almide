<!-- description: The v1 trust-spine backlog — soundness, drop completeness, self-host surface, walls -->
# V1 Backlog — trust spine の残件一覧（優先度付き）

> 2026-07-04、crush-pass クローズ時点の実測（nn unlinked 15 callees / spec 壁 499 /
> coverage 66.30%）に基づく純 v1 レーンの棚卸し。旧 v1-matrix-followups.md を統合。
> **推奨着手順: T1-1 → T2-3 → T1-2 → T4-10 → T3-5**（健全性 → リーク → 壁粒度 →
> gguf 消費面 → 呼び出し面）。T3-6（fast-exp）は nn end-to-end のマイルストーンと
> セットで計画する。

## Tier 1 — 健全性（証明系の信頼に直結）

1. **mir>ir breach — `wav.almd::find_chunk_at`（mir 10 > ir 5）**。
   elided-call marker の二重カウントが caps の偽 de-taint を許しうる（classify の
   BUG ゲートが常時 1 で汚れている）。pass-6 の find_chunk 開通時から。HEAD 無関係を
   確認済み。再開点: classify_corpus の `call_count_breaches` から bisect。
2. **anon-record 生成 drop の `f0` 未定義型エラー**。型注釈なしの
   `let m = { tensors: ts, data: data }`（List[record] field 持ち）で render が
   **program-level** で落ちる（type errors）。honest failure だが wall 粒度であるべき。
   `let m: Model = …` で回避可。再開点: `collect_recursive_anon_records` の shape 収集と
   `record_drop_field_frees` の `let f{i}` 発行の突き合わせ。

## Tier 2 — drop 完全性（出力は正しいがリーク）

3. **Matrix record-field の行リーク**。`record_drop_field_frees` に Matrix arm が無く
   `t if is_heap_ty` の flat `rc_dec` に落ち、行 block をリークする
   （nn WhisperWeights.conv1_w が実例）。修正: `__drop_list_str` 同型の行 rc_dec ループ
   arm（型注釈は `Matrix` で生成）。
4. **Option[Matrix]（deliberate defer）**。defunc find は matrix-shaped payload を意図的に
   壁る（2 レベル option drop 未整備）。開けるなら生成側に `__drop_opt_matrix` 相当を
   足してから gate を外す。
5. （記録）**List[flat variant] ctor field** は構築側で壁っており、生成側も free を
   emit しない — 現状は整合（never built）。開ける際は両側同時に。

## Tier 3 — 実行面の拡張（self-host registry / defunc エンジン）

6. **軽量 self-host 8 本（19 sites）**: `list.zip` / `list.enumerate` /
   `list.get_or_hshare` / `list.take_hshare` / `map.entries` / `map.from_list` /
   `fs.read_bytes` / `bytes.read_length_prefixed_strings_le`。
   matrix_core と同じ流儀（v0 オラクル転写 + registry 登録 + 3点 probe）で軽〜中量級。
7. **fast-exp 族 7 本（重）**: `matrix.softmax_rows` / `gelu` / `swiglu_gate` /
   `multi_head_attention` / `masked_multi_head_attention` / `rope_rotate` /
   `from_q1_0_bytes`。v0 は almide-kernel の SIMD fast-exp を **lane 順加算**するため、
   bit-exact 転写には exp_pd_{wasm,neon} の多項式 + 2-lane 部分和 + 奇数 tail の
   libm exp を忠実に再現する必要がある（math_exp.almd の前例に倣う）。
   nn を v1 で end-to-end 実行する日の前提条件。
8. **matrix.mul の契約注記**: k 昇順転写は v0-wasm（matmul_naive）と同一順序。
   native（Accelerate BLAS）との一致は org verify の実績ベースで原理保証はない。
   契約台帳での言語化のみ（コードは現状維持）。

## Tier 4 — spec 壁 499 の上位バケット（既存 roadmap 路線と接続）

9. `fan.*` capability 系 ~32 件 → effectful-27-blueprint。
10. ResultOk/Err/OptionSome 引数の材料化 ~15 件（今回の calls_p2 引数材料化と同族）。
11. **variant statement match の List-field 抽出 6 件** — `ArrV(xs) => for x in xs` の形。
    開けると gguf の消費側（ValArray の中身を読む側）が v1 で実行可能になる。
12. effectful stdlib（`datetime.parse_iso` / `env.os` / `env.temp_dir` / `fs.stat`）~8 件
    — capability 宣言の床。
13. その他材料化 ~10 件: EmptyMap/MapLiteral/List 引数、call-arg 位置の capturing
    lambda、opaque fn-value の HOF。

## Tier 5 — 契約・計測

14. **from_bytes_* OOB の 3 者合意**: native 契約は「offset+need > len → 全ゼロ行列」で
    v1 は**既に適合**。v0-wasm は範囲外 read（隣接 block の rc を f32 解釈）。
    C-NNN 起票 + v0-wasm 側修正は共有レーン。
15. **カバレッジ**: 66.30%。最大の未踏面は control_p5（defunc エンジン、43%）、
    次いで tail.rs / binds_p2 / binds_p4。計測は `bash proofs/coverage.sh`、
    台帳は proofs/COVERAGE.md。

## v1 レーン外（引き継ぎ事項 — ここでは直さない）

- **v0-native の matrix codegen 破損（develop-v1 既存）**: 生成 Rust の
  `bridge::AlmideMatrix` が enum 化される一方、vendored glue（transpose 等）が旧
  `Vec<Vec<f64>>` API のままで不整合 → `almide run` の matrix コードが
  「codegen produced invalid Rust」。flat ABI / burn リワーク（並行セッション領域）の
  過渡状態とみられるため**所有セッションへ引き継ぎ**。v1 の matrix parity oracle は
  当面 `almide run --target wasm`（byte-identical 保証）を使う。
  spec の matrix テストは WASM 経由で走るため `almide test` では露見しない点に注意。
