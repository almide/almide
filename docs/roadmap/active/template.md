# Template: Typed Document Builder [ACTIVE]

## Vision

Trustworthy document boundaries for humans, programs, and models.

Template は「文字列テンプレート」ではなく、Codec と同じ思想で document boundary を扱う **typed document builder** である。

核心の設計思想: **Document は「文字列を組み立てるもの」ではなく「型付き構造を組み立てて最後に render するもの」。** LLM に HTML 文字列を書かせず、型付き template を書かせて render する。

```
typed record --encode--> external data format       (Codec)
typed record --template--> typed document --render--> external text/markup  (Template)
```

- **Codec** は「構造化データ境界」
- **Template** は「人間・UI・LLM 向け文書境界」

---

## Design Principles

1. **template は document builder であり string builder ではない** — 値は `HtmlDoc` / `TextDoc` であり `String` ではない
2. **template は Almide の普通のコードである** — Jinja/Handlebars 風のミニ言語ではない。`if / for / match` はホスト言語そのもの
3. **keyword を増やさない** — builder block 内の式は型で判別する。`emit` / `embed` のような専用 keyword は入れない
4. **partial は普通の関数** — コンポーネント再利用は `fn -> HtmlFrag` で表現。template 専用の partial/macro/inheritance 機構は持たない
5. **`html {}` / `text {}` は普通の式** — template 内でだけ使えるのではなく、`fn` 内でも `let` 内でもどこでも書ける
6. **Phase 1 は pure / non-async** — template はデータ取得後に呼ぶ。boundary builder に effect を混ぜない

---

## 1. template の言語上の身分

### Surface Syntax

`template` は `fn` を置き換える宣言キーワード。LLM にとって「この関数は document を組み立てている」という明確なシグナル。

```almide
template welcome(user: User) -> HtmlDoc = html {
  body {
    h1 { "Hello, "; user.name }
  }
}
```

`fn` ではなく `template` と書くことで:
- 役割が宣言的に明示される
- LLM が壊しにくい（構造境界のシグナルが強い）
- compiler が template 固有の lint / diagnostic を適用できる

### Semantic Model

内部的には `Fn` の一種として扱う。`Decl::Template` を別立てしない。

```rust
enum FnKind {
    Regular,
    Async,
    Effect,
    Template,
}

Fn {
    name: String,
    kind: FnKind,
    // ... 既存フィールド (generics, params, return_type, body, visibility, ...)
}
```

理由:
- generics, params, visibility, body, diagnostics が `Fn` と完全に共通
- 二重化を避ける
- symbol table 上は callable として統一的に扱える
- boolean soup (`is_effect / is_async / is_test / is_template`) を避ける

### template の制約 (Phase 1)

- **pure のみ** — `effect template` / `async template` は不許可
- **戻り値型は Doc 型のみ** — `HtmlDoc`, `TextDoc`（将来: `PromptDoc`）
- **body は自由** — `html {}` に限らず、任意の式が書ける（ただし最終的に Doc 型を返すこと）

```almide
// ✅ 直接 builder
template invoice(x: Invoice) -> HtmlDoc = html { ... }

// ✅ 条件分岐で異なる document を返す
template page(user: User) -> HtmlDoc =
  if user.is_admin {
    admin_page(user)
  } else {
    normal_page(user)
  }

// ✅ 関数合成
template page(user: User) -> HtmlDoc = layout(
  title: "Home",
  body: content(user),
)

// ❌ Compile error: template must return a document type
template bad(x: Invoice) -> String = "hello"

// ❌ Compile error: template cannot be effect
effect template bad(x: Invoice) -> HtmlDoc = html { ... }
```

### `template` と `fn` の使い分け

| 宣言 | 用途 | 戻り値型の制約 |
|------|------|--------------|
| `template` | 完全な document を組み立てる entry point | `HtmlDoc`, `TextDoc` のみ |
| `fn` | 再利用 fragment / helper / 任意の関数 | 制約なし（`HtmlFrag` も可） |

`fn` が `HtmlFrag` を返すことは許可される — builder ブロックは `fn` 内でも使える。
`template` は **意図の明示** であり、compiler はより厳密な lint を適用する（例: `template` は `Doc` 型のみ返せる）。

---

## 2. `html {}` / `text {}` は普通の式

### どこでも使える

`html {}` / `text {}` は template の中だけでなく、**式としてどこでも使える**。

