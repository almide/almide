<a id="fan-against-js-rust-python-and-go-what-each-one-selects-waits-for-and-stops"></a>

# Fan と JS・Rust・Python・Go：それぞれ何を選び、待ち、止めるか

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-cross-language.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](fan-cross-language.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](fan-cross-language.zh-CN.md)

> 固定原文の非規範的な歴史読者スナップショット。現行実装の手引きでも独立した言語比較でもない。変更していない原本が正本。自動同期、新たなテスト・ベンチマーク・ソース検証は行っていない。監査自身の証拠固定点は0746eb689（10月17日）で、10月18日の状態追記があり、以下の読者版原文固定点とは別である。修正済み issue、残る知見、決定済みだが導入中の Duration キャンセルを各時点の文脈のまま保持する。原文の pure 分類でも println は許され、観測可能な出力がない保証ではない。他言語の主張、「唯一」の主張、select_ok／破棄の説明、絶対的な決定性の記述は歴史的原文主張であり、今回独立に再確立していない。コード・識別子・参照は原文どおり。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-cross-language.md)

> 最終更新：2026-10-18（Almide 列は採択された ADR-0028 を反映。K1 は `??` / `list.find_map`、K4 は `fan.min(compute…)`、K2 は `fan.min(duration…)` で、決定済み、#3788で導入中）。監査証拠は0746eb689に固定（Almide の各セルの根拠は [fan-semantics-audit.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-semantics-audit.md) を参照）。他言語の意味論は名前からでなく参照文書から得た（リンクは末尾）。

この文書は操作を名前で対応付けない。Almide の `race` は JS の `race` ではなく、Almide の `any` は JS の `any` でもない。各操作を3つの問いで説明する。

- **選ぶもの**：どの結果を返すか。
- **待つもの**：返るまでに何が完了する必要があるか。
- **止めるもの**：何の実行を、どのように阻止するか。

2種類の停止を混同してはならない。

- **破棄**：ランタイムが所有する計算の駆動をやめる。例えば poll されなくなる Rust future、取り出されない Rayon split、開始されない Almide 要素。
- **外部作業のキャンセル**：ランタイム外ですでに始まったものに停止を伝える。例えばすでに送信された HTTP リクエスト、子プロセス、発行済み書き込み。以下の全言語でこれは*協調的*である。Go の `context`、JS の `AbortSignal`、Python の `CancelledError` は、それらを確認するコードにしか届かない。future を破棄しても送信済みリクエストは取り消せない。

現在の Almide には外部キャンセルがまったくない。停止は「要素を開始しない」と「課金箇所で pure 本体を打ち切る」（fan.bounded / fan.timeout / fan.min(compute…) の予算）だけである。これらの本体は pure と検査される（E006）ため、打ち切る本体は外部 effect を持てない。決定済みの `fan.min(duration…)`（#3788）は、負けた分岐を次の課金箇所または I/O 境界で協調的にキャンセルし、形式全体が戻る前にそれらの終了を待つ。Haskell の `race` や Python の `TaskGroup` と同じ構造化された種類である。

<a id="the-five-selection-kinds"></a>

## 5種類の選択

| # | 種類 | 決定的か |
|---|---|---|
| K1 | **入力順**で最初の成功 | はい |
| K2 | **実時間**で最初の成功 | いいえ |
| K3 | 実時間で最初の**完了**（成功・失敗を問わない） | いいえ |
| K4 | **論理コストが最小**の成功（同値なら入力順） | コストモデルが与えられれば、はい |
| K5 | **すべて収集**（全結果、または全成功／エラー） | 値については、はい。エラーの選び方は言語により異なる |

<a id="mapping"></a>

## 対応表

