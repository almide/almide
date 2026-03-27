<!-- description: Swift-style result builder DSL for structured data construction -->
<!-- done: 2026-03-18 -->
# Result Builder

## Vision

Introduce a **general-purpose builder mechanism** into Almide, following the same philosophy as Swift's Result Builder. `builder` is a language core feature; Html / Text / Csv / Prompt are all builder instances (stdlib). HTML elements are not special syntax within the builder — they are **ordinary functions**.

```
builder (language core)
├── builder Html { ... }     ← stdlib
│   ├── fn div(...)          ← ordinary function (TOML-generated)
│   ├── fn h1(...)           ← ordinary function (TOML-generated)
│   └── fn a(...)            ← ordinary function (TOML-generated)
├── builder Text { ... }     ← stdlib
├── builder Csv { ... }      ← user-definable
└── builder Prompt { ... }   ← future
```

---

## Design Principles

1. **builder is a general-purpose language mechanism** — not Html-specific. Users can define their own builders
2. **Elements are ordinary functions** — HTML tag names are not special syntax. `div`, `h1` are stdlib functions
3. **builder inference** — when a function parameter's type is a builder's Output type, automatically apply builder transformation to trailing blocks
4. **lift is a declarative rule** — instead of overloading, enumerate type→Node conversion rules within the builder definition
5. **template is convenience** — the `template` keyword explicitly declares "this function returns a document". Internally a variant of `fn`
6. **Only 2 keywords added** — `builder` (for definition) and `template` (for declaration)

---

## 1. `builder` Declaration

### Syntax

```almide
builder Html {
  type Node = HtmlNode
  type Output = HtmlFrag

  // lift rules — type-to-Node conversion
  lift String => text_node(escape_html(value))
  lift Int => text_node(int.to_string(value))
  lift Float => text_node(float.to_string(value))
  lift Bool => text_node(bool.to_string(value))
  lift HtmlFrag => embed(value)

  // combinator functions
  fn block(items: List[HtmlNode]) -> HtmlFrag = fragment(items)
  fn optional(node: Option[HtmlNode]) -> HtmlNode =
    match node { some(n) => n, none => empty_node() }
  fn array(nodes: List[HtmlNode]) -> HtmlNode = fragment_node(nodes)
}
```

### Required Members

| Member | Role | Swift Equivalent |
|--------|------|-----------------|
| `type Node` | Intermediate representation type | `Component` |
| `type Output` | Return type of the entire builder block | `FinalResult` |
| `lift T => expr` | Rule to convert an expression to a Node (one or more) | `buildExpression` |
| `fn block(List[Node]) -> Output` | Combine multiple Nodes into Output | `buildBlock` |

### Optional Members

| Member | Role | Swift Equivalent | When Undefined |
|--------|------|-----------------|----------------|
| `fn optional(Option[Node]) -> Node` | Handle `if` without else | `buildOptional` | `if` without else is a compile error |
| `fn array(List[Node]) -> Node` | Handle `for` loops | `buildArray` | `for` is a compile error |

`buildEither(first/second)` is not introduced. Since Almide has variants + match, if/else branches can return the same Node type and be handled by `block`.

### Builder Constraints

- Builder definitions are top-level only (cannot define inside functions)
- `type Output` is unique per builder (cannot associate multiple builders with one Output type)
- `lift` rules cannot be duplicated (defining two lifts for the same type is a compile error)
- Builders are not values (cannot be assigned to variables or passed to functions)

---

## 2. Compiler Transformation Rules

Transform each construct inside a builder block `BuilderName { ... }` into builder method calls.

### Expression Statement → lift

```almide
// User writes
Html { "Hello" }

// Compiler transforms to
Html.block([Html.lift_String("Hello")])
// i.e.
Html.block([text_node(escape_html("Hello"))])
```

Selects the corresponding `lift` rule based on the expression's type. Compile error if no matching `lift`.

### Multiple Expressions → block

```almide
Html {
  "Hello"
  user.name
  sidebar(user)
}

// Transforms to
Html.block([
  Html.lift_String("Hello"),
  Html.lift_String(user.name),
  Html.lift_HtmlFrag(sidebar(user)),
])
```

