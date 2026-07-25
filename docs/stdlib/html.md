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

```almd
html.escape("<script>")  // SafeHtml holding "&lt;script&gt;"
```

### `html.raw(s: String) -> SafeHtml`

Wrap a string WITHOUT escaping it. For markup the program itself produced. Any
call is a place a reviewer should look — never pass caller-supplied text here.

```almd
let br = html.raw("<br>")
```

### `html.to_string(h: SafeHtml) -> String`

Unwrap to the underlying string, ready to write into a response body.

### `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml`

Join two fragments. Both operands are already safe, so the result is.

```almd
let row = html.concat(html.raw("<li>"), html.concat(html.escape(name), html.raw("</li>")))
```

### `html.empty() -> SafeHtml`

The empty fragment — the identity for `concat`, and the natural seed for a fold
over a list of rows.

```almd
items
  |> list.map((i) => html.escape(i))
  |> list.fold(html.empty(), (acc, frag) => html.concat(acc, frag))
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
