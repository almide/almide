<!-- description: The async grammar: fuel as the logical clock, deterministic race, oracle tier -->
# Logical-Time Async — the async grammar design

> 憲章: [async-inception.md](./async-inception.md)。本文書はその意味論詳細である。
>
> [deterministic-bounds.md](./deterministic-bounds.md) が立てた 4 設問（予算配分・勝者選択・
> キャンセル・効果隔離）への回答。[concurrency-stance.md](./concurrency-stance.md)（#1000）の
> 結論を 1 点だけ改訂する。これは設計文書であり、実装はステージング節の順で行う。
>
> **証明台帳**: 本意味論の成立は [logical-time-proofs.md](./logical-time-proofs.md) が
> 3 層（紙の定理 / Lean kernel-check / 全数モデル検査）で固定する。証明作業による
> 訂正 3 件（下の各訂正マーク）もそこに記録がある。

## 設計テーゼ

**壁時計は観測を決めない。言語の時間基底は論理時間（fuel）である。環境の時間は、宣言された
oracle 入力としてのみ入る。**

一行で言えば:

> Verse の `race` を EVM の gas 時間の上で走らせ、敗者を観測窓の外に置き、それを
> native ⇄ wasm の観測等価契約の内側で成立させる。

部品はすべて先行例がある — fuel の決定的中断（Wasmtime fuel / EVM gas）、論理時間と宣言順
tiebreak（Verse / Lingua Franca / Esterel）、制限による決定性（Par monad / LVars）。ないのは
合成である: **fuel をメトリクスとする race を、敗者非観測と入れ子予算まで込みで、クロス
ターゲット観測等価契約の中で定義した言語は見当たらない**。`fan` がライブラリではなく
コンパイラ既知の構文だから、予算配分・勝者選択・効果可視性の 3 つをコンパイラが所有できる。
これがこの設計の全根拠であり、2026 年の言語仕様として立てる主張そのものである。

## 表面 — 追加は 2 つ、キーワードはゼロ

> **形の改訂**: 表面の形（thunk 引数の関数形）は [fan-v2.md](./fan-v2.md) の block form
> — `fan.bounded(fuel: n) { body }` / `fan.race(fuel: n) { arm; arm }` — に置き換えられた。
> 本文書の意味論（fuel、lockstep、trap 窓、入れ子、CM-1）はすべてそのまま適用される。

```almide
fan.bounded(fuel: n, thunk)   // Result[T, String] — n fuel 以内に完了すれば Ok
fan.race(fuel: n, thunks)     // Result[T, String] — 各枝に n fuel、最小消費の成功が勝つ
```

将来の oracle 層（Stage 4、Path C 成立後）:

```almide
fan.timeout(ms, thunk)        // 環境相対。R_Ω 契約クラス。それまで tombstone 維持
```

### fan 族は「選択メトリクスの代数」になる

| 構文 | 選択規則 | 決定性の根拠 |
|---|---|---|
| `fan { }` / `fan.map` | 全部（リスト順 join-all、先頭 Err） | C-004 / C-199 |
| `fan.any` | **index 最小**の成功 | リスト順走査 |
| `fan.race(fuel: n)` | **(spend, index) 辞書式最小**の成功 | fuel + ソース順 tiebreak |
| `fan.settle` | 選択しない（全収集） | リスト順 |

`any` は index を最小化し、`race` は fuel を最小化して index で同点を割る。族の軸は
「何を最小化するか」の一本だけになり、点追加ではなく面として閉じる。

**完備性規則**（matrix gate に載せる形で述べる）: fuel 次元は `fan.bounded` による合成で
全コンビネータに届く（`fan.map(xs, (x) => fan.bounded(fuel: n, () => f(x)))` など）。
組み込みで fuel を統合するのは `fan.race` **だけ**であり、それは選択規則がメトリクスを
**消費する**（枝同士の spend を比較する）からである。ユーザー合成では枝間比較は書けない。
`bounded × {map, any, settle}` の専用形を追加しないのは意図的な省略であり、この規則ごと
ゲート化する。

