# ALS — 実行時規範（Runtime）

> Last updated: 2026-10-05

プログラム実行の観測規範（エラー終了・文字列補間の表示形・並行コンビネータ）。
参照方法は [strings.md](strings.md) 冒頭と同じ。

## ALS-R1 effect-main のエラー終了形

`effect fn main` が Err で終わるとき、stderr に `Error: <メッセージ>` を1行
出力し **exit code 1** で終了する。Ok 終了は exit 0。パニック級の異常
（ALS-T6 の abort を含む）も同じ `Error:` 接頭辞と exit 1 に統一される。
Contracts: C-035。

## ALS-R2 補間の表示形

文字列補間 `"${v}"` の表示形は型ごとに規範化される:

- **コンテナ**（List/Map/Set/タプル/Option/Result）: Almide リテラル形
  （`[1, 2]`、`("a", 1)`、`some(3)` 等）。ネストも再帰的に同形。
- **レコード/変種**: `TypeName { field: v, … }` — フィールドは**宣言順**。
  anonymous record はフィールド名の辞書順。再帰・ジェネリック ADT は
  インスタンス化ごとに同じ規則。
- **裸の Float**: Display は整数値の `.0` を落とす（`3`）。`float.to_string`
  は保持する（`3.0`）。この2形の区別は規範である。
Contracts: C-008, C-009, C-010, C-011。

## ALS-R3 fan 並行コンビネータの決定性

**受理形**(構文要素方向: `ExprKind::Fan` / `FanBounded` / `FanRace` /
`FanRaceMap` / `FanSettle` / `FanTimeout`): リスト形 `fan.map(xs, (x) =>
…)`、ブロック形 `fan.any { a(); b() }` / `fan.settle { … }`。全形が
**effect fn 文脈必須**。`fan.map` の mapper は **Result を返す契約**
(裸の値は検査時拒否、`(x) => ok(…)` へ誘導 — any/settle の thunk は
auto-wrap、map の mapper はしない)。`fan.settle { a; b }` の返りは
**Result のタプル**(要素数 = thunk 数、ALS-DT4)。

`fan.any`・`fan.map`・`fan.settle` の結果は**リスト順で決定的**(最初に
完了したものではなく、引数リストの先頭から評価した最初の該当)。エラーは
ALS-R1 の統一 abort 形で表面化する。

`fan.map` と `fan.settle` の観測(stdout・stderr・終了コード・返り値)は、
**全要素をリスト順に一つずつ評価した逐次評価**の観測と同一でなければならない。
実行基盤(逐次・スレッド・非同期 subtask)の選択はこの観測を変えてはならない
(C-004)。`fan.map` はある要素が Err を返した後も**残りの全要素を評価し**、
Err が複数あれば**最小 index の Err** を結果とする — ブロック形 `fan { }`
と同じ規則(C-005、C-199、`spec/wasm_cross/fan_map_err_runs_every_element.almd`)。
要素 k の trap は、要素 0..k-1 が完了して出力がリスト順に現れ、要素 k の
trap までの出力の後に abort する観測となり、k より後の要素の出力は現れない
(C-200、`spec/wasm_cross/fan_trap_waits_for_elements_below.almd`)。

`fan.race` と `fan.timeout` は 0.42.0 / 0.29.0 でいったん削除された後、
**決定的意味論を得て 0.47.0 で復活した**: race は (spend, index) 辞書式
最小の勝者則(ALS-DT3、C-205 — mapper 形 `fan.race(budget?, xs, f)` を含む)、
timeout は charge site 協調チェックの壁時計期限(ALS-DT5、C-208)。
`fan.bounded(c) { … }` は決定的予算(ALS-DT2、C-204)であり、mapper 形は
持たない。旧 tombstone 裁定(C-004/C-006)はこの復活で SUPERSEDED。
Contracts: C-004, C-005, C-006。

## ALS-R4 非有限浮動小数の定数表示

