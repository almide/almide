<!-- description: Stdlib redesign L2/L3 — regex: a compiled Regex type that stops bad patterns and the pipe swap, is_full_match, and the escape / position / all-captures / replace_with holes -->
# L2/L3: regex

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)
> 出典: Rust https://docs.rs/regex/latest/regex/ （標準外のクレート）・ Go https://pkg.go.dev/regexp (F) ・ Python https://docs.python.org/3/library/re.html (F) ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp (F) ・ Gleam https://hexdocs.pm/gleam_regexp/ （標準外のパッケージ）・ Elixir https://elixir.hexdocs.pm/Regex.html (F) ・ OCaml https://ocaml.org/manual/latest/api/Str.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/-regex/ (F) ・ Swift https://developer.apple.com/documentation/swift/regex

`regex` は 8 関数で、`import regex` が要ります。いまはパターンを文字列のまま渡し、コンパイル済みの型はありません（R-regex-02 で `Regex` 型を入れます）。エンジンは純粋な Almide（`stdlib/regex_engine.almd`）なので、native と wasm で同じ結果になります（probe-regex/p.almd を両 target で実行し、出力が一致）。

## L2: 役割の比較

| 役割 | Almide | Rust (regex) | Go | Python | TS/JS | Gleam (regexp) | Elixir | OCaml (Str) | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| どこかに一致するか | `regex.is_match` | `is_match` | `MatchString` (F) | `search` (F) | `test` (F) | `check` | `match?` (F) | `search_forward` (F) | `containsMatchIn` (F) | `contains` |
| 全体が一致するか | `regex.full_match` | `^…$` で `is_match` | `^…$` | `fullmatch` (F) | `^…$` で `test` | — | — | `string_match` + 長さ | `matches` (F) | `wholeMatch` |
| 最初の一致の文字列 | `regex.find` | `find` | `FindString` (F) | `search().group()` | `match` (F) | `scan` | `run` (F) | `matched_string` (F) | `find` (F) | `firstMatch` |
| すべての一致 | `regex.find_all` | `find_iter` | `FindAllString` (F) | `findall` (F) | `matchAll` (F) | `scan` | `scan` (F) | — | `findAll` (F) | `matches` |
| 全部置換（文字そのまま） | `regex.replace` | `replace_all` + `NoExpand` | `ReplaceAllLiteralString` (F) | `sub`（`\1` を解釈） | `replaceAll` | `replace` | `replace` (F) | `global_replace` (F) | `replace` (F) | `replacing` |
| 最初だけ置換 | `regex.replace_first` | `replace` | — | `sub(count=1)` | `replace` | — | `replace(global: false)` | `replace_first` (F) | `replaceFirst` (F) | — |
| 一致で分割 | `regex.split` | `split` | `Split` (F) | `split` (F) | `split` (F) | `split` | `split` (F) | `split` (F) | `split` (F) | `split` |
| 最初の一致のグループ | `regex.captures` | `captures` | `FindStringSubmatch` (F) | `search().groups()` | `exec` (F) | `scan` | `run` (F) | `matched_group` (F) | `find().groupValues` | `firstMatch().output` |
| すべての一致のグループ | — | `captures_iter` | `FindAllStringSubmatch` (F) | `finditer` (F) | `matchAll` (F) | `scan` | `scan` (F) | — | `findAll().groupValues` | `matches` |
| 一致の位置 | — | `find().start()` | `FindStringIndex` (F) | `search().start()` | `search` (F) | — | `run(return: :index)` | `search_forward` の戻り値 (F) | `find().range` | `firstMatch().range` |
| 特殊文字のエスケープ | — | `escape` | `QuoteMeta` (F) | `escape` (F) | `RegExp.escape`（ES2025） | — | `escape` (F) | `quote` (F) | `Regex.escape` | — |
| グループを使う置換（関数） | — | `replace_all(closure)` | `ReplaceAllStringFunc` (F) | `sub(fn)` (F) | `replace(fn)` | `match_map` | `replace(fn)` (F) | `global_substitute` (F) | `replace(transform)` (F) | `replacing(with: closure)` |
| 置換文字列の `$1` | — | `$1` | `$1` (F) | `\1` | `$1` | — | `\1` | `\1` | `$1` | — |
| 大文字小文字を区別しない | — | `(?i)` | `(?i)` | `re.I` / `(?i)` | `i` フラグ | `case_insensitive` | `i` (F) | `regexp_case_fold` (F) | `IGNORE_CASE` | `ignoresCase()` |
| 名前つきグループ | — | `(?P<n>)` / `(?<n>)` | `(?P<n>)` / `SubexpNames` (F) | `(?P<n>)` のみ (F) | `(?<n>)` | — | `named_captures` (F) | — (F) | `groups["n"]` | `Capture` |
| 不正なパターンを知らせる | — （黙って通る） | `Regex::new` が `Result` | `Compile` が error (F) | `re.error` | `SyntaxError` | `compile` が `Result` | `compile` が `{:error, _}` (F) | 例外 | 例外 | `throws` |
| 引数の順 | パターンが先 | パターンが主語 | パターンが主語 | パターンが先 (F) | パターンが主語 | パターンが先 | パターンが先 (F) | パターンが先 (F) | パターンが主語 | 文字列が主語 |

