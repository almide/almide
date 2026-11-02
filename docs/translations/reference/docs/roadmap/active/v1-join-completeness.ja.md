歴史的説明：bind 位置の join で継続の複製をなくし、証明済み checker を witness サイズに対して線形にする。
<a id="v1-join-completeness--linear-checkers"></a>

# v1 join の完全性と線形 checker

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-join-completeness.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-join-completeness.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-join-completeness.zh-CN.md)

> 変更していない固定原本の非規範的な歴史的読者スナップショット。自動同期、benchmarkの独立再実行、実装確認はしていない。日付、計測corpus/機械条件、着地済slice、未完段階、失敗試行は原範囲を保持し、形式モデルの主張から有限corpus検査を全実装の安全証明へ格上げしない。記載のclobber/leakとcertificate ACCEPTの併存を保ち、全生成コードの保証へ書換えない。整列fast pathと二次fallback、gateで制限したwitnessと無制限入力、信頼前提不変と受入れ拡張を区別する。歴史的コマンド・CI gate・レビュー人数・commit順序は記録で新たな実行指示ではない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-join-completeness.md)

状態：**Track C は C1/C2/C3/C6 まで、Track J は J0/J2a/J3 まで着地（2026-07-28）。
J1（非variantへの拡張）、J4、J5、C4/C5 は未完。**
独立したコード調査3本（join 機構図、複製一覧、cert 文法と checker 計算量）が完了。
2026-07-27 の trust-spine 事故から生まれた。

J の実測結果：事故 fixture の8連鎖 `main` を1関数に戻し、線形に lowering。
names witness は231,114B → **4,002B**（約58倍）、fan.any を1つ連ねるごとに+480B
（以前は×3.5の乗算増大）。全fan fixture の native/wasm byte 同一（#900 pins）、corpus walls 0、
ratchet clean、kernel oracle 74s、`cargo test -p almide-mir` 597件green。

C-track 実測（ローカル M-series、全corpus）：
- 事故クラスの231KB names witness：**10分超（完了せず）→7〜9s**
  （sorted merge 経路。未整列 FALLBACK は設計上二次のままで、64KB/行 gate がこのクラスを CI から除外）。
- kernel oracle：**254s→74s**、corpus-wall.sh 全体：**5m20s→2m16s**。
- corpus全体の witness bytes：names.cert −30%（1,351KB→939KB）、最大行−33%、caps_graph −24%
  （sort+dedup、集合意味論上健全）。
- proof spine + coqchk + claim-drift + gate.sh + `cargo test -p almide-mir`（597）全green。
  `merge_subset_sound` / `subset_check_fast_sound` を公理台帳に追加、kernel検査済み。

<a id="the-incident-that-named-the-problem"></a>

## 問題に名前を与えた事故

`457273df` が `fan_any_early_winner.almd`（1つの `main` 内に8回連続 `fan.any`）を追加。
fan inliner は match の ok 本体、すなわち関数の残り継続全体を連鎖レベルごとに複製し、
連続呼出で増殖する。末尾約1,700コピー、heap object 6,820個、**231KB names witness**
（corpus中央値88B）。証明済み checker は単一 witness 行に対して超線形なので、[names] は13s→53min、
kernel oracle は4h超で停滞。その後のpushは丸一日すべて CANCELLED（redにはならず）。
応急策は `30ea58de` に着地（fixtureを1関数1ケースに分割、corpus-wall.sh の
`MAX_WITNESS_LINE=65536` gate、`timeout-minutes: 100`）。この取り組みで2つの根因を除く：

1. **lowering が継続を複製する** — 8種類のrewriteが行う（最悪実測：約3.5倍/連鎖call）。
   let に束縛したheap branchにjoinの仕組みがなかったため。うち2つは `MAX_DESUGARED_NODES`
   fixpointの外で動き、上限なし。
2. **証明済み checker がwitness bytesに対して超線形** — Θ(n²) parse、単進nat上の
   Θ(k²·V̄) subset検査、本当に指数的な到達可能性foldが1つ。
   そのため有効だが大きいwitnessが1つあるだけでCIが止まる。

