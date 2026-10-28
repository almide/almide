# svg

Safe SVG construction and the `SafeSvg` type. `import svg`.

`SafeSvg` is a response body (ADR-0032): `http.response(200, image)` serves it
as `image/svg+xml`, and that media type is owned here, so `http.typed` cannot
label a String as SVG (E117). An SVG document runs script — `<script>`, an
`onload` attribute, an `<animate>` that rewrites an `href` into a
`javascript:` URL — so outside this module a `SafeSvg` is built only by the
builders below, or by `raw`, the named opt-out.

Every function is pure, so it behaves identically on both targets.

### `svg.escape(s: String) -> SafeSvg`

Escape `&`, `<`, `>`, `"`, `'`: text inside an element.

```almd check
import svg

fn main() -> Unit = println(svg.to_string(svg.escape("<text>")))
```

### `svg.attr(name: String, value: String) -> SafeSvg`

`name="value"` with the value escaped for a double-quoted attribute. Aborts on
a malformed name, a URL-valued one (`href`, `xlink:href`, `src`: use
`url_attr`) and one that runs script or styles (`on…`, `style`).

### `svg.url_attr(name: String, url: String) -> SafeSvg`

`name="url"`, keeping a relative URL or an `http` / `https` one and writing
`about:invalid#almide-blocked` for any other scheme.

### `svg.element(name: String, attrs: List[SafeSvg], children: List[SafeSvg]) -> SafeSvg`

`<name attrs…>children…</name>`. Aborts on a malformed name and on the
elements that run script or rewrite attributes at run time: `script`,
`foreignObject`, `set`, `animate`, `animateMotion`, `animateTransform`,
`handler`, `listener`.

```almd check
import http
import svg

fn badge(label: String) -> HttpResponse = http.response(200, svg.element("svg", [svg.attr("width", "80"), svg.attr("height", "20")], [svg.element("text", [svg.attr("x", "4"), svg.attr("y", "14")], [svg.escape(label)])]))

fn main() -> Unit = println(http.header(badge("ok"), "Content-Type") ?? "none")
```

### `svg.raw(s: String) -> SafeSvg`

`s` as `SafeSvg`, unescaped: trusted markup only.

### `svg.concat(a: SafeSvg, b: SafeSvg) -> SafeSvg`, `svg.empty()`, `svg.to_string(h)`

Join two values, the empty value, and the markup as a String.