const 畳み込みで生じた非有限値（inf / -inf / NaN）は名前付き定数として
表示される（`inf`・`-inf`・`NaN`）。ビットパターンや `1e999` 形は不適合。
Contracts: C-012。

## ALS-R5 プロセス環境

`env.args` は argv[0]（プログラム名）を**除いた**引数列を、`process.args`
は argv[0] を**含む**全列を返し(0.59.1 実測: 引数 2 個で env.args は長さ 2、
process.args は長さ 3・先頭がバイナリパス)、それぞれ両ターゲットで一致する。`env.get(name)` はホストプロセスの環境変数を
観測し、存在すれば some(値)、無ければ none を両ターゲットで同一バイトで
返す（wasm は WASI environ + ランナーの環境継承）。`random.int(a, b)` は
WASI entropy 下でも常に [a, b] 範囲内。

`env.os()` と `env.temp_dir()` は等価性則の**唯一の適用除外**である。両者は
実行中のホストを報告するため、native は実 OS 名と実 temp ディレクトリを、
wasm レグは WASI サンドボックス（`wasi` / `/tmp`）を返す。両ターゲットで
一致させることこそが欠陥であり、「今どのプラットフォームか」を問う
プログラムには真を返さねばならない。除外は無制限ではなく、両レグで同一の
決定的不変量（os は閉じた集合 {macos, linux, windows, wasi} の要素、
temp_dir は非空かつ posix ホストでは絶対パス）が証明対象となる。除外は現在
3 関数(env.os・env.temp_dir・fs.temp_dir — C-189)。第四の関数を
除外に加えるには C-189 の statement とその fixture の改訂を要する。

`process.exec_status` と `process.exec_status_timeout` の起動失敗は、
操作名とコマンド名を含む err とする。接頭辞はそれぞれ
`process.exec_status(<quoted cmd>): ` と
`process.exec_status_timeout(<quoted cmd>, <timeout_ms>): ` とし、
続けてホストのエラー説明を付ける。コマンド名は二重引用符で囲み、内部の
引用符・バックスラッシュ・制御文字をエスケープする。引数列は含めない。
存在しない実行ファイルは起動失敗であり、タイムアウトと報告してはならない。
期限が発火した場合の err は従来どおり `exec timed out after <ms>ms` とする。
テスト: `spec/stdlib/process_timeout_test.almd`

wasm ターゲットでは、埋め込みホスト（`almide run --target wasm` と `almide test`
の wasm レグ）が子プロセス族（`process.exec`・`exec_in`・`exec_with_stdin`・
`exec_status`・`exec_status_timeout`・`run`・`run_in`・`spawn`・
`kill`・`is_alive`・`pid`）を native と同じ観測で提供する。捕捉した stdout と stderr、
終了コード（シグナルで終わった子は -1）、err の文字列は同一バイトである。pid は
ホストの値であり、レグ間で比べない。単体の成果物（`almide build --target wasm`）
は、プログラムが子プロセス操作を含むときに限り、非公開インターフェース
`almide:process/spawn` の `call` をインポートする。含まないプログラムの成果物は
このインポートを持たない。このインポートを定義しないランタイムは、`_start` を
実行する前の読み込み時にモジュールを拒否する。したがって子プロセス呼び出しが
実行時に誤った答えを返すことはない。そのようなプログラムのコンポーネント
（`--component`）としてのビルドは E081 で拒否する。
テスト: `spec/embedded_cross/process_spawn_family.almd`

