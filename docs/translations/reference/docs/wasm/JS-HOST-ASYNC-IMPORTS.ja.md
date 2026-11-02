<a id="async-js-imports-on---host-js-jspi-3353-3371-3383"></a>

# `--host js` での非同期 JS import（JSPI、#3353、#3371、#3383）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](JS-HOST-ASYNC-IMPORTS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](JS-HOST-ASYNC-IMPORTS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](JS-HOST-ASYNC-IMPORTS.zh-CN.md)

> 固定原文の非規範読者用スナップショット。原文を正本とし、自動同期しない。現実装・runtime 互換性・性能・メモリ挙動は独立に検証していない。歴史的な版と測定は原文の主張であり、現在の普遍的保証ではない。コマンドや hook はここでは実行しない。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/JS-HOST-ASYNC-IMPORTS.md)


状態：JS host のみで実装済み（`src/cli/js_host_async.rs`、`src/cli/js_host_fan.rs`）。各要素が非同期 hook を 1 回呼ぶ `fan`（選択 7、#3383）を除き、module のバイト列は変わらない。この fan は重ねて実行するためのプロトコルを import する。それ以外では生成される `<mod>.js` / `<mod>.d.ts` のみ変わる。

<a id="the-problem"></a>

## 問題

`@extern(wasm, "js", NAME)` hook は同期的に値を返す必要がある。Workers KV、`fetch`、Cloud Storage は Promise としてしか利用できず、JS host は Almide コードからストレージ I/O を行えなかった。JSPI（`WebAssembly.Suspending` / `WebAssembly.promising`）はこれを解決する：

- Suspending import は hook の promise が確定するまで wasm stack を中断する。
- `promising` を通じて入る export は Promise を返す。

報告者の prototype は glue だけを編集した。Node 24、workerd、Cloudflare edge で正しく動作した。

<a id="choices"></a>

## 選択

**1. marker は extern 宣言に付ける**（#3371）。

```almide
@extern(wasm, "js", "kv_get", returns: promise)
fn kv_get(key: String) -> String
```

- marker は host binding の事実、つまりこの JS hook が Promise で応答することを述べる。wasm-bindgen の `suspending`、Emscripten の `JSPI_IMPORTS`、WIT の `async func` と同様、hook を結び付ける場所に置く。
- `async` ではなく名前付き引数である。ADR-0024 N1 は `async`、`await`、`Future` を言語の表面から除外しており、裸の `async` は言語にない `async fn` を誘発する。effect 軸は「色分けなし」の `fn` / `effect fn` のままである。
- 型、effect、呼出し側は変わらない。Almide の宣言は引き続き `fn kv_get(key: String) -> String`。呼出し側には普通の同期呼出しとして見え、native ターゲットは marker を無視する。
- parser は、`@extern(wasm, "js", ...)` の三つの位置引数の後でのみ `returns: promise`（または `returns = promise`）を受理する。ほかの名前付き引数や値、重複、別のターゲットや module への marker は parse error になる。hint に受理する綴りを示す。`almide fmt` は `returns: promise` と書く。
- `--host js` がなければ、marker は module を何も変えない。どちらでも module バイト列は同じ。`--host js` があれば、hook 呼出しを重ねる `fan` の箇所（選択 7）だけ module を変え、それ以外では生成 glue のみが marker を読む。
- marker 導入前、develop には暫定 build flag `--async-import NAME` があった。v0.67.0-rc1 より前に削除され、一度も release では出荷されていない。同じ hook をある host では同期、別の host では非同期に結び付ける必要が生じたら、build 時の override を復活させられる。属性をデフォルトとする。

**2. effect。** 非同期 import に新たな effect カテゴリーは与えない。

- ADR-0026 の下では extern 本体は foreign であり、すでに ⊤（全カテゴリー、`pass_effect_inference.rs`）と推論される。その呼出し側もすでにそれを持つ。
- 中断は host が import を実行する方法である。import がプログラムに及ぼす作用ではないので、型もカテゴリーも変わらない。

**3. どの export が非同期になるか。** 出荷バイト列（任意の `--wasm-opt` 後）から読み取った call graph が、非同期 import に到達できる export だけ。

- Node 24.21 で測定：`promising` を通さず入った export から呼ぶ Suspending import は `SuspendError` を投げる。hook が通常の値を返す場合でも起きる。したがって「中断し得る export だけ」という条件は保証でなければならない。
- すべての `call_indirect` / `call_ref` は table 内のすべての関数に到達すると仮定する。過大近似は export を不要に非同期にするだけで、中断し得るものを同期のまま残すことはない。
- 非同期 import が一つでもあれば全 export を非同期にする方法も正しい。しかし全呼出し側に純粋 helper（`width`）まで await させるので、到達可能性を用いる。
- `run()` も `_start` を通して同じ規則に従う。
- 非同期 export は `.d.ts` で `Promise<T>` を返す。marker 付き hook の型は `T | Promise<T>` になる。
- 作者が export に marker を付けることはない。したがって `promising` を通さず入った export が中断する import に到達することはなく、`SuspendError` へ到達できない。

