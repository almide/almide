<!-- description: Proof ledger for the logical-time async semantics: theorems, Lean core, model gate -->
# Logical-Time Async — the proof ledger

> [logical-time-async.md](./logical-time-async.md) の意味論が非同期機能として成立することの
> 証明台帳。証拠は 3 層:
>
> 1. **紙の定理**（本文書）— 形式モデル上の主張と証明
> 2. **Lean kernel-check** — 選択代数の核 7 定理、
>    [crates/almide-race-belt/](../../../crates/almide-race-belt/)（0 sorry、CI `lean-proofs`）
> 3. **全数モデル検査** — 74,898 構成 × 全スケジュール、
>    [research/spike/logical-time-race/](../../../research/spike/logical-time-race/)
>    （`run-gate.sh`、乖離 0）
>
> 証明作業が設計に強いた**訂正 3 件**も本文書が記録する — 証明が儀式でなかった証拠である。

## 何を証明し、何を証明しないか

- **証明する**: 意味論そのものの性質 — 選択の一意性、スケジュール非依存（合流）、
  枝刈り・遅延 fuel 検査の不可観測性、消費量の決定性、有界断片の有限観測性、
  そして「環境時間で勝者を選ぶ構文は決定層に存在できない」という境界の必然性。
- **証明しない**: 実装（両 renderer）が意味論に適合すること。それは charge-trace
  validator + `spec/wasm_cross` fixture の領分であり、
  [proven-vs-trusted](../../contracts/proven-vs-trusted.md) の流儀どおり **trusted 側**に
  置く。ここの定理群は「意味論が一貫している」ことを固定し、実装ゲートは「実装が
  その意味論を写している」ことを反証しにいく。役割は交わらない。
- **前提として明示する**（well-formedness、CM-1 実装時の受理条件）:
  - **W1**: CFG のすべてのサイクルと再帰呼び出しは charge site を ≥1 個通る。
  - **W2**: 隣接する charge site 間の仕事量は有限（basic block 入口 charge なら
    ブロック長で押さえられる）。

## 形式モデル

- **枝** = 決定的 charge トレース: コスト列 `c₁, c₂, …` と終端
  （`Complete` | `Trap` | 発散）。純粋断片（Rung 0）では (プログラム, 入力) の関数。
- **予算 n の下での枝の終わり** `end_j`（check-then-charge: 残量 < c で Exhaust）:
  `Complete(t)` | `Trap(t)` | `Exhaust(t)`、`t` = 累積消費。
- **事象** = `(time, idx)`（累積消費, 枝番号）。**merge 順** `≺` は (time, idx) の辞書式
  — 「1 tick に 1 fuel、同時はソース順」の lockstep をスケジューラなしで言い切った形。
- **決定的事象** = Complete / Trap 終端のうち merge 順最小のもの。Complete なら勝者、
  Trap ならプログラム trap、存在しなければ `Err(exhausted)`。
- **occurred stream** = 決定的事象に ≺ で先行する charge 事象たち（+ 決定枝自身の
  charge）。外側 region はこの列を merge 順に消費として観測する。

Rust モデル（spike）はこの定義の逐語的な実装であり、Lean は事象と `≺` の代数を
そのまま形式化している。

## 定理台帳

### T1 — 枝の決定性と全域性（紙）

**主張**: W1・W2 の下で、予算 n の枝実行は高々 n 回の charge と有限の仕事で
`Complete(t) | Trap(t) | Exhaust(t)`（t ≤ n）に必ず到達し、結果は (トレース, n) の関数。

**証明**: 純粋断片の small-step は決定的。W1 により無限実行は無限回 charge site を
通るが、n+1 回目の site 通過は不可能（残量が尽きる）。W2 により site 間は有限。∎

これが `fan.bounded` の全域性であり、deterministic-bounds.md が条件付きで述べた
「有界断片では 2-safety の枠組みが成立する」の条件を閉じる（T8）。

### T2 — lockstep ≡ least-spend（紙 + Lean `decisive_subset`）

