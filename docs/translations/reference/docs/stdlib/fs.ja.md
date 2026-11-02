# fs

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](fs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](fs.zh-CN.md)



> 原文：[docs/stdlib/fs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/fs.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）。非規範的な読者向けスナップショット訳。自動同期されず、独立した実測・実装検証は行っていない。API シグネチャとコード内コメントは原文を保持し、説明訳をコード外へ添える。生成索引の固定コピーであり、この訳は生成対象ではない。 原文の fold_lines_range 本文は行先頭直前のバイトで所属を決めるが、索引は範囲内で始まる行と略記する。原文の区別を保持する。リンクを辿らないという説明は walk/glob の走査対象についてで、指定した dir 自身のリンクは辿る。2026-10-25 の他言語比較は原文の測定記録であり、この訳で再測定していない。

ファイルシステム。import fs、effect。

### `_if_exists` 内容リーダー — 不在を値として扱う

`read_text_if_exists` / `read_bytes_if_exists` / `read_lines_if_exists` は `Result[Option[T], String]` を返す。パス（または親）がなければ `ok(none)`、読めれば `ok(some(content))`、実際の失敗（権限、パスがディレクトリ、IO）なら `err(msg)`。分類は 1 回の読取りの内側で行うため、`fs.exists` の事前チェックと異なり競合を生まない。契約 C-215。ネイティブと wasm の両経路に対応し、ターゲットでの利用可否はルート一覧に記載される。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/config.toml"
  let cfg = fs.read_text_if_exists(path)! ?? "default"
  println(cfg)
  fs.write_text(path, "port = 8080")!
  println(fs.read_text_if_exists(path)! ?? "default")
  fs.remove_all(dir)!
}
```
```output
default
port = 8080
```

### ファイル全体のリーダーは最大 1 GiB の通常ファイルを読む

`read_text` / `read_bytes` / `read_lines` と、それぞれの `_if_exists` 版は、最大 **1 GiB**（1073741824 バイト）の**通常ファイル**を読む。ネイティブ、組込み wasm ホスト、標準 WASI 経路で同じ（契約 C-431、#4201）。

- キャラクタ／ブロックデバイス、FIFO、ソケット（`/dev/zero`、`/dev/null`、名前付きパイプ）は `fs.read_text("/dev/zero"): Not a regular file` というエラーになる。書き手のない FIFO を open してもブロックせず、開いた記述子自体を判定するため、チェック後に差し替えたファイルがすり抜けることはない。
- 大きすぎるファイルは `fs.read_bytes("big.bin"): File too large (the read
  limit is 1073741824 bytes)` となる。1 バイトも読む前にサイズで判定し、サイズが過小申告の場合は実際に読んだバイト数でも判定する。
- ディレクトリは固有の `Is a directory (os error 21)` を維持する。

`_if_exists` 版もこれらを `err` とし、`ok(none)` にはしない。上限より大きいファイルの走査には `fs.fold_lines` / `fs.for_each_line` によるストリーミングを使う。標準入力を読むのは `io.read_all` であり、`fs.read_text("/dev/stdin")` ではない。

### `fs.read_text(path: String) -> Result[String, String]`

ファイル内容を UTF-8 文字列として読む。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/config.toml", "port = 8080")!
  let text = fs.read_text("${dir}/config.toml")!
  println(text)
  fs.remove_all(dir)!
}
```
```output
port = 8080
```

### `fs.read_bytes(path: String) -> Result[Bytes, String]`

ファイル内容を `Bytes` として読む。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_bytes("${dir}/image.png", bytes.from_list([137, 80, 78, 71]))!
  let data = fs.read_bytes("${dir}/image.png")!
  println("${bytes.to_list(data)}")
  fs.remove_all(dir)!
}
```
```output
[137, 80, 78, 71]
```

### `fs.write_text(path: String, content: String) -> Result[Unit, String]`

文字列をファイルへ書き、作成または上書きする。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/output.txt", "hello")!
  println(fs.read_text("${dir}/output.txt")!)
  fs.remove_all(dir)!
}
```
```output
hello
```

### `fs.write_bytes(path: String, data: Bytes) -> Result[Unit, String]`

