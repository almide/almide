<!-- description: Stdlib redesign L2/L3 — fs: Bytes is the only byte container (read_bytes/write_bytes keep their names and switch to Bytes, the _raw duplicates retire), stat owns size and mtime, and the _if_exists and streaming families keep their gates -->
# L2/L3: fs

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-25: 一時ディレクトリは fs の持ち分）
> 出典: Rust https://doc.rust-lang.org/std/fs/ ・ Go https://pkg.go.dev/os ・ Python https://docs.python.org/3/library/pathlib.html ・ TS/JS https://nodejs.org/api/fs.html ・ Gleam https://gleam-stdlib.hexdocs.pm/ （fs は stdlib に無い）・ Elixir https://hexdocs.pm/elixir/File.html ・ OCaml https://ocaml.org/manual/latest/api/In_channel.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io/java.io.-file/ ・ Swift https://developer.apple.com/documentation/foundation/filemanager

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 全文をテキストで読む | `read_text` | `read_to_string` | `os.ReadFile`+`string` | `read_text` | `readFile(p, "utf8")` | — | `File.read` | `In_channel.input_all` | `readText` | `String(contentsOfFile:)` |
| 全部をバイトで読む | `read_bytes`（List[Int]）・`read_bytes_raw`（Bytes）→ `read_bytes`（Bytes）に一本化 | `fs::read` | `os.ReadFile` | `read_bytes` | `readFile(p)` | — | `File.read` | `In_channel.input_all` | `readBytes` | `Data(contentsOf:)` |
| 行のリストで読む | `read_lines` | `read_to_string`+`lines` | `bufio.Scanner` | `read_text().splitlines()` | — | — | `File.stream!` | `In_channel.input_lines` | `readLines` | — |
| 無ければ none で読む | `read_*_if_exists` | — | — | — | — | — | — | — | — | — |
| 行を流して畳む | `fold_lines`・`for_each_line`・`fold_lines_range`・`fold_lines_chunked` | `BufRead::lines` | `Scanner` | `for line in f` | `readline` | — | `File.stream!` | `In_channel.fold_lines` | `forEachLine`・`useLines` | — |
| 書く（上書き） | `write`・`write_bytes`（List[Int]）・`write_bytes_raw` → `write`・`write_bytes`（Bytes） | `fs::write` | `os.WriteFile` | `write_text`・`write_bytes` | `writeFile` | — | `File.write` | `Out_channel.with_open_text` | `writeText`・`writeBytes` | `write(to:)` |
| 追記 | `append` | `OpenOptions::append` | `OpenFile(O_APPEND)` | `open("a")` | `appendFile` | — | `File.write(…, [:append])` | `open_gen [Open_append]` | `appendText` | `FileHandle.seekToEnd` |
| ディレクトリを作る | `mkdir_p`（親ごと） | `create_dir`・`create_dir_all` | `Mkdir`・`MkdirAll` | `mkdir(parents=)` | `mkdir({recursive})` | — | `mkdir`・`mkdir_p` | `Sys.mkdir` | `mkdir`・`mkdirs` | `createDirectory` |
| 消す | `remove`・`remove_all` | `remove_file`・`remove_dir_all` | `Remove`・`RemoveAll` | `unlink`・`shutil.rmtree` | `rm` | — | `rm`・`rm_rf` | `Sys.remove` | `delete`・`deleteRecursively` | `removeItem` |
| コピー・移動 | `copy`・`rename` | `copy`・`rename` | —・`Rename` | `shutil.copy`・`rename` | `copyFile`・`rename` | — | `cp`・`rename` | —・`Sys.rename` | `copyTo`・`renameTo` | `copyItem`・`moveItem` |
| 一覧・走査・glob | `list_dir`・`walk`・`glob` | `read_dir` | `ReadDir`・`WalkDir`・`Glob` | `iterdir`・`rglob`・`glob` | `readdir`・`glob` | — | `ls`・`Path.wildcard` | `Sys.readdir` | `listFiles`・`walk` | `contentsOfDirectory` |
| 存在と種類 | `exists`・`is_dir`・`is_file`・`is_symlink` | `exists`・`is_dir`・`is_file`・`is_symlink` | `Stat`+`IsDir` | `exists`・`is_dir`・`is_file`・`is_symlink` | `existsSync`・`stat` | — | `exists?`・`dir?`・`regular?` | `Sys.file_exists`・`is_directory` | `exists`・`isDirectory`・`isFile` | `fileExists` |
| メタデータ | `stat`・`file_size`・`modified_at` | `metadata` | `Stat` | `stat`・`getsize`・`getmtime` | `stat` | — | `stat` | `Unix.stat` | `length`・`lastModified` | `attributesOfItem` |
| 一時ファイル | `temp_dir`・`create_temp_file`・`create_temp_dir` | `env::temp_dir` | `CreateTemp`・`MkdirTemp` | `tempfile` | `mkdtemp` | — | `System.tmp_dir` | `Filename.temp_file` | `createTempFile` | `temporaryDirectory` |
| シンボリックリンクを作る・読む | — | `symlink`・`read_link` | `Symlink`・`Readlink` | `symlink_to`・`readlink` | `symlink`・`readlink` | — | `ln_s`・`read_link` | `Unix.symlink` | — | `createSymbolicLink` |
| 権限 | — | `set_permissions` | `Chmod` | `chmod` | `chmod` | — | `chmod` | `Unix.chmod` | `setExecutable` | `setAttributes` |
| ファイルハンドル | — | `File` | `os.File` | `open` | `open` | — | `File.open` | `In_channel`・`Out_channel` | `File` | `FileHandle` |
| 実パスに解決 | — | `canonicalize` | `EvalSymlinks` | `resolve` | `realpath` | — | — | `Unix.realpath` | `canonicalPath` | `resolvingSymlinksInPath` |

