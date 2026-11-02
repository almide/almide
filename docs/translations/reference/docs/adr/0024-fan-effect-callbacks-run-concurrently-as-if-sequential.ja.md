<a id="adr-0024-fan-runs-effect-callbacks-concurrently-on-every-target-as-if-sequential--fanmap-runs-every-element-the-compiler-picks-the-substrate-and-no-concurrency-concept-reaches-the-writer"></a>

# ADR-0024: `fan` は全ターゲットで effect コールバックを逐次実行と同じ観測を保って並行実行する — `fan.map` は全要素を実行し、基盤はコンパイラが選び、書き手に並行処理の概念を持ち込まない

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0024-fan-effect-callbacks-run-concurrently-as-if-sequential.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0024-fan-effect-callbacks-run-concurrently-as-if-sequential.zh-CN.md)

> 非規範的な歴史読者スナップショット。変更していない固定原本が正本。自動同期・独立ベンチマーク・現行実装保証ではない。採択された設計と日付付き部分実装を区別し、監査時点でD4とeffectコールバックのスレッド化は未実装と明記する。観測はstdout/stderr/exit/値であり、外界への全effectの一致ではない。trap残余と環境の要求順序の限定を保持する。他言語比較と時間値は原文当時の主張で、新しい測定ではない。後のfan選択の決定が旧キャンセル/any規則を置き換える場合があるが、この歴史的決定を黙って統一しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md)

- **状態**: 採択済み。メンテナは2026-09-30に未決の2件を採択した。(1) 外部システムがプログラムの要求を受け取る順序は、プログラムの観測ではなく環境の応答（ω）の一部である。(2) `fan.map` は全要素を実行し、結果はリスト順で最初の `Err` になる。採択時点ではいずれも未実装だった。2026-10-17時点（docs/design/fan-semantics-audit.md、固定0746eb689）ではD1、D5、D6が実装済み。2026-10-18（#3780）時点ではD3が `fan { }` ブロックに実装された。stdlibは順序付き集合に `@ordered` を付ける（`env.set` も初期一覧に加わる。`kv` と同じくプロセス内状態である）。armの実行可能クロージャがそのいずれかに到達するブロックは、nativeでもarm順に実行する（`mark_inline_fans`、spec/wasm_cross/fan_ordered_arms.almd）。D4（逐次実行のnote）は未実装。同じ2026-10-18（#3779、#3784）、nativeの並列版（`fan.map`、fan内の `list.map` / `filter` / `any` / `all`）は、要素ごとでなく割り当てた要素のブロックごとに1本の出力タイムラインを保つ。ブロック内では1スレッドでリスト順に要素を実行するため、ブロックあたり1回の登録でD5の順序とD6の最小インデックスのtrapを要素単位で満たす（spec/wasm_cross/fan_list_map_trap_order.almd）。軽いmapが逐次実行より遅かった原因は要素ごとの登録だった。step 4のeffect `fan.map` / `fan.settle` のスレッド実行も未実装。effectコールバックは、唯一のeffectがネストしたfanである場合を除いて逐次実行のままである（#2594）。2026-10-05（#3383）改訂: `--host js` のasync JS hookに関するD3とD8（参考文献の前の改訂節を参照）。
- **日付**: 2026-09-30
- **範囲**: nativeとwasmでの `fan { … }`、`fan.settle { … }`、`fan.map(xs, f)`、`fan.settle(xs, f)` の実行基盤、要素失敗後の `fan.map` の意味、fanが逐次実行されるとき書き手に伝える内容、並行処理を表層に持ち込まない不変条件。`fan.any` は範囲外（§8）。
- **設計を確定する対象**: #2594（nativeで `fan.map` がeffectコールバックを逐次実行する一方、ドキュメントは異なる説明をしている）。
- **基礎**: [ADR-0011](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0011-execution-substrate-is-a-free-variable.md)（基盤は自由変数。そのD1出力トランザクションをここでstep 1とする）、[ADR-0020](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md) §3（concurrent slotと実行可能クロージャ）と§5.5（nativeスレッド、RNG再シード）、[ADR-0022](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)（純粋fnもprintできるため純粋コールバックも観測可能）、[ADR-0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md)（wasm HTTPはp3 componentの `wasi:http@0.3`）。
- **憲章**: [concurrency-stance.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md)（#1000、決定的データ並列）、[async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md) §3（head × form表）とpillar 3（キャンセルは最適化であり意味論ではない）、[execution-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/execution-inception.md)（as-if規則）。
- **影響する契約**: C-004、C-005、C-200（§9）。契約変更は先にalmide/alsへ入れる。

