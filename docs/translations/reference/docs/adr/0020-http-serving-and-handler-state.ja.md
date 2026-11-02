<a id="adr-0020-a-served-handler-cannot-reach-a-var-state-lives-in-a-host-independent-kv-store-the-app-is-a-value-and-every-host-is-a-thin-shell"></a>

# ADR-0020: serveするhandlerは `var` に到達できない。状態はhost非依存KV storeに置き、appは値とし、全hostを薄いshellにする

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0020-http-serving-and-handler-state.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0020-http-serving-and-handler-state.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0020-http-serving-and-handler-state.zh-CN.md)

> 非規範的な歴史読者スナップショット。変更していない固定原本が正本。自動同期・独立benchmark・現行host/安全性保証・実行許可ではない。日付層を保持する。当初の「未実装」文と後日のstep 2 E095実装追記は併存し、epoch 6は一般化var規則、epoch 11はserve appのinstance閉包拒否である。旧P2 export本文は冒頭ADR0023改訂で置換済みとして保持し、現行P3要件とはしない。10月23日の接続I/O poolはhandler並列化/worker app factory/worker RNGを実装した意味ではなく、その注記で未完のまま。stock KV可用性、host比較、影響ゼロ数値は当時の証拠で今回の再検証ではない。HTTP意味論一致はhost管理framing、timing、request間log順を除き、KV受理はconformance条件付き。原文診断・例・コードコメントはbyte不変。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md)

- **状態**: 採択済み。メンテナが2026-09-27に方向性（下記4本柱）を決めた。本記録はその方向を正確にし、証拠で裏付け、実装順序を示す。まだ何も実装されていない。証拠によって必要になった精緻化は該当箇所で**[refined]**と記し、§9にまとめる。
- **2026-09-28改訂**: §5.3「どのexport形状か」（P2 `incoming-handler@0.2`）とstep 7/9の順序は[ADR-0023](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0023-wasm-http-over-wasi-http.md)が置き換える。stock serverは `wasi:http/handler@0.3.0` をexportする。P3が `-W component-model-more-async-builtins` を要するというP2の前提は、probeの同期stream builtinに由来した。async-builtin P3 handlerはwasmtime 47でflagなしにserveできる（ADR-0023 §2.1、§3.3）。
- **step 2実装済み（#2698）**: §5.2のinstance閉包規則は**E095**（`docs/diagnostics/E095.md`）。`crates/almide-frontend/src/concurrent_reach.rs` のconcurrent-slot探索で検査するため、wrapperのparameter経由で渡されたappはwrapperのcallerで検査される（§3.1）。hintはtop-level `let` とhandler-local値を名指しする。§5.2のkv節は `kv` moduleと共に加わる（step 3）。`spec/serve_cross/http_serve_replay.almd` は `/boot` を生成しhandler内で時計を読む。拒否はdialect epoch 11（`served-app-is-instance-closed`）。
- **日付**: 2026-09-27
- **範囲**: `http.serve` と `http` router family、`fan` body、新しい `kv` stdlib module、serverの `almide build --target wasm` 形状、関連契約台帳行（C-367と新規行）。
- **関連**: #2665（native serve逐次）、#2659（serverのstock wasm artifact）、#2664（structural経路のeffect fn値、前提PR #2693）、#2692（SIGTERMでstdout消失）、#2588（router、完了。#2665/#2659は分離issue）、#2594（`fan.map` effect callback逐次）、#1696（旧wasm emitter廃止）、[ADR-0011](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0011-execution-substrate-is-a-free-variable.md)（基盤は自由変数）、[ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md) §D6（effectはfallible）。
- **証拠の基盤**: `../almide-references/RESEARCH-http-serve-concurrency.md`（2026-09-27、測定23構成M1–M23、ソース読解18システムS1–S18、probe/raw出力は `../almide-references/research-2026/http-serve-concurrency/`）、§4のwasi:keyvalue、wasmtime、Spin、Workers KV、Durable Objectsの一次資料読解、#2659試作（branch `stock-wasm-serve-2659`、`research/spike/p3-http-handler-probe/`）、§6の移行測定（`research/spike/concurrent-var-reach/reach.py`）。

<a id="1-context"></a>

## 1. 背景

<a id="11-one-handler-four-answers"></a>

### 1.1 1つのhandler、4つの応答

probe handlerはその言語の可変状態保存先にcounterを置き、増加させ、`/slow` では2 s sleepし、再びcounterを読む。driverは `/warm` の後、t=0で `/slow`、t=0.3 sで `/fast`、その後逐次3回の `/seq` を送る。

| 観測応答 | `/slow` n→after | `/fast` latency | 測定先 |
|---|---|---|---|
| 共有、逐次 | 2→2 | **1.70 s** | Almide 0.64.0 native、captured/top-level `var`（M1、M2）、embedded wasm（M3）、Node busy-wait（M5）、Python `HTTPServer`（M9）、asyncio + `time.sleep`（M12）、Rust std threadなし（M17） |
| 共有、並行 | 2→**6** | 0.00 s | Node `await`（M4）、Go `net/http`（M6、`-race` はruntimeに `DATA RACE` を報告、M7）、Python threading（M8）、asyncio（M11）、Rust `Arc<Mutex>`（M16）、axum（M19、M20） |
| 分離 | **1→1**、全requestが `n=1` を見る | 0.00 s | Python `ForkingMixIn`（M10）、Rust `thread_local!` + 接続ごとthread（M14、まさにAlmideのtop-level `var` lowering）、requestごと新instanceの `wasmtime serve` P2とSpin P2（S12、S13） |
| workerごと分割 | 1, 1, 2, 2、担当worker次第 | 0.00 s | Rust `thread_local!` + 4 thread pool（M15）、wado `serve`（S9）、Actix（S16）、Workers global（S11） |

Almide自身のhostが既にこのうち3行を含む。

- nativeとembedded（`almide run --target wasm`、C-367）は逐次。
- P2 componentの `wasmtime serve` は分離。
- P3 componentの `wasmtime serve` は1instanceを最大128 request、同時16で再利用する（S12。#2659試作ではwasmtime 47.0.2でguest counterが1, 2, 3となることを測定）。
- Workersはrouting保証なしに「並行requestを含め」isolateを再利用する（S11）。

**可変状態を観測できないhandlerだけが全hostで同じ応答になる。** `var` counterはAlmide自身のtarget間でもportableではない。

<a id="12-what-the-close-peers-do"></a>

### 1.2 近い言語の動作

調査41システムのどれもhandler bodyの触る対象でserve modeを変えない（研究§4.3）。Almideに近い言語は共有可変状態を表現不能かcompile errorにする。

- **表現不能**: Gleam（S1）、Erlang/Elixir（S2）、Roc（S4: 永続可変状態はSQLiteか外部serviceへ）、aver（S7: 設計上共有可変状態なし）、Koka（S5: local `var` はescapeできない）。
- **compile error**: Rust（threadへの `Rc<Cell>` がE0277、M13、axum `State<Rc<Cell>>` でも同様、M18）、Swift 6（`mutation of captured var 'counter'
  in concurrently-executing code`、M21、`main actor-isolated var ... can not be
  mutated from a nonisolated context`、M22）。

Rustの `Send` 検査はAlmideのtop-level `var` 形状（`thread_local!`、M14/M15）を捕まえ**ない**。その形はcompileでき、黙って状態が分かれる。

<a id="13-what-almide-does-today"></a>

### 1.3 当時のAlmide

- `fan { … }` はcaptured local `var` をE008で拒否する（研究probe `raw/almide-fan-var.txt`）。E008はtop-level varや呼ぶfn経由のvarへ届かない。
- `var` に到達するcallback（captured、top-level、呼ぶ `effect fn` 経由）の `fan.map` は**受理され逐次実行する**。loweringが並列化するのはSend-safeスカラーcapture付きpure lambda literalだけ（`runtime/rs/src/fan.rs:23-40`、#2044）。内容依存切替であり、両版が同結果を返すからのみ安全。#2594の代償としてeffect callbackは常に逐次である。
- native `http.serve` は1接続ずつ応答する（`runtime/rs/src/http.rs:736-743`）。handlerは `Send` でない `Rc<dyn Fn>`。

