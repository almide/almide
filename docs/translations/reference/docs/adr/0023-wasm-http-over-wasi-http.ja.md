<a id="adr-0023-wasm-http-is-wasihttp03--a-program-that-uses-http-builds-as-a-p3-component-the-embedded-host-serves-the-same-imports-and-no-host-serves-a-custom-http-op"></a>

# ADR-0023: Wasm HTTPは `wasi:http@0.3` — HTTP使用プログラムはp3 componentになり、embedded hostも同じimportを提供し、独自HTTP opを提供するhostはなくす

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0023-wasm-http-over-wasi-http.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0023-wasm-http-over-wasi-http.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0023-wasm-http-over-wasi-http.zh-CN.md)

> 非規範的な歴史読者スナップショット。変更していない固定原本が正本。自動同期・独立実行・現行host互換性・安全性の保証ではない。採択方向、試作、計画gateは移行実装済みを意味せず、当時まだ何も実装されていないという原文を保持する。測定・エコシステム・版の主張は当時の証拠に属し、今回再検証していない。213対166 packageと追加64名は別々の原文記述として保持し、減算で訂正しない。stock hostの自由文internal errorはエラー一致の例外で、proxy/trust rootは各hostが管理する。HTTP component既定化、host op廃止、契約変更は記録された計画であり、記載コマンドは実行許可ではない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md)

- **状態**: 採択済み。メンテナは2026-09-28にWASI 0.3（`wasi:http@0.3`）を目標とし、p2段階を設けないと決めた。本記録は提案の方向性を測定と先行事例で検討し、実装順序を示す。まだ何も実装されていない。証拠によって提案を変えた箇所には**[refined]**と記す。一覧は§9。
- **日付**: 2026-09-28
- **範囲**: `--target wasm` のすべての `http` クライアント関数と `--target wasm` の `http.serve`、embedded host（`almide run --target wasm`、`crates/almide-wasm-run`）、p3 shim（`crates/almide-wasm-run/src/wasi_p3*.rs`）、`almide build --target wasm` の既定の形、契約行C-328、C-366、C-367。
- **置き換えるもの**: [ADR-0020](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md) §5.3「どのexport形状にするか」（P2の `wasi:http/incoming-handler@0.2` 選択）とstep 7/9の順序。ADR-0020のその他は維持する（下の§3.3で、その柱がP3 instance modelを安全にする理由を示す）。
- **関連**: #1710（wasi:http@0.3クライアント経路）、#1628（component model段階）、#2659（server用stock artifact）、#2702（ADR-0020 step 9）、#2633（call handle）、#2819–#2826（HTTP監査）、#865（wasm VM）、#1696（従来emitter廃止）、`proofs/wasi-pin-policy.toml`、`proofs/target-availability.toml`。
- **証拠の基盤**: §2の測定は2026-09-28、almide 0.64.0（5555cb251）、`wasmtime` CLI 47.0.2で実行。ソースを読んだcrateはwasmtime 47.0.4 / wasmtime-wasi 47.0.4 / wasmtime-wasi-http 47.0.3 / wasmparser 0.259.0。tree外に置いた2つの試作（branch `stock-wasm-serve-2659` の#2659 probeを元にwasm-encoder + wit-componentでbuildした `wasi:http/handler@0.3.0` componentと、embedding host §2.4）、`../almide-references/RESEARCH-component-model.md` と `RESEARCH-http-serve-concurrency.md`、§2.6の一次資料。

<a id="1-context"></a>

## 1. 背景

<a id="11-three-lanes-three-http-mechanisms"></a>

### 1.1 3つの経路、3つのHTTP機構

コンパイラはhost表層が `almide.*` import（op番号付き `fs_call`、`host_read`、`print`、`exit` など）であるcore moduleを1つ出す。その後は経路によって異なる。

| 経路 | 実行対象 | HTTP提供方法 |
|---|---|---|
| native | Rust binary | runtimeへ組み込んだ `crates/almide-rt-core`（client/server core） |
| embedded（`almide run --target wasm`） | wasmtime上のcore module。`crates/almide-wasm-run/src/host.rs` が `almide.*` を提供 | **host**が同じ `almide-rt-core` 関数を呼んで `fs_call` op 43..=50（client）、53..=59（call handle）、70..=72（serve）を提供 |
| stock p1（既定の `almide build --target wasm`） | `to_wasi` が `almide.*` をWASI preview-1 shimへ書き換え | なし。全HTTP関数はbuild時E081 |
| p3（`ALMIDE_COMPONENT_P3=1 … --component`） | `to_p3` が `almide.*` をWASI 0.3上の**guest側**shimへ書き換え、wit-componentで包む | shimが `wasi:http/client@0.3.0` 上でop 43..=50を提供 |

したがって「独自op」は2つの異なるものを指している。

- 生成コードとshim境界の**op番号**。stock artifact内ではhostへ到達しない。shimはguestコードであり、Rust p2 componentのpreview-1 adapterがguestコードであるのと同じ。
- embedded hostによるその番号の**host側サービス**。これがembedded経路を全stock runtimeとは別のABIにしている。

<a id="12-what-is-missing"></a>

