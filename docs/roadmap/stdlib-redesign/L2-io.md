<!-- description: Stdlib redesign L2/L3 — io: stdin/stdout; read_line keeps its name and tells EOF apart with Option, read_bytes/write_bytes on Bytes as in fs, writers become effect fns as ADR-0022 already says -->
# L2/L3: io

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)
> 出典: Rust https://doc.rust-lang.org/std/io/ ・ Go https://pkg.go.dev/bufio ・ Python https://docs.python.org/3/library/sys.html#sys.stdin ・ TS/JS https://nodejs.org/api/process.html#processstdin ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/io.html （出力だけ）・ Elixir https://hexdocs.pm/elixir/IO.html ・ OCaml https://ocaml.org/manual/latest/api/In_channel.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io/readln-or-null.html ・ Swift https://developer.apple.com/documentation/swift/readline(strippingnewline:)
> (F) = 今回取り寄せて確かめたセル。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 行読む（EOF を区別） | `io.read_line_opt` | `read_line`（0 で EOF） | `Scanner.Scan` | `readline()`（"" で EOF） | `readline` の `line` | — | `IO.gets`（`:eof`） | `In_channel.input_line`（option） | `readlnOrNull`（null）(F) | `readLine()`（nil） |
| 1 行読む（EOF を "" にする） | `io.read_line` | — | — | `sys.stdin.readline()` | — | — | — | — | — | — |
| 全部読む | `io.read_all` | `read_to_string` | `io.ReadAll` | `sys.stdin.read()` | — | — | `IO.read(:eof)` | `In_channel.input_all` | `System.in.readAllBytes` | `FileHandle.readDataToEndOfFile` |
| 全行をリストで | `process.stdin_lines` | `lines()` | `Scanner` | `readlines()` | — | — | `IO.stream` | `input_lines` | `generateSequence(::readLine)` | — |
| バイトを読む | `io.read_byte`（-1 で EOF）・`io.read_n_bytes`（List[Int]） | `read(&mut buf)` | `Read(p)` | `sys.stdin.buffer.read(n)` | `stdin.read(n)` | — | `IO.binread(n)` | `In_channel.input` | `System.in.read` | `FileHandle.readData(ofLength:)` |
| 文字列を改行なしで書く | `io.print` | `print!` | `fmt.Print` | `print(end="")` | `process.stdout.write` | `io.print` | `IO.write` | `print_string` | `print` | `print(terminator: "")` |
| バイトを書く | `io.write`（Bytes）・`io.write_bytes`（List[Int]） | `write_all` | `os.Stdout.Write` | `sys.stdout.buffer.write` | `stdout.write(buf)` | — | `IO.binwrite` | `Out_channel.output_bytes` | `System.out.write` | `FileHandle.write` |

### 完全性の規則

- 標準入力の読み取りは {1 行 `read_line`, 全部 `read_all`, n バイトまで `read_bytes`} の 3 つです。全行のリストは `string.lines(io.read_all()!)` で書けるので置きません（R-L0-01 の「合成で書けるもの」）。
- 「失敗」の表し方（R-L0-06 の 4: 失敗の目印になる値を返さない）: 1 行は EOF で none。バイトは EOF で空の `Bytes`（`net.tcp_read` と同じ。Rust の `read` が 0 を返すのと同じ約束）。`-1` や `""` を EOF の印にしません。
- バイト列の入れ物は `Bytes` だけです（R-L0-07 の 4）。読み書きの名前は fs と同じ `read_bytes`・`write_bytes` にそろえます（L2-fs R-fs-01）。
- 書き出しの関数はすべて effect fn です。ADR-0022 D1 が「`io.print`・`io.write`・`io.write_bytes` は effect fn で、純粋な fn からは E006」と決めています。
- 標準エラーへの書き出しは組み込みの `eprintln` が持ちます。

### L2 の判定

