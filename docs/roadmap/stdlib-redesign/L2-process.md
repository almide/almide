<!-- description: Stdlib redesign L2/L3 — process: one ProcessCommand record replaces the exec_in/exec_with_stdin/exec_status/exec_status_timeout axes, a start/poll/wait/stop handle (#2587) replaces spawn/is_alive, and the env/argv/sleep/stdin twins leave -->
# L2/L3: process

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-25: 子プロセス・自分の終了・pid は process の持ち分）
> 出典: Rust https://doc.rust-lang.org/std/process/ ・ Go https://pkg.go.dev/os/exec ・ Python https://docs.python.org/3/library/subprocess.html ・ TS/JS https://nodejs.org/api/child_process.html ・ Gleam https://gleam-stdlib.hexdocs.pm/ （無い）・ Elixir https://hexdocs.pm/elixir/System.html#cmd/3 ・ OCaml https://ocaml.org/manual/latest/api/Sys.html （`command`。`Unix.create_process` は unix ライブラリ）・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.system/exit-process.html （子プロセスは JVM の ProcessBuilder）・ Swift https://github.com/swiftlang/swift-subprocess
> 各言語のハンドル API の比較は #2587 本文の表（出典つき）を使いました。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 実行して stdout を受け取る（失敗は err） | `exec` | `Command::output` | `Cmd.Output` | `check_output` | `execFileSync` | — | `System.cmd` | — | `ProcessBuilder` | `Process` |
| 実行して終了コードと出力を受け取る | `exec_status` | `Command::output` | `Cmd.CombinedOutput`+`ExitCode` | `run(capture_output=)` | `spawnSync` | — | `System.cmd` | `Sys.command` | `ProcessBuilder` | `Subprocess.run` |
| 作業ディレクトリを指定 | `exec_in`・`run_in` | `current_dir` | `Cmd.Dir` | `cwd=` | `{cwd}` | — | `cd:` | — | `directory()` | `workingDirectory` |
| stdin を流し込む | `exec_with_stdin` | `stdin(piped)` | `Cmd.Stdin` | `input=` | `{input}` | — | — | — | `redirectInput` | `input:` |
| 時間切れで止める | `exec_status_timeout` | — | `CommandContext` | `timeout=` | `{timeout}` | — | — | — | `waitFor(t)` | — |
| 端末につないで実行 | `run`・`run_in`・`exec_attached`（非推奨） | `Command::status` | `Cmd.Run` | `run()` | `{stdio: 'inherit'}` | — | `into: IO.stream` | `Sys.command` | `inheritIO` | `Process.run` |
| 裏で起動し、待たずに様子を見る | `spawn`・`is_alive` | `spawn`+`try_wait` | `Start`+`Wait` | `Popen.poll` | `spawn` | — | `Port.open` | `Unix.create_process` | `start`+`isAlive` | `Subprocess.run` の中 |
| 止める | `kill`（シグナル 1 回） | `kill` | `Process.Kill`・`Cmd.Cancel` | `terminate`・`kill` | `kill` | — | `Port.close` | `Unix.kill` | `destroy` | `teardownSequence` |
| 自分を終える | `exit` | `process::exit` | `os.Exit` | `sys.exit` | `process.exit` | — | `System.halt` | `exit` | `exitProcess` | `exit` |
| 自分の pid | `pid` | `process::id` | `os.Getpid` | `os.getpid` | `process.pid` | — | `System.pid` | `Unix.getpid` | `ProcessHandle.current().pid()` | `ProcessInfo.processIdentifier` |
| argv・環境変数・sleep・stdin | `args`・`env`・`sleep`・`stdin_lines` | （`env`・`thread`・`io` に置く） | （`os`・`time`） | （`sys`・`os`・`time`） | （`process` に同居） | — | （`System`） | （`Sys`） | — | （`CommandLine`・`ProcessInfo`） |

### 完全性の規則

- **子プロセスの設定は 1 つのレコード `ProcessCommand` に集めます。** {コマンド, 引数, 作業ディレクトリ, stdin の文字列, 時間切れ} のどの組み合わせも書けるようにし、関数名の組み合わせ（`exec_in_with_stdin` のような）を作りません。
- **実行の形は 3 つに閉じます。**
  1. 一度に実行して結果のレコードを受け取る: `output(c) -> ProcessStatus`（どの終了コードも ok。err は起動できなかったときと時間切れだけ）
  2. 待たずに動かすハンドル: `start`・`poll`・`wait`・`stop`・`read_new_stdout`・`read_new_stderr`
  3. 端末につないで実行: `run`・`run_in`（端末のフォアグラウンドに居続ける必要があるので別物。#2540）
- 近道は `exec(cmd, args) -> String`（終了コード 0 なら stdout、そうでなければ stderr を err）1 つだけです。`http.get` と同じ「近道 1 つ ＋ 結果のレコード 1 つ」の形です。
- 結果のレコードは `ProcessStatus { code, stdout, stderr }` の 1 種類です（シグナルで終わったら `code = -1`。いまと同じ）。
- argv・環境変数・待ち・stdin は持ちません（R-L1-25。`env`・`io` の持ち分）。

### L2 の判定

- 軸の掛け算と穴: {stdout だけ, 全部} × {dir, stdin, timeout} のうち 4 つのセルだけが関数になっていて、`exec_status_in` や `exec_with_stdin_timeout` はありません → R-process-02
- 既知の欠陥: `spawn` は孫を残し、出力を親の端末に流し、終了コードを捨てます（#2587 の再現）→ R-process-03
- 重複・置き場所違い: `process.args`、`process.env`（純粋な fn で環境を読む穴）、`process.sleep`、`process.stdin_lines` → R-process-01
- 宣言の穴: `process.pid` と `process.is_alive` が純粋な `fn` で host の状態を読みます。実測で、純粋な fn から呼べます（probe-io）→ R-process-04
- 非推奨のまま期日のないもの: `exec_attached`（#3851 A）→ R-process-02

## 案

### R-process-01 argv・環境変数・待ち・stdin を持ち主に返す — ○ 採用

- 内容: `process.args()` → `env.args()`（argv[0] は返さない。L2-env の完全性の規則）、`process.env(k)` → `env.get(k)`、`process.sleep(ms)` → `env.sleep_ms(ms)`、`process.stdin_lines()` → `string.lines(io.read_all()!)`。
- 根拠の L0 規則: R-L0-01、R-L0-02、R-L0-13、R-L1-25。
- (a) Rust・Go・Python・Elixir は、子プロセスのモジュールに argv や環境変数を置きません（facts §3.10）。
- (b) 残る側はどれも `wasi:cli` と `wasi:clocks` で wasm に乗っています。`process.env` と `process.stdin_lines` は wasm でも動いています（台帳の 2026-09-21 の注記）。
- (c) `process.args` と `env.args` は argv[0] の有無だけが違い、取り違えると 1 つずれます（#3851「Dropped」）。
- (d) 撤去。`@deprecated(since=15, use="env.args")`・`use="env.get"`・`use="env.sleep_ms"`・`use="string.lines(io.read_all()!)"`、どれも `removal_epoch = 16`。`almide fix` は `process.env`・`process.sleep`・`process.stdin_lines` を書き換えます。`process.args()` は argv[0] の有無で結果が 1 つずれるので、機械的に書き換えず、ヒント「`env.args()` は argv[0]（プログラム名）を含みません。添字を 1 つ減らしてください」だけを出します。使用数: `args` 5/10/30/4、`env` 13/0/20/0、`sleep` 4/0/12/0、`stdin_lines` 0/3/11/3。
- ゲート案: 同義語ゲート規則 5（#3851 F）。
- issue: #3851 B（既存）

### R-process-02 `ProcessCommand` と `output` で一度きりの実行をまとめる — ○ 採用

- 内容:
  ```almide
  type ProcessCommand = { cmd: String, args: List[String], dir: String = "", stdin: String = "", timeout_ms: Int = 0 }
  effect fn output(c: ProcessCommand) -> ProcessStatus
  ```
  `dir = ""` はいまの作業ディレクトリ、`timeout_ms = 0` は時間切れなし。時間切れでは子をプロセスグループごと止めて err にします（C-214 の文言をそのまま使う）。
  - 撤去: `exec_status`（→ `output(process.ProcessCommand { cmd, args })`）、`exec_status_timeout`（→ `timeout_ms`）、`exec_in`・`exec_with_stdin`（→ `output` ＋ `code` の確認）、`exec_attached`（epoch 10 から非推奨。→ `run`）。
  - 据え置き: `exec`（近道）、`run`、`run_in`、`exit`、`kill`。
  - 型の名前は `ProcessStatus`・`HttpRequest` の前例に合わせて `Process` を頭に付けます（生成する Rust で利用者の型名とぶつけないため。`stdlib/http.almd` 冒頭の理由）。
- 書き方: 型名つきのレコード式 `process.ProcessCommand { cmd: "git", args: ["status"], dir: "repo" }` で、既定値のフィールドを省けます（実測、probe-process）。型名のない `{ cmd: …, args: … }` では省けず E005 になります。標準の書き方は型名つきの形に決め、CHEATSHEET と `docs/stdlib/process.md` の例もこの形で書きます（L0 への差し戻し 2、処理済み）。
- 根拠の L0 規則: R-L0-01（同じ実行に綴りを 1 つ）、CLAUDE.md の「族は行列で」。行列の穴を関数名で埋めると 2^3 個になるので、レコードの欄にします。#2587 の「決めたいこと 2」への答えです。
- (a) 設定を 1 つの値に集めるのは多数派です（Rust `Command` のビルダー、Go `exec.Cmd` のフィールド、Python のキーワード引数、Node の options、Swift の `Configuration`）。
- (b) wasm: `almide:process/spawn` の private host capability（ADR-0025）。stock WASI に子プロセスはありません（台帳の `host-capability` の行）。`output` は既存の op `exec-status-timeout` に dir と stdin を足した 1 つの新しい `enum op` の case で、ADR-0025 §3 の 4 の手順（WIT テスト・host の腕・cross fixture を 1 つの変更で）に従います。検品は ADR-0027 D4 の固定のコマンド集合。
- (c) `exec_status_in` のような、規則から推測できるのに存在しない名前がなくなります。
- (d) 追加と撤去。`exec_status`・`exec_status_timeout`・`exec_in`・`exec_with_stdin` に `@deprecated(since=15, use="process.output")`、`exec_attached` に `@deprecated(since=15, use="process.run")`、どれも `removal_epoch = 16`。`almide fix` の書き換え: `exec_status(c, a)` → `output(process.ProcessCommand { cmd: c, args: a })`、`exec_status_timeout(c, a, t)` → `output(process.ProcessCommand { cmd: c, args: a, timeout_ms: t })`、`exec_attached(c, a)` → `run(c, a)`（どれも 1 対 1）。`exec_in`・`exec_with_stdin` は戻り値の形が変わる（`String` → `ProcessStatus`）のでヒントのみ（「`output(process.ProcessCommand { cmd, args, dir })` の `code` が 0 なら `stdout`」）。使用数: `exec_status` 10/5/10/0、`exec_status_timeout` 9/8/14/0、`exec_in` 1/2/6/0、`exec_with_stdin` 1/5/6/0、`exec_attached` 1/1/4/0。
- ゲート案: `process` の公開関数の集合を {近道 `exec`, `output`, ハンドル 6 つ, `run`・`run_in`, `exit`, `kill`, `pid`} に固定する族ゲート。`ProcessCommand` の欄を足すときは、`output` と `start` の両方の fixture を同じ PR に要求します。
- issue: #2587（既存。決めたいこと 2 の答え）、新規（撤去）

### R-process-03 ハンドル（#2587）を足し、`spawn`・`is_alive` を置き換える — ○ 採用

- 内容:
  ```almide
  effect fn start(c: ProcessCommand) -> ProcessChild          // 自分のプロセスグループで起動。stdin は stdin 欄、出力はランタイムが吸い上げる
  effect fn poll(c: ProcessChild) -> ProcessStatus?            // 待たない。終わっていれば片付けて結果
  effect fn wait(c: ProcessChild) -> ProcessStatus             // 終わるまで待つ
  effect fn stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus  // グループに SIGTERM → grace_ms 待つ → SIGKILL → 片付け。何度呼んでもよい
  effect fn read_new_stdout(c: ProcessChild) -> String         // 前回から届いた分。待たない
  effect fn read_new_stderr(c: ProcessChild) -> String
  ```
  撤去: `spawn`（→ `start`）、`is_alive`（→ `poll(c)! == none`）。`kill(pid, signal)` は、自分の子でない pid に送る役割があるので残します。
- 綴りの決め方（R-L0-01、モジュールをまたぐ）: 同じ役割の既存の綴りを使います。待たずに確かめる → `http.poll`、終わるまで待つ → `http.wait`、前回からの差分を読む → `http.read_new`。#2587 の叩き台の `try_wait`・`read_stdout` は使いません。
- #2587 の決めたいことへの答え: 1 `grace_ms` は必須の引数（既定値の引数は言語にない）。2 `ProcessCommand`（R-process-02）。3 結果は `ProcessStatus` の 1 種類にし、別の `Exit` 型を足さない（同じ結果に型を 2 つ作らない。R-L0-01）。4 ハンドルを捨てても止めず、プログラムの終了時にランタイムが子のツリーを止める。
- 時間の引数は `Int` のミリ秒にします。このモジュールと `env.sleep_ms`・`HttpLimits`・`net.tcp_read_timeout` に合わせ、L2-datetime の差し戻しで「時間の量は単位を名前に含めた `Int`、`Duration` は `fan` の予算と期限だけ」と決めたとおりです（L0 への差し戻し 1、処理済み）。
- (a) 起動の設定と動いている子のハンドルを分け、待たない確認を持つのは Rust・Go・Python・Zig・.NET（#2587 の表）。止め方を「穏やかなシグナル → 猶予 → 強制」にするのは Go・execa・trio・swift-subprocess。
- (b) wasm: R-process-02 と同じ `almide:process/spawn` に `enum op` の case（start・poll・wait・stop・read）を足します（ADR-0025 §3 の 4）。stock WASI では読み込み時に拒否されます（host-capability）。ADR-0027 D3 の verdict: `poll` は動いている間 none、終わった後 some。
- (c) `spawn` の 3 つの欠陥（孫が残る、出力が混ざる、終了コードが消える）は、LLM がテストを書いても見つけにくい種類です。
- (d) 追加と撤去。`spawn` は `@deprecated(since=15, use="process.start")`、`is_alive` は `@deprecated(since=15, use="process.poll")`、どちらも `removal_epoch = 16`。引数が pid からハンドルに変わるので、`almide fix` は書き換えず、ヒントのみ出します（「`let c = process.start(process.ProcessCommand { cmd, args })!` で起動し、`process.poll(c)! == none` で動いているかを確かめる」）。使用数: `spawn` 10/4/8/0、`is_alive` 12/0/8/0。
- ゲート案: #2587 の完了条件（`stop` の後に孫が残らない、`poll` の none → some、出力が親の stdout に出ない、`exit` で子のツリーが残らない）をそのまま fixture にします。ハンドルの動詞の集合を `http_call_family_gate_test.rs` と同じ形で固定します。
- issue: #2587（既存）

### R-process-04 `pid` を effect fn にする — ○ 採用

- 内容: `fn pid() -> Int` を `effect fn pid() -> Int` にします（`is_alive` は R-process-03 で撤去）。
- 根拠の L0 規則: R-L0-13（環境を読む関数は effect）、ADR-0022 D3（純粋な fn は外を読まない）。
- (b) 検査器だけの変更です。(c) 同じ値を返す関数が純粋か effect かを、モジュールで覚えなくてよくなります。
- (d) 意味変更（純粋 → effect）。名前は変えません。epoch 8・13 の前例どおり epoch 15 の `breaks` に入れます。純粋な fn から呼んでいる箇所は E006 になり、ヒント「呼び出し元を `effect fn` にする」を出します。使用数 2/12/11/0。
- ゲート案: 「host の状態を読む intrinsic は effect」を R-L0-G に入れ、`process.pid` を許容リストから外します。
- issue: #3907

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `process.exec` | `effect fn exec(cmd: String, args: List[String]) -> String` | Go `Output` / Python `check_output` / Node `execFileSync` | 据え置き（近道） | R-process-02 |
| `process.exec_in` | `effect fn exec_in(dir: String, cmd: String, args: List[String]) -> String` | Rust `current_dir` / Python `cwd=` | 撤去（→ `output`） | R-process-02 |
| `process.exec_with_stdin` | `effect fn exec_with_stdin(cmd: String, args: List[String], input: String) -> String` | Python `input=` / Node `{input}` | 撤去（→ `output`） | R-process-02 |
| `process.exec_status` | `effect fn exec_status(cmd: String, args: List[String]) -> ProcessStatus` | Rust `output` / Python `run` | 撤去（→ `output`） | R-process-02 |
| `process.exec_status_timeout` | `effect fn exec_status_timeout(cmd: String, args: List[String], timeout_ms: Int) -> ProcessStatus` | Python `timeout=` / Go `CommandContext` | 撤去（→ `output` の `timeout_ms`） | R-process-02 |
| `process.output` | `effect fn output(c: ProcessCommand) -> ProcessStatus`（案） | Rust `Command::output` / Go `Cmd.Output` | 追加 | R-process-02 |
| `process.run` | `effect fn run(cmd: String, args: List[String]) -> Int` | Rust `status` / Go `Run` / Python `run()` | 据え置き | — |
| `process.run_in` | `effect fn run_in(dir: String, cmd: String, args: List[String]) -> Int` | 同上 ＋ cwd | 据え置き | — |
| `process.exec_attached` | `effect fn exec_attached(cmd: String, args: List[String]) -> Int`（非推奨） | — | 撤去（→ `run`） | R-process-02 |
| `process.spawn` | `effect fn spawn(cmd: String, args: List[String]) -> Int` | Rust `spawn` / Node `spawn` / Go `Start` | 撤去（→ `start`） | R-process-03 |
| `process.start` | `effect fn start(c: ProcessCommand) -> ProcessChild`（案） | Go `Start` / Rust `spawn` / Kotlin `start` | 追加 | R-process-03 |
| `process.poll` | `effect fn poll(c: ProcessChild) -> ProcessStatus?`（案） | Python `Popen.poll` / Rust `try_wait` | 追加 | R-process-03 |
| `process.wait` | `effect fn wait(c: ProcessChild) -> ProcessStatus`（案） | Rust・Go・Python `wait` | 追加 | R-process-03 |
| `process.stop` | `effect fn stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus`（案） | Go `Cancel`+`WaitDelay` / execa `forceKillAfterDelay` | 追加 | R-process-03 |
| `process.read_new_stdout` | `effect fn read_new_stdout(c: ProcessChild) -> String`（案） | — （`http.read_new` に合わせる） | 追加 | R-process-03 |
| `process.read_new_stderr` | `effect fn read_new_stderr(c: ProcessChild) -> String`（案） | 同上 | 追加 | R-process-03 |
| `process.is_alive` | `fn is_alive(pid: Int) -> Bool` | Kotlin `isAlive` / Rust `try_wait` | 撤去（→ `poll`） | R-process-03 |
| `process.kill` | `effect fn kill(pid: Int, signal: Int) -> Unit` | Rust `kill` / Python `os.kill` / Node `process.kill` | 据え置き | R-process-03 |
| `process.exit` | `effect fn exit(code: Int) -> Never` | Rust・Go・Python・OCaml・Swift `exit` | 据え置き | — |
| `process.pid` | `fn pid() -> Int` | Rust `id` / Go `Getpid` / Node `pid` | 意味変更（effect fn にする） | R-process-04 |
| `process.args` | `effect fn args() -> List[String]` | Rust `env::args` | 撤去（→ `env.args`） | R-process-01 |
| `process.env` | `fn env(key: String) -> String?` | Rust `env::var` | 撤去（→ `env.get`） | R-process-01 |
| `process.sleep` | `fn sleep(ms: Int) -> Unit` | Rust `thread::sleep` | 撤去（→ `env.sleep_ms`） | R-process-01 |
| `process.stdin_lines` | `effect fn stdin_lines() -> List[String]` | Python `readlines` | 撤去（→ `string.lines(io.read_all()!)`） | R-process-01 |

## L0 への差し戻し（処理済み）

1. **実時間の長さの引数の型**: L2-datetime の差し戻しで決まりました（`Duration` にはしない）。時間の量は、単位を名前に含めた `Int`（`sleep_ms`、`timeout_ms`、`grace_ms`）で揃えます。`Duration` は ADR-0001 のとおり `fan` の予算と期限に使う型に留めます。このページの `timeout_ms: Int` と `grace_ms: Int` はそのままです。
2. **型名のない無名レコード式で、既定値のあるフィールドを省けない**（実測: E005「expects Cmd but got { cmd, args }」）: `ProcessCommand` は型名つきのレコード式 `process.ProcessCommand { … }` で書く形のまま残します。この形では既定値のフィールドを省けます（実測）。期待型から既定値を埋める規則は、標準関数の設計ではなく言語の型検査の話なので、このリデザインの範囲の外で扱います。標準の側でできることとして、E005 のヒントに「型名つきの `process.ProcessCommand { … }` で書く」と出すことを R-process-02 のゲート（fixture）に含めます。
