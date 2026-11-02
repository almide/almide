<a id="adr-0032-a-responses-content-type-is-its-bodys-type--body-gives-each-body-type-its-media-type-header-writes-cannot-name-content-type-and-a-script-capable-type-is-built-only-by-its-safe-module"></a>

# ADR-0032：response 的 Content-Type 就是 body 的类型 — `Body` 为每种 body 类型赋予 media type，header 写入不能指定 Content-Type，script-capable 类型只能由其安全 module 构建

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0032-a-response-content-type-is-its-body-type.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0032-a-response-content-type-is-its-body-type.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0032-a-response-content-type-is-its-body-type.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0032-a-response-content-type-is-its-body-type.md)
> 此历史ADR并存已接受决定、早期提案及后续裁定。D1为to_text，D2/Q1仍为to_bytes；D4/表/Q5保留TypedBody/Typed及不同返回形式。Q11早期描述generic set_body，但owner摘要保留String并增加with_body。不静默调和原文差异。Q15 sealed及Q8 stdlib SVG由D7替代。比较／安全主张属原文，不是独立验证的当前事实或普遍XSS防护；raw仍为opt-out。

- **状态**：Accepted（2026-10-29）。owner 对方向和 Q2 判○、Q7 判×；其余按推荐判○，包括下方 Q14–Q16。见 [owner 决定](#owner-decisions-)。
- **日期**：2026-10-29
- **范围**：`http` response surface（`response`、`html`、`json`、`with_headers`、`set_header`、`set_body`、`set_body_bytes`、`serve`）、`html` module 的 `SafeHtml`、stdlib 外 media type owner 及 application 对其 grant（D7）、body 的 media type、security-table 行11e、contract C-275。
- **相关**：
  - `docs/roadmap/active/security-by-construction.md` 行11e（HTML injection）。其过渡机制——header string runtime guard 加 E115——在 `stdlib/http.almd` 和 `crates/almide-frontend/src/check/content_type_guard.rs`。在本 surface 落地前保留，随后移除。
  - R-L0-37（具体类型由 protocol 组合而成）；`crates/almide-frontend/src/canonicalize/protocols.rs` 中 builtin-protocol table（`HttpMessage`、`Encode` / `Decode` / `Codec`）。
  - C-367 / C-407（serve core 拥有 framing field，并添加 `X-Content-Type-Options: nosniff`）。

<a id="context"></a>

## 背景

行11e的临时修复拒绝在 String body 旁以 String header 写 HTML（及 XML）Content-Type。它可用，却是针对 *string* 的 guard，因此有三个结构弱点：

1. **http 持有 media 列表。** 哪些类型 script-capable 是 document format 属性，列表却在 transport module（`__http_response_html_types`）。新增 script-capable format 意味着修改 http。
2. **body 含义由 header string 决定。** `with_headers(200, s, [("Content-Type", t)])` 不论 `s` 为何，都宣称 body 是 `t` document。checker 能跟踪的是类型，body 却没有表达其含义的类型：始终是 `String`。
3. **response 可以完全没有 Content-Type。** 未提供它的 `with_headers` 让 browser sniff。`nosniff`（C-407）限制 sniffing，却不定义类型。

owner 裁定：Content-Type 来自 body 类型，绝非 header string；安全性属于 format module（`html`、D7 下 almide/svg 等 package），http 不知道 media 列表。

<a id="decision"></a>

## 决定

<a id="d1--body-a-body-type-knows-its-media-type"></a>

### D1 — `Body`：body 类型知道自身 media type

builtin protocol `Body`（采用 `Encode` / `Decode` / `HttpMessage` 已用的名词家族）有两项要求：

- `T.media_type() -> String`：拥有此 body 的 response 携带的 Content-Type。
- `body.to_text() -> String`：线上 body。在 #4311 给出精确 bytes slot 前，已构建 response 按 text 保存 body，因此要求是 text（Bytes body 按 lossy UTF-8 解码，Q16）。提案时为 `to_bytes`；用 text 后 String body 不必往返转换，`string` 不到达 `bytes`（`bundled_reach.rs` 中 #4289 reach gate）。stdlib 内两者都是标记 `__self_` 的 per-type function（`__safe_html_media_type`）；stdlib 外类型用普通名称 `fn T.media_type(self)` 和 `fn T.to_text(self)`（Q24）。

| Body 类型 | Content-Type | 声明位置 |
|---|---|---|
| `String` | `text/plain; charset=utf-8` | builtin conformer |
| `Bytes` | `application/octet-stream` | builtin conformer |
| `SafeHtml` | `text/html; charset=utf-8` | `html`（`type SafeHtml: Body`） |
| package 的类型 | 所声明 literal | package；script-capable 时由 application grant（D7） |
| `Value`（JSON） | `application/json` | `json`（子决定 Q3） |
| `Typed`（新） | 构建时指定类型 | `http`（D4） |

<a id="d2--a-response-is-built-from-a-body"></a>

### D2 — response 从 `Body` 构建

`http.response[B: Body](status, body: B) -> HttpResponse` 从 `B.media_type()` 设置 Content-Type，从 `body.to_bytes()` 设置 body。`set_body[B: Body](resp, body)` 同时替换两者，因此向 HTML response 写 String 会变为 `text/plain`。这里不拒绝任何内容，因为无法贴错标签。获取的 response 保留 upstream 发来的类型，直到 `set_body` 替换（子决定 Q11）。

<a id="d3--content-type-is-owned-by-the-body-so-header-writes-cannot-name-it"></a>

### D3 — Content-Type 由 body 拥有，header 写入因而不能指定它

`Body` 声明所拥有 header（protocol 声明上的 `@owns_header("Content-Type")`）。`set_header` / `with_headers` 拒绝 protocol 拥有的名称：

- 名称是 literal 时，在 check 时拒绝（E116）。
- 其他情况在 runtime 拒绝（指明 `http.response` 的 abort）。

规则读取 protocol 声明，绝非 http 列表。serve core 拥有的 framing field（C-367）是另一条既有规则。

<a id="d4--every-other-media-type-has-a-typed-way-in-and-a-script-capable-one-has-none"></a>

### D4 — 其他 media type 都有类型化入口，script-capable 类型则没有

`http.typed(media_type: String, body: Bytes) -> TypedBody` 构建任意格式正确、以 literal 写出的 media type body（Q14）；checker 读取它，所以 runtime 不存在 media type 相关信息。另一 `Body` 类型**拥有**的类型被拒绝。

所有权在安全类型自身声明：

- `SafeHtml` media fn 上的 `@media_owns("text/html", "application/xhtml+xml")`。
- package 类型 media fn 在 D7 下的 `@media_owns(...)`。

script-capable 集合由 `stdlib/html.almd` 的 `@script_capable(...)` 一次声明：`text/html`、`application/xhtml+xml`、`image/svg+xml`、`*/xml`、`*/*+xml`（任意 XML document 都可能含 XHTML namespace）。`http.typed` 拒绝 script-capable 类型，不论有无人拥有，因此 XML（Q7 ×、#4310）和 almide/svg 交付前的 SVG 属于已拥有但不可构建。

属性位于类型的 `Body` requirement fn（`__safe_html_media_type`），即声明类型 media type 的地方。

pattern 遵循 RFC 6839：`*` 是任意 type，`*<suffix>` 匹配以 `<suffix>` 结尾的更长 subtype。因此拒绝集合为声明并集。计算所得、格式错误或已拥有的 media type 在 check 时是 E117；把 `http.typed` 当值也是如此。`Body` 原先 sealed（Q15），D7 用 grant 替代。

<a id="d5--the-safe-modules"></a>

### D5 — 安全 module

- `html` 不变：context builder，以及具名 opt-out `raw`。
- SVG 不在 stdlib。外部 package almide/svg 在 D7 下拥有 `image/svg+xml`，采用此 ADR 最初为 stdlib `svg` 提出的 builder 集合（`escape`、`attr`、`url_attr` 拒绝 `on*`、`style` 和 script URL；`element` 拒绝 `script`、`foreignObject` 和 animation element；`raw` 为具名 opt-out）。所调查标准库没有交付 SVG；TyXML 和 Trusted Types 将 SVG 放在 HTML 旁，作为另一类 trusted markup。
- `xml`：Q7 ×、#4310。

<a id="d7--owners-outside-the-stdlib-granted-by-the-application-trusted-types-with-a-csp-allowlist"></a>

### D7 — stdlib 外 owner，由 application grant（带 CSP allowlist 的 Trusted Types）

`Body` 开放：package 类型可符合它，并用 `@media_owns` 拥有 media type。拥有 script-capable 类型意味着向 browser 提供 script 的权力，因此需要 application 同意：

```toml
[trust]
media = { "almide/svg" = ["image/svg+xml"] }
```

- **指定谁。** key 为 dependency source `owner/repo`（隐含 github.com；其他 host 用 `host/owner/repo`；path dependency 按 `[dependencies]` 名）。dependency 名可指向另一仓库，source 不可（Q17）。
- **什么需要 grant。** dependency 的 `@media_owns` 与 script-capable 集合重叠，却未获该 pattern grant 时 build 失败（E118），指明 package、类型及应添加行。grant 指定 package 未声明的类型也为 E118。仅拥有其他类型（`text/csv`）无须 grant。stdlib `html` 隐式获 grant；application 自身类型如 native code 一样无需 grant（Q18），并被列出。
- **media type 是 literal。** 非 stdlib `Body` 类型的 media fn 是 string literal；script-capable 类型必须被 fn 自己的 `@media_owns` 覆盖（Q19、E118）。因此获 grant 的 `text/csv` package 不能在 runtime serve `text/html`。
- **每种类型一个 owner。** 两 owner 的 pattern 若能匹配同一类型则 E118（Q22），所以 `http.typed` 拒绝恰好指出一个 owner。
- **审计。** `almide deps` 列出全部 media owner：html、每个获 grant package 的 pattern 和 grant 行，以及 application 自身 owner（Q23）。
- **一个代码。** 所有 media-ownership error 都是 E118（Q20）；其旧含义“user type 声明 Body”从未发布。

威胁模型：传递依赖不能悄悄成为 HTML 权限主体。添加、升级或替换声称 script-capable 类型的 package，会阻止 build，直到 application 指明该 package 和类型。grant 按 source 键控，所以重命名或重新指向的 dependency 不受旧 grant 覆盖。

<a id="d6--migration"></a>

### D6 — 迁移

- dialect-epoch break（Q12）。
- `almide fix` 改写 Content-Type 为 literal 的旧写法：
  - `http.html(st, h)` → `http.response(st, h)`。
  - `with_headers(st, s, [("Content-Type", "text/plain…"), …])` → String body 和剩余 header。
  - `application/json` → `http.json`。
  - 其他 literal type → `http.typed(t, bytes.from_string(s))!`。
- 计算所得 Content-Type 不可改写，收到 E116 hint。
- D2–D4 落地的同一变更移除临时 guard 和 E115。E115 像 E039 / E040 一样退役但保留文档。

<a id="cross-language-comparison"></a>

## 跨语言比较

| | Content-Type 来源 | header string 能否重新标记 body？ | script-capable text 构建者 |
|---|---|---|---|
| **axum**（Rust） | `IntoResponse` 类型：`Html<T>`、`Json<T>`；`String` → `text/plain; charset=utf-8`、`Bytes` → `application/octet-stream` | 能：`([(CONTENT_TYPE, "text/html")], s)` | 无限制；`Html(s)` 接受任意 string |
| **Go** `html/template` + `net/http` | `Header().Set`，否则 sniff（`DetectContentType`） | 能 | 上下文 auto-escape；`template.HTML` 是类型化 opt-out |
| **TyXML**（OCaml） | server library（Dream `Dream.html`） | 能，在 server 中 | typed HTML/SVG tree，按构造 valid |
| **lucid** + servant（Haskell） | endpoint 的 content-type list（`Get '[HTML] (Html ())`） | typed endpoint 不能 | `Html` monad 转义 text；`toHtmlRaw` opt-out |
| **Trusted Types**（browser） | （sink 侧）`innerHTML` 等仅接受 `TrustedHTML` | — | policy 为唯一 constructor，由 CSP 强制 |
| **Rails** | `render html:` / `render plain:` / `send_data type:` | 能：`send_data(s, type: "text/html")` | `SafeBuffer`、auto-escaped ERB、`html_safe` opt-out |
| **Almide（本 ADR）** | body 类型（`Body`） | **不能**：E116 / abort | `html` builder 或 application grant 的 package owner（D7）；`http.typed` 拒绝已拥有或 script-capable 类型 |

Almide 采用：

- axum 的 type-to-header table。
- servant 的规则：由类型而非 handler 决定。
- Trusted Types 的规则：script-capable 值的 constructor 只在 policy module 中。
- TyXML 的 typed SVG，作为 almide/svg package 形态。

Almide 封闭而这些方案均未封闭的，是 header relabel。

<a id="who-may-mint-a-trusted-type-d7"></a>

### 谁可创建 trusted type（D7）

| | 谁能创建 trusted type | 谁同意 | 何处可见 |
|---|---|---|---|
| **Trusted Types** + CSP `trusted-types` | 调用 `trustedTypes.createPolicy(name, …)` 的任意 script | page：`Content-Security-Policy: trusted-types a b` directive 列出允许 policy 名，其他名称 throw | response header |
| **Rust** `unsafe impl` | 任意 crate，针对自身类型（`unsafe impl Send for T`） | crate 作者写 `unsafe`，而非 crate 用户 | 可 grep 的 impl |
| **Go** `template.HTML` | 任意 package，通过转换 | 无人 | 无处 |
| **Rails** `html_safe` | 任意 gem，通过 method call | 无人 | 无处 |
| **Almide（D7）** | 类型声明 `@media_owns` 的 package | application：`[trust] media` 指定 package source 和 media type，否则 build 停止（E118） | `almide.toml` 及 `almide deps` |

模型是 Trusted Types：policy 为具名 constructor，由 deployment 而非 library 列出允许名称。Almide 把列表从 response header 移到 manifest，运行前检查，并按 package source 而非 package 自选名称键控。

<a id="owner-decisions-"></a>

## Owner 决定（○×）

回答（2026-10-29）：Q1 ○、Q2 ○（owner）、Q3 ○、Q4 ○、Q5 按 Q14 修订后○、Q6 ○、Q7 ×（owner，#4310）、Q8 ○、Q9 ○、Q10 ○、Q11 ○（`set_body` 保持 String signature 并替换类型；`http.with_body[B: Body]` 接受任意 body）、Q12 ○（rc1 未 tag，落地时复查）、Q13 ○、Q14 ○、Q15 ○、Q16 ○（#4311）。同日稍后裁定（owner ○）替换 Q15 和 Q8：`Body` 在 grant 下对 package 开放（D7），SVG 离开 stdlib 进入 almide/svg package；Q17–Q23 按推荐○。

- **Q1。** protocol 名为 `Body`，要求 `T.media_type()` 和 `to_bytes`。推荐：○。
- **Q2。** 一个 constructor：`http.response[B: Body]`。移除 `http.html`（其 `SafeHtml` 通过 `response`），`with_headers` 接受 `Body` 而非 `String`。推荐：○。
- **Q3。** JSON 类型化：`json.Value` 以 `application/json` 符合 `Body`。`http.json(status, String)` 保留 text 形式，因为以 JSON 加 nosniff serve 的 JSON 不运行 script。推荐：○。
- **Q4。** 拒绝经 `set_header` / `with_headers` 写 Content-Type：literal 名为 E116，否则 abort。替代方案为像 framing field 一样静默丢弃。推荐：○（拒绝）。
- **Q5。** 其他 media type 经 `http.typed(media_type, Bytes) -> Result[Typed, String]`：格式错误或已拥有类型为 `err`，literal 类型为 E117。推荐：○。
- **Q6。** 在安全类型以 `@media_owns(patterns…)`（RFC 6839 pattern）声明所有权。`typed` 拒绝声明并集。推荐：○。
- **Q7。** active XML（除 xhtml/svg 的 `*/xml`、`*/*+xml`）使用最小 `xml` module，由 `xml.element(name, attrs, children)` / `xml.text` / `xml.raw` 构造拥有这些 pattern 的 `SafeXml`。若×，仅声明的 owner 使其保持不可构建。推荐：暂时×。corpus 无 program serve XML；仅声明 owner 保持拒绝 pattern，将来可无 break 添加 `xml` module。
- **Q8。** `svg` 仿照 `html` builder 集合：`escape`、`attr`、`url_attr`、`concat`、`empty`、`to_string`、`raw`，再加 `svg.element(name, attrs, children)`。推荐：○。
- **Q9。** `String` body 为 `text/plain; charset=utf-8`。当时 `http.response` 初始化的是无 charset 的 `text/plain`，所以可观察。推荐：○。
- **Q10。** `Bytes` body 为 `application/octet-stream`。推荐：○。
- **Q11。** `set_body[B: Body]` 同时替换 body 和 Content-Type，向 HTML response 写 String 变成 `text/plain` 而非 abort。获取 response 保留 upstream 类型直到 `set_body`。推荐：○。
- **Q12。** break 进入 dialect epoch 15（0.69.0，尚未发布），而非新 epoch 16。推荐：落地时 v0.69.0-rc1 尚未 tag 则○，否则 epoch 16。
- **Q14。** `http.typed` 的 media type 仅接受 literal，其他参数为 E117，因此 runtime 无 media 列表。推荐：○。
- **Q15。** `Body` sealed：仅 stdlib 声明符合，user `type T: Body` 是 E118。推荐：○。*已由 D7 替换。*
- **Q16。** Bytes body 通过当时 String-bodied response 表示，非 UTF-8 bytes 因而有损解码；已文档化并提报 #4311。推荐：○。
- **Q13。** response 始终携带 Content-Type：无 body 构建即 `String ""`、`text/plain; charset=utf-8`。当时 `with_headers(st, s, [])` 不发送。推荐：○。

- **Q17。** media grant 指定 dependency source `owner/repo`（非 github.com 用 `host/owner/repo`；path dependency 按 `[dependencies]` 名）。推荐：○。
- **Q18。** application 自身类型如 native code 一样无须 grant，并被列出。推荐：○。
- **Q19。** 非 stdlib `Body` 类型 media fn 为 string literal，若 script-capable，须由自己的 `@media_owns` 覆盖。推荐：○。
- **Q20。** E118（未发布）复用于所有 media-ownership error。推荐：○。
- **Q21。** script-capable 集合只在 `stdlib/html.almd` 声明一次；html 仅拥有 `text/html` 和 `application/xhtml+xml`。推荐：○。
- **Q22。** 两 owner 的 pattern 能匹配同一类型即重复。推荐：○。
- **Q23。** `almide deps` 增加 media-owners 节，无新 command。推荐：○。
- **Q24。** stdlib 外类型如何书写 per-type requirement。原推荐为暂保内部 `__self_media_type` 写法；owner 判相反（现在修）：每个 per-type protocol——`Read`、`Write`、`Close`、`HttpMessage`、`Body`——都以普通名（`fn SafeSvg.media_type(self)`、`fn Conn.close(self)`）满足，所以第三方从不写 `__self_`。公开名在唯一位置 `stdlib_info::requirement_public_name` 导出（移除 requirement 的 `__self_` marker），由 checker conformance lookup（`protocols::requirement_fn_key`）和 monomorphization（`specialization::requirement_target`）读取；marker 留在 stdlib 自身 per-type function 内部。写为 `T.__self_media_type` 的 user method 是不满足任何 requirement 的普通 method，因此类型被拒绝为缺少 `media_type`，提示 `Add: fn T.media_type(...)`。这比另行拒绝 prefix 简洁：一条规则（普通名满足 requirement）、一个已存在 diagnostic，无新文档要求；fixture `tests/diagnostics/protocol-requirement-internal-name` 固定它。没有发布写法变更——本 ADR 前 `Body` sealed，也没有以 `__self_` 书写 I/O 或 message protocol user conformance 的文档或测试——所以没有 epoch 行或 `almide fix` rewrite。

<a id="implementation-plan-after-the-decisions"></a>

## 实现计划（决定后）

1. 先 als：将 C-275 修订为 D1–D4，为每种 body type 及拒绝增加 fixture。推进 pin。
2. `Body` builtin protocol 和 conformer；checker 与 runtime 从声明读取 `@owns_header` / `@media_owns`。E116、E117。
3. media grant（D7）：`[trust] media`、dependency 调查、E118 和 `almide deps` owner 节。SVG 属 almide/svg package。
4. 新 http surface，再进行 `almide fix` rewrite 和 epoch break，与移除临时 guard 及 E115 同一变更。
5. gate：matrix test 检查每个 `Body` conformer 的 media type 与声明一致，`http.typed` 精确拒绝 `@media_owns` 并集，以及没有 header writer 到达 Content-Type。每项配 negative control。