Almideのmission指標はmodification survival。最悪なのは編集で黙って動作が変わること。暫定案「handlerが `var` に到達しなければ並行、到達すればwarning付き逐次」はその例で、`counter = counter + 1` を1つ加えるだけでserver latencyが切り替わり、signalはwarningだけになる。他の言語はそうしない。

<a id="2-decision"></a>

## 2. 決定

4本柱。

1. **並行hostへ渡す関数は `var` に到達できず、checkerが強制する。** `http.serve` / `http.route` / `http.mount` / `http.wrap` のhandler/middlewareと全 `fan` bodyが対象。captured、top-level、推移的に到達するvarを全て拒否する（§3）。E008を一般化しcodeを維持。effect（IO、random、clock、http client）は許可したまま。`fan.map` の黙った逐次化の穴を塞ぐ。
2. **request間状態は明示的host非依存KV storeだけに置く**（§4）。`kv` moduleがpure `f` に対するatomic `update(key, f)` を持ち、keyごとlinearizableとする。nativeはlock付きin-process map、embeddedはhost mapに置く。**[refined]** stock wasm hostはconformance fixture合格時だけbackingになれる。現在該当はないため、1つ現れるまで `almide build --target wasm` の `kv` はE081（§4.4）。
3. **appは値、entry pointは薄いhost shell**（§5）。`let app = http.router([...])` をtop-level値とし、`main` の `http.serve(port, app!)` をnative/embedded shellとする。`almide build --target wasm` は同appを `wasi:http/incoming-handler` としてexportし、#2659のp3-sockets案を置き換える。**[refined]** serve式はtop-level itemで閉じ、`main` のlocalをcaptureできない（§5.2）。export hostで `main` はentry pointでない（§5.4）。app testは通常callでよい: `let h = app!` の後 `h(http.new_request(...))!`。
4. **契約はbyte-levelでなくHTTP意味論**（§7）。C-367を「response byte同一」から「status、header集合、body同一」へ変更。log行はatomicで、並行request間の順序は未規定。native serveは並行: worker pool、workerごとapp instance、workerごとRNG再seed（§5.5）。SIGTERMは実行中requestをdrainし出力をflushする（§5.6）。limitはrecordで渡す（§5.7）。

<a id="3-the-checker-rule-pillar-1"></a>

## 3. checker規則（pillar 1）

<a id="31-sites"></a>

### 3.1 site

**concurrent slot**は、引数が自身や兄弟の他invocationと同時に動きうるparameter。stdlibが宣言する。

| 表層 | concurrent slot |
|---|---|
| `http.serve(port, f)` と `http.serve_with_limits(port, f, limits)` | `f` |
| `http.route(pattern, handler)` | `handler` |
| `http.mount(prefix, sub)` | `sub` |
| `http.wrap(handler, middleware)` | `handler`、`middleware` の全要素 |
| `http.router(routes)` | 自前ではなし。handlerは既にsiteである `route` / `mount` 経由で入る |
| `fan { a, b, … }` | 全arm |
| `fan.map` / `fan.settle` / `fan.any` / `fan.any_map` / `fan.race` / `fan.timeout` / `fan.bounded` | 全fn値引数（callbackまたはthunk list） |

宣言はparameter上のstdlib属性（綴りは実装PRが決め、`@concurrent` は仮称）。関数名には依存しない。**site**はconcurrent slotの1つの引数式 `E`。

**user wrapper経由のslot推論。** `let` をたどった後の `E` が外側fnのparameter `p` なら、`p` をそのfnのconcurrent slotとする。そのfnの全call siteへ検査を移し、固定点まで繰り返す。例: `fn serve_on(port: Int, h: HttpHandler) = http.serve(port, h)` は `serve_on` の `h` をconcurrent slotにする。推論slotはfn interface（`almide compile`）に属しpackage境界も越える。

<a id="32-the-executable-closure-of-a-site"></a>

### 3.2 siteの実行可能クロージャ

**body**はfn body、lambda body、`fan` arm。

lambdaがstdlib関数の*concurrent slotでない*callback parameterへの直接引数なら、構文上それを含むbodyへ**fold**する。その関数はreturn前にcallbackを呼び保存しない: `list.map`、`list.fold`、`option.map`、`map.fold` など。foldしたlambdaは外側bodyのinvocation内で動く。それ以外は独自body。

**実行可能クロージャ** `X(E)` は以下を満たすbodyの最小集合。

1. `E` 内に構文上現れる全unfolded lambda（`E` が `fan` armなら `E` 自体）を `X` に含む。
2. `X` 内のbodyがtop-level fn `g` を名指せば、`g` を呼ぶか値として使うかにかかわらず `g` のbodyを `X` に含む。
3. `X` 内bodyか `E` 自体がtop-level `let` `x` を名指せば、`x` のinitializerのunfolded lambdaと、そのinitializerが名指すtop-level fnのbodyを `X` に含む。record/list内closureにもこうして到達する: `let app = http.router([http.route("GET /", h)])` は `app` 経由で `h` のbodyを `X` へ入れる。
4. 名指すbodyの外側で束縛されたlocal `let` も同様。
5. `X` 内bodyに構文上現れる全unfolded lambdaを `X` に含む。呼ばれる可能性があるため保守的な規則とする。

<a id="33-violation"></a>

### 3.3 違反

`X(E)` のあるbody `B` が、`var` `v` を読み取りまたは代入として名指し、`v` が `B` の外で宣言されているとき、`E` は `v` へ**到達する**。

この1節で各形状を扱う。

| 形状 | `B` 外にある理由 |
|---|---|
| `main` の `var c` をhandler lambdaがcapture | `c` は `main` bodyで宣言、lambdaは独自body |
| top-level `var` | 全body外で宣言 |
| handlerが呼ぶ `effect fn bump()` 内で書くtop-level `var` | `bump` bodyは `X` 内（規則2）、varはその外 |
| factory `effect fn mk() -> HttpHandler = { var c = 0; (req) => { c = c + 1 … } }` | 戻すlambdaはunfolded（規則5）で独自body、その外に `c` がある |
| 外側 `var` を読む `fan` arm | armは独自body |

違反でないもの:

- handler内やhandlerが呼ぶfn内で宣言する `var` はinvocationごと。
- そのfn内のfolded lambdaが使う `var`、例えば `var c = 0; xs |> list.map((x) => x + c)` もinvocationごと。
- site外でvar値をコピーする `let`、例えば `let scale = scale0` はsnapshotであってreachではない。これは既存E008 fix-itそのもの（`tests/diagnostics/e008-lambda-callback-capture/fixed.almd`）。

effectは直交する。siteはIO、乱数、時計読取り、http client、`kv` を使用してよい。

規則はinterface summary付きpackage内検査。各top-level fnがcompiled interfaceに2情報を持つ: 「top-level varへ到達する」（witness path付き）と推論concurrent slot。そのため自packageのtop-level `var` に触るeffect fnを公開すると、それを呼ぶ下流handlerはそのfnを名指すwitness付きで失敗する。

<a id="34-diagnostic"></a>

### 3.4 診断

codeは**E008**のまま。titleを「fan block captures mutable variable」から「並行実行bodyが `var` に到達する」へ一般化する。3変種それぞれでsiteとwitness pathを名指す。

```
error[E008]: this handler can reach `hits`, a `var` — handlers run concurrently
  --> app.almd:14:3
  in the handler passed to http.route (reached through fn `bump` at app.almd:4)
  hint: a handler cannot share mutable state. Keep state that must outlive a
        request in a kv store: `kv.incr(counts, "hits", 1)!`. For a value fixed
        before serving, use a top-level `let` instead of `var`.
```

```
error[E008]: this fan.map callback can reach `total`, a `var` — fan bodies run concurrently
  hint: return each element's contribution and combine the results after the fan,
        e.g. `fan.map(xs, f)! |> list.sum`.
```