**主張**: 決定的事象が Complete のとき、その枝は完了枝の中で (spend, index) 辞書式
最小である。

**証明**: 完了事象の time はその枝の spend に等しい。merge 順は (time, idx) 辞書式
なので、全候補の最小が Complete なら、それは Complete 部分集合でも最小（Lean:
`decisive_subset` を完了部分集合に適用）。∎

### T3 — 選択の一意性（Lean `decisive_unique`）

**主張**: 候補の枝番号が相異なるとき（枝は高々 1 回終端する）、決定的事象は一意。

`¬prec` 両向きから time と idx の一致が出る（omega）。「誰が勝ったか」に答えが
2 つある余地は代数的にない。∎（kernel-checked）

### T4 — cap の許容性（Lean `cap_admits_decisive` / `cap_admits_window`）

**主張**: 枝刈り cap（記録済み候補 d' に対し枝 k を `d'.time − (k > d'.idx ? 1 : 0)`
で切ってよい）は、**どの候補から計算しても**（逐次 scan の暫定 best でも、並列
スケジュールがたまたま先に記録した候補でも）、(a) 真の決定的事象の枝をその時刻まで
走らせ、(b) 決定的事象に ≺ で先行するすべての事象 — trap 可視窓を含む — を生かす。

∎（kernel-checked）。系: **静止まで走ったすべてのスケジュールは真の決定的事象を
記録する**（決定枝は cap では park できず、終端前に Exhaust もできない）。

### T5 — 合流（Lean `decide_stable` + モデル ADV）

**主張**: (i) 記録が真の候補集合の部分集合で、(ii) 真の決定的事象を含み、(iii) 記録の
merge 順最小を答える実装は、**必ず**真の決定的事象を答える。逐次 scan・並列実行・
cap ちょうどの枝刈り・cap を跨ぐ遅延 fuel 検査（overrun）はすべて (i)–(iii) に落ちる。

Lean が (i)–(iii) ⇒ 一致を証明し（`decide_stable`）、モデルが「実装が (i)–(iii) を
満たすこと」を 74,898 構成 × 全スケジュールの列挙で反証しにいって外さなかった。
勝者だけでなく **consumed fuel と occurred stream まで**一致する（T6 の検査を兼ねる）。

### T6 — 消費量の決定性と再構成可能性（紙 + モデル）

**主張**: occurred stream（したがって race が外側に課す消費量）はトレースの関数で
あり、cap 付きの実行が明かしたデータだけから正確に再構成できる。

**証明**: stream の定義は決定的事象（T3 で一意）への ≺ フィルタなのでトレースの
関数。再構成: cap 実行は time ≤ cap の全事象を明かし、T4(b) により cap は窓の
全事象を覆う。余分に明かした事象（overrun 分）は ≺ フィルタが落とす。∎
モデルは stream の列としての一致まで比較している。

### T7 — 入れ子の決定性（紙 + モデル）

**主張**: 外側 region は occurred stream を merge 順に消費し、途中で残量が尽きれば
その時点で外側 Exhausted（race は放棄）。この裁定は (トレース, 外側残量) の関数で
あり、region の入れ子深さに関する帰納法で全体が決定的。

**証明**: stream は T6 で決定的、prefix-sum との比較は算術。決定的事象が Trap なら
プログラムが落ち、消費は観測されない。∎

### T8 — 有界断片の有限観測性（紙）

T1 により有界断片のすべての実行は有限時間で観測に到達する。ゆえに native ⇄ wasm の
観測等価はこの断片上で有限トレース対により反証可能 — deterministic-bounds.md が
「断片の外では 2-safety と呼ぶな」と限定した主張の、断片の内側が確立する。

### T9 — 決定層の不可能性定理（紙）

**主張**: 「環境の壁時計で勝者を選ぶ」構文は決定層に存在できない。

**証明**: 決定層の定義は「観測が (プログラム, 入力) の関数」。壁時計順は host の
速度の関数であり (プログラム, 入力) の関数ではない。勝者が観測可能（枝が識別可能な
値を返す）である以上、壁時計選択は定義に矛盾。∎

系: I/O レース・タイムアウトは**必然的に** oracle 層（環境を宣言された入力 ω とし、
R_Ω で関係付ける契約クラス）に属する。これは設計の選好ではなく境界の定理である。
`fan.timeout` を Stage 4 まで tombstone に留める判断はこの系の帰結。

## 証明作業が設計に強いた訂正（3 件 + モデル自身の 1 件）

1. **no-winner 時の trap 規則が未定義だった。** 初稿の可視窓は「勝者確定 tick より
   前」とだけ述べ、勝者不在のケースを定義していなかった。決定的事象規則
   （Complete / Trap の merge 順最小が唯一の裁定者）への統一で消滅。
2. **消費量の式 Σ min(end_j, s\*) は境界で過大だった。** 勝者より後の index の枝の
   time = s\* の charge は merge 順で勝者の完了に後行し、発生しない。正しくは
   occurred stream（merge-prefix）の総和。
3. **入れ子は「race site での原子的 charge」ではなく streaming。** 外側は race の
   内部 charge を merge 順に観測し、途中で尽きれば race を放棄する。原子的 charge
   だと「race 内部で外側が尽きる」ケースの裁定点が定義できない。
4. モデル自身のバグ 1 件: 参照側 occurred stream が発散枝の cost-1 尾部を
   落としており、SEQ との consumed 乖離として即検出・修正（乖離が実際に検出面に
   出ることの実演でもある）。

logical-time-async.md には 1–3 を訂正マーク付きで反映済み。

## 非同期機能としての十全性（adequacy audit）

| 非同期パターン | v2 での表現 | 根拠 |
|---|---|---|
| 並行 fan-out / join（`Promise.all`） | `fan { }` / `fan.map`（実行は並列可、観測はリスト順） | C-004 / C-199 / C-200 |
| 全収集（`allSettled`） | `fan.settle` | 既存 |
| フォールバック連鎖（`any`） | `fan.any`（逐次フォールバック） | 既存 + fan-v2 正式化 |
| 計算の投機・ポートフォリオ（`race` の純粋形） | `fan.race(fuel:)` | T2–T5 |
| 計算量上限・全域化 | `fan.bounded(fuel:)` | T1 / T8 |
| I/O レース（最初に応答した方） | **決定層では不可能（T9）** → oracle 層 Stage 4 | T9 |
| タイムアウト | oracle 層 `fan.timeout(ms:)`（Stage 4）/ host boundary | T9 |
| streaming / backpressure | `Flow[T]`（flow-design.md）の領分 | 範囲外を明示 |
| fire-and-forget | 恒久却下（rush / spawn — fan-v2 の却下表） | stance |

誠実な要約: 非同期の中核ユース（並行 I/O の fan-out・全収集・フォールバック）は
決定層が今日すでに覆う。**選択的 I/O レースとタイムアウトだけが oracle 層待ち**で
あり、それが層の外にあることは T9 により選好ではなく必然である。

## 小スコープの但し書き

モデル検査は 枝 ≤ 3 / charge ≤ 3 / 予算 ≤ 7 の全数であり、その外は Lean 定理
（サイズ非依存）と紙の証明が覆う。Lean が覆うのは選択代数（事象と ≺ の理論）で
あり、「トレース → 候補集合」の接続（候補の idx 相異、記録 ⊆ 真、決定事象の記録）は
モデルが構成ごとに検査した。スコープを広げた再実行は `run-gate.sh` 一発で可能。

## 反証条件

- CM-1 実装が W1/W2 を満たせない配置を要求する（例: charge-free サイクルが必要に
  なる最適化）→ T1 が崩れ、全域性の主張を取り下げる。
- renderer の charge-trace 保存が破れる → 定理群は無傷のまま、**実装が意味論の外に
  出た**ことを validator / fixture が検出する（それがゲートの仕事）。
- スコープ外の反例 → `run-gate.sh` のスコープ定数を上げて再現・追加すれば、この
  台帳の該当定理に訂正が入る。訂正は上の節の流儀で可視のまま残す。