```almide
// template の body として
template welcome(user: User) -> HtmlDoc = html { ... }

// fn の body として
fn sidebar(user: User) -> HtmlFrag = html {
  nav { ... }
}

// let 束縛の右辺として
let fragment = html {
  p { "Hello" }
}

// 関数引数として (inline fragment)
card("Profile", html {
  p { user.bio }
})
```

### Parser の扱い

`html` / `text` は **予約語にしない**。式パーサの primary 分岐で contextual に認識する。

```rust
// src/parser/primary.rs
fn parse_primary(&mut self) -> ParseResult<Expr> {
    if self.check_ident("html") && self.peek_is(TokenType::LBrace) {
        return self.parse_html_builder();
    }
    if self.check_ident("text") && self.peek_is(TokenType::LBrace) {
        return self.parse_text_builder();
    }
    // ... 既存の primary parsing
}
```

これにより:
- `let html = 42` は引き続き可能（`html` の直後が `{` でなければ普通の識別子）
- `html { ... }` は式としてどこでも書ける
- `template` は独立した宣言キーワード（builder とは別の話）

---

## 3. Builder Block — 型で判別、keyword 不要

### 核心ルール

builder block 内の式は **型で挙動が決まる**。専用 keyword は不要。

| 式の型 | 挙動 |
|--------|------|
| `String`, `Int`, `Float`, `Bool` | text node として挿入（auto escape） |
| `HtmlFrag`, `TextFrag` | 構造的に差し込む |
| `HtmlDoc`, `TextDoc` | compile error（Doc は embed できない。Frag を使え） |
| `Record`, `List`, その他 | compile error（field を選べ、または変換しろ） |

Almide はすでにこれをやっている。`++` は `String` と `List` で挙動が変わる。`==` は型で dispatch する。builder block も同じ原理。

```almide
template invoice_page(inv: Invoice) -> HtmlDoc = html {
  head {
    title { "Invoice #"; inv.id }
  }
  body {
    h1 { "Invoice #"; inv.id }

    p { "Hello, "; inv.customer; "." }

    table {
      tbody {
        for item in inv.items {
          item_row(item)              // HtmlFrag → 構造的に差し込まれる
        }
      }
    }

    p { a(href: inv.pay_url) { "Pay now" } }
  }
}
```

### 診断

型に基づく診断は十分に強い:

```
error: cannot insert User in builder block
  --> app.almd:5:5
   |
 5 |     user
   |     ^^^^ type User is not insertable
   |
   = hint: use a field like `user.name`, or convert with a function

error: cannot insert HtmlDoc in builder block
  --> app.almd:8:5
   |
 8 |     full_page(user)
   |     ^^^^^^^^^^^^^^^ HtmlDoc cannot be inserted into a fragment
   |
   = hint: this returns a complete document; use a function that returns HtmlFrag instead
```

### Builder Block Grammar

```
BuilderItem ::=
    StringLiteral                        // bare text: "Hello"
  | Element                              // html element (html {} only)
  | Line                                 // text line (text {} only)
  | 'blank'                              // blank line (text {} only)
  | Expr                                 // type-dispatched: text or fragment
  | 'if' Expr BuilderBlock ['else' BuilderBlock]
  | 'for' Pattern 'in' Expr BuilderBlock
  | 'match' Expr '{' BuilderArm* '}'
  | 'let' Pattern '=' Expr

BuilderBlock ::= '{' BuilderItem* '}'

Element ::= TagName ['(' AttrList ')'] BuilderBlock

Line ::= 'line' BuilderBlock

AttrList ::= Attr (',' Attr)*
Attr ::= AttrName ':' Expr
```

builder ブロック内に置けないもの:
- `var` (mutable binding)
- `Assign` / `FieldAssign` / `IndexAssign`
- `guard`
- `while`

---

## 4. html ブロックの要素モデル

### HTML タグは contextual element syntax

HTML タグ名は **言語全体のキーワードにはしない**。`html {}` builder の中でだけ意味を持つ。

```almide
// html {} の外では普通の識別子
let div = 42  // OK: div is just a variable name

template page(x: Page) -> HtmlDoc = html {
  div { "content" }  // OK: div is an HTML element here
}
```

### Phase 1: Known Tags Only

Phase 1 では既知の HTML タグのみ許可。typo はコンパイルエラー。

```almide
html {
  diiv { "oops" }
}
// error: unknown HTML element `diiv`; did you mean `div`?
```

**Known tags (Phase 1 subset):**

