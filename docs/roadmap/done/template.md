<!-- description: Swift-style result builder DSL for structured data construction -->
<!-- done: 2026-03-18 -->
# Result Builder

## Vision

Swift の Result Builder と同じ思想の **汎用 builder 機構** を Almide に導入する。`builder` は言語コアの機能であり、Html / Text / Csv / Prompt は全て builder のインスタンス（stdlib）。HTML 要素は builder 内の特殊構文ではなく、**普通の関数**。

```
builder（言語コア）
├── builder Html { ... }     ← stdlib
│   ├── fn div(...)          ← 普通の関数（TOML 生成）
│   ├── fn h1(...)           ← 普通の関数（TOML 生成）
│   └── fn a(...)            ← 普通の関数（TOML 生成）
├── builder Text { ... }     ← stdlib
├── builder Csv { ... }      ← ユーザー定義可能
└── builder Prompt { ... }   ← 将来
```

---

## Design Principles

1. **builder は汎用言語機構** — Html 専用ではない。ユーザーが独自の builder を定義できる
2. **要素は普通の関数** — HTML タグ名は特殊構文ではない。`div`, `h1` は stdlib の関数
3. **builder 推論** — 関数パラメータの型が builder の Output 型なら、trailing block に自動で builder 変換を適用
4. **lift は宣言的ルール** — オーバーロードを使わず、型→Node の変換ルールを builder 定義内に列挙
5. **template は convenience** — `template` キーワードは「この関数は document を返す」という意図の明示。内部的には `fn` の一種
6. **keyword は 2 つだけ追加** — `builder`（定義用）と `template`（宣言用）

---

## 1. `builder` 宣言

### 構文

```almide
builder Html {
  type Node = HtmlNode
  type Output = HtmlFrag

  // lift ルール — 型ごとの Node への変換
  lift String => text_node(escape_html(value))
  lift Int => text_node(int.to_string(value))
  lift Float => text_node(float.to_string(value))
  lift Bool => text_node(bool.to_string(value))
  lift HtmlFrag => embed(value)

  // combinator 関数
  fn block(items: List[HtmlNode]) -> HtmlFrag = fragment(items)
  fn optional(node: Option[HtmlNode]) -> HtmlNode =
    match node { some(n) => n, none => empty_node() }
  fn array(nodes: List[HtmlNode]) -> HtmlNode = fragment_node(nodes)
}
```

### 必須メンバ

| メンバ | 役割 | Swift 相当 |
|--------|------|-----------|
| `type Node` | 中間表現の型 | `Component` |
| `type Output` | builder ブロック全体の返り値型 | `FinalResult` |
| `lift T => expr` | 式を Node に変換するルール（1 つ以上） | `buildExpression` |
| `fn block(List[Node]) -> Output` | 複数の Node を Output に結合 | `buildBlock` |

### オプショナルメンバ

| メンバ | 役割 | Swift 相当 | 未定義時 |
|--------|------|-----------|---------|
| `fn optional(Option[Node]) -> Node` | else なし `if` の処理 | `buildOptional` | else なし `if` はコンパイルエラー |
| `fn array(List[Node]) -> Node` | `for` ループの処理 | `buildArray` | `for` はコンパイルエラー |

`buildEither(first/second)` は導入しない。Almide は variant + match があるので、if/else の各ブランチは同じ型の Node を返せば `block` で処理できる。

### builder の制約

- builder 定義はトップレベルのみ（関数内に定義不可）
- `type Output` は builder ごとにユニーク（1 つの Output 型に複数の builder を紐づけることはできない）
- `lift` ルールは重複不可（同じ型に 2 つの lift を定義するとコンパイルエラー）
- builder は値ではない（変数に代入したり関数に渡したりできない）

---

## 2. コンパイラ変換規則

builder ブロック `BuilderName { ... }` 内の各構文を、builder のメソッド呼び出しに変換する。

### 式文 → lift

```almide
// ユーザーが書く
Html { "Hello" }

// コンパイラが変換
Html.block([Html.lift_String("Hello")])
// つまり
Html.block([text_node(escape_html("Hello"))])
```

式の型を見て、対応する `lift` ルールを選択。マッチする `lift` がなければコンパイルエラー。

### 複数の式 → block

```almide
Html {
  "Hello"
  user.name
  sidebar(user)
}

// 変換
Html.block([
  Html.lift_String("Hello"),
  Html.lift_String(user.name),
  Html.lift_HtmlFrag(sidebar(user)),
])
```

各式を `lift` で Node に変換し、全体を `block` で結合。

### if/else → optional または自然な分岐

```almide
// else なし → optional
Html {
  if show_title {
    h1 { "Title" }
  }
}

// 変換
Html.block([
  Html.optional(
    if show_title { some(Html.lift_HtmlFrag(h1(...))) }
    else { none }
  ),
])
```

```almide
// else あり → 両ブランチを lift して block に入れる
Html {
  if is_admin {
    admin_badge()
  } else {
    guest_badge()
  }
}

// 変換 — if/else は普通の式。結果の型が Node なら lift 不要
Html.block([
  if is_admin {
    Html.lift_HtmlFrag(admin_badge())
  } else {
    Html.lift_HtmlFrag(guest_badge())
  },
])
```

### for → array

```almide
Html {
  for item in items {
    li { item.name }
  }
}

// 変換
Html.block([
  Html.array(
    items |> list.map((item) =>
      Html.lift_HtmlFrag(li(Html.block([Html.lift_String(item.name)])))
    )
  ),
])
```

### match → 普通の式