Each expression is converted to a Node via `lift`, then combined with `block`.

### if/else → optional or natural branching

```almide
// No else → optional
Html {
  if show_title {
    h1 { "Title" }
  }
}

// Transforms to
Html.block([
  Html.optional(
    if show_title { some(Html.lift_HtmlFrag(h1(...))) }
    else { none }
  ),
])
```

```almide
// With else → lift both branches and put in block
Html {
  if is_admin {
    admin_badge()
  } else {
    guest_badge()
  }
}

// Transforms — if/else is a regular expression. No lift needed if result type is Node
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

// Transforms to
Html.block([
  Html.array(
    items |> list.map((item) =>
      Html.lift_HtmlFrag(li(Html.block([Html.lift_String(item.name)])))
    )
  ),
])
```

### match → regular expression

```almide
Html {
  match user.role {
    Admin => admin_panel(user),
    Member => member_panel(user),
    Guest => guest_panel(user),
  }
}

// Transforms — match stays as-is. Each branch's result is lifted
Html.block([
  match user.role {
    Admin => Html.lift_HtmlFrag(admin_panel(user)),
    Member => Html.lift_HtmlFrag(member_panel(user)),
    Guest => Html.lift_HtmlFrag(guest_panel(user)),
  },
])
```

### let → no transformation

```almide
Html {
  let filtered = items |> list.filter((x) => x.active)
  for item in filtered {
    li { item.name }
  }
}

// let stays as-is. Subsequent expressions are transformation targets
let filtered = items |> list.filter((x) => x.active)
Html.block([
  Html.array(filtered |> list.map((item) => ...))
])
```

### Transformation Summary

| User Writes | Compiler Generates |
|-------------|-------------------|
| Expression (type T) | `Builder.lift_T(expr)` |
| `{ item1; item2; ... }` | `Builder.block([v1, v2, ...])` |
| `if cond { ... }` (no else) | `Builder.optional(if cond { some(v) } else { none })` |
| `if cond { ... } else { ... }` | `if cond { lift(v1) } else { lift(v2) }` |
| `for x in xs { ... }` | `Builder.array(xs \|> list.map((x) => ...))` |
| `match expr { ... }` | `match expr { pat => lift(v), ... }` |
| `let x = expr` | As-is (no transformation) |

### Not Allowed Inside Builder Blocks

- `var` (mutable binding)
- Assignment (`=`, `.field =`, `[i] =`)
- `while`
- `guard`
- `return` (same as Swift — results are implicitly collected)

---

## 3. Builder Inference and Trailing Block

### Builder Inference

When a function parameter's type is a builder's `Output` type, automatically apply builder transformation to `{ }` blocks passed as arguments.

```almide
// Definition of div
fn div(class: String = "", children: HtmlFrag) -> HtmlNode

// HtmlFrag is Html.Output
// So Html builder is automatically applied to { } passed to children
```

The compiler maintains an `Output → Builder` registry:

```
HtmlFrag → Html
TextFrag → Text
CsvDoc   → Csv
```

Constraint: at most one builder per Output type.

### Trailing Block Syntax

**When the last argument of a function call is a builder's Output type, it can be passed via trailing `{ }`.**

```almide
// The following are all equivalent
div(class: "card", children = Html { p { "hello" } })
div(class: "card") { p { "hello" } }      // ← trailing block (recommended)

// Without attributes
div(children = Html { p { "hello" } })
div { p { "hello" } }                     // ← trailing block (recommended)
```

**Parser rules:**

```
CallExpr ::= Expr '(' ArgList? ')' TrailingBlock?
TrailingBlock ::= '{' BuilderItem* '}'
```

When a trailing block is present:
1. Resolve the called function's signature
2. Check if the last parameter's type is a builder's Output type
3. Identify the builder corresponding to the Output type
4. Apply builder transformation to `{ }` contents and pass as the last parameter

Ambiguity between `{ }` without trailing block being a block expression or builder block:
- Immediately after function call → trailing block
- In the form `BuilderName { }` → builder block
- Otherwise → regular block expression