結論前にjoinなしの再符号化をすべて実測：arm-inlining 約2〜3倍/call、
`let $r = <chain>; match $r` 約3.5倍/call（`desugar_let_bound_heap_branch`経由）、
guard付きvar再代入文 約2倍/level（`desugar_unit_if_heap_reassign`が同じ末尾複製へ流す）。
複製はdesugarで消せず、受入れ対象を広げて消す必要がある。

<a id="the-key-finding--the-join-already-exists-the-wall-is-admission-surface"></a>

## 核心の発見 — join は既存、壁は受入れ範囲

調査で得た不可欠な3事実：

- **certモデルはすでにjoinをサポートする。** `Op::IfThen{cond, dst}` は値を生成するmerge。
  armごとの `im` balanceは証明済み（brick 5a、`check_bc_unroll_sound`、“Closed under the global context”）。
  さらに `certificate_b.rs` はRELEASED merge dstに1つの `i` を既に与える
  （`ownership_certificate_released_merge_dsts` / `merge_dst_i_credits`）。
  よって「mergeを束縛しscope末尾でdrop」は今でも `im`/`id` としてcertifyできる。
- **bind位置joinはvariant型で既に動く。** `lower_bind_heap_if` は `is_variant_ty(ty)` を許可
  （Option/Result bindはjoinしてscope追跡される）。「merged resultにはflat certificate上で健全なscope-end dropがない」
  という壁メッセージは、書かれた後に着地した機構について古くなっている。
  String/List bindの壁は受入れ方針でありcertモデルではない。
- **rendererはMIRより先行。** wasmは `IfThen dst` を `(if (result i32/i64) …)` とrenderし、
  値を生成するN-way `br_table` joinも既存（`render_wasm_switch.rs`、純render rewrite）。
  nativeにはif-as-value backpatch joinがある。loweringより下は変更不要。

つまり「join point導入」ではなく「各複製desugarのtrigger集合が空になるまでbind位置join受入れを広げ、
その後削除する」。各段階でcorpus ratchetをかける。

<a id="constraints-violating-any-of-these-is-a-design-bug"></a>

## 制約（どれかを破れば設計バグ）

- **`mir == ir` / 両方の前でdesugar。** `desugar_heap_branches` は `lower_body_into`、
  `count_ir_calls` caps gate、pre-TCOで共有。trigger狭化は3呼出箇所すべて同一に適用する。
- **TCOは末尾複製を利用。** `mod_c.rs` pre-TCOは継続（再帰call含む）をarmへ押し込み、
  `tco_collect` が扱う分岐再帰を作ることに依存。J5までは自己再帰tail callを含む継続の複製経路を残す。
- **CBranchは二分岐、arm本体はFLAT。** 非平衡arm内の入れ子region delimiterは必ず拒否する
  `{i|}` に汚染（設計通り）。join拡張は各objectのarmごとのeventをflatかつbalancedに保つか延期。
  （また `parse_bc` は `{}` 内の2つ目の `|` を冪等に扱い、偽造 `{A|B|C}` を
  `CBranch A (B++C)` とparseする。emitterは作らないがparserを触る際に衛生的拒否を追加する。）
- **新公理なし。** `Print Assumptions` = “Closed under the global context”はclaim-drift gate対象。
  `PrimString`（O(1)文字列prim）は `PrimStringAxioms.v` から本当の `Axiom` をimportするため
  この取り組みでは却下。安易な短期成果として再発見されないよう記録。
- **cert表面の変更ごとに敵対的レビュー**（#49 / elided-callの前例）：
  certificate生成に触るJ段階commit前に、独立agent3人以上がaccept-but-unsafe構築を試みる。

<a id="track-j--join-admission-kill-the-duplication"></a>

## Track J — join受入れ（複製をなくす）

**J0 — 安全brick（動作変更なし）。上限制御完了2026-07-28。**
`desugar_tuple_empty_list_match` と `desugar_list_pattern_match`
（双方200k fixpoint capの外で走る、上限なしの2複製器）に成長上限を追加
（入力より+50k nodes。超えたrewriteは破棄し、matchを誠実にwallする。desugar_heap_branchesの前例）。
`parse_bc` の2本目bar衛生拒否は未完（外観上の問題。emitterは作らず、健全性定理はparse結果を量化）。