```almide
Html {
  match user.role {
    Admin => admin_panel(user),
    Member => member_panel(user),
    Guest => guest_panel(user),
  }
}

// 変換 — match はそのまま。各ブランチの結果を lift
Html.block([
  match user.role {
    Admin => Html.lift_HtmlFrag(admin_panel(user)),
    Member => Html.lift_HtmlFrag(member_panel(user)),
    Guest => Html.lift_HtmlFrag(guest_panel(user)),
  },
])
```

### let → 変換しない

```almide
Html {
  let filtered = items |> list.filter((x) => x.active)
  for item in filtered {
    li { item.name }
  }
}

// let はそのまま。後続の式が変換対象
let filtered = items |> list.filter((x) => x.active)
Html.block([
  Html.array(filtered |> list.map((item) => ...))
])
```

### 変換まとめ

| ユーザーが書く | コンパイラが生成 |
|--------------|----------------|
| 式（型 T） | `Builder.lift_T(expr)` |
| `{ item1; item2; ... }` | `Builder.block([v1, v2, ...])` |
| `if cond { ... }` (else なし) | `Builder.optional(if cond { some(v) } else { none })` |
| `if cond { ... } else { ... }` | `if cond { lift(v1) } else { lift(v2) }` |
| `for x in xs { ... }` | `Builder.array(xs \|> list.map((x) => ...))` |
| `match expr { ... }` | `match expr { pat => lift(v), ... }` |
| `let x = expr` | そのまま（変換しない） |

### builder ブロック内で使えないもの

- `var`（mutable binding）
- 代入（`=`, `.field =`, `[i] =`）
- `while`
- `guard`
- `return`（Swift と同じ — 結果は暗黙的に収集）

---

## 3. Builder 推論と Trailing Block

### Builder 推論

関数パラメータの型が builder の `Output` 型のとき、引数として渡される `{ }` ブロックに自動で builder 変換を適用する。

```almide
// div の定義
fn div(class: String = "", children: HtmlFrag) -> HtmlNode

// HtmlFrag は Html.Output
// だから children に渡す { } に Html builder が自動適用される
```

コンパイラは `Output → Builder` のレジストリを保持する:

```
HtmlFrag → Html
TextFrag → Text
CsvDoc   → Csv
```

制約: 1 つの Output 型に紐づく builder は最大 1 つ。

### Trailing Block 構文

**関数呼び出しの最後の引数が builder の Output 型のとき、trailing `{ }` で渡せる。**

```almide
// 以下は全て等価
div(class: "card", children = Html { p { "hello" } })
div(class: "card") { p { "hello" } }      // ← trailing block（推奨）

// 属性なしの場合
div(children = Html { p { "hello" } })
div { p { "hello" } }                     // ← trailing block（推奨）
```

**パーサルール:**

```
CallExpr ::= Expr '(' ArgList? ')' TrailingBlock?
TrailingBlock ::= '{' BuilderItem* '}'
```

trailing block がある場合:
1. 呼び出し先関数のシグネチャを解決
2. 最後のパラメータの型が builder の Output 型か確認
3. Output 型に対応する builder を特定
4. `{ }` 内を builder 変換し、最後のパラメータとして渡す

trailing block のない `{ }` がブロック式か builder block かの曖昧性:
- 関数呼び出しの直後 → trailing block
- `BuilderName { }` の形 → builder block
- それ以外 → 通常のブロック式

### Trailing Block のネスト

この仕組みにより HTML のような自然なネストが書ける:

```almide
Html {                                    // ← Html builder 明示
  div(class: "card") {                    // ← trailing block, Html 推論
    h2 { "Title" }                        // ← trailing block, Html 推論
    p { "Content" }                       // ← trailing block, Html 推論
    ul {                                  // ← trailing block, Html 推論
      for item in items {
        li { item.name }                  // ← trailing block, Html 推論
      }
    }
  }
}
```

最上位だけ `Html { }` を書けば、以降の関数呼び出しは trailing block + builder 推論で自動変換。`Html { }` のネストが不要。

---

## 4. `template` キーワード

`fn` を置き換える宣言キーワード。「この関数は document を組み立てる」という意図の明示。

```almide
template invoice_email(inv: Invoice) -> HtmlDoc = Html {
  head { title { "Invoice #${inv.id}" } }
  body {
    h1 { "Invoice" }
    invoice_table(inv.items)
  }
}
```

### Semantic Model

内部的には `Fn` の一種:

```rust
pub enum FnKind {
    Regular,
    Async,
    Effect,
    Template,
}
```

### template の制約

- **pure のみ** — `effect template` は不許可
- **戻り値型は Doc 型のみ** — `HtmlDoc`, `TextDoc` 等
- **body は自由** — builder block に限らず、`if` / `match` で異なる Doc を返せる

```almide
// ✅ 直接 builder
template page(user: User) -> HtmlDoc = Html { body { ... } }

// ✅ 条件分岐
template page(user: User) -> HtmlDoc =
  if user.is_admin { admin_page(user) }
  else { normal_page(user) }

// ❌ template must return a document type
template bad() -> String = "hello"

// ❌ template cannot be effect
effect template bad() -> HtmlDoc = Html { ... }
```

### template vs fn

| 宣言 | 用途 | 戻り値型の制約 |
|------|------|--------------|
| `template` | 完全な document の entry point | Doc 型のみ |
| `fn` | fragment / helper / 任意の関数 | 制約なし |

`fn` が builder の Output 型を返すことは許可:

```almide
fn sidebar(user: User) -> HtmlFrag = Html {
  nav { a(href: "/") { "Home" } }
}
```

---

## 5. Html Builder（stdlib）

### Builder 定義

