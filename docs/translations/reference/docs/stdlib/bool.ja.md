# bool

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bool.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](bool.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](bool.zh-CN.md)



> 原文：[bool.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/bool.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

`Bool` モジュール：真偽値の読み取り、順序付け、表示。自動インポートされます（`import` 不要）。

純粋で、ケイパビリティは不要、両ターゲットで同じです。`bool.to_string` と `bool.parse` は互いの逆操作です。`bool.compare` は `Bool` に対する `<` の関数形式であり、`false < true` です。

### `bool.parse(s: String) -> Result[Bool, String]`

完全に一致する `"true"` と `"false"` だけを受け入れます。`"True"` や `" true"` を含むその他のテキストは `err` です。

```almd run
fn main() -> Unit = {
  println("${bool.parse("true") ?? false} ${bool.parse("yes") ?? false}")
}
```
```output
true false
```

### `bool.compare(a: Bool, b: Bool) -> Int`

```almd run
fn main() -> Unit = {
  println("${bool.compare(false, true)} ${bool.compare(true, true)} ${bool.compare(true, false)}")
}
```
```output
-1 0 1
```



<a id="signature-index-3-functions"></a>

## 固定スナップショットの API 索引（3関数）

```
// "true" or "false"; bool.parse reads it back.
// @since unreleased
bool.to_string(b: Bool) -> String

// ok(true) for "true", ok(false) for "false", err for anything else.
// @since unreleased
bool.parse(s: String) -> Result[Bool, String]

// Negative if a < b, 0 if equal, positive if a > b; false < true.
// @since unreleased
bool.compare(a: Bool, b: Bool) -> Int
```




## API 説明の訳（上記署名順）

- `bool.to_string(b: Bool) -> String` — "true" または "false"。bool.parse で読み戻せます。
- `bool.parse(s: String) -> Result[Bool, String]` — "true" なら ok(true)、"false" なら ok(false)、それ以外は err。
- `bool.compare(a: Bool, b: Bool) -> Int` — a < b なら負、等しければ0、a > b なら正。false < true。
