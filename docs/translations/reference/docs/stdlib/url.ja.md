# url

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](url.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](url.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](url.zh-CN.md)



> 原文：[url.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/url.md)。読者向けの非規範的な固定スナップショットです。API 索引は原文固定時点の内容であり、この訳は自動生成・自動同期の対象ではありません。署名・型・`@since` とコード例（コメントを含む）は原文のまま保持し、API 説明の訳を末尾に併記しています。コード例はこの翻訳作業では独立実行していません。

URL の解析、組み立て、パーセントエンコード（RFC 3986-lite）。`import url`。

純粋でケイパビリティ不要、両ターゲットで同じです。3組の関数群は、それぞれ往復操作で閉じています：`parse ⇄ to_string`、`encode_component ⇄
decode_component`、`parse_query ⇄ build_query`。失敗は理由付きの `Result`（`int.parse` に倣う）で、パニックする派生版はありません。

対象範囲（偶発的でなく文書化された制限）：authority を持つスキームだけ（`scheme://…`）。userinfo、IPv6 の角括弧ホスト、punycode は非対応です。`+` はスペースへデコードしません。それは RFC 3986 ではなくフォームエンコードです。query や fragment が存在しても空である URL（`http://h?`）は、往復すると区切り文字のない形になります。

### `url.parse(s: String) -> Result[Url, String]`

`scheme://host[:port][/path][?query][#fragment]` を `Url` レコードへ解析します。省略時の `port` は `none`、authority の後に `/` がなければ `path` は `""`、区切りがなければ `query`/`fragment` は `""` です。次を拒否し、`err` に理由を入れます：`scheme://` authority のない入力、空のホスト（`http://`、`http://:8`）、pct-encoding を除く RFC 3986 reg-name 文字の範囲外のホスト（unreserved、sub-delims は可、`%` は拒否）、userinfo（`user@host`）、IPv6 リテラル（`[::1]`）。後二者は対象外であり、ホストとして読まず名前を明示して拒否します。ASCII 数字だけでないポートや `0..=65535` 範囲外のポートも拒否します。

ブラウザーが URL を別の宛先として読める場合、`url.parse` は WHATWG URL パーサーと同じように読むか、拒否します（ALS-T27、C-402、#4116）。

| 規則 | 例 |
|---|---|
| 先頭末尾の C0 制御文字とスペースを除去。タブ、LF、CR はどの位置でも除去 | `" http://good.com"` → ホスト `good.com` |
| スキームは最初の `:` より前の `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`、続いて `//` | `javascript:alert(1)//://good.com` → `url.parse: missing '://' scheme separator`（以前はホスト `good.com` として解析） |
| バックスラッシュやその他の制御文字を拒否 | `http:\\evil.com` → `url.parse: backslash in URL` |
| 数値ホストは正規形の10進ラベル4つでなければならない | `http://127.1/`、`http://0x7f.0.0.1/` → `url.parse: non-canonical IPv4 host: …` |
| スキームとホストを小文字化 | `HTTP://Good.COM/x` → `http://good.com/x` |
| `.` と `..` のセグメントを除去。`%2e` 表記も対象 | `http://good.com/a/%2e%2e/b` → パス `/b` |

同じプローブでの測定（2026-10-24）：WHATWG（Node 22 `URL`）と Python 3.14 `urllib` は空白を除去し、Go 1.27 `net/url` は拒否します。WHATWG は `\` を `/` として読み、`127.1` を `127.0.0.1` に正規化しますが、`url.parse` は代わりに拒否します。WHATWG は `%2e` セグメントを除去し、Go はデコードするだけで除去せず、Python は保持します。

```almd run
import url

fn host_of(s: String) -> String =
  match url.parse(s) {
    ok(u) => u.host,
    err(reason) => reason,
  }

fn main() -> Unit = {
  println(host_of("https://example.com:8080/a/b?x=1#top"))
  println(host_of("example.com/a/b"))
}
```
```output
example.com
url.parse: missing '://' scheme separator
```

### `url.to_string(u: Url) -> String`

`parse` の逆操作です。レコードを再構築し、存在する場合だけ `:port`、`?query`、`#fragment` を出力します。

### `url.encode_component(s: String) -> String`