### 完全性の規則

- 文字列モジュールとの対: `regex.replace`/`replace_first`/`split`/`find_all` は、`string.replace`/`replace_first`/`split` と同じ動詞・同じ意味（`replace` は全部、空の断片は残す）です。この対応を崩しません。
- 位置は `string.index_of` と同じくコードポイントの添字で返します。
- 判定（Bool）は `is_` で始めます（R-L0-09）。

### L2 の判定

- R-L0-09 の例外: `regex.full_match`（R-regex-01）。
- 黙って間違える箇所が 2 つ（実測）:
  1. 不正なパターンがエラーにならない。`regex.is_match("(", "abc")` は `true`、`regex.find("[", "abc")` は `none`、`(?i)ABC` は `abc` に一致せず `false`、名前つきグループ `(?P<n>a)` / `(?<n>a)` は `none`（probe-regex/p.almd・q.almd、native と wasm で同じ）。9/9 言語は不正なパターンを知らせます（R-regex-02）。
  2. `|>` でパイプすると、文字列がパターンの位置に入る。`"order 42" |> regex.is_match("[0-9]+")` は `false`、`|> regex.find(..)` は `none`（probe-regex/swap.almd）。`"  a \t b\nc  " |> string.trim |> regex.split("\\s+")` も黙って違う結果になります（probe-string/e.almd）。両方の引数が String なので、いまの型検査では止まりません。`Regex` 型で止めます（R-regex-02）。
- 抜けている役割で 5/9 以上: エスケープ（7）、一致の位置（8）、すべての一致のグループ（8）、大文字小文字の区別なし（9）、関数による置換（9）、名前つきグループ（7）。前の 4 つは足します（R-regex-04）。関数による置換は `replace_with` で足します（R-regex-05）。名前つきグループは足しません（R-regex-06）。

## 案

### R-regex-01 `regex.full_match` を `regex.is_full_match` に改名する — ○ 採用

- 内容: `regex.is_full_match(pat: String, s: String) -> Bool` を追加し、`regex.full_match` に `@deprecated(since=15, use="regex.is_full_match")` を付けます。`[[deprecation]]` ブロックで `removal_epoch = 16`。
- 根拠の L0 規則: R-L0-09（L0 が名指ししている例外）、R-L0-14
- (a) 綴りは割れています（Python `fullmatch`、Kotlin `matches`、Swift `wholeMatch`）。Almide 内の `regex.is_match` と並べると `is_full_match` が自然に読めます。
- (b) 影響なし。同じ実行時の本体を呼びます。
- (c) `is_match` と `full_match` の対は、片方だけ `is_` がないので、書き手が `regex.is_full_match` と書いて E002 になる形でした。
- (d) 改名。`almide fix` が 1 対 1 で書き換えます。使用数: spec 69 / tests 1 / docs 7 / stdlib 0。
- ゲート: R-L0-09 のゲートの許容リストから `regex.full_match` を外します。
- issue: #3940