### なぜ `fuel:` ラベルを必須にするか

`fan.timeout(1000)` の 1000 をミリ秒と読まない人類は存在しない、というのが 0.29.0 の教訓
だった。`fan.race(1000, ts)` にも同じ危険が残る。ラベル必須にすると全呼び出しサイトに
`fuel` の語が現れ、単位の誤読が構文レベルで死ぬ。

**訂正（2026-08-01）**: 初稿は `fan.map(xs, limit: 16, f)` をラベル引数の前例としたが、
これは**誤り**。`limit:` は実装されておらず（checker は fan.map を 2 引数固定で検査）、
言語にラベル引数構文は存在しない — fan-concurrency-next.md の status 表（✅）が stale
だった。ラベルの供給源は [fan-v2.md](./fan-v2.md) の block 文法である: fan head は関数
呼び出しではなく構文なので、`fuel:` は fan 文法自身の要素として実装でき、汎用ラベル引数
機構は導入しない。

### なぜ `fan.race` の名を再利用するか

0.42.0 の撤去理由は「決定的モデルの下で race という名前に与えられる意味が存在しない」
だった。**メトリクスなしでは今も真である。** fuel がメトリクスを与えたことで、初めて
race に決定的な意味が存在する。名前を変えない理由は 2 つ:

1. **LLM の事前分布**。並行の最速選択を書こうとするモデルは必ず `fan.race` を書く。
   新名（`fan.least` 等）は事前分布と戦う — MSR の逆行。
2. **署名が旧形式を弾く**。旧 `fan.race(thunks)` は 1 引数、新形式は `fuel:` 必須の
   2 引数。E027 tombstone は「撤去済み」から「署名移行ヒント」（`fuel:` を付けよ、
   意味論は最小消費勝者）に書き換える。tombstone の移行先は生きた表面でなければ
   ならない規則にも適合する。

## 意味論

### 論理時計 — charge event と CM-1

- **charge site** は共有 MIR の basic block 入口。静的コストはブロック内の意味論的 op 数。
  可変コスト intrinsic（list 連結、string 連結、サイズ n の構築など）はサイト内の
  **動的 charge**（`1 + サイズ比例項`）を追加する。
- **charge は最適化前の MIR 誕生時に付与し、以後の全パスは注釈を保存する**。renderer 側
  peephole が op を融合しても charge は合算して残る。これにより:
  - 最適化レベル不変（`-O` が exhaust を変えない）が**検査項目ではなく構成的に**成立する
  - RC 増減・MakeUnique・move・ANF 管理コードは**無料**（最適化の産物であり、ソース意味論の
    コストではない）。EVM gas が JIT の速さと無関係に定義されるのと同型
- **コスト表は versioned な意味論的オブジェクト**（`CM-1`）として契約台帳に載る。定数変更は
  semantic change であり、どの枝が勝つか・何が exhaust するかを変えうる。だから versioning は
  飾りではない。v1 定数のドラフト: 全 op weight 1、call は 1 + callee 本体、可変コスト
  intrinsic は `1 + ⌈size/16⌉`。**定数の確定は Stage 2 の実装 + fixture と同一 PR で行う**。
  ここで固定するのは形（サイズ線形、定数は台帳管理）だけである。
- **算術**: 予算は `Int`、内部カウンタは飽和演算。charge c に対し `remaining < c` で
  exhaustion。`n <= 0` は最初の charge で exhaust（検証分岐なしの全域意味論）。

### 中断点の統一原理

**charge site は言語の唯一の中断点である。** site 間の実行はアトミック。fuel 系構文は
site でカウンタを読み、将来の oracle 系構文（`fan.timeout`）は同じ site で環境を読む。
決定層と oracle 層の違いは「中断点で読むものが、プログラムの関数（fuel）か、環境入力
（時計）か」だけになる。中断点の位置は両層で共有され、そこは両ターゲットで同一である。

### `fan.bounded(fuel: n, thunk)`