`http.start(method, url, body, headers, limits)` は、要求を始めてすぐに呼び出し
ハンドル（`HttpCall`）を返す。上限は呼び出しごとに `limits = { total_ms,
idle_ms }`（ミリ秒、0 は上限なし）で渡す。`total_ms` は `start` から測る壁時計で、
接続・最初の 1 バイトまでの待ち・本文をすべて含む。`idle_ms` はバイトが届く間隔の
上限で、最初の 1 バイトまでの待ちも含む。発火した上限は、自分の名前を示す err
`request timeout: total_ms <n> exceeded` または
`request timeout: idle_ms <n> exceeded` で呼び出しを終わらせる。
`ALMIDE_HTTP_TIMEOUT_SECS` は上限を取らない呼び出しの既定値であり、上限を持つ
呼び出しには効かない。`poll` と `read_new` は待たない。`wait` は呼び出しが終わる
まで待つ。`cancel` と、ハンドルの最後の写しが捨てられることは、走っている呼び出しを
err `request cancelled` で終わらせて接続を閉じる。以後 `read_new` は空文字列を
返し、サーバーは接続が閉じたことを観測する。上限が発火するかどうかはホストの性質で
ある（C-214 と同じ規律）。固定するのは、発火したときの err の形である。wasm
ターゲットでは、埋め込みホスト（`almide run --target wasm`）が `start`・`poll`・
`read_new`・`wait`・`cancel` と `request_stream_with_limits` を native と同じ
観測で提供する。err の文字列、`cancel` で接続が閉じること、ハンドルの最後の写しを
捨てると呼び出しが取り消されることも同じである。単体の成果物
（`almide build --target wasm`）はこの族を持てず、`almide check --target wasm` は
E081 で拒否する。`openai_streaming_call_with_limits` と
`anthropic_streaming_call_with_limits` は native のみである。
テスト: `spec/stdlib/http_call_test.almd`、`spec/embedded_cross/http_call_handle_errs.almd`

