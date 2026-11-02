> 原文の説明：夜間差分 fuzz の歴史的トリアージ。発見・修正・撤回された仮説を保持する。
<a id="fuzz-findings-triage-re-green-the-nightly-differential-gate"></a>

# Fuzz 発見事項のトリアージ：夜間差分ゲートを再び緑にする

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fuzz-findings-triage.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](fuzz-findings-triage.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](fuzz-findings-triage.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fuzz-findings-triage.md)

> 原文履歴注記：2026年7〜8月の調査記録であり、現在の CI 状態、campaign 実行指示、独立検証した安全保証ではない。fixed/live/closed/残件は歴史上の順序で保持し、明示撤回された容量閾値説や leak 仮説も残す。R3 の途中進捗と後の close は併存し、後段の12番目cell未完調査を解決済みと推測しない。program 数・binary revision・有限 churn loop・出力比較・gate は原文報告の観測で、新規測定や普遍的証明ではない。誤った probe 成功条件と同一 binary/command での訂正も証拠履歴の一部。資源上限による skip、honest wall、abort 契約、観測された leak、誤出力を区別する。引用 command・CI 変更・bug fix は実行していない。


<a id="historical-triage-log-not-live-ci-status"></a>

## 歴史的トリアージログであり、現在の CI 状態ではない

このページは7〜8月のキャンペーン報告を保存し、後のプローブで明示的に撤回した仮説も含む。冒頭の夜間 red 報告は2026年7月とコミット `25585249` のもので、最初の12件は7月18日に clean と記録されている。後続 wave にはそれぞれの日付と結果がある。どの記録も今日の夜間実行が緑か赤かを示さない。現行 CI 状態は実際の workflow run を使い、このログで覆された仮説を新しい発見として再提起しない。

Fuzz (nightly) は2026-07-15以来赤い。生成型差分 fuzzer（`tools/xtarget-fuzz`）は、native ⇄ wasm の本物の観測可能な不一致を発見している。これはまさに cross-target 契約台帳が静かには起こり得ないと主張する種類だ。赤い夜が続くほど、byte-identical という主張と現実が離れる。

<a id="reproduction"></a>

## 再現

```bash
cargo build --release --bin almide          # the oracle binary
cd tools/xtarget-fuzz && cargo build --release && cd ../..
tools/xtarget-fuzz/target/release/xtarget-fuzz replay --seed 1784352208133210990 --index <i>
tools/xtarget-fuzz/target/release/xtarget-fuzz gen    --seed 1784352208133210990 --index <i>  # source
```

保存したコマンドの注釈訳：最初の build は oracle バイナリを作る。最後の gen コマンドはソースを出力する。

2026-07-18実行の12件すべてが現在の develop（25585249）で再現する。#782 の wall 削減で偶然直ったものはない。

注：同 workflow の coverage-ratchet job（#566）も失敗しており、別途調査が必要（job log は `gh api /repos/almide/almide/actions/jobs/<id>/logs`、この repo では `gh run view --log` は空を返す）。

<a id="findings-seed--1784352208133210990"></a>

## 発見事項（seed = 1784352208133210990）

| 分類 | Index | 症状 | 状態 |
|---|---|---|---|
| C. String 関数が `""` を返す | 323, 768, 904 | `ok(float.to_fixed(…))` → wasm `ok("")`、heap-Ok Result 上の `result.map_err` | **修正済み（2026-07-18）** — 根因2つ：(1) ok/err ctor の stdlib-call payload が遅延 Opaque に落ちた（binds_p4 Module-call String arms、C-138）；(2) result の値 combinator が cap-as-tag heap-Ok block に対して len-as-tag scalar 実装を link した（result_call_name の `_h` twins + `_x` walls、C-139） |
| D. Unicode 述語の反転 | 191 | `none` 対 `some("Ǆ")`（titlecase） | **修正済み** — C と同じ根因（値が Result/Option ctor payload を通った） |
| E. i32 境界 tuple | 609 | `(true, -2147483648)` 対 `(false, 2)` | **修正済み** — C と同じ根因 |
| A. 負のゼロの表示 | 67, 655 | native `-0` / wasm `0` | **修正済み（2026-07-18）** — 表示ではない。v1 self-host の `float.round` が `x >= 0.0`（IEEE では -0.0 に対して TRUE）で分岐し符号を失った。copysign で符号を運ぶ（C-140） |
| G. Build/run 失敗 | 65（wasm run 失敗）、96（wasm build 失敗） | 失敗の不一致 | **修正済み** — 65：`list.zip_with` が全インスタンス化に Int 型実装を link し、String zip が scalar closure table 型で trap → element-repr routing + `_str` twin（C-141）。96：v0 emitter の `result.unwrap_or_else` inline に F64 case がなく不正 module になった。option twin を模して追加（C-142） |
| F. Option 反転 | 858 | `some("5")` 対 `none` | **修正済み（2026-07-18）** — `some/ok/err(<heap if>)` payload が遅延 Opaque に落ちて `none` を読んだ。ctor piece match が If/Match String payload を heap-result-if 機構へ route する（C-143） |
| B. List が `[]` に潰れる | 198, 659 | `[1000000, 7, 256]` → `[]`、`[true,true,false,true]` → `[]` | **修正済み** — 4機構：(1) 非 literal scalar-list bind は無言の `[]` Opaque に遅延せず WALL（C-144）；(2) mono suffix 付き stdlib 名（`or_else__Int_String_String`）は base name で route（C-145）；(3) String-err Result capture を closure env に許可（`__drop_list_str` と厳密に同じ layout family）し、捕捉 or_else chain が v1-verified で動く；(4) 捕捉 alias を返す v0 lifted closure が co-owned +1 を渡す（C-146） |

**Seed 1784352208133210990：12/12 件 CLEAN（2026-07-18）。**

<a id="wave-2-seed-20260718-1000-program-campaign-88-resolved"></a>

## Wave 2（seed 20260718、1000プログラムのキャンペーン）：8/8 解決

| Index | 分類 | 解決 |
|---|---|---|
| 0 | NativeBuildFailure | 生成 run project は空の `[workspace]` table で自己隔離する。cargo workspace dir（fuzzer の .scratch）内で almide を動かすと cargo が親 workspace を解決し build を拒否した |
| 9 | RunFailureDivergence | `list.unique_by` を KEY repr で route：String key は `_sk` twin（prim byte-compare 内容等価性）、それ以外は `_x`（C-147） |
| 198 | Hang | Harness 修正：native hang が finding になるのは wasm が正常成功した場合だけ。4GB上限の wasm OOM-trap は停止の証拠ではない（`pos + 0` 変異は両方 hang）。ladder.rs に純粋 classifier と unit test |
| 248 | ok("") | Ctor Var payload は move せず Dup。`let a = ok(r0); let b = ok(r0)` が両方実在し、r0 は生存する（C-150） |
| 259 | WasmBuildFailure | v0 `list.scan` acc store が i64 固定 → valtype の三分岐。v1 `list.scan_str` twin + ACC-repr routing（C-148） |
| 590 | float のごみ値 | v0 result.unwrap_or_else heap-Ok payload の use-after-free → share +1（C-149） |
| 647 | some(ごみ値) | heap-Ok RESULT を持つ `result.map/map_err/flat_map`（scalar input）→ 決定的な `_x` wall（C-151） |
| 888 | err→ok(0) | 未許可 heap-call payload 上の ctor は WALL（bind-net 拡張、C-152。nested-Result drop の許可は F2 後続作業） |

合わせて通した harness/emitter 修正：v0 emit_result_call に実際の `or_else`/`flatten` arm を追加（lowered runtime fn のない pipeline で named-dispatch fallback が ICE になった。host-determinism gate が発見し、現在262/262 byte-identical）。walled-real baseline に DESIGNED-PROBE 節を追加（result_wall_escalations::main が C-152 の wall を固定）。v1 sweep baseline：PASS 288 / WALL 12（恒久9 + 設計どおりの fixture probe 3）/ INVALID·TRAP 0。

<a id="wave-3-seed-20260718-re-campaign-after-the-classifier-fixes-7-unique--1000"></a>

## Wave 3（classifier 修正後の seed 20260718 再キャンペーン）：unique 7 / 1000

意図的 abort の classifier 修正（compile 失敗は finding のまま、runtime abort は3点比較へ流す）により、旧「native non-zero はすべて NativeBuildFailure」規則が隠していた abort 形式不一致が露見した。

| Index | 形 | 分類 |
|---|---|---|
| 10 | corpus mutant：main 内の `assert_eq(sql, "hello")` | **修正済み（2026-07-18）** — ALS-T18：非 test assert 失敗を frontend lowering で一度だけ脱糖（if + eprintln + process.exit(1)）し、4 consumer 全部が `Error: assertion failed…` + exit 1、operand 一度評価を継承（C-153、3 fixture）。合わせて、裸の `eprintln` builtin が v1 で未 link / v0 で ICE だった点を、登録済 self-host（fd-2 print_str twin）+ 共通のパラメータ化 v0 runtime fn に修正 |
| 49 | C-138 fixture mutant | native 101 対 wasm 134 — 両 leg が生 abort 形式を漏らす |
| 119 | C-062 RawPtr fixture mutant | native 1 対 wasm 134（trap）— unsafe-bridge OOB 形式の裁定が必要 |
| 5 | `int.clamp(4, 3, 1)`（min > max） | **修正済み** — ALS-T6 裁定：4 consumer すべて `Error: clamp requires min <= max` + exit 1。float の `!(lo <= hi)` は NaN 境界も同じ行に統合（C-154、2 fixture） |
| 145 | `or_else(ok(..), (a) => ok(..))` | **修正済み** — E025 の未決定 slot validator が中間 call result を見る（推論時に全 call-result ty を enqueue、post-solve check は本当に未固定の slot だけで発火）。両 target が span + hint 付き check 拒否になり、受理一致を回復。fuzzer 分類は GeneratorReject |
| 149 | `Result[Option[Float], ..]` 上の `ok(result.unwrap_or(.., none))` | **修正済み** — option/result `unwrap_or` の heap arm が両 branch（保持 payload + Var default）で CO-OWNED（+1）参照を渡す。#727 share family の unwrap_or 版（C-149 拡張、新 fixture） |
| 98/92 | C-002 Int8-overflow mutant（`neg_one_i8(128)`） | **修正済み** — E024 の call-arg 版：SIZED param に流れる裸の int literal を check 時に範囲検査（context 記録 hook が sized context 内の i64 に収まる literal にも site を作成し、call-arg 推論点に接続）。native rustc が拒否したものを check で拒否し、check-vs-build gap を閉じた。fuzzer 分類は GeneratorReject |

