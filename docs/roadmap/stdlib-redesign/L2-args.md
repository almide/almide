<!-- description: Stdlib redesign L2/L3 — args: a minimal flag reader over env.args; has_flag replaces flag (Bool naming and the first-letter collision), raw and option_or retire -->
# L2/L3: args

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-25: argv 本体は `env`、フラグの読み取りは `args`。§3「フラグ解析の本格版 ×」）
> 出典: Rust https://doc.rust-lang.org/std/env/fn.args.html （フラグ解析は標準に無い）・ Go https://pkg.go.dev/flag ・ Python https://docs.python.org/3/library/argparse.html ・ TS/JS https://nodejs.org/api/util.html#utilparseargsconfig ・ Gleam https://gleam-stdlib.hexdocs.pm/ （無い）・ Elixir https://hexdocs.pm/elixir/OptionParser.html ・ OCaml https://ocaml.org/manual/latest/api/Arg.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/ （無い）・ Swift https://github.com/apple/swift-argument-parser （公式 pkg）
> 9 言語の事実は [facts/categories.md §3.4](facts/categories.md) から。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 生の argv | `args.raw`・`env.args` | `env::args` | `os.Args` | `sys.argv` | `process.argv` | — | `System.argv` | `Sys.argv` | `main(args)` | `CommandLine.arguments` |
| Bool のフラグ | `args.flag` | — | `flag.Bool` | `store_true` | `parseArgs` `type: 'boolean'` | — | `OptionParser` `:boolean` | `Arg.Set` | — | `@Flag` |
| 値を取るオプション | `args.option`・`args.option_or` | — | `flag.String` | `add_argument("--x")` | `parseArgs` `type: 'string'` | — | `OptionParser` `:string` | `Arg.String` | — | `@Option` |
| 位置引数 | `args.positional`・`args.positional_at` | — | `flag.Args`・`flag.Arg(i)` | `add_argument("x")` | `parseArgs` `positionals` | — | `OptionParser` の残り | `Arg.parse` の anon_fun | — | `@Argument` |
| ヘルプ文の生成 | — | — | ● | ● | — | — | — | ● | — | ● |

### 完全性の規則

- `args` は「スキーマを持たない最小の読み取り器」です（L1 §3 で本格版は ×）。役割は {Bool のフラグ, 値のオプション, 位置引数（全部・i 番目）} の 4 つに閉じます。
- 生の argv は `env.args` だけが持ちます（R-L1-25）。
- 既定値版は置きません。`args.option(n)! ?? d` で書きます（R-L0-06）。
- 意図して置かないもの: ヘルプ文の生成、型つきの値（`Int` への変換は `int.parse`）。

### L2 の判定

- 重複: `args.raw` は `env.args()` を 1 回呼ぶだけの関数（R-L0-01 の転送。同じ役割に数える）→ R-args-02
- 規則違反: `args.flag` は Bool を返すのに `is_`/`has_` でない（R-L0-09 の名指しの例外）→ R-args-01
- 規則違反: `args.option_or` は `_or` 版（R-L0-06）→ R-args-02
- 文書の食い違い: `docs/stdlib/args.md` は `args.raw` を「argv[0] included」と書くが、実測は除く（`raw=-v,--out,x,in.txt`、native と wasm の両方。probe-args）。撤去の窓の間に直します → R-args-02

## 案

### R-args-01 `args.flag` を `args.has_flag` にし、頭文字の自動の短縮形をやめる — ○ 採用

- 内容: `effect fn has_flag(name: String) -> Bool` を足します。`--name` があれば true。`name` が 1 文字なら `-n` も true。頭文字から短い形を作りません。`args.flag` は撤去します。
- 実測（probe-args）: `almide run a.almd -- -v` で、`args.flag("verbose")` と `args.flag("version")` の両方が true になります。`-v` が 2 つの名前に当たります。
- 根拠の L0 規則: R-L0-09（Bool は `is_`/`has_`。`args.flag` は L0 が名指しした例外）。意味も変えるので、R-L0-14 の 4（意味を変えるときは新しい名前）に合わせて新しい名前に移します。
- (a) 長い名前から短い形を自動で作る言語はありません。Go の `flag` は `-name` と `--name` を同じに扱い、短い形は別に定義します。Python・Node・Elixir も短い形は明示（`-v` を `alias`/`short` で指定）です。
- (b) `env.args` の上の純粋な計算です。native と wasm で同じです（`wasi:cli/environment`）。
- (c) `has_flag` は名前から Bool が読めます。短い形を `args.has_flag("verbose") or args.has_flag("v")` と書く形は、どの言語の癖とも衝突しません。頭文字の衝突は、LLM がテストを書かない限り気づけない誤りです。
- (d) 改名と意味変更。互換: `args.flag` に `@deprecated(since=15, use="args.has_flag")`、`removal_epoch = 16`。`almide fix` の書き換えは `args.flag("verbose")` → `(args.has_flag("verbose")! or args.has_flag("v")!)`（いまの意味をそのまま保つ形）。使用数 7/9/13/0。
- ゲート案: R-L0-G の Bool 名の検査で、許容リストから `args.flag` を外します。頭文字の衝突の fixture（`-v` で `has_flag("version")` が false）を置きます。
- issue: #3893

