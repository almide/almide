説明：byte-identity 下で fan.timeout を合法化する — fuel、決定的配分、勝者順序、effect 隔離
<a id="deterministic-bounds--legalising-a-bound-on-computation-under-byte-identity"></a>

# 決定的な上限 — byte-identity の下で計算の上限を合法化する

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](deterministic-bounds.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](deterministic-bounds.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](deterministic-bounds.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/deterministic-bounds.md)

> 原文注記：歴史的設計論であり、後続回答と構文はリンク先 logical-time 設計による。コード案は保存し、現行構文検証や実装済み証拠とはしない。先行要素が未統合、library 並行性では保証不能という表現は原位置づけで、独立確立した不可能性や先取性ではない。Wasmtime/EVM/Verse/LVars/AARA の説明と epoch 約10%は新規確認・benchmarkしていない。有限観測／2-safety と fuel 停止の議論は所定の断片・隔離前提に限定し、任意 host call や未計量作業には拡張しない。stdout/stderr/exit の観測一致は artifact byte 一致とは別。共有 fuel・共有 MIR 単独、fuel 合計単独、oracle と捉え直すだけでは不足と明記する。実装・gate・測定は実行していない。


> **設計上の回答**：この文書が提示する4つの未解決問題（budget 配分、勝者選択、cancel、effect 隔離）は [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md) で回答している。本書は問題設定と正しさの議論として残し、そちらを文法設計とする。

**改訂注記。** 初稿は fuel の導入だけで `fan` の cancel が決定的になると主張した。**それは誤り**であり、教訓として見えるまま残す。**共有** fuel counter は依然スケジューリングの関数で、先に budget を使う sibling によって誰が尽きるかが決まる。fuel は必要な材料であって十分ではない。他4つの過大主張も各所で訂正し、それぞれを明示する。

問いは「timeout を持てるか」でなく、契約が届かない場所で容認するのでなく、**byte-identity 契約内で計算の上限が合法になれるか**。

<a id="the-reframe-which-survives-review"></a>

## レビュー後も残る捉え直し

`fan.timeout` は wall-clock 上限が結果を host の速度の関数にするため 0.29.0 で削除された。捉え直しは、**失敗は上限を設けることではなく、使う量にある**。wall-clock をプログラム自体が定める量に置換すれば性質を維持し得る。

憶測ではない。Wasmtime は2つの割込み機構をまさにこの tradeoff と説明する。**fuel** は同一プログラム・初期状態・fuel に対して、他の非決定性がなければ同じ位置で割り込む。**epoch** は wall-clock ベースで、割込み位置は非決定的。業界は既に両側に名前を付けている。

**EVM gas** は同じ議論の大規模版。全 node が同じ状態遷移に同じ gas schedule を適用し、同じ out-of-gas 判定に到達する必要があり、Yellow Paper はそれを exceptional halt と定義する。上限の決定性は付加価値ではなく正しさの根拠。

<a id="what-the-relational-property-actually-is"></a>

## 関係的性質は実際には何か

*（訂正：初稿は無条件で2-safetyと呼んだ。）*

対象性質は2実装を関係付ける：

> ∀ p, i.  `observe(native(p, i)) = observe(wasm(p, i))`

これを **2-safety**（最大2本の有限 trace で違反を示せる、Clarkson & Schneider）と呼べるのは、**全実行が有限の観測に到達する断片だけ**。発散と co-termination を含む完全観測等価は無条件の2-safetyではない。反例に、どちらの実行も永遠に停止しないことの提示が必要となる場合があり、有限 trace の対では示せない。

これは避けるための形式的細部でなく、bounded 断片が重要な理由。**fuel 上限下では全実行が有限時間で `Ok` または `Exhausted` に至る**ため、その断片の性質は有限に反証でき、2-safety の枠組みが成立する。正直な表現は：

> **bounded 断片**の観測等価は関係的 safety property で、本書の有限観測意味論では2-safety。bounded 断片外での主張はより弱く、2-safety と述べるべきでない。

この repo の用語確認：上記は**観測出力**（契約台帳で定義する stdout、stderr、exit code）の等価性。「byte-identity」は org-verify 掃引では artifact レベルの byte 一致にも使うが、別の主張であり、本書は前者を指す。

<a id="path-a--fuel-and-the-part-that-is-genuinely-hard"></a>

## Path A — fuel と本当に難しい部分

**機構。** プログラム自身の実行で減る抽象 step の budget はプログラムの関数。同じプログラム・入力・budget ⇒ どの機械でも同じ結果。

**Almide の実際の利点を精密に述べる。** *（訂正：初稿は共有 MIR が2経路を一致させるとしたが、単独ではしない。）* 2 backend にまたがる fuel は step の定義について両者の合意が必要。共有 MIR はそれを**表現可能**にするが、**真**にはしない。必要な条件はもっと強い：

