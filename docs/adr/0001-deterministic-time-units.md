# ADR-0001: Deterministic budgets are written in time units

- **Status**: Accepted
- **Date**: 2026-08-01
- **決定範囲**: `fan.bounded` / `fan.race` の計算量予算の表面表記、および決定的時計の単位定義
- **関連**: [async-inception.md](../roadmap/active/async-inception.md)（憲章）、
  [logical-time-async.md](../roadmap/active/logical-time-async.md)（意味論）、
  [fan-v2.md](../roadmap/active/fan-v2.md)（文法）

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
`optimal_plan` をリファクタすれば `100_000` は腐る。**編集で腐る魔法数を必須引数に
するのは、modification survival を掲げる言語の自己矛盾に近い。**

しかも予算の誤りは**両方向とも静か**である。小さすぎれば常にフォールバックへ落ち、
大きすぎれば実質無制限になる。どちらも診断で捕まらない。

## Decision

**決定的な計算量予算は、時間の単位で書く。リテラルは裸（修飾なし）、区別は型が持つ。**

```almide
fan.bounded(100ms) { optimal_plan(g) } ?? greedy_plan(g)   // 決定的時計
fan.race { exact(p); heuristic(p) } ?? fallback(p)          // 予算なしが基本形
fan.race(500ms) { search_a(p); search_b(p) } ?? none        // 発散ガードとして任意
fan.timeout(5s) { http.get(url) } ?? cached                 // 壁時計（oracle 層）
```

付随して以下を確定する。

### D1. 論理時計の単位は時間である

CM-1 は各 MIR op に「**凍結された Almide 抽象機械での所要時間**」を割り当てる。
プログラムの消費はその総和 = ひとつの持続時間であり、(プログラム, 入力) の関数である。
どのホストでも同じ値を返す。**数えるものは決定的（fuel 側）、呼ぶ名前は時間。**
この 2 つは分離できる — 証拠は D6。

### D2. 型で分ける（単位ではなく）

決定的時計の量と壁時計の量は**別の型**とする（作業名 `Compute` / `Duration`。
最終的な型名は実装 PR で確定）。単位はどちらも ms / s を共有する。
呼び出しサイトは head が区別するが、変数や設定レコードを経由した混入は型でしか
止まらない。

裸のリテラル `100ms` は**文脈から型が決まる**（`fan.bounded` の引数位置なら
`Compute`、`fan.timeout` なら `Duration`）。文脈のない `let x = 100ms` は
曖昧エラーとし、注釈を要求する（沈黙の既定は MSR に反する）。

### D3. リテラルは言語所有の閉じた接尾辞集合

`ns` / `us` / `ms` / `s` / `min` / `h`。ASCII のみ（`µs` は採らない）。分は `min`
（`m` は曖昧）。日以上は入れない。ユーザー拡張は**不可** — 拡張機構は作らない。

変数からの構築は `duration.ms(n)` 系のコンストラクタ（接尾辞はリテラルにしか付かない）。
これは同義の 2 綴りではなく、リテラルと計算値の区別である。

**裸の整数は型エラー**。`fan.bounded(100)` はコンパイルしない。

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
| **ユーザー拡張可能なリテラル接尾辞**（C++ UDL 方式） | Google C++ スタイルガイドは標準ライブラリの `100ms` すら禁止しているが、理由は全部**拡張性の帰結**（名前空間修飾不可、`using` 必須、サードパーティは `_ms`）。CSS が反証で、言語所有の閉じた集合にはこれらの問題がない。よって**閉じた集合として採用し、拡張機構は作らない** |
| **コンストラクタ関数のみ**（Rust / Swift 方式） | Kotlin が両方向で実験済み。1.5 で `100.milliseconds` を廃止して静的ファクトリに寄せ、1.6 で「コミュニティのフィードバックに応えて」差し戻した。逆方向の失敗例（簡潔形が書きにくくて放棄された）は見つからなかった |
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
- **リテラル接尾辞の lexer 実装**が要る（閉じた集合なので小さいが、言語に他の接尾辞は
  ない）
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
- リテラル接尾辞が LLM に書けないと dojo の計測で出たら、コンストラクタ形へ寄せる
- ハードウェア進化への追随（D4 の凍結の放棄）が避けられない事情が出たら、
  本 ADR ごと supersede する（凍結が ms 表記の前提条件であるため）

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