**4. instance 内では一度に 1 呼出し。**

- JSPI では、一つの呼出しが中断している間に別の export 呼出しが入れる。structural runtime は呼出しを通じて stack の規律を守る必要がある instance ごとの状態を持つ：
  - region window を閉じると巻き戻す bump／heap head。
  - line-buffer の構築 cursor。
  - 決定的 meter の region global。
- その状態の上で中断した二つの stack を交互実行することこそ、ADR-0023（再入性の節）が p3 handler で拒否するものであり、backpressure を使って直列化する。
- glue も同様に直列化する：
  - 非同期 export は一つの FIFO queue（`serial`）を通る。次の呼出しが入る前に、中断を含めて各呼出しを完了させる。
  - 非同期呼出しが中断中に呼ばれた同期 export（または `run()`）は、中へ入らず `almide: <f> was called while an async call is suspended …` を投げる。
- 観測は、呼出し順に一つずつ続けて実行した場合と同じ（ADR-0024 の「as if sequential」）。fixture は、pending の hook が常に最大一つであることを検査する。

**5. JSPI がない場合**、`init()` は instance 化前に throw する。error は非同期 import と不足 API（`WebAssembly.Suspending /
WebAssembly.promising`）を名指しし、Node ≥ 24.20、Chrome ≥ 137、workerd を案内する。Node では実行中の版も示す。24.0 から 24.19 は JSPI が既定で無効の V8 を共有し、24.20 で初めて有効になる（#3362）。管理型「Node 24」runtime は遅れる場合があり、2026-10-04 の Google Cloud Run functions の `nodejs24` は 24.19.0 だった。それらの版では `--experimental-wasm-jspi` が機能するのは `node` のコマンドラインのみ。Node は `NODE_OPTIONS` 内では拒否し、実行時の `v8.setFlagsFromString` は API を組み込まない。途中まで試みることはない。marker 付き extern のない module は JSPI 不要で、以前動いた環境で動く。

**5a. marker の付け忘れは拒否する**（#3371）。marker のないすべての hook の応答は、一つの検査 `typeof r?.then === "function"` を通る。成立すると呼出しは次を投げる：

```
almide: hooks.js.kv_get returned a Promise; mark its @extern with returns: promise
```

診断文の訳：hooks.js.kv_get が Promise を返したため、@extern に returns: promise を付ける必要がある。

- 検査がなければ、Promise は以前、黙って `"[object Promise]"` または `0` として通過していた。
- throw は通常の hook の throw と同様に module の frame を unwind するので、`init()` を再実行するまで instance を放棄する。
- fallible extern でも、この拒否は hook の err ではない。throw を err に変える catch は、この拒否をそのまま通す。
- 捨てられた promise には何もしない rejection handler を付け、その promise 自体の失敗を unhandled rejection として二重報告しない。
- marker 付き hook は await し、この検査を持たない。

**6. 中断前後の marshalling。**

- 非同期 import の引数は hook 実行前（呼出し式内）に decode する。結果は promise の確定後に encode する。再開した hook から `__alloc` を呼んで問題ないことを報告者が確認した。
- export の引数は、自分より前の呼出しが確定した後、自分の queue 内の順番で構築する。
- 所有権と layout は同期 wrapper と同じ、記録された export ABI（#3352、#3354）。fixture の非同期 1,000 周でも `memoryBytes()` は一定。

**7. Fan の重ね合わせ**（#3383、2026-10-05 の ADR-0024 改訂）。

- `returns: promise` hook はプロセス外のシステムへの要求である。そのシステムが要求を受け取る順序は環境の ω（ADR-0024 D2/D3）なので、そのような hook の `fan` は待機を重ねられる。JS host での土台は JSPI（D8）。
- 各要素の 1 呼出しをそれぞれ Suspending 呼出しへ lowering すると要素ごとに待つ。代わりに glue は、何らかの fan が到達する各非同期 hook に対し、module `almide:fan` の三つの import を提供する：
  - `start:NAME(args) -> slot` は通常の import。引数を decode し、hook を呼び、その Promise を slot に保持する。
  - `wait()` だけが Suspending import。開始したすべての slot を await し、各 slot を値または rejection として確定する。
  - `take:NAME(slot) -> value` は通常の import。確定値を hook 自身の return と同じ helper で encode して返す。rejection は fallible extern では err、infallible extern では instance 放棄になり、直接呼出しとまったく同じ（#3356）。