- thunk は **pure**（効果制約。effect fn 呼び出し・oracle 効果を含めない）。構文自体は
  既存の fan 規則どおり effect fn 内でのみ使える（規則を 1 本に保つ）。
- 意味: thunk の消費 fuel `s` が `s <= n` なら `Ok(値)`、超えるなら台帳定義メッセージの
  `Err`。thunk が Result を返す場合は既存の auto-wrap 契約（race/any/settle と同じ）に従う。
- **trap は伝播する**。bounded は投機ではない。div-by-zero は通常どおりプログラムを落とす
  （C-200 の哲学と一貫）。
- exhaustion と thunk 自身の Err の区別が message 文字列に依存するのは、単一 String
  エラーチャネルという言語全体の性質であり、ここで点解決しない。`fan.any` の
  defined-Err 前例に従い、メッセージを台帳定数にして検査可能性だけは確保する。

### `fan.race(fuel: n, thunks)` — lockstep 定義

**定義（lockstep）**: 全枝が論理 tick ごとに 1 fuel ずつ進む。tick `s` で最初に完了
（成功値を返す）した枝が勝つ。同一 tick 内の事象はソース順（リスト index 順）に解決する。
race は最大 `n` tick 走る。

**特徴づけ（実装が使う形）**: 各枝 j の終端を
`end_j = min(spend_j, trap_j, n)`（完了 / trap / 枝予算切れの最初のもの）とすると、

- **勝者** = 完了した枝のうち `(spend_j, j)` の辞書式最小。存在しなければ台帳定義の `Err`
- 枝の値が `Err` の枝・exhaust した枝は候補にならない（`any` が失敗を飛ばすのと対称。
  race は「最小コストの**成功**」である）

lockstep での完了 tick はその枝の spend に等しいので、両定義は一致する。lockstep は
ユーザーに語る絵（Verse と同じ「同時ならソース順」）、least-spend はスケジューラを
消した計算式であり、**この一致こそが「物理時間なしの race」の内容**である。

**trap の可視窓 — 決定的事象規則**（*訂正 2026-08-01、証明作業による*: 初稿の窓規則は
勝者が存在するケースしか定義しておらず、勝者不在 + trap のケースが未定義だった。
以下の単一規則に統一する）:

> Complete / Trap の終端事象を merge 順（(累積 fuel, 枝 index) の辞書式）に並べ、
> **最初の決定的事象が唯一の裁定者**である。Complete ならその枝が勝者、Trap なら
> プログラムがその trap で落ちる、決定的事象が存在しなければ `Err(exhausted)`。

初稿の窓規則（`t_j < s*`、または `t_j = s*` かつ `j <` 勝者 index）はこの規則の
勝者存在ケースの系として従う。窓の外の trap は起こらなかったことになる（枝は
決定的事象の tick で消滅している）。投機がバグを黙って飲み込むことはなく（窓内なら必ず落ちる）、かつ勝者
確定後の敗者は存在ごと消える。C-200（fan{} の sibling trap 伝播）とは構文が違うのでは
なく、**join-all は「全部必要」、race は「どれか 1 つで足りる」という買っている保証が
違う**。その差がそのまま trap 規則の差になる。

**キャンセルは最適化である**。観測対象は「どの枝の値が採用されたか」「trap 窓」
「（入れ子時の）消費 fuel」だけ。native が敗者スレッドをいつ止めるかは自由。健全な
枝刈り: 現在の最良 `(s*, i*)` に対し、枝 `j < i*` は予算 `min(n, s*)`、枝 `j > i*` は
予算 `min(n, s* - 1)` まで走れば十分（同点はソース順で先勝ちのため）。この cap は trap
可視窓もちょうど覆う — cap 内で踏む trap は可視、cap 外は不可視。刈っても意味論が
変わらないことが構成から出る。

### 予算の入れ子 — EVM 式 min-cap

