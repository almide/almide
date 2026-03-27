<!-- description: HTML parsing, CSS selectors, and text extraction -->
# stdlib: html [Tier 2]

HTML パース・クエリ・テキスト抽出。スクレイピング、テスト、テンプレート出力検証に使う。

## 他言語比較

### パース

| 操作 | Go (`net/html` + `goquery`) | Python (`BeautifulSoup`) | Rust (`scraper`) | Deno (`deno-dom`) |
|------|----------------------------|--------------------------|------------------|-------------------|
| パース | `html.Parse(r)` | `BeautifulSoup(html, "html.parser")` | `Html::parse_document(html)` | `new DOMParser().parseFromString(html, "text/html")` |
| フラグメント | `html.ParseFragment(r, ctx)` | `BeautifulSoup(frag)` | `Html::parse_fragment(html)` | same |

### CSS セレクタ

| 操作 | Go (`goquery`) | Python (`BeautifulSoup`) | Rust (`scraper`) | Deno (`deno-dom`) |
|------|---------------|--------------------------|------------------|-------------------|
| 単一要素 | `doc.Find("div.class").First()` | `soup.select_one("div.class")` | `doc.select(&selector).next()` | `doc.querySelector("div.class")` |
| 全要素 | `doc.Find("div.class")` | `soup.select("div.class")` | `doc.select(&selector)` | `doc.querySelectorAll("div.class")` |
| タグ名 | `.Find("a")` | `.find_all("a")` | `Selector::parse("a")` | `.getElementsByTagName("a")` |
| ID | `.Find("#main")` | `.find(id="main")` | `Selector::parse("#main")` | `.getElementById("main")` |
| クラス | `.Find(".card")` | `.find_all(class_="card")` | `Selector::parse(".card")` | `.getElementsByClassName("card")` |

### 要素操作

| 操作 | Go | Python | Rust | Deno |
|------|-----|--------|------|------|
| テキスト取得 | `sel.Text()` | `el.get_text()` / `el.string` | `el.text()` | `el.textContent` |
| 属性取得 | `sel.Attr("href")` | `el["href"]` / `el.get("href")` | `el.value().attr("href")` | `el.getAttribute("href")` |
| 内部 HTML | `sel.Html()` | `str(el)` / `el.decode_contents()` | `el.inner_html()` | `el.innerHTML` |
| タグ名 | `sel.Nodes[0].Data` | `el.name` | `el.value().name()` | `el.tagName` |
| 子要素 | `sel.Children()` | `el.children` | `el.children()` | `el.children` |
| 親要素 | `sel.Parent()` | `el.parent` | `el.parent()` | `el.parentElement` |

### テキスト抽出パターン

| パターン | Python (典型) |
|---------|--------------|
| リンク一覧 | `[a["href"] for a in soup.select("a[href]")]` |
| テーブル行 | `[[td.text for td in tr.select("td")] for tr in soup.select("tr")]` |
| メタ情報 | `soup.select_one('meta[name="description"]')["content"]` |

## Almide の設計方針

DOM API は複雑すぎる。**CSS セレクタ + テキスト/属性取得** に絞る。

```almide
let doc = html.parse(text)
let title = html.select_one(doc, "h1") |> html.text()
let links = html.select_all(doc, "a[href]") |> list.map(fn(el) => html.attr(el, "href"))
let rows = html.select_all(doc, "tr")
  |> list.map(fn(tr) => html.select_all(tr, "td") |> list.map(fn(td) => html.text(td)))
```

## 追加候補 (~12 関数)

### P0 (パース + クエリ)
- `html.parse(text) -> HtmlDoc` — HTML パース
- `html.select_one(doc, selector) -> Option[HtmlElement]` — CSS セレクタで最初の要素
- `html.select_all(doc, selector) -> List[HtmlElement]` — CSS セレクタで全要素
- `html.text(el) -> String` — テキスト取得（子要素含む）
- `html.attr(el, name) -> Option[String]` — 属性取得
- `html.inner_html(el) -> String` — 内部 HTML

### P1 (ナビゲーション)
- `html.tag(el) -> String` — タグ名
- `html.children(el) -> List[HtmlElement]` — 子要素
- `html.parent(el) -> Option[HtmlElement]` — 親要素
- `html.attrs(el) -> Map[String, String]` — 全属性

### P1 (便利関数)
- `html.links(doc) -> List[String]` — 全リンクの href
- `html.tables(doc) -> List[List[List[String]]]` — 全テーブルを 2D リストに

## 型設計

```almide
// HtmlDoc と HtmlElement は opaque 型（内部構造は非公開）
// template.md の HtmlDoc/HtmlFrag とは別物（こちらはパース結果、あちらはビルダー出力）
// 将来的に統合可能だが Phase 1 は独立

type HtmlDoc    // パース結果のドキュメント
type HtmlElement  // ドキュメント内の要素
```

### template.md との関係

- `template.md` の `HtmlDoc`/`HtmlFrag` = **構築用**（型安全にHTMLを組み立てる）
- この `html` モジュールの `HtmlDoc`/`HtmlElement` = **パース用**（既存HTMLを解析する）
- 名前衝突を避けるため、パース側は `html.ParsedDoc` / `html.Element` にするか、namespace で分離

## 実装戦略

@extern。Rust: `scraper` crate（`html5ever` + `selectors`）。TS: `deno-dom` or ブラウザ組み込み DOM API。

self-host は非現実的（HTML5 パーサーは仕様が巨大）。CSS セレクタエンジンも同様。

## ユースケース

1. **API クローラー** — pkg.go.dev, docs.python.org のスクレイピング
2. **テスト** — template で生成した HTML の検証（`assert(html.select_one(doc, "h1") |> html.text() == "Hello")`)
3. **データ抽出** — Web ページからテーブル・リンク・メタ情報を取得
4. **スクレイピング** — `http.get` + `html.parse` + `html.select_all` のパイプライン
