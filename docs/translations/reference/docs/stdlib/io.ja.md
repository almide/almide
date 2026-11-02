# io

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](io.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](io.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](io.zh-CN.md)



> 原文：[io.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/io.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

標準ストリームと、任意のハンドルで動作するバイト I/O。`import io`、`effect fn`。

`io` には2層あります。**テキスト用ショートカット** — `read_line`、`read_all`、`print`、`eprint` — は標準ストリームを `String` として読み書きします。**ハンドル関数** — `read`、`read_exact`、`read_to_end`、`write`、`close` — は `Read`、`Write`、`Close` である任意の値を通じて `Bytes` を移動します。標準ストリーム（`io.stdin()`、`io.stdout()`、`io.stderr()`）、`net.TcpStream`、`unix.Fd` が対象です。入力元によらずバイトを読む方法は1つで、個別の `tcp_read` や `unix_recv` を持つモジュールはありません。

| ハンドル | 生成元 | Read | Write | Close |
|---|---|---|---|---|
| `io.Stdin` | `io.stdin()` | 可 | | |
| `io.Stdout`, `io.Stderr` | `io.stdout()`, `io.stderr()` | | 可 | |
| `net.TcpStream` | `net.tcp_connect`, `net.tcp_accept` | 可 | 可 | 可 |
| `net.TcpListener`, `net.UdpSocket` | `net.tcp_listen`, `net.udp_bind` | | | 可 |
| `unix.Fd` | `unix.connect`, `unix.listen`, `unix.accept`, ... | 可 | 可 | 可 |

型が対応しない関数へハンドルを渡すと型エラーになるため、`io.close(io.stdout()!)` はコンパイルできません。ハンドルは `Int` から作れず、そのハンドルのモジュールだけが生成します。

**ストリーム関数の呼び出し。** `io.stdin()`、`io.stdout()`、`io.stderr()` は `effect fn` で、呼び出すたびに `Result` を返すため、`io.stdout()!` と書きます。ハンドル関数もすべて `Result` を返します（ADR-0008：伝播は明示的）ので、書き込みは `io.write(io.stdout()!, data)!` です。

**stdout は1つのバッファー**です（#2245）。`println`、`io.print`、`io.write(io.stdout()!, ...)` はすべて同じ64 KiBバッファー経由で書き、プログラム順に現れます。バッファーは行バッファリングです（#3417）。行を終える書き込みは、端末・パイプ・ファイルのいずれでもフラッシュするため、長時間動作するプログラム（監視プログラム、サーバー）は各行を出力時に表示します。stdout が端末の場合は毎回の書き込みでフラッシュします。終了時、パニック時、`main` がエラーを返すとき、子プロセス実行前、stdin の毎回の読み取り前にもフラッシュします。`io.print` は必ずフラッシュするので、`io.print("")` は明示的なフラッシュです。`eprintln`、`io.eprint`、`io.stderr()` への書き込みは、stdout をフラッシュ後、バッファーなしで stderr へ送ります。stdout が端末でない場合、2つのストリーム間の相対順序は保持されませんが、各ストリーム内のバイト順序は保持されます。wasm 側では各呼び出しを直接ホストへ書き、同じバイトを同じ順序で出力します（C-162）。

**ターゲット。** ネイティブと wasm は標準ストリームもハンドル関数も提供します。他のモジュールのハンドルは、そのモジュールの関数が動く環境で動作します。[net](../../../../stdlib/net.md) と [unix](../../../../stdlib/unix.md) を参照してください。

## テキスト用ショートカット

### `io.read_line() -> String?`

標準入力の次の行です。入力終了時は `none`。改行（および末尾の `\r`）を除去します。空行は `some("")` なので、ループで空行を飛ばしつつ、入力終了（Ctrl+D またはパイプの終端）で停止できます。

行のバイトは UTF-8 としてデコードされます。不正なシーケンスごとに1つの `U+FFFD`（`�`）へ置き換え、すべてのターゲットで同じです（#4142）。読み取りエラーはファイル終端と同様に入力を終了します。

```almd check
import io

effect fn main() -> Unit = {
  var open = true
  while open {
    match io.read_line() {
      none => {
        open = false
      },
      some("") => (),
      some(line) => println("> ${line}"),
    }
  }
}
```

プログラムが1行だけを必要とするなら、`??` で入力終了時の値を与えられます。

```almd check
import io

effect fn main() -> Unit = {
  let name = io.read_line() ?? "stranger"
  println("Hello, ${name}!")
}
```

### `io.read_all() -> String`

標準入力の残りすべてを1つの文字列にします。入力終了時は空です。バイトは UTF-8 としてデコードされ、不正なシーケンスごとに1つの `U+FFFD`（`�`）となり、すべてのターゲットで同じです（#4142）。読み取りエラーで入力を終了しますが、それまでに読んだバイトは保持します。デコード前のバイトには `io.read_to_end(io.stdin()!)` を使います。

```almd check
import io

effect fn main() -> Unit = {
  let input = io.read_all()
  println("${list.len(string.lines(input))} line(s)")
}
```

### `io.print(s: String) -> Unit`

末尾の改行なしで `s` を stdout に書き、その後フラッシュします。

```almd run
import io

effect fn main() -> Unit = {
  io.print("Enter name: ")
  println("Alice")
}
```
```output
Enter name: Alice
```

### `io.eprint(s: String) -> Unit`

末尾の改行なしで `s` を stderr に書きます。改行しない `eprintln` であり、stderr のプロンプトや進捗行用です。

```almd check
import io

effect fn main() -> Unit = {
  io.eprint("working... ")
  eprintln("done")
}
```

## ストリームのハンドル

### `io.stdin() -> Stdin` / `io.stdout() -> Stdout` / `io.stderr() -> Stderr`

以下の関数で使う、標準ストリームのハンドルです。呼び出しごとに `!` が必要です。テキスト用ショートカットと同じストリームを共有します。`io.read_line` と `io.read(io.stdin()!, n)` は同じ入力、`println` と `io.write(io.stdout()!, b)` は同じバッファーを使います。

## 任意のハンドルで扱うバイト

### `io.read(h, max: Int) -> Result[Bytes, String]`

`h` から最大 `max` バイトを読み、一部が届くまでブロックします。短い読み取りは正常です。結果は届いた分で、`max` バイト未満になります。`h` が終了すると（入力終了または相手の切断）空になります。負の `max` は `err` です。

```almd check
import io

effect fn main() -> Unit = {
  let head = io.read(io.stdin()!, 4)!
  println("${bytes.len(head)} byte(s)")
}
```

### `io.read_exact(h, n: Int) -> Result[Bytes, String]`

必要な回数だけ読み、`h` から正確に `n` バイト取得します。先に `h` が終了すると、届いたバイト数を示す `err` になります（`io.read_exact: the stream ended after 3 of 8 bytes`）。長さを先頭に置くプロトコルや固定サイズのヘッダーに適した呼び出しです。

```almd check
import io

effect fn main() -> Unit = {
  let header = io.read_exact(io.stdin()!, 8)!
  println("header: ${bytes.len(header)} bytes")
}
```

### `io.read_to_end(h) -> Result[Bytes, String]`

`h` が終了するまでのすべてのバイトです。

```almd check
import io

effect fn main() -> Unit = {
  let input = io.read_to_end(io.stdin()!)!
  println("${bytes.len(input)} bytes in")
}
```

### `io.write(h, data: Bytes) -> Result[Unit, String]`

`data` のすべてを `h` に書き込みます。stdout ではバイトをそのまま共有バッファーに入れます。UTF-8 変換も改行の追加もしません。

```almd run
import io

effect fn main() -> Unit = {
  io.write(io.stdout()!, bytes.from_string("Hi\n"))!
}
```
```output
Hi
```

### `io.close(h) -> Result[Unit, String]`

`h` を閉じます。2回閉じても問題ありません。リソースを所有するハンドルだけが `Close` です。ソケットや記述子が該当し、標準ストリームは該当しません。

```almd check
import io
import net

effect fn main() -> Unit = {
  let s = net.tcp_connect("example.com", 80)!
  io.write(s, bytes.from_string("HEAD / HTTP/1.0\r\n\r\n"))!
  let reply = io.read(s, 4096)!
  io.close(s)!
  println("${bytes.len(reply)} bytes")
}
```



## 固定スナップショットの API 索引（12関数）

```
// Next stdin line, newline cut, bad UTF-8 as U+FFFD; none at EOF, some("") for an empty line.
// @since 0.5.0 or earlier
effect io.read_line() -> Option[String]

// Rest of stdin as text, bad UTF-8 as U+FFFD; empty at EOF.
// @since 0.5.0 or earlier
effect io.read_all() -> String

// Writes s to stdout, no newline, then flushes.
// @since 0.5.0 or earlier
effect io.print(s: String) -> Unit

// Writes s to stderr, no newline.
// @since unreleased
effect io.eprint(s: String) -> Unit

// Standard input, for io.read / read_exact / read_to_end.
// @since unreleased
effect io.stdin() -> Stdin

// Standard output, for io.write.
// @since unreleased
effect io.stdout() -> Stdout

// Standard error, for io.write.
// @since unreleased
effect io.stderr() -> Stderr

// Up to max bytes from h; empty once h has ended; err if max < 0.
// @since unreleased
effect io.read(h: H, max: Int) -> Bytes

// Exactly n bytes from h; err if h ends first, naming how many arrived.
// @since unreleased
effect io.read_exact(h: H, n: Int) -> Bytes

// Every byte until h ends.
// @since unreleased
effect io.read_to_end(h: H) -> Bytes

// Writes all of data to h.
// @since 0.9.8 or earlier
effect io.write(h: H, data: Bytes) -> Unit

// Closes h; closing twice is ok.
// @since unreleased
effect io.close(h: H) -> Unit
```




## API 説明の訳（上記署名順）

- `effect io.read_line() -> Option[String]` — stdin の次の行。改行を除去、不正UTF-8は U+FFFD。EOFは none、空行は some("")。
- `effect io.read_all() -> String` — stdin の残りをテキストとして取得。不正UTF-8は U+FFFD、EOFは空。
- `effect io.print(s: String) -> Unit` — s を stdout に改行なしで書き、その後フラッシュ。
- `effect io.eprint(s: String) -> Unit` — s を stderr に改行なしで書く。
- `effect io.stdin() -> Stdin` — 標準入力。io.read / read_exact / read_to_end 用。
- `effect io.stdout() -> Stdout` — 標準出力。io.write 用。
- `effect io.stderr() -> Stderr` — 標準エラー。io.write 用。
- `effect io.read(h: H, max: Int) -> Bytes` — h から最大 max バイト。h 終了後は空。max < 0 なら err。
- `effect io.read_exact(h: H, n: Int) -> Bytes` — h から正確に n バイト。先に h が終了したら届いた数を示す err。
- `effect io.read_to_end(h: H) -> Bytes` — h 終了まで全バイト。
- `effect io.write(h: H, data: Bytes) -> Unit` — data 全体を h に書く。
- `effect io.close(h: H) -> Unit` — h を閉じる。2回閉じても成功。