- 規則違反（失敗の目印）: `io.read_line` は EOF と空行がどちらも `""`、`io.read_byte` は EOF が `-1` → R-io-01
- 重複: `io.read_line_opt` と `io.read_line` が同じ役割（1 行読む）→ R-io-01（無印の名前を `Option` にする）
- 型の食い違いと重複: `io.read_n_bytes`・`io.write_bytes` が `List[Int]`、同じ役割の `io.write` が `Bytes` → R-io-01
- 置き場所違い: `process.stdin_lines`（標準入力は io の持ち分）→ R-io-01
- 宣言の穴: `io.write` と `io.write_bytes` が `fn`（純粋）として宣言されています。実測で、純粋な fn から `io.write` を呼んでもエラーになりません（probe-io）。ADR-0022 D1 に反します → R-io-02

## 案

### R-io-01 読み取りを「EOF を区別する 1 行」「Bytes」にそろえる — ○ 採用

- 内容:
  - 1 行: `io.read_line` の名前を残し、戻り型を `String` から `Option[String]`（EOF で none）に変えます。`io.read_line_opt` は撤去します（→ `io.read_line`）。
  - バイトを読む: `effect fn read_bytes(n: Int) -> Bytes`（最大 n バイト。EOF で空。`n <= 0` で空）を足し、`io.read_byte`（→ `io.read_bytes(1)`）と `io.read_n_bytes`（→ `io.read_bytes(n)`）を撤去します。
  - バイトを書く: `io.write_bytes` の名前を残し、引数を `List[Int]` から `Bytes` に変えます。同じ役割の `io.write`（`Bytes`）は撤去します（→ `io.write_bytes`）。読みと書きが `read_bytes`・`write_bytes` の対になり、fs と同じ形になります。
  - 全行: `process.stdin_lines()` を撤去します（→ `string.lines(io.read_all()!)`。L2-process の行）。
- 根拠の L0 規則: R-L0-06 の 1 と 4（無印が安全版。失敗の目印を返さない）、R-L0-07 の 4（`Bytes` が唯一のバイトの入れ物）、R-L0-01（同じ役割に綴りを 1 つ）、R-L0-14 の 4（旧い書き方が必ず型エラーになる変更は、同じ名前のまま許す）。
- (a) EOF を `Option`（nil/null/none）で返すのは Swift `readLine`・Kotlin `readlnOrNull`(F)・OCaml `input_line` で、Elixir も `:eof` を別の値で返します。`""` を EOF にするのは Python の `readline()` だけです。
- (b) 標準入力は `wasi:cli/stdin`、標準出力は `wasi:cli/stdout`。いまの本体はどれも native と組み込み host の両方で動いています（壁の行なし）。新しい `read_line` は `read_line_opt` の本体、`read_bytes` は `read_n_bytes` の本体の戻り型を `Bytes` にしたもの、新しい `write_bytes` は `io.write` の本体を使います。ADR-0027 D2 の固定 stdin で決定的に検品できます。
- (c) `while` で `io.read_line()` を読み、`""` で止めるコードは、空行で途中で止まります。LLM はこの誤りに気づけません。戻り型が `Option` になれば、`""` との比較は型エラーになり、書き直しが必ず起きます。
- (d) 意味変更（型を変える）と撤去。
  - `io.read_line`: 戻り型を `Option[String]` にします。旧い書き方（`String` として使う）は型エラーになるので、R-L0-14 の 4 で許されます。ヒント「`io.read_line()` は EOF で none を返します。`io.read_line()! ?? ""` で旧い挙動」と `almide fix` の書き換えを付けます。epoch 15 の `breaks` に入れ、件数を `summary` に書きます。
  - `io.read_line_opt`: `@deprecated(since=15, use="io.read_line")`、`removal_epoch = 16`。`almide fix` が `io.read_line_opt()` を `io.read_line()` に書き換えます（1 対 1）。
  - `io.read_byte`: `@deprecated(since=15, use="io.read_bytes(1)")`、`io.read_n_bytes`: `@deprecated(since=15, use="io.read_bytes(n)")`、どちらも `removal_epoch = 16`。`read_byte` は形が変わる（`Int` → `Bytes`）ので、ヒントだけ出します（「`bytes.get_u8(io.read_bytes(1)!, 0)` が none なら EOF」）。`read_n_bytes` は `almide fix` が `bytes.to_list(io.read_bytes(n)!)` に書き換えます。
  - `io.write_bytes`: 引数を `Bytes` にします。旧い書き方は型エラーになり、`almide fix` が `io.write_bytes(bytes.from_list(xs))` に書き換えます。
  - `io.write`: `@deprecated(since=15, use="io.write_bytes")`、`removal_epoch = 16`。`almide fix` が 1 対 1 で書き換えます。
  - `process.stdin_lines`: L2-process R-process-01 の行で扱います。
  - 使用数: `io.read_line` 5/24/20/8、`io.read_byte` 1/16/13/1、`io.read_n_bytes` 7/16/23/8、`io.write_bytes` 7/4/21/1、`io.write` 12/8/27/3。CHEATSHEET:885-890 の `io.read_line` の説明を「`Option[String]` を返し、EOF で none」に書き換えます（`check-llm-surface.sh`）。