### R-regex-02 コンパイル済みの `Regex` 型を入れ、不正なパターンを作る時点で止める — ○ 採用

- 内容:
  1. 不透明な型 `Regex` を足します。作り方は 2 つです。
     - `regex.new(pat: String) -> Regex`: パターンが文字列リテラルなら `almide check` の段階で構文を検査し、不正なら新しい E コードで止めます（対応していない構文、例えば名前つきグループも「対応していない」と言って止めます）。実行時に組み立てた不正なパターンは、呼び出し側の契約違反として abort します（R-L0-06 の 3。Go の `regexp.MustCompile` と同じ形）。
     - `regex.parse(pat: String) -> Result[Regex, String]`: 実行時に組み立てたパターンを、理由つきで確かめながら作ります（R-L0-06 の 1。Go の `regexp.Compile`、Rust の `Regex::new`、Gleam の `regexp.from_string`、Elixir の `Regex.compile`）。
  2. 一致・検索・置換・分割の関数は、第 1 引数に `Regex` を取ります（`regex.is_match(re: Regex, s: String) -> Bool` など）。名前は変えません。
  3. abort のメッセージは `Error: invalid regex: <理由>` の 1 行、終了コード 1 で、native と wasm で同じにします（新しい契約 C-NNN と fixture。als の台帳を先に通します）。
- 根拠の L0 規則: R-L0-00（9/9 が正規表現の型を持つ）、R-L0-02（主語＝`Regex`）、R-L0-03、R-L0-06、R-L0-14
- (a) 9 言語すべてが、コンパイル済みの正規表現の型を持っています（Rust `Regex`、Go `*regexp.Regexp`、Python `re.Pattern`、JS `RegExp`、Gleam `Regexp`、Elixir `Regex`（`~r` 印）、OCaml `Str.regexp`、Kotlin `Regex`、Swift `Regex`）。文字列のパターンをそのまま関数に渡せるのは Python の `re` のモジュール関数だけで、その Python にも `re.compile` があります。
- (b) エンジンは純粋な Almide（`stdlib/regex_engine.almd`）なので、`Regex` はコンパイル済みの状態を持つ値として両 target に同じ形で乗ります。host の機能は要りません。
- (c) 実測した 2 つの「黙って間違える」箇所が、どちらも型で止まります。
  1. 不正なパターン（`regex.is_match("(", "abc")` が `true`、`(?i)ABC` が一致しないなど）は、`regex.new` の時点でエラーになります。
  2. `|>` でパイプしたときの取り違え（`"order 42" |> regex.is_match("[0-9]+")` が `false`）は、`String` を `Regex` の位置に渡す型エラーになります。宣言に印を付けて検査器に読ませる仕組み（旧 R-regex-03 案）は要りません。
- (d) 引数の型の変更です。旧い呼び出し `regex.find("[0-9]+", s)` は、黙って別の意味になるのではなく、型エラーになります。ヒントは `regex.find(regex.new("[0-9]+"), s)` です（R-L0-14 の 4 が禁じるのは「型が合ったまま意味が変わる」変更で、これは当てはまりません）。`almide fix` は、文字列リテラルのパターンを `regex.new(...)` で包む書き換えを機械的に行います。`String` を `Regex` に自動で変換する仕組みは入れません（書き方が 2 つになるため）。epoch 15 の `breaks` に入れ、spec（約 500 か所）・tests・下流の件数を `summary` に書きます。
- 速さ: いまは呼び出しのたびにパターンを解析しています。`let re = regex.new(p)` を一度作ってループで使えば、解析が 1 回で済みます。
- ゲート: 不正なパターンの行列 fixture（`(`、`[`、`*`、`a{`、`(?P<n>a)`）で、check 時のエラー、`regex.new` の abort、`regex.parse` の err の 3 つを確かめます。native と wasm の一致は `spec/wasm_cross/` の fixture で確かめます。`tests/diagnostics/` に、文字列をパターンの位置へパイプした broken と、`regex.new` を使う fixed を置きます。
- issue: #3942

