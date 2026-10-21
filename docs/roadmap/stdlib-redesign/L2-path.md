<!-- description: Stdlib redesign L2/L3 — path: pure lexical path text; keep the ten functions, add components and relative_to (roles 6/9 languages have) -->
# L2/L3: path

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)
> 出典: Rust https://doc.rust-lang.org/std/path/struct.Path.html ・ Go https://pkg.go.dev/path/filepath ・ Python https://docs.python.org/3/library/pathlib.html ・ TS/JS https://nodejs.org/api/path.html ・ Gleam https://gleam-stdlib.hexdocs.pm/ （無い）・ Elixir https://hexdocs.pm/elixir/Path.html ・ OCaml https://ocaml.org/manual/latest/api/Filename.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.io.path/ ・ Swift https://github.com/apple/swift-system （`FilePath`）

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| つなぐ | `join` | `join` | `Join` | `/`・`joinpath` | `join` | — | `join` | `concat` | `resolve` | `appending` |
| 親 | `dirname` | `parent` | `Dir` | `parent` | `dirname` | — | `dirname` | `dirname` | `parent` | `removingLastComponent` |
| 最後の名前 | `basename` | `file_name` | `Base` | `name` | `basename` | — | `basename` | `basename` | `name` | `lastComponent` |
| 拡張子 | `extension` | `extension` | `Ext` | `suffix` | `extname` | — | `extname` | `extension` | `extension` | `extension` |
| 拡張子を除いた名前 | `stem` | `file_stem` | — | `stem` | — | — | `rootname`+`basename` | `remove_extension` | `nameWithoutExtension` | `stem` |
| 絶対か | `is_absolute` | `is_absolute` | `IsAbs` | `is_absolute` | `isAbsolute` | — | `type` | `is_relative` | `isAbsolute` | `isAbsolute` |
| 正規化（字句） | `normalize` | — | `Clean` | `normpath` | `normalize` | — | `expand`（cwd も使う） | — | `normalize` | `lexicallyNormalized` |
| 要素に分ける | — | `components` | `SplitList`（別の意味）・`strings.Split` | `parts` | `parse` | — | `split` | — | `iterator` | `components` |
| 相対パスにする | — | `strip_prefix` | `Rel` | `relative_to` | `relative` | — | `relative_to` | — | `relativeTo` | — |
| 走査の安全な型 | `SafePath`（`from_string`・`trusted`・`to_string`） | — | `os.Root`（1.24） | — | — | — | — | — | — | — |

### 完全性の規則

- `path` は文字列の上の字句の計算だけです（純粋）。ファイルシステムを見る操作（実パスへの解決、存在）は `fs` が持ちます。
- 分ける・組み立てるの対: {`join` ↔ `components`}、{`dirname`・`basename` ↔ 組み立ては `join`}、{`stem`・`extension`}。
- 失敗の表し方（R-L0-06 の 1 と 4。番兵を返さない）: 無いかもしれない部分は `Option`（`extension`）、理由のある拒否は `Result`（`from_string` の `..`）。`dirname("a")` の `""` は「親が無い」ではなく「空の親」の値なので番兵ではありません（Python と同じ）。
- 順序: `path` の値は `String` です。`SafePath` は順序を持たない型なので、`path.compare` は置きません（R-L0-16 は順序のある型だけが対象）。

### L2 の判定

- 抜けている役割: 要素に分ける（6/9）、相対パスにする（6/9）→ R-path-01
- 重複・規則違反: なし（`is_absolute` は R-L0-09 に合う。`from_string` は R-L0-07 の向き）
- 実測（probe-path）: `normalize("a/../../b/./c")` は `../b/c`、`join("a", "/b")` は `/b`、`extension(".bashrc")` は none、`from_string("a/../b")` は err。文書どおりです。

## 案

### R-path-01 `components` と `relative_to` を足す — ○ 採用

- 内容:
  - `fn components(p: String) -> List[String]` — 空でない要素の並び。絶対パスなら先頭が `"/"`。`path.join` で畳むと `normalize` 済みの同じパスに戻ります。
  - `fn relative_to(p: String, base: String) -> String?` — 字句の上で `base` の下にあれば、その相対パス。下に無ければ none。`..` を作りません。
- 綴り: 要素は `components`（Rust・Swift）、相対は `relative_to`（Python・Elixir、Kotlin `relativeTo`）。どちらも多数派はないので、すでに意味が衝突しない綴りを選びました。`split` は Python の `os.path.split`（2 つに分ける）と意味がぶつかるので使いません。
- 根拠の L0 規則: L1 の判定基準 1（5 言語以上）、R-L0-06（下に無いときは none）、R-L0-02（文字列のパスの関数は path に置く）。
- (a) 要素に分けるのは 6/9、相対パスは 6/9 が持っています。
- (b) 純粋な計算です。native と wasm で同じです。
- (c) 無いと LLM は `string.split(p, "/")` で書き、先頭の `/` や連続する `/` で空の要素を作ります。
- (d) 追加。使用数 0。
- ゲート案: `components` と `join` の往復、`relative_to` と `join` の往復（`join(base, relative_to(p, base)!) == normalize(p)`）を性質のテストにします。
- issue: #3936

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `path.from_string` | `fn from_string(s: String) -> Result[SafePath, String]` | Go `os.OpenRoot` に近い | 据え置き | — |
| `path.trusted` | `fn trusted(s: String) -> SafePath` | — | 据え置き | — |
| `path.to_string` | `fn to_string(p: SafePath) -> String` | Rust `to_str` / Python `str(p)` | 据え置き | — |
| `path.join` | `fn join(base: String, child: String) -> String` | Rust・Node・Elixir `join` / Go `Join` | 据え置き | — |
| `path.dirname` | `fn dirname(p: String) -> String` | Node・Elixir・OCaml `dirname` / Go `Dir` / Rust `parent` | 据え置き | — |
| `path.basename` | `fn basename(p: String) -> String` | Node・Elixir・OCaml `basename` / Go `Base` / Rust `file_name` | 据え置き | — |
| `path.extension` | `fn extension(p: String) -> String?` | Rust・OCaml・Kotlin `extension` / Python `suffix` / Node `extname` | 据え置き | — |
| `path.is_absolute` | `fn is_absolute(p: String) -> Bool` | Rust・Python `is_absolute` / Go `IsAbs` | 据え置き | — |
| `path.stem` | `fn stem(p: String) -> String` | Rust `file_stem` / Python `stem` / Swift `stem` | 据え置き | — |
| `path.normalize` | `fn normalize(p: String) -> String` | Go `Clean` / Python `normpath` / Node `normalize` | 据え置き | — |
| `path.components` | `fn components(p: String) -> List[String]`（案） | Rust `components` / Python `parts` / Elixir `split` | 追加 | R-path-01 |
| `path.relative_to` | `fn relative_to(p: String, base: String) -> String?`（案） | Python・Elixir `relative_to` / Go `Rel` / Node `relative` | 追加 | R-path-01 |