- ゲート案: R-L0-G に「`List[Int]` のバイト API を拒否」を足し、io・fs・string の許容リストを減らしていきます。EOF の fixture（空行のあとの EOF を none と some("") で区別）を native と wasm で置きます。
- issue: #3896

### R-io-02 `io.write_bytes` を effect fn に直す — ○ 採用

- 内容: `fn write_bytes(data: …) -> Unit` を `effect fn write_bytes(data: Bytes) -> Unit` にします（型の変更は R-io-01）。窓の間に残る `io.write` も、同じ epoch で `effect fn` にします。
- 根拠の L0 規則: R-L0-13（環境に触る関数は effect）と ADR-0022 D1（純粋な fn に許すのは 6 つの組み込みだけで、`io.write`・`io.write_bytes` は含まない）。
- (a) 該当なし（効果の型を持つ言語は少ない）。
- (b) 検査器だけの変更です。
- (c) `io.print` は effect、`io.write` は純粋という差は、宣言を読まないと分かりません。ADR の文と実装が違うと、文書を読んだ LLM のコードが検査の結果と合いません。
- (d) 意味変更（純粋 → effect）。名前は変えません。前例は dialect epoch 8・13（argv の読み取りを effect にした）で、同じく `breaks` に入れます。純粋な fn から呼んでいる箇所は E006 になり、ヒント「呼び出し元を `effect fn` にする」を出します。epoch 15 の `breaks` で件数を数えます。使用数 12/8/27/3（`io.write`）、7/4/21/1（`io.write_bytes`）。
- ゲート案: ADR-0022 D1 の名前の集合と、`io` の宣言の `effect` 欄を突き合わせるテスト。
- issue: #3907

### R-io-03 改行なしで標準エラーに書く `io.eprint` を足し、`print` の綴りから `io.print` へ案内する — ○ 採用

- いま: 改行なしの書き出しは `io.print`（標準出力）だけで、標準エラーにはありません。`print("x")` と書くと「もしかして `println`?」と出ます（改行が付くので出力が変わる案内です）。
- 案: `effect fn io.eprint(s: String) -> Unit` を足します。未定義の `print`・`eprint` の案内を `io.print`・`io.eprint` にします（R-L0-19）。
- (a) 改行なしの標準エラーは 9/9 が持ちます（Rust `eprint!` (F)、Gleam `io.print_error` (F)、Go `fmt.Fprint(os.Stderr, …)` など）。
- (b) `wasi:cli/stderr`。`eprintln` と同じ経路です。
- (c) 進み具合の表示（`\r` で書き直す）を LLM はほぼ必ず標準エラーに書きます。
- (d) 追加と診断です。
- 完全性の規則: 出力は {stdout, stderr} × {改行あり, 改行なし} の 4 セル。改行ありは組み込み、改行なしは `io` です。
- issue: #3974