```almide
builder Html {
  type Node = HtmlNode
  type Output = HtmlFrag

  lift String => text_node(escape_html(value))
  lift Int => text_node(int.to_string(value))
  lift Float => text_node(float.to_string(value))
  lift Bool => text_node(bool.to_string(value))
  lift HtmlFrag => embed(value)

  fn block(items: List[HtmlNode]) -> HtmlFrag = fragment(items)
  fn optional(node: Option[HtmlNode]) -> HtmlNode =
    match node { some(n) => n, none => empty_node() }
  fn array(nodes: List[HtmlNode]) -> HtmlNode = fragment_node(nodes)
}
```

### HtmlDoc と HtmlFrag

```
HtmlDoc  — 完全な HTML document（<html> ルート）
HtmlFrag — HTML fragment（要素 or ノードリスト）
```

- `HtmlFrag` は `Html.Output`
- `HtmlDoc` は `HtmlFrag` を `render` で完全な文書に変換したもの
- `HtmlFrag` は他の `HtmlFrag` に差し込める（lift ルールで処理）
- `HtmlDoc` は差し込めない（lift ルールなし → コンパイルエラー）

`HtmlDoc` への変換:

```almide
fn to_doc(frag: HtmlFrag) -> HtmlDoc     // frag を完全な HTML document にラップ
fn render(doc: HtmlDoc) -> String         // 文字列にシリアライズ
fn render_frag(frag: HtmlFrag) -> String  // fragment だけシリアライズ
```

---

## 6. HTML 要素 — 普通の関数

### 設計

HTML 要素は **stdlib の関数** として定義。builder 機構の知識を持たない。

各要素関数は:
- 要素固有の named attributes（デフォルト値付き）
- global attributes（全要素共通、デフォルト値付き）
- `children: HtmlFrag`（最後のパラメータ、trailing block で渡される）

```almide
// 生成される関数シグネチャの例

// Block elements
fn div(id: String = "", class: String = "", hidden: Bool = false,
       children: HtmlFrag) -> HtmlNode

fn p(id: String = "", class: String = "",
     children: HtmlFrag) -> HtmlNode

fn h1(id: String = "", class: String = "",
      children: HtmlFrag) -> HtmlNode

// Links
fn a(href: String = "", target: String = "", rel: String = "",
     id: String = "", class: String = "",
     children: HtmlFrag) -> HtmlNode

// Images (void element — children なし)
fn img(src: String, alt: String = "",
       width: Int = 0, height: Int = 0,
       id: String = "", class: String = "") -> HtmlNode

// Forms
fn input(type_: String = "text", name: String = "", value: String = "",
         placeholder: String = "", disabled: Bool = false, required: Bool = false,
         id: String = "", class: String = "") -> HtmlNode

fn button(type_: String = "button", disabled: Bool = false,
          id: String = "", class: String = "",
          children: HtmlFrag) -> HtmlNode

// Tables
fn table(id: String = "", class: String = "",
         children: HtmlFrag) -> HtmlNode
fn tr(children: HtmlFrag) -> HtmlNode
fn td(colspan: Int = 1, children: HtmlFrag) -> HtmlNode
fn th(colspan: Int = 1, children: HtmlFrag) -> HtmlNode
```

### 使い方

```almide
Html {
  div(class: "card") {                      // named arg + trailing block
    h1 { "Hello" }                          // trailing block のみ
    p(class: "subtitle") { "Welcome" }
    a(href: "/home", class: "btn") { "Go" }
    img(src: "/photo.jpg", alt: "Photo")    // void element, trailing block なし
    input(type_: "text", placeholder: "Search...")
  }
}
```

### コンポーネント — 同じパターン

ユーザー定義コンポーネントも `children: HtmlFrag` を最後のパラメータにすれば trailing block が使える:

```almide
fn card(title: String, children: HtmlFrag) -> HtmlNode = Html {
  div(class: "card") {
    h2 { title }
    div(class: "card-body") { children }
  }
} |> to_node

fn badge(text: String, color: String) -> HtmlNode = Html {
  span(class: "badge badge-${color}") { text }
} |> to_node

// 使い方 — HTML 要素と同じ構文
Html {
  card(title: "Profile") {
    p { user.name }
    badge("Admin", "green")
  }
}
```

### Void Element の扱い

`img`, `input`, `br`, `hr`, `meta`, `link` 等は `children` パラメータを持たない:

```almide
fn img(src: String, alt: String = "") -> HtmlNode          // children なし
fn br() -> HtmlNode                                         // 引数なし
fn input(type_: String = "text", name: String = "") -> HtmlNode
fn meta(charset: String = "", name: String = "", content: String = "") -> HtmlNode
```

trailing block を渡すとコンパイルエラー:

```
error: img does not accept children
  --> app.almd:3:5
   |
 3 |     img(src: "photo.jpg") { "text" }
   |                            ^^^^^^^^ unexpected block
   |
   = hint: img is a void element; remove the { } block
```

---

## 7. HTML 要素 — TOML 駆動生成

### TOML 定義