`http.serve(port, f)` は埋め込み wasm レーン（`almide run --target wasm`）でも
native と同じ意味で動く。1 回の実行のすべての要求を 1 つのインスタンスが、受理順に
1 件ずつ処理する。main は 1 回だけ走り、native と同じく `http.serve` を呼ぶ。
ホストは `0.0.0.0:<port>` に bind し、解析済みの要求を 1 件ずつゲストに渡す。
`serve` の前の main の効果は 1 回だけ起こる。`serve` に渡すアプリはインスタンスに
閉じている（main の局所変数を読まない。almide/almide#2698）。したがって main が
`serve` の前に計算した値については何も規定せず、アプリを評価するインスタンスの数は
観測できない。要求の読み取りは両レグで同じコードが行う。
要求行のメソッドとターゲット、最初のコロンで分けて前後の空白を除いたヘッダ行
（到着順）、Content-Length の本文（UTF-8、不正バイトは置換文字）を読む。
どの要求にも両レグは同じ status コード・ヘッダ集合・本文で答える。ヘッダ集合は
応答のヘッダフィールドを (名前, 値) の組として見たもので、名前は ASCII の大小を
区別せずに比べる。名前の異なるフィールドの順序は問わず、同じ名前のフィールドは
互いの相対順序を保つ（RFC 9110 §5.3）。ホストが管理するフィールド `date`・
`connection`・`keep-alive`・`transfer-encoding`・`content-length` は除く（これは
フレーミングであり、本文はフレーミングを外して比べる）。理由句は規範に含まない。
HTTP/2 と HTTP/3 は理由句を持たず（RFC 9113 §8.3.2、RFC 9114 §4.3.2）、ホストの
HTTP ライブラリは自分の理由句を書く（native コアは `418 OK`、hyper は
`418 I'm a teapot`。almide/almide#2659 の試作で測定）。`req_method`・`req_path`・
`req_body`・`req_header`（最初の一致、ASCII 大小無視）・`query_params`（最初の
`?` 以降を `&` で分け、各組を最初の `=` で分け、`=` の無い組は捨て、`+` と `%XX`
を復号し、後のキーが勝つ）は同じ値を返す。ハンドラの `err(m)` は本文
`Internal error: <m>`、`Content-Type: text/plain` の `500` になる。export ホスト
（C-375 の成果物を走らせる `wasmtime serve`）では、ハンドラの trap や中断には
ホスト自身の `500` が答え、そのインスタンスは捨てられる。ソケットホスト（native と
埋め込みレーン）では、bind の失敗は、呼び出しがどの位置にあっても実行を中断し、stderr に
`Error: bind failed: <os message>` を書いて終了コード 1 で終わる。`http.serve` は
err を返さない型なので、呼び出し側の `!` は何もしない（この規範以前、native
ランタイムが返す err が現れるのは呼び出しが関数の末尾にあるときだけで、それ以外の
位置ではプログラムはサーバー無しで先へ進んでいた）。ソケットホストでは、サーバーが
走る間もレーンは native のストリーム規則を保つ。stderr はバッファしないので、実行が stderr に書く行はどれも
両レグでストリームに届き、2 つの記録は同じ行を持つ（要求をまたぐ行の順序は規範に
含まない。`wasmtime serve` の行頭 `stdout [req_id] :: ` のようなホストのログ装飾は
記録に含まない）。stdout は端末なら書き込みごとに、それ以外は行を終える書き込みごとに flush する（C-162）。
ソケットホストでは、シグナルはサーバーの出力を失わせずに止める（almide/almide#2692）。`http.serve` が
動いている間に最初の SIGTERM か SIGINT（Windows では Ctrl-C か Ctrl-Break）が届くと、
ホストは受理をやめ（まだ受理していない接続は答えずに閉じる）、処理中の要求を最後まで
処理して答え、stdout を flush し、`http.serve` から戻る。したがって後続の文が走り、
終了コードは main のものになる。二つ目のシグナル（この後始末の間でも、`serve` が
戻った後でもよい）、または要求タイムアウト（30 秒）を過ぎても後始末が待っている
ことは、stdout を flush して終了コード 1 で終わらせ、処理中の要求には答えない。
どの停止も 128+シグナル番号では終わらない（C-350 の `0..=125`）。
対象外: HTTP のフレーミングと接続の再利用（ホストが決める）。その他のシグナル。
native の Windows では、強制停止は stdout を flush せずに終了コード 1 で終わる。
stdout のバッファには serve しているスレッドしか触れず、stdout がプロセス全体で
一つのバッファになる（almide/almide の ADR-0020 §5.5）まではそうなる。標準の成果物（`almide build --target wasm`）は待ち受けソケットを持たない。
serve 形のプログラムは `wasi:http/handler@0.3.0` を export するコンポーネントになり、
その形は次の段落（C-375）が記述する。それ以外の `http.serve` に届くプログラムは、
標準の成果物では check 時にもビルド時にも拒否される（E081。どちらも
almide/almide#2659）。
テスト: `spec/serve_cross/http_serve_replay.almd`（終了しないサーバー fixture で、
汎用ランナーは実行しない。実装側のドライバが両レグで起動し、同じ要求列を再生して
各応答の status コード・ヘッダ集合・フレーミングを外した本文と、行の多重集合としての
stderr の記録を比べ、使用中のポートで起動して中断を比べる）、
`spec/serve_cross/http_serve_shutdown.almd`（同じドライバが stdout をファイルに
向けて両レグで起動する終了しないサーバー fixture。ハンドラが眠っている要求の最中に
SIGTERM を 1 回送ると、その要求に答え、`http.serve` の後に main が書く行まで
すべての行をファイルに残し、終了コード 0 で終わる。止まった要求の最中に SIGTERM を
2 回送ると、停止前に書いたすべての行を残して終了コード 1 で終わる）