### R-io-04 標準入力をバイト列で全部読む `io.read_all_bytes` を足す — ○ 採用

- いま: `io.read_all() -> String`（全部をテキスト）と `io.read_bytes(n) -> Bytes`（最大 n バイト）。全部をバイト列で読む関数がありません。
- 案: `effect fn io.read_all_bytes() -> Bytes`。
- (a) 8/9 が持ちます（Python `sys.stdin.buffer.read()` (F)、Go `io.ReadAll(os.Stdin)`、Rust `read_to_end` など）。
- (b) `wasi:cli/stdin`。`read_all` と同じ経路で、UTF-8 に直しません。
- (c) いまは `io.read_all()` を経て不正な UTF-8 が置換文字になり、黙って結果が変わります。
- (d) 追加 1。
- 完全性の規則: 標準入力の読みは {1 行 `read_line`, 全部テキスト `read_all`, 全部バイト `read_all_bytes`, 最大 n バイト `read_bytes`} です。
- issue: #3974

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `io.read_line` | `effect fn read_line() -> String` | Swift `readLine` / Kotlin `readlnOrNull`(F) / OCaml `input_line` | 意味変更（`-> String?` にする。唯一の 1 行読み） | R-io-01 |
| `io.read_line_opt` | `effect fn read_line_opt() -> String?` | 同上 | 撤去（→ `read_line`） | R-io-01 |
| `io.read_all` | `effect fn read_all() -> String` | Rust `read_to_string` / Go `io.ReadAll` / OCaml `input_all` | 据え置き | — |
| `io.read_byte` | `effect fn read_byte() -> Int` | Rust `read` / Python `read(1)` | 撤去（→ `read_bytes(1)`） | R-io-01 |
| `io.read_n_bytes` | `effect fn read_n_bytes(n: Int) -> List[Int]` | Python `buffer.read(n)` / Elixir `binread(n)` | 撤去（→ `read_bytes(n)`） | R-io-01 |
| `io.read_bytes` | `effect fn read_bytes(n: Int) -> Bytes`（案） | 同上 | 追加 | R-io-01 |
| `io.print` | `effect fn print(s: String) -> Unit` | Rust `print!` / Go `fmt.Print` / Gleam `io.print` | 据え置き | — |
| `io.write` | `fn write(data: Bytes) -> Unit` | Rust `write_all` / Go `Stdout.Write` / Elixir `binwrite` | 撤去（→ `write_bytes`） | R-io-01・R-io-02 |
| `io.write_bytes` | `fn write_bytes(data: List[Int]) -> Unit` | Python `buffer.write` / Kotlin `writeBytes` | 意味変更（`data: Bytes` にし、effect fn にする） | R-io-01・R-io-02 |
| `io.eprint` | `effect fn eprint(s: String) -> Unit`（案） | Rust `eprint!` (F) / Gleam `io.print_error` (F) / OCaml `prerr_string` | 追加 | R-io-03 |
| `io.read_all_bytes` | `effect fn read_all_bytes() -> Bytes`（案） | Python `stdin.buffer.read()` (F) / Go `io.ReadAll(os.Stdin)` | 追加 | R-io-04 |

## L0 への差し戻し（処理済み）

1. **無印の名前を失敗の目印を返す版が使っている場合**: R-L0-14 の 4 で決まりました。戻り型を `String` から `Option[String]` に変えると旧い書き方は必ず型エラーになるので、無印の名前のまま安全版に変えます。`io.read_line` が `Option[String]` を返し、`read_line_opt` は撤去します（R-io-01）。`_opt` の接尾辞は置きません。
2. **`Option` を返す effect fn の `!` と `??` の扱い**: R-L0-17（呼び出しの型は宣言だけで決まる）で決まりました。`io.read_line` の呼び方は、本体の書き方によらず、宣言 `effect fn read_line() -> String?` だけから決まります。どちらの形に揃えるかの設計は R-L0-17 の issue で行います（L2-env R-env-03 と同じ）。