**J1 — 非variant heap型のbind位置join。** released-merge-dst credit機構で
`lower_bind_heap_if` / `lower_bind_heap_match` のString/List/typed merge壁を解除
（variant型の前例を一般化）。束縛mergeは `IfThen dst` としてjoinし、scope追跡、scope末尾drop
→ cert行 `im`/`id`。敵対的レビュー必須。gate：corpus walls 0を維持。
所有権countは変わり得る（armごとbalancedへ）が、ACCEPTが保たれれば健全。
部分sliceはJ3とともに2026-07-28着地：VARIANT match-bind join。
`lower_bind_heap_match` が `if` 側に既存のvariant経路を得た
（tail位置で信頼されている同じmatch-VALUE router：custom-variant / variant-value / Result / Option merge）、
bind + scope追跡 + `seed_variant_param`。新cert表面なし（既存tail-trusted router）。

> **試行1は反証（2026-07-28）— 以下の2修正なしに再試行しない。**
> 全面拡張を実装したが必須の敵対的レビューで棄却。独立監査者3人中2人が別々の欠陥でREFUTED、
> 複製経路を固定したlowering unit test 5件も失敗。具体試行は両bind箇所の `is_variant_ty` gate削除
> （`lower_bind_heap_if` は任意heap型で一般merge join、`lower_bind_heap_match` は
> `try_lower_list_match_value` + `desugar_match_to_if` → `try_lower_heap_result_if` fallback追加）、
> さらに `desugar_branch_b.rs` の `bind_join_lowerable` predicateがscalar condの下で
> {LitStr, heap Var, ConcatStr, その入れ子if}のarmを許可し、
> `desugar_let_bound_heap_branch` triggerと `rest_branch_binds` counterに配線して複製を止めた。
> 一切commitしていない。この段落と以下2修正から再導出する。
>
> **欠陥1 — live変数の破壊（潜在、経路変更で露呈）。**
> bare-Var else armを許すと、shadowしないbind（`let b = …; let j = if c
> then "A" else b; b + "/" + j`）がidentity-else accumulator slot fold
> （`try_lower_line_cond_acc`）へ届く。guardはelse armがownedでscope追跡されたVarかしか見ず、
> binderがそれをSHADOWするかを見ない。foldは `b` slotをその場で再束縛するため、
> 後の `b` 読取りが誤blockを返す。native `sB/A` 対wasm `A/A`、両方exit 0、所有権cert ACCEPT。
> developでは到達不能と確認済み（複製desugarが先に捕捉）なので出荷物は壊れていないが、
> 将来のどんな経路変更でも罠になる。fold箇所にrouting不変条件コメントと常設cross-target pin
> （`spec/wasm_cross/heap_result_if_bind_chain.almd::clobber`、2リンク。
> 4以上ではdense branch-liftが各bindをoutlineして隠す）を追加。
> 2026-07-28修正済み（J1前提）：foldにLAST-READER gateを追加。
> `LowerCtx::var_read_counts` が最終本体全体の変数読取り数を数える
> （全本体が見える `cell_vars` の隣で計算。文ごとのlowering自身は後のreadを見られない）。
> `try_lower_line_cond_acc` はaccumulatorの全本体read数がこのbind値内のread数と等しいとき、
> つまりこのbindが最後のreaderのときだけfoldする。ソースのshadow rebindがちょうど提供する条件なので、
> 重なるshadow accumulator（porta `serialize_opts`）はその場foldを維持。他binderは通常経路へ辞退。
> 前提は今や検査され、`desugar_let_bound_heap_branch` が先に捕捉する偶然では維持しない。
> まさにJ1が変える部分。
>
> **欠陥2 — 入れ子heap型のmerge-dst drop class不足（新規）。**
> 拡張はmerge dstを `live_heap_handles` へ入れるがdrop CLASSを登録しない。
> `seed_variant_param` を得るのはvariantだけ。よって `List[String]` mergeはflat `Op::Drop` となり、
> 外側blockを解放して全要素Stringをリーク。監査者2人が独立再現
> （N=2Mでnative有界105MB、wasm無界2.88GB。固定 `--max-memory-size` ならtrap）。
> 所有権certは `d` がflatと再帰dropを区別できずACCEPT。
> 変更前はこの形をWALLしており、拡張は誠実な拒否をcertified leakへ変えた。
> 修正：両join箇所で結果型からmerge dstのdrop classを登録
> （`seed_call_module_heap_drop_route` / `register_owned_heap_eq_drop` に倣う）、
> または着地までflat-drop-exact型（`String`）に拡張を限定。
> 分離不可 — J1と同時に着地し、先行させない（2026-07-28実測）。
> 既存variant joinにはこの穴がない：`Result[List[String],
> String]` merge bindは4MB wasm cap内で100k回churnし正しい値を返す。
> `seed_variant_param` が再帰dropを既に振り分けるため。さらに深い
> `Result[List[List[String]], String]` mergeはloweringせずWALL。
> つまり現時点でdrop classが欠け、かつ到達可能な形はない。
> 今登録しても検査不能なdead codeになる。誠実な順序は、必要な型を許可する変更と同時に呼出を追加し、
> その型のchurn probeで検証すること。
>
> **プロセス注記（意図的に保持）：** corpus-wallはGREEN、walled-real ratchetはCLEAN、
> 全既存cross-target fixtureは通り、証明済みcheckerは両欠陥をACCEPTした。
> 捕まえたのは敵対的レビューだけ。それこそ提案でなくgateとして残す理由のすべて。