`Bytes` をファイルへ書き、作成または上書きする。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_bytes("${dir}/out.bin", bytes.from_list([0, 1, 2]))!
  println("${bytes.to_list(fs.read_bytes("${dir}/out.bin")!)}")
  fs.remove_all(dir)!
}
```
```output
[0, 1, 2]
```

### `fs.append_text(path: String, content: String) -> Result[Unit, String]`

文字列をファイルへ追記し、不在なら作成する。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let log = "${dir}/log.txt"
  fs.append_text(log, "first line\n")!
  fs.append_text(log, "new line\n")!
  println("${fs.read_lines(log)!}")
  fs.remove_all(dir)!
}
```
```output
["first line", "new line"]
```

### `fs.append_bytes(path: String, data: Bytes) -> Result[Unit, String]`

`Bytes` をファイルへ追記し、不在なら作成する。全ターゲットで真の追記である。ファイルは追記用に開き、読み取らないので、ファイルサイズによらず、呼出しのコストは追記するバイト数分になる（C-447）。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let out = "${dir}/out.bin"
  fs.append_bytes(out, bytes.from_list([1, 2]))!
  fs.append_bytes(out, bytes.from_list([3]))!
  println("${bytes.to_list(fs.read_bytes(out)!)}")
  fs.remove_all(dir)!
}
```
```output
[1, 2, 3]
```

### `fs.mkdir_p(path: String) -> Result[Unit, String]`

ディレクトリとすべての親ディレクトリを作成する。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/data/cache/images")!
  println("${fs.is_dir("${dir}/data/cache/images")}")
  fs.remove_all(dir)!
}
```
```output
true
```

### `fs.exists(path: String) -> Bool`

ファイルまたはディレクトリが存在するか調べる。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/config.toml", "")!
  println(if fs.exists("${dir}/config.toml") then "found" else "missing")
  println(if fs.exists("${dir}/other.toml") then "found" else "missing")
  fs.remove_all(dir)!
}
```
```output
found
missing
```

### `fs.read_lines(path: String) -> Result[List[String], String]`

ファイルを行のリストとして読む。ファイル全体を実体化するため、大きな入力では代わりに `fs.fold_lines` / `fs.for_each_line` を使う（リーダーのバッファは O(最長行)、O(ファイル) ではない。アキュムレータとコールバックの割当ては除く）。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/data.csv", "a,1\nb,2\n")!
  let lines = fs.read_lines("${dir}/data.csv")!
  println("${lines}")
  fs.remove_all(dir)!
}
```
```output
["a,1", "b,2"]
```

### `fs.fold_lines(path: String, init: A, f: (A, String) -> A) -> Result[A, String]`

行を実体化せずファイルの各行を fold する。行の意味論は `read_lines` とバイト単位で一致する（契約 C-220）。大きなファイルの集約に使う基本形。

```almd run
import fs

fn parse_row(line: String) -> Int = int.parse(line) ?? 0

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/data.csv", "1\n2\n3\n")!
  let total = fs.fold_lines("${dir}/data.csv", 0, (acc, line) => acc + parse_row(line))!
  println(int.to_string(total))
  fs.remove_all(dir)!
}
```
```output
6
```
**失敗しうるコールバック**を使うと走査全体も失敗しうる（ADR-0006、契約 C-274）。本体が `!` で伝播するコールバックは、最初の err で短絡する形式を選ぶ。失敗した行より後の行ではコールバックを呼ばず、ネイティブのリーダーもそこで停止する。同じ名前に `!` を 1 つ足すだけ。

```almd run
import fs

fn add_row(acc: Int, line: String) -> Int! = {
  guard line != "oops" else err("bad row: ${line}")
  acc + (int.parse(line) ?? 0)
}

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/data.csv"
  fs.write_text(path, "1\n2\n3\n")!
  let stats = fs.fold_lines(path, 0, (acc, line) => add_row(acc, line)!)!
  println(int.to_string(stats))
  // first err wins; later lines are never visited
  fs.write_text(path, "1\noops\n3\n")!
  let failed = fs.fold_lines(path, 0, (acc, line) => add_row(acc, line)!)
  match failed {
    ok(n) => println(int.to_string(n)),
    err(e) => println("error: ${e}"),
  }
  fs.remove_all(dir)!
}
```
```output
6
error: bad row: oops
```

### `fs.for_each_line(path: String, f: (String) -> Unit) -> Result[Unit, String]`