<a id="1-context"></a>

## 1. 背景

<a id="11-what-2594-found"></a>

### 1.1 #2594で分かったこと

ドキュメントの約束とコンパイラの動作が異なる。

- `docs/CHEATSHEET.md` は `fan.map(urls, (u) => http.get(u))!` をfan-outのイディオムとして示し、純粋コールバックは「nativeで並列」と述べる。
- `runtime/rs/src/fan.rs` はすべての**effect**コールバックを、`Rc<dyn Fn>` 上の逐次ループである `almide_rt_fan_map` へ送る。`Send` スカラー上の純粋lambdaリテラルだけが `almide_rt_fan_map_par` へ到達する（#2044）。そこのコメントは理由を挙げる。`Rc<dyn Fn>` は `Send` でも `Sync` でもない。

その理由はクロージャ表現に属し、言語には属さない。ブロック形式がそれを実証している。2026-09-29、3つの `fan { … }` armがそれぞれ `String` と `List[String]` を500 ms sleepするeffect fnへ渡す試作プログラムは、**505 ms**で終了した。生成Rustはインラインの `std::thread::scope` で、armは所有キャプチャを持つ `move` クロージャである。`fan.map` に渡すlambdaリテラルも同じ方法でloweringできる。

同じ測定でADR-0011が記録した穴も分かった。3つのarmの `println` は `arm 1, arm 3, arm 2` の順に出力され、実行ごとに順序が変わった。ADR-0011 D1（armごとの出力トランザクション）はまだ入っていない。

<a id="12-the-substrate-today-per-head-and-target"></a>

### 1.2 当時の基盤: headとターゲット別

| 表層 | native | wasm（p3 component） | interp |
|---|---|---|---|
| `fan { a; b }` | armごとにスレッド、全arm実行、arm順の最初の `Err` | 逐次 | 逐次 |
| `fan.settle { a; b }` | 逐次（1 sのarm3本で3015 ms、#2594コメント） | 逐次 | 逐次 |
| `fan.map(xs, f)`、純粋スカラーlambda | チャンク単位のスレッド（#2044） | 逐次 | 逐次 |
| `fan.map(xs, f)`、effectコールバック | 逐次 | 逐次。本文がちょうど1つの `fs.read_text` ならasync subtaskとして先読みしリスト順に待つ（b973a1dd4） | 逐次 |
| `fan.settle(xs, f)` | 逐次 | 逐次 | 逐次 |

誰かがこの行列を選んだわけではない。各セルは表現上実装しやすかったものにすぎない。ADR-0011が「基盤が憲章の空白に住んでいる」と呼んだ状態である。

<a id="13-the-block-and-mapper-forms-disagree-after-an-err"></a>

### 1.3 `Err` 後にブロック形式とmapper形式が食い違う

憲章の表は、無印headについて両形式に同じ規則を与える。「all — 全部。リスト順で最初の `Err`」。実装は分かれている。

- `fan { a; b }` は**全**armを実行してから最初の `Err` を報告する（`crates/almide-wasm/src/fan.rs` のヘッダー:「全arm実行後、最初のerrでabort」。nativeの `thread::scope` は全armをjoinする）。
- `fan.map` は最初の `Err` で**停止**する（`stdlib/fan_map.almd`:「最初のerrで短絡」。nativeの `collect()` も停止する）。

短絡は、投機なしでは並行処理で再現できない規則である。並行mapは、要素 `k` が失敗したとき既に要素 `k+1` を開始している。印字出力は破棄できても、POSTは送信済みである。

<a id="14-what-the-peers-do"></a>

### 1.4 他の言語・ライブラリの動作

