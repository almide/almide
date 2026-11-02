<a id="l2l3-html"></a>

# L2/L3: html

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-html.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-html.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-html.zh-CN.md)

> Non-normative reader translation of the [canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-html.md), pinned at 35297a612fb29e3f78f6587aba253aa91d51f466. This snapshot is not automatically synchronized. The original remains authoritative. Roadmap decisions, dates, comparisons, implementation and measurement claims are reproduced without independent execution or revalidation; they are not assertions of current release behavior. Source metadata is displayed as a reader note, not active generator input.

> Source description (inactive metadata): Stdlib redesign L2/L3 — html: the five SafeHtml functions stay; no unescape, no template engine

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-29)
> Sources: Rust — (not in the standard library; https://doc.rust-lang.org/std/ )・ Go https://pkg.go.dev/html ・ https://pkg.go.dev/html/template#HTML ・ Python https://docs.python.org/3/library/html.html ・ TS/JS https://jsr.io/@std/html ・ https://bun.sh/docs/api/utils#bun-escapehtml ・ Gleam — (not in the standard library)・ Elixir — (not in the standard library; Phoenix.HTML is external)・ OCaml — (not in the standard library)・ Kotlin — (kotlinx.html is a package)・ Swift — (not in the standard library)
> Measured: `almide 0.68.0 (dev, 437b8f127)`, native and `--target wasm`. The probe is `probe-url/p1.almd` in the scratchpad.

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Convert special characters to entity references | `escape` → `SafeHtml` | — | `html.EscapeString` | `html.escape` | ◐ Deno `escape`, `Bun.escapeHTML` | — | — | — | — | — |
| Decode entity references | — | — | `html.UnescapeString` | `html.unescape` | ◐ Deno `unescape` | — | — | — | — | — |
| Represent “escaped” with a type | `SafeHtml`, `raw`, `to_string` | — | `template.HTML` | — | — | — | — | — | — | — |
| Concatenation and empty | `concat`, `empty` | — | (string concatenation) | — | — | — | — | — | — | — |
| Template engine | — (`"${}"` interpolation) | — | `html/template` | `string.Template` | — | — | EEx | — | — | — |

Escaping is standard in 2 languages plus ◐1 ([facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md)).

<a id="完全性の規則"></a>

### Completeness rules

1. There are only 2 ways into `SafeHtml`: `escape` (make safe) and `raw` (trusted input unchanged); the only way out is `to_string`.
2. Combine `SafeHtml` values using `concat` and `empty`. Neither double-escapes its contents.
3. **Intentional omissions**: no `unescape` (only 3 languages, below R-L1's threshold of 5). No template engine (for the same reason as R-L1 chapter 3, “printf-style formatting”: use string interpolation).

<a id="l2-の判定"></a>

### L2 verdict

- No missing roles (in 5 or more languages), duplicates, or family gaps.
- Measurement: the `concat` of `html.escape("<a href='x'>&\"</a>")` and `html.raw("<b>")` was `&lt;a href=&#39;x&#39;&gt;&amp;&quot;&lt;/a&gt;<b>` on both targets.
- R-L0-07: `html.to_string` (`SafeHtml` → `String`) is `to_<型>` on the source type, as the rule requires.

<a id="案"></a>

## Proposals

<a id="r-html-01-5-関数を据え置く---採用変更なし"></a>

### R-html-01 Keep the 5 functions — ○ Accepted (no change)

- Content: keep `escape`, `raw`, `to_string`, `concat`, and `empty`.
- Supporting L0 rules: R-L1-29 (do not move), R-L0-00.
- (b) Pure computation. The result is the same on both targets (measured).
- (c) Confusing a string with `SafeHtml` causes a type error, rather than silently double-escaping or missing escaping.
- Distinguishing `empty` from `new`: functions that create containers use `new` (`map.new`, `set.new`, `deque.new`), but `html.empty()` is a value, an “empty HTML fragment,” not a container, and is the identity element of `concat`. Keep `empty` as the value's name (it is not a synonym under R-L0-01).
- issue: none

<a id="r-html-02-unescape-を足す---不採用"></a>

### R-html-02 Add `unescape` — × Rejected

- Reason: only Go, Python, and Deno have it as standard — 3 languages, below L1's threshold (5 or more). Reading HTML is the concern of a package (an HTML parser).
- issue: none

<a id="r-html-03-safehtml-の列をまとめる-htmljoin-を足す---不採用"></a>

### R-html-03 Add `html.join` to combine a sequence of `SafeHtml` — × Rejected

- Reason: only 2 languages have an escaping type, below the threshold of 5. Write `list.fold(xs, html.empty(), html.concat)` instead.
- issue: none

<a id="r-html-04-文脈ごとの組み立て関数-attrurl_attrscript_json-を足す---採用4120"></a>

### R-html-04 Add context-specific builders `attr`, `url_attr`, and `script_json` — ○ Accepted (#4120)

- Content: `escape` is for body text (text context) only. Build attribute values with `html.attr(name, value)` (double-quoted and escaped), URL attributes with `html.url_attr(name, url)` (allow only relative URLs and `http`, `https`, and `mailto`; everything else becomes `about:invalid#almide-blocked`), and data inside `<script>` with `html.script_json(v)` (turn `<` `>` `&` U+2028 U+2029 into JSON `\u` escapes). `http.html(status, SafeHtml)` returns `SafeHtml` as `text/html`.
- Rationale: security table row 11e (HTML injection). Go's `html/template` changes escaping by context and replaces disallowed URLs with `#ZgotmplZ`. Prioritize the policy of preventing injection through types (R-html-01 (c)) over the five-language threshold.
- issue: #4120

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `escape` | `html.escape(s: String) -> SafeHtml` | Go `html.EscapeString` / Python `html.escape` / Bun `escapeHTML` | Keep | R-html-01 |
| `raw` | `html.raw(s: String) -> SafeHtml` | Go `template.HTML(s)` | Keep | R-html-01 |
| `to_string` | `html.to_string(h: SafeHtml) -> String` | Go `string(h)` | Keep | R-html-01 |
| `concat` | `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml` | — (most: string concatenation) | Keep | R-html-01 |
| `empty` | `html.empty() -> SafeHtml` | — | Keep | R-html-01 |

## Source caveats

The source retains a five-function description/table and a two-entry-path completeness rule while also accepting later context-specific builders in R-html-04. Both are preserved without silently updating the table. The measured sample is not a general security proof. Raw input is trusted, and text escaping does not extend to attribute, URL or script contexts; the context-specific rules retain their stated scope.