**J2 — `desugar_let_bound_heap_branch` 狭化。J2a完了2026-07-28。**
`is_first_ok_chain`（desugar_fan.rs内、chain builderと同居）は `first_ok_chain` が出す形だけ認識。
末尾複製triggerとrest-counterの両方がそれを除外し、chain bindは複製せずvariant bind位置joinへ
（辞退前は3.5倍/call実測）。predicateは意図的に厳密で手書き形は当たらず、J1の受入れと歩調を合わせてだけ拡張。
一般狭化（通常branch bind共通のjoin-admissibility predicate）はJ1待ち。
TCO例外（継続に自己再帰tail callがあれば複製継続）はその段階に適用しchain形には適用しない。
fan chainのarmは外側関数の再帰を含まず `ok($x)`/入れ子chainのみ。
ratchet：corpusのduplication-fire counterをwalled-real baseline同様に登録し、件数は縮小のみ。

**J3 — fan.anyの線形再着地。完了2026-07-28。** `inline_match_over_any` と#900のレベルごとの
fresh-binder機構を削除。match-over-any形は `first_ok_chain`（value位置と共有）+
1つのbind+元のmatchへfold。分割fixtureを元の8連鎖 `main` に戻し常設の線形性回帰とする。
witness-size gateが複製への回帰を関数名付きの即時redにする。
witness：231,114B→4,002B、連鎖callごと+480B。

**J4 — 残る複製器を移行または上限化。** 影響範囲順：`desugar_stmt_control_unwrap`
（各arm末尾へ継続、node数上限なし）、`group_option_result_arms` catch-allコピー、
`desugar_tuple_variant_match(_deep)` fall-through行、`desugar_unit_if_heap_reassign`
（再標的化：J1により複製bindでなくjoinするbindへSSA化でき、生成形は自動的に線形になる）。
各々join形でカバーするか、局所上限+wallへの辞退。

**J5 — join上のTCO、その後削除。** `tco_collect` にjoin形を教える（またはjoinから分岐再帰を再導出）。
その後複製経路と `MAX_DESUGARED_NODES` を削除。終着点は継続コピーなしloweringで、
近づくものがない200k capはdead codeになる。

<a id="track-c--linear-checkers-independent-start-immediately"></a>

## Track C — 線形checker（独立、直ちに着手）

231KB壁の実測内訳（n=bytes、k=ids、V̄=id平均値、F=関数）：
Θ(n²) `split_bar` accumulator-append（両backend）+ Θ(n²) 抽出 `String.sub` destructor
（binaryのみ）+ Θ(k²) `pnats` の `acc ++ [n]` + Θ(k·V̄) 単進nat桁累積 +
Θ(k²·V̄) `subset_check`。さらに `check_prog_cert` の `reaches` はmemoizationなし。
最悪Θ(F·b^F)、spineで最も急な潜在崖（現在はファイルごとの小graphで隠れている）。

