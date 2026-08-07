# ADR-0011: The execution substrate is a free variable — and one arm's output is the last hole in the determinism claim

- **Status**: Proposed
- **Date**: 2026-08-07
- **決定範囲**: `fan` の実行基体（何が実際に並行に走るか）、その target ごとの選択、
  基体を自由変数として扱うための前提条件、および C-004 EXCEPTION の分類の訂正
- **関連**: [async-inception.md](../roadmap/active/async-inception.md)（観測の憲章）、
  [execution-inception.md](../roadmap/active/execution-inception.md)（本 ADR の憲章）、
  [concurrency-stance.md](../roadmap/active/concurrency-stance.md)（#1000）、
  [ADR-0001](./0001-deterministic-time-units.md)（決定的時計）
- **台帳への影響**: C-004 の EXCEPTION 節の**分類が誤り**（Rationale R1）。C-006 の
  「sole stdlib surface」という記述も、その誤分類の結果として不正確になっている

## Context

### 実測 — 例外条項を再現した

契約台帳 C-004 は、次の例外条項を持つ。

> EXCEPTION: side-effect INTERLEAVING inside `fan { }` block arms and `fan.settle`
> thunks is wall-clock on native (both run on real threads) and sequential on wasm —
> they pin their RESULT (tuple / list) order only.

この条項は「native と wasm で出力の混ざり方が違う」と読める。**実際に走らせた**
（2026-08-07、almide 0.56.0 / wasmtime 47.0.2 / 14 コア / arm64 macOS）。

```almide
effect fn arm(tag: String, extra: Int) -> Int = {
  let v = spin(6000000 + extra, 1)   // extra は env.args() 由来 — 定数畳み込み不能
  println("${tag}")
  v
}

effect fn main() -> Unit = {
  let n = list.len(env.args()!)
  let (a, b, c, d) = fan { arm("A", n)  arm("B", n)  arm("C", n)  arm("D", n) }
  println("sum=${a + b + c + d}")
}
```

```
native × 10 回 — 出力順:
   2  ACDB
   1  DCAB / DBCA / CBAD / BDCA / BCAD / BACD / ADCB / ACBD
   → 10 回で 9 通り

wasm × 3 回 — 出力順:
   3  ABCD                                    ← 常に arm 順
```

条項の記述どおり native ⇄ wasm は食い違う。だが**記述にない事実**が同時に出た。

### 見つかった事実 — native は自分自身とも食い違う

10 回中 9 通り。これは target 間の差ではなく、**同一バイナリ・同一入力の実行間で
観測が変わる**という性質である。台帳はこの性質に名前を持っている。C-006 の文章：

> Worse than the cross-target divergence: whether the deadline fired depended on
> machine load, so the result was not a function of the program + its inputs
> **even between two native runs** — **the sole stdlib surface violating that property.**

`fan.timeout` が 0.29.0 で撤去された理由そのものである。そして C-006 は、それが
**唯一の**表面だったと書いている。上の実測は、その記述が今日不正確であることを示す。
`fan { }` に印字する arm を 2 本置けば、同じ性質が今日も出荷されている。

見落としは「例外がある」ことではない。**例外の分類**である。C-004 は現象を
cross-target の欄に書いた。cross-target の乖離は台帳が日常的に扱う既知のクラスで、
`env.os` のように「設計上そうである」ものもある。だから条項は目立たなかった。
実体は determinism hole — 0.29.0 の粛清が対象にしたクラスそのものだった。

C-004 の EXCEPTION 節が過小評価されるのはこれで 2 度目である。条項の末尾にはこうある：

> The block was an undercount here until #915's audit: it spawns per-arm scoped
> threads on native, the same interleaving class as settle.

#915 は**範囲**の過小評価（`fan.settle` だけでなく `fan {}` も該当する）を直した。
本 ADR が直すのは**クラス**の過小評価である。

### 第 3 のオラクルは native の側にいない