| | 結果の順序 | 失敗時 | 失敗後の兄弟 | 並行数の上限 | 全結果を収集する形式 |
|---|---|---|---|---|---|
| JS `Promise.all` | 入力順 | **時間順で**最初の失敗 | 続行 | 組み込みなし | `Promise.allSettled` |
| Go `errgroup` | 自分で収集 | 時間順で最初の失敗 | contextをキャンセル（協調的） | `SetLimit(n)` | 自分で書く |
| Python `asyncio.gather` / `TaskGroup` | 入力順 | 最初の例外 | gather: 続行 / TaskGroup: キャンセル | 組み込みなし | `return_exceptions=True` |
| Kotlin `coroutineScope` + `awaitAll` | 入力順 | 最初の例外 | キャンセル | `Semaphore` | `supervisorScope` |
| Swift `TaskGroup`、Trio nursery、Java `StructuredTaskScope` | 完了順 | 最初の例外 | キャンセル | 組み込みなし | ライブラリごと |
| Haskell `mapConcurrently` | 入力順 | 最初の例外 | キャンセル | 組み込みなし | — |
| Rust `try_join_all` / `buffered(n)` | 入力順 | 最初の `Err` | dropでキャンセル | `buffered(n)` | `join_all` |

どれも結果の順序を約束する。effectの順序は約束しない。いずれも、どの失敗が採用されるかはタイミングに依存する。並列度を変えるとプログラムの印字内容が変わる。

各エコシステムは、Almideが支払ってはならない代償も支払った（§3）。

- **Rust。** `async fn` と `fn` は2つの色となり、stdlibとエコシステムが分断された。`Send`、`'static`、`Pin` 境界が書き手へ露出し、しばしば編集行から遠い型のエラーになる（「future is not `Send`」）。キャンセルは `.await` での `drop` なので、キャンセル安全性は書き手の仕事になる。executorはライブラリの選択なのでエコシステムが分岐した（tokio、async-std）。
- **Go。** goroutineは並行処理を容易に書けるようにし、データ競合も容易に出荷できるようにした。書き手が対策しなければgoroutineは呼び出し元より長生きする。
- **JS/TS。** 誰もawaitしないpromiseは黙って失敗する。ブラウザは1スレッドである。
- **Swift 6。** 厳格な `Sendable` 検査でデータ競合はコンパイルエラーになったが、移行では書き手が変えたいコードでなく型についての大量のエラーに直面した。
- **C#。** `async`/`await` は読みやすいが、競合は依然コンパイルを通る。

<a id="2-decision"></a>

## 2. 決定

**`fan` は全ターゲットで並行実行し、その観測はリスト順の逐次評価と完全に同じにする。`fan.map` は全要素を実行してリスト順で最初の `Err` を報告する。基盤はコンパイラが選ぶ。書き手に並行処理の概念、そのエラー、その調整つまみを見せない。**

<a id="d1-the-observation-is-the-sequential-run-every-element-evaluation"></a>

### D1. 観測は全要素を逐次評価したもの

`fan { … }`、`fan.settle { … }`、`fan.map(xs, f)`、`fan.settle(xs, f)` の参照上の意味は、リスト順で要素を1つずつ、**すべて**評価してから、

- `fan { … }` / `fan.map`: どれかが `Err` を返せば最小インデックスの `Err` を結果とし、そうでなければリスト順の結果とする。
- `fan.settle`: 全要素の `Result` をリスト順にする。

これは `fan.map` の変更であり、最初の `Err` で停止しなくなる（§1.3、メンテナの決定2）。憲章の表が一貫して述べていた通り、ブロックとmapper形式が同じ規則になる。

「観測」は#1000の定義、すなわちstdout、stderr、exit code、戻り値である。

<a id="d2-the-environments-order-is-ω"></a>

### D2. 環境の順序はω

2つの要素が外部システムへ要求を送る場合、そのシステムが受け取る順序は環境の応答ωの一部である。async-inceptionのoracle層（B1）が既に宣言された入力として扱うクラスである（メンテナの決定1）。プログラムから見える応答は、他のI/Oと同じく入力である。

<a id="d3-the-compiler-chooses-the-substrate-per-fan-by-one-question"></a>

### D3. コンパイラはfanごとに1つの問いで基盤を選ぶ

実行可能クロージャが**順序付き操作**に到達しない限りfanは**並行**実行する。到達するなら**逐次**実行する。第3のクラスはない。

