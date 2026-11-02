# process

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](process.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](process.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](process.zh-CN.md)



> 原文：[docs/stdlib/process.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/process.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されず、独立した実測・実装検証は行っていない。API シグネチャとコード内コメントは原文を保持し、説明訳をコード外へ添える。生成索引の固定コピーであり、この訳は生成対象ではない。 原文は冒頭で組込み wasm ホストの全関数対応を述べる一方、バックグラウンド節に「現時点ではネイティブのみ」と記載する。この不一致は原文のまま残し、ここでは実装を再検証していない。

子プロセス、およびこのプログラム自身の終了と pid。`import process`、`effect fn`。

子プロセスの設定は 1 つのレコード `ProcessCommand` にまとめる。

```almd
type ProcessCommand = {
  cmd: String,          // the program
  args: List[String],   // its argument list
  dir: String = "",     // working directory; "" keeps this program's
  stdin: String = "",   // text fed to its stdin; "" gives it none
  timeout_ms: Int = 0,  // deadline; 0 means none
}
```

型名付きでレコードを書くと、デフォルト値のあるフィールドを省略できる：`process.ProcessCommand { cmd: "git", args: ["status"], dir: "repo" }`。型名なし（`{ cmd: ..., args: ... }`）では全フィールドの指定が必要。

プログラムと引数は常に argv リストである。ここではシェル文字列を `sh -c` に渡す処理はなく、引数を再分割・展開することもない。`ProcessCommand` の実行方法は 3 つ。

| 呼出し | 待機するか | 出力 | 返す値 |
|---|---|---|---|
| `output(c)` | 終了まで | 取得する | `ProcessStatus { code, stdout, stderr }` |
| `start(c)` | しない | 取得し、到着に応じて読む | `poll` / `wait` / `stop` 用の `ProcessChild` |
| `run(c)` | 終了まで | この端末へ | 終了コード |

唯一の簡略形は `exec(cmd, args)`。終了 0 なら stdout、それ以外は stderr のテキストを含む `err` を返す。

ここで失敗しうる呼出しはすべて `Result` を返す。`!` を書くか `err` を処理する（ADR-0008）。`poll`、`child_pid`、`read_new_stdout`、`read_new_stderr` は通常の値を返すが、`effect fn` なので呼出し結果も `Result` となり、やはり `!` が必要：`process.child_pid(c)!`。

**ターゲット**（[ADR-0025](../../../../adr/0025-wasm-process-is-a-private-host-capability.md)）。ネイティブは全関数に対応する。`almide run --target wasm` と `almide test` の wasm 経路も全関数に対応し、取得テキスト、増分読取り、終了コード、期限、err 文字列はネイティブと同じ。そこでの `start` の子はホストリソースであり、ホストがパイプと期限を保持する。実行終了時には、待機も停止もしていない子を、ネイティブ終了時と同様に停止する。`almide build
--target wasm` は子を起動するプログラムをビルドするが、その成果物は非公開の `almide:process/spawn` インターフェースをインポートするため、`wasmtime run` などの標準ランタイムはロード時に拒否する。このインポートを実装するホスト上では実行できる。`--component` ビルドは E081 で拒否する。

**`[permissions] proc`**：このモジュールが起動できるコマンドをプロジェクトで列挙できる。その場合、子を起動するすべての呼出しは、一覧にあるコマンドを文字列リテラルで指定しなければならない。指定箇所は `exec` の第 1 引数、または `output`、`run`、`start` の呼出し内に書く `ProcessCommand` の `cmd` フィールド。満たさなければコンパイルされない。

```toml
[permissions]
proc = ["git", "cargo"]
```

## 簡略形

### `process.exec(cmd: String, args: List[String]) -> Result[String, String]`

`cmd` を `args` で実行し、終了 0 なら stdout を返す。それ以外は stderr テキスト（空の場合もある）を含む `err`。

```almd check
import process

effect fn main() -> Unit = {
  let listing = process.exec("ls", ["-la"])!
  println(listing)
}
```

## 1 回実行して全出力を取得

### `process.output(c: ProcessCommand) -> Result[ProcessStatus, String]`

`c` を最後まで実行し、終了コードと書き出した内容全体を返す：`ProcessStatus { code: Int, stdout: String, stderr: String }`。シグナルで子が終了した場合、`code` は `-1`。どの終了コードも `ok` なので、`code` は自分で調べる。`err` になるのは起動不能（プログラムがない、`dir` が存在しない）と期限超過。

```almd check
import process

effect fn main() -> Unit = {
  let st = process.output(process.ProcessCommand { cmd: "git", args: ["status", "--short"], dir: "." })!
  if st.code == 0 then println(st.stdout) else println("git failed: ${st.stderr}")
}
```

`stdin` は子にテキストを渡す。指定しなければ子の stdin は空であり、端末ではない。

```almd check
import process

effect fn main() -> Unit = {
  let st = process.output(process.ProcessCommand { cmd: "sort", args: [], stdin: "b\na\n" })!
  println(st.stdout)
}
```

**期限。** `timeout_ms` は唯一許されるタイムアウト（C-214）。プロセス起動は既にバイト同一性契約の範囲外なので、その時間を制限しても非決定性は増えない。期限を過ぎると子を kill し、err は厳密に `exec timed out after <ms>ms` となる。期限を過ぎるかどうかはホストに依存する。終了コードを比較する呼出し側では、これを 124 に対応させることが多い。

出力を取得する子は独自のプロセスグループで動くので、期限超過時はそのツリー全体を kill する。そのため制御端末にとってはバックグラウンドジョブになる。端末を読む、または設定を変える（`stty -echo`）子は、カーネルにより SIGTTIN / SIGTTOU で停止される。その停止には期限まで待たず即座に err（`...: the child was stopped by SIGTTOU: it tried to use the terminal from a
background process group; run terminal programs with process.run`）で応答する。

```almd check
import process

effect fn main() -> Unit = {
  match process.output(process.ProcessCommand { cmd: "cargo", args: ["build"], timeout_ms: 60000 }) {
    ok(st) => println("exit ${st.code}"),
    err(e) => println(e),
  }
}
```

## この端末上で実行

### `process.run(c: ProcessCommand) -> Result[Int, String]`

このプログラムの端末上で `c` を実行する。子は stdout と stderr（`c.stdin` 未設定なら stdin も）を継承し、このプログラムのプロセスグループに留まる。したがってページャ、エディタ、`stty` など、端末を読む・再設定するプログラムが動作し、長いビルドは実行中に出力を流せる。子の起動前にこのプログラムの stdout をフラッシュする。出力は取得しない。戻り値は終了コード（シグナルで kill された場合は `-1`）。どの終了コードも `ok`。`err` になるのは起動不能（`dir` 不在なら err は `dir` と `cmd` の両方を示す）と `timeout_ms` 超過。

`output` は出力を取得して返し、`run` は端末に接続して終了コードを返す。

```almd check
import process

effect fn main() -> Unit = {
  let code = process.run(process.ProcessCommand { cmd: "cargo", args: ["build"], dir: "." })!
  println("cargo build exited ${code}")
}
```

## バックグラウンドで実行

`start` は直ちに `ProcessChild` ハンドルを返す。子は独自のプロセスグループで動き、stdout と stderr はランタイムが取得する（このプログラムの出力には混ぜない）。読取りには `read_new_stdout` / `read_new_stderr` を使う。現時点ではネイティブのみ（ターゲット節参照）。

### `process.start(c: ProcessCommand) -> Result[ProcessChild, String]`

`c` を起動して直ちに返る。起動できなければ `err`。

### `process.poll(c: ProcessChild) -> Result[ProcessStatus, String]?`

ブロックしない。`c` の実行中は `none`、その後は `some(ok(status))`、期限超過なら `some(err(e))`。`http.poll` と同じ形。

### `process.wait(c: ProcessChild) -> Result[ProcessStatus, String]`

`c` が終了するまでブロックし、その結果を返す。

### `process.stop(c: ProcessChild, grace_ms: Int) -> Result[ProcessStatus, String]`

`c` のプロセスグループに SIGTERM を送り、終了を `grace_ms` 待ってから SIGKILL し、回収する。シグナルはグループ全体に届くので、子自身の子も一緒に停止する。終了後に再度呼ぶと同じ結果を返す。

### `process.read_new_stdout(c: ProcessChild) -> String` / `process.read_new_stderr(c: ProcessChild) -> String`

前回の読取り以降に `c` がそのストリームへ書いた内容。ブロックしない。

### `process.child_pid(c: ProcessChild) -> Int`

`c` の OS プロセス ID。`process.kill` やログ行に使う。

```almd check
import process

effect fn main() -> Unit = {
  let c = process.start(process.ProcessCommand { cmd: "ping", args: ["-c", "3", "127.0.0.1"] })!
  let pid = process.child_pid(c)!
  println("started pid ${pid}")
  match process.poll(c)! {
    none => println("still running"),
    some(ok(st)) => println("exited ${st.code}"),
    some(err(e)) => println(e),
  }
  let st = process.wait(c)!
  let errs = process.read_new_stderr(c)!
  println(st.stdout)
  println(errs)
}
```

```almd check
import process

effect fn main() -> Unit = {
  let c = process.start(process.ProcessCommand { cmd: "sleep", args: ["30"] })!
  let st = process.stop(c, 500)!
  println("stopped, code ${st.code}")
}
```

## シグナル、終了、pid

### `process.kill(pid: Int, signal: Int) -> Result[Unit, String]`

プロセス `pid` に `signal` を送る（例：SIGTERM は 15、SIGKILL は 9）。シグナルは kill(2) システムコールを通し、`PATH` 上の `kill` プログラムは使わない。`pid` は 1..=2147483647 が必須で、`0`（呼出し元のプロセスグループ）と `-1`（ユーザーがシグナル送信できる全プロセス）は拒否する。`signal` は 0..=31（0 は存在確認のみ）。err は固定テキスト：`process.kill(<pid>, <signal>): pid must be in 1..=2147483647`、`…: signal must be in 0..=31`、または kill(2) の失敗として `no such process`、`operation not permitted`、`invalid signal`。`[permissions] proc` 使用時は `kill` を一覧に含める。このプログラムの子に対しては `process.stop` が待機・回収も行う。`kill` は任意の pid 向け。

```almd check
import process

effect fn main() -> Unit = {
  let c = process.start(process.ProcessCommand { cmd: "sleep", args: ["30"] })!
  process.kill(process.child_pid(c)!, 15)!
  let st = process.wait(c)!
  println("code ${st.code}")
}
```

### `process.exit(code: Int) -> Never`

指定されたステータスコード、POSIX 終了ステータス **0〜255** で終了する。ネイティブと `almide run --target wasm` では値をそのまま渡すので、ラッパーは 126、127、128+n を含む子のステータスで終了できる（#2780）。それ以外は定義域エラーとなり、stderr に 1 行 `Error: exit code must be in 0..=255` を出し終了 1。全ターゲットで同じ（C-350）。Rust、Go、Python、Node は代わりに下位 8 ビットへ切り詰めるので、`exit(256)` は成功を示す。

WASI preview-1 ビルド（`almide build --target wasm`、`wasmtime` などの標準ランタイムで実行）が渡せるのは 0..=125 のみ。標準ランタイムは `proc_exit(126)` 以上でトラップするため、このビルド自身が 126..=255 を `Error: a WASI preview-1 build cannot exit with a code in 126..=255` と終了 1 で拒否する。何も示さず 1 で終了することはない。

```almd check
import process

effect fn main() -> Unit = {
  println("fatal: giving up")
  process.exit(1)
}
```

### `process.pid() -> Int`

このプログラムの OS プロセス ID。

```almd check
import process

effect fn main() -> Unit = {
  println(int.to_string(process.pid()))
}
```



## シグネチャ索引（13 関数）

```
// Stdout on exit 0; else err with stderr text.
// @since 0.5.0 or earlier
effect process.exec(cmd: String, args: List[String]) -> String

// Runs c to its end: any exit code is ok; err if it cannot start or passes its deadline.
// @since unreleased
effect process.output(c: ProcessCommand) -> ProcessStatus

// Runs c on this terminal (its stdin too, unless c.stdin is set); the exit code, -1 if signalled.
// @since unreleased
effect process.run(c: ProcessCommand) -> Int

// Starts c in its own process group and returns at once; its output is captured.
// @since unreleased
effect process.start(c: ProcessCommand) -> ProcessChild

// Never blocks: none while c runs, then its outcome (err past its deadline).
// @since unreleased
effect process.poll(c: ProcessChild) -> Option[Result[ProcessStatus, String]]

// Blocks until c ends; its outcome.
// @since unreleased
effect process.wait(c: ProcessChild) -> ProcessStatus

// SIGTERM to c's group, grace_ms to end, then SIGKILL; again after the end is the same outcome.
// @since unreleased
effect process.stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus

// What c wrote to stdout since the last read; never blocks.
// @since unreleased
effect process.read_new_stdout(c: ProcessChild) -> String

// What c wrote to stderr since the last read; never blocks.
// @since unreleased
effect process.read_new_stderr(c: ProcessChild) -> String

// The OS process id of c, for process.kill.
// @since unreleased
effect process.child_pid(c: ProcessChild) -> Int

// Sends signal (0..=31) to pid (> 0) by kill(2); err names why.
// @since 0.12.3 or earlier
effect process.kill(pid: Int, signal: Int) -> Unit

// Ends with code 0..=255; others abort, exit 1.
// @since 0.5.0 or earlier
effect process.exit(code: Int) -> Never

// OS process ID of this program.
// @since 0.12.3 or earlier
effect process.pid() -> Int
```

## 型索引（2 型）

```
// Exit code plus captured stdout and stderr; code -1 if a signal ended the child.
// @since 0.15.0 or earlier
type process.ProcessStatus = { code: Int, stdout: String, stderr: String }

// A child's settings: dir "" keeps this one, stdin "" gives it none, timeout_ms 0 means no deadline.
// @since unreleased
type process.ProcessCommand = { cmd: String, args: List[String], dir: String, stdin: String, timeout_ms: Int }
```



## 索引コメントの説明訳

- `effect process.exec(cmd: String, args: List[String]) -> String` — 終了 0 なら stdout、それ以外は stderr テキストを含む err。 (@since 0.5.0 or earlier)
- `effect process.output(c: ProcessCommand) -> ProcessStatus` — c を最後まで実行。どの終了コードも ok。起動不能または期限超過なら err。 (@since unreleased)
- `effect process.run(c: ProcessCommand) -> Int` — c をこの端末上で実行（c.stdin 未設定なら stdin も）。終了コードを返し、シグナル終了なら -1。 (@since unreleased)
- `effect process.start(c: ProcessCommand) -> ProcessChild` — c を独自のプロセスグループで起動して直ちに返る。出力を取得する。 (@since unreleased)
- `effect process.poll(c: ProcessChild) -> Option[Result[ProcessStatus, String]]` — ブロックしない。実行中は none、その後は結果（期限超過なら err）。 (@since unreleased)
- `effect process.wait(c: ProcessChild) -> ProcessStatus` — c 終了までブロックし、その結果を返す。 (@since unreleased)
- `effect process.stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus` — c のグループへ SIGTERM、grace_ms 待機後 SIGKILL。終了後の再呼出しは同じ結果。 (@since unreleased)
- `effect process.read_new_stdout(c: ProcessChild) -> String` — 前回読取り以降 c が stdout に書いた内容。ブロックしない。 (@since unreleased)
- `effect process.read_new_stderr(c: ProcessChild) -> String` — 前回読取り以降 c が stderr に書いた内容。ブロックしない。 (@since unreleased)
- `effect process.child_pid(c: ProcessChild) -> Int` — c の OS プロセス ID。process.kill 用。 (@since unreleased)
- `effect process.kill(pid: Int, signal: Int) -> Unit` — kill(2) で pid（> 0）に signal（0..=31）を送る。err は理由を示す。 (@since 0.12.3 or earlier)
- `effect process.exit(code: Int) -> Never` — 0..=255 の code で終了。それ以外は abort、終了 1。 (@since 0.5.0 or earlier)
- `effect process.pid() -> Int` — このプログラムの OS プロセス ID。 (@since 0.12.3 or earlier)
- `type process.ProcessStatus = { code: Int, stdout: String, stderr: String }` — 終了コードと取得した stdout/stderr。シグナル終了なら code -1。 (@since 0.15.0 or earlier)
- `type process.ProcessCommand = { cmd: String, args: List[String], dir: String, stdin: String, timeout_ms: Int }` — 子の設定。dir "" は現在のディレクトリを維持、stdin "" は入力なし、timeout_ms 0 は期限なし。 (@since unreleased)