```toml
# stdlib/defs/html_elements.toml

# Global attributes（全要素に付与）
[global]
attrs = [
  { name = "id",       type = "String", default = "\"\"" },
  { name = "class",    type = "String", default = "\"\"" },
  { name = "title",    type = "String", default = "\"\"" },
  { name = "hidden",   type = "Bool",   default = "false" },
  { name = "tabindex", type = "Int",    default = "0" },
  { name = "lang",     type = "String", default = "\"\"" },
]

# data-* / aria-* は特殊処理（Phase 2）

# --- Block elements ---

[elements.div]
category = "flow"

[elements.p]
category = "flow"

[elements.h1]
category = "flow"

[elements.h2]
category = "flow"

[elements.h3]
category = "flow"

[elements.h4]
category = "flow"

[elements.h5]
category = "flow"

[elements.h6]
category = "flow"

[elements.pre]
category = "flow"

[elements.blockquote]
category = "flow"

# --- Inline elements ---

[elements.span]
category = "phrasing"

[elements.strong]
category = "phrasing"

[elements.em]
category = "phrasing"

[elements.code]
category = "phrasing"

[elements.small]
category = "phrasing"

[elements.a]
category = "phrasing"
attrs = [
  { name = "href",     type = "String", default = "\"\"" },
  { name = "target",   type = "String", default = "\"\"" },
  { name = "rel",      type = "String", default = "\"\"" },
  { name = "download", type = "String", default = "\"\"" },
]

[elements.time]
category = "phrasing"
attrs = [
  { name = "datetime", type = "String", default = "\"\"" },
]

# --- Lists ---

[elements.ul]
category = "flow"

[elements.ol]
category = "flow"
attrs = [
  { name = "start", type = "Int", default = "1" },
]

[elements.li]
category = "flow"

# --- Tables ---

[elements.table]
category = "flow"

[elements.thead]
category = "table"

[elements.tbody]
category = "table"

[elements.tfoot]
category = "table"

[elements.tr]
category = "table_row"

[elements.th]
category = "table_cell"
attrs = [
  { name = "colspan", type = "Int", default = "1" },
  { name = "rowspan", type = "Int", default = "1" },
  { name = "scope",   type = "String", default = "\"\"" },
]

[elements.td]
category = "table_cell"
attrs = [
  { name = "colspan", type = "Int", default = "1" },
  { name = "rowspan", type = "Int", default = "1" },
]

# --- Forms ---

[elements.form]
category = "flow"
attrs = [
  { name = "action",  type = "String", default = "\"\"" },
  { name = "method",  type = "String", default = "\"get\"" },
]

[elements.input]
category = "void"
attrs = [
  { name = "type_",       type = "String", default = "\"text\"" },
  { name = "name",        type = "String", default = "\"\"" },
  { name = "value",       type = "String", default = "\"\"" },
  { name = "placeholder", type = "String", default = "\"\"" },
  { name = "disabled",    type = "Bool",   default = "false" },
  { name = "required",    type = "Bool",   default = "false" },
  { name = "checked",     type = "Bool",   default = "false" },
]

[elements.textarea]
category = "flow"
attrs = [
  { name = "name",        type = "String", default = "\"\"" },
  { name = "placeholder", type = "String", default = "\"\"" },
  { name = "rows",        type = "Int",    default = "0" },
  { name = "cols",        type = "Int",    default = "0" },
  { name = "disabled",    type = "Bool",   default = "false" },
  { name = "required",    type = "Bool",   default = "false" },
]

[elements.select]
category = "flow"
attrs = [
  { name = "name",     type = "String", default = "\"\"" },
  { name = "disabled", type = "Bool",   default = "false" },
  { name = "required", type = "Bool",   default = "false" },
]

[elements.option]
category = "flow"
attrs = [
  { name = "value",    type = "String", default = "\"\"" },
  { name = "selected", type = "Bool",   default = "false" },
  { name = "disabled", type = "Bool",   default = "false" },
]

[elements.button]
category = "flow"
attrs = [
  { name = "type_",    type = "String", default = "\"button\"" },
  { name = "disabled", type = "Bool",   default = "false" },
]

[elements.label]
category = "phrasing"
attrs = [
  { name = "for_", type = "String", default = "\"\"" },
]

# --- Sections ---

[elements.header]
category = "flow"

[elements.footer]
category = "flow"

[elements.main]
category = "flow"

[elements.nav]
category = "flow"

[elements.section]
category = "flow"

[elements.article]
category = "flow"

[elements.aside]
category = "flow"

# --- Media ---

[elements.img]
category = "void"
attrs = [
  { name = "src",    type = "String" },
  { name = "alt",    type = "String", default = "\"\"" },
  { name = "width",  type = "Int",    default = "0" },
  { name = "height", type = "Int",    default = "0" },
]

[elements.video]
category = "flow"
attrs = [
  { name = "src",      type = "String", default = "\"\"" },
  { name = "controls", type = "Bool",   default = "false" },
  { name = "autoplay", type = "Bool",   default = "false" },
  { name = "width",    type = "Int",    default = "0" },
  { name = "height",   type = "Int",    default = "0" },
]

[elements.audio]
category = "flow"
attrs = [
  { name = "src",      type = "String", default = "\"\"" },
  { name = "controls", type = "Bool",   default = "false" },
]

[elements.source]
category = "void"
attrs = [
  { name = "src",  type = "String" },
  { name = "type_", type = "String", default = "\"\"" },
]

# --- Misc ---

[elements.br]
category = "void"

[elements.hr]
category = "void"

[elements.details]
category = "flow"

[elements.summary]
category = "flow"

[elements.dialog]
category = "flow"
attrs = [
  { name = "open", type = "Bool", default = "false" },
]

# --- Void / special ---

[elements.meta]
category = "void"
attrs = [
  { name = "charset", type = "String", default = "\"\"" },
  { name = "name",    type = "String", default = "\"\"" },
  { name = "content", type = "String", default = "\"\"" },
]

[elements.link]
category = "void"
attrs = [
  { name = "rel",  type = "String", default = "\"\"" },
  { name = "href", type = "String", default = "\"\"" },
  { name = "type_", type = "String", default = "\"\"" },
]

# --- Banned elements (security) ---

[banned]
elements = ["script", "style", "iframe", "object", "embed"]
reason = "security: use typed alternatives"
```

### build.rs による生成

`build.rs` が `html_elements.toml` を読み、以下を生成:

