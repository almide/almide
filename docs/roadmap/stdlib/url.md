<!-- description: URL parsing, construction, and query parameter manipulation -->
# stdlib: url [Tier 2]

URL のパース・構築・クエリパラメータ操作。現在 Almide にはない。

## 他言語比較

### パース

| 言語 | 関数 | 戻り値 |
|------|------|--------|
| Go | `url.Parse(s)` | `(*URL, error)` |
| Python | `urllib.parse.urlparse(s)` | `ParseResult` (named tuple) |
| Rust | `Url::parse(s)` | `Result<Url, ParseError>` |
| Deno | `new URL(s)` | `URL` (throws) |

### コンポーネントアクセス

| 要素 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| scheme | `.Scheme` | `.scheme` | `.scheme()` | `.protocol` |
| host | `.Host` | `.hostname` | `.host_str()` | `.hostname` |
| port | `.Port()` | `.port` | `.port()` | `.port` |
| path | `.Path` | `.path` | `.path()` | `.pathname` |
| query | `.RawQuery` | `.query` | `.query()` | `.search` |
| fragment | `.Fragment` | `.fragment` | `.fragment()` | `.hash` |
| username | `.User.Username()` | `.username` | `.username()` | `.username` |
| password | `.User.Password()` | `.password` | `.password()` | `.password` |

### クエリパラメータ

| 操作 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| parse | `url.ParseQuery(q)` | `parse_qs(q)` | `url.query_pairs()` | `url.searchParams` |
| get | `values.Get("k")` | `parse_qs(q)["k"]` | iterate pairs | `searchParams.get("k")` |
| set | `values.Set("k", "v")` | rebuild | `query_pairs_mut().append_pair()` | `searchParams.set("k", "v")` |
| delete | `values.Del("k")` | rebuild | clear+re-add | `searchParams.delete("k")` |

### エンコード/デコード

| 操作 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| encode (query) | `url.QueryEscape(s)` | `quote_plus(s)` | `form_urlencoded::Serializer` | `encodeURIComponent(s)` |
| decode (query) | `url.QueryUnescape(s)` | `unquote_plus(s)` | `form_urlencoded::parse` | `decodeURIComponent(s)` |
| encode (path) | `url.PathEscape(s)` | `quote(s)` | `utf8_percent_encode` | `encodeURI(s)` |
| decode (path) | `url.PathUnescape(s)` | `unquote(s)` | `percent_decode_str` | `decodeURI(s)` |

### 相対 URL 解決

| 言語 | 関数 |
|------|------|
| Go | `base.ResolveReference(ref)` |
| Python | `urljoin(base, url)` |
| Rust | `base_url.join(relative)` |
| Deno | `new URL(relative, base)` |

## 追加候補 (~12 関数)

### P0 (基本)
- `url.parse(s) -> Result[Url, String]` — URL パース
- `url.to_string(u) -> String` — URL → 文字列
- `url.scheme(u) -> String`
- `url.host(u) -> String`
- `url.port(u) -> Option[Int]`
- `url.path(u) -> String`
- `url.query(u) -> Option[String]`
- `url.fragment(u) -> Option[String]`

### P1 (クエリ)
- `url.query_params(u) -> Map[String, String]` — クエリパラメータ取得
- `url.set_query_param(u, key, value) -> Url` — パラメータ設定（immutable）
- `url.resolve(base, relative) -> Result[Url, String]` — 相対 URL 解決

### P1 (エンコード)
- `url.encode(s) -> String` — パーセントエンコード
- `url.decode(s) -> Result[String, String]` — パーセントデコード

## 実装戦略

self-host (.almd) で pure 実装可能。パースは RFC 3986 に従う文字列処理。@extern 不要。両ターゲット自動対応。
