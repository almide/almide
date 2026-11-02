原文の説明メタデータ：自己再帰末尾呼出しをスカラー状態ループへ — yaml parser の要石
<a id="v1-tco--self-recursive-tail-calls--scalar-state-loop-the-yaml-parser-keystone"></a>

# v1 TCO — 自己再帰末尾呼出し → スカラー状態ループ（yaml parser の要石）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-tco-self-recursion.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](v1-tco-self-recursion.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-tco-self-recursion.zh-CN.md)

> 原文コミット固定の非規範閲覧用スナップショット。原文を正本とし、自動同期しない。独立した実行、実装検証、証明検査、測定は行っていない。歴史的コマンドや設計指示は資料であり、実行した指示ではない。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-tco-self-recursion.md)

> 原文注記：日付付きの設計・訂正・改良・着地の順序を保持し、提案された全再帰クラスの出荷を主張しない。初期の途中結果 break primitive 必須という主張は、後の最終スカラーから再構成し loop 後に結果を出す subset により限定された。より難しい heap-carried・match-leaf・相互再帰は別の未完項目として記録に残る。初期の全 yaml がスカラー更新という想定と「heap 引数なし」は、不変 String 引数や後の反する棚卸しと併存し、黙って整合化しない。壁の件数は異なる時点で、22→19には先行 float.parse 修正を含む。checker 受理と例のバイト一致は範囲付き原文観測であり、普遍的停止・stack 安全性・memory 安全性の証明ではない。テスト・証明・深い入力を独立再実行していない。

**状態：設計（2026-06-19）。健全性に重大 → 設計優先、新規実装 + 敵対的検証、出荷前に証明済み checker に対して検証する。checker が拒否した witness は決して出荷しない。**

<a id="why"></a>

## 理由

`almide/yaml` の v1 の壁（28/74）は直接自己再帰 parser（`scan_quote`、`find_colon_at`、`oct_rec`、`bin_rec`、`flow_rec`、`esc_rec`、…）が支配的。heap-result-`if` arm lowering（`lower_heap_result_arm`、control.rs）は自己再帰呼出し arm を WALL にする（`if name == self.fn_name { return None }`）。v1 には TCO がなく、本物の再帰 `CallFn` を出力すると深い入力で wasm call stack が溢れる（fail-stop trap）ため。**v0 はこれらを TCO する**（2026-06-19 検証：深さ500kの自己再帰 `Option[Int]` が v0 で overflow なく動く。`pass_tco` の `is_tco_candidate` = `can_default_init(ret) ∧ all self-calls in tail position`）。従って TCO なしに v1 で再帰を実行すると深い入力で v0 と乖離（v0 は loop、v1 は trap）し、バイト同等ではない。guard は正しい。修正は v1 に v0 と同じ TCO を与えること。

<a id="the-key-simplification"></a>

## 主要な単純化

どの yaml 自己再帰 parser でも、自己呼出しが変更するのはスカラー引数だけで、ヒープ引数は不変のまま渡される：

```almide
fn scan_quote(s: String, pos: Int, in_q: Bool) -> Option[Int] =
  if pos >= string.len(s) then none                       // base → result, break
  else { let c = string.get(s, pos) ?? ""
    if in_q then scan_quote(s, pos + 1, not (...))        // recurse: pos,in_q := …  (s unchanged)
    else if c == "#" and … then some(pos)                 // base → result, break
    else scan_quote(s, pos + 1, …) }                      // recurse: pos,in_q := …
```

コードコメント訳：基底ケースは結果を出して break。再帰では pos、in_q を更新し、s は不変。後の基底 arm も結果を出して break、最後も pos、in_q を更新して再帰。

唯一の heap 引数 `s` は全自己呼出しで同一。変わるのはスカラーの `pos`/`in_q` だけ。従ってここでの TCO はスカラー状態ループで、`try_lower_scalar_while`（control.rs:1336）が `LoopStart`/`LoopBreakUnless`/`LoopEnd` + `Op::SetLocal` でスカラーの loop-carried 状態を再代入し、既に cert-clean に lower する形そのもの。heap 引数の再代入は不要（heap loop-carried 状態なし → `in_frame` heap 再代入の壁なし）。

<a id="the-shape"></a>

## 形

