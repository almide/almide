<a id="adr-0032-a-responses-content-type-is-its-bodys-type--body-gives-each-body-type-its-media-type-header-writes-cannot-name-content-type-and-a-script-capable-type-is-built-only-by-its-safe-module"></a>

# ADR-0032：response の Content-Type は body の型 — `Body` が各 body 型に media type を与え、header 書き込みは Content-Type を指定できず、script-capable な型は安全な module だけが構築する

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0032-a-response-content-type-is-its-body-type.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](0032-a-response-content-type-is-its-body-type.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](0032-a-response-content-type-is-its-body-type.zh-CN.md)

> 非規範的な履歴参照資料。固定原文を正本とし、自動同期しない。測定・証明 gate・現行実装の検査を独立に再実行していない。引用コマンドは実行しない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0032-a-response-content-type-is-its-body-type.md)
> この履歴ADRには受理済み決定、初期案、後の判断が併存する。D1はto_textだがD2/Q1はto_bytes、D4/表/Q5にはTypedBody/Typedと異なる返却形式が残る。Q11初期案はgeneric set_bodyだがowner要約はStringを保持してwith_bodyを加える。原文差を黙って整合させない。Q15 sealedとQ8 stdlib SVGはD7で置換済み。比較／安全性主張は原文のもので、独立検証した現行事実や普遍的XSS防御ではなく、rawのopt-outも残る。