リストを実体化せず、ファイルの各行を順に訪問する。コールバックは捕捉した `var` を変更できる。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/data.csv", "a,1\nb,2\nc,3\n")!
  var count = 0
  fs.for_each_line("${dir}/data.csv", (line) => { count = count + 1 })!
  println(int.to_string(count))
  fs.remove_all(dir)!
}
```
```output
3
```
`fold_lines` と同じ失敗しうるコールバック形式も取る。

```almd check
import fs

effect fn emit(line: String) -> Unit = {
  guard line != "" else err("empty line")
  println("got ${line}")
}

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/data.csv"
  fs.write_text(path, "one\ntwo\n")!
  fs.for_each_line(path, (line) => emit(line)!)!
  fs.remove_all(dir)!
}
```

以下の 2 つの**分割**セル（`fold_lines_range` / `fold_lines_chunked`）には、意図的に失敗しうる形式が**ない**。分割走査では「最初」の err が定義できない（どのチャンクが先に失敗するかはスレッドスケジュールによって観測結果が変わる）ため、失敗するチャンク本体は自分でエラーを処理する。

### `fs.fold_lines_range(path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> Result[A, String]`

バイト範囲 `[start, end)` に属する行だけを fold する。行は先頭バイトの直前のバイトを含む範囲に属する（最初の行はバイト 0 を含む範囲）。そのため `[0, size)`（サイズは `fs.stat` から得る）を分割して fold すると、すべての行を厳密に 1 回ずつ訪問する。

```almd check
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  let path = "${dir}/data.csv"
  fs.write_text(path, "a\nbb\nccc\n")!
  let part = fs.fold_lines_range(path, 0, 4096, 0, (acc, line) => acc + 1)!
  println(int.to_string(part))
  // a partition of [0, file_size) visits every line exactly once
  let size = fs.stat(path)!.size
  let first = fs.fold_lines_range(path, 0, 4, 0, (acc, line) => acc + 1)!
  let second = fs.fold_lines_range(path, 4, size, 0, (acc, line) => acc + 1)!
  println("${first} + ${second}")
  fs.remove_all(dir)!
}
```

### `fs.fold_lines_chunked(path: String, workers: Int, init: A, f: (A, String) -> A) -> Result[List[A], String]`

チャンクごとに fold し、部分結果をチャンク順で返す。条件を満たすネイティブ呼出しは利用可能コア数を上限とするランタイムのスレッドプールを使う。送信不能なアキュムレータやクロージャを伴う呼出しは等価な逐次経路を使う。プール枯渇時はその場で処理する場合もある。部分結果は自分でマージする。結果はスレッドスケジュールによらず決定的である。コールバックは純粋なステップ関数でなければならない（変更可能な状態の捕捉はコンパイルエラー）。

```almd check
import fs

fn step(acc: Int, line: String) -> Int = acc + 1
fn combine(a: Int, b: Int) -> Int = a + b

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/data.csv", "a\nb\nc\nd\ne\n")!
  let partials = fs.fold_lines_chunked("${dir}/data.csv", 8, 0, step)!
  let stats = partials |> list.fold(0, (acc, m) => combine(acc, m))
  println(int.to_string(stats))
  fs.remove_all(dir)!
}
```

### `fs.remove(path: String) -> Result[Unit, String]`

ファイルを削除する。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/temp.txt", "scratch")!
  fs.remove("${dir}/temp.txt")!
  println("${fs.exists("${dir}/temp.txt")}")
  fs.remove_all(dir)!
}
```
```output
false
```

### `fs.list_dir(path: String) -> Result[List[String], String]`

ディレクトリのエントリを列挙する。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src")!
  fs.write_text("${dir}/src/main.almd", "")!
  fs.write_text("${dir}/src/util.almd", "")!
  let entries = fs.list_dir("${dir}/src/")!
  println("${entries |> list.sort}")
  fs.remove_all(dir)!
}
```
```output
["main.almd", "util.almd"]
```

### `fs.is_dir(path: String) -> Bool`

パスがディレクトリか調べる。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src")!
  fs.write_text("${dir}/readme.md", "")!
  println(if fs.is_dir("${dir}/src") then "directory" else "not a directory")
  println(if fs.is_dir("${dir}/readme.md") then "directory" else "not a directory")
  fs.remove_all(dir)!
}
```
```output
directory
not a directory
```

### `fs.is_file(path: String) -> Bool`