```
LoopStart                                  ; markers carry no ownership (verify_ownership no-op)
  <recompute per-iteration locals>         ; e.g. `let c = string.get(s, pos) ?? ""` — a per-iter heap temp
  <if base-case-A> { result := <A>; brk }  ; base arm ALLOCs the heap result into `result` local, breaks
  <else if recurse> { pos := pos'; in_q := in_q'; <continue, no break> }
  <drop per-iteration heap temps>          ; `c` freed before the back-edge (per-iteration balance)
LoopEnd
ret = result                               ; the moved-out function result (rc=1)
```

図のコメント訳：LoopStart の marker は ownership を持たない（verify_ownership は no-op）。各反復の local を再計算し、例えば c は反復ごとの heap 一時値。基底 arm は heap 結果を result local に ALLOC して break。再帰 arm はスカラーを更新して break せず続く。反復ごとの heap 一時値 c は back-edge 前に解放して各反復を均衡させる。loop 後の ret = result は move-out する関数結果（rc=1）。

変換：本体が heap-result `if`/`match` 連鎖で、全自己呼出しが (a) 末尾位置、(b) スカラー引数だけを変更（heap 引数はビット同一）の自己再帰関数を上の loop にする。基底 arm（`none`/`some(p)`/`ok`/`err`/literal/concat）は `result` local を設定して break。再帰 arm は変更スカラーを `SetLocal` して back-edge に進む。

<a id="the-cert-the-soundness-critical-part--new-vs-scalar-while"></a>

## 証明書（健全性に重要 — scalar-while に対して新しい部分）

`try_lower_scalar_while` は「各反復 frame から正味の heap handle が逃げない」ことを要求する。TCO はこれに違反する：基底 arm の `Alloc`（関数結果）は `LoopEnd` 後に move-out されるため、必ず逃げる。従って新しい cert の形になる：

- 各反復：再帰経路は結果を確保せず（スカラーを `SetLocal` するだけ）、back-edge 前に一時 heap（`c`）を解放 → 均衡した `i…d`。
- 基底経路は結果を確保（cert `i`）して loop を抜け、結果を `LoopEnd` 後に1回だけ `Consume`（cert `m`）する（move-out）→ `im`、checker が既に受理する heap-result-`if` arm の均衡と同じ。
- heap 引数 `s` は借用（callee で acquire/drop なし）— 直線経路と同じ。

証明済み checker（`corpus-wall` ownership fold）に対して検証する主張：object 集合は {反復ごとの一時値（各々 loop frame 内で `id`）、結果（`im`、選ばれた基底ケースで確保、loop 後に move-out）}。checker が REJECT したら（例：loop から逃げる heap 結果に対して break-once / model-one-iteration を理解できない）、設計が誤り → revert、出荷しない。loop-result cert を拡張するまでは自己再帰 guard を残す（該当 yaml は壁のまま）のが fallback。

<a id="blocker-found-2026-06-19-the-loop-primitives-are-top-test-only"></a>

## 2026-06-19 発見の blocker：loop primitive は先頭判定専用

MIR loop marker（`LoopStart` / 先頭の `LoopBreakUnless{cond}` / `LoopEnd` back-edge / `SetLocal`）は古典的 `while cond { body }` — 先頭で判定する1つの出口、値なし — をモデル化する。一般 TCO は結果付きの途中 break が必要：自己再帰関数には ≥2 の基底ケース（`none` 対 `some(p)`）が再帰 arm の間にあり、それぞれ異なる heap 結果を作る。関数ごとに計算を書き換えない限り（一般変換ではない）、単一の先頭判定 while + loop 後の結果導出には表せない。

従って TCO には新しい loop primitive、途中で**関数結果を設定して抜ける break**（例：`Op::LoopBreakWith { val }` を `(result.set;
br $outer)` に render）が必要。さらに loop から逃げる結果 `Alloc`（`i`）が `LoopEnd` 後に正確に1回 `Consume`（`m`）される cert 推論が必要。これは MIR + render + **ownership-cert 拡張**（heap 値が loop 境界を越える）であり、既存の先頭判定 marker の再利用より厳密に大きく、健全性に重大。設計優先 + 敵対的に実装し、出荷前に逃げる結果 object の `im` trace が証明済み checker に ACCEPT されると検証する。loop から逃げる heap 結果を checker がモデル化できないなら本物の cert 前線（Camp 4 のようなもの）→ Mob-gate、拒否されたものを出荷しない。

<a id="scope--gates"></a>

## 範囲 / gate