`crates/almide-interp/src/eval.rs:148` — 参照インタプリタの fan は逐次である。

> Fan block: evaluate each expr SEQUENTIALLY in source order — the deterministic
> mode both backends collapse to

3-way オラクル（native / wasm / interp）のうち、**wasm と interp が逐次で一致し、
native だけが外れている**。2 対 1 であり、しかも外れている 1 つは
「どちらが正しいか」の議論を要さない — 逐次側は #1000 が定義した観測
（「リスト順に逐次評価した場合と厳密に同一」）そのものだからだ。

### 基体は誰も決めていない

なぜこうなったか。**実行基体が設計の産物ではなく実装都合の残り物だから**である。

| target | 基体 | 出どころ |
|---|---|---|
| native | OS スレッド（`std::thread::scope`） | `codegen/templates/rust.toml` `[fan_expr]` / `[fan_effect]` |
| wasm | 完全逐次（arm を inline 展開） | `crates/almide-mir/src/lower/desugar_fan.rs` |
| interp | 完全逐次 | `crates/almide-interp/src/eval.rs` |

憲章のどこにも「native は OS スレッドで、wasm は逐次にする」とは書いていない。
native が `std::thread::scope` なのは Rust にそれがあったから、wasm が逐次なのは
wasip1 にスレッドがなかったからである。**基体は憲章の空白地帯にあり、その空白が
観測に漏れた。**

漏れは 2 方向ある。下向き（基体 → 観測）が上の例外条項。上向き（基体 → 表面）も実測された：
`spec/wasm_cross/fan_race_mapper.almd` は wasm で正常終了し、**native は wall する**
（`op "Prim Handle" in main — outside the rung subset`、exit 1）。憲章 §3 の
head × form マトリクスは全セル確定と宣言しているが、native から見るとセルが空いている。

（`spec/wasm_cross/{fan_*,fuel_*}.almd` 全 23 本の両 target 比較では、この 1 本を除く
22 本が stdout・exit code とも byte 一致。上向きの漏れは 1 本、下向きの漏れは
fixture が印字する arm を 2 本置く構成を避けているため 0 本 — 避けていること自体が
条項の告白である。）

## Decision

**実行基体は観測から分離された自由変数である。基体は性能だけのノブであり、観測を
1 ビットも変えてはならない。wasm 側の並行性は WASI 0.3 の native async から取り、
共有メモリと atomics は採らない。**

### D1. Rung 1（arm 単位の出力トランザクション）は前提条件であり、改善ではない

`fan` の各 arm の stdout / stderr は arm ごとのバッファに入り、join 時に **arm 順**で
flush される。憲章 §4 柱 5 が Rung 1 として設計済みの機構だが、本 ADR はこれを
「あとで来る改善」から**基体を語る資格そのもの**へ格上げする。

理由は 2 つあり、順に重い。

1. **これは determinism hole であって、性能の話ではない**（Context）。0.29.0 が
   `fan.timeout` に下した判断と同じ判断を、同じ性質に対して下す
2. 順序が交換不能である。Rung 1 なしに基体を並列化すれば、今日「native だけが
   非決定」である乖離が「全 target 非決定」へ**悪化する**

Rung 1 が決めなければならない設計問題は 3 つある。いずれも「逐次実行と一致する」
という一本の基準で決まる。

- **stdout と stderr の相対順序**: 2 本を独立にバッファすると、arm 内での
  `println` → `eprintln` の相対順序が失われる。逐次実行では両者は fd レベルで
  混ざるので、**arm 内では 1 本のタイムライン**として記録し、flush 時に fd へ振り分ける
- **trap 時の flush**: C-200 は「trap した sibling は統一 abort を通り、in-flight の
  sibling を待たない」を pin している。逐次実行なら trap より前の arm の出力は
  出ているので、**完了済み arm のバッファを arm 順に flush してから abort する**。
  C-200 の fixture が「empty stdout」を測っているのは印字 arm がないためで、
  条項とは衝突しない（要・fixture 追加）