serve の形のプログラム（`main` の本体がちょうど 1 つの `http.serve(port, app)`
呼び出しで、その前には `port` だけが読む `let` しか置かない）を
`almide build --target wasm` で作ると、`wasi:http/handler@0.3.0` を export する
コンポーネントになり、`wasmtime serve` はフラグ無しでそれを読み込んで serve する。
コンポーネントが import するのはサービス world のインターフェースだけで、
`wasi:filesystem` は含まない。`main` は走らず、`port` も評価しない（アドレスは
ホストが決める）。アプリとトップレベルの `let` はインスタンスごとに評価する。
1 つのインスタンスで同時に走るハンドラは 1 つである（バックプレッシャー）。ホストは
並行する要求をインスタンスを増やして処理する。ゲストは native の上限を適用する。
上限（1 MiB）を超える要求本文には `413`、上限（8 KiB）を超える要求行には `414` で
答え、ハンドラは呼ばない。ハンドラの trap や中断にはホストの `500` が答え、ホストは
そのインスタンスを捨てる。それ以外の受理した要求には、native と同じ status コード・
ヘッダ集合・フレーミングを外した本文で答える。比べ方は C-367 の HTTP 意味論の比較で
あり、理由句・`date`・フレーミングはホストのもので、バイトでは比べない。
対象外: ホストのアドレス、受理、並行度、インスタンスの再利用、タイムアウト、停止、
ログ装飾（どれもホストの設定）。
テスト: `spec/serve_cross/http_serve_export.almd`（終了しないサーバー fixture で、
汎用ランナーは実行しない。実装側のドライバが標準 wasm 向けに作って `wasmtime serve`
で serve し、native でも走らせ、同じ要求列を再生して各応答の status コード・ヘッダ
集合・フレーミングを外した本文を比べ、成果物に上限超えの本文と長すぎる要求行を送り、
最後に `/trap` を要求する）

Contracts: C-096, C-112, C-118, C-133, C-189, C-214, C-366, C-367, C-375。

## ALS-R6 ファイルシステムのパス解決

wasm の fs ランタイムは起動時に WASI preopen ディレクトリ表を構築し、
絶対パスを最長一致 preopen + 相対残りに解決する（`./` 正規化込み）。
同一パスへの書き込み→読み戻しは native std::fs と同じホストファイルに
到達する（CWD 非依存）。open エラー文言は ALS-T6 系の native 文言規範
に従う。

`fs.list_dir(path)` はディレクトリの**全**エントリを返す。エントリ数に
上限はなく、ホストのバッファ境界（wasm の `fd_readdir` は resumable API で
あり、1 パスで返るのは呼び出し側バッファに収まる分だけ）は観測に現れない。
`.` と `..` は両ターゲットで除外され、順序は**バイト辞書順**に正規化される
（native は `names.sort()`、wasm は同じバイト比較の挿入ソート）。ファイル
システムの readdir 順は観測できない。読み取り失敗は短いリストではなく err。
テスト: `spec/wasm_cross/fs_list_dir_multipass.almd`
Contracts: C-042, C-272。

## ALS-R7 ストリーミング行走査の可謬コールバック

ADR-0006 の 1 ビット可謬性多相は fs のストリーミング行走査にも適用される。
`fs.fold_lines` / `fs.for_each_line` のコールバック本体が `!` を使うとき、
検査器は呼び先を内部キャリア `fs.__fallible_fold_lines` /
`fs.__fallible_for_each_line` に書き換え、呼び出し全体が可謬になる
（`list.map` → `list.__fallible_map` と同型）。キャリアは綴りではない —
ソースが直接名指しすると E043。

規範となる観測量は **コールバック呼び出し列** である。最初の err を返した
行までコールバックが呼ばれ、それ以降の行では**一度も呼ばれない**
(first-err 打ち切り)。err メッセージはコールバックのものがそのまま
呼び出し側の err チャネルに乗る。この 2 つは native ⇄ wasm でバイト同一。

native レッグは加えて**読み取り自体**を失敗行で止める
(`almide_rt_fs_fold_lines_effect` が BufReader ループから return する)。
wasm 自己ホストは C-220 と同じくファイル全体を先に読むため、
「リーダがどこで止まったか」は wasm の観測量ではない — RSS と同じく
native 限定の性質であり、観測可能な約束には含まれない。

区画走査 (`fold_lines_range` / `fold_lines_chunked`) には可謬形が**ない**。
分割走査の「最初の err」はスレッド実行順の観測量になるため、
定義できる打ち切り点が存在しない — 意図的省略として
`tests/fs_streaming_family_gate_test.rs` の行列が固定する。

テスト: `spec/wasm_cross/fs_fallible_stream_callback.almd`（両レッグ）,
`spec/stdlib/fs_streaming_test.almd`（for_each_line セル — native 限定）,
`tests/fs_streaming_family_gate_test.rs`（行列ゲート）。