### 1.2 不足するもの

`proofs/target-availability.toml`（develop f4d5c063b）から:

- `get_response` / `post_response` / `put_response` / `patch_response` / `delete_response` / `request_response`（6つ）と `request_stream` はstructural、stock-p1、**embedded**の各経路でwallになる。
- `anthropic_streaming_call`、`openai_streaming_call`、その `_with_limits` 形式（4つ）は全経路でwall。native intrinsicである（`@intrinsic("almide_rt_sse_*")`、`stdlib/http.almd:252`）。
- call handle（`start` / `poll` / `read_new` / `wait` / `cancel`、`request_stream_with_limits`）と `http.serve` はembedded hostだけが提供する。
- p3 shimが提供するのは5つのverbとframed family（op 43..=50）だけで、他の `http` はない。`env.get`（host op 26）も拒否する。§2.3で測定する。

<a id="13-the-question"></a>

### 1.3 問い

nativeとwasmが同じ応答をするよう、wasmのAlmideプログラムをHTTP client/serverにする理想的な方法は何か。提案された方向は、(1) 独自host opをなくし不足関数を `wasi:http` 上のguestへ、(2) embedded hostも独自opでなく `wasi:http` を実装し同一artifactをembedded/stock runtimeで動かす、(3) HTTP使用プログラムは既定でcomponentにする、(4) serverはhandlerをexportする、(5) 移行後に独自opを廃止する、というもの。ownerは版をWASI 0.3に固定した。

<a id="2-measured-evidence"></a>

## 2. 測定証拠

全コマンドをscratch directoryで、ローカルecho server（`python3 echo.py 18777`。content-length付きで `"<METHOD> <path> len=<body length>"` を返す `ThreadingHTTPServer`）と共に実行した。

<a id="21-what-wasmtime-47-needs-for-p3-and-why"></a>

### 2.1 wasmtime 47がp3に必要とするものと理由

`wasmtime 47.0.2`、`wasmtime run -S help` / `-W help` の確認後に実行:

| Artifact | コマンド | 結果 |
|---|---|---|
| `hello_p3.wasm`（Almide、`println` のみ、p3） | `wasmtime run hello_p3.wasm` | 拒否: `synchronous 'stream.write' requires the component model more async builtins feature` |
| 同上 | `wasmtime run -W component-model-more-async-builtins=y hello_p3.wasm` | `hello`（wasmtime 46以降p3は既定onなので `-S p3` 不要） |
| `get_p3.wasm`（Almide、`http.get`、p3） | `wasmtime run -W component-model-more-async-builtins=y get_p3.wasm` | 拒否: `component imports instance 'wasi:http/types@0.3.0', but a matching implementation was not found` |
| 同上 | `… -S http get_p3.wasm` | `GET /hello len=0` |
| `h_sync.wasm`（#2659 handler probe: 同期 `stream.write` / `future.write`） | `wasmtime serve -S p3 -S http h_sync.wasm` | 拒否: `synchronous 'stream.write' requires …` |
| `h_async.wasm`（同probeの2つのwriteを `[async-lower]` に変更し、writeがBLOCKEDなら `waitable-set.wait`） | `wasmtime serve -S p3 -S http h_async.wasm` | `HTTP/1.1 418 I'm a teapot`、`x-count: 1`、body `hello from p3 #1`、次のrequestで `#2` |
| 同上 | `wasmtime serve h_async.wasm`（**一切flagなし**） | 同じ応答 |

理由はwasmparser 0.259.0の `src/validator/component.rs` にある。`cm_more_async_builtins`（🚝）でgateされるcanonical builtinは、**同期** `stream.read` / `stream.write` / `future.read` / `future.write` と、**async** `stream.cancel-*` / `future.cancel-*` だけである。`waitable-set.new/wait/poll/drop`、`waitable.join`、`subtask.cancel/drop`、`backpressure.inc/dec`、`task.return`、async stream/future opが必要とするのは `cm_async` だけで、wasmtime 47は既定で `cm_async` を有効にする（`wasmtime-47.0.4/src/config.rs:2526-2531`）。

**発見1。** 固定wasmtimeでp3 componentが `-W` を必要とするのは、Almideのshimが🚝同期builtinを使うためだけである。baseline async builtinで作るshimは `wasmtime serve app.wasm`、`wasmtime run -S http app.wasm` で動く。ここで `-S http` は `--dir` 同様のcapability grantである。**wasmtimeの更新は不要。** flagなしhostが受け入れるのも同じ形で、🚝はWASI 0.3 baselineに属さない（RESEARCH-component-model.md「Built-ins under the baseline gate」）。

ADR-0020 §5.3がP2を選んだ理由は「固定wasmtimeでP3には `-W
component-model-more-async-builtins` が必要」だった。その前提はP3でなく#2659 probeの同期writeに由来した。

<a id="22-the-embedded-host-can-serve-the-same-imports-prototype"></a>

### 2.2 embedded hostは同じimportを提供できる（試作）