```rust
// src/generated/html_elements.rs — DO NOT EDIT

pub struct HtmlElementDef {
    pub name: &'static str,
    pub category: ElementCategory,
    pub attrs: &'static [HtmlAttrDef],
    pub is_void: bool,
}

pub struct HtmlAttrDef {
    pub name: &'static str,
    pub almide_name: &'static str,   // type_ → type_, for_ → for_
    pub html_name: &'static str,     // type_ → type, for_ → for
    pub ty: AttrType,
    pub has_default: bool,
    pub required: bool,
}

pub fn all_elements() -> &'static [HtmlElementDef] { ... }
pub fn global_attrs() -> &'static [HtmlAttrDef] { ... }
pub fn is_banned(name: &str) -> Option<&'static str> { ... }
pub fn suggest_element(typo: &str) -> Option<&'static str> { ... }
```

生成されるもの:
- **型チェッカー用**: 各要素関数のシグネチャ情報（パラメータ名、型、デフォルト値）
- **codegen 用**: 要素名 → HTML タグ名のマッピング（`type_` → `type`）
- **診断用**: typo 候補、banned element チェック

---

## 8. Text Builder（stdlib）

```almide
builder Text {
  type Node = TextNode
  type Output = TextFrag

  lift String => text_line(value)
  lift Int => text_line(int.to_string(value))
  lift Float => text_line(float.to_string(value))

  fn block(items: List[TextNode]) -> TextFrag = join(items)
  fn optional(node: Option[TextNode]) -> TextNode =
    match node { some(n) => n, none => empty_text() }
  fn array(nodes: List[TextNode]) -> TextNode = join_node(nodes)
}

// Text 専用の関数
fn line(children: TextFrag) -> TextNode      // 1 行（末尾改行）
fn blank() -> TextNode                       // 空行
fn indent(level: Int = 1, children: TextFrag) -> TextNode  // インデント

// 使い方
template receipt(x: Receipt) -> TextDoc = Text {
  line { "Receipt #${x.id}" }
  line { "Customer: ${x.customer_name}" }
  blank()
  for item in x.items {
    line { "  ${item.name} — ${money(item.price)}" }
  }
  blank()
  line { "Total: ${money(x.total)}" }
} |> to_text_doc
```

---

## 9. AST 表現

builder 機構は汎用なので、AST は Html/Text を区別しない。

```rust
// Expr に追加
pub enum Expr {
    // ... 既存 ...

    /// BuilderName { items }
    /// 明示的な builder ブロック
    BuilderBlock {
        builder_name: String,              // "Html", "Text", "Csv", ...
        items: Vec<BuilderItem>,
        #[serde(skip)] span: Option<Span>,
    },
}

/// Builder ブロック内のアイテム
pub enum BuilderItem {
    /// 式（lift 対象）
    Expr {
        expr: Expr,
        #[serde(skip)] span: Option<Span>,
    },
    /// if cond { items } [else { items }]
    If {
        cond: Expr,
        then_items: Vec<BuilderItem>,
        else_items: Option<Vec<BuilderItem>>,
        #[serde(skip)] span: Option<Span>,
    },
    /// for pat in expr { items }
    ForIn {
        pattern: Pattern,
        iterable: Expr,
        body: Vec<BuilderItem>,
        #[serde(skip)] span: Option<Span>,
    },
    /// match expr { arms }
    Match {
        subject: Expr,
        arms: Vec<BuilderMatchArm>,
        #[serde(skip)] span: Option<Span>,
    },
    /// let 束縛（変換しない）
    Let {
        pattern: Pattern,
        value: Expr,
        #[serde(skip)] span: Option<Span>,
    },
}

pub struct BuilderMatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Vec<BuilderItem>,
}
```

旧設計との違い:
- `BuilderItem::Element` 削除 — 要素は普通の関数呼び出し（`BuilderItem::Expr` に含まれる）
- `BuilderItem::Text` 削除 — 文字列リテラルも `BuilderItem::Expr` に統合
- `BuilderItem::Line` / `BuilderItem::Blank` 削除 — `line()`, `blank()` は普通の関数
- `HtmlBuilder` / `TextBuilder` の区別削除 — 汎用 `BuilderBlock` に統一

### Trailing Block の AST

trailing block 付きの関数呼び出しは、パーサが最後の引数として挿入:

```almide
div(class: "card") { p { "hello" } }
```

```rust
// AST: div(class: "card", <trailing_block>)
Expr::Call {
    target: "div",
    args: [
        NamedArg("class", Expr::Lit("card")),
        // trailing block は最後の引数として挿入
        // この時点では BuilderBlock ではなく TrailingBlock
        Expr::TrailingBlock {
            items: [
                BuilderItem::Expr { expr: Call("p", [TrailingBlock { ... }]) }
            ]
        }
    ],
}
```

チェッカーが `div` のシグネチャを解決し、最後のパラメータ `children: HtmlFrag` の型から builder を特定して変換を適用。

---

## 10. IR 表現

IR では builder 変換が完了し、lift が解決済み。

```rust
pub enum IrExprKind {
    // ... 既存 ...

    /// Builder の block 呼び出し（変換済み）
    BuilderBlock {
        builder: BuilderRef,          // どの builder か
        items: Vec<IrBuilderNode>,    // lift 済みのノード列
    },
}

/// IR builder node — lift 解決済み
pub enum IrBuilderNode {
    /// lift 済みの式 → Node
    Lifted {
        lift_rule: LiftRuleRef,       // どの lift ルールで変換したか
        expr: IrExpr,                 // 元の式
        span: Option<Span>,
    },
    /// if (else なし) → optional
    Optional {
        cond: IrExpr,
        then_node: Box<IrBuilderNode>,
        span: Option<Span>,
    },
    /// for → array
    Array {
        var: VarId,
        iterable: IrExpr,
        body: Box<IrBuilderNode>,
        span: Option<Span>,
    },
    /// if/else, match → 普通の条件式（結果が Node 型）
    Conditional {
        expr: IrExpr,                 // if/match 式（結果が Node）
        span: Option<Span>,
    },
    /// let 束縛
    Let {
        var: VarId,
        value: IrExpr,
        span: Option<Span>,
    },
}
```