`fan { … }` blockの文言は現状を維持する。

```
error[E008]: cannot capture mutable variable 'count' inside fan block
  hint: Use a `let` binding instead of `var` for values shared across fan expressions
```

fix-it判定:

- 到達varがprogramのどこでも代入されないなら**mechanical**: `let` 宣言へ直す。
- それ以外は**conditional**: site前で `let` copyを束縛（read-only使用）、または状態を `kv` へ移す。

実装PRで `docs/diagnostics/E008.md` を書き換える。

**既存codeとの関係。** E011（pure function内closureがmutable variableを変更）は残す。上のfactoryが `effect fn` を必要とする理由であり、E008より前に実行される。新規則は既存E008全caseを包含する。analyzerは9つの `tests/diagnostics/e008-*` fixtureのbroken site全12を検出し、fixedは一切検出しない（§6）。したがってE008をinferrerの `fan` arm（`crates/almide-frontend/src/check/infer_calls_closures.rs:215-234`）から、型推論後のsite一括passへ移す。

<a id="35-staging"></a>

### 3.5 段階

この規則は今compileできるprogramを拒否する。dialect台帳の基準「従来受理programを拒否」（`proofs/dialect-epochs.toml:13-15`）によりepochとなる。**決定: warning releaseを挟まず、新dialect epoch 6でerrorにする。** 理由:

1. 測定影響は既存手書きprogramゼロ（§6）。
2. epoch 4/5が先例で、0.63.0でそれぞれ即errorとなった。
3. warning期間があると、並行native serve（§8 step 4）がM14/M15の応答を黙って出しうるreleaseを少なくとも1つ出荷する。並行serveになる**前**にerrorでなければならず、計画順序が決まる。
4. 研究自身の判定: warningはLLM編集ループで最も見落とされやすいsignal（研究§6）。

epoch項目の `breaks` 行: 「http handlerか `fan` bodyから到達可能な `var`（captured、top-level、呼ぶfn経由）はE008。request間状態は `kv`、read-only使用は `let` copyを束縛する」。

<a id="4-the-store-pillar-2"></a>

## 4. store（pillar 2）

<a id="41-what-the-backends-actually-offer-primary-sources"></a>

### 4.1 backendが実際に提供するもの（一次資料）

| Backend | API | 明記された保証 | keyごとlinearizableな `update` を担えるか |
|---|---|---|---|
| **wasi:keyvalue**、Phase 2（README.md:7）。mainは `@0.2.0-draft2`、唯一のtagは `v0.2.0-draft` | `store.open(identifier) -> result<bucket, error>`、`bucket` 上に `get`、`set`、`delete`、`exists`、`list-keys(cursor)`。draft2の `atomics` は `resource cas { new(bucket, key); current() }`、`swap(cas, value) -> result<_, cas-error>`、`increment(bucket, key, delta: s64) -> result<s64, error>` を追加。tag付きdraftには `increment(…, delta: u64) -> u64` だけ。`batch`: get/set/delete-many | store.wit:1「結果整合のKV操作」、store.wit:14-25「自分の書き込みを読めるだけの整合性が必要…同clientのみの保証」、atomic.wit:3-5「成功するか何もしないか」、batch.wit:10-11「atomicityは保証しない」。未決PR #56はstore保証を結果整合のみへ弱める。未決PR #59は同keyで `increment` と他操作の混在は「実装依存」とする | draft2 `cas` loop経由だけ。提案は「linearizable」と一度も述べない |
| **`wasmtime serve` 47**（`-S keyvalue`） | `wasi:keyvalue@0.2.0-draft`（wasmtime `crates/wasi-keyvalue/wit/deps/keyvalue/world.wit:1`）、in-memory backend、identifierは `""` のみ（lib.rs:164-171） | `open` は `Bucket { in_memory_data: self.ctx.in_memory_data.clone() }` を返す（lib.rs:166-167）。各bucketはpreset dataの**private copy**。`serve` はinstanceごと新context（`src/commands/serve.rs:443-457`）、P2 reuse既定1 | **不可。** 書き込みは次requestにも同requestの2回目の `open` にも見えない。CASなし |
| **Spin 3.x / 4.1** | `wasi:keyvalue@0.2.0-draft2`（`wit/deps/keyvalue-2024-10-17/`）、`spin:key-value@3.0.0` store（atomicsなし）、旧 `fermyon:spin/key-value`。`"default"` storeはSQLite（runtime-config lib.rs:459-503） | docsは整合性/atomicityを述べない（kv-store-api-guide）。SQLite `increment` は `Mutex<Connection>` 下でtransactional、**8-byte i64 LE**で保存。他長のkeyではhostがpanicする（`crates/key-value-spin/src/store.rs:326-359`）。CAS `swap` は値を比較する（l.376-457）が、`current()` がkeyなしを見た場合は**無条件upsert**（l.436-446） | 既存keyなら可。**初回insertは不可**: 競争する2creatorが共に「成功」 |
| **Cloudflare Workers KV** | `get` / `put` / `delete` / `list` | 「結果整合で高性能を実現」、変更は「他のglobal network locationから見えるまで最大60秒またはそれ以上」、「atomic操作が必要な用途に不向き」、「同keyへのwriteは毎秒最大1回」（developers.cloudflare.com/kv/concepts/how-kv-works/、/kv/api/write-key-value-pairs/） | **不可。** increment/CASなし、最後のwriteが勝つ |
| **Cloudflare Durable Objects** | `ctx.storage.kv.get/put/delete/list`、`transaction`、`transactionSync`、`sql.exec`、`idFromName(name)` でrouting | 「durable、transactional、強整合storage」、「single-threadedで協調multitask」、「他I/Oを挟まない連続read後の連続writeは自動atomic」（developers.cloudflare.com/durable-objects/api/sqlite-storage-api/、/concepts/what-are-durable-objects/） | **可**、ただしJS Durable Object class経由であってwasm host importではない。Workersには公式wasi:http component対応もない（「WASI support is experimental…一部syscallのみ」、/workers/runtime-apis/webassembly/） |
| **Deno KV**（設計参考のみ） | versionstamp付き `atomic().check(...).set(...).commit()`、`KvU64` の `sum` | 「指定versionstamp一致時だけcommit」、writeは「常に強整合」（docs.deno.com/deploy/kv/transactions/） | 可。`versionstamp: null` のcheckで「不存在」もversionにし、Spinの初回insertの穴を避ける |

設計を決める事実は2つ。

- `wasmtime serve` はwasi:keyvalueでhandler状態を全く保持できない。
- 2つのstock wasi:keyvalue hostは**異なるpackage版**を実装する（`@0.2.0-draft` 対 `@0.2.0-draft2`）。一方をimportするcomponentは他方でlinkできない。

<a id="42-the-almide-surface"></a>

### 4.2 Almide表層

```almide
import kv

/// A handle naming a store. Pure: no IO, so it can be a top-level `let` and
/// captured by any handler.
fn store(name: String) -> KvStore

effect fn get(s: KvStore, key: String) -> String?
effect fn set(s: KvStore, key: String, value: String) -> Unit
effect fn delete(s: KvStore, key: String) -> Unit
/// Atomic read-modify-write, linearizable per key. `f` gets the current value
/// (none if absent) and returns the new one (none deletes the key). The result
/// is the value now stored. `f` is PURE (a plain arrow type), so a backend that
/// retries may call it more than once, and nobody can tell.
effect fn update(s: KvStore, key: String, f: (String?) -> String?) -> String?
/// `update` over the decimal text of an Int, defined in Almide over `update`
/// and never mapped to a host `increment` (the encodings differ between hosts,
/// §4.1). A missing key counts as 0; a non-Int value is err and is left
/// unchanged; the arithmetic wraps like `+`.
effect fn incr(s: KvStore, key: String, delta: Int) -> Int
```

全操作はeffectかつfallible（ADR-0002 §D6）。callerは `!` を書く。errorはwasi:keyvalueの `error`（`no-such-store`、`access-denied`、`other(string)`）を `err` 文字列として反映する。