### 完全性の規則

- **中身を読む関数の族:** {テキスト `read_text`, 行 `read_lines`, バイト `read_bytes`（`Bytes`）} の 3 つ。どれにも `_if_exists` 版がちょうど 1 つあります（`Result[Option[T], String]`: 無いなら none、それ以外の失敗は err）。メタデータ、ディレクトリ操作、書き込み、削除には付けません（いまのゲート `tests/fs_if_exists_family_gate_test.rs` の理由のとおり）。
- **行を流す族:** {`fold_lines`, `for_each_line`, `fold_lines_range`, `fold_lines_chunked`} に閉じます。失敗しうる callback の版は、最初の err が決まる逐次の 2 つだけ（`tests/fs_streaming_family_gate_test.rs`、ADR-0006）。
- **書く族:** {上書き `write`（String）, `write_bytes`（Bytes）, 追記 `append`（String）}。Bytes の追記は置きません（下の R-fs-04）。
- **バイトの入れ物は `Bytes` だけ**（R-L0-07 の 4）。
- **メタデータは `stat` が持ちます。** `FileStat` のフィールドを 1 つ取り出すだけの関数は置きません（R-L0-01）。存在と種類の述語（`exists`、`is_dir`、`is_file`、`is_symlink`）は、無いときに false を返す点で `stat`（無いと err）と違うので残します。

### L2 の判定

- 型の食い違いと重複: `read_bytes`・`write_bytes`・`read_bytes_if_exists` が `List[Int]`、同じ役割の `_raw` 版が `Bytes` → R-fs-01（無印の名前を `Bytes` にし、`_raw` 版を撤去）
- 重複: `file_size` ＝ `stat(p).size`、`modified_at` ＝ `stat(p).modified`（実測で一致、probe-fs）→ R-fs-02
- 重複（他モジュール）: `env.temp_dir` ＝ `fs.temp_dir` → fs が残る側（L2-env R-env-01）
- 族のゲート: R-fs-01 で `_if_exists` 族が 4 → 3 → R-fs-03
- 抜けている役割（5/9 以上）: ディレクトリ 1 段だけ作る、シンボリックリンクを作る・読む、権限、ファイルハンドル、実パス → R-fs-04（× と理由）

## 案

### R-fs-01 バイトの読み書きを無印の名前のまま `Bytes` にし、`_raw` 版を撤去する — ○ 採用

- 内容:
  - `fs.read_bytes(p) -> Bytes`、`fs.write_bytes(p, data: Bytes)`、`fs.read_bytes_if_exists(p) -> Bytes?` にします。名前は変えず、`List[Int]` を `Bytes` に変えます。
  - 同じ役割の `fs.read_bytes_raw`（→ `fs.read_bytes`）、`fs.write_bytes_raw`（→ `fs.write_bytes`）、`fs.read_bytes_raw_if_exists`（→ `fs.read_bytes_if_exists`）を撤去します。