> 両 renderer が**共有 MIR の charge event を保持**すること。同じ event、同じ順序、各1回。

MIR op を2つ融合したり並べ替えたりする renderer 局所 peephole は、MIR が共有でもこれを壊す。従って gate すべき不変条件は「MIR が1つ」でなく「lowering が charge trace を保存する」：

    chargeTrace(native(M)) = chargeTrace(wasm(M)) = chargeTrace(M)

**総消費 fuel の比較だけでは足りない。** 合計が同じでも、charge 順序が違えば一方だけが途中で上限に達し、分岐する。fixture は `result`、`consumed_fuel`、**charge-event trace** の3つを比較する必要がある。

**fixture は反証し、証明はしない。** 最も安価な反証器であり、機能より前に作るべき。証明に沿う証拠は、既存 ownership certificate と同種の、各 charge event が厳密に1回保存されることを確かめる validator と、生成 MIR 上の property testing。

<a id="the-cost-model-becomes-part-of-the-language-semantics"></a>

### cost model が言語意味論の一部になる

ユーザーが `Exhausted` を観測できた時点で cost model は実装詳細でなくなる。少なくとも必要なのは：

1. **versioning**。cost model 変更は意味変更であり契約台帳に属する。
2. **最適化レベル不変性**。`-O` がどのプログラムが尽きるかを変えてはならない。
3. **renderer 最適化不変性**。target 固有 pass は charge point を移動してはならない。
4. **算術の定義**。counter overflow と入れ子 `bounded` の意味。
5. **可変コスト操作**。list copy や string concat を定数と偽らず、サイズの関数で費用を定める。
6. **host call、allocation、RC traffic の課金方針**。消費量を計上するか無料かを明示。

<a id="path-b--quasi-determinism-a-precedent-not-a-theorem-to-lean-on"></a>

## Path B — 準決定性：先例であり、依拠する定理ではない

*（訂正：初稿は論文に存在しない「否定的定理」を引用した。）*

**LVars / Freeze-After-Writing**（Kuper、Turon、Krishnaswami、Newton — POPL 2014）が最も近い設計先例。非単調な観測が常に決定性を破壊する一般定理ではない。**quiescence のような非単調な実行途中の性質を公開すると、完全決定性が準決定性へ弱まり得る**、つまり全停止実行の値が一致するか、ある実行が error を出す。そして情報の外部流出を許さなければこの弱化を避けられる、という内容。

上限の2結果に適した枠組みであり、cost model が近似的にしか移植可能でなければ正直な退避先を名付ける。**決定的** cost model なら設計は準決定性より強くなり、`Exhausted` は通常値になる。

**ただし pure 断片のみ。** *（訂正：初稿は曖昧に述べた。）* `Exhausted` が通常値となるには bounded 計算が部分状態を漏らしてはならない：

- 外部 I/O なし。
- 中間結果が外へ出る mutable state なし（または transaction で rollback）。
- captured effectful closure なし。

従って signature は関数型だけでなく effect 制約を持つ：

    bounded(f: Pure[() -> T], n: Fuel) -> Result[T, Exhausted]

<a id="path-c--oracle-relative-contracts-and-what-they-do-not-buy"></a>

## Path C — oracle 相対契約と、それでは得られないもの

*（2点訂正。）*

fuel は subprocess や socket を助けない。標準的な関係的扱いで、例外でなく**外部入力**として整理できる。ただし共有 `ω` 1つを量化する素朴形は、両実装が同じ意味的点で同じ順序で oracle に問うと暗黙に仮定する。一般形は2つの oracle stream を関係付ける：

> ∀ p, i, ω_N, ω_W.  `R_Ω(ω_N, ω_W)` ⇒ `observe(native(p, i, ω_N)) = observe(wasm(p, i, ω_W))`

`R_Ω` は対応する HTTP response、filesystem result、process output の関係。

**制限を明記しなければならない**。wall-clock timeout を oracle event と捉え直しても byte-identity 互換には**ならない**。両実装が異なる意味的点で event を受け取る可能性は残る。Path C がするのは、C-189 の platform reporting 免除を含む増大する例外リストを置換し、環境由来の非決定性を1枠組みにまとめることだけ。

<a id="path-d--static-bounds-scoped-correctly"></a>

## Path D — 静的上限、正しい範囲で

*（訂正：「cost が静的に分かれば runtime bound は不要」は強すぎた。）*

**AARA**（RAML — Hoffmann、Aehlig、Hofmann と後続）は、first-order functional program の評価 step、heap、user-defined tick に対する多項式 resource bound を推論する。不完全で、多項式時間のプログラムでも解析できない場合がある。

正確な主張は：

> AARA が上界を証明し、その上界が policy budget 内なら、**その call に限り** runtime metering を省略できる。他は全て runtime counter を残す。