**[refined] 命名。** moduleは `kv`、handle型は `KvStore`。`store` / `Store` にしない測定上の理由は2つ。

- moduleと同名localは黙ってshadowする（`import json` 下の `let json = "x"` はE060だけで `almide check` を通る）。`store` は一般的local名。
- `http` は既にuser型と衝突する裸のnominal名を避ける（`stdlib/http.almd:4-6`:「`Request`/`Response` という名にしない」）。

適合stock hostが提供すべきWIT形状は次のwasi:keyvalue draft2 subset。Almide shellはその上のCAS loopに `update` を対応させる。

```wit
// imported from wasi:keyvalue@0.2.0-draft2
store.open: func(identifier: string) -> result<bucket, error>;
bucket.get: func(key: string) -> result<option<list<u8>>, error>;
bucket.set: func(key: string, value: list<u8>) -> result<_, error>;
bucket.delete: func(key: string) -> result<_, error>;
atomics.cas.new: static func(bucket: borrow<bucket>, key: string) -> result<cas, error>;
atomics.cas.current: func() -> result<option<list<u8>>, error>;
atomics.swap: func(cas: cas, value: list<u8>) -> result<_, cas-error>;
```

2点を意図的に除く。

- `increment`: encodingはhost定義（wasmtimeは十進text、Spinは8-byte LE）。`get` との混在は実装依存（PR #59）。
- `list-keys`、batch、TTL: 正しいcounter/session/cache（下記）に不要。`list-keys` は「古い一覧を示してもよい」（store.wit）、batchは「atomicity非保証」、TTLは上記backend1つだけにある。いずれの追加も別記録とする。

<a id="43-why-this-is-the-minimal-set"></a>

### 4.3 最小集合である理由

- **counter**: `kv.incr(hits, "total", 1)!`。linearizableなのでN並行requestの終了値は正確にN。
- **session**: loginで `kv.set(sessions, id, data)!`、各requestで `kv.get(sessions, id)!`、logoutで `kv.delete(sessions, id)!`。各keyは同時1writerなのでupdate不要。
- **cache**: `f` の外で値を計算（effectは外）、次に `kv.update(cache, url, (cur) => some(cur ?? fetched))!`。最初のwriterが勝ち、競争する全requestは保存値を返す。

完全なprogramとtest:

```almide
import http
import kv

let counts = kv.store("counts")

effect fn hit(req: HttpRequest) -> HttpResponse = {
  let n = kv.incr(counts, "hits", 1)!
  http.response(200, "n=${n}")
}

let app = http.router([http.route("GET /hit", hit)])

effect fn main() -> Unit = http.serve(8080, app!)!

test "two hits count to two" {
  let h = app!
  let _ = h(http.new_request("GET", "/hit", "", map.new()))!
  let r = h(http.new_request("GET", "/hit", "", map.new()))!
  assert_eq(http.body(r), "n=2")
}
```

<a id="44-per-backing-semantics"></a>

### 4.4 backing別の意味論

| 実行場所 | Backing | 整合性 | 耐久性 | 可視性 |
|---|---|---|---|---|
| native（`almide run`、build済binary） | process-globalのshard map、shardごと `Mutex`。`f` はkeyのshard lock下で正確に1回実行 | 全操作でkeyごとlinearizable | process寿命、exitで消失 | 全worker threadに即時 |
| embedded（`almide run --target wasm`） | 新host op経由のalmide host process内map | 同じ | 同じ | 同じ |
| 両経路の `almide test` | **各 `test` block**ごと新しい空map、全name空開始 | 同じ | block | そのblockのみ。block内で `http.new_request` により呼ぶappはblockのmapを見る |
| stock wasm（`almide build --target wasm`） | **[refined]** conformance fixture（並行初回write競争、CAS retry競争、instance間永続）合格hostのwasi:keyvalue draft2 | 適合hostでkeyごとlinearizable | hostによる（Spin既定SQLite fileはdurable） | hostによる |

**当時のstock可用性: 合格hostなし。**

- wasmtime serve 47はrequest間や `open` 間に状態を保たずCASなし。
- Spinに初回insertの穴がある。
- Workersにwasi:keyvalueがなくKVはatomicでない。

したがって `proofs/target-availability.toml` で `kv.*` は**stock-p1/component経路のE081**。理由に本記録と不足conformanceを示す。stateless appはどこへもexportできる。stateful appはnativeかembeddedで動き、hostが合格すればexportできる。これが「全hostで正しい」の「正しいか拒否する」解釈である。他の解釈（`wasmtime serve` でrequestごと黙ってcounterをreset、Workers KVでupdate消失）はpillar 1が排除するM10/M15失敗である。

<a id="5-the-app-the-hosts-and-main-pillar-3"></a>

## 5. app、host、`main`（pillar 3）

<a id="51-one-program-every-host"></a>

### 5.1 1つのprogram、全host

```almide
let app = http.router([ http.route("GET /users/{id}", get_user), … ])   // Result[HttpHandler, String]
effect fn main() -> Unit = http.serve(8080, app!)!                        // the socket hosts' shell
```

top-level `let` initializerはeffect fnを呼べない（測定: `let port = env.millis()` がE006）。したがって `app` はprogram textのpure functionで、workerごと/instanceごと1回評価する違いは観測不能。Rust loweringは既に `Rc` 値を持つtop-level `let` を**threadごとのlazy slot**にする。`let app = http.router(...)` の生成codeは `__AlmideTl_APP` の背後の `thread_local! { static SLOT … = Box::leak(Box::new(almide_rt_http_router(...))) }`、`String` の `let` は共有 `LazyLock`（0.64.0の `--target rust` で測定）。workerごとapp instanceでもcodegenに新しいコストはない。

`router` は `Result` を返し、top-level initializerで `!` は不可（測定E022）。shellは `app!` と書く。

- native/embeddedはbind前に1度評価。壊れたroute表は従来通りlisten前exit 1。
- export hostはinstance初期化で評価し、`err` ならstderrのmessage付きで初期化失敗。

<a id="52-refined-the-served-expression-is-closed-over-top-level-items"></a>

### 5.2 **[refined]** serve式はtop-level itemで閉じる

`http.serve` / `http.serve_with_limits` の引数は**instance-closed**でなければならない。実行可能クロージャ（§3.2）が読み、そのbody群のどれも束縛しないnameは全てtop-level item（fn、`let`、type、constructor）。export hostにmain localは存在せず（§5.4）、native worker threadも独自instanceを作る（§5.5）。実装PR割当の新code（E095、#2698）は次の通り。

```
error[E095]: the app passed to http.serve captures `boot`, a local of main
  hint: the app runs in several instances — one per worker natively, one per
        request on a wasi:http host — and main's locals do not exist there.
        Make `boot` a top-level `let`, compute it inside the handler, or keep it
        in a kv store.
```

これでinstance hostが守れないC-367の約束「mainが `serve` 前に計算した値（乱数、時計読取り）は同run全requestで同じ」を廃止する。研究はこれを未解決P2 divergenceと呼ぶ（研究§6「Out of scope for F」）。fixture（`spec/serve_cross/http_serve_replay.almd:52-56`、`boot`）はhandler-local値か `kv.update(meta, "boot", (b) => some(b ?? drawn))!` に移す。

<a id="53-hosts-and-what-the-program-can-observe"></a>

### 5.3 hostとprogramの観測対象