- 名前: 型を変えると、旧い書き方（`List[Int]` を渡す・受け取る）は必ず型エラーになります。黙って違う結果にはならないので、R-L0-14 の 4 が許す変更です。`_raw` は「バイト列の型が違う版」を表すだけの接尾辞で、`Bytes` が唯一の入れ物になれば意味を失います。io も同じ形にそろえます（`io.read_bytes`・`io.write_bytes`、L2-io R-io-01）。
- 根拠の L0 規則: R-L0-07 の 4、R-L0-01、R-L0-14 の 4。
- (a) 9 言語すべてで、ファイルのバイトは専用のバイト列の型です。数のリストで返す言語はありません。
- (b) `wasi:filesystem`。新しい `read_bytes`・`write_bytes` の本体は、いまの `_raw` 版の本体をそのまま使います。この本体はすでに native と wasm の両方で動いています（壁の行なし）。
- (c) `fs.write_bytes` は Int の下位 8 ビットだけを書きます（`-1` が 255 になる）。範囲外の値が黙って変わります。`List[Int]` は 1 バイトに 8 バイト使います（#2078 の実測）。
- (d) 意味変更（型を変える）と撤去。
  - 無印の 3 つ: 旧い呼び出しは型エラーになり、ヒントと `almide fix` で書き換えます。`fs.read_bytes(p)!` を `List[Int]` として使う箇所は `bytes.to_list(fs.read_bytes(p)!)`、`fs.write_bytes(p, xs)` は `fs.write_bytes(p, bytes.from_list(xs))` にします。型は epoch 15 の `breaks` に入れ、件数を `summary` に書きます。使用数: `read_bytes` 8/13/20/2、`write_bytes` 12/1/13/3、`read_bytes_if_exists` 10/4/5/2。
  - `_raw` の 3 つ: `@deprecated(since=15, use="fs.read_bytes")`（`write_bytes_raw` は `use="fs.write_bytes"`、`read_bytes_raw_if_exists` は `use="fs.read_bytes_if_exists"`）、`removal_epoch = 16`。名前だけの 1 対 1 の改名なので、`almide fix` が書き換えます。
  - 無印の型の変更は、`_raw` の窓と同じ epoch 15 で入れます。窓の間、`_raw` を使うコードはそのまま動き、無印を `List[Int]` で使うコードだけが型エラーになります。
- ゲート案: R-L0-G の「Bytes の対がある `List[Int]` バイト API の拒否」（L2-io R-io-01 と同じ検査）。
- issue: #3896

### R-fs-02 `file_size` と `modified_at` を撤去し、`stat` に一本化する — ○ 採用

- 内容: `fs.file_size(p)` → `fs.stat(p)!.size`、`fs.modified_at(p)` → `fs.stat(p)!.modified`。あわせて `stat` の一行説明「Size, kind and mtime」を、実際のフィールド名（`size`、`is_dir`、`is_file`、`modified`）に合わせます（実測: `.mtime` は E013）。
- 根拠の L0 規則: R-L0-01（同じ値を 2 つの綴りで返さない）。
- (a) Rust・Go・Node・Elixir・OCaml はメタデータの構造体だけを持ちます。個別の関数も持つのは Python（`getsize`）と Kotlin（`length`）です。
- (b) 変化なし（`wasi:filesystem` の `stat`）。
- (c) 撤去後のヒントは機械的です。`stat` の 1 つを覚えればよくなります。
- (d) 撤去。`@deprecated(since=15, use="fs.stat(path)!.size")`（`modified_at` は `use="fs.stat(path)!.modified"`）、`removal_epoch = 16`。`almide fix` で書き換えます。使用数: `file_size` 8/2/9/1、`modified_at` 3/0/8/1。
- ゲート案: 同義語ゲート規則 5 に「戻り値が別の公開関数の戻り値のフィールド 1 つと同じ」を足すのは本体の比較では検出できないので、R-L0-G の許容リストで止めます（新しい projection 関数を足すと失敗）。
- issue: #3912

### R-fs-03 2 つの族のゲートを新しい形に合わせる — ○ 採用

