<!-- description: Exact re-entry points for the remaining crush-pass items — nn 8 walls, CG-1 expansion, coverage raising, with per-item diagnosis and first move -->
# Crush-Pass Continuation — 残件の正確な再開点

> **Position**: 2026-07-03〜04 の「全部つぶす」パスの続き。ここまでの成果:
> 拡張3点観測（stdout+stderr+exit）完全ゼロ（baseline 177）、PCC チェーン全量
> ACCEPT（ローカル+CI green）、MIR テスト 512/0、nn walls 22→8、
> spec-keying 機構稼働（6契約 keyed）、miscompile 8クラス根絶。
> 本書は残件それぞれの**診断結果と最初の一手**を固定する。状況の全体像は
> [flight-evidence-gaps](flight-evidence-gaps.md) と memory
> `project_flight_evidence.md` を参照。

> **進捗 2026-07-04（続行パス2）**: nn **8→7**。A-1 ggml load **完了**（3欠陥: Ty::Bytes
> admission / err-arm ConcatStr piece / fs.read_bytes_raw self-host + WASI errno→native
> 文言マップ。probe gl/gm MATCH）。A-2 前進（Range call-arg 材料化 via list.range +
> caps 台帳整合、(String,Int) concat 要素 — probe pm1 MATCH。実物は list.push mutation
> + ADT が残り）。A-3 前進（scalar-aggregate concat 要素、heap-var list literal 引数
> `flatten([first, second])`、tuple unwrap_or→match desugar — 既存の invalid-wasm
> 型崩れも根治。実物 combine は「while 内の let-bind tuple-match」1形残り）。
> 全ゲート green 維持（spec 273 / parity 177 / MIR 512/0 / corpus PCC 0）。
>
> **続報（同日パス3）**: nn **7→6、A-3 fft combine 完了**（実物開通）。追加機構4つ:
> let-bound scalar-tuple Option match の成分 merge 実行（単一 Alloc、cert クリーン、
> LoadHandle i32→Handle 拡幅の型整合込み）、heap-var list literal の call 要素対応、
> scalar-aggregate 要素型の flat_content 追加。nn 残6 = Matrix 依存3 + gguf（list.push
> mutation + ADT）+ best_pair（A-5: find defunc + option.map 連鎖 + fold 多 stmt 拡張）
> + find_chunk（A-4: Option 成分）。A-4/A-5 は scalar-tuple fold の gate
> （stmts.len()==1）拡張が共通の入口。
>
> **続報（同日パス6）**: **A-4 find_chunk_at 完了**（(scalar, Option[scalar]) fold —
> tag+payload 2ローカル、match-over-found の if 射影、len-as-tag 後書きの単一 Option
> 材料化、borrow-view tuple）。実物 wav.almd 開通、3点一致、pin テスト済（**518/0**）。
> **nn 残4 = Matrix 値モデル依存3 + gguf 1（list.push mutation + ADT）**。
>
> **続報（同日パス5）**: MIR テスト **517/0**（strict-value モードで統一実行）。
> テスト昇格が **flatten の rc 規約違反による二重解放**（heap 要素 sublist の生コピー）
> を捕獲 → `list.flatten_rc`（rc_inc-on-copy）+ borrow-view list 引数（Dup なし・
> plain block Drop）で根治。probe 検証を stdout-only から**3点（stdout+stderr+exit）**
> に是正し、全12 probe が完全一致。print_str の旧 SCRATCH(512) 直書きも 768 に追随。
>
> **続報（同日パス4）**: nn **6→5、A-5 best_pair_index 完了**（fold の per-iteration
> preamble stmts 対応 — enumerate+find+option.map 連鎖は既存 bind 機構がそのまま
> 吸収）。残5 = Matrix 依存3 + gguf（list.push mutation + ADT）+ find_chunk（A-4 のみ:
> Option[Int] 成分を tag+payload の2 scalar ローカルへ開く + tail の match 射影）。

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

**完了（2026-07-04）**: 機構（`spec` フィールド + gate 検証、mutation 済み）に加え、
**全 127 契約を 7章59節の ALS へ keyed**: text-and-numbers（T1-T16）/ strings（S1-S6）/
collections（C1-C10）/ runtime（R1-R6）/ data-formats（D1-D7）/ semantics（M1-M13）/
implementation（I1-I3）。check-contracts.sh が spec↔contract↔fixture の三層を全量強制。
今後の新契約は spec キー必須の運用（節が無ければ gate FAIL）。
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