- 実行可能クロージャはADR-0020 §3.2の `X(E)` をそのまま使う。4つの表層のfn値引数はすべて、既にconcurrent slotである（ADR-0020 §3.1）。
- **順序付き操作**とは、プロセス内状態へのeffectを兄弟要素が読み取れ、その結果が順序に依存しうるstdlib関数である。stdlibは引数なし属性を付ける（仮称 `@ordered`。ADR-0020が `@concurrent` を残したように、綴りは実装PRが決める）。初期集合:
  - 作成、書き込み、追記、コピー、改名、削除するすべての `fs` 関数（`fs.write_*`、`fs.append_*`、`fs.copy`、`fs.rename`、`fs.remove*`、`fs.mkdir_p`、`fs.create_temp_*`）。
  - すべての `kv` 関数（ADR-0020 §4。読み取りは兄弟の書き込みを観測できる）。
  - stdinから読むすべての `io` 操作（どの要素がどの行を得るかは順序に依存する）。
  - `process` の子プロセス起動（子は上記すべてを行いうる）。
- 順序付きではないもの: 出力（`print`、`println`、`eprintln`。D5で扱う）、`fs` 読み取り、`http` クライアント呼び出し（ω、D2）、時計の読み取り（既に環境由来）、`random`（ADR-0020 §5.5。乱数には再現性の約束がないため、スレッドごとに異なるseedだけが必要）。

どれかの要素が書くときだけ読み取りと書き込みが競合する。書く要素があればfan全体を逐次実行する。規則はfanあたり1ビットで、全ターゲットで同じビットとなる。

<a id="d4-sequential-is-a-note-never-an-error"></a>

### D4. 逐次実行はnoteでありエラーにはしない

逐次基盤を選んでも変わるのは速度であり観測ではない。したがってコンパイラはそれを理由にプログラムを拒否しない。書き手の語彙で根拠となる経路を示した**note**を出す（問題はないためwarningではない）。

```
note: this fan runs its elements one at a time
  --> src/sync.almd:12:3
   |
12 |   fan.map(items, (it) => save(it))
   |   ^^^^^^^ `save` writes a file (fs.write_text, src/store.almd:8),
   |           and another element could read it
```

リスト内のクロージャ値など、コンパイラが別スレッドへ移せないものも同様である。noteはその値を名指しし、`Send`、`Sync`、スレッド、ライフタイムを名指ししない。

<a id="d5-output-is-transactional-per-element-adr-0011-d1-promoted-to-a-precondition"></a>

### D5. 出力は要素ごとのトランザクション（ADR-0011 D1を前提条件へ昇格）

各要素のstdoutとstderrを、要素ごとに1本のタイムラインへ入れる。先行要素が終わったらリスト順にflushする。要素0は先行要素がないため直接streamする。ネストしたfanの要素は、外側要素のタイムラインへflushする。stdoutとstderrを要素ごとの同じタイムラインに保ち、flush時に2つのfdへ分ける（ADR-0011 D1）。printできる純粋コールバックも対象である（ADR-0022）。

これが入るまで、どの表層でも並行基盤を有効にしてはならない（ADR-0011:「D2拒否後のRung 1」）。

<a id="d6-failures-err-waits-for-everyone-a-trap-waits-for-everyone-below-it"></a>

### D6. 失敗: `Err` は全員を待ち、trapは自分より小さいインデックスを全員待つ

- **`Err`。** D1により全要素を実行するので、並行基盤は全要素をjoinし最小インデックスの `Err` を選ぶ。投機はない。
- **trap**（ゼロ除算、overflow、範囲外、`panic`、失敗した `assert`）。逐次実行で要素 `k` がtrapするとは、`0..k` が実行され、`k` より後は実行されないことを意味する。並行基盤は要素の開始を止め、`k` より前の全要素を待ち、最小インデックスのtrapを採用する。その要素より前のタイムラインを順にflushし、trapした要素の部分タイムラインをflushしてから、統一main-error経路でabortする（C-005 / C-200の形は不変）。
- **残余。** 採用されたtrapより後で既に開始済みの要素は、逐次評価では送らない要求を送っている場合がある。出力は破棄するので観測は一致する。しかし外界へのeffectは一致しない。trapはバグ種別の終了であり、この差をなくすには `k` 終了前に `k+1` を決して開始しない必要がある。それは逐次実行である。この残余は隠さず本書とC-200に記録する。

<a id="d7-no-concurrency-knob-in-the-surface"></a>

### D7. 表層には並行数のつまみを設けない