---

## 11. 型チェック

### builder 解決

1. `BuilderBlock { builder_name: "Html", items }` を見つける
2. `Html` builder の定義を lookup
3. 各 `BuilderItem::Expr` の型を推論
4. 型に対応する `lift` ルールを検索
5. マッチしなければコンパイルエラー
6. 全 item を lift → `block` の引数型と一致するか検証

### trailing block 解決

1. `CallExpr` に `TrailingBlock` が付いている
2. 呼び出し先関数の最後のパラメータ型を取得
3. その型が builder の Output 型か registry で確認
4. 一致すれば builder を特定し、`TrailingBlock` を `BuilderBlock` に変換
5. 一致しなければエラー: `trailing block requires a builder output type`

### lift ルールの型チェック

```
// builder Html の lift ルール
lift String => text_node(escape_html(value))

// コンパイラの検証:
// 1. `value` は `String` 型で束縛される
// 2. `text_node(escape_html(value))` の型が `HtmlNode` (= Html.Node) か検証
// 3. 不一致ならエラー
```

### エラーメッセージ

| 状況 | メッセージ |
|------|-----------|
| lift ルールなし | `cannot insert User in Html builder; no lift rule for type User. Use a field like user.name (String), or write a function that returns HtmlFrag` |
| Doc を差し込み | `cannot insert HtmlDoc in Html builder; HtmlDoc is a complete document. Use a function that returns HtmlFrag instead` |
| void に trailing block | `img does not accept children; img is a void element, remove the { } block` |
| trailing block 型不一致 | `trailing block requires a builder output type; parameter 'x' has type Int, which is not a builder output` |
| 不明な builder | `unknown builder 'Foo'; did you mean 'Html'?` |
| optional 未定義 | `cannot use 'if' without 'else' in Csv builder; Csv does not define 'optional'. Add an else branch, or define fn optional in the builder` |
| array 未定義 | `cannot use 'for' in Csv builder; Csv does not define 'array'. Define fn array in the builder` |
| banned element | `function 'script' is banned in html context (security); use typed alternatives` |

---

## 12. Codegen

### Rust codegen

builder ブロックは **ノードツリーの構築コード** に変換:

```rust
// Almide:
// Html { div(class: "card") { h1 { "Hello" } } }

// 生成される Rust:
{
    let __b0 = almide_html_text_node(&almide_html_escape("Hello"));
    let __b1 = almide_html_element("h1", vec![], vec![__b0]);
    let __b2 = almide_html_element("div",
        vec![almide_html_attr("class", "card")],
        vec![__b1]);
    almide_html_fragment(vec![__b2])
}
```

### TS codegen

```typescript
// 生成される TypeScript:
(() => {
    const __b0 = __almd_html.text(__almd_html.escape("Hello"));
    const __b1 = __almd_html.el("h1", {}, [__b0]);
    const __b2 = __almd_html.el("div", { class: "card" }, [__b1]);
    return __almd_html.frag([__b2]);
})()
```

### render 関数

```rust
// Rust runtime
pub fn almide_html_render(doc: &HtmlDoc) -> String {
    let mut out = String::new();
    out.push_str("<!DOCTYPE html>");
    render_node(&doc.root, &mut out);
    out
}

fn render_node(node: &HtmlNode, out: &mut String) {
    match node {
        HtmlNode::Element { tag, attrs, children } => {
            out.push('<');
            out.push_str(tag);
            for attr in attrs {
                render_attr(attr, out);
            }
            if is_void(tag) {
                out.push_str(" />");
            } else {
                out.push('>');
                for child in children {
                    render_node(child, out);
                }
                out.push_str("</");
                out.push_str(tag);
                out.push('>');
            }
        }
        HtmlNode::Text(s) => {
            // すでに escape 済み（lift 時に escape_html 適用）
            out.push_str(s);
        }
        HtmlNode::Fragment(children) => {
            for child in children {
                render_node(child, out);
            }
        }
        HtmlNode::Empty => {}
    }
}
```

---

## 13. Runtime 型

### Rust

```rust
pub enum HtmlNode {
    Element {
        tag: String,
        attrs: Vec<(String, HtmlAttrValue)>,
        children: Vec<HtmlNode>,
    },
    Text(String),
    Fragment(Vec<HtmlNode>),
    Empty,
}

pub enum HtmlAttrValue {
    Str(String),
    Bool(bool),
    Int(i64),
}

pub struct HtmlFrag(pub Vec<HtmlNode>);
pub struct HtmlDoc { pub root: HtmlNode }

pub fn almide_html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

pub fn almide_html_element(tag: &str, attrs: Vec<(String, HtmlAttrValue)>, children: Vec<HtmlNode>) -> HtmlNode {
    HtmlNode::Element { tag: tag.to_string(), attrs, children }
}

pub fn almide_html_text_node(s: &str) -> HtmlNode {
    HtmlNode::Text(s.to_string())
}

pub fn almide_html_fragment(nodes: Vec<HtmlNode>) -> HtmlFrag {
    HtmlFrag(nodes)
}
```

### TypeScript