<a id="the-actual-question-can-fantimeout-be-legalised"></a>

## 実際の問い：`fan.timeout` を合法化できるか

できる。ただし fuel だけではなく、命名も重要。

    deterministic fan bound  =  fuel
                             +  deterministic budget allocation
                             +  logical winner selection
                             +  effect isolation

上の式は、決定的 fan 上限 = fuel + 決定的 budget 配分 + 論理的勝者選択 + effect 隔離を示す。

<a id="1-budget-allocation-must-not-be-a-race"></a>

### 1. budget 配分を競争にしてはならない

共有 counter 1つこそバグ。例えば：

    fan.bounded(1000) { A  B }

A を先に900 step進める schedule と、B を先に進める schedule では、同じ合計 budget でも scheduler によりどちらが尽きるかが変わる。**配分は source と MIR の関数**でなければならない。決定的分割（`A: 500`、`B: 500`）か、branch ごとの budget＋block 全体の別の logical-time cap。どの規則かは設計問題だが、規則が必要なことは選択ではない。

<a id="2-the-winner-is-chosen-by-logical-order-never-by-physical-completion"></a>

### 2. 勝者は物理完了順でなく、必ず論理順で選ぶ

「OS thread 上で先に終わった branch」はまさに native と wasm の差。選択規則は全域かつ構文的でなければならない：

- 決定的論理完了 step が早い方の勝ち。
- 同順位は source 順で決着。

従って logical step 250 で終わる branch は wall-clock に関係なく300より勝ち、同 step の2 branch は記述位置順になる。

<a id="3-cancellation-is-an-optimisation-not-an-observable"></a>

### 3. cancel は観測対象でなく最適化

全体を実装可能にする部分。決定的であるべきものは：

- どの branch の結果を採用するか。
- どの branch を `Exhausted` と見なすか。
- どの effect を commit するか。

native が負け branch を実際に**いつ**止めたかは、**観測不可能**なら wasm と一致不要。native は A を即停止し、wasm はさらに動かしてよい。どちらも漏らさなければ観測挙動は同じ。

成立には Path B の隔離が必要。pure branch、または effect を buffer し勝者分のみ commit、または effect を cancellation-safe 集合に制限する。

<a id="4-two-constructs-because-the-user-buys-two-different-guarantees"></a>

### 4. 異なる保証を求めるため、構文も2つ

*（分割を採用。）* 技術上 fuel を `fan.timeout(1000)` と綴れても、timeout を知るユーザーは `timeout(1000)` をミリ秒と読む。「決定的計算 budget」と「実時間が経てば諦める」は同じ製品ではない：

    fan.bounded(1000)      // deterministic: logical budget, byte-identical outcome
    fan.timeout(5.seconds) // oracle-dependent: environment-relative, Path C contract

上の注釈訳：bounded は決定的 logical budget、byte-identical な結果。timeout は oracle 依存、環境相対で Path C 契約。

従って `fan.timeout` は oracle 相対契約を持つ**正直に環境依存の**構文として復帰し、決定的上限には別名と、独自のはるかに強い約束を与える。

<a id="prior-art--every-part-is-solved-somewhere-and-the-parts-have-never-been-joined"></a>

## 先行例 — 各部分はどこかで解決済みだが、組み合わされていない

部品に新規性はない。どの部分を借りたかを明確にすることで主張が鮮明になり、借りた各部品は論文だけでなく動くシステムである。

<a id="verse--race-is-nearly-this-construct-already"></a>

### Verse — `race` は既にこの構文に近い

Epic の Verse は複数 async expression を開始し、最初の完了を採用する `race` を持つ：

```verse
Winner := race:
    A()
    B()
    Sleep(5.0)
```

意味論は stage 3 に必要なものに驚くほど近い。最初に完了した expression が勝ち、敗者を cancel、lifetime は `race` block に限定。そして重要なのは、**同じ simulation time に複数が完了すると source で最初に書いたものが勝つ**。上の logical completion＋source 順 tie-break が、既に言語として出荷されている。

Verse には failure context 内の変更を rollback する effect type（`transacts`、`decides`）もある。

**残る2 gap が Almide の追加2点そのもの。** 第一に、Verse は決定的 instruction budget でなく**simulation time**で競争するので、2 backend の計算 cost 一致には対処しない。第二に、負け branch の cancel は**その effect が絶対に観測されない**という保証とは違う。Verse は勝者選択、structured cancellation、transaction、effect type を持つが、敗者の effect が外へ出ないという単一契約に融合してはいない。

<a id="esterel--lustre--lingua-franca--logical-time-done-properly"></a>

### Esterel / Lustre / Lingua Franca — 論理時間を正しく扱う

同期言語群は何十年も「並行性を物理でなく論理時間で意味付ける」問題に取り組んだ。Lingua Franca は明示した logical time で reaction を実行し、同じ時刻なら宣言順に並べ、経過した物理時間から意図的に切り離す。これも早い logical step が勝ち、同順位は source 順。