| Host | instance model | programが観測できるもの（pillar 1–2成立） |
|---|---|---|
| native（`almide run`、binary） | 1process、worker thread pool、各自app instance、同時最大 `max_in_flight`（§5.5） | response、`kv`、request間順序未規定の完全なlog行、timing |
| embedded（`almide run --target wasm`） | 1instanceでrequestが完全に届いた順に逐次実行。hostはnative poolで最大64接続を同時read（C-367、C-389、#4021） | 同じ応答。遅いhandlerは次requestを遅らせる（timingのみ）が遅いclientは遅らせない |
| `wasmtime serve` 47、P2 component（`wasi:http/incoming-handler@0.2.x`、wasmtime 47は `wasi:http@0.2.12` を搭載） | requestごと新instance、並行なし（P2の `--max-instance-reuse-count` 既定1、「WASIp2 componentは並行呼出し不可なので1超の設定は無効」、`serve --help`、serve.rs:141-155） | 同じ応答。`kv` はE081（§4.4）。headはhyperが書く（§7） |
| `wasmtime serve` 47、P3 component（`wasi:http/handler@0.3.0`） | 最大128再利用、instanceあたり最大16並行、idle 1 s後drop。`-W component-model-more-async-builtins -S p3 -S http` が必要（#2659試作測定: `-W` なしではload不可） | guest shellがinstance内handlerを直列化する条件で同じ応答（下記） |
| Spin 3.x / 4.x | P2はrequestごと新instance、3.6 P3は「並行にも直列にも」再利用（spinframework.dev/blog/announcing-spin-3-6） | 同じ応答、ここでは未測定 |
| Cloudflare Workers | 公式component/wasi:http対応なし、未検証の非公式経路（`jco transpile` + JS fetch handler）のみ | 本記録のartifact対象外 |

**export形状。** stock artifactはP3でなくP2の **`wasi:http/incoming-handler@0.2`** をexportする。

- P2は固定wasmtimeでexperimental flagなしload可能。
- P2はguestがinterleaveしないinstance model。
- P2は方向性に名指しされたinterface。

P3（`wasi:http/handler@0.3.0`）はwasmtimeが `-W` なしで提供するときの後段（§8 step 9）。P3ではhandlerがstream builtinでblockするたび1linear memory/1RC heap上で最大16handlerがinterleaveする（#2659試作）。pillar 1は*program*状態を非共有にするが、*runtime*のallocator、lazy-`let` slot、scratch bufferは中断点をまたぐ再入可能性が未証明。そのため全中断点の再入をgateで証明するまで、P3 shellは**instance内handlerを直列化する**（backpressure付きguest queue）。どちらも測定していない。直列化shellは全hostで正しいため選ぶ。一方 `--max-instance-concurrent-reuse-count 1` は他hostが従わないwasmtime flagである。

<a id="54-what-main-means"></a>

### 5.4 `main` の意味

- **socket host**（native、embedded）: `main` は従来通り1回実行。`http.serve` はbindしshutdown signalまでserveする（§5.6）。その後 `()` を**返し**、後続文を実行し、exit codeはmainのもの。mainの `serve` 前の文は1回実行するが、§5.2によりappに影響しない。
- **export host**: `main` はentry point**ではない**。componentは `incoming-handler.handle` をexportし、appをinstanceごとlazy評価、`incoming-request` を `HttpRequest` に変換、appを呼んでresponseを書く。
- **export build形状規則。** `almide build --target wasm` がexportするprogramの `main` bodyは、正確に1call `http.serve[_with_limits](port, APP[, limits])`。その前に `port` / `limits` だけで使う `let` を置いてよい。export hostではこれら引数を評価しない: hostがsocketとlimitを所有する。他の `main` 文は `--target wasm` のcheck-time error（実装PRがcode割当）。fix-itは「nativeでは動くがwasi:http hostでは動かない。setupをtop-level `let` またはhandlerへ移し、readiness行はhost自身が出すので削除」。新表層であり、今build可能なものを拒否しない: それまではstock buildの `http.serve` がE081。

「mainのserve前effectをinstance初期化ごとに動かし、idempotent必須」は選ばなかった。冪等性は検査不能、instance数はhostが選び、そのprologueの `kv` writeはP2でrequestごと観測可能に繰り返される。

<a id="55-native-concurrency-closes-2665"></a>

### 5.5 native並行処理（#2665を閉じる）

pillar 1–3成立後の最安の正しいmodel。

- **accept threadとworker thread pool。** workerを必要時に最大 `max_in_flight` まで起動し、受理した `Send` な `TcpStream` をidle workerへ渡す。全員busyならacceptを止め、kernel backlogをbackpressureにする。
- **workerは各自app instanceを作る。** compilerはinstance-closed serve式（§5.2）を通常の `fn() -> Rc<dyn Fn(…)>` **factory**へloweringする。fn pointerなので `Send`。workerは1度factoryを呼び、既存thread-local slot経由でtop-level letを読む。mainも先に1度式を評価して `err` ならbind前exitする。
- **`kv` はprocess-global**（§4.4）。値は所有された `Send` な `String`。Almide `Rc` はthreadを越えない。
- **stdoutをprocess-globalにする。** 現在の `ALMIDE_STDOUT_BUF` は `thread_local!` の64 KiB `BufWriter`（`crates/almide-codegen/src/lib.rs:346-359`）で、exit時flushはmain thread分だけ。worker出力が失われるため、`Mutex` 配下の1bufferにする。各 `print` / `println` はlock下でbyteをappendし**行はatomic**、terminal-flush/64 KiB規則は維持。stderrはunbufferedのまま、各 `eprintln` はRust stderr lock下の1 `write`。
- **workerごとRNG再seed。** 現在xorshift状態は `thread_local!` で `SystemTime` nanosecondをseedにする（`runtime/rs/src/random.rs:6-10`）。同tick開始の2workerは同streamを引く。各seedにOS entropy（可能なら `getrandom`、それ以外clock）、worker index、thread idを混ぜる。乱数は再現性を約束しないので、規定するのは異なることだけ: 生存worker2つはseedを共有しない。
- **forkは却下**。`Send` 不要でもin-process `kv` mapが子ごとになり、未flush親stdoutが全子に複製され、RNGもfork後同streamを続ける（研究§4.2）。
- **`Send` な値表現（全体Rc→Arc）は却下**。runtime全体のコストとなる（研究§5案C）。

embeddedは逐次のまま。応答はnativeと一致し、異なるのは新C-367が約束しないtimingだけ。

<a id="56-shutdown-closes-2692"></a>

### 5.6 shutdown（#2692を閉じる）

最初のSIGTERM/SIGINTでsocket hostは:

1. accept停止。
2. `request_timeout_ms` 内で実行中requestの完了を待つ。
3. stdoutをflush。
4. `http.serve` からreturn。

drain中の2回目信号は待機を省きflushしてcode 1でexit。embeddedもhostで同じ処理をしguestのserve loopがreturnする。exit codeはC-350の `0..=125` 内で、128+signal codeはない。

`almide run` は信号をprogramへ正確に1回届ける（#2809）。Unix native launcherはcompiled programを `exec` するため、supervisorが持つpidはprogram自身で、そのpidだけへのSIGTERMがdrainを開始し、terminal groupへのCtrl-Cもforwardする親により二重送信され強制停止になることはない。`almide run` の終了statusはprogram自身のもの（exit codeまたはsignal死としての報告）で、launcher自身はC-350が `process.exit` 用に予約する128+nを加えない。Windowsではlauncherが待機する。Ctrl-Cはconsoleの全processへ届くためforwardせず、program終了まで自身はeventを無視する。

<a id="57-limits"></a>

### 5.7 limit

```almide
type ServeLimits = { max_in_flight: Int, request_timeout_ms: Int, max_body_bytes: Int }
effect fn serve_with_limits(port: Int, f: effect (HttpRequest) -> HttpResponse, limits: ServeLimits) -> Unit
```

名前は `*_with_limits` family（`request_stream_with_limits`、`HttpLimits`）に従う。`http.serve(port, f)` の既定は同時64、request timeout 30 s、body 1 MiB。

| Field | native | embedded | export host |
|---|---|---|---|
| `max_body_bytes` | shared server coreが強制。大bodyは `413 Payload Too Large`、handlerは呼ばない | 同じ | 同じ。guest shellがbody streamを読むとき強制 |
| `request_timeout_ms` | 強制。超過後 `503` で接続close、handler結果drop | 同じ | host自身の値 |
| `max_in_flight` | pool sizeとして強制 | 逐次のため1 | host自身の値 |

