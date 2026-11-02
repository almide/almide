<a id="l2l3-html"></a>

# L2/L3：html

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-html.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-html.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-html.zh-CN.md)

> 这是固定于 35297a612fb29e3f78f6587aba253aa91d51f466 的[正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-html.md)的非规范性阅读译本，不会自动同步。原本仍具权威性。路线图决策、日期、比较、实现及测量主张未经独立执行或重新验证而保留，并非对当前版本行为的断言。原本元数据以阅读注记显示，不作为生成器的有效输入。

> 原本说明（已停用元数据）: 标准库重设计 L2/L3 — html：保留 5 个 SafeHtml 函数；不提供 unescape 或模板引擎。

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-29）
> 来源： Rust —（标准中没有。https://doc.rust-lang.org/std/ ）・ Go https://pkg.go.dev/html ・ https://pkg.go.dev/html/template#HTML ・ Python https://docs.python.org/3/library/html.html ・ TS/JS https://jsr.io/@std/html ・ https://bun.sh/docs/api/utils#bun-escapehtml ・ Gleam —（标准中没有）・ Elixir —（标准中没有。Phoenix.HTML 是外部库）・ OCaml —（标准中没有）・ Kotlin —（kotlinx.html 是 package）・ Swift —（标准中没有）
> 实测：`almide 0.68.0 (dev, 437b8f127)`，native 与 `--target wasm`。探针为 scratchpad 中的 `probe-url/p1.almd`。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 将特殊字符转换为实体引用 | `escape` → `SafeHtml` | — | `html.EscapeString` | `html.escape` | ◐ Deno `escape`、`Bun.escapeHTML` | — | — | — | — | — |
| 还原实体引用 | — | — | `html.UnescapeString` | `html.unescape` | ◐ Deno `unescape` | — | — | — | — | — |
| 用类型表示“已转义” | `SafeHtml`、`raw`、`to_string` | — | `template.HTML` | — | — | — | — | — | — | — |
| 连接与空值 | `concat`、`empty` | — | （字符串连接） | — | — | — | — | — | — | — |
| 模板引擎 | —（`"${}"` 插值） | — | `html/template` | `string.Template` | — | — | EEx | — | — | — |

标准中具有转义功能的是 2 种语言加 ◐1（[facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md)）。

<a id="完全性の規則"></a>

### 完整性规则

1. 进入 `SafeHtml` 的途径只有 `escape`（使其安全）与 `raw`（原样使用受信任输入）这 2 种，离开的途径只有 `to_string` 这 1 种。
2. `SafeHtml` 之间使用 `concat` 和 `empty` 组合。两者都不会对内容重复转义。
3. **有意省略**：不提供 `unescape`（仅 3 种语言，未达到 R-L1 的 5 种门槛）。不提供模板引擎（与 R-L1 第 3 章“printf 类格式化”理由相同，使用字符串插值）。

<a id="l2-の判定"></a>

### L2 判定

- 不存在缺失角色（5 种以上语言）、重复或函数族缺口。
- 实测：`html.escape("<a href='x'>&\"</a>")` 与 `html.raw("<b>")` 的 `concat` 在两个 target 上均为 `&lt;a href=&#39;x&#39;&gt;&amp;&quot;&lt;/a&gt;<b>`。
- R-L0-07：`html.to_string`（`SafeHtml` → `String`）是原类型上的 `to_<型>`，符合规则。

<a id="案"></a>

## 提案

<a id="r-html-01-5-関数を据え置く---採用変更なし"></a>

### R-html-01 保持 5 个函数 — ○ 采纳（无变更）

- 内容：保持 `escape`、`raw`、`to_string`、`concat`、`empty`。
- 依据的 L0 规则：R-L1-29（不移动）、R-L0-00。
- (b) 纯计算。两个 target 的结果相同（实测）。
- (c) 混淆字符串与 `SafeHtml` 会产生类型错误，而不是悄悄发生重复转义或漏转义。
- `empty` 与 `new` 的区别：创建容器的函数使用 `new`（`map.new`、`set.new`、`deque.new`），但 `html.empty()` 不是容器，而是“空 HTML 片段”这个值，是 `concat` 的单位元。保留 `empty` 作为值的名称（不属于 R-L0-01 的同义词）。
- issue：无

<a id="r-html-02-unescape-を足す---不採用"></a>

### R-html-02 添加 `unescape` — × 不采纳

- 理由：标准中提供它的只有 Go、Python、Deno 这 3 种语言，未达到 L1 标准（5 种以上）。读取 HTML 的处理属于 package（HTML 解析器）的职责。
- issue：无

<a id="r-html-03-safehtml-の列をまとめる-htmljoin-を足す---不採用"></a>

### R-html-03 添加汇总 `SafeHtml` 序列的 `html.join` — × 不采纳

- 理由：具有转义类型的语言只有 2 种，未达到 5 种语言门槛。可以写成 `list.fold(xs, html.empty(), html.concat)`。
- issue：无

<a id="r-html-04-文脈ごとの組み立て関数-attrurl_attrscript_json-を足す---採用4120"></a>

### R-html-04 添加按上下文构建的函数 `attr`、`url_attr`、`script_json` — ○ 采纳（#4120）

- 内容：`escape` 仅用于正文（文本上下文）。属性值用 `html.attr(name, value)` 构建（加双引号并转义），URL 属性用 `html.url_attr(name, url)` 构建（仅允许相对 URL 和 `http`、`https`、`mailto`，其余变为 `about:invalid#almide-blocked`），`<script>` 内的数据用 `html.script_json(v)` 构建（把 `<` `>` `&` U+2028 U+2029 转为 JSON 的 `\u` 转义）。`http.html(status, SafeHtml)` 以 `text/html` 返回 `SafeHtml`。
- 依据：安全表第 11e 行（HTML 注入）。Go 的 `html/template` 按上下文改变转义方式，并将不允许的 URL 替换为 `#ZgotmplZ`。优先采用以类型防止注入的方针（R-html-01 的 (c)），而非 5 种语言门槛。
- issue：#4120

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，显示 mut） | 其他语言的主要写法 | 判定 | 提案 |
|---|---|---|---|---|
| `escape` | `html.escape(s: String) -> SafeHtml` | Go `html.EscapeString` / Python `html.escape` / Bun `escapeHTML` | 保持 | R-html-01 |
| `raw` | `html.raw(s: String) -> SafeHtml` | Go `template.HTML(s)` | 保持 | R-html-01 |
| `to_string` | `html.to_string(h: SafeHtml) -> String` | Go `string(h)` | 保持 | R-html-01 |
| `concat` | `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml` | —（多数：字符串连接） | 保持 | R-html-01 |
| `empty` | `html.empty() -> SafeHtml` | — | 保持 | R-html-01 |

## 原本注意事项

原本保留了 5 个函数的说明与表、仅有 2 个入口的完整性规则，同时又在 R-html-04 采纳后来的上下文专用构建函数。两者均保留，不暗中更新表格。实测样例不构成一般性安全证明。raw 输入是受信任的；文本转义不扩展至属性、URL 或 script 上下文，各上下文规则保持其声明范围。
