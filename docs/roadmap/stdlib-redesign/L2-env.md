<!-- description: Stdlib redesign L2/L3 — env: the owner of argv, environment variables, cwd, OS name and sleep; the twins in process/args/fs/datetime retire into their owners -->
# L2/L3: env

> Last updated: 2026-10-26
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-25 の持ち分表がこのページの土台）
> 出典: Rust https://doc.rust-lang.org/std/env/ ・ Go https://pkg.go.dev/os ・ Python https://docs.python.org/3/library/os.html ・ TS/JS https://nodejs.org/api/process.html ・ Gleam https://gleam-stdlib.hexdocs.pm/ （env は stdlib に無い）・ Elixir https://hexdocs.pm/elixir/System.html ・ OCaml https://ocaml.org/manual/latest/api/Sys.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.system/ ・ Swift https://developer.apple.com/documentation/foundation/processinfo
> 9 言語の綴りは [facts/naming-conventions.md §20](facts/naming-conventions.md) と [facts/categories.md §1.H・§3.10](facts/categories.md) の確認済みの表から取りました。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| argv（引数） | `env.args`（argv[0] を除く）・`process.args`（含む）・`args.raw`（= env.args） | `env::args`（含む） | `os.Args`（含む） | `sys.argv`（含む） | `process.argv`（含む）・`Deno.args`（除く） | — | `System.argv`（除く） | `Sys.argv`（含む） | `main(args)`（除く） | `CommandLine.arguments`（含む） |
| 環境変数を 1 つ読む | `env.get`・`process.env` | `env::var` | `os.LookupEnv` | `os.environ.get` | `process.env.K` | — | `System.get_env/1` | `Sys.getenv_opt` | `System.getenv`(JVM) | `ProcessInfo.environment[k]` |
| 環境変数を全部読む | — | `env::vars` | `os.Environ` | `os.environ` | `process.env` | — | `System.get_env/0` | `Unix.environment` | `System.getenv()`(JVM) | `ProcessInfo.environment` |
| 環境変数を設定 | `env.set` | `env::set_var` | `os.Setenv` | `os.environ[k] = v` | `process.env.K = v` | — | `System.put_env` | `Unix.putenv` | — | `setenv`(libc) |
| 環境変数を消す | — | `env::remove_var` | `os.Unsetenv` | `del os.environ[k]` | `delete process.env.K` | — | `System.delete_env` | — | — | `unsetenv`(libc) |
| 作業ディレクトリ | `env.cwd` | `env::current_dir` | `os.Getwd` | `os.getcwd` | `process.cwd()` | — | `File.cwd` | `Sys.getcwd` | JVM `user.dir` | `FileManager.currentDirectoryPath` |
| OS 名 | `env.os` | `env::consts::OS` | `runtime.GOOS` | `sys.platform` | `process.platform` | — | `:os.type` | `Sys.os_type` | — | `ProcessInfo.operatingSystemVersion` |
| 一時ディレクトリ | `env.temp_dir`・`fs.temp_dir` | `env::temp_dir` | `os.TempDir` | `tempfile.gettempdir` | `os.tmpdir()` | — | `System.tmp_dir` | `Filename.get_temp_dir_name` | JVM `java.io.tmpdir` | `FileManager.temporaryDirectory` |
| 待つ（sleep） | `env.sleep_ms`・`process.sleep` | `thread::sleep` | `time.Sleep` | `time.sleep` | `setTimeout`（Node `timers/promises`） | — | `Process.sleep` | `Unix.sleep` | `Thread.sleep`(JVM) | `Thread.sleep` |
| 現在時刻（実時間） | `env.unix_timestamp`・`env.millis`・`datetime.now` | `time::SystemTime` | `time.Now` | `time.time` | `Date.now()` | — | `System.os_time` | `Unix.time` | `Clock.System.now()` | `Date()` |

### 完全性の規則

- `env` は「このプロセスが外から受け取ったもの」の持ち主です（R-L1-25）。argv、環境変数、作業ディレクトリ、OS 名、待ち。
- 環境変数は {1 つ読む `get`, 全部読む `vars`, 設定 `set`, 消す `remove`} の 4 つをそろえます。`set` があって `remove` が無いと、子プロセスに渡す前に変数を消す手段がありません。
- 時計は `datetime`、一時ディレクトリは `fs`、子プロセスと終了と pid は `process`、フラグの読み取りは `args` が持ちます。`env` にはそれらの別名を置きません（R-L0-01）。
- 意図して置かないもの: argv[0]（プログラム名）。実測では、native の `almide run` では一時ディレクトリの中のバイナリのパス、wasm では `wasm-harness` が入ります（probe-args）。どの実行の仕方でも同じ値にならないので、移植できるプログラムは頼れません（R-L0-15）。