パスが通常ファイルか調べる。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src")!
  fs.write_text("${dir}/readme.md", "")!
  println(if fs.is_file("${dir}/readme.md") then "file" else "not a file")
  println(if fs.is_file("${dir}/src") then "file" else "not a file")
  fs.remove_all(dir)!
}
```
```output
file
not a file
```

### `fs.copy(src: String, dst: String) -> Result[Unit, String]`

src から dst へファイルをコピーする。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/a.txt", "hello")!
  fs.copy("${dir}/a.txt", "${dir}/b.txt")!
  println(fs.read_text("${dir}/b.txt")!)
  fs.remove_all(dir)!
}
```
```output
hello
```

### `fs.rename(src: String, dst: String) -> Result[Unit, String]`

ファイルを改名または移動する。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/old.txt", "hello")!
  fs.rename("${dir}/old.txt", "${dir}/new.txt")!
  println("${fs.exists("${dir}/old.txt")} ${fs.exists("${dir}/new.txt")}")
  fs.remove_all(dir)!
}
```
```output
false true
```

### `fs.walk(dir: String) -> Result[List[String], String]`

ディレクトリツリーの全エントリ、ファイルとディレクトリの両方を再帰的に列挙する。

シンボリックリンクはエントリとして列挙するだけで、辿らない（C-396）。ツリー外のディレクトリへのリンクがあっても外側を列挙せず、リンクのループで走査が繰り返されることもない。`fs.glob` も同じ走査を行う。`dir` 自身は指定通りに開き、そこにあるシンボリックリンクは辿る。

現時点でリンク追跡は対象外であり、有効化する手段はない。他者が書き込めるディレクトリ（アップロード、スプール、展開済みアーカイブ、クローンしたリポジトリ）には、追跡しないことが安全なデフォルトである。測定したウォーカー（2026-10-25）のうち、Go の `filepath.WalkDir` にも追跡オプションはなく、Python の `os.walk(followlinks=True)` と Rust walkdir の `follow_links(true)` にはある。リンク先ディレクトリに入るには、自分でそのパスを `fs.walk` に指定する。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/src/util")!
  fs.write_text("${dir}/src/main.almd", "")!
  fs.write_text("${dir}/src/util/io.almd", "")!
  let all_files = fs.walk("${dir}/src")!
  println("${all_files |> list.map((f) => string.replace(f, dir, "")) |> list.sort}")
  fs.remove_all(dir)!
}
```
```output
["/src/main.almd", "/src/util", "/src/util/io.almd"]
```

### `fs.remove_all(path: String) -> Result[Unit, String]`

ディレクトリと全内容を再帰的に削除する。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.mkdir_p("${dir}/build/obj")!
  fs.write_text("${dir}/build/obj/main.o", "")!
  fs.remove_all("${dir}/build/")!
  println("${fs.exists("${dir}/build")}")
  fs.remove_all(dir)!
}
```
```output
false
```

### `fs.temp_dir() -> String`

システムの一時ディレクトリパスを得る。

```almd run
import fs

effect fn main() -> Unit = {
  let tmp = fs.temp_dir()
  println("${fs.is_dir(tmp)}")
}
```
```output
true
```

### `fs.stat(path: String) -> Result[{size: Int, is_dir: Bool, is_file: Bool, modified: Int}, String]`

ファイルのサイズ、種類、更新時刻のメタデータを得る。

```almd check
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/file.txt", "hello")!
  let info = fs.stat("${dir}/file.txt")! // {size, is_dir, is_file, modified}
  println("${info.size} ${info.is_dir} ${info.is_file} ${info.modified > 0}")
  fs.remove_all(dir)!
}
```

### `fs.glob(pattern: String) -> Result[List[String], String]`

glob パターンに一致するパスを探す。パターンのリテラルな接頭パス以下（相対パターンなら現在のディレクトリ）のパスに、セグメントごとに照合する。`*` は 1 セグメント内の任意の文字列、`**` は 0 個以上の完全なセグメント、それ以外はリテラル。ファイルとディレクトリの両方が対象。結果は整列され、パターン自身の接頭パスで表記される。相対パターンなら cwd 相対で先頭の `./` は付かない。接頭ディレクトリが存在しなければ `[]`。

```almd check
import fs