- **状態**：Accepted（2026-10-29）。owner は方向性と Q2 に○、Q7 に×を判断し、以下の Q14–Q16 を含む残りは推奨どおり○とした。[owner 判断](#owner-decisions-)参照。
- **日付**：2026-10-29
- **範囲**：`http` response surface（`response`、`html`、`json`、`with_headers`、`set_header`、`set_body`、`set_body_bytes`、`serve`）、`html` module の `SafeHtml`、stdlib 外の media type owner と application による grant（D7）、body の media type、security-table 行11e、contract C-275。
- **関連**：
  - `docs/roadmap/active/security-by-construction.md` 行11e（HTML injection）。暫定機構である header string の runtime guard と E115 は `stdlib/http.almd` と `crates/almide-frontend/src/check/content_type_guard.rs` にある。本 surface が導入されるまで残し、その後除去する。
  - R-L0-37（具体型は protocol の合成）、`crates/almide-frontend/src/canonicalize/protocols.rs` の builtin-protocol table（`HttpMessage`、`Encode` / `Decode` / `Codec`）。
  - C-367 / C-407（serve core が framing field を所有し、`X-Content-Type-Options: nosniff` を追加する）。

<a id="context"></a>

## 背景

行11eの暫定修正は、String body に添える String header として書かれた HTML（および XML）Content-Type を拒否する。動くが、*string* への guard なので構造上3つの弱点がある。

1. **http が media 一覧を持つ。** script-capable な型の判断は document format の性質なのに、一覧が transport module（`__http_response_html_types`）にある。新たな script-capable format には http 編集が必要。
2. **body の意味が header string にある。** `with_headers(200, s, [("Content-Type", t)])` は、`s` が何であれ body を `t` document とする。checker が追えるのは型だが、body にはその意味を表す型がなく、常に `String` である。
3. **Content-Type のない response があり得る。** 指定なしの `with_headers` は browser に sniff させる。`nosniff`（C-407）は sniffing を狭めるが、型を定義しない。

owner の判断：Content-Type は header string ではなく body の型から来る。安全性は format module（`html`、D7 下の almide/svg のような package）に置き、http は media 一覧を知らない。

<a id="decision"></a>

## 決定

<a id="d1--body-a-body-type-knows-its-media-type"></a>

### D1 — `Body`：body 型は自身の media type を知る

builtin protocol `Body`（`Encode` / `Decode` / `HttpMessage` がすでに使う名詞系統）には2つの要件がある。

- `T.media_type() -> String`：この body を持つ response の Content-Type。
- `body.to_text() -> String`：wire 上の body。#4311 が厳密な bytes slot を与えるまでは、構築済み response は body を text として保持するので、要件は text である（Bytes body は lossy UTF-8 decode、Q16）。提案時は `to_bytes` だった。text なら String body は往復変換不要で、`string` は `bytes` に到達しない（`bundled_reach.rs` の #4289 reach gate）。stdlib 内では両方とも `__self_` 印付きの per-type function（`__safe_html_media_type`）。stdlib 外の型は通常名 `fn T.media_type(self)` と `fn T.to_text(self)` で書く（Q24）。

| Body 型 | Content-Type | 宣言場所 |
|---|---|---|
| `String` | `text/plain; charset=utf-8` | builtin conformer |
| `Bytes` | `application/octet-stream` | builtin conformer |
| `SafeHtml` | `text/html; charset=utf-8` | `html`（`type SafeHtml: Body`） |
| package の型 | 宣言した literal | package。script-capable なら application が grant（D7） |
| `Value`（JSON） | `application/json` | `json`（下位決定 Q3） |
| `Typed`（新規） | 構築時に指定した型 | `http`（D4） |

<a id="d2--a-response-is-built-from-a-body"></a>

### D2 — response は `Body` から構築する

`http.response[B: Body](status, body: B) -> HttpResponse` は Content-Type を `B.media_type()`、body を `body.to_bytes()` から設定する。`set_body[B: Body](resp, body)` は両方を置換するため、HTML response に String を書くと `text/plain` になる。誤った label を付けられないので、そこで拒否するものはない。取得した response は `set_body` が置換するまで upstream が送った型を維持する（下位決定 Q11）。

<a id="d3--content-type-is-owned-by-the-body-so-header-writes-cannot-name-it"></a>

### D3 — Content-Type は body の所有物なので header 書き込みは指定できない

`Body` は所有する header を宣言する（protocol 宣言の `@owns_header("Content-Type")`）。`set_header` / `with_headers` は protocol 所有の名前を拒否する。

- 名前が literal なら check 時（E116）。
- それ以外は runtime（`http.response` を名指す abort）。

規則は http 内の一覧でなく protocol 宣言を読む。serve core 所有の framing field（C-367）は別の既存規則。

<a id="d4--every-other-media-type-has-a-typed-way-in-and-a-script-capable-one-has-none"></a>

### D4 — その他の media type には型付きの入口があるが、script-capable なものにはない

`http.typed(media_type: String, body: Bytes) -> TypedBody` は、literal として書かれた、形式の正しい任意 media type の body を構築する（Q14）。checker が読むので media type に関する情報は runtime に存在しない。別の `Body` 型が**所有する**型は拒否する。

所有は安全な型自身で宣言する。

- `SafeHtml` の media fn に `@media_owns("text/html", "application/xhtml+xml")`。
- package 型の media fn に D7 下で `@media_owns(...)`。

script-capable な型は `stdlib/html.almd` の `@script_capable(...)` で一度宣言する。`text/html`、`application/xhtml+xml`、`image/svg+xml`、`*/xml`、`*/*+xml`（任意 XML document は XHTML namespace を持ち得る）。`http.typed` は所有者の有無にかかわらず script-capable 型を拒否するので、XML（Q7 ×、#4310）と almide/svg 出荷までの SVG は、所有されるが構築不能である。

属性は型の media type を述べる宣言、型の `Body` requirement fn（`__safe_html_media_type`）に置く。

pattern は RFC 6839 に従う。`*` は任意 type、`*<suffix>` は `<suffix>` で終わる、より長い subtype に一致する。拒否集合はしたがって宣言の和集合。計算された、形式不正の、または所有済みの media type は check 時 E117 となり、`http.typed` を値として指定する場合も同じ。`Body` は sealed だった（Q15）が、D7 が grant 方式に置き換える。

<a id="d5--the-safe-modules"></a>

### D5 — 安全な module

- `html` は不変。context builder と、名前付き opt-out の `raw`。
- SVG は stdlib に置かない。外部 package almide/svg が D7 下で `image/svg+xml` を所有し、本 ADR が最初に stdlib `svg` 向けに提案した builder 集合を持つ（`escape`、`attr`、`url_attr` は `on*`、`style`、script URL を拒否、`element` は `script`、`foreignObject`、animation element を拒否、`raw` は名前付き opt-out）。調査した標準ライブラリに SVG を出荷するものはない。TyXML と Trusted Types は SVG を HTML と並ぶ別の trusted markup として扱う。
- `xml`：Q7 ×、#4310。

<a id="d7--owners-outside-the-stdlib-granted-by-the-application-trusted-types-with-a-csp-allowlist"></a>

### D7 — stdlib 外の owner、application による grant（CSP allowlist 付き Trusted Types）

`Body` は open。package 型は適合でき、`@media_owns` で media type を所有できる。script-capable 型の所有は browser に script を提示する権限なので、application の同意を必要とする。

```toml
[trust]
media = { "almide/svg" = ["image/svg+xml"] }
```

- **何を指定するか。** key は依存の source `owner/repo`（github.com は暗黙、別 host は `host/owner/repo`、path dependency は `[dependencies]` 名）。依存名は別 repository に向けられるが、source はそうできない（Q17）。
- **grant が必要なもの。** `@media_owns` が script-capable 集合と重なり、その pattern を grant されていない依存は build 失敗（E118）。package、型、追加すべき行を示す。package が宣言しない型を指定する grant も E118。他型（`text/csv`）だけの所有には grant 不要。stdlib `html` は暗黙 grant。application 自身の型も native code 同様 grant 不要（Q18）で、一覧に載せる。
- **media type は literal。** 非 stdlib の `Body` 型の media fn は string literal。script-capable なものは fn 自身の `@media_owns` で覆わなければならない（Q19、E118）。したがって grant された `text/csv` package は runtime に `text/html` を serve できない。
- **型ごとに owner は一つ。** 同じ型に一致し得る pattern を持つ owner 2つは E118（Q22）。よって `http.typed` の拒否は owner を厳密に一つ名指す。
- **監査。** `almide deps` は全 media owner、つまり html、pattern と grant 行付きの各 grant 済み package、application 自身の owner を列挙する（Q23）。
- **コードは一つ。** 全 media-ownership error は E118（Q20）。以前の意味「user type が Body を宣言する」は一度も release されていない。

脅威モデル：推移的依存が無言で HTML の権限者になれない。script-capable 型を主張する package の追加・更新・入れ替えは、application が package と型を指定するまで build を止める。grant は source を key とするので、名前変更や参照先変更した依存は古い grant で覆われない。

<a id="d6--migration"></a>

### D6 — 移行

- dialect-epoch break（Q12）。
- `almide fix` は Content-Type が literal の旧表記を書き換える。
  - `http.html(st, h)` → `http.response(st, h)`。
  - `with_headers(st, s, [("Content-Type", "text/plain…"), …])` → String body と残りの header。
  - `application/json` → `http.json`。
  - その他の literal type → `http.typed(t, bytes.from_string(s))!`。
- 計算された Content-Type は書き換え不能で、E116 hint を出す。
- D2–D4 を導入する同じ変更で暫定 guard と E115 を除去する。E115 は E039 / E040 と同様、文書を残して退役する。

<a id="cross-language-comparison"></a>

## 言語横断比較

| | Content-Type の出所 | header string で body を relabel できるか | script-capable text の構築元 |
|---|---|---|---|
| **axum**（Rust） | `IntoResponse` 型：`Html<T>`、`Json<T>`。`String` → `text/plain; charset=utf-8`、`Bytes` → `application/octet-stream` | はい：`([(CONTENT_TYPE, "text/html")], s)` | 制限なし。`Html(s)` は任意 string を受け取る |
| **Go** `html/template` + `net/http` | `Header().Set`、なければ sniff（`DetectContentType`） | はい | 文脈依存 auto-escape。`template.HTML` は型付き opt-out |
| **TyXML**（OCaml） | server library（Dream `Dream.html`） | はい、server 内で | typed HTML/SVG tree、構築上 valid |
| **lucid** + servant（Haskell） | endpoint の content-type list（`Get '[HTML] (Html ())`） | 型付き endpoint では不可 | `Html` monad が text を escape、`toHtmlRaw` は opt-out |
| **Trusted Types**（browser） | （sink 側）`innerHTML` 等は `TrustedHTML` のみ受理 | — | policy だけが constructor、CSP が強制 |
| **Rails** | `render html:` / `render plain:` / `send_data type:` | はい：`send_data(s, type: "text/html")` | `SafeBuffer`、自動 escape の ERB、`html_safe` opt-out |
| **Almide（本 ADR）** | body の型（`Body`） | **不可**：E116 / abort | `html` builder、または application が grant した package owner（D7）。`http.typed` は所有済み／script-capable 型を拒否 |

Almide が取り入れるもの：

- axum の type-to-header table。
- handler でなく型が決めるという servant の規則。
- script-capable 値の constructor を policy module にだけ置く Trusted Types の規則。
- almide/svg package の形としての TyXML の typed SVG。

Almide が閉じ、他のどれも閉じないもの：header による relabel。

<a id="who-may-mint-a-trusted-type-d7"></a>

### 誰が trusted type を発行できるか（D7）

| | trusted type を発行できる者 | 同意する者 | 見える場所 |
|---|---|---|---|
| **Trusted Types** + CSP `trusted-types` | `trustedTypes.createPolicy(name, …)` を呼ぶ任意 script | page。`Content-Security-Policy: trusted-types a b` directive が許す policy 名を列挙し、他名は throw | response header |
| **Rust** `unsafe impl` | 任意 crate、自身の型について（`unsafe impl Send for T`） | `unsafe` を書く crate 作者。crate 利用者ではない | grep 可能な impl |
| **Go** `template.HTML` | 任意 package、変換により | 誰も同意しない | どこにもない |
| **Rails** `html_safe` | 任意 gem、method call により | 誰も同意しない | どこにもない |
| **Almide（D7）** | 型が `@media_owns` を宣言する package | application。`[trust] media` が package source と media type を指定しなければ build 停止（E118） | `almide.toml` と `almide deps` |

モデルは Trusted Types である。policy は名前付き constructor で、許す名前を列挙するのは library ではなく deployment。Almide は一覧を response header から manifest へ移し、program 実行前に検査する。また package が自ら選ぶ名でなく package source を key とする。

<a id="owner-decisions-"></a>

## Owner 判断（○×）

回答（2026-10-29）：Q1 ○、Q2 ○（owner）、Q3 ○、Q4 ○、Q5 は Q14 修正付きで○、Q6 ○、Q7 ×（owner、#4310）、Q8 ○、Q9 ○、Q10 ○、Q11 ○（`set_body` は String signature を保って型を置換、`http.with_body[B: Body]` は任意 body を受け取る）、Q12 ○（rc1 未 tag、導入時に再確認）、Q13 ○、Q14 ○、Q15 ○、Q16 ○（#4311）。同日後の判断（owner ○）は Q15 と Q8 を置換した。`Body` は grant 下で package に open（D7）、SVG は stdlib から almide/svg package へ移す。Q17–Q23 は推奨どおり○。

- **Q1。** protocol 名は `Body`、要件は `T.media_type()` と `to_bytes`。推奨：○。
- **Q2。** constructor は一つ、`http.response[B: Body]`。`http.html` を除去（その `SafeHtml` は `response` を通る）し、`with_headers` は `String` でなく `Body` を受け取る。推奨：○。
- **Q3。** JSON は型付き。`json.Value` が `Body` に `application/json` として適合する。`http.json(status, String)` は text 形式として残す。nosniff 付き JSON として serve した JSON は script を実行しないため。推奨：○。
- **Q4。** `set_header` / `with_headers` による Content-Type 書き込みを拒否。literal 名なら E116、それ以外は abort。代案は framing field のように黙って捨てること。推奨：○（拒否）。
- **Q5。** その他 media type は `http.typed(media_type, Bytes) -> Result[Typed, String]` を通す。形式不正または所有済み型は `err`、literal 型なら E117。推奨：○。
- **Q6。** 所有は安全型に `@media_owns(patterns…)`（RFC 6839 pattern）で宣言。`typed` は宣言の和集合を拒否。推奨：○。
- **Q7。** active XML（xhtml/svg 以外の `*/xml`、`*/*+xml`）に最小 `xml` module を用意し、`xml.element(name, attrs, children)` / `xml.text` / `xml.raw` から構築する `SafeXml` が pattern を所有する。×なら宣言のみの owner が構築不能のままにする。推奨：現時点は×。corpus に XML を serve する program はない。宣言のみの owner は pattern の拒否を保ち、`xml` module は後から break なしで追加できる。
- **Q8。** `svg` は `html` の builder 集合を踏襲：`escape`、`attr`、`url_attr`、`concat`、`empty`、`to_string`、`raw`、追加で `svg.element(name, attrs, children)`。推奨：○。
- **Q9。** `String` body は `text/plain; charset=utf-8`。当時の `http.response` は charset なし `text/plain` を初期設定するので観測可能な変更。推奨：○。
- **Q10。** `Bytes` body は `application/octet-stream`。推奨：○。
- **Q11。** `set_body[B: Body]` は body と Content-Type を同時に置換し、HTML response に String を書けば abort せず `text/plain` にする。取得 response は `set_body` まで upstream 型を保持。推奨：○。
- **Q12。** break は新 epoch 16 でなく dialect epoch 15（0.69.0、まだ unreleased）に入れる。推奨：導入時 v0.69.0-rc1 が未 tag なら○、そうでなければ epoch 16。
- **Q14。** `http.typed` の media type は literal のみ。その他の引数は E117、よって runtime に media 一覧は存在しない。推奨：○。
- **Q15。** `Body` は sealed、stdlib 宣言だけが適合し、user の `type T: Body` は E118。推奨：○。*D7 で置換済み。*
- **Q16。** Bytes body は当時の String-bodied response 表現を通すため、非 UTF-8 bytes は lossy decode する。文書化し #4311 として起票。推奨：○。
- **Q13。** response は常に Content-Type を持つ。body なしで構築すれば `String ""`、`text/plain; charset=utf-8`。当時の `with_headers(st, s, [])` は送らない。推奨：○。

- **Q17。** media grant は依存の source `owner/repo`（github.com 以外は `host/owner/repo`、path dependency は `[dependencies]` 名）を指定する。推奨：○。
- **Q18。** application 自身の型は native code 同様 grant 不要で、一覧に載せる。推奨：○。
- **Q19。** 非 stdlib の `Body` 型の media fn は string literal で、script-capable なら自身の `@media_owns` に覆われる。推奨：○。
- **Q20。** unreleased の E118 を全 media-ownership error に再利用。推奨：○。
- **Q21。** script-capable 集合は `stdlib/html.almd` に一度宣言。html が所有するのは `text/html` と `application/xhtml+xml` のみ。推奨：○。
- **Q22。** 共通の型に一致し得る pattern を持つ owner 2つは重複。推奨：○。
- **Q23。** `almide deps` に media-owners 節を追加し、新 command は設けない。推奨：○。
- **Q24。** stdlib 外の型が per-type requirement を書く方法。推奨は暫定的に内部 `__self_media_type` 表記を残すことだったが、owner は逆を判断（今直す）。すべての per-type protocol — `Read`、`Write`、`Close`、`HttpMessage`、`Body` — は通常名（`fn SafeSvg.media_type(self)`、`fn Conn.close(self)`）で満たし、第三者が `__self_` を書くことはない。公開名は一箇所 `stdlib_info::requirement_public_name` で導出（requirement の `__self_` marker を除去）し、checker の conformance lookup（`protocols::requirement_fn_key`）と monomorphization（`specialization::requirement_target`）が読む。marker は stdlib 自身の per-type function 内部に残る。`T.__self_media_type` と書いた user method は何も満たさない通常 method なので、型は `media_type` 欠落として拒否し、hint は `Add: fn T.media_type(...)`。prefix を別途拒否するより簡潔で、規則は一つ（通常名で requirement を満たす）、diagnostic も既存の一つ、文書化する新項目はない。fixture `tests/diagnostics/protocol-requirement-internal-name` が固定する。release 済み表記の変更はない。本 ADR まで `Body` は sealed で、I/O/message protocol への user conformance に `__self_` を使う方法は文書化もテストもされていなかった。そのため epoch 行や `almide fix` rewrite はない。

<a id="implementation-plan-after-the-decisions"></a>

## 実装計画（判断後）

1. まず als：C-275 を D1–D4 へ改訂し、body 型ごとの fixture と拒否を追加。pin を進める。
2. `Body` builtin protocol と conformer。checker と runtime が宣言から `@owns_header` / `@media_owns` を読む。E116、E117。
3. media grant（D7）：`[trust] media`、依存調査、E118、`almide deps` の owner 節。SVG は almide/svg package のもの。
4. 新 http surface、続いて `almide fix` rewrite と epoch break。暫定 guard と E115 の除去と同じ変更で行う。
5. gate：各 `Body` conformer の media type が宣言どおり、`http.typed` が `@media_owns` の和集合に厳密に一致して拒否、header writer は Content-Type に届かないことを matrix test する。それぞれ negative control を用意。