入れ子の `fan.bounded` / `fan.race` の実効予算は `min(自身の n, 外側の remaining)`。
内側の charge は全外側予算からも同時に減る（同一実行だから）。内側が外側由来の cap で
exhaust すると、外側は remaining ~0 で継続し、次の charge で外側が exhaust する。
region タグ付きの伝播機構は不要 — すべて共有 charge trace 上の算術になり、両ターゲットの
一致は trace の一致から従う。EVM のサブコール gas 上限（EIP-150）と同じ構図。

**race が外側に課す消費量**（*訂正 2026-08-01、証明作業による*: 初稿の
`Σ_j min(end_j, s*)` は境界で過大だった — 勝者より後の index の枝の time = s\* の
charge は merge 順で勝者の完了に後行し、発生しない。また「race site での原子的
charge」では race 内部で外側が尽きるケースの裁定点が定義できない）:

> race の **occurred stream** — 決定的事象に merge 順で先行する charge 事象列
> （+ 勝者自身の charge）— を、外側 region は **merge 順にそのまま streaming で**
> 消費として観測する。途中で外側残量が尽きればその点で外側 Exhausted（race は放棄）。
> race が完走した場合の総消費は stream の総和である。

これは枝ごとの trace の関数であり、刈った実装でも cap 実行が明かしたデータだけから
正確に復元できる（cap は可視窓の全事象を覆う — Lean `cap_admits_window`）。**実装が
実際に費やした仕事ではなく、意味論的消費量をカウンタから引く** — RC を無料にしたのと
同じ「意味論のコスト」原理の適用である。

### 効果隔離の梯子

- **Rung 0（v1）**: race / bounded の枝は pure。既存の purity 機構でそのまま検査できる。
  典型ユース: アルゴリズムポートフォリオ（厳密解 vs ヒューリスティクス、二つの文法での
  パース、SAT 流ポートフォリオ）。native では枝刈り付き並列で壁時計も速くなる。
- **Rung 1（後続）**: stdout/stderr のみの枝を許し、**勝者の出力だけを決着後に flッシュ**
  （敗者は破棄）。#1026 で保留になった arm 出力バッファリングと同じ機構で、C-004 の
  EXCEPTION 節退役と部品を共有する。
- **恒久的に不許可**: oracle 効果（fs / http / process / random / env）を race の枝に
  書くことはコンパイルエラー。**I/O の race は環境が勝者を選ぶ**ので、決定層には原理的に
  入らない。診断はこう言う: 「racing I/O is environment-dependent — restructure, or use
  the oracle-tier fan.timeout (Stage 4)」。

Almide の効果規律（var キャプチャ E008、mut 引数 E007、単一エラーチャネル）のおかげで、
隔離すべき効果は最初から {pure | 出力 | oracle} の 3 区分に落ちている。Verse が
transactional effect で解こうとしたものの大部分が、ここでは**型で最初から存在しない**。

## deterministic-bounds の 4 設問への回答

| 設問 | 回答 | 決め手 |
|---|---|---|
| 1. 予算配分 | **per-branch で各枝に n**（共有カウンタの決定的分割ではなく） | 分割だと枝の追加が既存枝の予算を変える。LLM が 3 本目の枝を足した瞬間、1 本目が exhaust し始める — modification survival の直接の破壊。per-branch は枝の意味を局所化する |
| 2. 勝者選択 | **lockstep ≡ (spend, index) 辞書式最小** | スケジューラが定義から消える。Verse の UX（同時ならソース順）と EVM の決定性を同時に満たす唯一の形 |
| 3. キャンセル | **最適化。観測は採用値・trap 窓・消費量のみ** | 枝刈り cap（`s*` / `s* - 1`）が trap 窓と一致するので、刈っても意味論が不変 |
| 4. 効果隔離 | **v1 pure、Rung 1 で出力 transactional、oracle は恒久拒否** | 既存の効果規律が隔離の大部分を型で済ませている |

## 型付け・診断・表面変更

- `fan.race(fuel: n, thunks) -> Result[T, String]`、`fan.bounded(fuel: n, thunk) ->
  Result[T, String]`。thunk の auto-wrap は race/any/settle の既存契約に従う。