effect fn main() -> Unit = {
  // From the repo root: ["src/deep/util.almd", "src/main.almd"] — every
  // .almd at any depth under src/, sorted; "src/*.almd" would stop at depth 1
  // and "*.almd" lists only the current directory.
  let files = fs.glob("src/**/*.almd")!
  for f in files {
    println(f)
  }
}
```

### `fs.create_temp_file(prefix: String) -> Result[String, String]`

指定接頭辞の一時ファイルを作成し、そのパスを返す。

```almd run
import fs

effect fn main() -> Unit = {
  let path = fs.create_temp_file("almide-")!
  println("${fs.is_file(path)} ${string.contains(path, "almide-")}")
  fs.remove(path)!
}
```
```output
true true
```

### `fs.create_temp_dir(prefix: String) -> Result[String, String]`

指定接頭辞の一時ディレクトリを作成し、そのパスを返す。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("build-")!
  println("${fs.is_dir(dir)} ${string.contains(dir, "build-")}")
  fs.remove_all(dir)!
}
```
```output
true true
```

### `fs.is_symlink(path: String) -> Bool`

パスがシンボリックリンクか調べる。

```almd run
import fs

effect fn main() -> Unit = {
  let dir = fs.create_temp_dir("fs-doc-")!
  fs.write_text("${dir}/link", "a regular file, not a symlink")!
  println(if fs.is_symlink("${dir}/link") then "symlink" else "not a symlink")
  fs.remove_all(dir)!
}
```
```output
not a symlink
```



## シグネチャ索引（37 関数）

```
// Whole regular file (<= 1 GiB) as UTF-8; err if missing or not UTF-8.
// @since 0.5.0 or earlier
effect fs.read_text(path: String) -> String

// Whole regular file (<= 1 GiB) as Bytes; err if missing.
// @since 0.5.0 or earlier
effect fs.read_bytes(path: String) -> Bytes

// Overwrites or creates; err if no parent dir.
// @since unreleased
effect fs.write_text(path: String, content: String) -> Unit

// Writes data verbatim, replacing the file.
// @since 0.5.0 or earlier
effect fs.write_bytes(path: String, data: Bytes) -> Unit

// Appends content; creates the file if missing.
// @since unreleased
effect fs.append_text(path: String, content: String) -> Unit

// Appends data verbatim; creates the file if missing.
// @since unreleased
effect fs.append_bytes(path: String, data: Bytes) -> Unit

// Creates dir and parents; ok if it exists.
// @since 0.5.0 or earlier
effect fs.mkdir_p(path: String) -> Unit

// False if missing or a dangling symlink.
// @since 0.6.0 or earlier
effect fs.exists(path: String) -> Bool

// CRLF-aware lines of a regular file <= 1 GiB; a final newline adds no line.
// @since 0.5.0 or earlier
effect fs.read_lines(path: String) -> List[String]

// Streamed fold over lines; err if missing.
// @since 0.57.0
effect fs.fold_lines(path: String, init: A, f: (A, String) -> A) -> A

// Calls f per line, streamed; may err mid-file.
// @since 0.57.0
effect fs.for_each_line(path: String, f: (String) -> Unit) -> Unit

// Folds lines starting in bytes [start, end).
// @since 0.57.0
effect fs.fold_lines_range(path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A

// Parallel chunk partials, in order; [] if empty.
// @since 0.57.0
effect fs.fold_lines_chunked(path: String, workers: Int, init: A, f: (A, String) -> A) -> List[A]

// Text, or none if missing; err if not UTF-8.
// @since 0.54.0
effect fs.read_text_if_exists(path: String) -> Option[String]

// File Bytes, or none if missing.
// @since 0.54.0
effect fs.read_bytes_if_exists(path: String) -> Option[Bytes]

// read_lines result, or none if missing.
// @since 0.54.0
effect fs.read_lines_if_exists(path: String) -> Option[List[String]]

// Deletes a file or empty dir; err if missing.
// @since 0.5.0 or earlier
effect fs.remove(path: String) -> Unit

// Sorted names (not paths); err if not a dir.
// @since 0.5.0 or earlier
effect fs.list_dir(path: String) -> List[String]

// True for a dir; follows symlinks.
// @since 0.6.0 or earlier
effect fs.is_dir(path: String) -> Bool

// True for a regular file; follows symlinks.
// @since 0.6.0 or earlier
effect fs.is_file(path: String) -> Bool

// Copies src to dst; overwrites dst.
// @since 0.5.0 or earlier
effect fs.copy(src: String, dst: String) -> Unit

// Moves src to dst; replaces an existing dst.
// @since 0.5.0 or earlier
effect fs.rename(src: String, dst: String) -> Unit

// Sorted paths below dir; links listed, not followed.
// @since 0.5.0 or earlier
effect fs.walk(dir: String) -> List[String]

// Deletes a tree or file; err if missing.
// @since 0.5.0 or earlier
effect fs.remove_all(path: String) -> Unit

// Host temp dir; a location, not a safe name.
// @since 0.5.0 or earlier
effect fs.temp_dir() -> String

// Size, kind and mtime; err if missing.
// @since 0.5.0 or earlier
effect fs.stat(path: String) -> FileStat

// Sorted * and ** matches; links not followed.
// @since 0.6.0 or earlier
effect fs.glob(pattern: String) -> List[String]

// New private file, random name; its path.
// @since 0.6.0 or earlier
effect fs.create_temp_file(prefix: String) -> String

// New private dir, random name; its path.
// @since 0.6.0 or earlier
effect fs.create_temp_dir(prefix: String) -> String

// True for a symlink, even a dangling one.
// @since 0.6.0 or earlier
effect fs.is_symlink(path: String) -> Bool

// dir as an FsRoot; err if dir is not a directory.
// @since unreleased
effect fs.open_root(dir: String) -> FsRoot

// read_text of path inside root; err if it escapes.
// @since unreleased
effect fs.root_read_text(root: FsRoot, path: String) -> String

// read_bytes of path inside root; err if it escapes.
// @since unreleased
effect fs.root_read_bytes(root: FsRoot, path: String) -> Bytes

// write of content to path inside root; err if it escapes.
// @since unreleased
effect fs.root_write_text(root: FsRoot, path: String, content: String) -> Unit

// list_dir of path inside root; err if it escapes.
// @since unreleased
effect fs.root_list_dir(root: FsRoot, path: String) -> List[String]

// exists of path inside root; err if it escapes.
// @since unreleased
effect fs.root_exists(root: FsRoot, path: String) -> Bool

// remove of path inside root; err if it escapes.
// @since unreleased
effect fs.root_remove(root: FsRoot, path: String) -> Unit
```