Document structure: `html`, `head`, `body`, `title`, `meta`, `link`

Sections: `header`, `footer`, `main`, `nav`, `section`, `article`, `aside`

Block: `div`, `p`, `h1`-`h6`, `pre`, `blockquote`, `hr`, `br`

Inline: `span`, `a`, `strong`, `em`, `code`, `small`, `sub`, `sup`, `abbr`, `time`

Lists: `ul`, `ol`, `li`, `dl`, `dt`, `dd`

Tables: `table`, `thead`, `tbody`, `tfoot`, `tr`, `th`, `td`, `caption`, `colgroup`, `col`

Forms: `form`, `input`, `button`, `select`, `option`, `optgroup`, `textarea`, `label`, `fieldset`, `legend`, `output`

Media: `img`, `video`, `audio`, `source`, `picture`, `figure`, `figcaption`, `canvas`, `svg`

Interactive: `details`, `summary`, `dialog`

**Phase 1 で除外** (安全性優先):
- `script` — JS 実行
- `style` element — CSS injection
- `iframe`, `object`, `embed` (HTML の embed 要素) — external content

### Typed Attributes

属性はタグごとに型を持つ。

```almide
a(href: user.profile_url) { "Profile" }     // href: Url
button(disabled: true) { "Submit" }          // disabled: Bool
div(class: "card highlight") { ... }         // class: String (Phase 1)
```

**Global attributes** は全タグで使用可能: `id`, `class`, `title`, `hidden`, `tabindex`, `lang`, `dir`

**Phase 1 で制限する global attributes:**
- `style` — 使用時にコンパイルエラー。将来 `TrustedCss` / sanitizer policy 導入後に開放

**`data-*` / `aria-*`** は prefix マッチで許可。値は `String`。

**`on*` イベントハンドラ属性は Phase 1 で全面禁止。**

### Component は普通の関数

再利用コンポーネントは `fn -> HtmlFrag` として定義。HTML タグと同じ構文空間には乗せない。

```almide
fn card(title: String, body: HtmlFrag) -> HtmlFrag = html {
  div(class: "card") {
    h2 { title }
    body
  }
}

template page(user: User) -> HtmlDoc = html {
  body {
    card("Profile", html {
      p { user.bio }
    })
  }
}
```

将来 custom element が必要になった場合は明示 form で追加:
```almide
element("my-widget", foo: x) { ... }
```

---

## 5. Document 型と IR

### Document Types

```
HtmlDoc   — 完全な HTML document (html root)
HtmlFrag  — HTML fragment (element or node list)
TextDoc   — 完全な text document
TextFrag  — text fragment

将来:
PromptDoc — structured prompt document
```

`HtmlDoc` と `HtmlFrag` の関係:
- `HtmlDoc` は `html {}` builder の最上位で生成
- `HtmlFrag` は element や node list として生成
- `HtmlFrag` は `HtmlDoc` / `HtmlFrag` に差し込める
- `HtmlDoc` は差し込めない（complete document は入れ子にならない）

### AST Representation

```rust
/// Builder block item — used inside html {} / text {} contexts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BuilderItem {
    /// Static text: "Hello, "
    Text { value: String, #[serde(skip)] span: Option<Span> },
    /// Expression: type-dispatched (text insertion or fragment embed)
    Expr { expr: Expr, #[serde(skip)] span: Option<Span> },
    /// HTML element: tag(attrs) { children }  (html {} only)
    Element {
        tag: String,
        attrs: Vec<BuilderAttr>,
        children: Vec<BuilderItem>,
        #[serde(skip)] span: Option<Span>,
    },
    /// text line: line { items }  (text {} only)
    Line { children: Vec<BuilderItem>, #[serde(skip)] span: Option<Span> },
    /// blank line  (text {} only)
    Blank { #[serde(skip)] span: Option<Span> },
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
    /// let binding
    Let { pattern: Pattern, value: Expr, #[serde(skip)] span: Option<Span> },
}
```

**設計判断:**
- `BuilderItem::Emit` / `BuilderItem::Embed` は不要。`BuilderItem::Expr` に統合
- checker が式の型を見て text insertion か fragment embed かを判別
- AST 段階では区別しない。IR lowering で分離する

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderAttr {
    pub name: String,
    pub value: Expr,
    #[serde(skip)]
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderMatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Vec<BuilderItem>,
}

// Expr に追加
pub enum Expr {
    // ... 既存 ...