`limit:` 引数は設けない。async-inceptionの却下表は既に「`fan.map(xs, limit: n, f)` — 必要が実証されるまで追加しない」と述べる。実行中要素数はruntime自身が制限する。初期上限はADR-0020の `max_in_flight` 既定値64で、プロセスに数値を1つだけ持たせる。`ALMIDE_FAN_SEQUENTIAL=1` はablation用スイッチとして残す。測定用環境変数であり言語機能ではない。
（2026-10-17監査注記: スイッチが効くのは `fan.map` と `fan { list.map }` の並列版だけ。nativeの `fan { }` ブロックはこの設定でもスレッド実行のまま。）

動機となった検索ツールで64では吸収できないproviderのrate limitが示されたら、それが表の求める実証となり、新しいADRで綴りを決める。

<a id="d8-wasm-gets-its-concurrency-from-wasi-03-async-below-the-ir"></a>

### D8. Wasmの並行処理はIRより下のWASI 0.3 asyncが担う

p3 componentでは、要素のhost callをasync-loweringしたsubtaskにし、waitable setで駆動する。b973a1dd4が `fs.read_text` 先読みに、ADR-0023が `wasi:http@0.3` に既に使う仕組みである。host callを複数連続で行う要素は、その間で要素自身を中断する必要がある。コンパイラが状態機械かstack switchで実現する。**その仕組みはIRより下に置き、型、シグネチャ、診断、ドキュメントに一切出さない**（N1）。

一般形が入るまではwasmのfanを逐次実行する。D1により観測は同一なので、これは速度差であり動作差ではない。共有メモリもatomicsも使わない。ADR-0011 D3とC-210の `FORBIDDEN` は維持する。

<a id="3-invariants-what-almide-does-not-repeat"></a>

## 3. 不変条件: Almideが繰り返さないこと

これらは規範的である。将来これを破る変更には、この節を置き換える新しいADRが必要となる。

| # | 不変条件 | 排除する失敗 |
|---|---|---|
| N1 | 表層に `async`、`await`、`Future`、task handle、`Pin`、executorを持ち込まない。並行処理は `fan` 式。effect軸は `fn` / `effect fn` のままで色を追加しない。 | Rustの2色と二重のエコシステム、状態機械の型への漏出 |
| N2 | 他スレッドで要素を実行できるかはコンパイラが決め、書き手向けエラーにはしない。`Send`、`Sync`、`Sendable`、`'static`、「thread」を診断に出さない。 | Rustの「future is not `Send`」、Swift 6の `Sendable` 移行のエラーの壁 |
| N3 | キャンセルは最適化であり意味論ではない（async-inception pillar 3）。開始済み要素をキャンセルする表層はない。`race` と `select` は削除したままとする（E027）。 | Rustの `.await` でのキャンセル安全性、未完了処理をdropする `select!` |
| N4 | runtimeは1つ、ターゲットごとにコンパイラが選ぶ。書き手はexecutorを選ばない。 | tokio / async-stdの分岐 |
| N5 | `fan` 式が値を出すとき全要素は終了している。detachしない。spawnはない。 | Goのgoroutineリーク、JSのawaitされないpromise |
| N6 | 要素から `var` に到達できない（ADR-0020 §3、E008）。 | GoとC#のデータ競合 |
| N7 | 基盤を変えても観測は変わらず、gateで検査する（ADR-0011 D5）。反復実行と基盤強制を行う（ADR-0011 F5）。 | 他の各例: 出力がscheduleの関数になる |
| N8 | 実証された必要が生じるまで並行度の表層つまみを設けず（D7）、その場合も綴りは1つ。 | `buffered(n)` / `SetLimit` / p-limitなど、各エコシステム独自のもの |

<a id="4-rationale"></a>

## 4. 根拠

1. **他の例の失敗には、書き手がscheduleを推論しなければならなくなった、という共通原因がある。** Rustは型で表せるようにし、Goは安価にし、JSは失敗するまで見えなくした。Almideの憲章は既に観測を逐次評価へ固定した（#1000）ので、scheduleの漏出先はない。不足していたのは約束の後半、すなわちコンパイラが自由にscheduleでき、実際に行うことであった。
2. **全要素実行だからD3を1ビットにできる。** 短絡では並行処理に投機が必要で、投機が安全なのは元に戻せるeffectだけである。これではコールグラフから推論した読み取り、不可逆の書き込み、共有状態にコールバックを分けることになる。深いhelperの1編集がfanのクラスを黙って変えてしまう。それはRustのauto-trait推論で、エラーの代わりに性能の崖がある形である。短絡がなければ残る問いは「この要素のプロセス状態へのeffectを兄弟が観測できるか」だけになる。
3. **ブロック形式は既に全armを実行する。** D1で `fan.map` を `fan { }` と憲章の表に合わせる。新たな意味は導入しない。
4. **基盤は自由変数である（ADR-0011）。** エラーでなくnoteにすることは直接導ける。コンパイラが並行化できなくてもプログラムは正しく、遅いだけである。拒否すれば正しさに利得なくSwift 6と同じ結末となる。
5. **動機のプログラムに合う。** 検索集約は `fan.settle(providers, (p) => query(p, q))` で複数providerを呼ぶ。`http` 呼び出しは順序付きではないためnativeでもp3 componentでも並行実行する。投機的に開始する要素はないためPOSTで問い合わせるproviderでも同じである。

