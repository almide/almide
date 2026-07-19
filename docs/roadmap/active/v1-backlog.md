<!-- description: The v1 trust-spine backlog — soundness, drop completeness, self-host surface, walls -->
# V1 Backlog — trust spine の残件一覧（優先度付き）

> 2026-07-04 更新。Tier 1（健全性）・Tier 2（drop 完全性）・Tier 3 の軽量 self-host は
> クローズ済み。加えて 2 件の pre-existing 健全性バグを捕獲・是正（cert 二重 move /
> guard early-exit の silent miscompile）。残る重量級（fast-exp、spec 壁バケット、
> 契約は v0-native 依存）を以下に正確な再開点付きで残す。

## ✅ クローズ済み（2026-07-04）

- **T1-1 mir>ir breach**（wav.almd::find_chunk_at）— opt-tuple fold の body を成分ごと
  3回 lower していたのを 1 回/iteration に是正。BUG ゲート 0。
- **T1-2 anon-record drop の `f0` 型エラー** — List-element drop field を構造型
  （`List[{…}]`）で束縛。型注釈なし anon record が render 通過。
- **T2-3 Matrix record-field リーク** — `record_drop_field_frees` に Matrix /
  List[Matrix] arm（`__drop_matrix` / `__drop_list_matrix` 生成、行 rc_dec 掃引）。
- **T3-5 軽量 self-host** — list.enumerate/zip（scalar + flat-heap rc）、map.entries/
  from_list（skv repr）、list.get_or_hshare/take_hshare、fs.read_bytes、
  bytes.read_length_prefixed_strings_le。nn unlinked 29→12 sites。
- **T4-10 variant statement match の List-field 抽出** — ctor pattern を List[…] 等
  全 heap field へ拡張 + record-ctor pattern（`Data { seq, .. }`）+ arm-tail の
  for/while ループ実行 + deferred-subject match の wall。record-ctor 値の tagged
  construction（plain record 誤認の miscompile 修正）も同梱。spec 壁 6→2。
- **（捕獲）cert 二重 move** — `let base=…; if c then …+base else base` 形で、else-arm
  の Dup 済み value を `EndIf` val-move が二重計上し ownership REJECT（`iammd`）。
  runtime は元から健全（`Consume` は move マーカーで rc_dec しない）。cert 側で
  consumed-value を val-move から除外して是正。T4-10 が in-profile を広げて露出。
- **（捕獲）guard early-exit の silent miscompile** — `guard cond else E; …` を
  always-continue で defer していたため `!cond` パスが誤り（`guard len>0 else err();
  ok(x)` が空入力で ok を返した）。v1 に early-return 制御が無いため honest wall へ。
  parity baseline regression ゼロ（byte-verified 集合に guard 関数は不在）。

## Tier 2 残 — drop 完全性

1. **Option[Matrix]（deliberate defer）**。defunc find は matrix-shaped payload を意図的に
   壁る（2 レベル option drop 未整備）。実利用なし。開けるなら生成側に
   `__drop_opt_matrix` 相当を足してから gate を外す。
2. （記録）**List[flat variant] ctor field** は構築側で壁っており、生成側も free を
   emit しない — 現状は整合（never built）。開ける際は両側同時に。

## Tier 3 残 — 実行面の拡張（重量級）

3. **fast-exp 族 7 本（重・要精度検証）**: `matrix.softmax_rows` / `gelu` /
   `swiglu_gate` / `multi_head_attention` / `masked_multi_head_attention` /
   `rope_rotate` / `from_q1_0_bytes`。v0 は almide-kernel の SIMD fast-exp を
   **lane 順加算**するため、bit-exact 転写には exp_pd_{wasm,neon} の多項式
   （Horner 6 項）+ 2-lane 部分和 + 奇数 tail の libm exp を忠実再現する必要がある
   （crates/almide-kernel/src/silu.rs::exp_pd_neon が参照実装、math_exp.almd の
   前例に倣う）。**医療グレードのため ULP を急いで誤らない**こと。nn を v1 で
   end-to-end 実行する日の前提。現状 8 sites が honest wall。
4. **list.enumerate_h / zip_h（rich 要素）**: `Complex=(Float,Float)` 要素の
   enumerate は `(Int, tuple)` を filter→map で `e.1` 抽出する heap-tuple パイプに
   繋がる（fft の 4 補助サイト、honest wall）。ROI 低・非推論経路。

## Tier 4 残 — spec 壁バケット（既存 roadmap 路線）

5. **guard-else early return の modeling**（39 sites、現状 honest wall）— v1 に
   early-return 制御を導入し `guard cond else E; rest` を `if cond then rest else E`
   に構造化すれば in-profile 復帰。今回 miscompile を wall 化した分の回収。
6. `fan.*` capability 系 ~32 件 → effectful-27-blueprint。
7. ResultOk/Err/OptionSome 引数材料化 ~15 件。**注意**: `Result[String,String]` の
   `ok(payload)` tail ctor は容易だが、guard を含む関数（大半）は T4-5 の early-return
   が先。guard-free の if/else 形は既に lower する。
8. effectful stdlib（`datetime.parse_iso` / `env.os` / `env.temp_dir` / `fs.stat` /
   `http.*` / `net.*` / `process.*`）— capability 宣言の床。fs.read_bytes と同様に
   admitted-effectful リスト + 対応 prim で開通可能なものから。
9. その他材料化 ~10 件: EmptyMap/MapLiteral/List 引数、call-arg 位置の capturing
   lambda、opaque fn-value の HOF。

## Tier 5 — 契約・計測

10. **matrix.mul / from_bytes OOB の契約台帳**: v1 は native 契約（mul は k 昇順、OOB は
    全ゼロ行列）に適合済み。ブロッカーだった v0-native matrix codegen 破損は
    38382cf8（2026-07-07、"Stop embedding the kernel bridge module and retire the stale
    burn matrix splicer that corrupted every native matrix build"）で解消済みを実測で確認
    （`matrix.zeros`/`matrix.rows`/`matrix.cols`/`matrix.transpose` を使う fixture が
    `almide run` で今日コンパイル・実行できる）。**unblocked — 未着手だが着手可能**。
11. **カバレッジ**: 直近 66.30%。最大の未踏面は control_p5（defunc エンジン）、
    次いで tail.rs / binds_p2 / binds_p4。計測は `bash proofs/coverage.sh`。

## v1 レーン外（引き継ぎ事項 — ここでは直さない）

- **（解消済み 2026-07-07）v0-native の matrix codegen 破損**: 生成 Rust の
  `bridge::AlmideMatrix` が enum 化される一方、vendored glue（transpose 等）が旧
  `Vec<Vec<f64>>` API のままで不整合 → `almide run` の matrix コードが
  「codegen produced invalid Rust」だった件は、38382cf8（"Stop embedding the kernel
  bridge module and retire the stale burn matrix splicer that corrupted every native
  matrix build"）で解消済み。`matrix.zeros`/`matrix.rows`/`matrix.cols`/
  `matrix.transpose` の native 実行を実測で再確認済み。Tier 5-10 の契約 fixture は
  もうブロックされていない（unblocked、未着手）。