- 自己再帰のみ（直接 `name == self.fn_name`）。相互再帰は別のより難しい問題で、壁のまま残す。
- スカラーを変える自己呼出しのみ（heap 引数はビット同一）。heap 引数を変える自己呼出し（list/string 再構築）は heap loop-carried 状態が必要 → 保留（`in_frame` heap 再代入の壁が既に拒否）。
- 両側より前に脱糖：IR 書換えなら `count_ir_calls` も同じ変換を見る必要がある（自己呼出しが消える — 件数同等を保つ）。
- 各段階を検証：`render_program` プローブのバイト一致（浅い入力と深い入力の両方で、trap なし = TCO が実際に適用されたことを確認）+ `corpus-wall` ACCEPT（3性質 + mir≤ir）+ `cargo test -p almide-mir` + `output-parity` baseline 無退行。

<a id="expected-payoff"></a>

## 期待する効果

約6つの直接自己再帰 yaml parser（およびコーパスの自己再帰 heap-returning 関数）を解放する。(2) 非自己再帰 heap-arm 形（after_colon の heap-Value 付き tuple など）、(3) Camp-4 heap-payload variant match（4つの `looks_numeric`/`is_compound`/… + float.parse self-host）と組み合わせ、yaml の v1 の壁をゼロにする道筋となる。TCO が要石。

<a id="cleanest-tco-entry-point-found-at-the-function-level-2026-06-19-scan_quote--find_colon_at"></a>

## 最も明快な TCO 入口、2026-06-19 関数レベルで発見：scan_quote / find_colon_at

この2つの yaml 関数が最も扱いやすい TCO 対象（22の壁のうち2を直接解放。同形の oct_rec/bin_rec/flow_rec もおそらく対象）：
```almide
fn scan_quote(s: String, pos: Int, in_q: Bool) -> Option[Int] =
  if pos >= string.len(s) then none
  else { let c = string.get(s, pos) ?? ""
    if in_q then scan_quote(s, pos + 1, not (c == "'" or c == "\""))   // TAIL self-call
    else if c == "#" and ... then some(pos)                            // base: scalar-payload Option
    else scan_quote(s, pos + 1, c == "'" or c == "\"") }               // TAIL self-call
```
コードコメント訳：最初と最後は末尾自己呼出し。中央はスカラーペイロード Option の基底ケース。

形 = 教科書的 TCO：**末尾自己再帰、スカラー loop-carried 引数（pos、in_q）、スカラーペイロード `Option[Int]` 結果、基底ケース `none`/`some(scalar)`**。heap 引数なし、heap accumulator なし。

構築（既存 loop 機構を拡張 — control.rs 約1349の scalar-while + 約2027の for-range は `LoopStart`/`LoopBreakUnless`/`LoopEnd`/`SetLocal` を使う）：自己再帰 guard（control.rs:1641 `if name == self.fn_name { return None }`）を、この形の TCO 変換で置換。if/match arm が (a) 同じ関数 + スカラー更新引数の直接自己呼出し、または (b) 基底式である本体を検出。`LoopStart` … 各反復で分岐条件を計算し、基底 arm では結果 local を設定（`none`/`some(pos)` Option を `Op::Alloc` — 唯一の heap 確保、`i`）して break。自己呼出し arm では更新したスカラー引数を `SetLocal` して loop。`LoopEnd` 後に結果 local を返す（`m`）。cert は1つの `i`（選ばれた break の結果 Option）+ 1つの `m`（return）= 均衡を見る。スカラー SetLocal は ownership を持たない。検証：`scan_quote`/`find_colon` 入力で v0 とバイト一致 + corpus-wall + tests。焦点を絞った適切な範囲の段階（新しい再帰→loop 変換）であり、セッション終了前の一行修正ではない。しかし次に最も効果が高い一手（TCO は22の壁の約6つを解放）。

<a id="correction-implementation-level-2026-06-19-tco-needs-a-new-mir-primitive-not-a-loop-reuse"></a>

### 訂正（実装レベル、2026-06-19）：TCO は loop 再利用でなく新 MIR primitive を要する

op レベルで調査：scan_quote の基底ケース（`none`、`some(pos)`）は loop 先頭でなく本体途中（`if in_q … else if # … else …` 連鎖内）にある。既存 primitive は先頭判定 `LoopBreakUnless { cond }`（control.rs:1377/2032）で、反復の先頭でしか抜けられない。そのため再帰→loop 変換は再利用できず、**本体途中の結果付き break** primitive（`Op::LoopBreak { result: ValueId }` など）が必要。任意の本体位置で計算した heap 結果を持って loop を抜ける。新 MIR op + render（結果 local 設定後の `(br $loopexit)`、結果を受け渡す wasm `block`/`loop` 内）+ cert 処理（break 結果の `i` と関数の `m` が均衡し、cert は loop 本体の反復ごとの op を均衡として、break 結果を move-out する戻り値として扱う）。従って本書題名が挙げる「途中 break-with-result MIR primitive」は実在の必要項目。TCO は既存先頭判定 loop 上の変換ではなく、焦点を絞った MIR+render+cert の段階である。（だから control.rs:1641 の自己再帰 guard は残る。新 primitive なしには健全な lowering がなく、深い入力で trap する再帰しかない。）

