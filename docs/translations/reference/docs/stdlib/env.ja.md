# env

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](env.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](env.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](env.zh-CN.md)



> 原文：[env.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/env.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

> 翻訳上の注意：秘密値のログ出力・定時間比較については原文の説明を訳したもので、この翻訳による独立した安全保証ではありません。`secret.reveal` で取り出した値まで保護するとの主張ではありません。OS 名はホスト／実行方式に依存します。固定原文の本文と API コメントには Wasm の OS 名に関する説明範囲の差があり、戻り型の見出しと effect 署名にも表記差があります。この訳では独断で統一していません。

環境とシステム。import env、effect。

### `env.args() -> List[String]`

コマンドライン引数を文字列リストとして取得します。すべてのターゲットで `argv[1..]` をそのまま返します。プログラム引数内の `--` はプログラムのものです（`./p a -- b` は `["a", "--", "b"]` を受け取ります）。`almide run app.almd -- a -- b` は自身の区切りだけを消費し、残りを変更せず転送します。

```almd check
import env

effect fn main() -> Unit = {
  let args = env.args()
  println("${args}")
}
```

### `env.get(name: String) -> Option[String]`

環境変数の値を取得します。未設定なら none です。

```almd check
import env

effect fn main() -> Unit = {
  let home = env.get("HOME") // => some("/Users/alice")
  println(home ?? "unset")
}
```

### `env.secret(name: String) -> Option[Secret[String]]`

認証情報（API トークン、webhook キー）を持つ環境変数を読みます。値は `env.get` と同じですが、最初から `Secret[String]` です（`import secret`）。どこでも `Secret(***)` と表示され、`==` は定時間比較（C-426）なので、トークンはログへ出ず、情報を漏らす `==` で比較されません。`secret.reveal` で中身を読みます。未設定なら none です（C-438）。

```almd check
import env
import secret

effect fn main() -> Unit = {
  match env.secret("API_TOKEN")! {
    some(token) => println("${token} ${token == secret.new("expected")}"),
    none => println("API_TOKEN is not set"),
  }
}
```

### `env.set(name: String, value: String) -> Unit`

環境変数を設定します。名前が空、名前に `=` または NUL が含まれる、値に NUL が含まれる場合は、エラーとなり何も設定しません。これらはプロセス環境に保持できない組合せです（C-421）：`env.set: empty name`、`env.set: name contains '='`、`env.set: name contains NUL`、`env.set: value contains NUL`。

```almd check
import env

effect fn main() -> Unit = {
  env.set("MY_VAR", "hello")!
  println(env.get("MY_VAR") ?? "unset")
}
```

### `env.remove(name: String) -> Result[Unit, String]`

このプロセスと、以後そこから起動する子プロセスの環境変数を解除します。その後 `env.get` は none を返し、`env.vars` はその名前を含めません。未設定の名前を削除しても成功します。名前は `env.set` の規則（C-421）に従い、自身の関数名で同じエラーを返します：`env.remove: empty name`、`env.remove: name contains '='`、`env.remove: name contains NUL`。

```almd check
import env

effect fn main() -> Unit = {
  env.set("MY_VAR", "hello")!
  env.remove("MY_VAR")!
  println(env.get("MY_VAR") ?? "unset") // => unset
}
```

### `env.vars() -> Map[String, String]`

このプロセスのすべての環境変数を、名前をキーとし名前順で返します。プログラム自身の `env.set` / `env.remove` 呼び出しも反映されます。秘密を含め、値は平文です。認証情報を読む場合は環境全体を列挙せず、`env.secret` で名前を指定してください。呼び出しは `env.vars()!` と書きます。現在、Almide の本体を持つすべての標準ライブラリ effect fn が使う呼び出し形式です（R-env-03）。

```almd check
import env

effect fn main() -> Unit = {
  env.set("MY_VAR", "hello")!
  let all = env.vars()!
  println(all["MY_VAR"] ?? "unset") // => hello
}
```

### `env.cwd() -> Result[String, String]`

現在の作業ディレクトリーを取得します。

```almd check
import env

effect fn main() -> Unit = {
  let dir = env.cwd()!
  println(dir)
}
```

### `env.sleep_ms(ms: Int) -> Unit`

指定ミリ秒だけスリープします。0または負の期間は経過済みとして、すべてのターゲットですぐ戻ります。期限は別です。`process.ProcessCommand` の `timeout_ms` が `0` なら、期限自体がないことを意味します。

```almd check
import env

effect fn main() -> Unit = {
  env.sleep_ms(1000) // sleep 1 second
  println("awake")
}
```

### `env.os() -> String`

ホストが報告する OS 名を取得します（ネイティブランタイムでは `linux`、`macos`、`windows`、`unknown`）。埋め込み Wasm ホストもホスト OS を報告します。stock-WASI アダプテーションでは `wasi` を返します。

```almd check
import env

effect fn main() -> Unit = {
  println(env.os()) // => "macos"
}
```



## 固定スナップショットの API 索引（9関数）

```
// Program arguments, argv[0] excluded.
// @since 0.5.0 or earlier
effect env.args() -> List[String]

// Value of name, or none if unset.
// @since 0.5.0 or earlier
effect env.get(name: String) -> Option[String]

// Value of name as a Secret (prints Secret(***), constant-time ==), or none if unset. For a token or key.
// @since unreleased
effect env.secret(name: String) -> Option[Secret[String]]

// Sets name for this process and later spawns; err if name is empty or holds `=` or NUL, or value holds NUL.
// @since 0.5.0 or earlier
effect env.set(name: String, value: String) -> Unit

// Removes name for this process and later spawns; ok if unset; err if name is empty or holds `=` or NUL.
// @since unreleased
effect env.remove(name: String) -> Unit

// Every environment variable of this process, keyed and ordered by name.
// Holds secrets as plain values: read a credential by name with `env.secret` instead.
// @since unreleased
effect env.vars() -> Map[String, String]

// Absolute current directory; err if unreadable.
// @since 0.5.0 or earlier
effect env.cwd() -> String

// Blocks ms milliseconds; zero or negative ms returns without waiting.
// @since 0.5.0 or earlier
effect env.sleep_ms(ms: Int) -> Unit

// Host OS: macos, linux, windows or unknown; "wasi" on wasm (C-189).
// @since 0.5.0 or earlier
env.os() -> String
```




## API 説明の訳（上記署名順）

- `effect env.args() -> List[String]` — プログラム引数。argv[0] は除外。
- `effect env.get(name: String) -> Option[String]` — name の値。未設定なら none。
- `effect env.secret(name: String) -> Option[Secret[String]]` — name の値を Secret として返します（`Secret(***)` と表示、定時間 ==）。未設定なら none。トークンやキー用。
- `effect env.set(name: String, value: String) -> Unit` — このプロセスと以後の子プロセスに name を設定。名前が空、`=`/NUL を含む、値が NUL を含む場合は err。
- `effect env.remove(name: String) -> Unit` — このプロセスと以後の子プロセスから name を削除。未設定なら成功。名前が空、`=`/NUL を含む場合は err。
- `effect env.vars() -> Map[String, String]` — このプロセスの全環境変数。名前をキーとし名前順。秘密も通常の値として含むため、認証情報は `env.secret` で名前を指定して読みます。
- `effect env.cwd() -> String` — 絶対パスの現在ディレクトリー。読めない場合は err。
- `effect env.sleep_ms(ms: Int) -> Unit` — ms ミリ秒ブロック。0または負なら待たずに戻ります。
- `env.os() -> String` — ホスト OS：macos、linux、windows、unknown。wasm では "wasi"（C-189）。
