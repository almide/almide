# ADR-0001: Deterministic budgets are written in time units

- **Status**: Accepted
- **Date**: 2026-08-01
- **決定範囲**: `fan.bounded` / `fan.race` の計算量予算の表面表記、および決定的時計の単位定義
- **関連**: [async-inception.md](../roadmap/active/async-inception.md)（憲章）、
  [logical-time-async.md](../roadmap/active/logical-time-async.md)（意味論）、
  [fan-v2.md](../roadmap/active/fan-v2.md)（文法）

## 前提 — 「時間」は 1 つではない

本 ADR は「予算を時間の単位で書く」と決める。その前に、**同じ『秒』でも測っている
ものが違う**という区別を共有する必要がある。ここが混ざると以降の議論は読めない。

1 回の実行から、少なくとも 2 つの違う時間が取れる。

```
プログラム開始 ──────────────────────────────────────────── 終了
              [計算]   [ネット待ち]   [計算]   [ディスク待ち]  [計算]
               30ms      500ms        20ms      200ms        10ms

  壁時計時間 = 760ms   ← 部屋の時計で測った経過。待ち時間も込み
  CPU 時間   =  60ms   ← CPU が実際にこのコードを実行していた分だけ
```

手元で確かめられる。2 秒眠るだけのプロセスと、2 秒弱ひたすら計算するプロセスを
同じ `time` で測るとこうなる（実測）:

```
$ time python3 -c "import time; time.sleep(2)"
0.04s user   0.02s system   2% cpu   2.173 total     ← 壁時計 2.17s / CPU 0.04s

$ time python3 -c "x=0
for i in range(8000000): x+=i"
0.39s user   0.03s system  80% cpu   0.514 total     ← 壁時計 0.51s / CPU 0.39s
```

上は**壁時計が CPU 時間の 54 倍**。眠っている間、CPU 時間は進んでいない。
Cloudflare のタクシー比喩がよくできている — 運転手が給油と軽食に寄っている間も
メーターは回り続ける。それが壁時計で、**実際に走った分だけ**が CPU 時間である。

区別が要る理由は、壁時計が**あなたのコード以外の都合で変わる**からだ。API が遅い日、
混んでいるマシン、隣で走っている別プロセス。Cloudflare が課金を CPU 時間にした
理由もそこにある — 「CPU time is more predictable and **under your control**…
**purely a function of the logic and processing of inputs on outputs to your Worker**」。

Almide の**決定的時計は、この方向をもう一歩進めたもの**である。

| | 待ち時間を含む | マシンの速さに依存する | (プログラム, 入力) の関数 |
|---|---|---|---|
| 壁時計時間 | **含む** | する | ✗ |
| CPU 時間 | 含まない | **する**（速い CPU なら短い） | ✗ |
| **決定的時計**（本設計） | 含まない | **しない** | **✓** |

CPU 時間はまだマシン依存である。同じコードでも速い CPU で走らせれば短くなるので、
「どの機械でも同じ値」にはならない。決定的時計は最後の一歩として**実測をやめる** —
凍結した抽象機械のコスト表（CM-1）が「この op は何ナノ秒ぶん」を定義し、実行した op を
足し合わせる。測らないので、速い機械でも遅い機械でも native でも wasm でも**必ず同じ値**
になる。

したがって `compute.ms(100)` の読み方は「壁時計で 100 ミリ秒待つ」ではなく、
**「計算 100 ミリ秒ぶんの仕事をさせる」**である。速い機械なら壁時計 40ms で終わるかも
しれないし、遅ければ 300ms かかるかもしれない。それでも**どの機械でも同じところで
打ち切られる** — それが「決定的」の内容である。

「単位は秒だが壁時計ではない」という形自体は新しくない。POSIX の `ulimit -t 60`
（CPU 時間 60 秒でプロセスを殺す）が 50 年運用されている。本設計はそこに
**マシン非依存性**を足したものだ、と位置づけるのが最も正確である。

## 用語集