`wasmtime-wasi-http 47.0.3` に `p3` featureがある（`src/p3/mod.rs`。「experimental, unstable」と記され、p3のみのbugfixにはpatch releaseが出ない）。外向きtransportはtrait method `WasiHttpHooks::send_request(request,
options, fut)` である。`default-send-request` featureなしならembedderが提供する必要があり、そのcrateからrustls / webpki-roots / tokio-rustlsは引き込まれない。

試作host（約80行）は `wasm_component_model_async(true)` を持つ `Engine` を作り、`wasmtime_wasi::p3` と `wasmtime_wasi_http::p3` をlinkする。`send_request` はrequest bodyを集め、blocking threadで `almide_rt_core::http_client_core::request_response` を呼ぶ。今nativeとembedded hostが呼ぶのと同じ関数である。固定crateで最初の試行からcompileできた（`cargo build --release -j2 --offline`: coldで3 min 41 s）。Almideがbuildしたp3 componentを変更なしで実行する。

```
$ MORE=1 embedhost post_p3.wasm        # http.post, then http.get to a refused port
POST /p len=6
err: http request failed (p3 transport)
```

| 同じ `post.almd` を実行した経路 | stdout |
|---|---|
| native（`almide run`） | `POST /p len=6` / `err: connection failed: Connection refused (os error 61)` |
| embedded、host op（`almide run --target wasm`） | nativeと同一 |
| `wasmtime run … -S http post_p3.wasm` | `POST /p len=6` / `err: http request failed (p3 transport)` |
| 試作host（wasi-http p3 + rt-core hook） | wasmtime CLI行と同一 |

**発見2。** 固定版でembeddingは実現可能で、1つのartifactがembedded hostとstock CLIで同じ応答をして動く。成功した通信は既に4経路すべて一致する。エラー文は異なるが原因はhostでなく**guest**にある。試作hookは `internal-error(some("connection failed: Connection refused (os
error 61)"))` を返したのに、shimが全 `error-code` を1つの固定文へ潰す（C-328は既にp3文言を約束しないと記録）。§4.2で文言を一致させる方法を決める。

embeddingの依存コスト: 試作の依存treeは213 package、現在の `almide-wasm-run` は166。追加された64の名前は `wasmtime-wasi`、`wasmtime-wasi-http`、`wasmtime-wasi-io`、`wiggle`、`tokio`、`hyper`、`h2`、`http`、`http-body(-util)`、`cap-std` とそれらの推移的crate。試作binaryは25 MB、インストール済み `almide` binaryは50 MB。

<a id="23-the-p3-lane-today-refuses-what-an-http-program-usually-needs"></a>

### 2.3 当時のp3経路はHTTPプログラムが通常必要とするものを拒否する

```
$ ALMIDE_COMPONENT_P3=1 almide build envget.almd --target wasm --component
error[E081]: env.get (host op 26) is unavailable in the direct WASI 0.3 component.
$ ALMIDE_COMPONENT_P3=1 almide build resp.almd --target wasm --component
error[E081]: `http.get_response` is not available on --target wasm
```

p3で `component_availability::check` が提供するのはop 1..=9、13..=16、30..=32、34..=35、40..=50、60、73。LLM clientは `env.get` でkeyを読むため、現時点ではp3でbuildできない。

<a id="24-cost-size"></a>

### 2.4 コスト: サイズ

同じプログラムの `almide build`（structural経路、wasm-optなし）:

| プログラム | p1 core module | p2 component（`--component`） | p3 component |
|---|---|---|---|
| `println("hello")` | 1,322 B | 6,107 B | 18,421 B（core module 7,521 B） |
| `http.get` + `println` | E081 | E081 | 30,278 B（core module 12,735 B） |

p3 componentサイズの大半はcodeでなくcomponent型情報（`wasi:http/types` interface）である。差はkilobyte単位で、支払うのはHTTPプログラムだけ（§3.3）。

<a id="25-cost-time"></a>

### 2.5 コスト: 時間

`bench.py`: loopback echoにN回逐次 `http.get` するプログラム。2回warm-up後9回の中央値。N = 1とN = 500。傾きがrequestあたりコスト。

| 経路 | N=1（ms） | N=500（ms） | requestあたり（µs） |
|---|---|---|---|
| native binary | 2.4 | 52.3 | 100 |
| embedded、host op（`almide run --target wasm`、compile込み） | 102.2 | 153.2 | 102 |
| `wasmtime run -W … -S p3 -S http`（hyper transport） | 4.8 | 66.3 | 123 |
| 試作host（wasi-http p3 + rt-core transport、compile uncached: 8 ms） | 11.8 | 77.5 | 132 |

`drive.py`: `http.response(200, …)` serverへ、各1接続で500回逐次request。

| Server | 中央値 | p90 |
|---|---|---|
| native `http.serve` | 88 µs | 135 µs |
| embedded `http.serve`（host op 70..=72） | 69 µs | 83 µs |
| `wasmtime serve` p3 handler（既定のinstance再利用） | 93 µs | 113 µs |
| `wasmtime serve --max-instance-reuse-count 1`（requestごとに新instance） | 107 µs | 129 µs |

**発見3。** component経路はhost op比でloopback requestあたり20–30 µs、uncached embeddingで一度だけ約10 msのcompileコストがある。いずれも実ネットワークの往復に比べ小さく、第2のABIを残す理由にはならない。