<a id="5-alternatives"></a>

## 5. 代替案

| 案 | 判断 | 理由 |
|---|---|---|
| 結果順だけ約束しeffect順は変動させる（このADRの議論の初稿） | **却下** | #1000とADR-0011 D1に反する。観測がscheduleに依存し、ADR-0011が塞ぐ決定性の穴になる |
| 短絡を保ち「元に戻せる」コールバック（読み取り、GET、出力）だけ投機する | **却下** | コールグラフで3クラスを推論し、1編集でfanのクラスが見えないまま変わる（§4.2） |
| コンパイラが並行実行できないコールバックを拒否する（warningを経てコンパイルエラーへ） | **却下** | プログラムは正しい。エラーはRustの `Send` エラーとSwift 6の移行の壁になる（N2） |
| 黙って逐次実行へ戻す | **却下** | 正しいが診断できない。D4のnoteなら妨げずに理由を見えるようにできる |
| 今 `fan.map(xs, f, limit: n)` を追加する | **延期** | 憲章は必要の実証を要求する（D7） |
| I/O中心の並行処理のため `async fn` / `await` を加える | **却下** | N1。何を重ねるかは既に `fan` で表せる |
| 全値を `Send`（Rc→Arc）にして全コールバックを移動可能にする | **却下** | ADR-0020 §5.5でruntime全体のコストとして却下済み。所有キャプチャと順序付き操作で必要を満たす |
| effectコールバックの `fan.map` を逐次のままにしドキュメントを直す | **却下** | 動機のプログラムに並行処理がないままで、基盤も偶然で選ばれたままになる（§1.2） |

<a id="6-consequences"></a>

## 6. 帰結

<a id="gains"></a>

### 利点

- `fan.map(urls, (u) => http.get(u))` が、cheatsheetの既存の主張通りnativeで並行実行され、D8が入ればp3 componentでも並行実行される。
- `fan { }` の出力混在がなくなる（D5）。ADR-0011の穴を塞ぎC-004のEXCEPTION節を削除する。
- `Err` 後の `fan { }` と `fan.map` に1つの規則を設ける。
- N1–N8を明文化するため、後の「`await` を加える」「limitつまみを加える」提案はこれらに反論する必要がある。

<a id="costs"></a>

### コスト

- **動作変更。** `fan.map` 要素が失敗したプログラムも後続要素を実行するようになる。後続の出力はabort前に順序通りflushされ、effectも生じる。意味の変更なのでdialect epochとなる（§9）。
- **`Err` 後の無駄な処理。** 1 000 URLのmapで最初の要素が失敗しても残り999を取得する。最初の失敗で止めたい書き手は、そう明示する `!` 付き `for` ループを書く。
- **バッファリング。** 要素の出力を先行要素の終了まで保持する。最悪は最初以外の全要素の出力総量である（ADR-0011 D1）。
- **trapの残余**（D6）。
- D8の一般形が入るまで**wasmの速度は遅れる**。

<a id="7-falsifiers"></a>

## 7. 反証条件

- **F1。** 基盤間で観測が異なりD5/D6で閉じられないプログラム。その表層では当該基盤を削除する（ADR-0011 F1: 基盤を諦め、観測は諦めない）。
- **F2。** 全要素実行のコストが仮定でなく実際のプログラムに現れる。たとえば検索ツールやdojo corpusで、`for` ループが自然でない場面で問題になる。そのとき§4.2の3クラスを戻さない設計で短絡を再検討する。
- **F3。** D3の順序付き集合が誤っている。集合外のstdlib関数が兄弟に順序を観測させる（見落としたクラス）、または集合内の関数が実プログラムで理由なく逐次化を強いる。その集合を修正しD5形式のgateへその事例を加える。
- **F4。** 書き手に見えるlimitが必要になる（D7）。
- **F5。** D8の仕組みをIRより下に保てない。たとえばcomponent modelがユーザー関数へasyncシグネチャを強いる。その形ではwasmを逐次のままにする。N1は緩めない。

