# json

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](json.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](json.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](json.zh-CN.md)



> 原文：[json.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/json.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

> 翻訳上の注意：前段パーサーとの解釈差に関する記述は固定原文の説明です。この翻訳がすべてのパーサーの正しさ・相互互換性を検証または証明したという意味ではありません。末尾の Float から Int への変換を「拡張」とする表現も原文どおりで、訳者による仕様の訂正・統一はしていません。

JSON **形式**：`Value` ツリーの解析と出力。import json。

データモデル自体は `value.*`（自動インポート）にあります。コンストラクター（`value.null` / `value.int` / `value.string` / `value.object` / …）、アクセサー（`value.as_*` / `value.get` / `value.as_object`）、パス（`value.root` / `value.key` / `value.get_path`）については [value.md](../../../../stdlib/value.md) を参照してください。これらの古い `json.*` 表記は廃止された別名で、1リリースの E040 非推奨期間後に削除されました。移行対応表は末尾の[名前が変わった操作](#renamed-operations)にあります。

`value.root` / `value.key` / `value.index` で作るパスの型は `value.Path` です。パスを扱うヘルパーにはその型を注釈してください。

### `json.parse(text: String) -> Result[Value, String]`

JSON 文字列を Value に解析します。

```almd run
import json

fn main() -> Unit = {
  let v = json.parse("{\"name\": \"Alice\"}")
  match v {
    ok(j) => println(json.stringify(j)),
    err(e) => println("parse error: ${e}"),
  }
}
```
```output
{"name":"Alice"}
```

文法は RFC 8259 と I-JSON（RFC 7493）で、**厳密に**読みます（ALS-T3、C-400 / C-401）。テキスト全体は JSON 空白（スペース、タブ、LF、CR）に囲まれた1つの値でなければならず、その他の入力はケースごとに1つのエラーテキストを持つ `err` になります。すべてのターゲットで同じです。`N` は問題が始まる文字インデックスです。

| 入力 | エラー |
|---|---|
| `"ad\ud800min"`（上位＋下位のペアではないサロゲート） | `json: lone surrogate at pos N` |
| `"ad\qmin"`（`\" \\ \/ \b \f \n \r \t \u` 以外のエスケープ） | `json: unknown escape at pos N` |
| `"\u12"`（16進4桁が続かない `\u`） | `json: invalid unicode escape at pos N` |
| 文字列内の生の U+0000–U+001F | `json: control character in string at pos N` |
| U+FDD0–U+FDEF、U+xFFFE、U+xFFFF（エスケープでも生でも） | `json: noncharacter at pos N` |
| `"abc` | `json: unterminated string at pos N` |
| `{"a":1,"a":2}` | `json: duplicate key at pos N` |
| `[1,2] [3]`, `{} garbage` | `json: trailing data at pos N` |
| `{x: 1}` | `json: unquoted key at pos N` |
| `[1,]`, `{"a":1,}` | `json: trailing comma at pos N` |
| `{"a" 1}` | `json: expected ':' at pos N` |
| `[1 2]` / `{"a":1 "b":2}` | `json: expected ',' or ']' at pos N` / `json: expected ',' or '}' at pos N` |
| `01`, `1.`, `-`, `1e` | `json: invalid number at pos N` |

何も捨てず、推測もしません。単独サロゲートや未知のエスケープを削除せず（`ad\ud800min` は `admin` になりません）、重複キーも先勝ちや後勝ちで解決しません。そのため、プログラムの前段のパーサーが同じバイトから別の文書を読むことはできません。

```almd run
import json

fn main() -> Unit = {
  for text in ["{\"user\":\"ad\\ud800min\"}", "{\"user\":\"alice\",\"user\":\"admin\"}", "[1,2] [3]"] {
    match json.parse(text) {
      ok(j) => println(json.stringify(j)),
      err(e) => println(e),
    }
  }
}
```
```output
json: lone surrogate at pos 11
json: duplicate key at pos 16
json: trailing data at pos 6
```

### `json.stringify(v: Value) -> String`

Value を JSON 文字列へ変換します。非有限 Float（NaN、+inf、-inf）には JSON 表記がなく、`null` として出力するため、出力は必ず再解析できます。`json.stringify_pretty` も同じ規則です。

```almd run
import json

type Person: Codec = { name: String, age: Int }

fn main() -> Unit = {
  let person = Person { name: "Alice", age: 30 }
  println(json.stringify(person.encode()))
}
```
```output
{"name":"Alice","age":30}
```

### 型付きデコード：`T.decode` と `T.decode_strict`

`: Codec` レコード型 `T` は `T.decode`、`T.decode_strict`（および `T.encode`）を導出します。`T.decode` は未知のキーを無視し、`T.decode_strict` は入れ子のレコードやコンテナーを含め、未知のキーを再帰的に拒否します。各レコードではフィールドのデコード前にキーを検査し、入れ子のエラーには失敗した値へのパスを含めます。`@skip_decode` 付きフィールドは入力から読み取らず（既定値を維持）、エンコードは行います。`@skip` は読み取りも書き込みもしません。信頼できない入力は、クライアントが設定してよいものだけを持つ型へデコードするのが適切です（CHEATSHEET、JSON Rule 5）。

```almd run
import json

type User: Codec = { name: String, @skip_decode is_admin: Bool = false }

fn main() -> Unit = {
  let body = json.parse("{\"name\": \"m\", \"is_admin\": true}") ?? value.null()
  match User.decode(body) {
    ok(u) => println("${u.name} admin=${u.is_admin}"),
    err(e) => println(e),
  }
  match User.decode_strict(body) {
    ok(u) => println("${u.name} admin=${u.is_admin}"),
    err(e) => println(e),
  }
}
```
```output
m admin=false
unknown key 'is_admin'
```

### `json.stringify_pretty(j: Value) -> String`

Json 値をインデント付きの整形済み JSON 文字列に変換します。非有限 Float は `json.stringify` と同様に `null` と書きます。

```almd run
import json

fn main() -> Unit = {
  let j = json.parse("{\"name\": \"Alice\", \"tags\": [\"a\", \"b\"]}") ?? value.null()
  println(json.stringify_pretty(j))
}
```
```output
{
  "name": "Alice",
  "tags": [
    "a",
    "b"
  ]
}
```



## 固定スナップショットの API 索引（3関数）

```
// Strict JSON Value or err; trailing non-whitespace text and nesting over 1000 are errors.
// @since 0.5.0 or earlier
json.parse(text: String) -> Result[Value, String]

// Compact JSON; a Float 1.0 prints as 1, NaN/inf as null.
// @since 0.5.0 or earlier
json.stringify(v: Value) -> String

// JSON with 2-space indent; [] and {} inline, NaN/inf null.
// @since 0.5.0 or earlier
json.stringify_pretty(v: Value) -> String
```



<a id="renamed-operations"></a>

## 名前が変わった操作

1リリースの非推奨期間（E040、v0.53.1 – v0.53.4）後、#1078 で削除されました。廃止表記は現在、通常の E002 未定義関数エラーです。

| 廃止 | 現行 |
|---|---|
| `json.null` / `json.object` / `json.array` | `value.null` / `value.object` / `value.array` |
| `json.from_string` / `json.from_int` / `json.from_bool` / `json.from_float` | `value.string` / `value.int` / `value.bool` / `value.float` |
| `json.as_string` … `json.as_array`（`Option` を返していた） | `value.as_string(v)?` … `value.as_array(v)?` — `?` は `Result` を同じ `Option` に変換 |
| `json.get(j, k)`（`Option` を返していた） | `value.get(j, k)` |

動作が厳格化された点があります。削除された `json.as_int` は Float 値を Int に拡張していましたが、`value.as_int` は行いません（`err("expected Int")`）。拡張方向は、Int を受け入れる `value.as_float` にあります。


## API 説明の訳（上記署名順）

- `json.parse(text: String) -> Result[Value, String]` — 厳密な JSON Value または err。末尾の空白以外のテキストと1000を超える入れ子はエラー。
- `json.stringify(v: Value) -> String` — コンパクトな JSON。Float 1.0 は1、NaN/inf は null と表示。
- `json.stringify_pretty(v: Value) -> String` — 2スペースインデントの JSON。[] と {} はインライン、NaN/inf は null。