gap：その logical time は**event と timer**の時間であり、**計算 cost**でない。その言語群は reaction の作業量を制限しない。

<a id="evm--fuel-and-rollback-fused"></a>

### EVM — fuel と rollback の融合

EVM は本書が要する2半分の最も強い既存の組合せ。各命令に gas cost、尽きれば決定的停止、out-of-gas transaction は**状態変更を revert**。fuel＋effect 隔離を1機構で大規模に提供。

gap：`fan` がない。EVM が解くのは**決定的有界逐次実行**で、2 branch を走らせ論理的に先に終わる方を採用する概念はない。

<a id="haskells-par-monad-and-lvars--determinism-under-a-free-scheduler"></a>

### Haskell の `Par` monad と LVars — 自由な scheduler 下の決定性

両者とも並列 code ができることを制限して決定的並列性を得る。`Par` は利用可能操作を制限し、LVars は単調更新だけを受け入れる。work-stealing scheduler は自由に動いても観測結果は不変。

gap：計算 budget、先着結果採用、上限を中心に組織されていない。

<a id="the-map"></a>

### 対応表

| 要素 | 解くもの |
|---|---|
| 決定的 fuel | EVM gas、Wasmtime fuel |
| 論理的勝者選択、source 順 tie-break | Verse `race`、Lingua Franca、Esterel |
| structured cancellation | Verse `race` |
| 敗者の effect が観測されない | EVM rollback、Verse transaction、purity |
| 自由な scheduler 下の決定性 | Haskell `Par`、LVars |

**各行に強い答えがあるが、どの行の答えも他の行を覆わない。** 従って提案を1行で述べると：

> **EVM の gas clock 上で動く Verse の `race`。**

4 library でなく1構文になれる理由も同じ。`fan` を compiler が理解するため、budget 配分、勝者選択、effect の可視性は user code と scheduler に分散せず、compiler が決定する。

これが正直な位置づけ。新しい発想ではなく、**よく解かれた3つの下位問題を、これまでなかった同時成立へ合流**させる。それに、どれにもない要件——2 backend が cost に合意すること——を加え、実装ごとの決定性に留まらず byte-identical な結果とする。

<a id="the-claim-pushed-to-its-real-strength"></a>

## 主張を本来の強さまで押し出す

興味深い主張は「fuel が timeout を直す」ではなく：

> **`fan` は library でなく compiler が知る構文だから、fuel 以外に決定的上限が要する3つ——budget 配分、勝者選択、effect 可視性——を compiler が担う。** 並行性が library の言語では、これらの判断が user code と scheduler にあるため、この保証をできない。

共有 MIR と first-class `fan` がもたらすものはこれであり、単一機能でなく言語形状についての主張。

<a id="staged-plan-each-stage-falsifiable"></a>

## 段階計画、各段階を反証可能に

1. **MIR op の cost model＋両 renderer の charge-trace 保持**。反証器は `spec/wasm_cross` fixture で `result`、`consumed_fuel`、**charge-event trace** を比較。1単位または1位置の不一致で計画を止める。それは charge 保持が偽であることを意味し、それ自体知る価値がある。
2. **`bounded(f: Pure[() -> T], n) -> Result[T, Exhausted]`** を逐次 pure 計算に提供し、cost model を version 付き意味対象として契約台帳に登録。
3. **決定的 `fan.bounded`**：budget 配分規則、論理的勝者選択、effect 隔離。#1000 の cancellation 除外を stage 1 の帰結でなく fan 意味論の再設計として見直す。
4. **oracle 相対契約**：`process` / `http` / `fs` に `R_Ω` を用い、#1040 effect-surface timeout と C-189 を統合し、環境相対構文 `fan.timeout` を再受理。
5. **MIR 上の AARA**：証明した bound が budget 内の箇所で metering を省略。

<a id="what-would-falsify-this"></a>

## 何が反証になるか

- **lowering が charge trace を保存しない**。最もあり得るのは renderer 局所 peephole。stage 1 が発見し、stage 2～3を無効にする。
- **範囲を限定しても overhead が許容できない**。Wasmtime は epoch 割込みをより安価と説明し、およそ10%と報告する。fuel はより高価で、独自 benchmark なしに包括的数値をここで引用すべきでない。overhead は workload と charge 粒度に依存し、だから budget は全体でなく明示した bounded 領域に適用する。
- **ユーザーが bounded 領域内に求める effect の隔離が非現実的**。なら `bounded` は pure-only のまま。それでも `fan.timeout` がかつてそうでなかった価値は残る。
- **AARA が stdlib の形状を扱えない**。一部ではありそうだが、runtime counter が fallback であり、部分的な静的上限にも実質がある。