## 型索引（1 型）

```
// Path metadata; modified is Unix seconds.
// @since 0.15.0 or earlier
type fs.FileStat = { size: Int, is_dir: Bool, is_file: Bool, modified: Int }
```



## 索引コメントの説明訳

- `effect fs.read_text(path: String) -> String` — 通常ファイル全体（<= 1 GiB）を UTF-8 として読む。不在または非 UTF-8 なら err。 (@since 0.5.0 or earlier)
- `effect fs.read_bytes(path: String) -> Bytes` — 通常ファイル全体（<= 1 GiB）を Bytes として読む。不在なら err。 (@since 0.5.0 or earlier)
- `effect fs.write_text(path: String, content: String) -> Unit` — 上書きまたは作成。親ディレクトリがなければ err。 (@since unreleased)
- `effect fs.write_bytes(path: String, data: Bytes) -> Unit` — data をそのまま書き、ファイルを置き換える。 (@since 0.5.0 or earlier)
- `effect fs.append_text(path: String, content: String) -> Unit` — content を追記。不在なら作成。 (@since unreleased)
- `effect fs.append_bytes(path: String, data: Bytes) -> Unit` — data をそのまま追記。不在なら作成。 (@since unreleased)
- `effect fs.mkdir_p(path: String) -> Unit` — ディレクトリと親を作成。既存なら ok。 (@since 0.5.0 or earlier)
- `effect fs.exists(path: String) -> Bool` — 不在または宛先のないシンボリックリンクなら false。 (@since 0.6.0 or earlier)
- `effect fs.read_lines(path: String) -> List[String]` — <= 1 GiB の通常ファイルの CRLF 対応行。末尾改行は行を増やさない。 (@since 0.5.0 or earlier)
- `effect fs.fold_lines(path: String, init: A, f: (A, String) -> A) -> A` — 行をストリーム処理して fold。不在なら err。 (@since 0.57.0)
- `effect fs.for_each_line(path: String, f: (String) -> Unit) -> Unit` — ストリーム処理で各行に f を呼ぶ。ファイル途中で err になる場合あり。 (@since 0.57.0)
- `effect fs.fold_lines_range(path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A` — バイト [start, end) で始まる行を fold。 (@since 0.57.0)
- `effect fs.fold_lines_chunked(path: String, workers: Int, init: A, f: (A, String) -> A) -> List[A]` — 並列チャンクの部分結果を順序通りに返す。空なら []。 (@since 0.57.0)
- `effect fs.read_text_if_exists(path: String) -> Option[String]` — テキスト、不在なら none。非 UTF-8 なら err。 (@since 0.54.0)
- `effect fs.read_bytes_if_exists(path: String) -> Option[Bytes]` — ファイルの Bytes、不在なら none。 (@since 0.54.0)
- `effect fs.read_lines_if_exists(path: String) -> Option[List[String]]` — read_lines の結果、不在なら none。 (@since 0.54.0)
- `effect fs.remove(path: String) -> Unit` — ファイルまたは空ディレクトリを削除。不在なら err。 (@since 0.5.0 or earlier)
- `effect fs.list_dir(path: String) -> List[String]` — 整列済みの名前（パスではない）。ディレクトリ以外は err。 (@since 0.5.0 or earlier)
- `effect fs.is_dir(path: String) -> Bool` — ディレクトリなら true。シンボリックリンクを辿る。 (@since 0.6.0 or earlier)
- `effect fs.is_file(path: String) -> Bool` — 通常ファイルなら true。シンボリックリンクを辿る。 (@since 0.6.0 or earlier)
- `effect fs.copy(src: String, dst: String) -> Unit` — src を dst へコピー。dst は上書き。 (@since 0.5.0 or earlier)
- `effect fs.rename(src: String, dst: String) -> Unit` — src を dst へ移動。既存 dst を置換。 (@since 0.5.0 or earlier)
- `effect fs.walk(dir: String) -> List[String]` — dir 配下の整列済みパス。リンクは列挙するが辿らない。 (@since 0.5.0 or earlier)
- `effect fs.remove_all(path: String) -> Unit` — ツリーまたはファイルを削除。不在なら err。 (@since 0.5.0 or earlier)
- `effect fs.temp_dir() -> String` — ホストの一時ディレクトリ。場所であり安全な名前ではない。 (@since 0.5.0 or earlier)
- `effect fs.stat(path: String) -> FileStat` — サイズ・種類・mtime。不在なら err。 (@since 0.5.0 or earlier)
- `effect fs.glob(pattern: String) -> List[String]` — 整列済みの * と ** マッチ。リンクは辿らない。 (@since 0.6.0 or earlier)
- `effect fs.create_temp_file(prefix: String) -> String` — ランダム名の新しい非公開ファイル。そのパス。 (@since 0.6.0 or earlier)
- `effect fs.create_temp_dir(prefix: String) -> String` — ランダム名の新しい非公開ディレクトリ。そのパス。 (@since 0.6.0 or earlier)
- `effect fs.is_symlink(path: String) -> Bool` — 宛先がなくてもシンボリックリンクなら true。 (@since 0.6.0 or earlier)
- `effect fs.open_root(dir: String) -> FsRoot` — dir を FsRoot として返す。ディレクトリ以外なら err。 (@since unreleased)
- `effect fs.root_read_text(root: FsRoot, path: String) -> String` — root 内の path を read_text。範囲外へ出るなら err。 (@since unreleased)
- `effect fs.root_read_bytes(root: FsRoot, path: String) -> Bytes` — root 内の path を read_bytes。範囲外へ出るなら err。 (@since unreleased)
- `effect fs.root_write_text(root: FsRoot, path: String, content: String) -> Unit` — root 内の path に content を書く。範囲外へ出るなら err。 (@since unreleased)
- `effect fs.root_list_dir(root: FsRoot, path: String) -> List[String]` — root 内の path を list_dir。範囲外へ出るなら err。 (@since unreleased)
- `effect fs.root_exists(root: FsRoot, path: String) -> Bool` — root 内の path を exists。範囲外へ出るなら err。 (@since unreleased)
- `effect fs.root_remove(root: FsRoot, path: String) -> Unit` — root 内の path を remove。範囲外へ出るなら err。 (@since unreleased)
- `type fs.FileStat = { size: Int, is_dir: Bool, is_file: Bool, modified: Int }` — パスのメタデータ。modified は Unix 秒。 (@since 0.15.0 or earlier)