| 種類 | Almide | JS | Rust | Python asyncio | Go |
|---|---|---|---|---|---|
| K1 | `list.find_map(xs, (x) => f(x)?)`、`a() ?? b()` — 逐次であり fan 形式ではない | — | `Iterator::find_map`；Rayon `find_first`（pure な述語） | — | — |
| K2 | `fan.min(duration.ms(n)) { }` / `(duration…, xs, f)` — 決定済み、#3788 | `Promise.any` | `futures::future::select_ok`；Rayon `find_any` | `as_completed` ループ / 成功を確認する `wait(FIRST_COMPLETED)` | 手書き：channel と `context` cancel |
| K3 | —（失敗した分岐はスキップし、選ばない。K2参照） | `Promise.race` | `futures::select!` / `future::select` / `select_all` | `asyncio.wait(return_when=FIRST_COMPLETED)` | 共有 channel からの最初の受信 |
| K4 | `fan.min(compute.us(n)) { }`、`fan.min(compute.us(n), xs, f)` — clock 引数は必須 | — | — | — | — |
| K5 全成功またはエラー | `fan.map(xs, f)`、`fan { }` | `Promise.all` | Result に対する `join_all` / `try_join_all`；Rayon `collect::<Result<_,_>>()` | `gather()`、`TaskGroup` | `errgroup.Group` |
| K5 全結果 | `fan.settle(xs, f)`、`fan.settle { }` | `Promise.allSettled` | `join_all` | `gather(return_exceptions=True)` | インデックス別エラースロットを持つ `errgroup` |

衝突した2つの名前が落とし穴だった。**JS の `race` は K3 だが Almide の `race` は K4 だった。JS の `any` は K2 だが Almide の `any` は K1 だった。** JS の意味を持ち込む読者は、壁時計順序がリスト順序やコスト順序と異なる入力で違う結果を得る。dialect epoch 14 がこれを解消した。`fan.race` は `fan.min(clock)` となり、`fan.any` は削除され、上記の逐次表記に置き換わった（ADR-0028）。

<a id="selects--waits-for--stops-per-operation"></a>

## 操作ごとの選択・待機・停止

<a id="k1-first-success-in-input-order"></a>

### K1：入力順で最初の成功

| | 選ぶもの | 待つもの | 止めるもの |
|---|---|---|---|
| Almide `list.find_map(xs, (x) => f(x)?)` / `a() ?? b()`（旧 `fan.any`） | 最小インデックスの Ok。全要素が失敗すれば `??` の fallback | 勝者が判明するまで。その*前*の全要素が失敗する必要がある | **勝者の後の要素は一切開始しない**（監査§4、テスト `any_runs_nothing_after_the_winner…`）。勝者より前の要素はすべて実行済みで、effect は保持する。旧 `fan.any` の fs-prefetch 例外は、置き換え先の `list.find_map` ループの性質ではない。 |
| Rayon `find_first` | 述語を満たす最も左の要素 | split tree 全体の決着 | 見つけた一致の右側の split を「可能な限り早く」スキップする。他スレッドで実行中の作業は完了するため、答えより後の要素でも述語が実行され得る。pure な述語は慣習にすぎず、チェッカーはない。 |

Almide の K1 は観測上逐次であり、実行した要素集合は勝者までの接頭部分に正確に一致する。Rayon の K1 は同じ答えを返すが、実行された集合はタイミングに依存するその上位集合となる。

<a id="k2-first-success-in-real-time"></a>

### K2：実時間で最初の成功

| | 選ぶもの | 待つもの | 止めるもの |
|---|---|---|---|
| JS `Promise.any` | 最初に fulfil した promise。すべて reject なら全理由の `AggregateError` | 最初の fulfilment、または全 rejection | **何も止めない**。promise はキャンセルできず、他の promise は走り続ける。`AbortController` を手動で接続する必要がある。 |
| `futures::future::select_ok` | 最初に完了する `Ok` future。全失敗なら最後のエラー | 最初の `Ok`、または全失敗 | 残りの future を**破棄**し、以後 poll しない。すでに始めた外部作業（送信済みリクエスト、起動済みタスク）は取り消されない。 |
| Rayon `find_any` | 順序を問わず、一致するいずれかの要素 | 1つ見つかるまで | 残りの split をスキップし、実行中のものは完了する |
| Python `wait(FIRST_COMPLETED)` ループ | 結果が検査を通る最初のタスク | 各ラウンドで最初の完了 | それ自体では何もしない。呼び出し側が残りを `task.cancel()` する必要があり、次の `await` で `CancelledError` を起こす |
| Go | 手書きコードが選ぶもの | 最初の受信 | worker が検査すべき `context` に対する `cancel()` |

