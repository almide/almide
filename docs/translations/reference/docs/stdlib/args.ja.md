# args

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](args.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](args.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](args.zh-CN.md)



> 原文：[args.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/args.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

`env.args()` を使ったコマンドライン引数の解析。`import args`。

`args` は薄い、割り当ての少ない読み取り機能です。パーサーオブジェクトもスキーマもありません。各呼び出しは任意の順序でプロセス引数を読み直します。

すべての読み取り関数は `effect fn` です（dialect epoch 13）。引数は入力であり、通常の `fn` は入力を読みません。他の effect fn 呼び出しと同様に、`effect fn` から呼んで `!` で伝播させるか、`effect fn main` で一度読み、通常の fn に値を渡します。

プログラムの引数は `--` の後に置きます。

```bash
almide run app.almd -- --verbose --output=out.txt input.csv
```

### `args.option(name: String) -> Option[String]`

`--` より前の `--name` の値です。`--name=value` と `--name value` の両方を受け入れます。フラグがない場合や、その後に値がない場合は `none` です。

```almd check
import args
import fs

effect fn main() -> Unit = {
  let body = "report body"
  match args.option("output")! {
    some(path) => fs.write_text(path, body)!,
    none => println(body),
  }
}
```

### `args.positional(options: List[String]) -> List[String]`

`env.args()`（argv[0] は既に除外済み）のうちフラグでない引数に、`--` より後のすべての引数を続けたものです。`options` は、プログラムが `args.option` で読むすべてのオプション名を指定します。`--name` の後の引数は位置引数ではなくそのオプションの値なので、`--out report.txt input.txt` に `["out"]` を指定すると `["input.txt"]` になります。オプションを読まないプログラムでは `[]` を渡します。

```almd check
import args

fn process(file: String) -> Unit = println("processing ${file}")

effect fn main() -> Unit = {
  let out = args.option("out")! ?? "report.txt"
  for file in args.positional(["out"])! { process(file) }
  println("writing ${out}")
}
```



## 固定スナップショットの API 索引（3関数）

```
// True on --name before `--`; a one-letter name also matches -name. No short form is derived.
// @since unreleased
effect args.has_flag(name: String) -> Bool

// Value of --name=v or --name v before `--`; none if absent.
// @since 0.2.0 or earlier
effect args.option(name: String) -> Option[String]

// Args before `--` that are not flags or values of the named options, then all after `--`.
// @since 0.2.0 or earlier
effect args.positional(options: List[String]) -> List[String]
```




## API 説明の訳（上記署名順）

- `effect args.has_flag(name: String) -> Bool` — `--` より前の --name なら true。1文字名は -name にも一致します。短縮形は自動導出しません。
- `effect args.option(name: String) -> Option[String]` — `--` より前の --name=v または --name v の値。なければ none。
- `effect args.positional(options: List[String]) -> List[String]` — `--` より前の、フラグでも指定オプションの値でもない引数と、`--` より後の全引数。