- **バッファの上限**: 1GB 印字する arm は 1GB バッファする。逐次実行にはないコストで
  ある。arm 順で**先頭の arm は即時 flush してよい**（前に誰もいないので逐次と一致する）
  ため、最悪ケースは「先頭以外の arm の出力総量」に落ちる。これで足りない構成が
  出たら F4

### D2. wasm の並行性は WASI 0.3 async から取る

`async func` / `stream<T>` / `future<T>`（Component Model の Canonical ABI に吸収済み、
`wasi:io` は撤廃）を基体とする。共有メモリも atomics も要らない。

### D3. wasi-threads / shared-everything-threads は採らない

前者は 2023-08 に撤回された legacy proposal で、wasmtime 47.0.2 は
`-S threads=y` を**実行時に拒否**する。後者はどの WASI host runtime にも未実装（R2）。

帰結として `crates/almide-mir/tests/deterministic_profile_test.rs` の
`FORBIDDEN = ["relaxed", "atomic", "shared"]` は**無傷で残る**。C-210 は改訂しない。

### D4. native の OS スレッドは維持する

基体差が観測に出ないなら、target ごとに最も速い基体を選んでよい。それが自由変数の
意味である。native を wasm に合わせて逐次化するのは観測のために基体を縛る行為で、
因果が逆になる。並列実行は #1000 が明示的に許した性能事項である
（「実行は並列でよいが、観測可能な振る舞いは逐次評価と厳密に同一」）。

### D5. 基体差分ゲートを新設する

印字 arm を含むプログラムを全基体（native 逐次 / native スレッド / wasm 逐次 /
wasm async / interp）で **N 回ずつ**走らせ、stdout・stderr・exit code の
一致を機械検査する。実行間の反復が要件である — 本 ADR の欠陥は 1 回の実行では
観測されず、10 回で 9 通りとして初めて見えた。

### D6. C-004 と C-006 を訂正する

Rung 1 が landing する PR で、C-004 の EXCEPTION 節を削除し（ratchet は下がる方向）、
C-006 の「the sole stdlib surface violating that property」に、
**`fan {}` / `fan.settle` の出力インターリーブが同じクラスの 2 例目として
0.24.0 から存在し、本 PR で閉じた**旨を追記する。台帳の主張は、正しくなった時点で
だけでなく、不正確だったと分かった時点でも直す。

## Rationale

### R1. 「例外条項つきの等価性」は、この repo の運用と整合しない

契約台帳は `flagged-for-revision` の ratchet をゼロに保ってきた。C-006 の文章は
それを誇っている — 「This retired the ledger's LAST flagged-for-revision entry:
the ratchet stands at zero, so the equivalence claim carries no clause for
divergences awaiting a fix.」

その同じ文が、例外を 1 つ認めている：`env.os` / `env.temp_dir` の platform-reporting
（C-189）。この carve-out は**設計上そうである**もので、直す対象ではない。

C-004 の EXCEPTION はそのどちらでもない。ratchet に載っていないが、設計上の
carve-out でもない。**分類されないまま 0.24.0 から 2 年間存在した。** 分類さえ
正しければ 0.29.0 の粛清で一緒に死んでいたはずのものである。

### R2. 「共有メモリで並列にする」は 2026 に選択肢として存在しない（実測）

concurrency-stance.md は wasm 側の逐次性を「wasm32 にスレッドはない」で根拠づけた。
この前提は 2026-08 時点で素直には成り立たない — threads proposal 自体は標準化され、
wasmtime は `-W threads` / `-W shared-everything-threads` を feature flag として受け付ける。
根拠が変質した却下は復活を疑うべきである（ADR README 規則 2）。疑って、実機で潰した。