### R-args-02 `args.raw` と `args.option_or` を撤去する — ○ 採用

- 内容: `args.raw()` → `env.args()`。`args.option_or(n, d)` → `args.option(n)! ?? d`。撤去までの間に `docs/stdlib/args.md` の「argv[0] included」を「excluded」に直します。
- 根拠の L0 規則: R-L0-01（ほかの公開関数を 1 回呼ぶだけの転送も同じ役割に数える）、R-L0-06（`_or` 版は `??` に一本化）、R-L1-25。
- (a) フラグ解析のモジュールが生の argv も返すのは Almide だけです（facts §3.4）。既定値の引数は Python の `default=` などがありますが、関数を分ける言語はありません。
- (b) 変化なし。
- (c) `args.raw` は README と args.md で説明が食い違っており、どちらを読んだ LLM も半分は外れます。
- (d) 撤去。`@deprecated(since=15, use="env.args")` と `use="args.option(name)! ?? fallback"`、`removal_epoch = 16`。`almide fix` は両方を書き換えます。`option_or` の書き換え先の形（`!` が要るか）は R-L0-17 で決まる呼び出しの型に合わせます。使用数: `args.raw` 1/4/11/0、`args.option_or` 6/4/8/0。
- ゲート案: 同義語ゲート規則 5（#3851 F）。`_or` で終わる公開関数を R-L0-G で拒否します。
- issue: #3851 B（raw）、R-L0-06 の新規 issue（option_or）

### R-args-03 `positional_at` を撤去する — ○ 採用

- いま: `args.positional_at(i)` は `list.get(args.positional()!, i)` と同じ値です。R-args-02 は同じ理由（ほかの関数の組み合わせで書ける）で `option_or` を撤去しました。
- 案: 撤去し、`list.get(args.positional()!, i)` と書きます。
- 根拠の L0 規則: R-L0-01（同じ役割を 2 か所に置かない）。
- (d) 撤去 1。`@deprecated(since=15, use="list.get(args.positional()!, i)")`、`removal_epoch = 16`、`almide fix` で書き換えます。
- issue: #3979

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `args.raw` | `effect fn raw() -> List[String]` | Rust `env::args` / Go `os.Args` / Python `sys.argv` | 撤去（→ `env.args`） | R-args-02 |
| `args.flag` | `effect fn flag(name: String) -> Bool` | Go `flag.Bool` / Python `store_true` / Elixir `:boolean` | 撤去（→ `has_flag`） | R-args-01 |
| `args.has_flag` | `effect fn has_flag(name: String) -> Bool`（案） | 同上 | 追加 | R-args-01 |
| `args.option` | `effect fn option(name: String) -> String?` | Go `flag.String` / Python `add_argument("--x")` / Node `parseArgs` | 据え置き | — |
| `args.option_or` | `effect fn option_or(name: String, fallback: String) -> String` | Python `default=` | 撤去（→ `option(n)! ?? d`） | R-args-02 |
| `args.positional` | `effect fn positional() -> List[String]` | Go `flag.Args` / Node `positionals` | 据え置き（オプションの値も入ることを文書に明記。実測 `--out x` の `x` が入る） | — |
| `args.positional_at` | `effect fn positional_at(i: Int) -> String?` | Go `flag.Arg(i)` | 撤去（→ `list.get(args.positional()!, i)`） | R-args-03 |

## L0 への差し戻し（処理済み）

- `args.option` と `env.get` は同じ `effect fn … -> String?` なのに、`??` の書き方が違います: L0 の R-L0-17（呼び出しの型は宣言だけで決まる）で決まりました。2 つは宣言が同じなので、同じ呼び方になります。どちらの形にそろえるかは R-L0-17 の issue（新規）で設計し、このページの関数の名前と宣言は変えません（L2-env R-env-03）。