### Trailing Block Nesting

This mechanism enables natural nesting like HTML:

```almide
Html {                                    // ← Html builder explicit
  div(class: "card") {                    // ← trailing block, Html inferred
    h2 { "Title" }                        // ← trailing block, Html inferred
    p { "Content" }                       // ← trailing block, Html inferred
    ul {                                  // ← trailing block, Html inferred
      for item in items {
        li { item.name }                  // ← trailing block, Html inferred
      }
    }
  }
}
```

Only the top level requires `Html { }`. All subsequent function calls are automatically transformed via trailing block + builder inference. No nested `Html { }` needed.

---

## 4. `template` Keyword

A declaration keyword that replaces `fn`. Explicitly communicates the intent "this function assembles a document".

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

Internally a variant of `Fn`:

```rust
pub enum FnKind {
    Regular,
    Async,
    Effect,
    Template,
}
```

### Template Constraints

- **Pure only** — `effect template` is not allowed
- **Return type must be a Doc type** — `HtmlDoc`, `TextDoc`, etc.
- **Body is unrestricted** — not limited to builder blocks; can return different Docs via `if` / `match`

```almide
// ✅ Direct builder
template page(user: User) -> HtmlDoc = Html { body { ... } }

// ✅ Conditional branching
template page(user: User) -> HtmlDoc =
  if user.is_admin { admin_page(user) }
  else { normal_page(user) }

// ❌ template must return a document type
template bad() -> String = "hello"

// ❌ template cannot be effect
effect template bad() -> HtmlDoc = Html { ... }
```

### template vs fn

| Declaration | Use Case | Return Type Constraint |
|-------------|----------|----------------------|
| `template` | Entry point for a complete document | Doc types only |
| `fn` | fragment / helper / any function | No constraint |

`fn` is allowed to return a builder's Output type:

```almide
fn sidebar(user: User) -> HtmlFrag = Html {
  nav { a(href: "/") { "Home" } }
}
```

---

## 5. Html Builder (stdlib)

### Builder Definition

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

### HtmlDoc and HtmlFrag

```
HtmlDoc  — Complete HTML document (<html> root)
HtmlFrag — HTML fragment (element or node list)
```

- `HtmlFrag` is `Html.Output`
- `HtmlDoc` is `HtmlFrag` converted to a complete document via `render`
- `HtmlFrag` can be inserted into other `HtmlFrag` (handled by lift rules)
- `HtmlDoc` cannot be inserted (no lift rule → compile error)

Conversion to `HtmlDoc`:

```almide
fn to_doc(frag: HtmlFrag) -> HtmlDoc     // wrap frag into a complete HTML document
fn render(doc: HtmlDoc) -> String         // serialize to string
fn render_frag(frag: HtmlFrag) -> String  // serialize fragment only
```

---

## 6. HTML Elements — Ordinary Functions

### Design

HTML elements are defined as **stdlib functions**. They have no knowledge of the builder mechanism.

Each element function has:
- Element-specific named attributes (with default values)
- Global attributes (shared across all elements, with default values)
- `children: HtmlFrag` (last parameter, passed via trailing block)

```almide
// Examples of generated function signatures

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

// Images (void element — no children)
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

### Usage

```almide
Html {
  div(class: "card") {                      // named arg + trailing block
    h1 { "Hello" }                          // trailing block only
    p(class: "subtitle") { "Welcome" }
    a(href: "/home", class: "btn") { "Go" }
    img(src: "/photo.jpg", alt: "Photo")    // void element, no trailing block
    input(type_: "text", placeholder: "Search...")
  }
}
```

### Components — Same Pattern

User-defined components can also use trailing blocks by placing `children: HtmlFrag` as the last parameter:

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

// Usage — same syntax as HTML elements
Html {
  card(title: "Profile") {
    p { user.name }
    badge("Admin", "green")
  }
}
```

### Void Element Handling

`img`, `input`, `br`, `hr`, `meta`, `link`, etc. do not have a `children` parameter:

```almide
fn img(src: String, alt: String = "") -> HtmlNode          // no children
fn br() -> HtmlNode                                         // no arguments
fn input(type_: String = "text", name: String = "") -> HtmlNode
fn meta(charset: String = "", name: String = "", content: String = "") -> HtmlNode
```

Passing a trailing block produces a compile error:

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

## 7. HTML Elements — TOML-Driven Generation

### TOML Definition

```toml
# stdlib/defs/html_elements.toml

# Global attributes (applied to all elements)
[global]
attrs = [
  { name = "id",       type = "String", default = "\"\"" },
  { name = "class",    type = "String", default = "\"\"" },
  { name = "title",    type = "String", default = "\"\"" },
  { name = "hidden",   type = "Bool",   default = "false" },
  { name = "tabindex", type = "Int",    default = "0" },
  { name = "lang",     type = "String", default = "\"\"" },
]

# data-* / aria-* handled separately (Phase 2)

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

### Generation via build.rs

`build.rs` reads `html_elements.toml` and generates:

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

What gets generated:
- **For the type checker**: signature information for each element function (parameter names, types, default values)
- **For codegen**: element name → HTML tag name mapping (`type_` → `type`)
- **For diagnostics**: typo suggestions, banned element checks

---

## 8. Text Builder (stdlib)

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

// Text-specific functions
fn line(children: TextFrag) -> TextNode      // One line (trailing newline)
fn blank() -> TextNode                       // Empty line
fn indent(level: Int = 1, children: TextFrag) -> TextNode  // Indentation

// Usage
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

## 9. AST Representation

Since the builder mechanism is general-purpose, the AST does not distinguish between Html/Text.

```rust
// Added to Expr
pub enum Expr {
    // ... existing ...

    /// BuilderName { items }
    /// Explicit builder block
    BuilderBlock {
        builder_name: String,              // "Html", "Text", "Csv", ...
        items: Vec<BuilderItem>,
        #[serde(skip)] span: Option<Span>,
    },
}