### L2 の判定

- 重複: `env.temp_dir`＝`fs.temp_dir`、`env.unix_timestamp`＝`datetime.now`、`process.env`＝`env.get`、`process.sleep`＝`env.sleep_ms`、`args.raw`＝`env.args` → R-env-01
- 置き場所違い: `env.millis`（時計なので `datetime`）→ R-env-01
- 抜けている役割: 環境変数を全部読む（7/9）、消す（6/9）→ R-env-02
- 呼び出しの形の食い違い: `env.get` だけ `??` を `!` なしで書ける → R-env-03（L0 の R-L0-17 で解決）

## 案

### R-env-01 OS まわりの持ち分を 1 つにする — ○ 採用

- 内容:
  - 撤去: `env.temp_dir`（→ `fs.temp_dir()`）、`env.unix_timestamp`（→ `datetime.now()`）
  - 移動: `env.millis` → `datetime.now_ms()`（L2-datetime 側で追加。同じ本体）
  - ここに寄せる側（各ページで撤去）: `process.env(k)` → `env.get(k)`、`process.sleep(ms)` → `env.sleep_ms(ms)`、`args.raw()` → `env.args()`
  - `process.args()`（argv[0] を含む）は撤去し、置き換え先は `env.args()` です。argv[0] を読む関数は足しません（上の完全性の規則）。
  - `env.sleep_ms` の説明「a negative ms hangs」は古いので直します。実測では負の値ですぐ戻ります（probe-env、native と wasm の両方）。
- 根拠の L0 規則: R-L0-01（同じ役割に綴り 1 つ）、R-L0-02（主語の型・持ち主のモジュールに置く）、R-L0-13（`process.env` は純粋な fn のまま環境を読む穴）。L1 の R-L1-25。
- (a) argv と環境変数を同じモジュールに置くのは Rust `std::env`、Go `os`、Elixir `System`、OCaml `Sys` です。時刻を `env` に置くのは Elixir と OCaml だけで、Rust と Go は別のモジュールです。
- (b) native/wasm: どれも残る側の本体をそのまま使います。`env.get`・`env.args`・`env.sleep_ms`・`env.cwd`・`env.os`・`env.temp_dir` は p3 component では `wasi:cli/environment@0.3.0` と monotonic clock の `wait-for` で、組み込み host では既存の op で動いています（`proofs/target-availability.toml` に壁の行なし）。`datetime.now_ms` は `wasi:clocks/wall-clock`。ADR-0027 D3 の検品では、時計は「窓の中にあるか」、sleep は「単調時計の差が k ms 以上か」の verdict 行で比べます。
- (c) `env.temp_dir` と `fs.temp_dir` の両方が文書に出ています（CHEATSHEET は fs、llms.txt は env）。片方に寄せれば、どちらで覚えた LLM も 1 つの答えに行き着きます。argv[0] の有無が `env.args` と `process.args` で違うのは、取り違えると 1 つずれる種類の誤りです。
- (d) 撤去と移動。互換: `env.temp_dir` に `@deprecated(since=15, use="fs.temp_dir")`、`env.unix_timestamp` に `use="datetime.now"`、`env.millis` に `use="datetime.now_ms"`、`[[deprecation]]` で `removal_epoch = 16`（`process.args`・`process.env`・`process.sleep` は L2-process R-process-01、`args.raw` は L2-args R-args-02 に同じ形で書きます）。`almide fix` は 1 対 1 で書き換えます（`env.temp_dir()` → `fs.temp_dir()`、`env.unix_timestamp()` → `datetime.now()`、`env.millis()` → `datetime.now_ms()`）。使用数（spec/tests/docs/stdlib）: `env.temp_dir` 9/0/22/3、`env.unix_timestamp` 5/0/16/5、`env.millis` 11/7/21/3。
- ゲート案: 同義語ゲートの規則 5（モジュールをまたぐ同じ本体、#3851 F）と、R-L0-01 の「ほかの公開関数を 1 回呼ぶだけの関数（転送）も同じ役割に数える」で、`args.raw` 型の転送が戻ってこないようにします。
- issue: #3851 B（既存）。`env.millis` の移動は同じ issue に追記します。

### R-env-02 `env.vars` と `env.remove` を足す — ○ 採用