- module は分岐順にすべての要素を開始し、一度中断し、分岐順に値を取り出す。すべての要素が動き（D1）、分岐順で最初の err が結果になる（D6）ため、観測は逐次 lowering と同じ。（最初の Ok の選択は dialect epoch 14 以降 `list.find_map` / `??` chain であり、定義上逐次なので重ね合わせ経路を通らない。）
- emitter（`crates/almide-wasm/src/fan_js_async.rs`）が働くのは、module が `--host js` で build され、各要素が `returns: promise` extern の 1 呼出しで、その引数が変数または literal、値が scalar または scalar 上の fallible extern の Result である場合だけ。対象は三つの形：fallible hook への `fan.map(xs, (x) => hook(x))`、infallible hook への `(x) => ok(hook(x))`、全分岐がそのような呼出しである `fan { hook(a), other(b) }` block。
- それ以外はすべて逐次で、要素ごとに一つの Suspending 呼出しを行う。引数を計算する要素、同期 hook を呼ぶ要素、複数呼出しを順に行う要素が該当する。複数呼出しには、その間で要素が中断する必要がある（ADR-0024 D8 本来の要件）。
- export 推論は不変。call graph が `wait` に到達する export は `promising` を通して入り、ほかの非同期 export と同じく直列化する。
- **module バイト列。** このような fan があるプログラムは `--host js` で module バイト列が変わる。三つの追加 import が入り、hook の直接呼出しはなくなる。その他のプログラムと、`--host js` なしのすべての build は従来とバイト同一。`ALMIDE_FAN_SEQUENTIAL=1`（ADR-0024 D7）は逐次 lowering を build する。

<a id="not-covered"></a>

## 対象外

- **module へ callback する hook。** 非同期 hook 内から呼ぶ同期 export は busy guard が拒否する。非同期 export なら、自分を待っている呼出しの後ろに並び、決して確定しない。そのようにしてはならない。
- **reject する hook** は throw する同期 hook と同じように扱う（#3356、docs/wasm/WASM-OUTPUT.md「JS host」）。fallible extern（`effect fn` または `Result[T, String]`）は rejection を err として受け取り、infallible extern は `init()` 再実行まで instance を放棄する。
- **Flow／stream**（`docs/roadmap/on-hold/flow-design.md`、「JSPI での Flow 実装」）。この文書は単一値 import のみ扱う。

<a id="gate"></a>

## ゲート

`spec/wasm_host_js/async_imports.almd` を `scripts/check-js-host.sh` が実行する。非同期 hook には `returns: promise` を付ける。fixture が検査すること：

- どの export が非同期で、どれが同期のままか。
- 重なる呼出しに、正しく、順番に、一つずつ応答すること。
- closure を通じた非同期 import への到達と、effect fn からの err。
- reject する infallible hook（instance を放棄、`init()` で復旧）。
- メモリが一定であること。
- JSPI がない場合の拒否。

`hook_errors_async.almd` は `hook_errors.almd` の reject する対応版。`fan_async_overlap.almd` は fan の重ね合わせ（7）を検査する。ゲートは重ね合わせ版と `ALMIDE_FAN_SEQUENTIAL=1` 版を一度ずつ build し、両実行が同じ `.expected` を出力しなければならない。分岐順の値、途中の分岐の rejection、`list.find_map` の最初の Ok（逐次）、fan block、一定のメモリ、instance を放棄する infallible hook を含む。host は、重ね合わせ実行では全 hook が同時に進行し最遅 hook 程度の時間となること、逐次実行では一つだけ進行し合計時間になることを検査する。Node 24.21 では、10–80 ms の八つの hook は重ね合わせで 82 ms、逐次で 369 ms だった。`unmarked_promise.almd` は infallible、fallible、`Unit` hook について 5a の拒否を検査する。何も marker を付けないため JSPI は不要。

marker 付き extern の fixture は JSPI 付き node が必要なので、CI の JS host job は JSPI feature probe とともに固定 Node 24.22.1（`.github/workflows/ci.yml`）を使う。ローカルでは JSPI のない node は警告を出してそれらの fixture を skip する。

parser の受理と拒否は `crates/almide-syntax/src/parser/test_attributes.rs` および診断 fixture `tests/diagnostics/plain-extern-returns-unknown` にある。

> 範囲の注記：選択 4 の pending hook が一つという検査は直列化された export 呼出しを述べ、選択 7 は対象 fan 内の hook の重ね合わせを別途認める。SuspendError なしという記述は生成 wrapper の到達可能性規則の範囲で保持し、wrapper を迂回する利用への保証ではない。fixture のメモリ一定は、全プログラムのリーク不在の証明ではない。