- 新診断: (a) race/bounded の枝に効果呼び出し → 「branches must be pure」+ 上記 oracle
  ヒント、(b) `fuel:` ラベル欠落 → 署名ヒント、(c) 空 thunk リスト → 旧 race 同様
  コンパイルエラー。
- **E027 の改訂**: 「removed」から「signature migration」へ。移行先が生きた表面になる。
  `fan.timeout` tombstone のヒントは Stage 4 まで現状維持（host boundary への誘導）。
- unknown-fan-fn の Available リストに `fan.race` / `fan.bounded` を追加（実装 PR で）。

## クロスターゲット lowering

- **計測は bounded region 内のみ**。region から到達可能な呼び出しグラフを fuel 変種として
  特殊化する（mono と同じ機構の追加次元）。グローバルな計測オーバーヘッドはゼロ。閉包
  テーブルの metered スロットの扱いは実装時の未決点として明示しておく（設計上の答えは
  「region に流入しうる閉包は metered 変種を持つ」、コストは race 対象コードのサイズに
  比例）。
- **wasm（逐次）**: リスト順 scan。best `(s*, i*)` を更新しながら各枝を cap 付きで実行。
  途中の trap は `(t_j, j)` として**記録して走査を続け**、全枝の end 確定後に可視窓を
  判定して再送出する（後続の枝がより小さい `s*` を出せば窓の外に出るため、即死させては
  ならない）。
- **native（並列）**: 枝ごとにスレッド、trap は checked lowering で捕捉・記録（物理的に
  勝者確定前に敗者が trap に到達しうるため、遅延させて選択後に窓判定）。cap は best の
  更新に応じて縮む。
- 両者は同じ観測（採用値、trap 窓、消費量）を計算する。これが fixture の比較対象になる。

## 契約とゲート

1. **CM-1** を versioned オブジェクトとして台帳に追加。コスト定数の変更 = semantic change。
2. 新契約（番号は実装 PR で採番）: bounded の exhaustion 等価、race の勝者等価、trap
   可視窓、入れ子の min-cap と race 消費量。各契約に `spec/wasm_cross` fixture、fixture は
   `result` + `consumed_fuel` + **charge-event trace** の三点を比較する（総量一致だけでは
   順序の乖離を見逃す — deterministic-bounds の指摘どおり）。
3. **charge-trace 保存 validator**: 全 charge event が lowering を通って正確に 1 回ずつ
   emit に到達することを検査する、ownership certificate と同種のオブジェクト。fixture は
   反証器、validator が証明形の証拠。
4. 生成 MIR 上の property test（ランダム MIR に対する trace 保存）。

## 既知の境界（隠さず書く）

- **スタック**: fuel は charge（呼び出しも charge する）を通じて再帰深度も抑えるが、巨大な
  予算では native / wasm のスタック上限差が fuel より先に来うる。これは言語全体が既に
  持つ境界であり、この設計は改善も悪化もさせない。契約の観測範囲外として明記する。
- **消費 fuel の非公開**: `spend` を値として返さない（返すと CM の定数改定が全プログラムの
  値を変える）。コスト模型が観測に影響する経路は「勝者が誰か」「exhaust するか」に限定され、
  fixture は emit 時 Σ-probe（既存の evidence class）で内部比較する。
- **単一エラーチャネル**: exhaustion の判別が message 定数に依存する。エラー型の充実は
  言語全体の問い（point-wise に解かない）。

## 系譜との対応 — 何を借り、何が新しいか