<a id="8-out-of-scope"></a>

## 8. 範囲外

- **`fan.any`。** 早期打ち切りがその意味（「最小インデックスの成功、逐次fallback」）である。並行化には§4.2で `fan.map` に対して却下した投機が必要になる。逐次基盤を維持する。p3 componentの既存の読み取り専用先読みは、放棄するのが読み取りだけなので残す。別の決定で再検討してもよい。
- `fan.race` / `fan.bounded` / `fan.timeout`: async-inceptionが管轄する。
- `http.serve` の並行処理: ADR-0020 §5.5。

<a id="9-contracts-epoch-plan"></a>

## 9. 契約、epoch、計画

**契約変更**（先にalmide/als）:

- **C-004。** 「`fan.map` は要素fnをリスト順に逐次実行する」を「`fan.map` と `fan.settle` は基盤にかかわらず全要素のリスト順逐次評価として振る舞う」に変更する。D5を入れるPRでEXCEPTION節を削除する（ADR-0011 D6）。
- **C-005。** 「最初にErrを返す要素fn」を限定する。全要素を実行し、表面化するのは最小インデックスの `Err` とする。
- **C-200。** 「実行中の兄弟を待たない」をD6へ変更する。trapした要素より前の兄弟を待ち、残余を明記する。

**Dialect epoch。** 台帳は「構成要素の意味が変わった」を数える（`proofs/dialect-epochs.toml`）。D1が該当するためADR-0020のepoch 6の次を使う。`breaks` 行は「`fan.map` は1つが `Err` を返しても全要素を実行する。最初の失敗で止めるには `!` 付き `for` ループを使う」。`spec/`、`examples/`、dojo corpusへの移行影響（`fan.map` の失敗要素の後にprint/writeする要素を持つプログラム）はepoch確定前のstep 3で測定する。

**計画。**

| Step | 作業 | 依存 |
|---|---|---|
| 0 | almide/alsでC-004 / C-005 / C-200の文言変更 | — |
| 1 | nativeの `fan { }`、`fan.settle { }`、`fan_map_par` にD5を適用し、C-004のEXCEPTIONを削除 | 0 |
| 2 | D3集合の `@ordered` 属性、ADR-0020の `X(E)` から算出する基盤ビット（`fan { }` は2026-10-18、#3780で完了）、D4 note（未完） | ADR-0020 step 1（#2697） |
| 3 | 全経路のD1: `stdlib/fan_map.almd`、wasm emitter、native、`almide-interp`、参照評価器（ADR-0015）、影響測定、epoch | 0 |
| 4 | native: `fan.map` / `fan.settle` のlambdaリテラル/named-fnコールバックをブロックarm同様の所有 `move` クロージャへlowering、`fan.settle { }` のスレッド実行、実行中上限（D7）、スレッドごとのRNG seed（ADR-0020 §5.5） | 1, 2, 3 |
| 5 | ADR-0011 D5 gateを4表層へ拡張し、反復と基盤強制で実行 | 4 |
| 6 | wasm p3: 単一呼び出し要素のhost callをasync subtask化（b973a1dd4を `http.*` へ一般化）、次に複数呼び出し要素（D8） | 1, 3, ADR-0023 |
| 7 | ドキュメント: CHEATSHEET fan節、`docs/specs/als/runtime.md` ALS-R3、LLM向けドキュメント | 4 |

<a id="amendment-2026-10-05-async-js-hooks-on---host-js-3383"></a>

## 2026-10-05改訂: `--host js` のasync JS hook（#3383）

#3383でメンテナが裁定し採択した（最初のsliceのみ）。#3371は「このJS hookはPromiseで応答する」をexternに載せた（`@extern(wasm, "js", NAME, returns: promise)`）。この改訂は、そうしたhook上の `fan` がJS hostで何を意味するかを定める。