RFC 3986 の unreserved 集合（ALPHA / DIGIT / `-` `.` `_` `~`）外の全バイトをパーセントエンコードします。まず UTF-8 とし、16進数字は大文字です。

```almd run
import url

fn main() -> Unit = {
  println(url.encode_component("a b"))
}
```
```output
a%20b
```

### `url.decode_component(s: String) -> Result[String, String]`

`encode_component` の逆操作です。壊れたエスケープ（`%zz`、途中で切れた `%A`）は位置を示す `err` になります。`+` は変更せず通します。

### `url.build_query(pairs: List[(String, String)]) -> String`

`parse_query` の逆操作で、各キーと値を `encode_component` によりパーセントエンコードします。

```almd run
import url

fn main() -> Unit = {
  println(url.build_query([("q", "a b"), ("lang", "ja")]))
}
```
```output
q=a%20b&lang=ja
```



## 固定スナップショットの API 索引（9関数）

```
// All but A-Za-z0-9-._~ as UTF-8 %XX escapes.
// @since 0.60.0 or earlier
url.encode_component(s: String) -> String

// Percent-decoded s; err on bad escape/UTF-8; + kept.
// @since 0.60.0 or earlier
url.decode_component(s: String) -> Result[String, String]

// Url from s, cleaned as WHATWG does; err on a bad scheme, host or port, a backslash or control char, userinfo, IPv6.
// @since 0.6.0 or earlier
url.parse(s: String) -> Result[Url, String]

// URL text of u; empty query/fragment omitted.
// @since 0.6.0 or earlier
url.to_string(u: Url) -> String

// k=v joined by &, both sides percent-encoded.
// @since 0.60.0 or earlier
url.build_query(xs: List[(String, String)]) -> String

// Like encode_component, but a space becomes +.
// @since unreleased
url.encode_form(s: String) -> String

// + as space, then percent-decoded; err on bad escape/UTF-8.
// @since unreleased
url.decode_form(s: String) -> Result[String, String]

// Decoded (k, v) pairs; bare key gets ""; err on a bad escape.
// @since unreleased
url.parse_query(query: String) -> Result[List[(String, String)], String]

// ref cleaned as parse does, resolved against base (RFC 3986 5.2); err when the result is not a URL.
// @since unreleased
url.resolve(base: Url, ref: String) -> Result[Url, String]
```

## 固定スナップショットの型索引（1型）

```
// Parsed URL; port none, other parts "" when absent.
// @since 0.6.0 or earlier
type url.Url = { scheme: String, host: String, port: Option[Int], path: String, query: String, fragment: String }
```




## API 説明の訳（上記署名順）

- `url.encode_component(s: String) -> String` — A-Za-z0-9-._~ 以外を UTF-8 の %XX エスケープにする。
- `url.decode_component(s: String) -> Result[String, String]` — s をパーセントデコード。不正エスケープ/UTF-8は err。+ は保持。
- `url.parse(s: String) -> Result[Url, String]` — s から Url。WHATWG と同様に整形。不正なスキーム/ホスト/ポート、バックスラッシュ/制御文字、userinfo、IPv6 は err。
- `url.to_string(u: Url) -> String` — u の URL テキスト。空の query/fragment は省略。
- `url.build_query(xs: List[(String, String)]) -> String` — k=v を & で結合。両側をパーセントエンコード。
- `url.encode_form(s: String) -> String` — encode_component と同様だが、スペースは + になる。
- `url.decode_form(s: String) -> Result[String, String]` — + をスペースにし、その後パーセントデコード。不正エスケープ/UTF-8は err。
- `url.parse_query(query: String) -> Result[List[(String, String)], String]` — デコード済み (k, v) ペア。値のないキーには ""。不正エスケープは err。
- `url.resolve(base: Url, ref: String) -> Result[Url, String]` — ref を parse と同様に整形し base に対して解決（RFC 3986 5.2）。結果が URL でなければ err。
- `type url.Url = { scheme: String, host: String, port: Option[Int], path: String, query: String, fragment: String }` — 解析済み URL。省略時 port は none、他の部分は ""。

> 原文の比較について：WHATWG や他のパーサーとの比較は、原文が示す時点・プローブの記録です。この翻訳は比較の再計測や全入力での同等性を独立に確認したものではありません。
