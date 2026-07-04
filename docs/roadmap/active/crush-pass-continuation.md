<!-- description: Exact re-entry points for the remaining crush-pass items — nn 8 walls, CG-1 expansion, coverage raising, with per-item diagnosis and first move -->
# Crush-Pass Continuation — 残件の正確な再開点

> **Position**: 2026-07-03〜04 の「全部つぶす」パスの続き。ここまでの成果:
> 拡張3点観測（stdout+stderr+exit）完全ゼロ（baseline 177）、PCC チェーン全量
> ACCEPT（ローカル+CI green）、MIR テスト 512/0、nn walls 22→8、
> spec-keying 機構稼働（6契約 keyed）、miscompile 8クラス根絶。
> 本書は残件それぞれの**診断結果と最初の一手**を固定する。状況の全体像は
> [flight-evidence-gaps](flight-evidence-gaps.md) と memory
> `project_flight_evidence.md` を参照。

## A. nn 残 8 walls（各個撃破、推定30-90分/件）

### A-1. ggml_whisper `load` — heap-result match（診断済み、最短）
形: `match fs.read_bytes_raw(path) { ok(data) => parse(data), err(e) => err("…" + e) }`
（tail 位置、Result[Model,String]、ok アームが **Result 返し call のパススルー**）。
診断: vvm（try_lower_variant_value_match）には入る（subject の fs effect 化・Bytes
Ok admission は通過）が、**アーム lower（lower_heap_result_arm）到達前の vvm 内部
gate で decline**。probe = scratchpad の `gl.almd`（v0 出力込みで再現確認済み）。
**最初の一手**: vvm の各早期 return None に一時 eprintln（ALMIDE_MIR_DEBUG ゲート、
台帳管理）を入れて decline 行を特定 → その gate を「ok アーム = 同型 Result call
パススルー」に開ける。

### A-2. gguf `parse_metadata_entries` — for + heap-acc + ADT（複合）
形: `var entries: List[(String, GGUFValue)] = []` + `for _ in 0..count` +
`entries = entries + [(key, value)]` + `let (key, after) = r_string(...)`（call の
tuple destructure）。GGUFValue = custom ADT。
壁は「heap-result Range in call-argument position」— for の 0..count が式位置。
**最初の一手**: 最小 probe（ADT なしの `var acc: List[(String,Int)] = []; for i in
0..n { acc = acc + [(k, i)] }`）で Range/for-acc/(String,ADT) のどれが落ちるか分解。

### A-3. fft `combine` — while + heap-accumulator reassignment
壁メッセージ: 「while body with a heap-accumulator reassignment cannot be
faithfully lowered」— for ループの loop-carried slot 機構（`i(id)m`）が while に
未配線。**最初の一手**: for 側の slot 機構（mod_p3 の loop_reassigned_vars +
append-accumulator 経路）を while の lowering に移植。OwnershipLoop.v の証明は
ループ形に依存しないはず（per-iteration balance）。

### A-4. wav `find_chunk_at`（実物）— (Int, Option[Int]) tuple acc + match
scalar-tuple fold（実装済み）は両成分 scalar 限定。実物は第2成分が Option[Int]
（heap block）+ body が `match found { some(_) => state, none => … }`。
**最初の一手**: Option[Int] 成分を「tag scalar + payload scalar の2ローカル」に
展開する成分表現（none = tag 0）を scalar-tuple fold に足す。match アームは
「tag 条件の if」へ射影。

### A-5. tokenizer `best_pair_index` — 内部に enumerate+find
scalar-tuple fold（(Int,Int)）自体は通るはずだが、body 内の
`merges |> list.enumerate |> list.find((entry) => …)` が **find の defunc 未対応**
（defunc は map/filter/fold/flat_map/filter_map のみ）。
**最初の一手**: defunc に `find` を追加（write-cursor でなく early-break loop、
結果は Option — scalar 要素なら Option[Int] 材料化、heap 要素は借用 Dup）。
enumerate+find の融合は detect_enum_map_fusion の兄弟で。

### A-6〜8. Matrix native-runtime 依存 3件（per_head_rms_norm / repeat_kv / concat_rows）
**前提が「Matrix 値モデル」ブリック**: Matrix は `@intrinsic("almide_rt_matrix_*")`
の native 型（AlmideMatrix）で、v1 に runtime が存在しない。map/if の形を直しても
callee (matrix.split_cols_even / rms_norm_rows / concat_cols / from_lists / to_lists)
が unlinked。
**設計方針（要決定）**: (a) v1 では Matrix = `List[List[Float]]` の self-host として
実装（to_lists/from_lists が恒等になる。48 fns のうち nn が使う ~12 から）、または
(b) prim 床に AlmideMatrix 相当の flat buffer + (rows, cols) ヘッダを新設。
(a) が最短（既存の nested-list 機構が全部効く）。nn の使用 fn 一覧:
`grep -oh 'matrix\.[a-z_]*' ~/workspace/github.com/almide/nn/src/*.almd | sort -u`

## B. CG-1 spec-keying の全契約展開（F1 完了条件）

機構は完成・稼働済み（`spec = "ALS-xx"` フィールド + check-contracts.sh の解決検証、
mutation テスト済み）。残り = **127 契約中 121 の ALS 節執筆**。
**進め方**: 契約の `statement` は既に規範文なので、クラスタ単位で ALS 章へ昇格する
（例: Cluster-H → ALS-T7 のように、fixture 群を共有する契約束 = 1節）。
優先順: (1) 数値/文字列系（ALS-T の続き、~20契約）、(2) コレクション semantics、
(3) effect/module 系。1節 = 契約 3-8 本を keyed できる見込み → ~20節で完了。
`docs/specs/als/` に章ファイルを増やす（text-and-numbers.md の形式踏襲）。

## C. カバレッジを上げる（F2 残）

現状 65.89%（v0 生産経路込み、per-file 台帳 = proofs/COVERAGE.md）。
**ターゲット順**（出力に効く分岐優先）:
1. `lower/control_p5.rs` 43% — defunc エンジン。A-1〜A-5 の各修正が pin テスト同梱で
   ここを直接上げる（今パスの5テスト昇格と同じ運び）
2. `lower/tail.rs` 61%、`binds_p2` 64%、`binds_p4` 63% — bind/tail の未踏 arm は
   「fixture が無い形」= 新 spec/wasm_cross fixture の種
3. 計測は `bash proofs/coverage.sh`（手動 llvm-cov パイプライン — cargo-llvm-cov の
   オーケストレーションは使わない）

## D. 運用メモ（再開時に必ず確認）

- **バイナリ置換**: 機構特定済み（同一 working copy の並行セッションの make install）。
  刻印が FATAL 停止するので、ゲート前に `make install` を癖にする。watcher は
  `/tmp/almide-binwatch.log`（プロセス名の現行犯記録用、常駐中）
- **並行セッション**: org に `ceangal`（76 walls）を追加した主体。同リポの
  frontend/codegen を触るので、git status の予期しない M はコミットせず放置
- **規律**（今パスで確立、継続すること）: v0 ベースライン先行固定 / 影響半径宣言 /
  計装は台帳管理しコミット前 grep ゼロ / FIXED 宣言は横断バッテリー後 /
  ratchet は分離コミット（lefthook が強制）/ baseline 更新は --update のみ
- **横断バッテリー**: `make install && almide test spec/ && bash proofs/output-parity.sh
  && cargo test -p almide-mir --release && bash proofs/corpus-wall.sh`（PCC 込み全 green が基準線）