- 内容: `tests/fs_if_exists_family_gate_test.rs` の族を {`read_text`, `read_lines`, `read_bytes`} の 3 つにします（`read_bytes_raw_if_exists` の撤去で、epoch 16 に数が 4 → 3）。窓の間は `@deprecated` の行を数えません。`read_bytes_if_exists` の型が `Bytes?` であることもゲートで確かめます。行を流す族のゲートは変えません。
- 根拠: CLAUDE.md「API families are extended by matrix」、R-L0-14。
- (b) 影響なし。(c) 族の形が文書と一致します。(d) ゲートの変更だけ。
- issue: R-fs-01 と同じ issue
- issue: #3896

### R-fs-04 5/9 以上が持つが Almide に無い役割 — × 不採用（いまは足さない）

- ディレクトリを 1 段だけ作る `mkdir`: `mkdir_p` で足ります。「すでにあると err」を使うのはロックの用途だけで、2 つあると選び間違えます（R-L0-01）。
- シンボリックリンクを作る・読む: `wasi:filesystem` に `symlink-at`・`readlink-at` はありますが、使用の要望がまだありません。`is_symlink` で判定はできます。要望が出たら `symlink`・`read_link` の対で足します。
- 権限（chmod）: WASI の filesystem に権限の操作がありません。R-L0-15 の 3 つの経路のうち、標準の WASI インタフェースには乗らず、暫定の host op を足す理由（使用の要望）もありません。要望が出たら package（native の権限と、それを持たない wasm の扱いを package 側で決める）として提供します。標準には足しません。
- ファイルハンドル: 読むほうは行を流す族、書くほうは `append` で足ります。ハンドルの型を足すと、同じ処理に書き方が 2 つできます（L1 の遅延イテレータ × と同じ理由）。
- 実パスへの解決: `path.normalize` が字句の上で足ります。シンボリックリンクをたどる版は R-L0-15 の実装（`readlink-at` の繰り返し）が要るので、要望が出たら足します。
- Bytes の追記: `append_<型>` は R-L0-04 で「`mut` のバイト列に書き足す」動詞に予約されているので、`fs.append_bytes` は名前の規則にぶつかります。使用例もありません。
- issue: なし

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `fs.read_text` | `effect fn read_text(path: String) -> String` | Rust `read_to_string` / Python `read_text` / Kotlin `readText` / Elixir `File.read` | 据え置き | — |
| `fs.read_bytes` | `effect fn read_bytes(path: String) -> List[Int]` | Rust `fs::read` / Go `ReadFile` / Python `read_bytes` | 意味変更（`-> Bytes` にする。唯一のバイト読み） | R-fs-01 |
| `fs.read_bytes_raw` | `effect fn read_bytes_raw(path: String) -> Bytes` | 同上 | 撤去（→ `read_bytes`） | R-fs-01 |
| `fs.read_lines` | `effect fn read_lines(path: String) -> List[String]` | Kotlin `readLines` / OCaml `input_lines` | 据え置き | — |
| `fs.read_text_if_exists` | `effect fn read_text_if_exists(path: String) -> String?` | — （Almide 固有。ADR-0004 D4） | 据え置き | R-fs-03 |
| `fs.read_bytes_if_exists` | `effect fn read_bytes_if_exists(path: String) -> List[Int]?` | — | 意味変更（`-> Bytes?` にする） | R-fs-01・R-fs-03 |
| `fs.read_lines_if_exists` | `effect fn read_lines_if_exists(path: String) -> List[String]?` | — | 据え置き | R-fs-03 |
| `fs.read_bytes_raw_if_exists` | `effect fn read_bytes_raw_if_exists(path: String) -> Bytes?` | — | 撤去（→ `read_bytes_if_exists`） | R-fs-01・R-fs-03 |
| `fs.fold_lines` | `effect fn fold_lines[A](path: String, init: A, f: (A, String) -> A) -> A` | OCaml `In_channel.fold_lines` / Kotlin `useLines` | 据え置き | — |
| `fs.for_each_line` | `effect fn for_each_line(path: String, f: (String) -> Unit) -> Unit` | Kotlin `forEachLine` / Rust `lines()` | 据え置き | — |
| `fs.fold_lines_range` | `effect fn fold_lines_range[A](path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A` | —（Almide 固有） | 据え置き | — |
| `fs.fold_lines_chunked` | `effect fn fold_lines_chunked[A](path: String, workers: Int, init: A, f: (A, String) -> A) -> List[A]` | —（Almide 固有。`@concurrent`） | 据え置き | — |
| `fs.write` | `effect fn write(path: String, content: String) -> Unit` | Rust `fs::write` / Go `WriteFile` / Python `write_text` / Elixir `File.write` | 据え置き | — |
| `fs.write_bytes` | `effect fn write_bytes(path: String, bytes: List[Int]) -> Unit` | Python `write_bytes` / Kotlin `writeBytes` | 意味変更（`data: Bytes` にする） | R-fs-01 |
| `fs.write_bytes_raw` | `effect fn write_bytes_raw(path: String, data: Bytes) -> Unit` | 同上 | 撤去（→ `write_bytes`） | R-fs-01 |
| `fs.append` | `effect fn append(path: String, content: String) -> Unit` | Node `appendFile` / Kotlin `appendText` | 据え置き | R-fs-04 |
| `fs.mkdir_p` | `effect fn mkdir_p(path: String) -> Unit` | Rust `create_dir_all` / Go `MkdirAll` / Elixir `mkdir_p` | 据え置き | R-fs-04 |
| `fs.exists` | `effect fn exists(path: String) -> Bool` | Rust・Python `exists` / Elixir `exists?` | 据え置き（R-L0-09 の動詞集合） | — |
| `fs.is_dir` | `effect fn is_dir(path: String) -> Bool` | Rust `is_dir` / Kotlin `isDirectory` | 据え置き | — |
| `fs.is_file` | `effect fn is_file(path: String) -> Bool` | Rust `is_file` / Kotlin `isFile` | 据え置き | — |
| `fs.is_symlink` | `effect fn is_symlink(path: String) -> Bool` | Rust `is_symlink` / Python `is_symlink` | 据え置き | — |
| `fs.remove` | `effect fn remove(path: String) -> Unit` | Rust `remove_file` / Go `Remove` / Elixir `rm` | 据え置き | — |
| `fs.remove_all` | `effect fn remove_all(path: String) -> Unit` | Rust `remove_dir_all` / Go `RemoveAll` / Elixir `rm_rf` | 据え置き | — |
| `fs.list_dir` | `effect fn list_dir(path: String) -> List[String]` | Rust `read_dir` / Go `ReadDir` / Python `listdir` / Elixir `ls` | 据え置き（多数派なし） | — |
| `fs.walk` | `effect fn walk(dir: String) -> List[String]` | Go `WalkDir` / Python `walk` | 据え置き | — |
| `fs.glob` | `effect fn glob(pattern: String) -> List[String]` | Go `Glob` / Python `glob` / Node `glob` | 据え置き | — |
| `fs.copy` | `effect fn copy(src: String, dst: String) -> Unit` | Rust `copy` / Elixir `cp` | 据え置き | — |
| `fs.rename` | `effect fn rename(src: String, dst: String) -> Unit` | Rust・Go・Python・Node `rename` | 据え置き | — |
| `fs.stat` | `effect fn stat(path: String) -> FileStat` | Rust `metadata` / Go `Stat` / Python `stat` / Node `stat` | 据え置き（一行説明を直す） | R-fs-02 |
| `fs.file_size` | `effect fn file_size(path: String) -> Int` | Python `getsize` / Kotlin `length` | 撤去（→ `stat(p)!.size`） | R-fs-02 |
| `fs.modified_at` | `effect fn modified_at(path: String) -> Int` | Python `getmtime` / Kotlin `lastModified` | 撤去（→ `stat(p)!.modified`） | R-fs-02 |
| `fs.temp_dir` | `effect fn temp_dir() -> String` | Rust `env::temp_dir` / Go `TempDir` / Elixir `System.tmp_dir` | 据え置き（`env.temp_dir` を吸収） | L2-env R-env-01 |
| `fs.create_temp_file` | `effect fn create_temp_file(prefix: String) -> String` | Go `CreateTemp` / Python `mkstemp` | 据え置き | — |
| `fs.create_temp_dir` | `effect fn create_temp_dir(prefix: String) -> String` | Go `MkdirTemp` / Python `mkdtemp` / Node `mkdtemp` | 据え置き | — |