<a id="k3-first-completion"></a>

### K3：最初の完了

| | 選ぶもの | 待つもの | 止めるもの |
|---|---|---|---|
| JS `Promise.race` | fulfil／reject **どちらでも**、最初に settle したもの | 最初の settlement | なし（`Promise.any` 参照） |
| `futures::select!` / `select_all` | 最初に完了する分岐、Ok または Err | 最初の完了 | 他の分岐を破棄する（`select!`）、または残りの future を返す（`select_all`） |
| Python `wait(FIRST_COMPLETED)` | 完了した集合 | 最初の完了 | なし。pending 集合を呼び出し側へ返す |
| Go | 最初の受信 | 最初の受信 | context cancel、協調的 |

<a id="k4-least-logical-cost"></a>

### K4：最小の論理コスト

これを持つのは Almide だけである。`fan.min` は決定的な課金メーターの下ですべての分岐を1つずつ実行し、消費が最小の Ok を選ぶ。同値ならソース／リスト順となる（監査§5）。安い分岐を早く見つけても節約はない。全分岐が完了するか予算に達するまで走る。必須の Compute clock 引数は分岐ごとの予算を与え、課金箇所で打ち切られた分岐は除外する。分岐は pure なので外部では何も開始していない。答えは全マシンで同じで、#3778以降は全ターゲットでも同じである。両メーターが単一の課金表を読む（ループのない wrapper の入口は免除、補間は無料）。

最も近いものは、自分で計算するコストに対する Rayon の `min_by_key`（すべてを完了まで実行するため節約はない）と、明示的予算を持つ探索フレームワークである。4つのエコシステムのいずれにも標準 combinator はない。

<a id="k5-collect-all"></a>

### K5：すべて収集

| | 選ぶもの（エラー時） | 待つもの | 止めるもの |
|---|---|---|---|
| Almide `fan.map` / `fan { }` | **最小インデックス**の Err | 失敗後も含め**全**要素 | なし。全要素を実行する（ADR-0024 D1）。エラーは時間でなくインデックスで選ぶ。 |
| Almide `fan.settle` | 全 Result | 全要素 | なし |
| JS `Promise.all` | **時間上最初の rejection** | 最初の rejection、または全 fulfilment | なし。返された promise が reject した後も他は走り続ける |
| JS `Promise.allSettled` | 全 outcome | 全 promise | なし |
| `futures::future::join_all` | 全出力 | 全 future | なし |
| `futures::future::try_join_all` | **時間上最初の Err** | 最初の Err、または全 Ok | 最初の Err で残りを**破棄** |
| Rayon `collect::<Result<Vec<_>,_>>` | *いずれかの* Err。どれかは未規定 | 実行中の全 split | Err を検出後、新たな split の取得を止める |
| Python `gather()` | 時間上最初に発生した例外 | 最初の例外 | なし。「他の awaitable はキャンセルされず、実行を続ける」 |
| Python `gather(return_exceptions=True)` | 入力順の全結果または例外 | 全 awaitable | なし |
| Python `TaskGroup` | **全**失敗の `ExceptionGroup` | キャンセルしたものも含む全タスク | 最初の失敗時に残りのタスクを**キャンセル**し、次の `await` で `CancelledError` を届ける |
| Go `errgroup.Group.Wait` | **時間上最初の non-nil エラー** | 全 `Go` 呼び出しが戻るまで | それ自体ではなし。`WithContext` は最初のエラーで派生 context をキャンセルし、`ctx` を確認する worker だけが止まる。 |
| Go `errgroup.SetLimit(n)` | （同上） | （同上） | 実行 goroutine 数を制限。空き slot ができるまで `Go` はブロックする |

