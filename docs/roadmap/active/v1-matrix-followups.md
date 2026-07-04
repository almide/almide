<!-- description: Follow-ups surfaced while closing the nn matrix walls — exact re-entry points -->
# V1 Matrix Follow-Ups — 残課題の正確な再開点

> 2026-07-04、crush-pass 完了時に切り出した副産物。どれも nn walls 0 の達成には
> 不要だったが、正確な再開点をここに固定する。

## 1. v0-native の matrix codegen が develop-v1 で既存破損（重大・v0 側）

`almide run <matrix を使う何か>` が「codegen produced invalid Rust」で落ちる。
生成 Rust の `bridge::AlmideMatrix` が **enum** として出るのに、vendored runtime glue
（`almide_matrix_transpose` 等）は旧 `Vec<Vec<f64>>` API（`m.len()` / `m[0]` /
`FromIterator`）を期待して不整合。spec の matrix テストは WASM 経由で走るため
`almide test` では露見しない（263 via WASM / 10 native fallback の内訳に注意）。
再現: `almide run` で `matrix.from_lists([[1.0]])` を含む main。
v1 側の matrix parity oracle は当面 `almide run --target wasm`（byte-identical 保証）。

## 2. anon record の生成 drop ソースが `f0` 未定義で型エラー（v1・program-level fail）

`let m = { tensors: ts, data: data }`（**型注釈なし** anon record、field に
List[record] を含む）を持つプログラムが render 全体で
`type errors: ["undefined variable 'f0'"]` になる。`let m: Model = …` と注釈すれば回避。
`generate_record_drop_sources` の anon-record 経路（`__drop_anonrec_<hash>` /
`record_drop_field_frees` の let f{i} 発行）と `collect_recursive_anon_records` の
шейプ収集の突き合わせから再開。honest failure（wrong bytes ではない）だが
program-level fail は wall 粒度に直すべき。

## 3. record drop 生成の Matrix field arm（v1・leak 級）

`record_drop_field_frees` に Matrix field の arm が無く、`t if is_heap_ty` の
flat `rc_dec` に落ちて**行 block をリーク**する（出力は正しい）。
`List[List[Float]]` 掃引の arm（`__drop_list_str` 同型の行 rc_dec ループ、
型注釈は `Matrix` で生成）を足す。nn の WhisperWeights.conv1_w が実例。

## 4. v0-wasm の from_bytes_f32_le が OOB で範囲外 read（v0-wasm・コントラクト差）

native オラクル（runtime/rs/src/matrix.rs）は「offset+need > len → 全ゼロ行列」。
v1 はこれを転写。v0-wasm は範囲外 read（bits=1 = 隣接 block の rc を f32 扱い）を
返す。OOB は sane domain 外だが、3 者の合意を C-1xx 契約にして v0-wasm を直すべき。

## 5. mir>ir calls (BUG) = 1 — wav.almd::find_chunk_at（既存）

pass-6 の find_chunk 開通時から。elided-call marker の二重カウント
（mir 10 > ir 5）。HEAD でも再現確認済み（今回の変更とは無関係）。
classify_corpus の `call_count_breaches` から bisect。

## 6. Matrix self-host の残り（nn の非 wall 呼び出し面）

unlinked (b) バケットに残る matrix fns: softmax_rows / gelu / swiglu_gate /
multi_head_attention / masked_multi_head_attention（fast-exp 系 — v0 は
almide-kernel の SIMD fast-exp を lane 順で加算するため、bit-exact 転写には
exp_pd_{wasm,neon} の多項式 + lane 和 + 奇数 tail の libm exp を忠実に再現する
必要がある）、rope_rotate / conv1d / from_q1_0_bytes / from_bytes(f64) /
select_rows / append_rows / mul_scaled / fma 系。`matrix.mul` は BLAS
（macOS native）との合意が原理的に summation order 依存 — v0-wasm
（matmul_naive、k 昇順）と一致させてあり、org verify の実績上 native とも
一致してきた面のみ保証。

## 7. Option[Matrix] / find over List[Matrix]

defunc find は matrix-shaped payload を deliberate に defer（2 レベル option drop
が未整備）。必要になったら `optrec:` 相当の `__drop_opt_matrix` を生成側に足す。