/// Items inside a builder block
pub enum BuilderItem {
    /// Expression (lift target)
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
    /// let binding (no transformation)
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

Differences from the old design:
- Removed `BuilderItem::Element` — elements are ordinary function calls (included in `BuilderItem::Expr`)
- Removed `BuilderItem::Text` — string literals are also unified into `BuilderItem::Expr`
- Removed `BuilderItem::Line` / `BuilderItem::Blank` — `line()`, `blank()` are ordinary functions
- Removed `HtmlBuilder` / `TextBuilder` distinction — unified into generic `BuilderBlock`

### Trailing Block AST

Function calls with trailing blocks have them inserted by the parser as the last argument:

```almide
div(class: "card") { p { "hello" } }
```

```rust
// AST: div(class: "card", <trailing_block>)
Expr::Call {
    target: "div",
    args: [
        NamedArg("class", Expr::Lit("card")),
        // trailing block is inserted as the last argument
        // At this point it's TrailingBlock, not BuilderBlock
        Expr::TrailingBlock {
            items: [
                BuilderItem::Expr { expr: Call("p", [TrailingBlock { ... }]) }
            ]
        }
    ],
}
```

The checker resolves `div`'s signature, identifies the builder from the last parameter `children: HtmlFrag`'s type, and applies the transformation.

---

## 10. IR Representation

In the IR, builder transformation is complete and lifts are resolved.

```rust
pub enum IrExprKind {
    // ... existing ...

    /// Builder's block call (transformed)
    BuilderBlock {
        builder: BuilderRef,          // which builder
        items: Vec<IrBuilderNode>,    // sequence of lifted nodes
    },
}

/// IR builder node — lifts resolved
pub enum IrBuilderNode {
    /// Lifted expression → Node
    Lifted {
        lift_rule: LiftRuleRef,       // which lift rule was used
        expr: IrExpr,                 // original expression
        span: Option<Span>,
    },
    /// if (no else) → optional
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
    /// if/else, match → regular conditional expression (result is Node type)
    Conditional {
        expr: IrExpr,                 // if/match expression (result is Node)
        span: Option<Span>,
    },
    /// let binding
    Let {
        var: VarId,
        value: IrExpr,
        span: Option<Span>,
    },
}
```

---

## 11. Type Checking

### Builder Resolution

1. Find `BuilderBlock { builder_name: "Html", items }`
2. Look up the `Html` builder definition
3. Infer the type of each `BuilderItem::Expr`
4. Search for a `lift` rule matching the type
5. Compile error if no match
6. Lift all items → verify they match the `block` argument type

### Trailing Block Resolution

1. `CallExpr` has a `TrailingBlock` attached
2. Get the called function's last parameter type
3. Check in the registry if that type is a builder's Output type
4. If it matches, identify the builder and convert `TrailingBlock` to `BuilderBlock`
5. If no match, error: `trailing block requires a builder output type`

### Lift Rule Type Checking

```
// lift rule for builder Html
lift String => text_node(escape_html(value))

// Compiler verification:
// 1. `value` is bound with type `String`
// 2. Verify that the type of `text_node(escape_html(value))` is `HtmlNode` (= Html.Node)
// 3. Error if mismatch
```

### Error Messages

| Situation | Message |
|-----------|---------|
| No lift rule | `cannot insert User in Html builder; no lift rule for type User. Use a field like user.name (String), or write a function that returns HtmlFrag` |
| Inserting Doc | `cannot insert HtmlDoc in Html builder; HtmlDoc is a complete document. Use a function that returns HtmlFrag instead` |
| Trailing block on void | `img does not accept children; img is a void element, remove the { } block` |
| Trailing block type mismatch | `trailing block requires a builder output type; parameter 'x' has type Int, which is not a builder output` |
| Unknown builder | `unknown builder 'Foo'; did you mean 'Html'?` |
| optional undefined | `cannot use 'if' without 'else' in Csv builder; Csv does not define 'optional'. Add an else branch, or define fn optional in the builder` |
| array undefined | `cannot use 'for' in Csv builder; Csv does not define 'array'. Define fn array in the builder` |
| banned element | `function 'script' is banned in html context (security); use typed alternatives` |

---

## 12. Codegen

### Rust codegen

Builder blocks are transformed into **node tree construction code**:

```rust
// Almide:
// Html { div(class: "card") { h1 { "Hello" } } }

// Generated Rust:
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
// Generated TypeScript:
(() => {
    const __b0 = __almd_html.text(__almd_html.escape("Hello"));
    const __b1 = __almd_html.el("h1", {}, [__b0]);
    const __b2 = __almd_html.el("div", { class: "card" }, [__b1]);
    return __almd_html.frag([__b2]);
})()
```

### render Function

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
            // Already escaped (escape_html applied during lift)
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

## 13. Runtime Types

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

### Added as Reserved Words

| Keyword | Reason |
|---------|--------|
| `builder` | Keyword for builder definitions |
| `template` | Declaration of document entry points (variant of `fn`) |

### NOT Reserved Words

| Identifier | Reason |
|------------|--------|
| `lift` | Only meaningful inside builder definitions |
| `Html`, `Text`, `Csv` | Builder names are ordinary identifiers |
| HTML tag names | Ordinary functions. Unrelated to builder |

**Only 2 reserved words are added: `builder` and `template`.**

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
# Call from Python
from almide_lib import render_email
html = render_email("Alice", 30)
send_email(to=addr, body=html)
```

---

## 16. Phase Roadmap

### Phase 1: Builder Core

Add `builder` mechanism to language core:

- [ ] `builder` keyword + parser (parse builder definitions)
- [ ] `template` keyword + parser (FnKind::Template)
- [ ] `BuilderBlock` AST node (generic)
- [ ] Trailing block parser (`fn(args) { items }` → insert as last argument)
- [ ] Builder registry (Output type → Builder mapping)
- [ ] Builder inference (identify builder from trailing block's Output type)
- [ ] `lift` rule resolution (expression type → lift rule lookup → Node conversion)
- [ ] Transformation to `block` / `optional` / `array` (IR generation)
- [ ] `if` / `for` / `match` / `let` transformation inside builders
- [ ] Constraint checking inside builder blocks (var/assign/while prohibited)
- [ ] Diagnostics (no lift rule, using if when optional is undefined, etc.)

### Phase 2: Html Builder

Add Html builder to stdlib:

- [ ] `builder Html` definition (lift rules + block/optional/array)
- [ ] `html_elements.toml` + `build.rs` generation
- [ ] HTML element functions (div, h1, p, a, img, ... — ~70 functions generated from TOML)
- [ ] Global attributes (id, class, hidden, ...)
- [ ] Element-specific attributes (href, src, alt, ...)
- [ ] Void element handling (img, input, br — trailing block prohibited)
- [ ] `HtmlNode` / `HtmlFrag` / `HtmlDoc` types
- [ ] `render(HtmlDoc) -> String` / `render_frag(HtmlFrag) -> String`
- [ ] Context-aware HTML escaping（`escape_html`）
- [ ] Banned element checks (script, style, iframe)
- [ ] Rust runtime implementation (add to `core_runtime.txt`)
- [ ] TS runtime implementation (add to `emit_ts_runtime`)
- [ ] HTML element typo diagnostics (`diiv` → `did you mean div?`)

### Phase 3: Text Builder + Ergonomics

- [ ] `builder Text` definition
- [ ] `line()`, `blank()`, `indent()` functions
- [ ] `TextNode` / `TextFrag` / `TextDoc` types
- [ ] `render(TextDoc) -> String`
- [ ] `data-*` / `aria-*` attribute support
- [ ] `TrustedHtml` type and raw insertion API

### Phase 4: Prompt Builder + Advanced

- [ ] `builder Prompt` definition
- [ ] `PromptDoc` type / system/user/context nodes
- [ ] Codec integration (`expect T using Json`)
- [ ] Custom element support（`element("my-widget") { ... }`）
- [ ] Source map（rendered → template source）

---

## Key Design Decisions

### Why a General-Purpose Builder (Differences from Old Design)

The old design embedded `html {}` / `text {}` as special syntax in the parser. The new design follows Swift's Result Builder approach, implementing it as a **language-level general-purpose transformation mechanism**.

| | Old Design | New Design |
|---|---|---|
| HTML tags | Contextual keywords inside builder | Ordinary functions |
| Element vs Expr ambiguity | Resolved via lookahead | Doesn't exist (everything is function calls) |
| Adding new builders | Requires compiler changes | Users can define with `builder` |
| Recognizing `html {}` | Parser recognizes contextually | `Html { }` is a builder block |
| Known tags check | Parser level | Type checker level (function existence check) |

### Why Not Introduce `buildEither`

Swift's `buildEither(first/second)` builds a binary tree so that if/else branches can return different types. Almide doesn't introduce this:

- Almide's variant + match is more powerful than Swift's and handles branch type unification naturally
- `buildEither`'s binary tree makes types complex and hard for LLMs to understand
- The constraint that if/else branches return the same Node type is sufficient

### Trailing Block Ambiguity Resolution

Whether `f { ... }` is "passing a block expression to function f" or "passing a trailing builder block to function f" is determined by f's signature:

- f's last parameter type is a builder's Output type → trailing builder block
- Otherwise → regular block expression (preserves existing behavior)

No existing code is broken (no functions currently have builder Output types).

### Why `lift` Uses Declarative Rules Instead of Overloading

Almide has no overloading. Making `lift` a function would require same-named functions with different signatures. Instead, the `lift T => expr` syntax within builder definitions declaratively maps types to conversions. The compiler references these rules to generate code for each type.

## Supersedes

Fully replaces the old template.md (Typed Document Builder). The introduction of the generic `builder` mechanism makes the following unnecessary:

- `BuilderItem::Element` AST (elements unified into function calls)
- `BuilderItem::Text` / `Line` / `Blank` (unified into functions)
- `HtmlBuilder` / `TextBuilder` AST distinction (unified into generic `BuilderBlock`)
- Element vs Expr ambiguity resolution logic (unnecessary)
- `html_schema.toml` tag/attr validation (replaced by function signatures + type checking)
