# ALS — 実行時規範（Runtime）

> Last updated: 2026-08-20

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
Contracts: C-096, C-112, C-118, C-133, C-189。

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
Contracts: C-275。