```
$ wasmtime --version
wasmtime 47.0.2 (90fed3c6a 2026-07-21)

$ wasmtime run -S threads=y t.wasm
Error: the `-Sthreads` flag is no longer supported        ← ヘルプには載っているが拒否される

$ wasmtime run -W shared-everything-threads=y t.wasm
hi                                                        ← flag は通るが提案自体が未実装
```

`-S help` は今も `threads[=y|n] -- Enable support for WASI threading imports
(experimental). Implies preview2=false.` と表示する。ヘルプを読んだだけなら
「experimental だが使える」と結論する。**実行すると拒否される。**

一次情報も一致する。wasi-threads は 2023-08 に shared-everything-threads へ道を譲って
撤回され、preview1 しか支えられないエンジンのための legacy として残置。
shared-everything-threads は早期段階で、どの WASI host runtime にも実装がない。

**倒す相手がいなかった。** C-210 と並列性のトレードオフは 2026 の地形に存在しない。

### R3. WASI 0.3 は atomics なしに並行 I/O をくれる

WASI 0.3.0 は 2026-06-11 出荷、Component Model へ native async を導入。`wasi:io` は
撤廃され機能は Canonical ABI へ吸収、0.2 の `start-foo` / `finish-foo` / `subscribe`
の三段舞踏は消滅。対応は **Wasmtime 43+**、手元は 47.0.2。

決定的に重要なのは、これが命令セットの話ではないことだ。並行性は component 境界の
ABI にあり、線形メモリを共有しない。atomics も shared memory も emit しないので、
C-210 の deterministic-profile 適合はそのまま立つ。「決定性を売って並列性を買う」は
誤った二択だった。2026 の wasm は、売らずに買える。

### R4. 憲章 §6 の Go 敗北行は、決定性を売らずに埋まる

> | Go | 実運用 async の成熟度 | **oracle 層が未実装で、本物の並行 I/O レースが書けない** |

oracle 層に要るのは、環境の応答列 ω を宣言された入力として扱う契約クラス（B1 で
着地済み）と、**実際に I/O を重ねられる基体**である。後者が D2 で、憲章が
「時間の問題」と分類した負けの、時間の中身がこれだった。

### R5. 基体非依存性は、この言語だけが機械検査で言える

Go・Rust・JS で並列度を変えれば出力の混ざり方が変わる。欠陥ではなく、観測が
スケジュールの関数だという設計を選んだ結果である。Almide は 2 年かけて観測を
論理時間に固定した。**その投資の配当が、基体の自由である。**

ただし今日は言えない — 上の実測がその反例だからだ。D1 はこの一文を買う工事である。

## Alternatives

| 案 | 判定 | 理由 |
|---|---|---|
| wasi-threads を採用し C-210 を改訂 | **却下** | 機能が存在しない。wasmtime 47.0.2 が実行時に拒否（R2、2026-08-07 実測） |
| shared-everything-threads を待つ | **地平送り** | host runtime 未実装。実装が現れ、かつ atomics を emit せずに使える形なら D3 を再査定（F3） |
| native を逐次化して例外を消す | **却下** | 観測は揃うが基体の自由を放棄する。観測のために基体を縛るのは因果が逆（D4）。#1000 は並列実行を明示的に許している |
| EXCEPTION を C-189 型の「設計上の carve-out」として正式化する | **却下** | `env.os` の carve-out は**プラットフォームを報告する関数**という、目的が乖離そのものである surface に限定されている。出力の混ざり方は乖離を目的としない。carve-out 化は determinism hole の恒久化にすぎない |
| Rung 1 を後回しにして D2 を先に入れる | **却下** | 乖離が「native のみ」から「全 target」へ悪化する（D1） |
| 現状維持 | **却下** | C-006 の主張が不正確なまま残り、憲章 claim 2 が例外条項つきで固定される |

## Consequences

### 得るもの

- **determinism hole が閉じる。** 「観測は (プログラム, 入力) の関数である」が、
  出力の混ざり方まで含めて真になる