| 系譜 | 借りるもの | 借りないもの / 差分 |
|---|---|---|
| Verse `race` | 勝者選択の UX、「同時ならソース順」、構造化キャンセル | simulation time は wall-clock 由来で計算量の決定性はない。transactional effect は Rung 1 で限定的に。Epic のドキュメントの記述に依拠（tie 規則は実装時に再確認） |
| EVM gas / EIP-150 | 決定的 out-of-gas、入れ子の min-cap、コスト表 = 意味論 | EVM に race はない（逐次トランザクションのみ） |
| Wasmtime fuel | fuel vs epoch の区別そのもの（決定 / 壁時計の二層をベンダーが命名済み） | Wasmtime の fuel は wasm op 単位。CM-1 はソース意味論単位で、最適化不変を構成的に得る |
| Lingua Franca / Esterel | 論理時間、同一論理時刻の宣言順解決 | 論理時間の単位がイベントであり計算量ではない |
| Par monad / LVars | 制限による決定性、quasi-determinism という誠実な退却先の命名 | fuel・選択・キャンセルは扱わない |
| Kahn process networks | （将来）決定的チャネルの理論的地盤 | 本設計の範囲外、下の「地平」参照 |

**新しいのは合成である**: fuel メトリクスの race + 敗者の観測窓 + 入れ子予算を、二backend の
観測等価契約とその検証装置（trace validator、三点 fixture）ごと一体で定義すること。

## MSR — この文法が「LLM が最も正確に書ける」に効く形

- **決定点は一つ増えるだけ**: 「これは投機か？」。投機なら race、逐次上限なら bounded。
  async/await/Future/task handle は引き続き存在しない。新キーワードもゼロ。
- **per-branch 予算**は編集の生存率のためにある。枝の追加・削除・並べ替えが他の枝の予算を
  変えない。ソース順 tiebreak は diff で見える形で勝敗を固定する。
- **`fuel:` ラベル**が単位の誤読クラス（ms と読む）を構文で除去する。
- **走るたび同じ**: race を含むテストが flaky にならない。並行構文がベンチ・スナップショット・
  3-way oracle にそのまま載る。決定性は性能特性ではなく正しさの定義の一部、という stance の
  文言がそのまま race にまで届く。

## concurrency-stance.md の改訂点（1 点のみ）

stance の「決定的モデルの下では race という名前に与えられる意味が存在しない」は、
**メトリクスなしでは真のまま**。fuel がメトリクスを与えたとき、そしてそのときに限り、
リスト順逐次評価と厳密同一の観測を持つ race が定義できる（上の scan がその逐次評価で
ある）。stance の原理「リスト順の意味を与えられないものは、言語に入れない」は**保存**
される — 本設計はその原理の適用例であって、例外ではない。

## 将来の地平（本設計の範囲外、方向だけ記す）

論理時計を持った言語は、その上に**決定的な並行性**（並列性ではなく）を定義できる:
論理 sleep、tick 順の決定的 select、charge 点 round-robin による決定的インターリーブ、
Kahn network 型の決定的チャネル。structured concurrency を再訪する日が来るなら、その
土台はこの時計である。stance が「再訪可能、ただし contract が先」とした条件を満たす
経路が、ここで初めて具体化する。

## ステージング（deterministic-bounds の 5 段に対応）

1. **CM-1 + charge-trace 保存**: MIR への charge 付与、両 renderer の trace 保存、
   validator、三点比較 fixture。1 単位・1 位置の乖離でプログラム停止。
2. **`fan.bounded`**: 逐次 pure、CM-1 定数の確定と台帳登録を同一 PR で。
3. **`fan.race`**: lockstep 意味論、枝刈り、trap 窓、入れ子消費量。E027 改訂。
4. **oracle 層**: R_Ω 契約、`fan.timeout` 再導入（charge 点で環境を読む形）、C-189 の
   吸収。
5. **AARA over MIR**: 証明済み上限が予算内の呼び出しで計測を省略。

## 反証条件

- charge trace が lowering で保存できない（renderer peephole が最有力容疑）。Stage 1 が
  検出し、Stage 2 以降を無効化する。
- 計測オーバーヘッドが region 限定でも許容外。→ bounded region の粒度を粗くする
  （block 単位 charge の統合）か、Stage 5 の AARA 前倒し。
- fuel 特殊化のコードサイズが実用外。→ race 対象を自己完結な関数に制限する診断で退却。
- pure 限定の race に実ユースが薄い。→ Rung 1（出力 transactional）を前倒しし、それでも
  薄ければ race は「bounded の N 本比較」への糖衣として価値を再査定する。