export hostのlimitはhost設定（例 `wasmtime serve
--max-concurrent-requests`）。これはtiming/admissionであり、受理requestへの応答ではない。

**状態（2026-09-28、#2823）。** recordが存在する前からshared server core（`crates/almide-rt-core/src/http_server_core.rs`）が既定を強制するので、native/embeddedは共に#2823攻撃を拒否する。`Content-Length` 宣言かchunkedで1 MiB超bodyが来れば、宣言サイズをallocateせず `413`、handlerは呼ばない。`Content-Length` 未満で途切れたbodyは `400`。accept後30 sでreadできないrequestは `503`、それより遅いhandler responseも `503` に置換。coreはrequest/header行も制限（8 KiB: `414` / `431`）、header数も制限（100: `431`）、chunked request bodyをdecodeし他transfer codingに `501`。`ServeLimits` と `serve_with_limits` はworker pool（#2665）と共に入り、`http_server_next_with` へ値を渡す。

**状態（2026-10-23、#4021、C-389）。** 接続I/Oはshared coreの有界pool（`http_server_pool`）で実行する。accept threadが最大64接続を受け入れ64でaccept停止（kernel backlogがbackpressure）、各接続workerがrequest read/response write。request headはaccept後10 s以内に全体が届く必要があり、それ以外は `408`（何も送らないclientは無応答close）。bodyとhandler待ちは30 s request timeoutと `503` の下にある。handlerはまだ `serve` を呼んだthreadで、requestが完全に届いた順に1件ずつ実行する。§5.5のworkerごとapp factory、process-global stdout、workerごとRNG seedは依然#2665。そのため遅いclientは両socket hostを止めなくなったが、遅いhandlerは依然止める。

**listen address（#2826、#2829）。** `http.serve` は `0.0.0.0:<port>` bindを維持。C-367がそのaddressを明記し、container/VM配備serverはhost外から到達される。黙ってloopbackに変えればerrorなしに到達不能となる。addressを明示optionにし、`serve_with_limits`（#2829）と共に `ServeLimits` の `host: String` field、既定 `"0.0.0.0"` を追加する。

<a id="6-migration-measured-impact"></a>

## 6. 移行: 測定影響

`research/spike/concurrent-var-reach/reach.py` は `almide --emit-ast` 上に§3.1–3.3（escape/let-snapshot節含む）を実装し、2026-09-27にalmide 0.64.0で実行した。

先に検証した。

- varへ到達する研究probeを全て検出: 4つの `serve` probe（captured/top-level、native/wasm）と5つの `fan` probe（block、captured、read-only captured、top-level、`bump` 経由）。
- factory probe（自身の `var` を持つclosureを返す `mk()`）を検出。
- fn-local var付き `list.map` probeは通す。
- `tests/diagnostics/e008-*` ではbroken site正確に12を検出しfixedはゼロ。

| Corpus（commit） | `.almd` file | site付きfile | site | `var` へ到達するsite |
|---|---|---|---|---|
| `spec/`（develop b0d48b31b） | 1,446 | 58 | 282（`fan` arm 55、`fan.map` 122、`fan.any` 56、`fan.settle` 18、`http.serve` 2、`http.route` 21、`http.mount` 2、`http.wrap` 6） | **0** |
| `stdlib/` | 317 | 6 | 0。stdlibに**top-level `var` がない**ため推移的reachを持つstdlib fnなし | **0** |
| `research/benchmark/exercises/` | 25 | 0 | 0 | 0 |
| almai（316db10） | 50 | 0 | 0 | 0 |
| comide（fb9d654） | 20 | 0 | 0 | 0 |
| golemide（1f7c7f7） | 26 | 0 | 0 | 0 |
| gramide-cli（5f67679） | 1 | 1 | 依存packageを呼ぶ8 `fan` arm。gramide依存7つにtop-level `var` はない（`^var ` 一致はGo/JS sourceのheredoc test string内だけ） | **0** |
| almide-dojo bank: `tasks/`、`msr/`、`src/`（f58aaa2） | 123 | 1 | 0 | 0 |
| 他almide-org repo（playground 123、porta 36、bindgen 24、docs 16、web 6、lander 4、sqlite 3、js 1） | 213 | 0 | 0 | 0 |
| `tests/diagnostics/`（negative fixture） | — | 99 | 69 | 12、全て既存 `e008-*` broken fixtureで既にerror |

**var規則が壊す手書きprogramはゼロ。**

**instance閉包規則（§5.2）**は2つの `http.serve` siteのうち1つを壊す: `spec/serve_cross/http_serve_replay.almd` の `boot` captureはまさに廃止するC-367の約束である。docsの全serve例（`docs/stdlib/http.md` ×7、`docs/CHEATSHEET.md:351`）は既に閉じている。

**export形状規則**は何も壊さない。stock buildは当時 `http.serve` を拒否するため。

したがって§3.5通りepoch 6でerror、warning期間なし。

<a id="7-contract-changes-pillar-4"></a>

## 7. 契約変更（pillar 4）

これらは提案である。各々先にalmide/alsへ入れ、pinを進めてから実装する（CLAUDE.md、Behavior Contracts）。IDもそこで割り当てる。

<a id="71-c-367-rewritten-http-semantic-identity-across-every-serving-host"></a>

### 7.1 C-367改訂: 全serving hostのHTTP意味論的一致