合わせて coverage-ratchet job の赤（もう一つの #795 job）も修正。原因は GNU find が拒否する BSD 構文 `find -perm +111` で、可搬な `/111` にした（proofs/coverage.sh）。Wave-3末尾（batch-5 binary に対する1000プログラム）：index92（check-vs-build）、995（JSON の nbsp/emsp key の表示不一致、新しい形）、998（native exit1 対 wasm trap134、abort形式の漏れ）。Interp に Flow::Exit（process.exit）を追加し、assert 脱糖を3-way oracle で評価できるようにした。

Wave3 は wave1〜2（個別 lowering bug）とは異なる。主に abort 形式の正規化（ALS-T6 が `Error: …` + exit1 を約束する場所に生101panic・134trapが漏れる）と受理一致（未解決型変数が codegen に到達）である。clamp/RawPtr の境界には修正前に ALS の規範的裁定が必要。

乾くまで繰り返す状態：wave3トリアージは open。残る完了条件：finding なし1000-run、coverage-ratchet job の診断、2夜連続 green。

#777/F3 に渡す教訓：C分類の両根因は「遅延／誤 link 値が wall なしに観測出力へ到達」だった。(1) 遅延 Opaque ctor payload が `ok("")` と表示、(2) 名前をキーにした registry link が型の示す layout を無視。F3 gate は両方を構造的にすべきだ。display/eq/観測 op に流れる Opaque は fn を wall し、self-host link は repr 互換性検査を持つ（`_h`/`_x` suffix 規律を機械化）。

<a id="wave-4-2026-07-2830-nightly-findings--the-first-post-revival-campaigns-unit-042"></a>

## Wave 4（2026-07-28..30 の夜間発見事項 — 復活後最初のキャンペーン、Unit 0.42）

0.41 の計測器復活により毎夜キャンペーンが完走した。以下はその完走夜の記録。2026-07-31に develop e610331d で replay・分類した（macOS、check/build段階の判定はhost非依存）。

| # | 夜 / seed | 種別 | 症状 | 状態 |
|---|---|---|---|---|
| 861 | 7/28, 1785217538023450905 | OutputDivergence | native `r15 = 100`、wasm `r15 = 0` — `result.unwrap_or_else(err, closure returning captured Float)` | **解決済み — 帰属確認（B2）**：`020021df`（「is_heap_ty を読んで任意 scalar を closure capture 可能にし、Float と全 sized int 幅が lift で wall しないようにする」）。発見時の closure `(s14) => r12` は FLOAT を capture する。変更前は lift が wall し、HOF は closure 不在で動いてゼロ埋め結果（wasm `r15 = 0`）になった。変更後は capture が lift し両 target が100を読む。機構が厳密に一致し、以前の7021f11fへの疑いは撤回 |
| 535 | 7/28, 同seed | NativeBuildFailure | check が `let m: Int8 = --9223372036854775808` を受理し、後に rustc が拒否 | **解決済み — 帰属確認（B2）**：`6ac44503`（「括弧と単項マイナスの連鎖全体を通じ int literal に到達し、狭い注釈が範囲を定め最終符号が決める」）。C-173（18f96604）は既に7/28夜の compiler に入っていたが、二重単項マイナスを見通せなかった。最終符号を使う chain walk が `--9223372036854775808` を E024 で拒否する機構そのもの。厳密一致 |
| 57 | 7/29, 1785304212462799529 | RunFailureDivergence | C-044変異 fixture：`factorial(n - -2147483648)` — native は停止（LLVM の accumulator tail-recursion elimination が約40億段の非末尾再帰を、≤1まで回り込んで unwind する loop に変換）、wasm は忠実に再帰し約16k frameで `call stack exhausted` trap | **契約化済み（2026-07-31、M2 #1017 option1）** — C-196：call-stack枯渇は観測挙動の約束外にある資源制限。収束境界 fixture `recursion_depth_within_limits.almd`（深さ1000でbyte-identical）、oracle の `one_sided_stack_exhaustion` 規則（unit test ×4）は両leg規則に倣い skip に分類。replay は C-196 を名指しして SKIPPED。depth-guard正規化（両target同じT6 abort深さ）は強化の後続作業として記録 |
| 12 | 7/29, 同seed | WasmBuildFailure | wall は最外症状（「callを持つarmのあるBoolのIf」）を名指ししたが、最小化m1〜m9で根因は別と証明：nested-map literal `Map[String, Map[String, Bool]]` が実体化しない。msv route（`map_heap_val_nested_route`）が許可する内側は `Map[String, String]` のみ。これは正当で、`$__drop_map_msv` が内側2m slotすべてをStringとして sweep するため、内側scalar-value mapだとごみhandleとしてfreeされる（受理するがunsafe）。上のif-condition chainはstatement wallまで拒否を伝播 | **修正済み（2026-07-31）** — 設計したmsb拡張が着地：`map_msv.almd` のkey-sweep drop `__drop_map_msb` / `__drop_list_str_msb`、両drop登録点の `is_map_msb_ty` 型routingとpairs-literal `StrMapSkv` classifier arm、msv from_list/get_or gateを内側scalar-value mapまで拡張（実装はhandle-generic、dropだけ型固有）。replay CLEAN、churn spec testがwasm legのdropを検査。正直な既知残件：外側nested mapの `len`/`contains`/`set` はなおwall（このfindingには不要） |
| 29 | 7/30, 1785389912282950207 | WasmBuildFailure | stdlib call `map.get_or_str_wall` 未link — String値の `map.get_or` variantがself-host registryになかった | **修正済み（2026-07-31）** — `stdlib/map_str.almd` の `map_get_or_str` twin（hit armはstr族のcopy規律に従いdeep-copy、miss armはhval branch形に従いdefaultを返す）、registry entry、`mod_p4_e.rs` の `_str` routing listへ `get_or` を許可。replay CLEAN、spec test追加 |

<a id="wave-4-additions-the-loop-until-dry-tail"></a>

### Wave 4追加（乾くまで繰り返す末尾）

Round2ローカルキャンペーン（2026-07-31、1,283プログラム / 1,093 clean / 165生finding eventをunique2分類へ重複排除）：

| # | Seed | 種別 | 症状 | 状態 |
|---|---|---|---|---|
| L2 | 1785460667454423000 idx5 | WasmBuildFailure | wallメッセージ（「解決不能condition上のif（Call of Bool）」）は誤報。#904のarm-declineをconditionと名付ける罠の2例目。delta-debugging（p1〜p8、g5a〜g5f）は閾値相互作用を示した：完全programはwallだが、r6削除、r1のunwrap-over-nested-if chain簡略化、r4aliasをr1に置換、5要素list短縮、1e-300非正規defaultを0.5へ置換という単独の簡略化すべてが通る。単一構文ではなくlowering budget/相互作用（疑い：elided-call/caps分類、#848 lane）が組合せ時のみ拒否 | **修正済み（2026-07-31）** — 2つの新計測器（cond側trace、捨てられたmodule-call理由trace）がdecline chainを名付け、根因はmatch arm不在と判明。`str_list_literal_elems_lowerable` に `If` 要素armがなく、`["a", (if r2 then "b" else "d")]` が `_ => false` → literal全体拒否 → Opaque → 正直な拒否となった。「閾値」の謎は定数畳込みで、全簡略版の `if` は最適化で潰れ、完全版だけ欠損armへ到達した。修正：String-result literal-armの `if` 要素（cond＝追跡scalar VarまたはBool literal、証明済 `try_lower_heap_result_if` が必ずlowerする形で途中build decline禁止guardを遵守）をpre-checkとelement builder双方で許可。replay CLEAN、`spec/lang/list_literal_if_element_test.almd`（runtime condなのでfoldで経路を隠せない）をwasm legへ、mir601/0 |
| L3 | 1785460667454423000 idx54 | 参照インタプリタとのOutputDivergence | C-182変異fixture（Float32/Float64負literalのcontext typing）：nativeとwasmは完全一致（7行、`-2.5 … 123456792.0 -1.5`）だが両方almide-interpと不一致。共通lowering bug（第3判定器が捕らえる分類）かinterp Float32モデルの穴。replayがinterp期待出力を表示しない報告上の穴も同時修正対象 | **修正済み（2026-07-31）** — 誤った判定器はinterpだった。`let p: Float32 = 123456789.12345679` がf64表記を維持したのは、interpがFloat32型literalを生成時に狭めなかったため（拡張carrier規約ではfloat32.* bridgeで狭めるがliteralはそこを通らない）。両backendは正しくf32へfold（`123456792.0`）。eval.rsのliteral評価で修正（`ty == Float32` 時 `as f32 as f64`）。報告の穴も修正し、finding summaryが最初の差分行を名指し（`first_line_diff`、unit test×2）。C-182にfixture `float32_literal_excess_precision.almd` を追加し3-way gateが恒久的に分類を守る。replay CLEAN、interp tests56/0 |

