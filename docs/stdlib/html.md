# html

HTML escaping and the `SafeHtml` capability type. `import html`.

The point of this module is that `SafeHtml` and `String` are different types.
A template that concatenates `SafeHtml` values cannot accidentally splice in an
unescaped string, because the type system will not let it — the only ways to
obtain a `SafeHtml` are `escape` (which escapes) and `raw` (which is explicit
about trusting its input).

Every function is pure, so it behaves identically on both targets.

### `html.escape(s: String) -> SafeHtml`

Escape the five XML/HTML metacharacters — `&`, `<`, `>`, `"`, `'` — and wrap
the result. The ordinary path for anything that came from a user, in the
**text context only** (element content). It writes no quotes, keeps a
`javascript:` URL and is not safe inside `<script>`: an attribute is `attr`,
a URL attribute is `url_attr`, and data for a script is `script_json`.

```almd check
import html

fn main() -> Unit = {
  println(html.to_string(html.escape("<script>")))
}
```

### `html.raw(s: String) -> SafeHtml`

Wrap a string WITHOUT escaping it. For markup the program itself produced. Any
call is a place a reviewer should look — never pass caller-supplied text here.

```almd check
import html

fn main() -> Unit = {
  let br = html.raw("<br>")
  println(html.to_string(br))
}
```

### `html.to_string(h: SafeHtml) -> String`

Unwrap to the underlying string, ready to write into a response body.

### `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml`

Join two fragments. Both operands are already safe, so the result is.

```almd check
import html

fn main() -> Unit = {
  let name = "Tom & <Jerry>"
  let row = html.concat(html.raw("<li>"), html.concat(html.escape(name), html.raw("</li>")))
  println(html.to_string(row))
}
```

### `html.empty() -> SafeHtml`

The empty fragment — the identity for `concat`, and the natural seed for a fold
over a list of rows.

```almd check
import html

fn main() -> Unit = {
  let items = ["a<b", "c&d"]
  let page = items
    |> list.map((i) => html.escape(i))
    |> list.fold(html.empty(), (acc, frag) => html.concat(acc, frag))
  println(html.to_string(page))
  println("[${html.to_string(html.empty())}]")
}
```

### `html.attr(name: String, value: String) -> SafeHtml`

`name="value"` with the value escaped for a double-quoted attribute, so it
cannot end the attribute or the tag. Aborts when `name` is not a plain
attribute name, takes a URL (`href`, `src`, `action`, … — use `url_attr`),
or runs script or styles (`on…`, `style`, `srcdoc` — no builder writes those).

```almd check
import html

fn main() -> Unit = {
  let cls = "x\" onmouseover=\"alert(1)"
  println("<div ${html.to_string(html.attr("class", cls))}>")
}
```

### `html.url_attr(name: String, url: String) -> SafeHtml`

`name="url"`, escaped, keeping the URL only when it is relative or its scheme
is `http`, `https` or `mailto` (any case). Any other scheme — `javascript:`,
`data:`, a scheme with a control character or a space in it — is replaced by
the inert `about:invalid#almide-blocked`.

```almd check
import html

fn main() -> Unit = {
  println(html.to_string(html.url_attr("href", "https://example.com/?a=1&b=2")))
  println(html.to_string(html.url_attr("href", "javascript:alert(1)")))
}
```

### `html.script_json(v: Value) -> SafeHtml`

`json.stringify(v)` with `<`, `>`, `&`, U+2028 and U+2029 written as JSON
`\u` escapes, so the text cannot close the `<script>` element and parses back
to the same value.

```almd check
import html
import json

fn main() -> Unit = {
  let v = value.object([("x", value.str("</script>"))])
  println("<script>const d = ${html.to_string(html.script_json(v))}</script>")
}
```

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (5 functions)

```
// s with & < > " ' turned into HTML entities.
// @since 0.18.1 or earlier
html.escape(s: String) -> SafeHtml

// s as SafeHtml, unescaped; trusted input only.
// @since 0.18.1 or earlier
html.raw(s: String) -> SafeHtml

// HTML text inside h, as-is.
// @since 0.18.1 or earlier
html.to_string(h: SafeHtml) -> String

// a then b as one SafeHtml; nothing re-escaped.
// @since 0.18.1 or earlier
html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml

// SafeHtml of the empty string.
// @since 0.18.1 or earlier
html.empty() -> SafeHtml
```

<!-- END GENERATED SIGNATURE INDEX -->
