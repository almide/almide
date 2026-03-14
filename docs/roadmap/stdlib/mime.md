# stdlib: mime [Tier 3]

MIME タイプ判定。Go, Python, Deno に存在。ファイルアップロード、HTTP レスポンス、コンテンツ判定で使う。

## 他言語比較

| 操作 | Go (`mime`) | Python (`mimetypes`) | Deno (`@std/media-types`) |
|------|-----------|---------------------|--------------------------|
| 拡張子→MIME | `mime.TypeByExtension(".html")` | `mimetypes.guess_type("file.html")` | `contentType(".html")` |
| MIME→拡張子 | `mime.ExtensionsByType("text/html")` | `mimetypes.guess_extension("text/html")` | `extension("text/html")` |
| パース | `mime.ParseMediaType(s)` | ❌ | `parseMediaType(s)` |

## 追加候補 (~4 関数)

- `mime.from_extension(ext) -> Option[String]` — `.html` → `text/html`
- `mime.to_extension(mime_type) -> Option[String]` — `text/html` → `.html`
- `mime.parse(s) -> { type: String, params: Map[String, String] }` — Content-Type パース
- `mime.is_text?(mime_type) -> Bool` — テキスト系か判定

## 実装戦略

self-host (.almd)。MIME マッピングテーブルを定数として持つだけ。外部依存不要。