### R-regex-03 パイプの取り違えを、宣言の印で止める — × 不採用（R-regex-02 の型で止まるため）

- R-regex-02 で第 1 引数が `Regex` 型になるので、`s |> regex.find(re)` の形の取り違えは型エラーになります。引数に「パイプ禁止」の印を付けて検査器に読ませる新しい言語の仕組みは、足しません（同じ取り違えは、型を分ければ他のモジュールでも止められます）。

### R-regex-04 エスケープ・位置・全一致のグループ・大文字小文字の区別なしを足す — ○ 採用

- 内容:
  - `regex.escape(s: String) -> String`: 特殊文字の前に `\` を付けます。
  - `regex.index_of(re: Regex, s: String) -> Option[Int]`: 最初の一致の開始位置（コードポイントの添字）。`string.index_of` と同じ綴り・同じ単位です。
  - `regex.captures_all(re: Regex, s: String) -> List[List[String]]`: 一致ごとの `[全体, グループ1, ...]`。
  - パターンの先頭の `(?i)` を、大文字小文字を区別しない印として受け付けます（関数は増やしません）。
- 根拠の L0 規則: R-L0-01（`index_of` は string と同じ綴り）、R-L0-03、R-L0-12（位置の単位）
- (a) エスケープ 7/9、位置 8/9、全一致のグループ 8/9、大文字小文字 9/9（多くは (F)）。
- (b) 純粋な計算です。
- (c) `regex.escape` がないと、ユーザーの入力を埋め込んだパターンが黙って別の意味になります。
- (d) 追加。使用数 0。
- ゲート: `captures_all` と `captures`・`find_all` の整合を性質テストで確かめます。`index_of` は R-L0-12 の境界入力の行列に入れます。
- issue: #3941

### R-regex-05 一致ごとに関数で置換文字列を作る `regex.replace_with` — ○ 採用

- 内容: `regex.replace_with(re: Regex, s: String, f: (List[String]) -> String) -> String`。`f` は `[全体, グループ1, ...]` を受け取ります。`replace` と同じく、すべての一致を置き換えます。
- 根拠の L0 規則: R-L0-10（`_with` = 値の代わりに関数を受け取る版）
- (a) 9/9 言語が持ちます（Rust `replace_all(closure)`、Go `ReplaceAllStringFunc`、Python `sub(fn)`、JS `replace(fn)`、Elixir `replace(fn)`、Kotlin `replace(transform)` など）。
- (b) 純粋な計算です。
- (d) 追加。
- 置換文字列の中の `$1` を解釈するようにはしません（× 不採用）。いまの `regex.replace` は `$1` を文字そのままとして扱います（probe: `replace("(a)", "xax", "$1$1")` → `x$1$1x`）。解釈を変えると `$` を含む既存の置換が黙って別の結果になり、R-L0-14 の 4 に反します。グループを使う置換は `replace_with` で書きます。
- issue: #3941

### R-regex-06 名前つきグループを足さない — × 不採用

- 内容: `(?<name>...)` と、名前で取り出す API は足しません。
- (a) 7/9 が持ちますが、構文が割れています（Python は `(?P<n>)` だけ (F)、JS は `(?<n>)`）。
- (c) 位置で取り出す `captures` と同じ役割に 2 つ目の形ができます（R-L0-01）。名前つきの構文を書いた場合は、R-regex-02 で `regex.new` が「対応していない」と止めるので、黙って間違えることはありません。
- issue: なし

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `regex.is_match` | `is_match(pat: String, s: String) -> Bool` | Rust is_match / Go MatchString / Python search / JS test / Kotlin containsMatchIn | 意味変更（第 1 引数を `Regex` に） | R-regex-02 |
| `regex.full_match` | `full_match(pat: String, s: String) -> Bool` | Python fullmatch / Kotlin matches / Swift wholeMatch | 改名（`is_full_match`、第 1 引数は `Regex`） | R-regex-01、R-regex-02 |
| `regex.find` | `find(pat: String, s: String) -> Option[String]` | Rust find / Go FindString / Kotlin find / Swift firstMatch | 意味変更（第 1 引数を `Regex` に） | R-regex-02 |
| `regex.find_all` | `find_all(pat: String, s: String) -> List[String]` | Python findall / Go FindAllString / Kotlin findAll / Elixir scan | 意味変更（第 1 引数を `Regex` に） | R-regex-02 |
| `regex.replace` | `replace(pat: String, s: String, rep: String) -> String` | Rust replace_all / Go ReplaceAllLiteralString / JS replaceAll / Kotlin replace | 意味変更（第 1 引数を `Regex` に） | R-regex-02、R-regex-05（`$1` は解釈しない） |
| `regex.replace_first` | `replace_first(pat: String, s: String, rep: String) -> String` | Rust replace / Kotlin replaceFirst / OCaml replace_first | 意味変更（第 1 引数を `Regex` に） | R-regex-02 |
| `regex.split` | `split(pat: String, s: String) -> List[String]` | 全言語 split | 意味変更（第 1 引数を `Regex` に） | R-regex-02 |
| `regex.captures` | `captures(pat: String, s: String) -> Option[List[String]]` | Rust captures / Go FindStringSubmatch / JS exec / Elixir run | 意味変更（第 1 引数を `Regex` に） | 注: 一致しなかった省略可能なグループは `""`（probe: `captures("(a)(b)?", "xa")` → `["a", "a", ""]`）。Go・Elixir と同じ扱いで、変えると意味の付け替えになるので据え置き |
| `regex.is_full_match` | `is_full_match(re: Regex, s: String) -> Bool` | （上の full_match と同じ） | 追加 | R-regex-01 |
| `regex.new` | `new(pat: String) -> Regex` | Go MustCompile / Rust Regex::new / Kotlin Regex() / JS RegExp / Swift Regex | 追加 | R-regex-02 |
| `regex.parse` | `parse(pat: String) -> Result[Regex, String]` | Go Compile / Rust Regex::new / Gleam from_string / Elixir compile | 追加 | R-regex-02 |
| `regex.replace_with` | `replace_with(re: Regex, s: String, f: (List[String]) -> String) -> String` | Rust replace_all(closure) / Go ReplaceAllStringFunc / Python sub(fn) / JS replace(fn) | 追加 | R-regex-05 |
| `regex.escape` | `escape(s: String) -> String` | Rust escape / Go QuoteMeta / Python escape / Elixir escape / OCaml quote | 追加 | R-regex-04 |
| `regex.index_of` | `index_of(re: Regex, s: String) -> Option[Int]` | Go FindStringIndex / JS search / Rust find().start() | 追加 | R-regex-04 |
| `regex.captures_all` | `captures_all(re: Regex, s: String) -> List[List[String]]` | Rust captures_iter / Go FindAllStringSubmatch / Elixir scan | 追加 | R-regex-04 |

（現行 8 関数 + 追加 7 行。追加した関数の第 1 引数はすべて `Regex` です）

R-L0-05 との関係: regex の関数はすべて値を返す純粋な関数で、文として書く誤用はまれです。特記事項はありません。

## L0 への差し戻し（処理済み）

- 「1 引数の関数で値を作る」接尾辞: R-L0-10 で `_with` を「値の代わりに関数を受け取る版」と定めたので、`regex.replace_with` にしました（R-regex-05）。
- パイプの取り違えを宣言の印で止める仕組み: 足しません。型を分けて止めます（R-regex-02・R-regex-03）。