Contracts: C-274。

## ALS-R8 HTTP レスポンスヘッダの規範

`http` のレスポンス構築族（`response` / `json` / `redirect` / `with_headers` /
`status` / `body` / `set_header` / `get_header`）は**ネットワークに触れない純
データ操作**で、両ターゲットで走る。ヘッダ名の規範は3つ:

- **フィールド名は大小文字を区別しない**（RFC 9110 §5.1）。畳み込みは
  **ASCII のみ**（フィールド名は `token` であり、v0 の
  `eq_ignore_ascii_case` と一致させる — Unicode の `string.to_lower` を
  使ってはならない）。`get_header` は最初の一致を返し、無ければ none。
- **1つのフィールド名につきエントリは高々1つ。** 書き込み（`set_header`・
  `with_headers`）は大小文字を無視して既存エントリの**値をその場で上書き**
  し（名前の綴りは最初に格納されたものを保つ・位置も動かない）、無ければ
  末尾に追加する。ゆえに `get_header(set_header(r, k, v), k') == some(v)`
  は `k` と `k'` が ASCII 大小文字違いなら常に成り立つ。
- **`with_headers` は渡された Map そのもの**（map 順）を返し、
  `Content-Type` を勝手に**播かない**。既定の Content-Type は、名前を与える
  手段が他にない `response`（`text/plain`）と `json`
  （`application/json`）だけが持つ。`redirect` は `Location` のみを持つ。

テスト: `spec/wasm_cross/http_response_headers.almd`,
`spec/stdlib/http_response_test.almd`。

ハンドラは `HttpRequest` を受けて `HttpResponse` を返す関数
（`HttpHandler = effect (HttpRequest) -> HttpResponse`）であり、ルーターと
ミドルウェアを掛けたアプリもまたハンドラである。`http.new_request(method,
target, body, headers)` はソケットなしでリクエストを作り、読み取り族
（`req_method` / `req_path` / `req_body` / `req_header` / `query_params` /
`param`）はそれに対して両ターゲットで同じ値を返す。`req_path` は target を
クエリ文字列ごと返す。ルーティングは両ターゲットで同じ結果になる:

- ルートは `"METHOD /path"`（メソッド省略は全メソッド）で、`{name}` は 1
  セグメントを束縛し、最後の `{name...}` は残りのパス（空でもよい）を束縛する。
  束縛値はパーセントデコードされ、`+` は `+` のまま残る。照合はクエリ文字列を
  除いたパスで行い、空のセグメントは数えない。
- 一致したルートのうち**最も具体的なもの**が応答し、登録順は結果に影響しない。
  ルート A が一致するパスをすべて B も一致するとき A は B 以上に具体的であり、
  メソッドを持つルートは同じパスのメソッドなしルートより具体的である。
- `http.router(routes)` は、あるリクエストに共に一致しどちらも他方より具体的で
  ないルートの組（同じパターンの二度書きを含む）、最後でない `{name...}`、二度
  束縛される名前、大文字英字でないメソッド、`/` で始まらないパスを持つ表を、
  問題をすべて名指す `err` で拒否する。拒否された表は応答しない。
- どのルートもパスに一致しなければ `404 Not Found`、パスに一致するルートが別の
  メソッドにしかなければ `405 Method Not Allowed` と `Allow` ヘッダ（メソッドを
  整列し `, ` で連結、GET があれば HEAD を含む）を返す。HEAD は GET のルートに
  落ち、本文を空にして返す。target が `/` で始まらない、またはパス中の `%` の
  後に 16 進 2 桁が続かないリクエストは `400 Bad Request` である。
- `http.mount(prefix, sub)` は prefix 以降のパスとクエリを `sub` に渡し、prefix
  が束縛した名前は `sub` からも読める。`http.wrap(h, [a, b])` は `a(b(h))` で
  あり、リストの先頭が最も外側になる。