- 内容: `effect fn vars() -> Map[String, String]`（全部の環境変数。キーの順に並べた map）、`effect fn remove(name: String) -> Unit`（このプロセスとこの後に起こす子プロセスから消す。無くてもよい）
- 根拠の L0 規則: R-L0-15（WASI に乗るものだけ足す）、L1 の判定基準 1（5 言語以上）。
- (a) 全部読むのは 7/9、消すのは 6/9 です。綴りは割れています（`vars`/`Environ`/`environ`/`get_env`）。`set` と対になるよう、Rust の `vars` と `remove_var` に近い `vars`・`remove` にします。
- (b) `vars` は `wasi:cli/environment` の `get-environment` そのものです。`remove` は `env.set` と同じく、p1 shim の guest 側の上書き記録（op 37、#3223）で wasm でも同じに動きます。新しい host op は要りません。結果は ADR-0027 D2 の固定された env で決定的です。
- (c) 「全部の環境変数を出す」課題で、LLM は他言語の癖で 1 関数を探します。無いと書けません。
- (d) 追加。使用数 0。
- ゲート案: env の 4 つ組（get/vars/set/remove）が全部あることを、family の行列テストで確かめます（`tests/env_variable_family_gate_test.rs`）。
- 実装（#3908）: `remove` は `set` と同じ名前の検査（空の名前、`=`、NUL）をするので、`set` と同じく `Result[Unit, String]` を返します（`env.remove: empty name` など）。
- issue: #3908

### R-env-03 `env.get` を `??` で受ける形を、自前の effect fn と同じにする — ○ 採用（R-L0-17 に従う）

- 実測（probe-io）: 戻り型が `String?` の effect fn でも、本体が `@intrinsic` か Almide かで呼び出しの型が変わります。
  - `env.get("X") ?? "d"` は通り、`env.get("X")!` は none を err `"none"` として伝えます（`match env.get("X")! { some(..) => … }` は「String に some は当たらない」エラー）。`io.read_line`（R-io-01 で `Option` を返すようになる版）も同じです。
  - `args.option("x") ?? "d"` はエラーで、`args.option("x")! ?? "d"` と書く必要があります（#3522 の形）。
- 内容: L0 の R-L0-17（呼び出しの型は宣言だけで決まる）に従い、`env.get`・`io.read_line`・`args.option`・`args.positional_at` の呼び出しの型を、宣言 `effect fn … -> T?` だけから決まる 1 つの形にそろえます。どちらの形にそろえるか（`??` を直接書ける形か、`!` の後に `??` を書く形か）は、ADR-0002 と ADR-0008 に沿って R-L0-17 の issue で設計します。このページの関数は、その結果に従うだけで、名前と宣言は変えません。
- 根拠の L0 規則: R-L0-17。
- (a) 該当なし（effect と Option を別の軸で持つ言語はありません）。
- (b) 検査器だけの変更です。両 target で同じです。
- (c) 同じ宣言の関数が本体の書き方で呼び方を変えると、LLM は宣言を読んでも正しい呼び方を決められません。
- (d) 言語の変更です。揃えた結果がいまの呼び出しを変える場合は、R-L0-17 のとおり epoch 15 の `breaks` に入れ、ヒント（「`env.get(k)! ?? d` と書く」など、揃えた側の形）と `almide fix` の書き換えを付けます。関数の名前の変更はありません。
- ゲート案: R-L0-17 のゲート（標準の effect fn すべてで、intrinsic 版と Almide 版の `??` と `!` の受け付けが同じになる fixture）に、`env.get` と `args.option` の組を入れます。
- issue: R-L0-17 と同じ issue（新規）
- issue: #3900

### R-env-04 待ち時間・タイムアウトの引数を `Duration` にする — × 不採用

- 内容: `env.sleep_ms(ms: Int)`、`net.tcp_set_timeout(stream, timeout_ms: Int)`、`net.tcp_read_timeout`、`unix.select(xs, timeout_ms: Int)`、`process.stop(c, grace_ms: Int)` の時間の引数を `Duration` にし、`sleep_ms` を `sleep` に改名する案です。
- 理由: 9 言語で待ち時間を時間の型で受け取るのは Go（`time.Duration`）・Rust（`Duration`）・Swift（`Duration`）・Kotlin（`delay` の `Duration` 版）の 4 つで、5 言語の基準に届きません。JS・Elixir・Kotlin はミリ秒の整数、Python・OCaml は秒の数で受け取ります。Almide は量を単位つきの名前の `Int` で表すと決めています（`sleep_ms`・`add_seconds`。L2-datetime の「時間の量を受け取る関数の型」）。`Duration` は ADR-0001 のとおり `fan` の予算と期限のための型で、数値を読み出す関数を持ちません（S7）。待ちの関数だけ `Duration` にすると、`Int` のミリ秒を持っている呼び出し側が `duration.ms(x)` で包む手間が増え、名前に単位がある今の形より読み誤りが減る根拠もありません。
- 再検討の条件: `Duration` に読み出し関数を足す判断（ADR-0001 S7 の見直し）をしたとき。
- issue: なし