```typescript
type HtmlNode =
  | { tag: string; attrs: Record<string, string | boolean | number>; children: HtmlNode[] }
  | { text: string }
  | { frag: HtmlNode[] }
  | { empty: true }

type HtmlFrag = HtmlNode[]
type HtmlDoc = { root: HtmlNode }

const __almd_html = {
  escape: (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;'),
  text: (s: string) => ({ text: s }),
  el: (tag: string, attrs: Record<string, any>, children: HtmlNode[]) => ({ tag, attrs, children }),
  frag: (nodes: HtmlNode[]) => nodes,
  empty: () => ({ empty: true as const }),
  render: (doc: HtmlDoc) => { /* ... */ },
}
```

---

## 14. Keywords

### 予約語として追加

| keyword | 理由 |
|---------|------|
| `builder` | builder 定義のためのキーワード |
| `template` | document entry point の宣言（`fn` の variant） |

### 予約語にしないもの

| 識別子 | 理由 |
|--------|------|
| `lift` | builder 定義内でのみ意味を持つ |
| `Html`, `Text`, `Csv` | builder 名は普通の識別子 |
| HTML タグ名 | 普通の関数。builder と無関係 |

**追加する予約語は `builder` と `template` の 2 つ。**

---

## 15. Complete Examples

### HTML Email

```almide
import html exposing (div, h1, p, a, table, thead, tbody, tr, th, td)

type Invoice = {
  id: String,
  customer: String,
  pay_url: String,
  items: List[LineItem],
}
type LineItem = { name: String, price: Int }

fn money(n: Int) -> String = "$${int.to_string(n / 100)}.${int.to_string(n % 100)}"

fn price_row(item: LineItem) -> HtmlFrag = Html {
  tr {
    td { item.name }
    td(class: "price") { money(item.price) }
  }
}

template invoice_email(inv: Invoice) -> HtmlDoc = Html {
  h1 { "Invoice #${inv.id}" }
  p { "Hello, ${inv.customer}." }
  table {
    thead { tr { th { "Item" }; th { "Price" } } }
    tbody {
      for item in inv.items {
        price_row(item)
      }
    }
  }
  p { a(href: inv.pay_url) { "Pay now" } }
} |> to_doc
```

### Conditional / Match

```almide
import html exposing (div, h2, p, span)

type User = { name: String, role: Role, verified: Bool, joined: String }
type Role = | Admin | Member | Guest

fn badge(text: String, color: String) -> HtmlFrag = Html {
  span(class: "badge badge-${color}") { text }
}

template user_card(user: User) -> HtmlDoc = Html {
  div(class: "card") {
    h2 { user.name }
    if user.verified { badge("Verified", "green") }
    else { badge("Unverified", "gray") }
    match user.role {
      Admin => p { "Administrator" },
      Member => p { "Member since ${user.joined}" },
      Guest => p { "Guest — please sign up" },
    }
  }
} |> to_doc
```

### Layout + Component Composition

```almide
import html exposing (head, body, title, meta, header, main, footer, h1, h2, div, p, ul, li)

fn page_shell(page_title: String, children: HtmlFrag) -> HtmlFrag = Html {
  head { title { page_title }; meta(charset: "utf-8") }
  body {
    header { h1 { page_title } }
    main { children }
    footer { "© 2026 Almide" }
  }
}

fn card(card_title: String, children: HtmlFrag) -> HtmlFrag = Html {
  div(class: "card") {
    h2 { card_title }
    div(class: "card-body") { children }
  }
}

template dashboard(user: User, items: List[Item]) -> HtmlDoc = Html {
  page_shell(page_title: "Dashboard") {
    card(card_title: "Profile") {
      p { user.name }
      p { user.email }
    }
    card(card_title: "Items") {
      ul {
        for item in items {
          li { "${item.name} — ${money(item.price)}" }
        }
      }
    }
  }
} |> to_doc
```

### Pipeline in Builder

```almide
import html exposing (body, h1, div, p)

template catalog(products: List[Product]) -> HtmlDoc = Html {
  body {
    h1 { "Catalog" }
    let available = products
      |> list.filter((p) => p.in_stock)
      |> list.sort_by((p) => p.price)
    div(class: "grid") {
      for p in available {
        product_card(p)
      }
    }
    p { "${int.to_string(list.len(available))} products available" }
  }
} |> to_doc
```

### Text Document

```almide
template receipt(x: Receipt) -> TextDoc = Text {
  line { "Receipt #${x.id}" }
  line { "Customer: ${x.customer_name}" }
  blank()
  for item in x.items {
    line { "  ${item.name} — ${money(item.price)}" }
  }
  blank()
  line { "Total: ${money(x.total)}" }
} |> to_text_doc
```

### Custom Builder — CSV

```almide
builder Csv {
  type Node = CsvRow
  type Output = CsvFrag

  lift List[String] => csv_row(value)

  fn block(items: List[CsvRow]) -> CsvFrag = csv_frag(items)
  fn array(nodes: List[CsvRow]) -> CsvRow = csv_flatten(nodes)
}

fn csv_header(columns: List[String], children: CsvFrag) -> CsvFrag = ...

template product_report(products: List[Product]) -> CsvDoc = Csv {
  csv_header(["Name", "Price", "Stock"]) {
    for p in products {
      [p.name, money(p.price), int.to_string(p.stock)]
    }
  }
} |> to_csv_doc
```

### Testing

```almide
import html exposing (body, h1)

template greeting(name: String) -> HtmlDoc = Html {
  body { h1 { "Hello, ${name}" } }
} |> to_doc

test "greeting renders correctly" {
  let s = greeting("Alice") |> render
  assert(string.contains(s, "Hello, Alice"))
  assert(string.contains(s, "<h1>"))
}

test "greeting escapes HTML in name" {
  let s = greeting("<script>alert(1)</script>") |> render
  assert(string.contains(s, "&lt;script&gt;"))
  assert(not(string.contains(s, "<script>")))
}
```

### Rainbow FFI — builder + export