    /// html { ... } builder block
    HtmlBuilder {
        children: Vec<BuilderItem>,
        #[serde(skip)] span: Option<Span>,
        #[serde(skip)] resolved_type: Option<ResolvedType>,
    },

    /// text { ... } builder block
    TextBuilder {
        children: Vec<BuilderItem>,
        #[serde(skip)] span: Option<Span>,
        #[serde(skip)] resolved_type: Option<ResolvedType>,
    },
}
```

### IR Representation

IR では checker の結果を受けて text / embed を明示的に分離する。

```rust
pub struct IrFunction {
    pub name: String,
    pub params: Vec<(VarId, Ty)>,
    pub ret_ty: Ty,
    pub body: IrExpr,
    pub kind: IrFnKind,
    pub is_test: bool,
}

pub enum IrFnKind { Regular, Async, Effect, Template }

pub enum DocKind { Html, Text }

/// IR builder node — type-dispatched by checker
pub enum IrBuilderNode {
    /// Emittable type (String, Int, etc.) → text node with auto-escape
    TextValue { expr: IrExpr, span: Option<Span> },
    /// Static string literal
    TextLiteral { value: String, span: Option<Span> },
    /// Fragment type (HtmlFrag, TextFrag) → structural embed
    Fragment { expr: IrExpr, span: Option<Span> },
    Element {
        tag: String,
        attrs: Vec<IrBuilderAttr>,
        children: Vec<IrBuilderNode>,
        span: Option<Span>,
    },
    Line { children: Vec<IrBuilderNode>, span: Option<Span> },
    Blank { span: Option<Span> },
    If {
        cond: IrExpr,
        then_nodes: Vec<IrBuilderNode>,
        else_nodes: Option<Vec<IrBuilderNode>>,
        span: Option<Span>,
    },
    ForIn {
        var: VarId,
        iterable: IrExpr,
        body: Vec<IrBuilderNode>,
        span: Option<Span>,
    },
    Match {
        subject: IrExpr,
        arms: Vec<(IrPattern, Vec<IrBuilderNode>)>,
        span: Option<Span>,
    },
    Let { var: VarId, value: IrExpr, span: Option<Span> },
}

pub struct IrBuilderAttr {
    pub name: String,
    pub value: IrExpr,
    pub expected_type: AttrType,
    pub span: Option<Span>,
}
```

**設計判断:**
- AST では `BuilderItem::Expr` (未分化)
- checker が型を解決
- IR では `TextValue` (Emittable) / `Fragment` (HtmlFrag/TextFrag) に分離
- codegen はこの区別を使って escape 有無を決定

### Runtime Representation (Rust codegen)

```rust
enum HtmlNode {
    Element {
        tag: HtmlTag,
        attrs: Vec<HtmlAttr>,
        children: Vec<HtmlNode>,
    },
    Text(String),          // 生テキスト。render 時に context-aware escape
    Trusted(TrustedHtml),  // 明示的に信頼済み (Phase 2)
}

struct HtmlAttr {
    name: HtmlAttrName,
    value: HtmlAttrValue,
}

enum HtmlAttrValue {
    Text(String),
    Bool(bool),
    Url(AlmideUrl),
    Tokens(Vec<String>),   // class (将来)
}
```

### Lowering Pipeline

```
Source (.almd)
  → AST (BuilderItem::Expr — type unknown)
  → Type Check (resolve types, validate tags/attrs)
  → IR (IrBuilderNode::TextValue or IrBuilderNode::Fragment — type-dispatched)
  → Rust/TS codegen (HtmlNode tree construction + render call)
```

### Context-Aware Escaping

text insertion は挿入先の文脈に応じて自動 escape:
- **HTML text node**: `<`, `>`, `&`, `"` を entity escape
- **HTML attribute value**: 同上 + context-specific
- **Text document**: escape なし（plain text）

raw HTML の挿入は `TrustedHtml` 型を経由する明示 API のみ (Phase 2):

```almide
fn trusted_html(s: String) -> TrustedHtml

html {
  div { trusted_html(sanitized_content) }
}
```

---

## 6. text {} Builder

HTML と同じ思想で plain text document を構築。

```almide
template receipt(x: Receipt) -> TextDoc = text {
  line { "Invoice "; x.invoice_id }
  line { "Customer: "; x.customer_name }
  blank
  line { "Total: "; money(x.total) }
}
```

### text 固有の要素

- `line { ... }` — 一行のテキスト（末尾に改行）
- `blank` — 空行
- 将来: `indent { ... }` — インデントブロック