**D3改訂。** `returns: promise` 付きexternは、プロセス外のシステム（Workers KV、`fetch`、Cloud Storage）への要求である。受信順序は環境のω（D2）であり、D3が既に `http` クライアント呼び出しへ与えたクラスである。したがって `returns: promise` externは順序付き操作**ではなく**、fanを逐次化しない。同期externは従来通り、一切重ねて実行しない。

**D8改訂。** `--host js` では、p3 componentのWASI 0.3 asyncと同じ位置づけでJSPI（`WebAssembly.Suspending` / `WebAssembly.promising`）を基盤とする。全要素が既に持つ値に対しasync-hook呼び出しを**1回**行うfanは、p3先読み同様にloweringする（`fan.rs`、host op 40 start / 41 await / 42 abandon）。fanが到達するhookごとにglueが3つのimportを生成する。

- `start:NAME(args) -> slot`: 通常のimport。hookを呼び、Promiseをslotに保持する。
- `wait()`: **唯一の**Suspending import。開始済みの全slotを待ち、それぞれを値またはrejectとして確定させる。
- `take:NAME(slot) -> value`: 通常のimport。確定値をhook自身の戻り値へmarshalして返す。rejectはfallible externではerrとなり、infallible externでは直接呼び出しと同様にinstanceを放棄する（#3356）。

moduleは全要素をarm順に開始し、1回中断し、arm順に値を取る。全要素を実行し（D1）、arm順で最初の `Err` が結果になる（D6）ので観測は逐次loweringと同じ（N7）。`fan.any` は最初のokまで取る。取らなかったslotも `wait` で待機済み（N5）で、未読のままdropする。このsliceは `fan.map`、`fan.any`、`fan { }` ブロックである。JS hostの他のfanはD4 noteの下で逐次のまま: 引数を計算する要素、同期hookを呼ぶ要素、複数呼び出しを連続する要素。複数呼び出しにはその間の要素の中断が必要で、D8本体の状態機械またはstack switchとなる。

型、シグネチャ、診断、LLM向けドキュメントには何も露出しない（N1）。`wait` へ到達するexportには、直接async呼び出しのときと同じく `promising` 経由で入る（#3353）。moduleのbyteが逐次loweringと異なるのは、そうしたfanを持つプログラムを `--host js` でbuildするときだけで、他のbuildはすべてbyte同一である。`ALMIDE_FAN_SEQUENTIAL=1`（D7）は逐次loweringを強制する。gate `spec/wasm_host_js/fan_async_overlap` は両loweringをbuild/実行して同じ出力を要求し、重ねた側で全hookが同時に実行中になることをassertする（issueの測定は50 msの読み取り10回で510 ms → 50 ms）。

<a id="references"></a>

## 参考文献

- #1000 / [concurrency-stance.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md)、[async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md) §3とpillar 3、[execution-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/execution-inception.md)
- 本リポジトリ: `runtime/rs/src/fan.rs`、`stdlib/fan_map.almd`、`crates/almide-wasm/src/fan.rs`、commit b973a1dd4、`docs/CHEATSHEET.md` fan節、`docs/contracts/contracts.toml` C-004 / C-005 / C-200、`proofs/dialect-epochs.toml`。2026-09-29の試作測定（heap引数を持つ3 armの `fan { }` が505 ms、arm間の `println` 混在）
- [MDN: Promise.all](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/all)、[Promise.allSettled](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/allSettled)
- [Go: errgroup](https://pkg.go.dev/golang.org/x/sync/errgroup)、[Go: race detector](https://go.dev/doc/articles/race_detector)
- [Python: asyncio task groups and gather](https://docs.python.org/3/library/asyncio-task.html)
- [Kotlin: composing suspending functions / structured concurrency](https://kotlinlang.org/docs/composing-suspending-functions.html)
- [Swift: TaskGroup](https://developer.apple.com/documentation/swift/taskgroup)、[Swift 6 migration guide](https://www.swift.org/migration/documentation/migrationguide/)
- [Haskell: Control.Concurrent.Async](https://hackage.haskell.org/package/async/docs/Control-Concurrent-Async.html)
- [Rust: tokio::select! and cancellation safety](https://docs.rs/tokio/latest/tokio/macro.select.html)
- [N. J. Smith, Notes on structured concurrency](https://vorpus.org/blog/notes-on-structured-concurrency-or-go-statement-considered-harmful/)
- [B. Nystrom, What color is your function?](https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/)
- [WASI 0.3](https://wasi.dev/releases/wasi-p3)