```almide
import html exposing (body, h1, p)

template email_body(name: String, age: Int) -> HtmlDoc = Html {
  body {
    h1 { "Hello, ${name}" }
    p { "You are ${int.to_string(age)} years old." }
  }
} |> to_doc

export fn render_email(name: String, age: Int) -> String =
  email_body(name, age) |> render
```

```python
# Python から呼ぶ
from almide_lib import render_email
html = render_email("Alice", 30)
send_email(to=addr, body=html)
```

---

## 16. Phase Roadmap

### Phase 1: Builder Core

言語コアに `builder` 機構を追加:

- [ ] `builder` keyword + parser（builder 定義の parse）
- [ ] `template` keyword + parser（FnKind::Template）
- [ ] `BuilderBlock` AST node（汎用）
- [ ] Trailing block parser（`fn(args) { items }` → 最後の引数に挿入）
- [ ] Builder registry（Output 型 → Builder のマッピング）
- [ ] Builder 推論（trailing block の Output 型から builder を特定）
- [ ] `lift` ルール resolution（式の型 → lift ルール検索 → Node 変換）
- [ ] `block` / `optional` / `array` への変換（IR 生成）
- [ ] builder 内の `if` / `for` / `match` / `let` 変換
- [ ] builder ブロック内の制約チェック（var/assign/while 禁止）
- [ ] 診断（lift ルールなし、optional 未定義で if 使用、等）

### Phase 2: Html Builder

stdlib に Html builder を追加:

- [ ] `builder Html` 定義（lift ルール + block/optional/array）
- [ ] `html_elements.toml` + `build.rs` 生成
- [ ] HTML 要素関数（div, h1, p, a, img, ... — TOML から ~70 関数生成）
- [ ] Global attributes（id, class, hidden, ...）
- [ ] Element-specific attributes（href, src, alt, ...）
- [ ] Void element 処理（img, input, br — trailing block 禁止）
- [ ] `HtmlNode` / `HtmlFrag` / `HtmlDoc` 型
- [ ] `render(HtmlDoc) -> String` / `render_frag(HtmlFrag) -> String`
- [ ] Context-aware HTML escaping（`escape_html`）
- [ ] Banned element チェック（script, style, iframe）
- [ ] Rust runtime 実装（`core_runtime.txt` に追加）
- [ ] TS runtime 実装（`emit_ts_runtime` に追加）
- [ ] HTML 要素 typo 診断（`diiv` → `did you mean div?`）

### Phase 3: Text Builder + Ergonomics

- [ ] `builder Text` 定義
- [ ] `line()`, `blank()`, `indent()` 関数
- [ ] `TextNode` / `TextFrag` / `TextDoc` 型
- [ ] `render(TextDoc) -> String`
- [ ] `data-*` / `aria-*` 属性サポート
- [ ] `TrustedHtml` 型と raw 挿入 API

### Phase 4: Prompt Builder + Advanced

- [ ] `builder Prompt` 定義
- [ ] `PromptDoc` 型 / system/user/context ノード
- [ ] Codec 統合（`expect T using Json`）
- [ ] Custom element support（`element("my-widget") { ... }`）
- [ ] Source map（rendered → template source）

---

## Key Design Decisions

### なぜ汎用 builder か（旧設計との違い）

旧設計は `html {}` / `text {}` を特殊構文として parser に組み込んでいた。新設計は Swift Result Builder に倣い、**言語レベルの汎用変換機構** として実装する。

| | 旧設計 | 新設計 |
|---|---|---|
| HTML タグ | builder 内の contextual keyword | 普通の関数 |
| Element vs Expr の曖昧性 | lookahead で解消 | 存在しない（全部関数呼び出し） |
| 新しい builder の追加 | コンパイラ変更が必要 | ユーザーが `builder` で定義可能 |
| `html {}` の認識 | parser が contextual に認識 | `Html { }` は builder ブロック |
| known tags チェック | parser レベル | 型チェッカーレベル（関数の存在チェック） |

### なぜ `buildEither` を導入しないか

Swift の `buildEither(first/second)` は、if/else の各ブランチが異なる型を返せるようにするためにバイナリツリーを構築する。Almide はこれを導入しない:

- Almide の variant + match は Swift より強力で、分岐の型統一を自然に扱える
- `buildEither` のバイナリツリーは型が複雑になり、LLM が理解しにくい
- if/else の各ブランチは同じ Node 型を返す制約で十分

### trailing block の曖昧性解消

`f { ... }` が「関数 f にブロック式を渡す」か「関数 f に trailing builder block を渡す」かは、f のシグネチャで決まる:

- f の最後のパラメータ型が builder の Output 型 → trailing builder block
- それ以外 → 通常のブロック式（既存動作を維持）

既存コードの破壊はない（現在 builder Output 型を持つ関数は存在しない）。

### `lift` をオーバーロードではなく宣言的ルールにする理由

Almide にはオーバーロードがない。`lift` を関数にすると同名で異なるシグネチャの関数が必要になる。代わりに builder 定義内の `lift T => expr` 構文で型→変換の対応を宣言的に書く。コンパイラがこのルールを参照して型ごとのコードを生成する。

## Supersedes

旧 template.md（Typed Document Builder）を全面置換。`builder` 汎用機構の導入により、以下が不要になった:

- `BuilderItem::Element` AST（要素は関数呼び出しに統合）
- `BuilderItem::Text` / `Line` / `Blank`（関数に統合）
- `HtmlBuilder` / `TextBuilder` の AST 区別（汎用 `BuilderBlock` に統一）
- Element vs Expr の曖昧性解消ロジック（不要）
- `html_schema.toml` の tag/attr validation（関数シグネチャ + 型チェックで代替）