<a id="26-how-others-do-it-2026"></a>

### 2.6 他の実装の方法（2026年）

stock runtimeを対象にする実装はHTTP用のprivate host ABIを出荷しない。どのtoolchainも `wasi:http` へloweringし、transportはhostが所有する。

- **Rust**: `wasm32-wasip2`（Tier 2）がcomponentを出力し、`wasip3` crateとwit-bindgen async modeは `wasi:http@0.3` を対象にする。`wasm32-wasip3` はTier 3。`wstd` / `waki` / `wasi-fetch` はbinding上の薄いclient/handler library。
- **Go**: componentize-goがgoroutineをcallback ABIへ対応させる（golang/go#77141が提案）。TinyGo `wasip2` は `wasi:http` を使う。
- **Python**（componentize-py、asyncio）と**JS**（jco / ComponentizeJS、`wasi:http` 上のStarlingMonkey `fetch`）: 出荷済み。jcoはWASI 0.3全体をサポート。
- **Host**: wasmtime 46+はWASI 0.3とcomponent-model asyncを既定有効化。Spin 3.6はWASIp3 instanceを「並行にも直列にも」再利用。wasmCloud 2.5はwasmtime 46で動く。
- MoonBitのp3 asyncは「予定」、Grainにはない。

WASI 0.3.0は2026-06-11、0.3.1は2026-08-11に批准された（`proofs/wasi-pin-policy.toml` [sources]）。出典: [WASI 0.3 launch](https://bytecodealliance.org/articles/WASI-0.3)、[wasi.dev WASI 0.3](https://wasi.dev/releases/wasi-p3)、[Spin 3.6](https://spinframework.dev/blog/announcing-spin-3-6)、[wasmCloud on WASI P3](https://wasmcloud.com/blog/wasi-p3-on-wasmcloud/)、[wasip3 crate](https://crates.io/crates/wasip3)、[rustc wasm32-wasip3](https://doc.rust-lang.org/rustc/platform-support/wasm32-wasip3.html)、`../almide-references/RESEARCH-component-model.md` §「Reference-compiler wasm backends」。

<a id="27-what-a-stock-host-does-with-the-audits-issues-source-reading"></a>

### 2.7 stock hostは監査のissueをどう扱うか（ソース読解）

`wasmtime-wasi-http 47.0.3`、既定transport（`src/p3/request.rs`）:

- 直接接続する（`TcpStream::connect(&authority)`）。proxy対応はない（#2819）。
- `webpki_roots::TLS_SERVER_ROOTS` のみ信頼する（#2820）。
- connect、first-byte、between-bytes timeoutはguestの `request-options` に由来し、各既定値は600 s（#2825）。
- `fields.append` は `http::HeaderValue::from_bytes` で検証するため、値のCR/LFは `header-error.invalid-syntax`（#2822）。
- 9つのheader名が禁止される（`DEFAULT_FORBIDDEN_HEADERS`、`src/lib.rs:142`）: `connection`、`keep-alive`、`proxy-authenticate`、`proxy-authorization`、`proxy-connection`、`transfer-encoding`、`upgrade`、`host`、`http2-settings`。nativeは現在すべて受理する。
- hyperがresponseをparseするので、1xx interim responseとchunk extension（#2824）は処理される。
- header名は小文字でguestに届く（#2659 probe: `x-Count` → `x-count`）。

<a id="3-decision"></a>

## 3. 決定

<a id="31-the-five-points"></a>

### 3.1 5つの項目

| # | 提案 | 判断 |
|---|---|---|
| 1 | 独自host opをなくし、不足client関数を `wasi:http` 上のguestへ | **確認、精緻化。** 新opをhostに提供させない。op番号は生成コードとp3 shim間のguest内部callでのみ許可し、shimは `wasi:http/client@0.3.0` 上で提供する（adapter pattern、§1.1）。6つの `*_response`、`request_stream`、call handleはshimへ入れる。4つのstreaming LLM関数は**call handle上のAlmideでself-host**し、shim/host codeを一切不要にする。 |
| 2 | embedded hostは独自opでなく `wasi:http`（wasmtime-wasi-http）を実装 | **確認、精緻化**（§2.2）。hostは固定47で `wasmtime_wasi::p3` と `wasmtime_wasi_http::p3` をlinkし、`default-send-request` を使わず `send_request` hookを `almide-rt-core` とする。transportはnative共有codeのままで、これが構造上C-328を保つ。embedded **server** loopはaccept、parse、limitに `http_server_core` を維持し、guestの `wasi:http/handler.handle` exportを呼ぶ（§3.4）。 |
| 3 | HTTP使用プログラムは既定でcomponentとしてbuild | **p3について確認、前提は2つ。** 任意の `http` 関数を呼ぶプログラムは既定でWASI 0.3 componentになる。まずshimはbaseline async builtinだけを使い（発見1）、artifactが `-W` なしで動く。次にshimはHTTPプログラムが使うop、env/args（`wasi:cli/environment@0.3.0`）、sleep、§7 step 3の残りを提供する。**[refined]** HTTPを**使わない**プログラムはpreview-1 core moduleのまま。18 KBでなく1.3 KBで、wasmtime LTS 36（既定p3なし）と#865 VMでも動く。既定値は既に `wants_http` が `to_p3` へ振り分けるのと同じく、プログラムごとのop集合で決める。 |
| 4 | serverのhandlerをexport | **確認、対象を `wasi:http/handler@0.3.0` へ変更。** ADR-0020 §5.3の `incoming-handler@0.2` を置き換える（§3.3）。 |
| 5 | 移行後に独自opを廃止 | **HTTPについて確認。** embedded経路でHTTPプログラムをp3 componentとして動かすようになったら、host op 43..=50、53..=59、70..=72を `almide-wasm-run` から除く。embedded hostの非HTTP `almide.*` 表層（fs、print、alloc counter、上限付きmemory）は範囲外。非HTTPプログラムは引き続きcore moduleとして動かし、alloc/size台帳もそれを測る。 |

<a id="32-one-artifact-two-runners"></a>

### 3.2 1つのartifact、2つのrunner

HTTP使用プログラムについて:

```
almide build app.almd --target wasm -o app.wasm   # a WASI 0.3 component
wasmtime run -S http app.wasm                     # a client, on any wasmtime ≥ 46
wasmtime serve app.wasm                           # a server
almide run app.almd --target wasm                 # the SAME bytes, on the embedded host
```

embedded経路は別ABIではなくなる。transport、TLS、error分類がnativeのものであるstock artifactのrunnerとなる。

<a id="33-why-p3-for-the-server-superseding-adr-0020-53"></a>

### 3.3 ADR-0020 §5.3を置き換えてserverにP3を選ぶ理由

ADR-0020 §5.3はP2を選ぶ理由を3つ挙げた。

1. 「P2は固定wasmtimeでexperimental flagなしにloadできる」。発見1で反証された。async-builtin P3 handlerは**一切flagなし**でloadでき、同期版は `-S p3` の有無によらず失敗する。
2. 「P2はguestがinterleaveしないinstance modelである」。正しいが、ADR-0020自身の柱が既にinstance間のprogram状態を観測不能にする。到達可能な `var` なし（pillar 1）、request間状態は `kv` のみ（pillar 2）、instance内で閉じたapp（§5.2）。残るのはruntime自身の再入（allocator、lazy-`let` slot、scratch buffer）。P3 shellはentryの `backpressure.inc` とresponse body終了後の `backpressure.dec` でinstance内handlerを直列化する。このbuiltinはWASI 0.3 baselineであり（wasmparserのgateは `cm_async` のみ）、準拠hostは従わねばならない。ADR-0020がguest側queueを計画していた箇所に使う仕様上の機構である。step 8測定でbackpressureを無視するhostが見つかれば、ADR-0020通りguest queueへfallbackする。
3. 「P2は方向性に名指しされたinterface名である」。方向は今WASI 0.3である（owner裁定、2026-09-28）。

裁定以外にもP3の理由はある。Spin 3.6と `wasmtime
serve` はP3 instanceを再利用するので、appはrequestごとでなくinstanceごとに1度buildする。P2 artifactはp3 clientの隣に第2のcomponent worldを作る（P2の `wasi:http@0.2` clientとP3 clientは別interface）。つまり同期を保つshimがもう1つ必要になる。

<a id="34-the-embedded-server-loop"></a>

### 3.4 embedded server loop

embedded hostは `wasmtime serve` のhyper serverを**使わない**。accept、request parse、`max_body_bytes`、`request_timeout_ms`、shutdownには `almide-rt-core` の `http_server_core` を維持する（ADR-0020 §5.5–5.7）。requestごとに `wasi:http` request resourceを作り、componentの `handle` exportを呼ぶ。したがってnativeとembeddedはserver codeを共有し続け、これがC-367の基盤となる。`wasmtime serve` はC-367のHTTP意味論比較（status、header集合、body）の下でのstock hostであり、hyperのreason phraseと `date:` はhostの装飾である。

<a id="4-semantics-who-owns-what"></a>

## 4. 意味論: 誰が何を所有するか

<a id="41-the-rule"></a>

### 4.1 規則

- **guestはprogramが値として観測するものを所有する**: URL分解、header妥当性、body limit、deadline、cancel、errorの**文言**。stdlibまたはshimに一度書き、全経路で動かす。
- **hostはtransportとその信頼を所有する**: DNS、socket、proxy、TLS、trust root、接続再利用。stock hostではhost設定である。nativeとembeddedでは `almide-rt-core` に両経路分を一度だけ実装する。

<a id="42-error-text"></a>

### 4.2 エラー文

transport失敗を `wasi:http` の `error-code` case（`DNS-error`、`connection-refused`、`connection-timeout`、`TLS-certificate-error`、`HTTP-response-body-size` など）に分類する。1つのrendererがcaseを `err` 文字列へ変える。native（rt-core）と、任意のhostからcaseを受け取るshimがrendererを呼ぶ。同じ失敗はnative、embedded、全stock hostで同じ文言になる。自由文を持つのは `internal-error(some(msg))` だけで、その文言一致はnative/embedded間だけ保証する。これでnativeの文言が変わる（現在の `connection failed: Connection refused (os error 61)` はOS固有errnoを含む）ため、先にalsでC-328を改訂する（§7 step 0）。

<a id="43-per-concern"></a>

### 4.3 関心事ごとの分担

| 関心事（監査issue） | native / embedded（`almide-rt-core`） | stock p3 host | 一致させる機構 |
|---|---|---|---|
| streaming body | rt-core read loop | `consume-body` stream | shimがasync `stream.read` で同じcall-handle状態機械へ読む |
| `total_ms`（C-366） | rt-core deadline | `wasi:http` にはない | guest側: `wasi:clocks/monotonic-clock` の `wait-for` と `send` を1つのwaitable setで競争させ、発火したら `subtask.cancel` とbodyのdrop。guestが全経路で `request timeout: total_ms <n> exceeded` を書く |
| `idle_ms` | rt-coreのreadごとtimeout | hostが対応すれば `request-options` first-byte / between-bytes timeout | guest timerが正本。optionはhint（hostは `not-supported` と返してよい） |
| connect timeout、上限なしresponse（#2825） | rt-coreで修正（`connect_timeout`、size cap） | host既定（wasmtime: 600 s） | guestの `total_ms` deadlineとbody-size limitで全経路を制限 |
| cancel（C-366） | rt-core shutdown | `subtask.cancel` / response streamのdrop | serverがcloseを観測。各経路のfixtureとする |
| proxy（#2819） | rt-coreで修正（`HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` / `NO_PROXY`） | host設定。wasmtimeにはない | native/embeddedで約束。stockは契約外と行に明記 |
| TLS trust root（#2820） | rt-coreで修正（`SSL_CERT_FILE` / `SSL_CERT_DIR`、OS store） | host設定。wasmtimeはwebpki-rootsのみ | 上と同じ。wasm guestは独自trust storeを持てず、持つべきでもない |
| header内CR/LF（#2822） | 拒否 | 拒否（`invalid-syntax`） | 全経路の送信前にstdlibがname/valueを検査し、`err` 文言をどこでも同じにする。`http.serve` responseも同じ検査 |
| 禁止header | 現在は受理 | 拒否（§2.7の9名） | stdlibが全経路で同じ9名を拒否。compile拒否でなくruntime `err` なのでdialect epochではない |
| URL parse（#2821） | 現在はrt-core `parse_url` | guestがscheme / authority / path-with-queryに分ける必要あり | stdlibの `url` module上のparserを1つ。分解結果をrt-coreと `request.set-*` へ渡す |
| 1xx、chunk extension（#2824） | rt-coreで修正 | hyperが処理 | native/embeddedのfixture |
| response header名 | serverが送った通り | 小文字 | 全経路で小文字（nativeが正規化） |
| server body limit（#2823） | `http_server_core` | shellがrequest streamを読む間に数える（ADR-0020 §5.7） | 全経路で `413` |

<a id="5-contracts"></a>

## 5. 契約

- **C-328**（client）を「native ⇄ embedded」から「native ⇄ embedded ⇄ stock p3 host」へ拡張する。status、小文字化header集合、body、error-code分類が3つすべてで一致する。分類済みerrorのrender文は3者で一致し、`internal-error` はnative/embedded間で一致する。証拠に `wasmtime run -S http` 下のp3 fixtureを追加する。
- **C-366**（call handle）にstock p3経路を追加。そこでの `total_ms`、`idle_ms`、`cancel` はguestが強制し、文言も同じ。
- **C-367**（serve）はADR-0020のHTTP意味論の文言を保ち、同じartifactを動かす `wasmtime serve` を第3経路にする。
- 新契約でheader拒否（CR/LF、禁止9名）を全経路の同じ `err` とする。

新規または改訂文はすべて先にalmide/alsへ入れ、次にpinを動かし、それから実装する（CLAUDE.md「A new C-NNN lands in almide/als FIRST」）。

<a id="6-migration"></a>

## 6. 移行

<a id="61-what-breaks-for-deployed-artifacts"></a>

### 6.1 配備済みartifactで壊れるもの

- **HTTP未使用プログラム**: 何もない。byte単位でpreview-1 core moduleのまま。
- **HTTP使用プログラム**: 以前は `almide build --target wasm` がE081で拒否してbuildできなかったため何もない。今度はcomponentとしてbuildできる。
- **`ALMIDE_COMPONENT_P3=1` 利用者**: artifactは `-W component-model-async=y,component-model-more-async-builtins=y` を不要にする。旧flagも動く。環境スイッチは非HTTPでは意味を維持（p3 opt-in）し、HTTPではno-op。
- **runtime**: HTTP artifactにはWASI 0.3 hostが必要。wasmtime ≥ 46、jco、Spin 3.6+、wasmCloud 2.5+。wasmtime LTS 36では動かない。build行にそう表示し、p1限定runtime向けE081 noteは適用されなくなる。

<a id="62-proofstarget-availabilitytoml"></a>

### 6.2 `proofs/target-availability.toml`

- `p3-component` 経路（既にheader語彙にあり「まだ宣言行なし」）を測定対象経路とし、HTTP使用プログラムの**build**経路とする。
- `http.*` のstock-p1行をp3-component行へ置き換える。§7 step 3–5が入るにつれ空になり、`pending_port_ceiling` も下げる。
- embedded経路の `http.*` 行は同一artifactなのでp3-componentに追随する。
- `http.serve` 行は「stock p1 artifactにlisten socketなし」からhandler exportによる提供へ移る。

<a id="63-diagnostics"></a>

### 6.3 診断

- **E081**は「この関数を提供するbuild経路なし」のcodeとして残す。HTTP programの理由はpreview 1でなくWASI 0.3 componentを名指しする。現在の `component_availability` は「direct WASI 0.3 componentで利用不可。… `almide run --target wasm` はembedded hostを使う」とする。embeddedもcomponentとまったく同じものしか提供しなくなるため、第2文を削除する。ユーザーを誘導する別経路はなくなる。
- **E082**（両emitter経路がprogram形状を拒否）は不変。
- `almide check --target wasm` もbuildと同じ振り分けを行い、check時にE081が出る。

<a id="64-the-865-vm"></a>

### 6.4 #865 VM

`almide-wasm-vm` は閉じた命令集合でCritical profile programの出荷preview-1 artifactを動かす。Critical profile programにはnetwork capabilityを付与しないためHTTPを含まず、p1 core moduleとしてbuildし続ける。VMにcomponent対応は不要で、閉じた命令集合も変えない。将来VMがcomponentを動かすなら別の記録とする。

<a id="65-pins"></a>

### 6.5 pin

`proofs/wasi-pin-policy.toml` [runtime].flagsを `-S http=y` にする。最低版46でp3は既定onなので `-W` の組と `-S p3=y` を除く。`scripts/check-wasi-pins.sh` と `tests/component_p3_test.rs` は同じPRで変更する。embedded hostの `wasmtime-wasi`、`wasmtime-wasi-http` pinを `wasmtime` と足並みを揃えて[runtime].crate_major（47）へ加える。wasmtime-wasi-httpのp3はexperimentalでp3専用fixはbackportされないため、patchでなくmajorと共にpinを進める。

<a id="7-implementation-plan"></a>

## 7. 実装計画

各stepをそのgate付きの1PRとし、以下の順序で進める。

| # | Step | Gate |
|---|---|---|
| 0 | als: C-328 / C-366 / C-367を改訂（§5）、header拒否契約追加、`proofs/als-pin.txt` を進める | `scripts/check-als-pin.sh` |
| 1 | **🚝なしshim**: `to_p3` の全同期 `stream.read/write` と `future.read/write` を `[async-lower]` と `waitable-set.wait` 上の共通「完了まで待つ」helperへ（fs先読みとhttp body pumpには既存ループあり）。pin flagを `-S http=y` へ | **`-W` なし**で `component_p3_test` green、`check-wasi-pins.sh`、test内で `cm_more_async_builtins` offのwasmparser検証 |
| 2 | **error分類**: rt-coreが失敗を `error-code` caseへ対応させ、rt-coreのrendererとshimの対応版を設ける。shimは固定 `(p3 transport)` 文でなくhostのcaseをrender | 拒否port / DNS / TLS fixtureの文言がnative ⇄ embedded ⇄ `wasmtime run -S http` で同一 |
| 3 | **HTTP programに必要なp3 shim対応**: `env.get` / `env.args` / `env.cwd`（`wasi:cli/environment@0.3.0`）、`env.sleep_ms`（monotonic `wait-for`）、`env.os` / `env.temp_dir` は定義した回答か理由付きE081 | それらopのp3-component行を削除 |
| 4 | **shim内client完全化**: 6つの `*_response`（`response.get-headers` → `fields.entries`）、`request_stream`、call handle（`start` / `poll` / `read_new` / `wait` / `cancel`、`request_stream_with_limits`）をasync `send`、async body read、monotonic deadline上に（§4.3） | 各native ⇄ p3 cross fixture、行削減、`pending_port_ceiling` 低下 |
| 5 | **SSE familyをself-host**（`anthropic_streaming_call`、`openai_streaming_call`、2つの `_with_limits`）: call handle上のAlmideにし `almide_rt_sse_*` intrinsicを削除 | 全経路から4行が消え、interp bridgeも対応（3-way oracle） |
| 6 | **共有層の監査修正**: stdlib URL parser（#2821）、CR/LF/禁止header拒否（#2822）、guest body limit（#2823 client側）、rt-core proxy（#2819）、trust root（#2820）、connect timeout/size cap（#2825）、1xx/chunk extension（#2824） | 各issueの再現をnative/embedded fixture化。stdlib検査はp3でも |
| 7 | **wasi:http上のembedded host**: `almide-wasm-run` がrt-core `send_request` hookで `wasmtime_wasi::p3` + `wasmtime_wasi_http::p3` をlink（§2.2）、`almide run --target wasm` がHTTP programのp3 componentを実行 | op 43..=50と53..=59を**未登録**にして `tests/embedded_cross_test.rs` green |
| 8 | **server export**（ADR-0020 step 7の対象変更）: `wasi:http/handler@0.3.0`、ADR-0020 §5.4形状規則、`backpressure` 直列化、shell `max_body_bytes`、embedded serve loopが `http_server_core` でexportを駆動（§3.4） | `tests/http_serve_cross_test.rs` 3経路: native、embedded、`wasmtime serve app.wasm`（flagなし）。16並行probeでinstanceあたり実行中handler1つを示す |
| 9 | **既定切替**: `almide build --target wasm` が `--component` / 環境スイッチなしにHTTP programをp3 componentへ。`target-availability.toml` 再構成（§6.2）、E081文言（§6.3） | `scripts/check-target-availability.sh`、E081診断fixture |
| 10 | **廃止**: host op 43..=50、53..=59、70..=72を廃止。`http_call_host.rs`、`host_serve.rs` と `host.rs` armを削除。C-328 / C-366 / C-367証拠をcomponent fixtureへ移す | `almide-wasm-run` がHTTP opを登録しないgrep gate |
| 11 | **文書化**: CHEATSHEET / llms.txt / `docs/stdlib/http.md` に配備（`wasmtime run -S http`、`wasmtime serve`）を記し、`docs/roadmap/on-hold/wasm-http-client.md` を閉じる | `scripts/check-llm-surface.sh` |

step 1、2、6は互いに独立。step 7は1と2に依存。step 8はADR-0020 step 0a–2（#2664、#2696、#2697、#2698）に依存。step 9は3、4、7、step 10は7、8、9に依存する。従来emitter codeは一切追加しない（#1696）。shimはstructural経路のmoduleで動く。

<a id="8-alternatives"></a>

## 8. 代替案

- **host opを維持し不足関数をop追加で補う**（現状）。1programに2ABIとなる。embeddedはstock hostが動かせない応答を続け、新関数をhostとshimに2度書く。
- **embeddedでwasmtime-wasi-http既定transport（hyper + rustls）を使う。** hookは1つ減るがnative/embeddedがclientを共有しなくなる。C-328のerror文と#2819/#2820/#2824/#2825のfixが、本来同一であるべき2経路で分岐する。
- **先にP2段階**（`wasi:http@0.2`、`incoming-handler`）。owner裁定で拒否。shim対象も第2interfaceになる。P2 client（`outgoing-handler`、`wasi:io` pollable）はP3（`client.send`、component-model stream）と何も共有しない。唯一の利点だった `-W` なしloadはなくなった（発見1）。
- flagなしp3のため**wasmtimeを更新**。不要。flagはguest由来だった（発見1）。最低46、pin 47を保つ。
- **全programをcomponentにする。** 利得のないprogramに1.3 KB → 18 KBを負担させ、wasmtime LTS 36を落とし、#865 VMから入力形式を奪う。
- **serverに `wasi:sockets`**（#2659案1）。serving platformについてADR-0020 §10で却下済み。その理由を維持する。

<a id="9-refinements-the-evidence-forced"></a>

## 9. 証拠により必要となった精緻化

1. **「P3にはexperimental flagが必要」はP3でなくAlmide shimの性質だった。** gateされるのは🚝同期builtinだけで、async-builtin handlerはwasmtime 47でflagなしserveできる（§2.1）。wasmtime更新の理由とADR-0020 §5.3の前提をなくした。
2. **「独自opなし」はhostが提供するopなしを意味する。** 生成codeとguest shimのop番号はartifactの外へ出ない内部ABI（§1.1）。
3. **embedded hostはrt-core transportを維持する。** hyperでなく `send_request` hook経由で `wasi:http` を実装するため、native/embeddedが1つのclientを共有し続ける（§2.2、§8）。
4. **error一致をguestへ移す**: 1つの `error-code` 分類とrendererをnativeも採用する（§4.2）。
5. **componentを既定にするのはHTTP使用programだけ**（§3.1項目3）。
6. **p3 shimは先にenvとargsを提供する必要がある**。そうしないとLLM clientをbuildできない（§2.3）。
7. **instance内直列化はbaseline builtin `backpressure.inc/dec` を使い**、guest queueではない（§3.3）。
8. **SSE関数はshim codeでなくcall handle上のAlmideになる**（§3.1項目1）。

<a id="10-consequences"></a>

## 10. 帰結

得られるもの:

- `wasmtime run`、`wasmtime serve`、Spin、wasmCloud、jco、`almide run --target wasm` 用に1つのHTTP artifact。
- serverを含む全 `http` 関数がwasmでbuild可能になり、一方のwasm経路だけで提供される関数がなくなる。
- 監査fixは層ごとに1回。programの観測にはstdlib、transportにはrt-core。
- `almide-wasm-run` から3つのop familyと約320行のhost code（`http_call_host.rs`、`host_serve.rs`）がなくなる。

コスト:

- embedded hostに約64 crate追加（tokio、hyper、wasmtime-wasi、wasmtime-wasi-http）し、wasmtimeのexperimental p3 moduleに依存する。
- HTTP artifactは約30 KBのcomponentになりWASI 0.3 hostを必要とする。それらにはLTS runtimeを使えない。
- native transportのerror文言を一度変更する（§4.2）。
- stock hostのproxy/trust rootはAlmideでなくhostのもので、契約は約束せずそのことを明記する。