| # | 出典 / seed | 種別 | 症状 | 状態 |
|---|---|---|---|---|
| L0 | 2026-07-31ローカルB4キャンペーン（707プログラム、607 clean）、seed1785458401504935000 index0 | WasmBuildFailure | wall：「unliftable/closure-list高階引数を持つmap.foldは、このbrickで忠実に実行できない（wallであり誤値ではない）」。最小化t3：HEAP-map accumulatorとmap-literalを返すclosureを持つ `map.fold` | **修正済み（2026-07-31）** — 鍵となる洞察：Option[String]の `Some(s)` は1要素DynListStrそのもの（`materialize_opt_str_some`）なので、Option[String]値はskv内側mapと同じ「len@4で数えるString slot」規律に従い、msb dropをそのまま利用できる。修正は純粋な型gate拡張：`is_map_msb_ty`、`StrMapSkv` pairs classifier、2つのmsv routeが内側 `Option[String]` を許可。foldには修正不要。`map.fold_skv_hacc`（skv主語に任意heap acc）が既存で、literal bodyがlowerするとclosureもliftした。replay CLEAN、spec test（get_or some/none/miss、fold-into-mso-acc、churn）をwasm legで実行。既知残存セル：mso `get_or` 結果への `??` はread-shape seedingを欠く（正直にfallback、記録済み、L0には不要） |
| L1 | L0最小化中（probe t1）に発見 | Checkerの穴（ICE、両target） | SCALAR accumulatorの `map.fold` closure引数の検査不足。型不正body（(k,v,acc)順序間違いによる `Int + String`）が `almide check` を通りIR verifyで死ぬ。list.foldと具体heap-acc map.foldは正しく拒否 | **起票済み** — [#1018](https://github.com/almide/almide/issues/1018)、0.52 hole-hunt行へ割当（silentではなくloud、generatorはwell-typed closureのみ生成するためnightly赤リスクではない） |

Round3キャンペーン（2026-07-31、1,304プログラム / 1,108 clean / 163生event → unique1）：

| # | Seed | 種別 | 症状 | 状態 |
|---|---|---|---|---|
| L4 | 1785464182375979000 idx0 | WasmBuildFailure | `Map[String, Result[Int, String]]` 値（get_or/remove）が `result.unwrap_or` に入り、STRICT-mode「value subset外のscalar binding」拒否 | **修正済み（2026-07-31）** — 記録した編集リストどおり着地。族のinner集合を `is_msv_family_inner` に集約（内側String-keyed map、Option[String]、TがscalarかStringのResult[T,String] — 「len@4で数えるlow-32 String handle」blockすべてで、`$rc_dec (param i32)` wrap下でtag-safe）。3route（get_or/remove/from_list）を統一し、`is_map_msb_ty` とpairs classifierを拡張、新 `map_remove_msv`（set_copy規律で1slot省略）を登録。issue-sweep台帳からもう1つの罠を再確認：rc_incを使うhelperは `coown_names.rs` whitelistにも入らないと未link描画になる。replay CLEAN、spec test（ok/err get_or、remove、churn）をwasm legへ、mir601/0 |

Round4キャンペーン（2026-07-31、1,271プログラム / 1,072 clean / unique3）：

| # | Seed1785466162321453000 | 種別 | 症状 | 状態 |
|---|---|---|---|---|
| L5 | idx985 | RunFailureDivergence | C-045変異：`for i in 0..<4294967295 { list.push(ys, i*2) }`（約34GB push）。nativeは64bitアドレス空間で完了、wasm32はlinear-memory上限に達しmemory-size境界ちょうど（0x80010000）でOOB memory fault。allocatorがgrow失敗後にきれいにabortせず末尾を越えて走る | **修正＋契約化済み（2026-07-31、M2 #1019 option1完了）** — 3層：(1) 拒否された `memory.grow` で `$oom` 発火；(2) 第2層根因は **i32 frontier overflow**。1回約2.1GBの要求で `$bump = p + n` が2^32を越えてwrapし、wrapしたbumpがgrow checkを飛ばし、block書込みが境界ちょうどでmemory末尾を越えた（観測0x80010000 fault）。`$alloc`/`$alloc8` のunsigned wrap guard（`new < old → $oom`）と `$list_new` のcap×8乗算wrap guardで修正。probeは規定の `Error: out of memory` + exit1を表示；(3) C-197着地（active197/flagged0、収束fixture `allocation_within_limits.almd` byte-identical）。oracleの `one_sided_memory_exhaustion` skip規則（unit test×3、fuzzer26/0）はC-196のstack規則に倣う |
| L6 | idx33 | WasmBuildFailure | 同じ「List argument cannot be faithfully materialized」分類 — 許可集合外の次のlist要素セル（要素種別はsourceから確定予定） | **LIVE — 分類済み**：`List[Option[Map[String, Bool]]]` literal（`[some(["k0": true, …]), some(n1), …]`）がOption-map defaultを持つ `list.get_or` に連鎖。この合成は「len-counted String slot」規律を破る。`some(map)` blockのslot0はMAP handleなので、flat rc_decではlast-ref時にmapのkeyをleakし、dropはpayload routingが必要。既存の足場：`materialize_opt_aggregate_some` + `optrec:<drop_fn>` route（`Op::DropWrapperRec`）が `Option[record]` にまさに同じことをする。修正はopt-of-map sibling（payload drop_fn＝msb/skv map sweep）+ list要素許可 + その要素型への `list.get_or`。実装設計（q1/q2でprobe。wallは `try_lower_bind_heap_fresh_scalar_list`、binds_p2_b.rsの明示nested-ownership gate）：(1) map_msv.almdに静的3層drop `__drop_list_omb` / `__drop_list_omb_loop`。要素ごとにoption block（1slot DynListStr）がlast-refかつ `len@4 == 1` なら、slot0のMAP payloadを `__drop_msb_inner` にrouteしてからblockをrc_dec；(2) 構築：新 `ListElemDrop::OptMapSkv` classifier arm（Tuple→Option[Map[String,scalar]]要素）と、opt-aggregate block経由で `some(<map literal|tracked var>)` を実体化するelement lowering（map handleをslot0へmove/Dup）、`none` は0len block、drop `"list_omb"` として登録；(3) 要素型への `list.get_or` 許可（heapelem hshare route）；(4) churnを含むfmt済spec test。第2パスで精密化：`List[Option[String]]` / `List[Option[Int]]` literalは既にbuildできる。機構は `lenlist_elem_class`（repr_sources_d.rs）がgateするCtorFlat/CtorLenLoop経路で、「one-level-exact」payloadだけ許可（scalar→Flat、String/flat scalar list→共通生成 `$__drop_list_lenlist` 経由LenLoop）。Map payloadが除外される理由は内部sweepが必要だから。msb洞察は合成でき、len-loop規律は再帰的（option block len-slot→map len-slot→String）で静的 `__drop_list_omb` がそれを表す。`lenlist_elem_class`/`$__drop_list_lenlist` をその場で拡張しない（既存利用者の共通dropを変える）。別 `OptMapSkv` class→drop `"list_omb"` を、binds_p3_b classifierのlenlist armより前（約243行）で決める。要素payload実体化（some(map-literal)）はmapをfrom_list機構へrouteする必要がある。`try_lower_option_ctor` のaggregate-payload pieceが到達するか確認し、しなければ拡張する。**修正済み（2026-07-31、第4パス）** — 2部品が既存だったためbrickは設計より小さくなった。map literalは `map.from_list` callへ脱糖され、opt-ctorのcomputed-map piece arm（`is_str_int_map_ty`＝任意heap-key/scalar-value map）が既に実体化する（q6）。着地：(1) 静的3層 `__drop_list_omb`（list slot→option len-slot→msb key sweep、len-loop規律が合成）；(2) lenlist arm前の `ListElemDrop::OptMapSkv` がdrop `"list_omb"` 付きctor element経路へroute；(3) 新link罠を発見修正。静的dropはDropVariant opから描画されdemand-linkerのCallFn scanには見えず、map_msv.almdへの唯一の需要がDROPだと `$__drop_list_omb` が未解決。linkerはmsv-family static-drop opがあればmoduleを強制（value_core先例）。i33 replay CLEAN、spec test（literal/get_or/churn）をwasm legへ、mir601/0。[#1020](https://github.com/almide/almide/issues/1020)（単独 `let o = some(map_var)` bindの露出）は同日反証されclosed。C-197 OOM abortでleakが観測可能となり、仮説上最悪のdrop順（`keep = some(n1)`、optionがmap varより長生き）を含む3churn variantが32byte keyで150〜200万反復して一度もabortしなかった。`some(<live heap var>)` は **Dup** し、optionが+1を共同所有するため、どちらが先にdropしてもflat rc_decで釣り合う。記録した教訓：コードを読んだ仮説はfindingではない。起票前に測る。今日自分で作った計測器が決着をつけた |

Nightly2026-07-31（run30608326062、05:59UTCのhead、B2帰属確認までの全修正を含む）：キャンペーン完走（full-budget streak4/14）、finding2件 —
| # | Seed1785477905242041784 | 種別 | 状態 |
|---|---|---|---|
| N1 | idx93 | NativeBuildFailure：`literal out of range for i32` | **修正済み（2026-07-31）** — call-arg loopで宣言tuple-variant payload型をliteral ctor argへ固定（ctor callにはcall_sigがないので位置をhookが見ていなかった）。回帰testで罠を固定：大文字ctor calleeはIdentではなくTypeNameとparseされ、最初の修正はIdentだけmatchして無言で未発火だった。両reproともE024、範囲内payloadは受理維持。frontend + literal-domain（8/8）+ spec/lang（159）green |
| N2 | idx9 | WasmBuildFailure（再びcondition-wall誤報の形） | **修正済み（2026-07-31）** — 以前の「規律が成立しない」判断は誤り。`try_lower_tuple_construct` はtupleを `DynList {{ len = slot count }}` として構築しheap slotをhandle保存するので、全String tupleはlen-counted規律そのもの（len2、両slotはString handle）で `__drop_msb_inner` が正確にfreeする。N2は純粋なgate拡張に縮小：`is_msv_family_inner` + `is_map_msb_ty` + pairs classifierが全String tuple値を許可。replay CLEAN、両target probe一致、spec test（get_or hit/miss+churn）をwasm legへ、mir601/0。**Wave4 live数：0** |

戦略的注記（round1〜4）：約1,300プログラムのキャンペーンごとに、より深い型合成の新セルが1〜3現れる（map値→option値→result値→option-of-map要素）。手書きfamily/drop registryはfindingごとに1セル増え、機械的な最終形はcompleteness-by-construction（型由来drop routing）への道筋。それまでは地道な反復こそ設計で、streakベースの完了条件がそれを吸収する。
| L7 | idx757 | NativeBuildFailure | checkがrustc拒否のFloat32 literalを受理：`error: literal out of range for f32` — 535整数literal-domain分類のfloat sibling（C-173は整数のみ） | **修正済み（2026-07-31）** — E024のfloat twin：`FloatOverflowSite` queue（f64でfinite、f32でinfiniteのliteralに事前限定）+ 注釈bindingのcontext pin（int hookに倣い括弧/単項を辿る `float_literal_chain`。裸literal自体の解決型は `Float` のままなのでFloat32 contextはbindingに置く）+ post-solve validator。replayはE024付きGENERATOR REJECT。回帰test `float32_range_is_checked_and_precision_is_not`（両符号の範囲外を拒否、過剰精度とplain-Floatは受理）。frontend + literal-domain + spec/langすべてgreen。興味深い由来：このsessionで追加したC-182 fixtureをfuzzerが変異させて発見 |

Round5キャンペーン（2026-07-31、1,385プログラム / 1,198 clean、Wave4とnightlyの全修正込み）：unique2、両方新分類（修正済みセルの退行なし）。

| # | Seed1785481464003870000 | 種別 | 症状 | 状態 |
|---|---|---|---|---|
| P1 | idx961 | **OutputDivergence — 無言の誤値分類** | `bytes.read_f32_le(verts, dst + 9223372036854775807)`（index演算をoverflowさせるi64::MAX offset）：nativeは `z=0.0`、wasmは `z=2.0`。つまり範囲外byte readはtarget間で構造的等価ではない。2件中最優先。`bytes.read_*` の境界規律はmemory-safety面であり、nativeが0.0を返す場所でwasmが隣接値を返すのは、一方が未検査（または検査が違う）ことを示す | **修正済み（2026-07-31）** — 契約裁定は不要。文書化挙動「範囲外→+0.0」は既に規範で、両legがoverflow offsetで守れなかっただけ。根因はguard `pos + k > n`。`pos = i64::MAX` で加算が負へwrapしcheckを通ってreadが進む（wasmは隣接値、nativeはアドレス演算が偶然ゼロに着地）。通常の範囲外は正しく、overflow帯だけが漏れた。API完全性規則で族全体を修正：`bytes_core.almd` の **14** 箇所すべてをoverflow-safeな `pos > n - k` に変更（lengthは非負かつ≤2^31なので `n - k` はoverflowしない）。replay CLEAN、回帰test `bytes reads are +0.0 at an overflowing offset` がf32/f64/u32/u16/i32と変わらない範囲内/通常OOR結果を検査、spec/stdlib112files green |
| P2 | idx 27 | WasmBuildFailure（scalar-element list literal、要素がsubset外） | 要素は `map.get_or(list.get_or([<map literals>], 1, ["k0": true]), "ΑΒΓ", false)` — `list.get_or` に渡す `List[Map[String, Bool]]` リテラル。mapの値の行列は広がったが、要素がmapのlistは次の合成境界（L6を一段外へ：option-of-map→裸のmap要素） | **修正済み（2026-07-31）** — 裸map list要素クラスを導入：`ListElemDrop::MapSkv`（`binds_p3_b.rs` のlenlist armより前のclassifier arm）、`stdlib/map_msv.almd` の静的2段 `$__drop_list_mb`、demand linkerの `MSV_STATIC_DROPS` に `list_mb`、spec test3件（literal/index/get_or、churn loop、計算された要素）。p27とb3–b15の全probeはbuildしnative ⇄ wasm一致。spec/lang159＋spec/stdlib112files、almide-mir601unit tests green。以下は誤った道で1session費やしたため残す調査履歴。**最初の分類は誤りで、probeで訂正した**：map-element cell欠落ではない。7probe（w1–w7）は、束縛済み `List[Map[String,Bool]]` literal、引数位置のinline literal、要素としての `map.map` HOF結果、fold由来String mapなど、同じに見える部品を含めすべてBUILDするため、単一構文の欠落ではない。decline chainは実在し追跡済み（`[defunc] fold route declined (rolled back)` → `[scalar-call] map.get_or declined: "List argument cannot be faithfully materialized"`）。つまりFOLDのdefunctionalization rollbackが後のlist引数materializationを汚染する、L2同様のstate/threshold相互作用でありtype-cell gapではない。二分探索（b1–b5）でwallには3条件同時成立が必要、どれか1つを除けばbuild：(a) `list.fold` 由来String（b2：literalへinlineするとbuild）、(b) inner list内 `map.map` HOF要素（b4：通常map literalへ置換するとbuild）、(c) inner listの**4**要素（b3：2ならbuild、b5：3ならbuild）。上流foldとHOF要素がある3→4要素の崖はBUDGET/stateの特徴でtype-cell gapではない。L2同族だが閾値を数えられるのでsession最有力の手掛かり。訂正（b6）：要素数の崖ではない。最後2つが1-key mapの4要素はBUILD。inner list内の総key-value組数で並べるとb3=4組✅、b6=6✅、b5=8✅、p27=12❌。よって閾値は8〜12組のSIZE budget（総entry数に比例するops/value-id/instruction数）で、foldとHOF要素も加わる場合だけ達する。これが先の3条件解釈の理由。境界確定（b7=10組✅、b8=11✅、p27=12❌）：崖は厳密に12番目のentry、前後1key。共有key集合説は試験で反証（b9：同一key集合の2要素でも6組ならbuild）されたためinterningではなく数。すべてのprobeはfoldとHOF要素も加わるlist引数materialization経路の11→12map entry付近の硬い容量限界と整合。残作業は機械的だが新鮮なcontextが必要：経路をinstrument（b8対p27のops/value-id数をdump。差は1keyなので数opsの差、越えるcounterは明らかなはず）し限界を上げるか除き、旧上限2倍のspec testで固定。引継ぎ注意：これは容量wallなのでHONEST wall（誤bytesなし）、正しさではなくcoverageの損失。**再訂正 — 今度は1つのbinaryでprobe集合を再現可能（2026-07-31）。** 上記境界表の「BUILDS」はすべて無効：単一binaryでb3/b5/b6/b7/b8/b9/p27を `almide build <f> --target wasm` により再実行するとb3（2要素4組）を含めすべてwall。崖もsize budgetも12番目閾値もない。wasm経路を実行しない成功条件のprobe runから出た解釈で、証拠として記録すべきではなかった。手続上の教訓：境界表が証拠になるのは、すべてのcellを同じcommand・binary・1runで測った場合だけ。
3行reproへ縮小すると最初の分類が復活：`let xs: List[Map[String, Bool]] = [["k0": true]]` の後に何らかの利用を続けると `non-empty List[heap] literal with nested-ownership elements` でwall。同じ位置の `List[Int]` literalはbuild（m9 control）。つまりP2は初見どおり、**要素がmapのlistリテラルというcell欠落** — L6が閉じたoption-of-mapの裸map版。容量wallでもfold/HOF相互作用でもない。修正設計（検討済み、実装準備完了。L6のoption-of-map経路が、間接参照1段少ない厳密なtemplate）：
(1) `stdlib/map_msv.almd` — `__drop_list_omb_loop` の隣に2段drop追加。`__drop_msb_inner(ih)` はmapのkey String掃引とmap blockのrc_decを既に行うため、loop bodyは1callで、ombのoption-block len-slot操作は不要：
`fn __drop_list_mb_loop(h, n, i) = if i >= n then () else { __drop_msb_inner(prim.load64(h + 12 + i * 8)); __drop_list_mb_loop(h, n, i + 1) }` および `fn __drop_list_mb(xs: List[Map[String, Bool]]) = { let h = prim.handle(xs); if prim.load32(h + 0) == 1 then __drop_list_mb_loop(h, prim.load32(h + 4), 0) else (); prim.rc_dec(h) }`。
(2) `crates/almide-mir/src/lower/binds_p3.rs` — `ListElemDrop::MapSkv`。
(3) `binds_p3_b.rs` — `Map[String, Bool|Int|Float]` のclassifier armを `OptMapSkv` armとともに `lenlist_elem_class` callより前へ配置。同じ理由（lenlistがmap payloadへNoneを返し、nested-ownership wallへfall-throughするため）。drop-handle matchに `list_mb` を登録。
(4) `pipeline_c.rs` — `MSV_STATIC_DROPS` に `"list_mb"` を追加しdemand linkerが `map_msv` を強制導入するようにする。
(5) `spec/stdlib/` の既存「list of option-maps」test隣にspec test、target間観測可能と分かれば `spec/wasm_cross` fixtureも追加。
要素のmaterializationは既に機能するはず — 現在 `OptMapSkv` はoption block内にmap literalを構築するので裸mapはwrapperを除いたもの。dropを書く前にm5reproで検証する。なお依然有効な行き止まりの記録：先に変数へ束縛しても回避できない（b12）。literal自体で1つ手前のmessageでwallになり、より明快なreproとなる |

<a id="definition-of-done"></a>

## 完了の定義

1. すべてのfindingを最小化（`gen` → delta）し根因を特定、修正（ledger規則に従う `spec/wasm_cross/` fixture＋contract entry付き）またはHONEST WALL化（無言の誤値は不可）する。
2. Class Cのwall coverage穴を構造的に閉じ、機構を#777 tracking-set/wall-consistency gate設計へ反映。
3. coverage-ratchet job失敗を診断し修正（または#566規律どおり独立commitでfloorを再正当化）。
4. local `xtarget-fuzz run --count 1000` campaignでfindingなし、その後Fuzz (nightly)が2夜連続green。

<a id="ownership-boundary"></a>

## 所有範囲の境界

fuzzer自体（generator、oracle ladder、delta-debugger）は `tools/xtarget-fuzz` にあり、このstreamの対象ではない — 対象はfindingだけ。triage中に発見したfuzzer bug（例：verdict誤分類）は独自test付きで随時修正する。

<a id="wave-5--round-6-local-campaign-2026-07-31"></a>

## Wave 5 — Round 6ローカルcampaign（2026-07-31）

P2修正済みbinary（0.41.0 install、run全体で同一binary）で3,990programs /30min / `--jobs 6`。**unique finding2件**でB4「0 findings」条件は未達。

| # | seed / index | 種別 | 形 | 状態 |
|---|---|---|---|---|
| R1 | 1785489842024900000 / 5 | WasmBuildFailure（honest wall） | 条件がcallである `if` を要素に持つ `List[String]` literal | **修正済み** — admission predicateが保護対象builderより厳しかった。`try_lower_heap_result_if` は一般条件（pure scalar、またはheap引数を取りBool/Intを返すpure call。引数はcondごとのframeで解放）を既にmaterializeし、不可能なら全体rollbackする。predicateが「condはtracked VarかBool literal」という狭い規則を再記述し、builder対応形をwallにしていた。predicateはarmへ再帰し条件をbuilderへ任せる修正 — `binds_p3`/`binds_b.rs` の `str_list_literal_elem_lowerable`。spec test3件 |
| R2 | 1785489842024900000 / 3132 | **nativeのみhang** | `matrix.masked_multi_head_attention(q, k, v, n_heads = -2147483648)`。nativeはhang、wasmはshapeを出力しexit0 | **修正済み** — C-198：head-count族全体へ単一domain規則、両targetで定義済みabort、4testのmatrix gate。下記参照 |

<a id="r2-root-cause--one-function-two-divergences-and-a-family-that-guards-inconsistently"></a>

### R2根因 — 1関数、2つの乖離、一貫しないguardの族

`almide_rt_matrix_mha_core`（runtime/rs/src/matrix.rs:480）は `let n_heads = n_heads as usize;` で始まる。`n_heads = i64::from(i32::MIN)` では18446744071562067968となり、`n_heads == 0` early-outは発動せず、`for h in 0..n_heads` が約1.8e19回反復する — hangの原因。

self-hosted `__mha_impl`（stdlib/matrix_activations.almd:285）にはhead-count guardが一切ない：`let dh = dm / n_heads`。したがって同じ関数で2つの乖離がある：

| `n_heads` | native | wasm |
|---|---|---|
| `0` | 空matrixを返す（明示early-out） | **ゼロ除算trap** |
| `< 0` | `as usize` →約1.8e19反復loop→**hang** | 約0反復→ゼロ埋め(sq,d)matrixを返す |

いずれも点的欠陥ではない。`almide_rt_matrix_rms_norm_heads`（matrix_p2.rs:450）は既に `n_heads.max(1) as usize` と書く。同じ危険を族の一員でguardし他ではしない。まさにCLAUDE.mdのAPI-family規則が防ぐための点的な拡張である。

**族**：head countを取るすべての `matrix` fn — `multi_head_attention`、`masked_multi_head_attention`、`rope_rotate`、`rope_rotate_at`、`rope_rotate_neox_at`、`rms_norm_heads`、および `n_q_heads` / `n_kv_heads` を取るGQA entry point。
**完全性規則**：head count `< 1` は全memberでdomain外、全memberが両targetで同じ報告をする。現在の3挙動（hang/trap/無言で1へclamp）を1つへ統合。gateは族×{i32::MIN,-1,0}のmatrix testで、次のmemberを未guardのまま出荷することを止める。

**Wave5の両findingは同日close。** 共通の教訓はCLAUDE.mdのAPI-family規則に既にある：R2は1つの質問への3つの点的回答、R1はguard対象より狭く再記述したguard。どちらも機能欠落ではなく、1点ずつ拡張されたsurfaceだった。

<a id="wave-6--round-7-local-campaign-2026-07-31"></a>

## Wave 6 — Round 7ローカルcampaign（2026-07-31）

Wave5修正済みbinaryで3,424programs /25min / `--jobs 6`。**unique finding1件**（Round6の2件から減少）でB4「0 findings」条件はなお未達。

| # | seed / index | 種別 | 状態 |
|---|---|---|---|
| R3 | 1785492509375906000 / 17 | WasmBuildFailure — `OptionSome argument cannot be faithfully materialized (a heap payload outside the executable subset)` | **CLOSED（2026-08-01）** — 4cellすべてbuildしnative ⇄ wasm一致。C-201＋ `spec/wasm_cross/option_tuple_payload_matrix.almd` がmatrix gate。native-render残件→#1037 |

<a id="r3--the-tuple-payload-family-is-keyed-three-different-ways"></a>

### R3 — tuple payload族のキーが3通り

最小repro（2行）：

```almide
let s0: Option[(String, Int)] = none
let r: (String, Int) = option.unwrap_or(some(option.unwrap_or(s0, ("a", 1))), ("b", 2))
```

payloadはtuple literalではなくCALL RESULT。同一binary・1runで測定したsweep：

| `Some` payload型 | admission key（`binds_p4_b.rs::try_lower_opt_tuple_and_variant_payloads`） | call-result payload |
|---|---|---|
| `(Int, String)` | `_ if Self::is_int_str_tuple(&expr.ty)` — **型のみ** | ✅ build |
| `(String, Int)` | `IrExprKind::Tuple { .. } if is_str_int_tuple(…)` — **形＋型** | ❌ wall |
| `(String, String)` | `IrExprKind::Tuple { .. } if is_str_str_tuple(…)` | ❌ wall |
| `(Int, Int)`（全scalar） | `IrExprKind::Tuple { .. } if is_all_scalar_tuple(…)` | ❌ wall |

4cell中1つは型キー、3つは形キー。tuple literal payloadは4つすべて機能（s1）、call結果を先に `let` へ束縛しても4つすべて機能（s8）する — inline call-result payloadだけが分断する。本日同族の3例目で、R2（head-countの1問に3回答）、R1（builderより狭く再記述したadmission predicate）に続く。

<a id="fix-design-and-why-it-was-not-implemented-in-the-same-pass"></a>

### 修正設計と同じpassで実装しなかった理由

非対称なのはmatch armだけでなくbuilder：

* `try_opt_int_str_tuple_payload` → `self.lower_owned_heap_field(expr)` — 任意の式からowned heap valueをmaterializeするため、cellが型キー。
* `try_opt_str_int_tuple_payload` / `_str_str_` / `_scalar_` → `IrExprKind::Tuple { elements }` を分解し `try_lower_tuple_construct(&elements)` / `try_lower_scalar_tuple_construct(&elements)` を呼ぶ — 構造上literal限定。

修正は3つの形キーbuilderにint_str既存と同じfallbackを与えること。payloadがliteral `Tuple` でなければ `lower_owned_heap_field`、次に既存 `materialize_opt_str_some`＋既存 `variant_drop_handles` entry（`opt_str_int` / `opt_str_str`）を通す。

**別passにする理由となるリスク**：各cellに登録されたdrop handleはpieceの形を仮定し、`lower_owned_heap_field` が `try_lower_tuple_construct` と同じ形を作るとは限らない。違えばfallbackはdouble-freeかleakとなり、wallではなくmemory-safety欠陥になる。確認には両piece構築を `opt_str_int` / `opt_str_str` drop bodyと照合しrefcount誤りを露呈するchurn-loop testを走らせる必要がある。慎重な作業なので長いsession終盤には意図して着手しなかった。上記診断は実装を探索でなく慎重な機械作業にするだけの完全さがある。

probe file：session scratchpadの `s_s1`..`s_s11`。識別的な3つはs9（`(Int, String)` call result→build）、s5（`(String, Int)`→wall）、s11（`(Int, Int)`→wall）。

<a id="r3-outcome-2026-07-31"></a>

### R3結果（2026-07-31）

保留理由だったdrop規律のリスクは**発生せず、理由を検証できる**：`materialize_opt_int_str_some` と `materialize_opt_str_some` は結果を登録するownership set以外byte-for-byte同一。両者は `Op::Consume`＋ `live_heap_handles.retain(…)` でpieceを受け取るため、構築経路は解放方法に影響できず、dropはpayloadの型 — layout特性 — で選ばれる。よってfallbackは単にもっともらしいのでなく安全になった。

3つの形キーcellすべてが、`(Int, String)` cellに元からある `lower_owned_heap_field` fallback付きの型キーになった。同一binary・1runで検証：s5 `(String, Int)`、s6（lambda形）、s9 `(Int, String)`、s10 `(String, String)` すべてbuildしてnative ⇄ wasm一致。spec312files green（lang159/stdlib112/integration41）、加えてchurn loopを含む新test5件。ownership推論が誤りならrefcount誤りを露呈するもの。

**名前付き残件 — 全scalar cell。** `(Int, Int)`（s11）はなおwall。`lower_owned_heap_field` が全scalar tuple call結果をmaterializeしないため — tuple armはheap pair（`is_flat_heap_pair_ty`、`is_str_closure_pair_ty`、`is_flat_heap_scalar_pair_ty`）を扱い、全scalar tupleはどれにも一致しない。族は4/4でなく3/4対称。この記録は最後のcellを再発見でなく名前付きgapにするため。閉じるにはOption cellを再度触るのではなく、`lower_owned_heap_field` に全scalar tuple call結果を教える。**さらに限定（2026-07-31）**：tupleリテラルは既に対応 — `owned_heap_field.rs` の `IrExprKind::Record { .. } | IrExprKind::Tuple { .. }` armが `try_lower_scalar_tuple_construct` でscalar-only aggregateを許容する。`(Int, Int)` を返すCALLは別arm（aggregate groupの `CallTarget::Module` → `lower_pure_module_value_call`、`calls.rs:89`）を通り、そこでdeclineする。修正はtuple経路でなくCALL経路：`lower_pure_module_value_call`（または上位arm）に全scalar aggregate結果のmaterializeを教える。**再限定（2026-07-31）、しかも「call経路」より狭い**：USER-FN call結果は両tuple形で機能 — `s_s12`（`fn pair() -> (Int, Int)`）と `s_s13`（`fn pair() -> (String, Int)`）は `some(pair())` 経由でともにBUILD。全scalar tuple cellが一般に壊れているのではなく、fallbackが対応する。

残るwallは特に `some(option.unwrap_or(s0, (1, 2)))` — MODULE call生成の全scalar tuple（`s_s11`）。同じmodule callで `(String, Int)` はbuild（`s_s5`）し、両形は同じfallbackから `lower_owned_heap_field` へ到達する。差は選択armだけ：`is_all_scalar_tuple`（先に検査し `(Int, Int)` を取る）対 `is_str_int_tuple`。残る問いはなぜ同じfallbackが一方のarmで成功し他方では失敗するか — おそらく `repr_of(Option[(Int,Int)])`、または全scalar tuple結果がString含有の場合のように `live_heap_handles` で追跡されず、`materialize_opt_str_some` の `Consume`＋ `retain` が異なる扱いをするため。

次の手順：`s_s5` 対 `s_s11` の経路を `lower_owned_heap_field` のreturn地点でdiff。tuple要素型だけ異なる入力に、一方は `Some(id)`、他方は `None` を返す。両probeは2行でsession scratchpadにある。

**元のR3programは1段後でwall**、空listに対する `list.fold(list.sort(tmp3), …)` が `List argument cannot be faithfully
materialized (would borrow an empty deferred heap value)`。なおhonest wall（exit1、出力なし）で誤bytesではない。別cellでありregressionではない。

<a id="dogfood-findings-unit-046-2026-07-31"></a>

## Dogfoodのfinding（Unit 0.46、2026-07-31）

fuzz findingではない — 実program（`tools/almide-gates`）を書いて発見し、live defect ledgerがここにあるため記録する。

| # | 形 | 深刻度 |
|---|---|---|
| [#1029](https://github.com/almide/almide/issues/1029) | `for` loop bodyで呼ぶ `effect fn` がauto-unwrapされず、nativeは不正Rustをemit、wasmはheap addressを値として出力 | **#1030で修正済み** |
| [#1030](https://github.com/almide/almide/issues/1030) | **#1029の根因** — list concatが右operandの要素型を制約しない | **修正済み** — `infer_plus_op_concat` が `unify_infer` でなく `constrain` を使う |

**#1030は今まで見つかったchecker許可・lowering再解釈族の最小例**：effectもconcurrencyもgenericsもない3行。対比は厳密：`[1, "a"]`（literal）はE001で正しく拒否されるが `[1] + ["a"]`（concat）は拒否されない。同じ特性を一方で検査し他方ではしない。

**coverageへの示唆。** 実program180行が、spec324filesとfuzz campaign7回で見つからなかったものを見つけた。spec corpusはcompilerを知る人が書くが、仕事のために書くprogramは誰もtestを思いつかなかった形を使う。同族が1日3例（#1027,#1029,#1030）、detectorでなく偶然発見された。これがladder row0.52（#912）のhole-hunt視点を支持する。

<a id="1030-fix-2026-07-31"></a>

### #1030修正（2026-07-31）

`check/infer_statements.rs::infer_plus_op_concat` の1call変更：

```rust
- self.unify_infer(le, re);
+ self.constrain(le.clone(), re.clone(), "list concatenation element");
```

`unify_infer` はinference variableを束縛し、両側がconcreteで異なると無言のまま。`constrain` もunifyするので規則の存在理由であるinference-variable case（`List[?0] + List[Int]`）は影響を受けないが、unification不可能なら報告もする。

両reproは両targetでcheck-time errorになった：

- `[1] + ["a"]` → `error[E001]: type mismatch in list concatenation element: expected Int but got String`
- `out = out + [one(p)]`（#1029）→ `… expected Int but got Result[Int, String]`

2つ目は注目に値する。concatがhonestになり、#1029のmessageはeffect callが `Result`、listが `Int` を求めるという誤りを正確に述べ、ユーザーを「Almide bug」panicでなく `!` へ導く。1修正2報告で、#1029だけのために書いたはずの診断より良い。

回帰：335files green（spec324＋examples11）、diagnostic harness green、さらに `tests/diagnostics/e001-concat-element-mismatch`。

<a id="r3-remainder--one-hypothesis-refuted-2026-08-01"></a>

### R3 の残件 — 仮説を1つ反証（2026-08-01）

検証して排除した仮説：**「全要素が scalar の tuple は heap ではないため別経路を通る」**。
実際は heap です。`crates/almide-types/src/types/heap.rs:62` は `Ty::Tuple(..)` を無条件に heap と分類します。
match は wildcard なしで網羅され、113行の単体テストが `Ty::Tuple(vec![Ty::Int, Ty::Bool])` を heap 側に固定しています。

したがって、OptionSome のペイロード経路で `(Int, Int)` と `(String, Int)` を区別するのは heap か否かではありません。
両者は同じ fallback から `lower_owned_heap_field` に達し、どちらも heap です。違うのは選択した分岐のみです
（先に検査する `is_all_scalar_tuple` と、`is_str_int_tuple`）。

これは最初に思いつく推測であり、捨てると誰かが同じ20分を費やすため、反証として記録します。
残る作業は、`lower_pure_module_value_call`（`calls.rs:89`）のどの早期 return が
`option.unwrap_or(s0, (1, 2))` では発生し、`option.unwrap_or(s0, ("a", 1))` では発生しないかの計装です。
2つの probe はセッション scratchpad の `s_s11` と `s_s5`、それぞれ2行です。

**第2仮説も排除**：`repr_of`（`lower/mod.rs`）は tuple を拒否しません。早期 `Err` は `Ty::Unknown` だけであり、
その下の scalar 幅の表は分類であって gate ではありません。よって拒否箇所は `repr_of` でもありません。

**ここで意図的に追跡を止めます。** 3つ連続で仮説を排除してもセルは閉じませんでした。
これは次に必要なのが追加読解ではなく計装（各 probe が `lower_pure_module_value_call` のどの分岐を通るか出力）だという合図です。
推測を続けたことが、このサイクルの先の P2 bracket を撤回に至らせました。その規律のコストはここでも払う価値があります。

疑いのない点：このセルは正直な wall（exit 1、出力なし、誤バイトなし）であり、正しさでなく網羅性を損ないます。
区別する2行 probe は scratchpad の `s_s5`（build 成功）と `s_s11`（wall 拒否）です。

<a id="r3-remainder--the-decline-is-localised-to-one-call-with-a-named-next-hypothesis-2026-08-01"></a>

### R3 の残件 — 拒否を1つの呼出しに局所化、次の仮説を明記（2026-08-01）

読解で追跡し、2つの probe で確認：

- `s_s5` `Option[(String, Int)]` → `try_opt_str_int_tuple_payload` → `lower_owned_heap_field(expr)` → **成功**
- `s_s11` `Option[(Int, Int)]` → `try_opt_scalar_tuple_payload` → `lower_owned_heap_field(expr)` → **失敗**

異なる分岐ですが、同じ式形状で同じ呼出しを行います
（`option.unwrap_or(s0, <tuple literal>)`、`CallTarget::Module`）。異なるのは結果の型のみです。
したがって拒否は `lower_pure_module_value_call`（`calls.rs:89`）内で、型依存です。

**次に検証する仮説**（「早期 return を計装」より具体的）：stdlib の `option.unwrap_or` はペイロード型で振分け/単相化され、
String を含む tuple のセルはあるが全 scalar のセルはない。
`self_host_registry.rs` / `stdlib_info.rs` で `option.unwrap_or` 特殊化の登録方法と、全 scalar tuple のインスタンス化が存在するかを確認します。

それが原因なら修正は lowering 変更でなく registry 項目であり、先の仮説が示唆した所有権解析変更よりはるかに小さく安全です。

<a id="r3-remainder--a-wrong-root-cause-retracted-2026-08-01"></a>

### R3 の残件 — 誤った根本原因を撤回（2026-08-01）

上記の仮説は正しく、欠落は正確に特定できました。`unwrap_or` はペイロード型で振り分けられ、2モジュールのセル集合は非対称です：

| payload | `result` | `option` |
|---|---|---|
| String | `result.str_unwrap_or` | `option.unwrap_or_str` |
| Value | `result.value_unwrap_or` | `option.value_unwrap_or` |
| List[Value] | `result.list_value_unwrap_or` | `option.listvalue_unwrap_or` |
| List[Int] / List[String] | （flat 経由） | `option.listint_unwrap_or` / `option.liststr_unwrap_or` |
| **flat scalar block**（全 scalar の TUPLE、`List[<scalar>]`、`Bytes`） | **`result.flat_unwrap_or`** | **— 欠落 —** |
| heap fallback | `result.unwrap_or_hx` | `option.unwrap_or_hx` |

`Option[(Int, Int)]` はどのセルにも一致せず、振分けが `None` を返し呼出し側が wall 拒否します。
`Option[(String, Int)]` は全 scalar でないため heap 経路で build できます。
probe `s_s11` と `s_s5` の違いはこれだけです。

**このサイクルで発見した他の問題と同じ類型**：族を一点ずつ育てた結果です。
`result` は `result.zip` の `(Int, Int)` が必要になったとき flat セルを得ましたが、fuzz プログラムが要求するまで誰も
`option` の対を必要とせず、追加しませんでした。

**具体的修正：**

1. `stdlib/value_core.almd` に `option_flat_unwrap_or` を追加します。`result_flat_unwrap_or`（705行）を手本としますが OPTION layout 用です。
   Option は len-as-tag であり、Result の cap-as-tag-at-@16 ではないので本体のコピーではありません。
   len slot を読み、`0` = none なら fallback を返し、それ以外なら payload handle を load し、裸の Var 末尾の自動取得に任せます。
   同じファイルの隣接する `?? d`-over-`Option[List[String]]` helper の方が近い雛形です。コメントも「同じ len-as-tag prim」と書いてあります。
2. `self_host_registry.rs` の `("result_flat_unwrap_or",
   "result.flat_unwrap_or")` の隣に登録します。
3. `unwrap_or_call_name_option`（`lower/mod_p4_f.rs`）へ振分け分岐を追加し、`result` 分岐の述語を写します
   （`Ty::Tuple(ts)` の全要素が scalar、または非 heap 要素の `List`）。
4. **同じ PR で行列 gate を追加**。族の規則は `option` と `result` が同じ payload セルを持つことで、
   セル集合の一致を主張するテストが次の乖離を止めます。この欠落はそれを検査するものがなかったため存在しました。

probe は scratchpad の `s_s11`（wall）と `s_s5`（build）、各2行です。

**撤回 — 上記は誤りです。** 出力される名前（`Some("option.*"` と `Some("result.*"`）を grep して比較し、
`option` に flat セルがないと結論しました。しかし存在します。
`unwrap_or_call_name_option_flat_scalar`（`lower/mod_p4_f.rs`）は
`Ty::Tuple(ts) if all non-heap` に一致し `option.listint_unwrap_or` へ振り分けます。
option 側は別の `option.flat_unwrap_or` を作らず、その名前を flat 族全体に再利用しています。
したがって名前レベルの比較は、存在しない欠落を読み取ってしまいます。

**述語でなく名前を読んだため自信のある誤答を出しました**。これは撤回した P2 bracket と同じ失敗の別の姿です。
防げた規則：2つの族を比較するとき、名前でなく何に一致するかを比較する。

その結論から加えた変更（`stdlib/value_core.almd` の `option_flat_unwrap_or` と登録）は戻しました。
戻す作業自体が2度失敗したため記録します。削除処理が直前のコメントブロックを遡り、隣の `option_liststr_unwrap_or` まで削除しました。
その間に `almide fmt` が1100行ファイル全体を整形していたため、diff では私の編集だけを分離できなくなっていました。
HEAD からファイルを復元するのが正解でした。HEAD との差分関数一覧が隣接関数の欠落を検出し、
`spec/stdlib` の再実行は一覧検査だけでは見逃した第2の兆候（通常6の native fallback が43）を検出しました。

**よって R3 の原因はまだ不明です。** 排除済み：heap 分類（`Ty::Tuple` は heap）、`repr_of`（`Ty::Unknown` のみ拒否）、
そして振分けセルの欠落（セルは存在し述語がこのケースを覆う）。変わらない点：`s_s5` は build、`s_s11` は wall、
違うのは要素型のみ、両方が同じ呼出しで `lower_owned_heap_field` へ到達します。
次は変わらず計装による分岐出力であり、追加読解ではありません。

<a id="r3--narrowed-again-the-decline-is-before-the-call-not-inside-it-2026-08-01"></a>

### R3 — 再び絞込み：拒否は呼出し内でなく、その前（2026-08-01）

`lower_pure_module_value_call` を末尾（`calls.rs:227+`）まで読むと、`repr_of` 後に名前を選び
`Op::CallFn` を push して `Ok` を返します。末尾に型依存の失敗はなく、整形式の `option.unwrap_or` は両ペイロード型で成功します。

先の局所化（「拒否は `lower_pure_module_value_call` 内で型依存」）の後半は誤りでした。
分岐する場所はこの呼出しではなく、**片方はここへ到達しません。**

すると `lower_owned_heap_field` の group dispatch が候補です。その `_leaf` group は
`Option<Option<ValueId>>` を返し、2つの否定形を明確に区別して文書化しています：
`None` =「自分の group でない、次へ」、`Some(None)` =「自分の group だがこの field は拒否」。
`_leaf` 内の分岐が全 scalar tuple に一致して `Some(None)` を返すなら、router はそこで止まり、
`CallTarget::Module` 分岐を持つ `_aggregate` を試しません。

**次の probe**（安く決着する）：`owned_heap_field.rs` で、全 scalar tuple 型の `Call` 式に一致する `_leaf` 分岐があるか確認します。
それが拒否するならそこがバグで、修正は呼出し経路でなく分岐の guard です。

排除済み仮説：heap 分類、`repr_of`、振分けセル欠落（撤回済み、セルあり）、そして
`lower_pure_module_value_call` 内の型依存失敗。排除のたび記録を残したことだけが、堂々巡りでなく絞込みになった理由です。

<a id="r3--fifth-hypothesis-eliminated-and-a-note-on-when-to-stop-reading-2026-08-01"></a>

### R3 — 第5仮説も排除、読解を止める時点について（2026-08-01）

`_leaf` group に `Call` 分岐は一つもありません。分岐は `LitStr`、`BinOp::ConcatStr`、
`BinOp::ConcatList`、`StringInterp`、`Lambda`、`Var`、`Member`、`TupleIndex` です。
したがって `Call` 式はペイロード型に関係なく `_aggregate`（`CallTarget::Module` 分岐あり）へ通過します。
「`_leaf` 分岐が全 scalar tuple を拒否」という仮説は誤りです。

**読解で5仮説を排除し、確認はゼロ。** この比率が合図です。読解はもう割に合わず、追加の推測ごとに撤回のリスクがあります。
このサイクルでも「振分けセル欠落」がすでに撤回され、戻す際に stdlib ファイルまで壊しました。

**次のセッションは読むのでなく計装すべきです。** 具体的には `lower_owned_heap_field` と
`try_opt_scalar_tuple_payload` の各 `return`/`?` 箇所に一時的な `eprintln!` を置き、build して
`s_s5` と `s_s11` を実行し、trace の diff を取り、print を除去します。
2回の build と diff で、丁寧な読解5回でわからなかった箇所を一度で名指しできます。

繰り返しを防ぐため記録した排除済み仮説：
1. 全 scalar tuple は heap でない → heap である（`heap.rs:62`、単体テストあり）
2. `repr_of` が拒否 → `Ty::Unknown` だけを拒否
3. `option` に flat 振分けセルがない → 存在し述語もケースを覆う（撤回）
4. `lower_pure_module_value_call` が型依存で失敗 → 末尾は常に `Op::CallFn` へ到達
5. `_leaf` 分岐が Call を拒否 → `_leaf` に `Call` 分岐がない

ずっと不変の点：`s_s5` は build、`s_s11` は wall、要素型だけが異なります。
wall は正直（exit 1、出力なし、誤バイトなし）であり、失うのは正しさでなく網羅性です。

<a id="r3--instrumented-and-the-answer-arrived-in-one-pass-2026-08-01"></a>

### R3 — 計装すると一度で答えが出た（2026-08-01）

読解5回では何も見つからず、一時 `eprintln!` 付き build 2回ですぐ見つかりました。
応用できるのは手法なので、答えと手法の両方を記録します。

**式は `Call` でなく `Match` です。**

```
[R3] NO group claimed it
[R3] scalar_tuple: lower_owned_heap_field -> false for kind
     Match { subject: IrExpr { kind: Var { id: VarId(0) },
     ty: Applied(Option, [Tuple([Int, Int …
```

`option.unwrap_or(s0, (1, 2))` は `lower_owned_heap_field` 到達前に `match` へ脱糖されます。
**排除した5仮説はすべて、この式が決して通らない `Call` 経路についてでした。**
丁寧に読んでも見逃し続けた理由は、推論は正しく前提が誤っていたからであり、trace だけがその前提を示せました。

**実際の拒否箇所。** `owned_heap_field.rs:476` には確かに `Match` 分岐があります：

```rust
IrExprKind::Match { subject, arms } => {
    let if_expr = self.desugar_match_to_if(subject, arms, &expr.ty)?;
    let IrExprKind::If { cond, then, else_ } = &if_expr.kind else { return Some(None) };
    let obj = self.try_lower_heap_result_if(cond, then, else_, &expr.ty)?;
    …
}
```

よって拒否は `desugar_match_to_if` または `try_lower_heap_result_if` 内です。
`(String, Int)` probe は同じ分岐へ10回達して build できるため分岐自体は動き、この2つのどちらかが全 scalar tuple を拒否します。
最有力候補は、脱糖後の `else` 分岐が tuple リテラル `(1, 2)` で、`try_lower_heap_result_if` が分岐値として具体化する必要がある点です。
（`try_lower_heap_result_if` 自身の `is_heap_ty(result_ty)` guard ではありません。`Ty::Tuple` は heap です。）

**次の手順**：同じ trace を一段下へ。`try_lower_heap_result_if` / `lower_heap_result_if_inner` の
各 `?`/`return` 内で出力し、`s_s11` を走らせ拒否する分岐を読む。build は1回です。

**手法の一般化。** 丁寧な読解5回は計装 build 2回より高くつき、誤りました。
兆候は排除5・確認0の比率でした。残すべき規則：*排除が確認より多く何も収束しないとき、前提がおそらく誤りであり、どの前提か示せるのは trace だけです。*

<a id="r3--closed-and-the-trace-beat-five-readings-by-two-builds-2026-08-01"></a>

### R3 — 解決、trace は build 2回で読解5回を上回った（2026-08-01）

**原因を1行で。** `lower_owned_heap_field` の `Match` 分岐はリテラル分岐連鎖しか知らず、
`option.unwrap_or` の脱糖先である `some`/`none` 対を即座に拒否していました。

前の記録の推測は誤りなので、見えるままにしておく価値があります。そこでは `try_lower_heap_result_if` が拒否すると推測しました。
呼出し地点で `inner -> {is_some}` を出す trace はコンパイル全体を通して `false` を一度も出さず、この関数は拒否しません。
拒否は一段前の `desugar_match_to_if` で、第2の trace は到達形状を出しました：

```
[R3m] subj_ty=Applied(Option, [Tuple([Int, Int])]) arms=2
[R3m]   pat=Some { inner: Bind { var: VarId(2), ty: Tuple([Int, Int]) } } guard=false
[R3m]   pat=None                                                        guard=false
```

`desugar_match_to_if` は Bool の2分岐形式を扱い、それ以外は `build_match_chain` に落ちます。
これは int リテラル分岐と catch-all を求めるため、variant pattern 対はどちらにも一致しません。

**修正は新機構でなく、他の6箇所がすでに行う呼出しです。**
`try_lower_variant_value_match` はまさに「Option/Result 対象の match を値へ lowering」で、
拒否時は自身の `ops`/`lifted`/`live_heap_handles` mark を戻します。
`binds_p2`、`calls_p4`、`control_p3`、`tail_b`、`heap_result_ctrl_arms` はこれを最初に試します。
`owned_heap_field` だけがリテラル脱糖へ直接進む match-lowering 箇所でした。
既存分岐の前に `is_variant_ty(&subject.ty)` で制限した1分岐を置きます。

**族は今や標本でなく行列です。** 1つの binary を1回走らせ測った4要素型組合せ
— `(Int, Int)`、`(String, Int)`、`(Int, String)`、`(String, String)` — はすべて build でき native ⇄ wasm で一致します。
`spec/wasm_cross/option_tuple_payload_matrix.almd`（C-201）が実行可能 gate なので、
将来要素型で族を再分割する変更は、fuzzer の再発見を待たずここで失敗します。

**見過ごさず測定した残件。** 修正後、NATIVE の検証済み render はこの族を
`ownership verification failed` で wall 拒否し、標準 codegen へ fallback します。
修正が壊したように見えるため、一時 env guard の背後に新分岐を置き、1つの binary で A/B 比較しました：

| program | 分岐なし | 分岐あり |
|---|---|---|
| `option.unwrap_or(s0, (1,2))` を `let` に束縛（この分岐には到達しない） | ownership verification failed | ownership verification failed |
| 行列 fixture | outside the MIR-lowering subset | ownership verification failed |

第1行で決着します。所有権 wall はこの分岐が触れないプログラムにも存在します。
修正が変えたのは第2プログラムの wall の理由で、「subset」から「ownership」へ変わりました。
双方 wall で、片方は厳密に情報が増えています。#1037 に登録し C-201 の文中で明記したため、
契約は stdout+exit の一致を約束するだけで、存在しない検証済み native render を主張しません。

**再び手法。** 計装 build 2回で読解5回が見逃したものを見つけ、第3の trace が第2から生まれた推測を訂正しました。
前記の規則 — *排除が確認より多く何も収束しないとき、前提がおそらく誤り* — は成立しました。
今回追加する系：**1仮説を確認した trace も、行動前に次の仮説へ向ける価値があります。**
`try_lower_heap_result_if` が拒否するという推測は「最有力候補」と書いたものの単に誤りで、
return 箇所の `eprintln!` 1つはその推測を生んだ読解より安く済みました。

<a id="a-cheap-rule-the-r3-fixture-bought-2026-08-01"></a>

### R3 fixture がもたらした安価な規則（2026-08-01）

新 fixture は全 local gate — `spec/lang`、`spec/stdlib`、`spec/integration`、`spec/wasm_cross`、
全 `cargo test`、target 間バイト比較 — を通ったのに CI の `Emit & Format` を失敗させました。
`almide test` は整形を検査せず、fmt gate は `spec/` と `examples/` を対象とするためです。

**`spec/` 以下を追加/編集したら `almide fmt --check spec/ examples/` を走らせる。**
数秒で済み、テストコマンドからは含意されない唯一の gate です。
（formatter は文間の空行も除去するので、段落間隔で書いた fixture は必ず書換えられます。diff に驚く前に知っておく価値があります。）

範囲に注意：gate は `spec/ examples/` のみです。`tools/` は意図的に対象外で、
dogfood を整形すると複数行シグネチャやパイプ連鎖が長い1行へ畳まれるため、そこでの正解は「修正」せず放置することです。

<a id="q1_0-sign-of-zero-1358-fuzz-nightly-run-31667874852"></a>

## Q1_0 のゼロ符号（#1358、fuzz-nightly run 31667874852）

Seed 506685997636 / index 703 — `spec/wasm_cross/matrix_select_rows_oob.almd`（C-229）の変異。
全ゼロ18-byte block の `matrix.select_rows_q1_0` は native `-0`、wasm `0`。
等しいと比較されるため下流は気づかず、だからこそリリースを生き残りました。

**根本原因：1つの綴りで2つの答え。** native の逆量子化器は負側分岐を `let neg_scale = -scale;` と書き、
self-host wasm decoder は `0.0 - scale` と書いていました。非ゼロ scale ではビット一致しますが、
`scale = +0.0` では違います：`-(+0.0)` は `-0.0`、`0.0 - 0.0` は `+0.0` です。

**裁定：大きさゼロの逆量子化要素は `+0.0`。** 量子化要素は *大きさ*（block の fp16 scale）× *方向*
（Q1_0 符号 bit / Q8_0 int8 quant）です。大きさ0では bytes は方向を符号化していないため、
IEEE-754 が返す符号はデータでなく演算の書き方の産物です。
`+0.0` は残りの API がすでに作るゼロでもあります（OOB→zeros 行、列数非倍数 guard、`vec![0.0; _]`）。
非ゼロ weight は正確な符号と bits を保ちます。float decoder の `from_bytes_f16_le` / `_f32_le` / `_f64_le` は
明示的にこの規則の対象外で、そこでは保存された `-0.0` がデータそのものです。

**全体確認で同じ decoder にさらに2つの乖離、どちらもより重大。** Q1_0 と Q8_0 は2-byte fp16 block scale を共有します。
10個の raw scale を probe すると、native `fp16_bits_to_f32` と wasm `__q10_scale` は3種の答えで異なりました：

| fp16 入力 | native（修正前） | wasm（修正前） |
|---|---|---|
| `0x0000` 全ゼロ block、符号 bit 0 | `-0.0` | `0.0` |
| `0x0001` … `0x03FF`（非正規化数） | `5.96e-8` … `6.0975e-5` | **`0.0` — ゼロ化** |
| `0x7C00` / `0xFC00` / `0x7C01` | `inf` / `-inf` / `NaN` | **`65536` / `-65536` / `65600`** |

非正規化数の行は実 weight の無言の大きさ誤りで、符号だけではありません。
wasm はすべての `exp == 0` パターンで符号 bit だけを返しました。`exp == 31` 行はさらに悪く、
wasm に Inf/NaN 分岐がなく通常数の式へ落ちていました。
`__fp16_bits` を native decoder に忠実に揃えることで両方を解決しました（仮数正規化を再帰で実装、`0xFF` 指数分岐）。

**族の全体確認 — 検査対象と、すでに問題がなかった部分。** wasm 経路を持つ全量子化 loader/byte loader のゼロ block 挙動：

| loader | 修正前 | 判定 |
|---|---|---|
| `select_rows_q1_0` | native `-0.0` / wasm `+0.0` | **乖離** — 報告されたバグ |
| `from_q1_0_bytes` | native `-0.0` / wasm `+0.0` | **乖離** — 同根の対の decoder、報告にはなし |
| `select_rows_q8_0_dq` | 両経路で `-0.0`（zero scale × negative quant；±0 scale × どちらの quant） | 一致するため fuzzer は見つけないが同じ規則違反、修正済み |
| `select_rows_f32` | 同一、保存した `-0.0` も残る | 問題なし |
| `from_bytes_f32_le` / `_f16_le` / `_f64_le` | 同一、保存した `-0.0` も残る | 問題なし — それで正しい |
| `linear_q1_0_row_no_bias` / `linear_q8_0_row_no_bias` | native のみ（wasm は wall） | target 間の主張なし；要素でなく内積を公開する |

`runtime/rs/burn/matrix_burn_p4.rs` には同じ `-scale` の q1_0 decoder 第3コピーがありますが、
その backend は `[lib]` に入らずビルド対象でもないため、意図的に触れませんでした。

**教訓と gate。** C-229 fixture はこの入力をすでに覆い、合格していました。
主張していたのが大きさ（list 表示ではどちらも `0` となる値への `${x}`）で、乖離は符号 bit だったためです。
bit pattern を主張する fixture は bit pattern を観測する必要があります。
ゼロは今や `-0.0` を表示する `float.to_string` で表記します。
新 C-269 fixture は両 q1_0 入口を通して fp16 scale 全定義域を固定し、対の decoder が再び乖離しないようにします。
A/B 検証：修正前 binary では双方の fixture が失敗します。

<a id="1375--one-source-shape-two-independent-wrong-value-mechanisms-2026-08-13"></a>

## #1375 — 1つのソース形状に、独立した2つの誤値機構（2026-08-13）

`json.parse("hi") ?? json.parse("[1,2]")!` は wasm で `true`、native で `[1,2]` を表示しました。
出力 WAT を一度読むだけで機構が判明：`??` は正しい骨格へ lowering（tag は `+16`、payload は `+12`、
`tag != 0` が fallback を選択）しましたが、fallback 分岐末尾は
`(local.set $v13 (call $json.parse …)) (local.get $v13)` となり、tag を読まず伝搬もせず `Result` block 全体を merge に渡していました。
ヒープ分岐 lowering は `e! == e` を適用しますが、これは RESULT 型分岐では恒等でも payload 型ではありません。

この記録の追加規則：**機構へ絞り込んだ後で近傍全体を再実行する。1つの表面形状に複数の機構がありうるためです。**
`a ?? f(..)!` を `(match a { ok($p) =>
ok($p), err(_) => f(..) })!` へ書換えると11セルが直りましたが、同じ分岐をリテラル `match` と書いた12番目はなお `true` を表示しました。
出力コードに理由がありました：その綴りは `branch_lift` へ達し、分岐を合成した非 effect の `fn … -> <payload>` に持ち上げます。
持ち上げ helper 内の `!` には外へ出るチャネルがなく、native は非 `Result` fn 内に `?` を出力（rustc E0277、明示的失敗）し、
wasm は helper 本体を常に包み、呼出し箇所は wrapper を裸の payload として束縛（無言の誤り）しました。
1形状2機構であり、第2が見えたのは最初だけでなく、第1修正後に近傍を再測定したからです。

修正を実行可能にした ANF も残す価値があります。`(match a { … })!` は wall 拒否されます。
`desugar_let_unwrap` が作る伝搬 match の対象自体が match になるためです。
先に束縛（`let $u = match a { … }; let v = $u!`）すると、対象位置が実証済み形状である `Var` になります。
文が1つ増えるだけで評価は同じく1回、呼出し回数は不変です。
fallback 自体を持ち上げる ANF は誤りです。`??` は ok 経路で fallback を評価してはならず、fixture は未使用 fallback が失敗するセルでこれを固定します。