> `http.serve(port, app)` は全ての受理requestへ、native、embedded wasm（`almide run --target wasm`）、および `almide build --target wasm` が受理するprogramの `wasi:http/incoming-handler` hostで同じ**status code、header集合、body**を返す。**header集合**はresponse fieldの(name, value)対で、nameはASCII大文字小文字を区別せず比較。異名fieldは順不同、同名fieldは相対順維持（RFC 9110 §5.3）。host管理fieldは除外: `date`、`connection`、`keep-alive`、`transfer-encoding`、`content-length`（framing。body byteはde-frame後に比較）。**reason phraseは契約外**。HTTP/2、HTTP/3にはなく（RFC 9113 §8.3.2、RFC 9114 §4.3.2）、hyperはcanonical phrase、native coreは自身の表を使う（#2659試作でnative `418 OK`、`wasmtime serve` では `418 I'm a teapot` を測定）。request reader（`req_method`、query込み `req_path`、`req_body`、`req_header`（最初の一致、ASCII大文字小文字無視）、`query_params`、`param`）は全hostで同値。handlerの `err(m)` はbody `Internal error: <m>` と `Content-Type:
> text/plain` の `500`。socket hostでbind失敗は `Error: bind failed:
> <os message>` とexit 1でabort。appはinstanceごと評価（nativeはworker threadごと、export hostは `wasi:http` instanceごと）。instance-closedで `var` に到達しないためinstance数は観測不能。timingと並行requestのserve順は約束しない。対象外: HTTP framing/接続再利用（host側）、stock hostの `kv` store（store契約参照）。

fixtureはreplay scriptを維持し `boot` を除き、header集合規則で3経路を比較する。§8 step 7後に `wasmtime serve` を加える。

<a id="72-new-rows-ids-allocated-in-als"></a>

### 7.2 新規行（IDはalsで割当）

- **serve並行処理とlog。** 「native `http.serve` は最大 `max_in_flight` requestを並行serveするので、sleepするhandlerは途中到着requestを遅らせない。各 `print` / `println` / `eprintln` callのbyteは連続してstreamへ届く。異なる実行中requestの行順は未規定、同requestの行はprogram順を保つ。worker thread2つでRNG seedを共有しない」。証拠: 2 sの `/slow` と0.3 s後の `/fast`（`/fast` が `/slow` より先に応答）のloopback fixture、多writer行完全性fixture。（ADR-0011 D1は `fan` arm出力を逐次評価に一致させる。その参照順はprogramが定めるarm順。並行requestにはnetworkが選ぶ入力の到着順しかなくprogram定義順はないので、serve log順はtransactionalでなく誠実に未規定とする。）
- **kv store。** 「`kv.update(s, k, f)` はnative/embeddedでkeyごとlinearizable。N並行 `kv.incr(s, k, 1)` 後は `N`。どのworkerでも完了した `set` / `update` 後の `get` はその値か後の値を返す。storeはprocess寿命。各 `test` blockは全store空で開始。stock `--target wasm` buildではhostがconformance fixtureを通るまで `kv` をcheck時拒否」。証拠: 64writer increment fixture（native/embedded）、testごとの分離fixture、E081可用性行。
- **shutdown。** 「SIGTERM/SIGINTでsocket hostはaccept停止、`request_timeout_ms` 内に実行中request完了、stdout flush、`http.serve` からreturn。2回目信号はflushしてexit 1」。証拠: print、request受信、stdout redirect中SIGTERM送信、全行確認するfixture（#2692再現）。
- **limit。** 「`max_body_bytes` 超bodyはhandlerを呼ばず `413`。`request_timeout_ms` 超requestは `503`」。証拠: native/embeddedの2fixture。
- **export artifact。** 「mainが単一serve callのprogramを `almide build --target wasm` すると `wasi:http/incoming-handler@0.2` をexportするcomponentになる。`main` は動かずappはinstanceごと評価」。証拠: 固定 `wasmtime serve` 下のC-367 replay。

<a id="8-implementation-plan"></a>

## 8. 実装計画

以下の順でstepごとgate付き1PR。step 1/2はstep 4より先（§3.5）。step 0は既に見つかった前提。

各stepのissue: 0a #2664、0b #2696、1 #2697、2 #2698、3 #2699、4 #2665、5 #2692、6 #2700、7 #2659、8 #2701、9 #2702。追跡issue #2705。

| # | Step | Gate | 解決/関連 |
|---|---|---|---|
| 0a | structural wasmで宣言fn型経由のeffect fn値を提供 | `proofs/target-availability.toml` router行のstructural wall除去 | #2664（進行中PR #2693） |
| 0b | wasmで `http.router` + handler callを動かす。現在 `serve` なしでも**両経路**E082（旧:「match over an UNTRACKED subject …」、structural: `ty-mismatch:Fn`、#2659試作 `call:__parts`、`stdlib/http.almd:410`）。**structuralだけ**で修正 | `spec/stdlib/http_router_test.almd` の `h(http.new_request(...))!`（`let h = app!` 使用）がnative/`--target wasm` でbyte同一 | #2696、7とwasmでappをcallしてtestするための前提 |
| 1 | 一般化E008（§3）: concurrent-slot属性、slot推論、実行可能閉包pass、escape/snapshot節、witness path、interface summary。`CURRENT_DIALECT = 6` でepoch 6 | §3.3各形状の `tests/diagnostics/` broken/fixed対（captured、top-level、fn経由、local `let` 経由、top-level `let` 経由、record/list経由、factory escape、推論wrapper slot、`fan.map`、`fan.settle`）、foldした `list.map` のfixed対、`scripts/check-dialect-epochs.sh`、`spec/` のreach.pyが0維持 | `fan.map` 黙った逐次化を閉じ#2594を解放（effect callback並列化は別決定） |
| 2 | instance閉包規則（§5.2）とcode、`spec/serve_cross/http_serve_replay.almd` から `boot` 移行。**先行**: C-367新文言（§7.1）のals PRとpin更新 | diagnostics対、header集合比較で両経路replay green | 4/7前提 |
| 3 | `kv` module（§4.2）、native backing、testごと新store、interp bridge（3-way oracle）、embedded host op、stock E081行。**先行**: als store契約 | native/embedded64writer fixture、test分離fixture、availability ratchet | pillar 2 |
| 4 | native並行serve（§5.5）: accept thread/worker pool、serve引数factory lowering、global stdout、worker RNG seed、`serve_with_limits` / `ServeLimits`（§5.7）。**先行**: als並行/limit契約 | slow/fast loopback（#2665完了条件）、行完全性、413/503 fixture、非serve bench性能比不変 | **#2665解決** |
| 5 | native/embedded shutdown（§5.6）。**先行**: als shutdown契約 | 両経路で#2692再現fixture | **#2692解決** |
| 6 | `tests/http_serve_cross_test.rs` のHTTP意味論比較（header集合、de-framing） | 新C-367のnative ⇄ embedded replay | 7前提 |
| 7 | `almide build --target wasm` でserve programを `wasi:http/incoming-handler@0.2` export（§5.3–5.4）。形状規則/code、component shell（request/response変換、`max_body_bytes`、instanceごとのapp init）。**structural**とcomponent経路（`crates/almide-wasm-run/src/wasi_p2.rs`）で構築、旧経路は使わない。**先行**: als export契約 | 固定 `wasmtime serve` 下C-367 replay（P2、`-W` なし）、形状診断対fixture、`http.serve` stock E081行変更 | **#2659解決**、p3-sockets案を置換 |
| 8 | 表層文書: CHEATSHEET / llms.txt / `docs/stdlib/http.md` / `docs/diagnostics/E008.md`、§4.3 programの `almide check` fence | `scripts/check-llm-surface.sh` | 1–7の後。本記録はその表層を変更しない |
| 9 | （後で）wasmtimeが `-W` なしserveできるようになったらP3 `wasi:http/handler@0.3.0` exportとguest instance内直列化（§5.3）。最初の適合hostにwasi:keyvalue draft2 stock `kv` binding | P3 replay、conformance fixture | #2659後続 |

**#1696（旧wasm emitter廃止）との関係。** 旧codeは一切加えない。

- step 0b/7はstructuralとcomponent経路と明記。
- embedded `kv` host op（step 3）は `almide-wasm-run` のhost側とstructural lowering。
- 現在の旧router提供は暫定で、step 0bにより不要になり、旧経路を残す理由を減らす。

`proofs/target-availability.toml` の全新規行はstructural経路を名指す。

**副次発見、bug #2703/#2704として登録**（0.64.0、2026-09-27確認）:

- **embeddedのclock。** `http.serve` を呼ぶ**任意**のprogramでclock読取り（`datetime.now()` / `env.millis()`）は `almide run --target wasm` のE082。handler限定ではなく `main` の `serve` 前も拒否。structuralは `call:datetime.now` / `call:env.millis` をwallし、旧経路は `http.serve` をwallする。`serve` がなければ同じclock readは動く。handlerに原因を限定した研究の副次観測を精緻化する。
- **effect-slot lambda内intrinsic effect fnの `!`。** `effect (…) -> …` slot（`http.serve` handlerや `let f: effect (Int) -> Int = …`）として検査するlambda内の `random.int(0, 999)!` / `env.millis()!` は**E022**。同位置の `fs.read_text(p)!`（`Result` を返す）とuser `effect fn … -> Int` は受理され、effect fn bodyの `random.int(0, 9)!` も受理される。これは `stdlib/http.almd:19-25`（「effect-fn body ergonomics（伝播付き `!`…）」）とCHEATSHEET:342-354に反する。

<a id="9-refinements-the-evidence-forced"></a>

## 9. 証拠による精緻化

1. **「stock wasmはhost KVを使う」を「適合host KV、現在適合なし」へ変更。**
   - `wasmtime serve` 47は `open` ごとbucket clone、instanceごと新context（§4.1）。
   - Spin CASは初回insertで無条件upsert。
   - Workers KVは結果整合でatomicsなし。
   - したがって適合fixtureを通るhostが現れるまでstock buildの `kv` はE081。
2. **serve appはinstance-closed必須。** handlerはmainのvarだけでなくlocalをcapture不可。export hostは `main` を実行せずnative workerも独自instance。C-367の「mainが `serve` 前に計算した値は全request同一」を廃止する。そのfixture自身の `boot` である。
3. **export hostで `main` はentry pointでない。** export buildは単一serve callの `main` を受理。冪等prologue案は冪等性が検査不能でP2の `kv` writeがrequestごと繰り返すため却下。
4. **stock exportはP3でなくP2 `incoming-handler@0.2`。** 固定wasmtimeのP3には `-W component-model-more-async-builtins` が必要、1heapで最大16handlerがinterleave。P3 shell導入時はinstanceごと直列化する。
5. **escape節とsnapshot節が必要。**
   - 自身の `var` を持つclosureを返すfactory fnはそのvarを共有。E011はfactoryを `effect fn` にさせるが止めない。
   - var値の `let` copyはreachでない。この節なしではanalyzerがE008自身の `fixed.almd` を検出した。
6. **native modelはforkでなくthreadごとapp instanceのworker pool。**
   - top-level `Rc` letの既存thread-local lazy slotでinstanceはほぼ無料。
   - forkは `kv` を分割し未flush stdoutを複製する。
   - thread-local stdoutをglobal化しないとworker出力が失われる。
7. **名前。** moduleは `kv`、型は `KvStore`（§4.2）。limit entry pointは既存 `*_with_limits` familyに従う `serve_with_limits`。
8. **export artifactに新しい前提2つ。** #2664に加え、router callが現在両wasm経路でwall（0b）。

<a id="10-alternatives"></a>

## 10. 代替案

- **handlerが `var` へ到達するとwarning付き逐次serve**（研究案E）、暫定案。
  - 測定/ソース調査にhandler内容でserve modeを切り替えるsystemなし（§1.2）。
  - Almide唯一の先例 `fan.map` は両版同結果だからのみ安全。`serve` ではlatencyが変わり、まさに#2665: 1.70 s対0.00 s（M1対M6/M16/M19）。
  - LLMが「request counterを加えるだけ」で足す1行がtrigger、signalはwarningのみ。
- **requestごと分離**（fork/deep copy、研究案B）。
  - counterは黙って永遠に1（M10、M14）、embeddedは一致できない（2..6対1）。
  - fork後childが親のRNG streamを続ける（§5.5）。
  - forkがin-process `kv` を分割。
- **lock付き共有heap**（研究案Cまたはglobal lock）。
  - lockなしならraceは静か。Goもruntime `-race` のみで報告（M6/M7）。
  - global lockでは再び逐次serve（M1/M5/M9/M12/M17の1.70 s）。
  - 全値 `Arc` 化はruntime全体のコスト。
  - embeddedは `after=6` を生成できずP2/Workersはそもそもmemory共有不可。
- **stock artifactのp3 `wasi:sockets`**（#2659案1）。
  - 動く: #2659試作commit e63493268はlocalで3経路byte同一replayに合格。
  - それでも却下。`wasmtime run -S p3 -S inherit-network` はCLI runnerでserving platformではなく、serving platform（`wasmtime
    serve`、Spin、Workers）はincoming-handlerを話す。
  - request parse/response writeをguestへ移すのでbyte一致が転写になる。
  - 保持できた唯一のもの、native単一instanceはpillar 1–2成立後は観測不能。
- **opt-in並行処理**（`http.serve` は逐次、別 `serve_concurrent` に規則適用、研究案G）。modelが最初に書く既定が依然blockする（#2665）。
- **明示state fold**（`http.serve_state(port, init, (s, req) => (s2,
  resp))`、研究案H）。
  - foldは構造上逐次なのでfast requestが再び待つ。
  - 状態はinstance memoryなのでP2ではrequestごとreset、P3/Workersでは分割。
  - keyごとstoreなら両問題なしに同じ明示性を持つ。
- **`kv.incr` をhost `increment` へ対応。** encodingが異なる（wasmtime十進text、Spin 8-byte LE）、`get` 混在は実装依存（wasi-keyvalue PR #59）。

<a id="11-consequences"></a>

## 11. 帰結

利点:

- 1programがAlmideの全serve hostで1組の応答を意味する。編集がserve modeやcounter意味を黙って切り替えることはない。
- 2 s `/slow` の後の `/fast` がnativeですぐ応答（#2665）。
- stock artifactが通常の `wasi:http` componentになる（#2659）。
- `fan.map` の黙った逐次化の穴が閉じ、#2594の意味論上の障害がなくなる。
- 0b後は両経路でappをcallしてtestできる。

代償:

- 新dialect epoch。
- 新stdlib module。
- printごとlockする共有runtime stdout buffer。
- workerごとのtop-level `Rc` let評価: workerごとのmemory/startup。
- export hostではmain計算のprocess値を失う。
- 適合hostが現れるまでstateful appはstockへexportできない（§4.4）。

<a id="12-falsifier"></a>

## 12. 反証条件

1. Dojoがmodel作成handler/`fan` taskを測り、E008後に正しい `kv` / `let` 書換えより*誤った*programを多く作ることが2release続けば、診断/fix-itが誤りで再検討する。適合代替案が見つからない限り規則自体は維持。
2. 並行bodyから到達可能な `var` が正しい動作に必要で `kv` で表せない手書きprogram（例: 接続ごとstreaming状態機械）が現れたら、規則の存在でなく到達範囲を再検討。
3. workerごとapp instanceにより、1clientのspec serve fixtureで現在の逐次loopよりnative serveが遅くなるなら、worker別instance modelを再検討（例: pure値の `Send` 表現による共有immutable app）。
4. wasi:keyvalueがCASなし、またはkeyごと保証より弱い形で安定化したら（未決PR #56の方向）、標準のatomic primitiveに合わせstock `kv` bindingを再設計する。Almide表層は維持。

<a id="13-references"></a>

## 13. 参考資料

- 研究note: `../almide-references/RESEARCH-http-serve-concurrency.md`（M1–M23、S1–S18、§4–§6）、probe/raw出力は `../almide-references/research-2026/http-serve-concurrency/`。
- wasi-keyvalue: https://github.com/WebAssembly/wasi-keyvalue （README.md:7、55-57、82-86、`aa972c86e` のwit/store.wit、wit/atomic.wit、wit/batch.wit、tag `v0.2.0-draft`、PR #56/#59/#60）。
- wasmtime 47: tag v47.0.3の `src/commands/serve.rs`（44-68、141-155、380-460、535-545行）、`crates/wasi-keyvalue/src/lib.rs`（100-171、226-246行）、https://github.com/bytecodealliance/wasmtime/tree/v47.0.3 、`wasmtime serve --help` / `-S help`（47.0.2）、`wasmtime-wasi-http-47.0.3/wit/deps/http.wit` の `wasi:http@0.2.12` WIT（`interface incoming-handler`、636行）。
- Spin: https://github.com/spinframework/spin （`wit/deps/keyvalue-2024-10-17/`、`crates/key-value-spin/src/store.rs:326-457`、`crates/factor-key-value/src/host.rs:706-745`）、https://spinframework.dev/v4/dynamic-configuration 、https://spinframework.dev/blog/announcing-spin-3-6 。
- Cloudflare: https://developers.cloudflare.com/kv/concepts/how-kv-works/ 、https://developers.cloudflare.com/kv/api/write-key-value-pairs/ 、https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/ 、https://developers.cloudflare.com/durable-objects/concepts/what-are-durable-objects/ 、https://developers.cloudflare.com/durable-objects/api/namespace/ 、https://developers.cloudflare.com/workers/reference/how-workers-works/ 、https://developers.cloudflare.com/workers/runtime-apis/webassembly/ 。
- Deno KV: https://docs.deno.com/deploy/kv/transactions/ 。
- RFC 9110 §5.3（field順序）、https://www.rfc-editor.org/rfc/rfc9110#section-5.3 、RFC 9113 §8.3.2、https://www.rfc-editor.org/rfc/rfc9113#section-8.3.2 、RFC 9114 §4.3.2、https://www.rfc-editor.org/rfc/rfc9114#section-4.3.2 。
- tree内: `runtime/rs/src/http.rs:723-743`、`runtime/rs/src/fan.rs:3-40`、`runtime/rs/src/random.rs:6-10`、`crates/almide-codegen/src/lib.rs:346-359`、`crates/almide-frontend/src/check/infer_calls_closures.rs:215-234`、`stdlib/http.almd:19-30, 355-626`、`proofs/dialect-epochs.toml`、`docs/contracts/contracts.toml`（C-367、C-368）、C-350。
- 番号: 0018は進行中branch（value-strings記録）が確保しているため本記録は0020。