<a id="refinement-2026-06-19-the-gate-verifiable-cert-clean-structure-no-new-primitive-no-cert-change"></a>

### 改良 2026-06-19：gate で検証可能な cert-clean 構造（新 primitive なし、cert 変更なし）

再分析で、新しい `Op::LoopBreak` primitive なしに TCO を実装できると分かった（従って cert 変更もなし。corpus-wall は cert を使うので、その変更は検証不能になってしまう）。cert-clean な形：**スカラーのみの先頭判定 loop + loop 後の heap-result-if**。
- loop 本体はスカラー loop-carried 引数（`pos`、`in_q`）を `SetLocal` で更新するだけ。本体内 heap はなく、各反復は自明に均衡（既存 cert がそのまま通る）。
- heap 結果（`none`/`some(pos)`）は loop 後に、最終スカラー状態に対する通常の heap-result-if として計算。通常どおり均衡（`i`+`m`）し、loop 本体から Alloc は逃げない。
前提：基底結果を最終スカラー状態から再構成できること。scan_quote/find_colon_at は満たす：`if pos >= len then none else some(pos)`。（最終スカラー以外が基底結果に必要な関数は result-in-loop 形を要し、Alloc が loop 本体から逃げて cert が検出する。その subset には引き続き新 primitive が必要。まず再構成可能 subset から始める。）
変換（複雑な部分）：葉が自己呼出し `f(invariant, pos', in_q')` または基底式の入れ子 if/else 本体から、(a) continue 条件 = ¬(いずれかの基底に到達)、(b) loop 本体 = 自己呼出し経路のスカラー更新、(c) loop 後の結果 = 最終スカラーで再評価した基底式連鎖、を導出。`LoopStart` / `LoopBreakUnless{continue}` / 本体 `SetLocal` 群 / `LoopEnd` を出力し、続いて loop 後の heap-result-if。検証：scan_quote 入力で v0 とバイト一致 + corpus-wall（既存 cert）+ tests。gate で検証可能で有界（誤導出はバイト一致に失敗 → revert）だが、本当に複雑な変換で、焦点を絞った段階。まず scan_quote/find_colon_at（2つの壁、再構成可能 subset）、oct_rec/bin_rec/flow_rec/flow_step も同形と推測。

<a id="landed-2026-06-19-commit-77c91648-scalar-arg-tco--scan_quotefind_colon_at-yaml-2219"></a>

### 着地 2026-06-19（commit 77c91648）：scalar-arg TCO — scan_quote/find_colon_at、yaml 22→19

`try_tco_rewrite`（lower/mod.rs）を実装：result_kind + scalar-loop + post-loop-dispatch 変換、heap 結果の末尾自己再帰でスカラー loop-carried 引数の場合に制限。検証：scan_quote が v0 とバイト一致（3/99/99/0）、corpus-wall ACCEPT、output-parity 61/61、466 tests。yaml 22→19（scan_quote + find_colon_at を lower できる。先の float.parse 認識で22→21、TCO で21→19）。

残る自己再帰の壁はすべて、より難しい拡張を要する（heap loop-carried 引数 = heap back-edge merge、`while_body_heap_accumulator_walls` クラス）：
- `oct_rec`/`bin_rec`：`match list.first(cs)` の葉（tco_collect/rewrite に MATCH-leaf 対応が必要）。さらに `cs: List[String]` は heap loop-carried 引数（毎回 drop(cs,1)）でスカラー gate 外。
- `flow_rec`/`flow_step`：相互再帰（flow_rec↔flow_step、直接自己再帰でない）かつ `acc:
  List[String]` heap accumulator（acc+[x]）。
従って次の TCO 段階は HEAP-LOOP-CARRIED。heap accumulator を運ぶスカラーループは、back-edge を跨いで accumulator を合流する必要がある（毎反復 heap local を再束縛、解放は1回）。scalar-while が現在明示的に拒否する難例である。さらに match-leaf + 相互再帰対応。各々追加の焦点を絞った拡張であり、scalar-arg subset（本 commit）が明快な基盤。