- C-004 の EXCEPTION がゼロになり、等価性の主張から例外条項が消える（D6）
- 憲章 claim 2（native ⇄ wasm 観測等価）が「例外つき」から無条件へ
- 新しい 6 文目：**基体を切り替えても観測は変わらない（機械検査つき）**
- `fan_race_mapper` の native wall のような「基体が表面を決める」病が、
  基体差分ゲート（D5）で構造的に検出される

### 払うもの

- **arm の出力がリアルタイムに出なくなる。** 先頭 arm は即時 flush できる（D1）が、
  2 本目以降は前の arm の完了を待つ。逐次実行と一致する挙動なので定義上は正しいが、
  進捗表示のある長時間 arm では体感が変わる（F4）
- バッファのメモリコスト。最悪ケースは「先頭以外の arm の出力総量」（D1）
- **wasm backend の component 化が前提工事になる。** 今日は
  `wasi_snapshot_preview1` の core module を出している（`render_wasm_p3.rs`）。
  WASI 0.3 async は Component Model の上にあり、この移行は小さくない。
  本 ADR は D2 を**方向の決定**として採り、着手時期は execution-inception.md に委ねる
- ゲートの実行時間が増える。D5 は全基体 × N 回の直積を回す

## Falsifier

- **F1**: 基体を切り替えると観測が変わる構成が出て、Rung 1 で塞げない場合 —
  その基体を撤去する。D1 が守れないなら D2 も D4 も撤回し、全 target 逐次へ倒す
  （観測を守るために基体の自由を捨てる。逆順にはしない）
- **F2**: WASI 0.3 async の実測オーバーヘッドが逐次を上回る場合 — D2 のみ取り下げ、
  wasm 基体は逐次で据え置き。D1 / D3 / D5 は基体の数によらず残る
- **F3**: shared-everything-threads が host runtime に実装され、C-210 の FORBIDDEN を
  倒さずに使える形が現れた場合 — D3 を再査定。倒す必要があるなら据え置き
- **F4**: arm 単位バッファが長時間 arm の進捗ストリーミングを壊すという実使用の
  苦情が出た場合 — **opt-out は採らない**（観測が基体依存へ戻り、この ADR を無効化する）。
  flush 境界の粒度を再設計するか、進捗表示を `fan` の外の構文で受ける
- **F5**: D5 のゲートが N 回反復しても本欠陥を再現できない場合（例: CI が 1 コア）—
  ゲートは通るのに hole は残る。反復回数ではなく**基体の直接指定**
  （逐次基体を強制するフラグ）で検査する形へ設計変更する

## References

- [WASI 0.3 · WASI.dev](https://wasi.dev/releases/wasi-p3) — 0.3.0 の内容（native async、`async func` / `stream<T>` / `future<T>`）
- [Bytecode Alliance — WASI 0.3 Launched](https://bytecodealliance.org/articles/WASI-0.3) — 2026-06-11 出荷、Wasmtime 43+、`wasi:io` 撤廃
- [WebAssembly/wasi-threads](https://github.com/WebAssembly/wasi-threads) — legacy proposal、2023-08 撤回
- [WebAssembly/shared-everything-threads](https://github.com/WebAssembly/shared-everything-threads) — 後継 draft、host runtime 未実装
- [Roadmap · WASI.dev](https://wasi.dev/roadmap) — 0.3.x release train と WASI 1.0（2026 末〜2027 初）
- 本 repo（2026-08-07 実測、almide 0.56.0 / wasmtime 47.0.2 / 14 コア arm64 macOS）:
  4-arm `fan {}` の native 10 回 9 通り × wasm 3 回 1 通り、
  `spec/wasm_cross/{fan_*,fuel_*}.almd` 23 本の両 target 比較、
  `codegen/templates/rust.toml` `[fan_expr]`、`crates/almide-interp/src/eval.rs:148`、
  `crates/almide-mir/src/render_wasm_p3.rs`