### Whitespace Policy

- `html {}` ではレイアウト用空白は text node にしない
- `text {}` では空白・改行を明示的に扱う（`line`, `blank`）

---

## 7. HTML Schema — TOML + Codegen

HTML タグ・属性の定義は **`stdlib/defs/html_schema.toml` に宣言し、`build.rs` で `src/generated/html_schema.rs` を生成する**。stdlib と同じパターン。

### TOML 定義

```toml
# stdlib/defs/html_schema.toml

[global_attrs]
attrs = [
  { name = "id",       type = "String" },
  { name = "class",    type = "String" },
  { name = "title",    type = "String" },
  { name = "hidden",   type = "Bool" },
  { name = "tabindex", type = "Int" },
  { name = "lang",     type = "String" },
  { name = "dir",      type = "String" },
]

restricted_attrs = [
  { name = "style", reason = "use typed styles (future)" },
]

prefix_attrs = ["data-", "aria-"]
banned_attr_prefixes = ["on"]

[elements.div]
children = "flow"

[elements.a]
children = "phrasing"
attrs = [
  { name = "href",     type = "Url" },
  { name = "target",   type = "String" },
  { name = "rel",      type = "String" },
  { name = "download", type = "String" },
]

[elements.img]
children = "void"
attrs = [
  { name = "src",    type = "Url" },
  { name = "alt",    type = "String" },
  { name = "width",  type = "Int" },
  { name = "height", type = "Int" },
]

# ... 全タグ定義
```

### 生成されるコード

```rust
// src/generated/html_schema.rs — DO NOT EDIT

pub struct HtmlElementInfo {
    pub children_model: ChildrenModel,
    pub own_attrs: &'static [(&'static str, AttrType)],
}

pub enum ChildrenModel { Flow, Phrasing, Metadata, TextOnly, Void, TableContent, TableCells }
pub enum AttrType { String, Bool, Int, Url }

pub fn html_elements() -> HashMap<&'static str, HtmlElementInfo> { ... }
pub fn global_attrs() -> &'static [(&'static str, AttrType)] { ... }
pub fn is_prefix_attr(name: &str) -> bool { ... }
pub fn is_banned_attr(name: &str) -> bool { ... }
pub fn is_restricted_attr(name: &str) -> Option<&'static str> { ... }
pub fn suggest_element(typo: &str) -> Option<&'static str> { ... }
pub fn suggest_attr(element: &str, typo: &str) -> Option<&'static str> { ... }
```

---

## 8. Keywords

### 予約語として追加

| keyword | tokens.toml category | 理由 |
|---------|---------------------|------|
| `template` | declaration | `fn` を置き換える宣言キーワード。唯一の追加 keyword |

### 予約語にしないもの

| 識別子 | 理由 |
|--------|------|
| `html` | builder expr の開始。contextual に認識。変数名として使える |
| `text` | 同上 |
| `line` | text builder 内でのみ意味を持つ。外では普通の識別子 |
| `blank` | 同上 |
| HTML タグ名 | builder 内でのみ意味を持つ。外では普通の識別子 |

**追加する予約語は `template` の 1 つだけ。**

---

## 9. Parser Integration

### template 宣言の parse

```rust
// src/parser/declarations.rs — parse_top_decl 内
if self.check(TokenType::Template) {
    return self.parse_template_decl(visibility);
}
```

`parse_template_decl` は `parse_fn_decl` とほぼ同じ:
- `template` keyword を consume（`fn` の代わり）
- 名前、generics、params、return type を parse（同じロジック）
- body は **`fn` と同じ式パーサ** を使う（builder に限定しない）
- effect / async modifier は禁止

### builder 式の parse

```rust
// src/parser/primary.rs — parse_primary 内
if self.check_ident("html") && self.peek_is(TokenType::LBrace) {
    return self.parse_html_builder();
}
if self.check_ident("text") && self.peek_is(TokenType::LBrace) {
    return self.parse_text_builder();
}
```

`parse_html_builder` — BuilderItem dispatch:
- `StringLiteral` → `BuilderItem::Text`
- `if` → `BuilderItem::If`
- `for` → `BuilderItem::ForIn` (with `Pattern`)
- `match` → `BuilderItem::Match`
- `let` → `BuilderItem::Let` (with `Pattern`)
- `Ident` → 曖昧性解消ルール（下記）で Element か Expr を判定
- その他の式 → `BuilderItem::Expr` (checker が型で判別)