| 用語 | 意味 | 参照 |
|---|---|---|
| **壁時計**（wall clock） | ホストの実時間。`Date.now()` / `Instant::now()` が返す量。ホストの速度・負荷・スケジューラに依存し、(プログラム, 入力) の関数では**ない** | 一般用語 |
| **論理時計**（logical clock） | 物理時間から独立した進行の尺度。**用語の衝突に注意** — 分散システムの Lamport 論理時計は happens-before の半順序を数えるものであり、本 ADR の用法（プログラム自身の実行が進める決定的カウンタ）とは別概念である | [Lamport 1978](https://lamport.azurewebsites.net/pubs/time-clocks.pdf)（同名の別概念） |
| **決定的時計**（deterministic clock） | 本設計の論理時計。CM-1 のコスト表に従って charge site ごとに進む。値は (プログラム, 入力) の関数で、native / wasm / 全ホストで一致する。壁時計と対をなし、**どちらを読むかは fan の head が名乗る** | [logical-time-async.md](../roadmap/active/logical-time-async.md) |
| **charge site** | 決定的時計が進む**唯一の点**。共有 MIR の basic block 入口などに置かれる。fuel 系の構文はここでカウンタを読み、oracle 系の構文は同じ点で環境を読む（中断点統一原理） | [logical-time-implementation.md](../roadmap/active/logical-time-implementation.md) |
| **lockstep 意味論** | race の勝者選択を語る絵 — 全枝が決定的時計を同じ歩幅で進み、最初に完了した枝が勝つ。同着はソース順。実装が使う等価な特徴づけは「(消費, index) の辞書式最小」で、**この 2 つが一致すること**が「物理時間なしの race」の内容である | [logical-time-async.md §決定的事象規則](../roadmap/active/logical-time-async.md)、[Decisive.lean `Ev.prec`](../../crates/almide-race-belt/AlmideRaceBelt/Decisive.lean) |
| **versioned な抽象コスト表**（CM-1） | 各 MIR op に決定的時計の進み幅を割り当てる表。契約台帳に載る**意味論的オブジェクト**であり、定数の変更は semantic change（版数バンプ）として扱う。EVM の gas schedule が同型の先例 | [Yellow Paper](https://ethereum.github.io/yellowpaper/paper.pdf) Appendix G |
| **EVM**（Ethereum Virtual Machine） | Ethereum の実行環境。全ノードが同じ状態遷移に同じ gas schedule を適用し、同じ out-of-gas 判定へ到達することで合意を保つ。**決定的な計算量上限の、最大規模の実運用例** | [Yellow Paper](https://ethereum.github.io/yellowpaper/paper.pdf) / [ethereum.org: gas](https://ethereum.org/developers/docs/gas/) |
| **race belt の 7 定理** | `crates/almide-race-belt/` の Lean 4 定理群（0 sorry、CI `lean-proofs` で kernel-check）。決定的事象の一意性・部分集合安定性・枝刈り cap が決定事象と trap 可視窓を隠せないこと・合流。**すべて単位に依存しない**（事象の時刻は `Nat`）ため、本 ADR の単位変更で 1 文字も変わらない | [Decisive.lean](../../crates/almide-race-belt/AlmideRaceBelt/Decisive.lean) |
| **fuel** | 決定的時計を実装する**機構側**の語（内部カウンタ、`--fuel-probe`、Wasmtime / EVM の系譜）。表面の語彙には出さない | [Wasmtime: fuel vs epoch](https://docs.wasmtime.dev/examples-interrupting-wasm.html) |

## Context

決定的な計算量予算（`fan.bounded` / `fan.race`）の表面表記を決める必要があった。
設計は 3 案を順に通過している。

1. **`fuel:`** — Wasmtime / EVM の系譜が名前から見える。却下：機構のメタファーが
   表面に漏れる。fuel は燃料であって単位ではない。
2. **`ticks:`** — 「ラベル = 単位名」で `ms:` と揃う。lockstep 意味論の単位そのもの。
   ここまでは筋が通っていた。
3. しかし `ticks: 100_000` には**書けない数字**という欠陥があった。

3 の欠陥が本 ADR の出発点である。tick の絶対値は CM-1（versioned な抽象コスト表）に
依存し、人間にも LLM にも事前の直感がない。さらに予算は body の実装に結合するので、
`optimal_plan` をリファクタすれば `100_000` は腐る。**編集で腐るマジックナンバーを必須引数に
するのは、modification survival を掲げる言語の自己矛盾に近い。**

しかも予算の誤りは**両方向とも静か**である。小さすぎれば常にフォールバックへ落ち、
大きすぎれば実質無制限になる。どちらも診断で捕まらない。

## Decision

**決定的な計算量予算は、時間の単位で書く。単位は型が運び、構築は現行構文の関数呼び出しで行う（新しいリテラル構文は追加しない）。**

```almide
fan.bounded(compute.ms(100)) { optimal_plan(g) } ?? greedy_plan(g)   // 決定的時計
fan.race { exact(p); heuristic(p) } ?? fallback(p)                    // 予算なしが基本形
fan.race(compute.ms(500)) { search_a(p); search_b(p) } ?? none        // 発散ガードは任意
fan.timeout(duration.s(5)) { http.get(url) } ?? cached                // 壁時計（oracle 層）
```

付随して以下を確定する。

### D1. 論理時計の単位は時間である

CM-1 は各 MIR op に「**凍結された Almide 抽象機械での所要時間**」を割り当てる。
プログラムの消費はその総和 = ひとつの持続時間であり、(プログラム, 入力) の関数である。
どのホストでも同じ値を返す。**数えるものは決定的（fuel 側）、呼ぶ名前は時間。**
この 2 つは分離できる — 証拠は D6。

### D2. 型で分ける（単位ではなく）

決定的時計の量と壁時計の量は**別の型**とする（型名確定: `Compute` / `Duration` — 仕様 S1）。単位はどちらも ms / s を共有する。
呼び出しサイトは head が区別するが、変数や設定レコードを経由した混入は型でしか
止まらない。Go の `time.Duration` に相当する **wrap した型**であり、現行構文で
表現できる（`type Compute = Compute(Int)` の単一ケース variant、または `fan` と
同じくコンパイラ既知の型）。実装形は PR で確定するが、**どちらでも言語構文の追加は
不要**である。

**裸の整数は型エラー**。`fan.bounded(100)` は「expected Compute, found Int」で
コンパイルしない。単位のない整数を受ける API が起こす 1000 倍事故は Go の
`time.Sleep(10)`（10 ナノ秒）から Jenkins の「300 秒 → 3.5 日」まで実例に事欠かない。

### D3. 構築は現行構文の関数呼び出し — 新しいリテラル構文は追加しない

単位はモジュール関数の名前が運ぶ。**モジュールが時計を、関数が単位を名乗る。**

```almide
compute.ms(100)     compute.s(2)      compute.us(500)     // 決定的時計 → Compute
duration.s(5)       duration.ms(300)  duration.min(3)     // 壁時計 → Duration
compute.ms(n)                                             // 変数もそのまま
```

単位名の集合は `ns` / `us` / `ms` / `s` / `min` / `h`（ASCII のみ、`µs` は採らない。
分は `min` — `m` は曖昧）。日以上は入れない（Go が Day を持たない理由と同じ）。
この集合は**完備性 matrix としてゲートに載せる** — 手で維持される表面はドリフトし、
LLM は `msec` や `5m` を発明する。

**リテラル接尾辞（`100ms`）は採らない。** 初版の決定はこの形だったが、lexer への
数値サフィックス追加という**言語構文の新規追加**が必要になる。現行の Almide に
その機構はなく、`fan` の表面のためだけに言語全体の字句規則を広げる取引は割に合わない。
関数形は語数が増えるだけで、単位の明示性・型安全・ゲート可能性はすべて同じである。

将来 `100ms` を入れる場合も、**この関数形が脱糖先になる**（`100ms` ≡ `compute.ms(100)`）。
つまり今の決定は将来の糖衣を閉じない。糖衣を足す条件は D3-F（下記 Falsifier）。

### D4. 抽象機械は凍結する

モデル誤りの訂正（ある op の相対コストが間違っていた）は行う。**ハードウェア進化への
追随（全体の再スケール）は行わない。** 10 年で壁時計との比が数倍ずれるが、それは
今日すでにある機械間のばらつきと同じオーダーであり、代わりに「プログラムの意味は
永久に変わらない」を買う。

### D5. 校正はゲートで機械検査し、対応幅を宣言する

決定的 ms と実測壁時計 ms の比を CI で測り、宣言した帯の中にあることを検査する。
帯を外れるワークロード種別は文書化する。レポートは**常に両方を併記**する:

```
this region: 0.42ms deterministic (≈0.31ms wall here)
```

### D6. 「比のみが契約、絶対値は非契約」を初日に宣言する

op 間の相対コストが契約であり、絶対値は契約ではない。CM 版数バンプで絶対値は動きうる。

## 仕様 — 時間指定の完全な表面（normative）

Decision（D1–D6）を、実装 PR がそのまま転写できる完全な表面仕様に落とす。
**完全性の定義は repo の matrix 原則に従う**: 存在するセルを面で列挙し、存在しない
セルは意図的省略として理由と再開条件つきで宣言し、実行可能ゲートが面を検査する。

### S1. 型

| 型 | 意味 | 構築モジュール | derive |
|---|---|---|---|
| `Compute` | 決定的時計の量（前提節の第 3 行） | `compute` | `Eq, Ord, Repr` |
| `Duration` | 壁時計の量 | `duration` | `Eq, Ord, Repr` |

- **表現**: 内部単位はナノ秒、`Int`（i64）1 本。上限 ≈292 年。Go と同じ天井だが、
  調査で確認した事故源は天井ではなく **wraparound**（Swift `sleep(nanoseconds: .max)`
  即返りバグ）と **D×D**（Go #64420）であり、前者は飽和演算（S3）、後者は型エラー
  （S3）で殺す。
- 両型は**同じ演算面**を持つ（S2/S3 の matrix は型ごとに複製され、ゲートは両方を
  検査する）。
- コンパイラ既知型か stdlib `.almd` 型かは実装 PR の自由（観測可能な仕様は本節で
  確定しており、どちらでも変わらない）。

### S2. 構築 — 2 時計 × 6 単位の完全 matrix

| 単位 | `compute.*` | `duration.*` |
|---|---|---|
| `ns` | `compute.ns(n)` | `duration.ns(n)` |
| `us` | `compute.us(n)` | `duration.us(n)` |
| `ms` | `compute.ms(n)` | `duration.ms(n)` |
| `s` | `compute.s(n)` | `duration.s(n)` |
| `min` | `compute.min(n)` | `duration.min(n)` |
| `h` | `compute.h(n)` | `duration.h(n)` |

12 セル全部が存在する（ゲート検査対象）。規則:

- 引数は `Int` のみ（`Float` 版は意図的省略 — S7）。
- **負値は決定的 trap**（台帳定数メッセージ、両ターゲット一致を fixture で pin）。
  構築後の値は不変量 ≥0 を持つ。0 は有効。
- 構築時の乗算オーバーフロー（`compute.h(大値)` 等）は**飽和**。
- **UFCS 形 `n.ms()` は曖昧エラー** — レシーバ `Int` に対し `compute.ms` と
  `duration.ms` の両候補が該当し、どちらの時計かを推論で選んではならない（沈黙の
  既定は本 ADR の存在理由に反する）。診断は両候補を名指しし、修飾形を要求する。
- 単位名集合は CLI（S4）と**単一ソース共有**（`scripts/lib/contract-classes.txt` 方式）。

### S3. 演算 matrix

`T` は `Compute` / `Duration` のそれぞれ（型ごとに同じ面）:

| 演算 | 判定 | 根拠 |
|---|---|---|
| `T + T → T`（飽和） | **あり** — 予算・期限の合成 | |
| `T - T → T`（0 で飽和） | **あり** — 残量計算 | 負にしない（不変量 ≥0） |
| `T * Int → T` / `Int * T → T`（飽和） | **あり** — 非対称スケーリング | Go #20757 の教訓: 「数でスケールする。duration ではスケールしない」 |
| `T * T` | **型エラー** | Go #64420 の 10⁹ 倍サイレント事故。Go は型で表現できず専用リンタが要った — 我々は型で殺す |
| `T / Int` | 意図的省略（S7） | 丸め意味論の決定を先送り |
| `==` / `<` 等（同型内） | **あり**（`Ord`） | |
| `Compute ⊕ Duration`（全演算） | **型エラー** | 時計の混合。Ada が別型にした理由そのもの |
| `Int ⊕ T`（スケーリング以外） | **型エラー** | 裸整数の遮断（D2） |
| `Compute ↔ Duration` の変換関数 | **存在しない** | 2 つの時計に意味論上の橋はない。唯一の橋は tooling の併記表示（D5）であり、それは値ではなく報告である |

### S4. 消費者 matrix — どこに何の型が現れるか

| 表面 | 取る型 | 多重度 |
|---|---|---|
| `fan.bounded(c) { body }` | `Compute` | 必須 1 |
| `fan.race { arms }` / `fan.race(c) { arms }` | `Compute` | 任意 0..1（発散ガード） |
| `fan.race(xs, f)` / `fan.race(c, xs, f)` | `Compute` | 任意 0..1 |
| `fan.timeout(d) { body }`（Stage 4、oracle 層） | `Duration` | 必須 1 |
| 効果表面の期限（http / process 等 — 形は将来の設計。型だけここで pin） | `Duration` | — |
| CLI `almide run/test --budget 500ms` | `Compute`（CLI 文字列は同じ 6 単位の接尾辞表記 — CLI は言語構文ではないので接尾辞可） | 任意 |
| レポート（`--time-report`） | 両方併記: `0.42ms deterministic (≈0.31ms wall here)` | D5 |

誤配置の診断は fixture で文言ごと pin する（`expected Compute, found Duration` +
head 誘導ヒント — fan-v2-examples/diagnostics.almd と一致させる）。

### S5. 端の意味論

- **ゼロ予算**: check-then-charge なので、charge 前に完了する式は成功する —
  `fan.bounded(compute.ms(0)) { 42 }` → `Ok(42)`。最初の charge を踏む式は exhaust。
- **負**: 構築時 trap（S2）により、消費側に負値は到達しない。
- **飽和上限**: `Compute` の最大値は有限（≈292 年）だが実行不能量であり、race の
  「予算なし = n が無限」とは別物として扱う（予算なしは引数の不在、上限は値）。

### S6. 実行可能ゲート（実装 PR と同一 PR で land）

1. 構築子 12 セルの存在と型シグネチャ（S2 matrix の面検査）
2. 負値 trap の両ターゲット観測一致 fixture（`spec/wasm_cross`）
3. 型エラー各 1 の診断 fixture: 裸整数 / 時計混合 / `T * T` / UFCS 曖昧
4. CLI の単位名集合 = 言語の単位名集合（単一ソースからの生成を検査）
5. 飽和演算の境界 fixture（オーバーフロー構築、`T - T` の 0 飽和）

### S7. 意図的省略の台帳（再開条件つき）

| 省略 | 理由 | 再開条件 |
|---|---|---|
| `d`（日）以上の単位 | 予算・期限に無意味。Go が Day を持たない判断と同根 | 実需要の記録 |
| `Float` 構築子（`duration.s(1.5)`） | Go #20757 の float 誤用クラス。`ms` で書けば整数で足りる | dojo で整数換算の書き誤りが観測されたら |
| `T / Int` | 丸め意味論（切り捨て/最近接）の決定を先送り | 実需要 |
| `min` / `max` ヘルパ | 入れ子の min-cap は意味論が暗黙に担う（EIP-150 式） | 明示的な予算合成の需要 |
| accessor（`in_ms(c)` 等の読み出し） | 表示は `Repr` で足りる | 予算を計算に使う実例 |
| `Codec` derive | 直列化境界を跨ぐ用例がない | 用例の出現 |
| リテラル接尾辞 `100ms` | D3 のとおり（lexer 拡張の取引が不成立）。脱糖先は確定済み | D3-F（dojo 実測） |

## Rationale

### 決定的だったのは「計算予算は実質すでに時間である」という発見

EIP-2929 の Motivation、逐語:

> "Generally, the main function of gas costs of opcodes is to be **an estimate of the
> time needed to process that opcode**, the goal being for the gas limit to correspond
> to a limit on the time needed to process a block."

「gas は意図的に時間を避けた」という一般的理解は一次資料に否定される。gas は時間の
見積もりであり、単位だけが無次元である。ewasm はさらに露骨で、換算を名指しの
2014 年 Haswell CPU に固定している（「1 second of CPU execution equals to 10 million
gas」）。Solana は内部で 30 CU/µs、eBPF の 100 万命令上限は「典型的な x86 で 0.1 秒」
から選ばれている。

Go の pprof に至っては、返す「cpu / nanoseconds」が実際には**サンプル数 × 10ms の
定数**である（`b.period = 1e9 / hz`, `hz = 100`）。クロックは読んでいない。
カウント × 校正定数を「ナノ秒」と名乗る構成は、すでに本番で動いていて誰も嘘だと
言っていない。

よって問いは「計算量を時間で書かせてよいか」ではなく、**「すでに時間の見積もりで
あるものを、時間と名乗るか、抽象単位の名前を着せ続けるか」**である。

### 抽象単位に留まる理由が、我々には無い

他系が無次元を選んだ理由は 2 つに集約され、どちらも当たらない。

- **gas は通貨単位である。** gas price を掛けて課金するので、単位が gas でないと
  経済が成立しない。Almide に価格の次元はない。
- **EVM は再価格付けを前提とする。** 実測との対応が改訂され続ける量に ms を貼ると
  改訂のたびに単位の意味が壊れる。これは**再スケールする場合の**議論であり、
  D4（凍結）が答えになる。凍結の先例は gem5（`1 Tick == 1 ps`、一度固定したら
  変更は panic）と POSIX（`CLOCKS_PER_SEC` を 10^6 に凍結し、「実解像度は
  マイクロ秒精度である必要はない」と規格が自白）。

### 抽象単位では直感が育たないことが、全系で実証されている

> **調査の結論: 「ユーザーが数字を直感することに成功した系は 1 つも見つからなかった。」**

- **EVM**: `eth_estimateGas` は二分探索。63/64 則で必要 gas が非単調になり
  （gas used ≠ 必要 gas limit）、MetaMask は自動設定 + 推定失敗時にブロック上限の
  95% というフォールバックで UX が破綻した
- **NEAR**: 200 Tgas 時代、Aurora の 27.4 万トランザクションのうち **9.2% が上限
  到達で失敗**（失敗分が gas 使用量の 26%）。300 Tgas 化で 1.0% に。参照 FT 実装は
  実消費の何倍もの 30 TGas を添付しており、**過大添付が業界標準の回避策**になっている
- **eBPF**: 「4k limit was confusing to users」「コードを足すと verifier が通り、
  削ると落ちる」
- **Earth Engine EECU**: 「見積もりが非常に困難」
- **F\***: `--fuel` は「既定 0 で作業し、非ゼロは文書化せよ」— 調整不能な抽象単位を
  0 に固定する設計

解決策は全系で同一（実行して測る推定器 + 安全マージン + 大きい既定値）であり、
**抽象単位に逃げても問題は解決しない。**

### 時間単位を表面に出した先例は存在する

- **Cloudflare Workers**: 設定キーが literally `"limits": { "cpu_ms": 300000 }`。
  壁時計側は "No limit"（HTTP duration は無制限）で、制限は CPU 時間だけ。
  理由づけが我々の主張と逐語で一致する — 「CPU time is more predictable and
  **under your control**… **purely a function of the logic and processing of inputs
  on outputs to your Worker**」
- **NEAR**: 「we define 10^15 gas to be executed in at most 1s… 1 Tgas corresponds
  to **1ms execution time**」。しかも Runtime Parameter Estimator が「1ms = 1 Tgas 則」
  に対して既存パラメータを**機械検査**している（D5 の先例）
- **F\* `--z3rlimit`**: 「決定的、機械非依存」と述べたうえで「n は **mythical
  powerful laptop** で許される秒数を直感的に数える」「correspondence is quite good,
  **it's not perfect**」（D5 の正直さの型）
- **Ada**: `type CPU_Time is private` — 単位は秒建てだが `Real_Time.Time` とは
  **別の型**（D2 の先例）
- **gem5**: `simSeconds` / `simTicks` / `hostSeconds` を全部別名で併記（D5 の型）

### MSR の観点では時間単位が「正しさ」に効く

LLM は tick の予算を一度も見たことがない。`ticks: 100_000` は 6 桁外しうる。
`100ms` には事前分布がある。予算誤りが両方向とも静かである以上、
**数字が推測可能であることは利便性ではなく正しさの問題**である。

### 単位の変更が意味論の変更でないことは、機械検査で担保されている

Lean の `Ev { time : Nat }` も全数モデルの `u64` も**単位に一切依存していない**。
tick を ns と読み替えても [race belt](../../crates/almide-race-belt/) の 7 定理と
74,898 構成の合流ゲートは 1 文字も変わらない。Wasmtime が名指しした軸
（fuel = 決定的 / epoch = 壁時計で非決定的）は「**何を数えるか**」の軸であって
「何と呼ぶか」の軸ではない。全系がこの 2 つを分離してこなかったのは分離する理由が
なかったからで、我々には分離できる証拠がある。

### 我々に有利な構造的事実

「見積もれない」の真因は抽象単位ではなく**コストの状態依存性と非単調性**である
（EVM の 63/64 則、eBPF の verifier 探索）。**Almide のコストは単調で状態非依存**
であり、あの病理は最初から存在しない。

## Alternatives

| 案 | 却下理由 |
|---|---|
| **`fuel:`** | 機構のメタファーが表面に漏れる。fuel は単位ではない |
| **`ticks:`** | 書けない数字（本 ADR の Context）。抽象単位で直感が育った系はゼロ |
| **`compute_ms:` 等の修飾ラベル** | Cloudflare が `cpu_ms` と修飾したのは設定ファイルに壁時計も同居するため。Almide は head が時計を名乗る（`bounded` / `timeout`）ので二重表示になる。型（D2）が同じ役割をより強く果たす |
| **Postgres 型の妥協**（既定は無次元、ms は特定環境向けオプトイン） | Postgres のコスト定数は**管理者が調整する設定値**だから両建てが成立する。ソースコードに書く予算には移せず、同じ量に 2 つの綴りを許すのは共存負債 |
| **参照機を物理マシンで定義**（F\* / ewasm 方式） | 老朽化する。ewasm は「CPU は改善し続けるので 3 年ごとに調整」と明記しており、その調整が既存プログラムの意味を壊す（EIP-1884 は 2300 gas stipend 前提の `transfer()` を壊し、Aragon の関数は 1759→2359 gas で閾値超え）。D4（凍結した抽象機械）を採る |
| **ユーザー拡張可能なリテラル接尾辞**（C++ UDL 方式） | Google C++ スタイルガイドは標準ライブラリの `100ms` すら禁止しているが、理由は全部**拡張性の帰結**（名前空間修飾不可、`using` 必須、サードパーティは `_ms`）。拡張機構は作らない |
| **言語所有の閉じたリテラル接尾辞**（CSS 方式、`100ms`） | **見送り（却下ではない）**。CSS は閉じた集合が機能する反証であり設計として健全だが、lexer への数値サフィックス追加＝**言語構文の新規追加**が要る。`fan` の表面のためだけに字句規則を広げる取引は割に合わない。関数形と意味は同じで、将来入れるならその脱糖先になる（D3） |
| **コンストラクタ関数のみ**（Rust / Swift 方式） | **採用**（D3）。Kotlin が 1.5 で簡潔形を廃止して静的ファクトリに寄せ 1.6 で差し戻した事実は「簡潔形が好まれる」証拠だが、Kotlin の簡潔形は**拡張プロパティ**という既存の言語機能で実現されており、新しい字句規則は要らなかった。Almide にはその機能がないので、同じ簡潔さを買う価格が違う |
| **裸の整数 + ドキュメント**（Go 方式） | Go の `time.Sleep(10)` は 10 ナノ秒。実事故（hudl/fargo #56）があり staticcheck が SA1004 を持ち、`Duration × Duration` が型検査を通る問題（#64420）は *not planned* で閉じ専用リンタが必要になった。Go チーム自身が #20757 で「Go の Duration は Rust/Java/C#/Python より危険」と認めている。Swift と Zig はどちらも最近ベア整数 API から型付き Duration へ移行した |

### 前回の却下を訂正する

[ticks-interface-audit.md](../roadmap/active/ticks-interface-audit.md) は
「時間風単位の tick 換算（`budget: 100.ms`）」を**誤読の逆再演**として却下していた。
これは**深刻度の混同**だった。`fan.timeout(1000)` の事故は「壁時計依存 = 決定性が
壊れる」であり、論理時間を時間単位で書かせる案は**決定性を一切壊さない**（ずれるのは
壁時計との対応だけ）。同じ棚に置いた前回の判断が誤りである。本 ADR がその節を
supersede する。

## Consequences

**得るもの**

- 数字が書ける。`100ms` には人間にも LLM にも事前分布がある
- 単位系が 1 つになる。決定的時計と壁時計が同じ語彙（ms / s）を共有し、
  区別は head と型が持つ
- ラベル機構がパーサから消える。`fan.bounded(<expr>) { body }` でよく、
  head-args のラベル解析は不要になる（fan-v2 の表面が縮む）
- **言語構文の追加がゼロ**。型と stdlib モジュールの追加だけで成立し、lexer も
  parser も触らない
- コスト表に外部アンカーが生まれる。tick 表は「間違いようがない」= 校正不能だが、
  時間の表は参照機械に対して**間違いうる → 直せる**。CM 改版が恣意的ドリフトではなく
  真値への収束になり、校正をゲートで機械検査できる（D5）
- 裸の整数が型エラーになり、単位取り違えの事故クラス（Gearman の 17 分タイムアウト、
  Jenkins の 300 秒 → 3.5 日）が構文で死ぬ

**払うもの**

- **看板と中身の幅**。gem5 はサイクル精度シミュレータでありながら実機との runtime
  誤差が平均絶対 13〜17%（ARM サーバで MAPE 26〜30%）。我々の重み付き op カウントは
  キャッシュも分岐予測もメモリ階層もモデル化しないので、ワークロードによっては
  数倍ずれる。「ms」と名乗る看板に対して中身に幅があることは、D5 の帯宣言と
  レポート併記で正直に扱う
- **誤読の可能性**。「100ms で返ってくる」と読んだ利用者は、遅い機械で壁時計 300ms を
  見る。ただし**プログラムの振る舞いは全ホストで同一**であり、壊れるのは壁時計
  レイテンシの期待だけで、正しさと決定性は無傷。`fan.timeout(1000)` の事故
  （非決定性）とは深刻度が 2 桁違う
- **語数**。`compute.ms(100)` は `100ms` より長い。最頻の呼び出しサイトで 12 文字の
  差が出る
- **型が 2 つ増える**（`Compute` / `Duration`）。Vocabulary Economy に触れるが、
  Ada の先例と D2 の事故防止で購う

**変わらないもの**

- Lean の 7 定理、74,898 構成の合流ゲート、T1–T9 — すべて単位非依存
- 意味論（lockstep、決定的事象規則、trap 可視窓、min-cap 入れ子）
- `fan {}` / `fan.map` / `fan.any` / `fan.settle` の表面

## Falsifier

- **校正ゲートの実測で対応幅が広すぎたら**（宣言する帯を超えたら、目安として 5 倍）、
  ms の看板を下ろして無次元の名前に戻す。gem5 が 13〜30% なのだから、我々が数倍で
  収まる保証はまだない。**Stage 1 の probe が最初の実測を出す**
- 誤読が事故クラスとして観測されたら（`fan.bounded` の予算を壁時計と信じたコードが
  実害を出したら）、D2 の型名を自己説明的なものに変えるか、修飾ラベルへ戻す
- **D3-F**: 関数形が MSR を実測で下げたら（dojo で `compute.ms(100)` の書き誤りが
  有意に出たら）、リテラル接尾辞 `100ms` を糖衣として足す。脱糖先は既に関数形なので、
  意味論の変更にはならない
- ハードウェア進化への追随（D4 の凍結の放棄）が避けられない事情が出たら、
  本 ADR ごと supersede する（凍結が ms 表記の前提条件であるため）

- 負値 trap が実地で過酷すぎたら（構築引数が計算値で負に落ちる正当なパターンが
  記録されたら）、0 飽和構築へ改訂する（S5 の不変量は保たれる）。
- UFCS 曖昧エラーが MSR を下げたら（修飾忘れが高頻度なら）、短い修飾形を検討する
  — ただし時計の暗黙選択は選択肢にない。

## References

**一次情報**

- [EIP-2929](https://eips.ethereum.org/EIPS/eip-2929) — gas costs are "an estimate of the time needed"
- [EIP-150](https://eips.ethereum.org/EIPS/eip-150), [EIP-1884](https://eips.ethereum.org/EIPS/eip-1884) — 再価格付けと既存契約の破壊
- [ewasm: determining wasm gas costs](https://github.com/ewasm/design/blob/master/determining_wasm_gas_costs.md) — 2014 Haswell 固定、3 年ごと調整
- [NEAR nomicon: Gas](https://nomicon.io/architecture/gas/index.html) — 「1 Tgas = 1ms」
- [NEAR parameter definition](https://near.github.io/nearcore/architecture/gas/parameter_definition.html) — estimator による機械検査
- [NEPs discussion #305](https://github.com/near/NEPs/discussions/305) — 9.2% 失敗
- [Cloudflare Workers limits](https://developers.cloudflare.com/workers/platform/limits/) — `cpu_ms`
- [Cloudflare pricing blog](https://blog.cloudflare.com/workers-pricing-scale-to-zero) — 「purely a function of the logic」
- [F\* rlimits](https://github.com/FStarLang/FStar/wiki/rlimits:-Machine-Independent-Resource-Limits-for-Deterministic-Execution) — mythical powerful laptop
- [Wasmtime Store::set_fuel](https://docs.wasmtime.dev/api/wasmtime/struct.Store.html) / [Interrupting Execution](https://docs.wasmtime.dev/examples-interrupting-wasm.html) — fuel と epoch の軸
- [gem5 core.cc](https://raw.githubusercontent.com/gem5/gem5/stable/src/sim/core.cc) — `1 Tick == 1 ps` 凍結
- [Gutierrez et al., ISPASS 2014](https://tnm.engin.umich.edu/wp-content/uploads/sites/353/2017/12/2014.03.Sources-of-error-in-full-system-simulation.pdf) — gem5 の 13〜17% 誤差
- [Ada RM D.14](https://ada-lang.io/docs/arm/AA-D/AA-D.14/) — `type CPU_Time is private`
- [POSIX clock()](https://pubs.opengroup.org/onlinepubs/9699919799/functions/clock.html) — `CLOCKS_PER_SEC` 凍結
- [PostgreSQL planner cost constants](https://www.postgresql.org/docs/current/runtime-config-query.html) — 「arbitrary scale, only relative values matter」
- [Solana block_cost_limits.rs](https://github.com/solana-labs/solana/blob/master/cost-model/src/block_cost_limits.rs) — 30 CU/µs
- [Linux commit c04c0d2](https://github.com/torvalds/linux/commit/c04c0d2b968ac45d6ef020316808ef6c82325a82) — eBPF「1M instructions in 1/10 of a second」

**言語の Duration 設計**

- [Go time](https://pkg.go.dev/time), [golang/go#20757](https://github.com/golang/go/issues/20757), [#64420](https://github.com/golang/go/issues/64420), [staticcheck SA1004](https://github.com/dominikh/go-tools), [hudl/fargo#56](https://github.com/hudl/fargo/issues/56)
- [Kotlin 1.6 リリースノート](https://kotlinlang.org/docs/whatsnew16.html) — 拡張プロパティの差し戻し
- [JetBrains/kotlin#4119](https://github.com/JetBrains/kotlin/pull/4119) — 1.5 での廃止
- [Rust pre-RFC: custom suffixes](https://internals.rust-lang.org/t/pre-rfc-custom-suffixes-for-integer-and-float-literals/8029)
- [Google C++ Style Guide](https://google.github.io/styleguide/cppguide.html) — UDL 禁止とその理由
- [CSS Values 4 §time](https://www.w3.org/TR/css-values-4/#time) — 閉じた組み込み接尾辞
- [SE-0329](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0329-clock-instant-duration.md), [Zig lib/std/Io.zig](https://github.com/ziglang/zig/blob/master/lib/std/Io.zig)

**内部**

- [race belt](../../crates/almide-race-belt/) — 単位非依存の 7 定理
- [logical-time-race spike](../../research/spike/logical-time-race/) — 74,898 構成の合流ゲート