**C1 — emitter sort + dedup（証明変更ゼロ）。完了2026-07-28。**
`subset_prop` は集合所属で、`certificate.rs` 自体が「duplicates are harmless」と記載。
`sorted_dedup_ids` が今は `name_witness_string` / `cap_witness_string` /
`transitive_cap_witness_string` / `program_cap_graph_witness` をserialize
（callee edgeもdedup、memoizeしない `reaches` foldを増幅するため）。
corpus：names.cert −30%、最大行−33%。

**C2 — GallinaでΘ(n) parse。完了2026-07-28。** `pnats` / `split_bar` / `split_semi` は
名前とシグネチャを保ちつつ `list ascii` を走査（cons累積、境界で1回 `rev_append`。
`list_ascii_of_string` はO(n)の `List.init` 1回へ抽出）。parseの二次計算4つを両backendで除去。
同file内Examples + 3-way corpus gateで動作固定。

**C3 — 整列済みfast path。完了2026-07-28。** Subset.vの `sortedb` + `drop_lt` + `merge_subset`。
`subset_check_fast` がdispatch、`check_names` / `check_caps` が通る。
merge健全性（`merge_subset_sound`）は整列仮定不要。受理要素は文字通り `sup` のsuffixで見つかるため、
信頼定理は無条件で、整列は完全性のみgateする
（`subset_check` へfallback、全既存witnessはbit同一で検証）。
`CapabilityReach.prog_within` は意図的に `subset_check` に残す（C5までreach list未整列）。

**C4 — id型 `nat` → `N`。** 両backendで単進nat桁/eqbコストを除く。
witness bytes不変、`NameWitness`/`CapWitness`/`Fn` + parserのproof側refactor。

**C5 — `reaches` memoization。** Visited-set BFS（`MSetPositive` — binary `positive` key、
vm_compute向き、既にstdlib closure内）がfuel有界の再展開を置換。
corpus fileがdense call graphを育てる前に指数崖を消す。

**C6 — kernel-oracle衛生。完了2026-07-28。** goalをchunk化
（names/caps/tcaps goalごと500witness、ownership goalごと4,000行。
健全：`check_bc` は行ごとfold、`forallb` は++に分配）。peak termを制限しgoal間でcoqc GCを可能に。
行gate横に `MAX_CERT_TOTAL=8388608` aggregate gate追加（現全corpus総量約1.1MB）。
kernel oracle：254s→74s。

<a id="order-and-gates"></a>

## 順序とgate

C1→C6→J0は独立した即効策（C1+C6はkernel oracle termサイズを即時削減、J0は無上限複製器クラスを閉じる）。
次にC2/C3を1つのproof PRへ。J1が要：敵対的レビュー後、J2 ratchetで進捗を単調かつ可視に。
J3が成果デモ（事故形を線形化）。C4/C5とJ4/J5は後続。

C-track残務を理由付き延期（誠実さの前例）：**C4**（nat→N）は残る単進nat算術、
つまり事故クラスwitnessでまだ払う7〜9sを除く。しかしそのクラスは64KB gateでCIに入らず、
refactorはCallModes/CapabilityReach proof表面に跨るので独立の集中PRとする。
**C5**（`reaches` memoization）は現状小さなファイル別graphに隠された実在の指数崖を閉じる。
踏んでも100分workflow timeoutでREDとなりzombie化しない。caps graphがdenseになった時点で急務、その前ではない。

各段階のgate：`make verify-trust` が約17分でgreen
（事故は判定だけでなく時計を見ることを教えた。3倍遅いgreenは回帰）、corpus walls=0、
walled-realとduplication-fire ratchetは縮小のみ、`almide test` + cross-target parityは不変。

<a id="honesty"></a>

## 誠実さ

- プログラムについて何がPROVENかは変えない。loweringが受け入れるプログラム（増える）とcheckerの速さ
  （大幅）を変える。信頼基底は増えず、C-trackはそれを増やす唯一の手段 `PrimString` を明示拒否。
- J1はcertificate生成、歴史的accept-but-unsafe gapのあった表面そのものに触る。
  だから敵対的レビューは提案でなくgateで、J1はJ2と融合せず単独着地。
- 実測倍率（2〜3.5倍/call）と事故数値は2026-07-27/28 session由来。
  almide-mir file分割が落ち着いた後に依拠するなら再測定する。