### Element vs Expr の曖昧性解消

builder block 内で `Ident` が出現したとき、Element（HTML タグ）か Expr（関数呼び出し等）かを判定する必要がある。
`header`, `nav`, `main`, `section`, `code`, `time`, `output` 等は known HTML tag でありながらユーザー定義関数名としても自然に使われる。

**解消ルール:**

```
Ident が known HTML tag のとき:
  1. 直後が `{`                              → Element
  2. 直後が `(` で、中が `Ident ":"` パターン → Element (named attrs)
  3. 直後が `(` で、中がそれ以外              → Expr (関数呼び出し)
  4. それ以外                                → Expr

Ident が known HTML tag でないとき:
  → Expr
```

**具体例:**

```almide
html {
  body {
    // Element: div は known tag、直後が {
    div { "hello" }

    // Element: header は known tag、直後が ( で中が named attr
    header(class: "top") { "title" }

    // Expr: header は known tag だが、直後が ( で中が positional arg → 関数呼び出し
    header("Welcome")

    // Expr: sidebar は known tag じゃない → 式として parse
    sidebar(user)

    // Expr: user も known tag じゃない → 式として parse
    user.name

    // Element: a は known tag、直後が ( で中が named attr
    a(href: url) { "click" }
  }
}
```

**実装:**

```rust
fn parse_builder_item_ident(&mut self) -> ParseResult<BuilderItem> {
    let name = self.current().value.clone();
    let is_tag = is_known_html_tag(&name);

    if is_tag {
        // Case 1: tag { ... }
        if self.peek_is(TokenType::LBrace) {
            return self.parse_element(name);
        }

        // Case 2 vs 3: tag( ... )
        if self.peek_is(TokenType::LParen) {
            // lookahead: ( Ident : → Element, otherwise → Expr
            if self.lookahead_is_named_attr() {
                return self.parse_element(name);
            }
            // fall through to Expr
        }
    }

    // Default: parse as expression
    let expr = self.parse_expr()?;
    Ok(BuilderItem::Expr { expr, span })
}

fn lookahead_is_named_attr(&self) -> bool {
    // current = Ident (tag name)
    // peek(0) = LParen
    // peek(1) = Ident or RParen
    // peek(2) = Colon (if named attr)
    //
    // tag()         → RParen at peek(1) → Element (void attrs)
    // tag(name: ..) → Ident + Colon → Element
    // tag(expr)     → Ident + not Colon, or non-Ident → Expr

    let after_paren = self.peek(1);
    if after_paren.token_type == TokenType::RParen {
        return true; // tag() — empty attrs, still Element
    }
    if after_paren.token_type == TokenType::Ident {
        let after_ident = self.peek(2);
        return after_ident.token_type == TokenType::Colon;
    }
    false
}
```

**text builder 内の追加ルール:**

1. `Ident("line")` で直後が `{` → `BuilderItem::Line`
2. `Ident("blank")` → `BuilderItem::Blank`
3. `Ident` が known HTML tag → error: `HTML elements are only valid in 'html {}' blocks`
4. それ以外 → Expr

---

## 10. Diagnostics

### Phase 1 で出す診断

| 状況 | メッセージ |
|------|-----------|
| 不明なタグ | `unknown HTML element 'diiv'; did you mean 'div'?` |
| 属性型不一致 | `attribute 'href' expects Url, got String` |
| insertable でない型 | `cannot insert User in builder block; use a field like user.name` |
| Doc を差し込もうとした | `HtmlDoc cannot be inserted; use a function that returns HtmlFrag` |
| 不明な属性 | `unknown attribute 'hreef' on element 'a'; did you mean 'href'?` |
| effect template | `template cannot be effect; fetch data before calling the template` |
| template returns non-Doc | `template must return a document type (HtmlDoc, TextDoc), got String` |
| banned element | `element 'script' is not allowed (security)` |
| banned attr | `event handler attributes (onclick, ...) are not allowed` |
| restricted attr | `attribute 'style' is restricted; use typed styles (future)` |
| Element in text {} | `HTML elements are only valid in 'html {}' blocks` |
| Line in html {} | `'line' is only valid in 'text {}' blocks` |

---

## 11. Complete Examples

### HTML Email

```almide
type InvoiceEmail = {
  invoice_id: String
  customer_name: String
  pay_url: Url
  items: List[LineItem]
}

type LineItem = { name: String, price: Int }

fn money(n: Int) -> String = "$${int.to_string(n / 100)}.${int.to_string(n % 100)}"

fn price_row(item: LineItem) -> HtmlFrag = html {
  tr {
    td { item.name }
    td(class: "price") { money(item.price) }
  }
}

template invoice_email(x: InvoiceEmail) -> HtmlDoc = html {
  head {
    title { "Invoice #"; x.invoice_id }
  }
  body {
    h1 { "Invoice #"; x.invoice_id }

    p { "Hello, "; x.customer_name; "." }

    table {
      thead {
        tr { th { "Item" }; th { "Price" } }
      }
      tbody {
        for item in x.items {
          price_row(item)
        }
      }
    }

    p { a(href: x.pay_url) { "Pay now" } }
  }
}
```

### Conditional / Match

```almide
type User = { name: String, role: Role, verified: Bool, joined: String }
type Role = | Admin | Member | Guest

fn badge(text: String, color: String) -> HtmlFrag = html {
  span(class: "badge badge-${color}") { text }
}

template user_card(user: User) -> HtmlDoc = html {
  body {
    div(class: "card") {
      h2 { user.name }

      if user.verified {
        badge("Verified", "green")
      } else {
        badge("Unverified", "gray")
      }

      match user.role {
        Admin => p { "Administrator" },
        Member => p { "Member since "; user.joined },
        Guest => p { "Guest — please sign up" },
      }
    }
  }
}
```

### Composition

```almide
fn admin_page(user: User) -> HtmlDoc = html {
  body {
    h1 { "Admin: "; user.name }
  }
}

fn normal_page(user: User) -> HtmlDoc = html {
  body {
    h1 { "Welcome, "; user.name }
  }
}

template page(user: User) -> HtmlDoc =
  match user.role {
    Admin => admin_page(user),
    _ => normal_page(user),
  }
```

### Layout Pattern

```almide
fn page_shell(title: String, content: HtmlFrag) -> HtmlFrag = html {
  head {
    title { title }
    meta(charset: "utf-8")
  }
  body {
    header { h1 { title } }
    main { content }
    footer { "© 2026 Almide" }
  }
}

fn card(title: String, body: HtmlFrag) -> HtmlFrag = html {
  div(class: "card") {
    h2 { title }
    div(class: "card-body") { body }
  }
}

template dashboard(user: User, items: List[Item]) -> HtmlDoc = html {
  page_shell("Dashboard", html {
    card("Profile", html {
      p { user.name }
      p { user.email }
    })

    card("Items", html {
      ul {
        for item in items {
          li { item.name; " — "; money(item.price) }
        }
      }
    })
  })
}
```

### Pipe Chain in Builder

```almide
template catalog(products: List[Product]) -> HtmlDoc = html {
  body {
    h1 { "Catalog" }

    let available = products
      |> list.filter(fn(p) => p.in_stock)
      |> list.sort_by(fn(p) => p.price)

    div(class: "grid") {
      for p in available {
        product_card(p)
      }
    }

    p { int.to_string(list.len(available)); " products available" }
  }
}
```

### Text Document

```almide
template receipt(x: Receipt) -> TextDoc = text {
  line { "Receipt #"; x.id }
  line { "Customer: "; x.customer_name }
  blank
  for item in x.items {
    line { "  "; item.name; " — "; money(item.price) }
  }
  blank
  line { "Total: "; money(x.total) }
}
```

### Variant + Record + Template

```almide
type Notification =
  | Success { message: String }
  | Warning { message: String, detail: Option[String] }
  | Error { message: String, code: Int }

fn notification_class(n: Notification) -> String = match n {
  Success { .. } => "alert-success",
  Warning { .. } => "alert-warning",
  Error { .. } => "alert-danger",
}

fn notification_frag(n: Notification) -> HtmlFrag = html {
  div(class: "alert ${notification_class(n)}") {
    match n {
      Success { message } => p { message },
      Warning { message, detail } => {
        p { message }
        match detail {
          some(d) => small { d },
          none => {},
        }
      },
      Error { message, code } => {
        p { message }
        small { "Error code: "; int.to_string(code) }
      },
    }
  }
}

template alerts(notifications: List[Notification]) -> HtmlDoc = html {
  body {
    h1 { "Notifications" }
    for n in notifications {
      notification_frag(n)
    }
  }
}
```

### Testing

```almide
template greeting(name: String) -> HtmlDoc = html {
  body { h1 { "Hello, "; name } }
}

test "greeting renders correctly" {
  let doc = greeting("Alice")
  let s = render(doc)
  assert(string.contains(s, "Hello, Alice"))
  assert(string.contains(s, "<h1>"))
}

test "greeting escapes HTML in name" {
  let doc = greeting("<script>alert(1)</script>")
  let s = render(doc)
  assert(string.contains(s, "&lt;script&gt;"))
  assert(not(string.contains(s, "<script>")))
}
```

---

## 12. Codec / Schema との接続 (将来)

Template は Codec と並行する boundary 機構。将来の統合ポイント:

### PromptDoc (Phase 3)

```almide
template support_prompt(input: SupportInput) -> PromptDoc = prompt {
  system {
    "You are a support assistant."
    expect SupportReply using Json
  }

  user {
    input.message
  }

  context("order") {
    record input.order encode using Json
  }
}
```

### Template 入力と Codec

Template の入力は普通の typed record。その record が `deriving Codec` を持てば、
同じ型定義から template rendering と JSON encode/decode の両方が使える。

---

## 13. 同じコアでカバーするもの / 分けるべきもの

### Template core でカバー

- HTML / email HTML
- plain text / email text
- 将来の prompt

### Template にしないもの

- **JSON** — record → Codec の世界
- **SQL** — parameterized query / query AST の世界
- **コード生成** — 言語別 AST / quasiquote / pretty printer

---

## 14. Phase Roadmap

### Phase 1: Template Core

- `template` keyword (唯一の追加予約語)
- `FnKind` enum (replacing boolean soup)
- `HtmlDoc`, `HtmlFrag`, `TextDoc`, `TextFrag` 型
- `html {}`, `text {}` builder expression (usable anywhere)
- `BuilderItem` AST — type-dispatched (keyword なし)
- `IrBuilderNode` with `TextValue` / `Fragment` distinction, spans preserved
- Known HTML tags + typed attributes (`html_schema.toml` + codegen)
- `script` / `style` element banned, `on*` / `style` attr banned
- Default HTML escaping (context-aware)
- `render(doc) -> String`
- Diagnostics (tag typo, attribute type, type-based insertion errors)

### Phase 2: Safety & Ergonomics

- `TrustedHtml` 型と raw 挿入 API
- `Url` 型の attribute 統合
- `on*` 属性の段階的開放
- `style` attribute を `TrustedCss` 経由で開放
- Whitespace policy options
- Template-specific diagnostics 強化

### Phase 3: PromptDoc & Codec Integration

- `PromptDoc` 型 / `prompt {}` builder
- `expect T using Json` — schema 接続
- structured context 埋め込み
- Email renderer

### Phase 4: Advanced

- Document path / repair / validate / introspection
- Source map (rendered → template source)
- Sanitizer pipeline
- Custom element support
- Localization / i18n

---

## Key Design Decisions

### keyword を増やさない

- builder block 内の式は **型で挙動が決まる**
- `emit` / `embed` のような専用 keyword は不要
- 追加する予約語は `template` の 1 つだけ
- `html` / `text` / `line` / `blank` / HTML タグ名は全て contextual

### template vs fn

- `template` は `fn` を置き換える宣言 keyword
- 内部は `FnKind::Template`
- `template` → `Doc` 型のみ。body は自由
- `fn` → 任意の型。builder 式は `fn` 内でも使える

### 型 dispatch

- `String`, `Int`, `Float`, `Bool` → text node (auto escape)
- `HtmlFrag`, `TextFrag` → structural embed
- `HtmlDoc`, `TextDoc` → compile error
- `Record`, `List`, etc. → compile error

### `html {}` / `text {}` は普通の式

- どこでも書ける
- contextual に認識（予約語にしない）

### HTML tag model

- builder 文脈専用。global keyword にしない
- Phase 1 は known tags only
- `script` / `style` / `on*` は禁止
- Component は `fn -> HtmlFrag`
- Element vs Expr の曖昧性は lookahead で解消: `tag {` → Element, `tag(name: ...)` → Element, `tag(expr)` → Expr

### AST → IR の分離

- AST: `BuilderItem::Expr` (未分化)
- IR: `IrBuilderNode::TextValue` / `IrBuilderNode::Fragment` (type-dispatched)
- codegen はこの区別で escape 有無を決定

### HTML schema

- `stdlib/defs/html_schema.toml` + `build.rs` 生成
- "did you mean?" 診断付き

## Supersedes

This is a new roadmap item. No prior documents superseded.