エラーを入力位置で選び、それでも残りの要素を実行するのは Almide だけである。これにより `fan.map` は逐次・並列・wasm 実行でバイト同一になる。代償として、長いリストの早い位置で失敗しても作業は節約できない。cheatsheet は最初の失敗で止めるには `!` 付き `for` ループを使うよう伝える。

<a id="what-a-translator-must-change"></a>

## 翻訳者が変更すべきこと

| 出発元 | 書いてしまう形 | Almide の意味 | 正しい Almide |
|---|---|---|---|
| JS `Promise.any` | `fan.any` | E027 | #3788導入後は `fan.min(duration.ms(n), mirrors, f)`（実時間で最初の成功、敗者をキャンセルして待機）。それまではリスト先頭側でよければ `list.find_map(mirrors, (m) => f(m)?)` |
| JS `Promise.race` | `fan.race` | E027 | pure な分岐の最小 compute なら `fan.min(compute.ms(n)) { … }`。最初の完了なら `fan.min(duration.ms(n)) { … }`（#3788）だが、失敗分岐は選ばずスキップする |
| JS `Promise.all` | `fan.map` | 全部実行、最小インデックスのエラー | `fan.map`（成功時の値は同じ） |
| Python `TaskGroup` | `fan.map` | 兄弟のキャンセルなし | `fan.map`。停止が必要なら `for … !` ループ |
| Go `errgroup.WithContext` | `fan.map` | context なし、最も早いインデックスのエラー | `fan.map` |
| Rust `try_join_all` | `fan.map` | 何も破棄しない | `fan.map` |
| Rayon `find_first` / `Iterator::find_map` | `list.find_map(xs, (x) => f(x)?)` | 同じ答え、勝者より後の要素は開始しない | `list.find_map` |
| I/O の timeout（`asyncio.wait_for`、`context.WithTimeout`） | `fan.timeout { http.get(u) }` | 現在は拒否。本体は pure 必須（E006） | 決定済み（ADR-0028）：`Duration` clock は I/O 本体を許し、次の課金箇所または I/O 境界でキャンセルし、終了を待つ — #3788 |

<a id="references"></a>

## 参考文献

- JS: MDN, [Promise.all](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/all), [allSettled](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/allSettled), [any](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/any), [race](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/race), [AbortController](https://developer.mozilla.org/en-US/docs/Web/API/AbortController)
- Rust futures 0.3: [join_all](https://docs.rs/futures/latest/futures/future/fn.join_all.html), [try_join_all](https://docs.rs/futures/latest/futures/future/fn.try_join_all.html), [select](https://docs.rs/futures/latest/futures/future/fn.select.html), [select_all](https://docs.rs/futures/latest/futures/future/fn.select_all.html), [select_ok](https://docs.rs/futures/latest/futures/future/fn.select_ok.html)
- Rayon 1.10: [find_first](https://docs.rs/rayon/1.10.0/rayon/iter/trait.ParallelIterator.html#method.find_first), [find_any](https://docs.rs/rayon/1.10.0/rayon/iter/trait.ParallelIterator.html#method.find_any), [with_min_len](https://docs.rs/rayon/1.10.0/rayon/iter/trait.IndexedParallelIterator.html#method.with_min_len)
- Python 3.12: [asyncio.gather](https://docs.python.org/3/library/asyncio-task.html#asyncio.gather), [TaskGroup](https://docs.python.org/3/library/asyncio-task.html#task-groups), [asyncio.wait](https://docs.python.org/3/library/asyncio-task.html#asyncio.wait), [Task.cancel](https://docs.python.org/3/library/asyncio-task.html#asyncio.Task.cancel)
- Go: [golang.org/x/sync/errgroup](https://pkg.go.dev/golang.org/x/sync/errgroup) (`Wait`, `WithContext`, `SetLimit`)