### R-env-05 資格情報を `Secret[String]` で読む `env.secret` を足す — ○ 採用（2026-10-26）

- いま: API のトークンや webhook の鍵は `env.get` で `String` として読みます。`String` はどの描画にもそのまま出て（ログ、assert の失敗）、`==` は最初に違うバイトで返るので、比較の時間からトークンが漏れます（セキュリティ表の行 20）。
- 案: `effect fn env.secret(name: String) -> Secret[String]?`。`env.get(name)` と同じ値を `secret.new` で包んで返します。値はどこでも `Secret(***)` と表示し（C-425）、`==` は定数時間の比較です（C-426）。`env.get` は据え置きます。
- 根拠: 秘密は生まれた所から `Secret` にする（L2-secret）。`hash.hmac_sha256` の鍵が `Secret[Bytes]` になったので（L2-hash の R-hash-07）、その鍵の出どころも `Secret` で返します。
- 契約: C-438（`spec/wasm_cross/env_secret.almd`）。
- issue: なし
## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `env.args` | `effect fn args() -> List[String]` | Rust `env::args` / Go `os.Args` / Python `sys.argv` / Elixir `System.argv`（argv[0] を除くのは Elixir・Kotlin・Deno） | 据え置き | R-env-01（ここに集める） |
| `env.get` | `effect fn get(name: String) -> String?` | Rust `var` / Go `LookupEnv` / Python `environ.get` / OCaml `getenv_opt` / Elixir `get_env` | 据え置き | R-env-01、R-env-03 |
| `env.set` | `effect fn set(name: String, value: String) -> Unit` | Rust `set_var` / Go `Setenv` / Elixir `put_env` | 据え置き（空の名前、名前の `=` と NUL、値の NUL は err。#4200、C-421） | — |
| `env.cwd` | `effect fn cwd() -> String` | Rust `current_dir` / Go `Getwd` / Python `getcwd` / OCaml `getcwd` | 据え置き | — |
| `env.os` | `fn os() -> String` | Rust `consts::OS` / Go `GOOS` / OCaml `os_type` | 据え置き（target ごとの定数。C-189） | — |
| `env.sleep_ms` | `effect fn sleep_ms(ms: Int) -> Unit` | Rust `thread::sleep` / Go `time.Sleep` / Python `time.sleep` / Elixir `Process.sleep` | 据え置き（説明文だけ直す） | R-env-01 |
| `env.temp_dir` | `effect fn temp_dir() -> String` | Rust `env::temp_dir` / Go `os.TempDir` / Elixir `System.tmp_dir` | 撤去（→ `fs.temp_dir`） | R-env-01 |
| `env.unix_timestamp` | `effect fn unix_timestamp() -> Int` | Python `time.time` / Elixir `os_time` | 撤去（→ `datetime.now`） | R-env-01 |
| `env.millis` | `effect fn millis() -> Int` | JS `Date.now()` / Elixir `os_time(:millisecond)` | 撤去（→ `datetime.now_ms` へ移動） | R-env-01 |
| `env.vars` | `effect fn vars() -> Map[String, String]` | Rust `vars` / Go `Environ` / Python `environ` / Elixir `get_env/0` | 追加 | R-env-02 |
| `env.remove` | `effect fn remove(name: String) -> Result[Unit, String]` | Rust `remove_var` / Go `Unsetenv` / Elixir `delete_env` | 追加 | R-env-02 |
| `env.secret` | `effect fn secret(name: String) -> Secret[String]?` | — | 追加（値は `Secret` で生まれる） | R-env-05 |

## L0 への差し戻し（処理済み）

1. **Option を返す effect fn の呼び出しの型**（R-env-03）: L0 に R-L0-17（呼び出しの型は宣言だけで決まる）が入りました。`@intrinsic` の本体なら `Option[T]`、Almide の本体なら `Result[Option[T], String]` という 2 通りの扱いをやめ、宣言から決まる 1 つの形にそろえます。どちらの形にするかの設計は R-L0-17 の issue（新規）で行います。影響を受けるのは `env.get`、`io.read_line`、`args.option`、`args.positional_at`、利用者の effect fn です。R-env-03 は ○ にしました。