- `http.decode_json(req, decode)` は本文を JSON として読み decode に渡す。
  失敗の `err` は理由を本文に持つ `400 Bad Request` のレスポンスである。
  ハンドラの `err` は `err` のまま呼び出し側に返る。

テスト: `spec/stdlib/http_router_test.almd`。
Contracts: C-275, C-368。

## ALS-R9 プロセス終了コードの値域

`process.exit(code)` が受理する code は、POSIX の終了ステータスである
**0..=255** である。この範囲の値は native と埋め込みホスト
（`almide run --target wasm`）で**その値のまま**終了する。
子プロセスの 126・127・128+n をそのまま返すラッパーが書けることが、この値域の
目的である（#2780）。Rust の `std::process::exit`、Go の `os.Exit`、Python の
`sys.exit`、Node の `process.exit` も同じ値を通す。

計算された引数が 0..=255 の外なら、定義済みの領域エラーになる。stderr に
`Error: exit code must be in 0..=255` を1行出力して **exit 1** で終了する
（全ターゲットで同一バイト）。上に挙げた言語は下位 8 bit を OS に任せるため、
`exit(256)` は成功（0）を、`exit(-1)` は 255 を報告する。呼び出し元はこの
切り詰めを検出できないので、Almide はここだけそれらの言語と違う扱いにする。

| 層 | 運べる値 | 範囲外に何が起きるか |
|---|---|---|
| POSIX `exit(status)` | 下位 8 bit のみ | 親プロセスは `256` を `0` として観測する（黙った切り詰め） |
| WASI preview-1 `proc_exit`（stock ランタイム） | `[0, 126)` | ホスト trap。wasmtime 47 は `exit with invalid exit status outside of [0..126)` を出して exit 1 になり、exit 要求としての情報は残らない |
| component model `wasi:cli/exit#exit-with-code(u8)` | 0..255 | trap しない |

**WASI preview-1 のビルドは 126..=255 を届けられない。** `almide build --target
wasm` が出力する preview-1 の成果物を stock ランタイムで動かすと、その値の
`proc_exit` は trap になる（#2303）。そのためこのビルドは、その帯の値を自分で
拒否する。stderr に `Error: a WASI preview-1 build cannot exit with a code in
126..=255` を1行出力して **exit 1** で終了する。これは preview-1 の降下が持つ
診断付きの壁であり（wasm 上の `env.cwd` と同じ種類）、黙った 1 でも、原因の
分からないホスト trap でもない。0..=125 はどのレッグでもその値で終了する。

この壁は wasm の性質ではなく preview-1 の降下の性質である。終了コードとして
1 バイト全体を運べる成果物（component model の `exit-with-code(u8)`）が既定に
なれば、壁は外せる。外すことは後方互換な拡大である。

直接呼び出す `process.exit` の引数が範囲外の整数リテラルなら、checker は
引数を指す **E084** で拒否する。括弧・単項マイナス・各基数のリテラルを含み、
モジュールの別名・選択的 import・パイプ経由でも同じ規則を適用する。
`-0` は 0、二重の単項マイナスは正の値として判定する。

```almide check-fail=E084
import process
effect fn main() -> Unit = process.exit(256)
```

変数や算術式などの計算された引数は、このリテラル診断では拒否しない。
関数値を格納した変数への呼び出しも、この診断では追跡しない。
実行時の範囲チェックは引き続き適用される。
同名のユーザ関数やローカル関数値は標準ライブラリの `process.exit` ではなく、
この診断の対象外である。

テスト: `spec/wasm_cross/exit_code_out_of_range.almd`,
`spec/wasm_cross/exit_code_upper_bound.almd`,
`spec/wasm_cross/exit_code_passthrough.almd`,
`spec/embedded_cross/exit_status_passthrough.almd`,
`tests/diagnostics/e084-exit-code-domain/broken.almd`,
`tests/diagnostics/e084-exit-code-domain/fixed.almd`,
`tests/exit_literal_check_test.rs`。
Contracts: C-350, C-351。
