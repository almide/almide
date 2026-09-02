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
the result. The ordinary path for anything that came from a user.

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

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (5 functions)

```
html.escape(s: String) -> SafeHtml
html.raw(s: String) -> SafeHtml
html.to_string(h: SafeHtml) -> String
html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml
html.empty() -> SafeHtml
```

<!-- END GENERATED SIGNATURE INDEX -->
