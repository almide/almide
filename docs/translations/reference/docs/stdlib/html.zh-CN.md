# html

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](html.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](html.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](html.zh-CN.md)



> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/html.md)的非规范读者译文，不自动生成或同步。签名、代码及代码注释保持原文，API 说明译文另附在代码外。翻译时没有独立执行示例或验证实现。

> 安全说明必须结合使用上下文的限制理解；这不是包括 `raw` 或错误上下文插入在内、所有路径都不会出现 XSS 的保证。


HTML 转义与 `SafeHtml` 能力类型。`import html`。

本模块的要点是 `SafeHtml` 和 `String` 是不同的类型。连接 `SafeHtml` 值的模板无法意外混入未转义的字符串，因为类型系统不允许这样做。使用适合上下文的构造器（`escape`、`attr`、`url_attr`、`script_json`）构造片段，以 `concat` / `empty` 组合，或通过 `raw` 显式信任标记。片段仍必须用于其预期的上下文。

本模块在两个目标上都公开 pure 函数。跨目标行为由 HTML 规范样例覆盖；仅有纯性并不能证明后端等价。

### `html.escape(s: String) -> SafeHtml`

转义五个 XML/HTML 元字符 — `&`、`<`、`>`、`"`、`'` — 并包装结果。这是处理用户输入的常规路径，但**仅适用于文本上下文**（元素内容）。它不添加包围用的引号，会保留 `javascript:` URL，在 `<script>` 内并不安全：属性用 `attr`，URL 属性用 `url_attr`，脚本数据用 `script_json`。

```almd check
import html

fn main() -> Unit = {
  println(html.to_string(html.escape("<script>")))
}
```

### `html.raw(s: String) -> SafeHtml`

不做转义，直接包装字符串。用于程序自身生成的标记。每个调用点都应接受审查；绝不要在此传入调用方提供的文本。

```almd check
import html

fn main() -> Unit = {
  let br = html.raw("<br>")
  println(html.to_string(br))
}
```

### `html.to_string(h: SafeHtml) -> String`

解包为底层字符串，以便写入响应正文。

### `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml`

连接两个片段。两个操作数已经安全，因此结果也安全。

```almd check
import html

fn main() -> Unit = {
  let name = "Tom & <Jerry>"
  let row = html.concat(html.raw("<li>"), html.concat(html.escape(name), html.raw("</li>")))
  println(html.to_string(row))
}
```

### `html.empty() -> SafeHtml`

空片段——`concat` 的单位元，也是对多行列表执行 fold 时自然的初始值。

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

输出 `name="value"`，将 value 按双引号属性进行转义，使其无法结束属性或标签。当 `name` 不是普通属性名、接收 URL（`href`、`src`、`action` 等——应使用 `url_attr`），或执行脚本与样式（`on…`、`style`、`srcdoc`——没有构造器会输出这些属性）时中止。

```almd check
import html

fn main() -> Unit = {
  let cls = "x\" onmouseover=\"alert(1)"
  println("<div ${html.to_string(html.attr("class", cls))}>")
}
```

### `html.url_attr(name: String, url: String) -> SafeHtml`

转义后的 `name="url"`；仅当 URL 是相对形式，或协议为 `http`、`https`、`mailto`（大小写不限）时保留 URL。任何其他协议——`javascript:`、`data:`、包含控制字符或空格的协议——都替换为无害的 `about:invalid#almide-blocked`。

```almd check
import html

fn main() -> Unit = {
  println(html.to_string(html.url_attr("href", "https://example.com/?a=1&b=2")))
  println(html.to_string(html.url_attr("href", "javascript:alert(1)")))
}
```

### `html.script_json(v: Value) -> SafeHtml`

将 `json.stringify(v)` 中的 `<`、`>`、`&`、U+2028 和 U+2029 写为 JSON 的 `\u` 转义，使文本无法结束 `<script>` 元素，并能重新解析为相同的值。

```almd check
import html
import json

fn main() -> Unit = {
  let v = value.object([("x", value.string("</script>"))])
  println("<script>const d = ${html.to_string(html.script_json(v))}</script>")
}
```



## 签名索引 (8 个函数)

```
// s with & < > " ' turned into HTML entities; text context only.
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

// name="value" with value escaped for a double-quoted attribute; aborts on a URL, event-handler or malformed name.
// @since unreleased
html.attr(name: String, value: String) -> SafeHtml

// name="url" escaped, url kept only if relative or http, https or mailto; else about:invalid#almide-blocked.
// @since unreleased
html.url_attr(name: String, url: String) -> SafeHtml

// json.stringify(v) with < > & U+2028 U+2029 as \u escapes; safe inside <script>.
// @since unreleased
html.script_json(v: Value) -> SafeHtml
```



## API 说明参考译文

- `html.escape(s: String) -> SafeHtml` — 把 s 中的 & < > " ' 转为 HTML 实体；仅适用于文本上下文。
- `html.raw(s: String) -> SafeHtml` — 将 s 直接作为 SafeHtml，不转义；仅限可信输入。
- `html.to_string(h: SafeHtml) -> String` — 原样返回 h 内的 HTML 文本。
- `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml` — 将 a 接 b 组成一个 SafeHtml；不重新转义。
- `html.empty() -> SafeHtml` — 空字符串的 SafeHtml。
- `html.attr(name: String, value: String) -> SafeHtml` — 输出 name="value"，value 按双引号属性转义；URL、事件处理器或格式错误的名称会中止。
- `html.url_attr(name: String, url: String) -> SafeHtml` — 转义 name="url"；仅保留相对 URL 或 http/https/mailto，否则为 about:invalid#almide-blocked。
- `html.script_json(v: Value) -> SafeHtml` — 将 json.stringify(v) 中的 < > & U+2028 U+2029 写成 \u 转义，可用于 `<script>` 内。
