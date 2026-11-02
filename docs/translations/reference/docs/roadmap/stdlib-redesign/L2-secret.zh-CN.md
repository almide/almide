<a id="l2l3-secret"></a>

# L2/L3：secret

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-secret.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-secret.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-secret.zh-CN.md)

> 这是固定于 35297a612fb29e3f78f6587aba253aa91d51f466 的[正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-secret.md)的非规范性阅读译本，不会自动同步，原本仍具权威性。安全性、安全防护、比较、实现和测量主张仅作为该设计时点原本的陈述保留，未经独立执行、计时测量、审计或认证，不构成当前版本的新保证。代码块和注释原样保留，元数据为已停用的阅读注记。

> 原本说明（已停用元数据）: 标准库重设计 L2/L3 — secret：每个 renderer 都显示为 Secret(***) 的 Secret[A]、常数时间 ==、Codec 仅 decode。

> 最后更新：2026-10-24
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-31）、#4107
> 来源： Rust https://docs.rs/secrecy ・ Python https://docs.pydantic.dev/latest/api/types/#pydantic.types.SecretStr ・ Go https://pkg.go.dev/log/slog#LogValuer ・ Kotlin https://kotlinlang.org/docs/data-classes.html ・ Swift https://developer.apple.com/documentation/os/oslogprivacy ・ TS/JS https://nodejs.org/api/util.html#utilinspectcustom ・ Elixir https://hexdocs.pm/elixir/Inspect.html ・ https://hexdocs.pm/ecto/Ecto.Schema.html#field/3 ・ Java https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/lang/Record.html#toString()
> 实测：`almide 0.68.0 (dev, fcb2e6827)`，native 与 `--target wasm`。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust (secrecy) | Python (pydantic) | Go | Kotlin | Swift | TS/JS | Elixir | Java |
|---|---|---|---|---|---|---|---|---|---|
| 包装秘密的类型 | `Secret[A]`、`secret.new` | `SecretBox<T>` | `SecretStr`、`Secret[T]` | —（手写 `String()`） | —（手写 `toString`） | —（`os.Logger` 的 `<private>`） | — | —（`@derive {Inspect, except:}`） | — |
| 提取内容 | `secret.reveal` | `expose_secret()` | `get_secret_value()` | — | — | `privacy: .public` | — | 读取字段 | — |
| 显示 | 始终为 `Secret(***)` | `[REDACTED]` | `'**********'` | 手写 | 手写 | `<private>` | 手写 | `**redacted**` | 手写 |
| 写出（encode） | 拒绝（E023） | 拒绝（需要 `SerializableSecret`） | 以掩码文本写出 | 原样写出 | 原样写出 | 原样写出 | 取决于 `toJSON` | 单独配置 | 原样写出 |
| 读入（decode） | 派生 | 派生 | 派生 | — | — | — | — | — | — |
| `==` | 常数时间（String / Bytes），其他为 E103 | 无 | 值比较 | — | — | — | — | — | — |

默认显示记录全部字段的是 8/8。隐藏内容在每种语言中都是显式选择。专用类型占 2/8（Rust secrecy、pydantic），两者都不隐式暴露内容。

<a id="完全性の規則"></a>

### 完整性规则

1. 进入 `Secret[A]` 的途径是 `secret.new`，离开的途径是 `secret.reveal`，只有这两种。在模块外读取字段和使用记录字面量会产生 E033。
2. 显示从单一声明（`Secret.repr`）覆盖所有路径：插值 `"${…}"`，外层记录、列表、Option、Result、元组、Map 内部，`assert_eq` / `assert_ne` 的失败块，abort 消息，以及 `almide test` 报告。每条路径均为 `Secret(***)`。
3. 只有 `Secret[String]` 和 `Secret[Bytes]` 具有 `==`，以常数时间比较。其他 `A` 的 `Secret`、含有 `Secret` 的 variant 和 Map 不具有 `==`（E103）。
4. Codec：`Secret[A]` 字段按 A 的形状 decode，不派生 encode（E023）。`Secret[Bytes]` 与 `Bytes` 一样为 E023。
5. **有意省略**：不提供 Hash、Ord（Map 的键、排序键）。避免 `map` / `set` 内容显示或是否存在比较本身成为可观察信息（目前尚未在类型层面拒绝）。

<a id="l2-の判定"></a>

### L2 判定

- 没有缺失的角色（5 种以上语言）。虽仅 2 种语言有专用类型，但只有类型能满足 #4107“从一个声明导出所有显示路径”的要求（字段属性只能保护一个记录的一条显示路径）。
- 实测：`Login { user: "u", pw: secret.new("CANARY") }` 的 `"${l}"`、`[l]`、`some(s)`、`(1, s)`、variant 的 payload、`map.from_list([("k", s)])`，在 native 和 wasm 上均包含 `Secret(***)`，不包含金丝雀值。

<a id="案"></a>

## 提案

<a id="r-secret-01-secretnew-と-secretreveal-を足す---採用"></a>

### R-secret-01 添加 `secret.new` 和 `secret.reveal` — ○ 采纳

- 内容：`fn secret.new[A](x: A) -> Secret[A]`、`fn secret.reveal[A](s: Secret[A]) -> A`。需要 `import secret`（不自动 import）。
- 依据的 L0 规则：R-L0-00（2/8 的类型形式）、R-L0-03（主语在前）、R-L0-30（元素值为 `x`，主语名称为 `s`）。
- 写法：没有主语的构造器为 `new`（R-L0-02，与 `map.new`、`regex.new` 相同）。提取操作没有多数写法（Rust secrecy 为 `expose_secret`，pydantic 为 `get_secret_value`，Go、Swift、Kotlin 没有）。`to_string` 是 `Repr`（R-L0-38）的名称，对 Secret 返回 `Secret(***)`，因此不能采用（`html.to_string` 返回 `SafeHtml` 内容，但 SafeHtml 不是隐藏类型）。Almide 没有 `unwrap`（统一为 `??`，R-L0-06）。选择 `reveal`，作为没有其他含义、可用 grep 找到内容提取位置的动词。
- protocol（R-L0-37）：`Repr`（始终为 `Secret(***)`），仅当 `A` 是 `String`、`Bytes` 时具有 `Eq`（有条件遵循，其他为 E103），没有 `Hash`、`Ord`，`Codec` 只有 decode 这一半。待有条件遵循进入语言后，再列入 [protocol-view](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) 的类型表（设想中的设计）。
- (b) 纯操作。两个 target 相同。
- (c) 常见名为 `secret` 的局部变量，自动 import 会造成冲突。因此采用能看到 import 行的显式启用方式。
- issue：#4107

<a id="r-secret-02-表示は-secret宣言-1-か所---採用"></a>

### R-secret-02 显示为 `Secret(***)`，仅一处声明 — ○ 采纳

- 内容：插值、容器内部、assert / abort、测试报告均读取类型的 `repr` 声明（`Secret.repr`）。类型未声明 repr 的值继续按原有结构显示。
- issue：#4107

<a id="r-secret-03--は定数時間string--bytes-以外は-e103---採用"></a>

### R-secret-03 `==` 为常数时间，String / Bytes 以外为 E103 — ○ 采纳

- 内容：`Secret[String]` / `Secret[Bytes]` 的 `==` / `!=` 使用 `string.is_equal_constant_time` / `bytes.is_equal_constant_time` 比较。记录、元组、列表、Option、Result 中的 `Secret` 也相同。
- issue：#4107（使用 #4106 的常数时间比较）

<a id="r-secret-04-codec-は-decode-だけ導出encode-は-e023---採用"></a>

### R-secret-04 Codec 仅派生 decode，encode 为 E023 — ○ 采纳

- 内容：采用 Rust secrecy 的“拒绝”一列。若以掩码文本写出（pydantic），`***` 会悄悄进入数据库或外发请求，在远离原因的位置导致故障。
- issue：#4107

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，显示 mut） | 其他语言的主要写法 | 判定 | 提案 |
|---|---|---|---|---|
| `new` | `secret.new[A](x: A) -> Secret[A]` | Rust `SecretBox::new` / pydantic `SecretStr(x)` | 添加 | R-secret-01 |
| `reveal` | `secret.reveal[A](s: Secret[A]) -> A` | Rust `expose_secret()` / pydantic `get_secret_value()` | 添加 | R-secret-01 |

## 原本注意事项

原本同时包含已采纳的新增项、金丝雀实测报告和设想中的有条件遵循设计。保留其关于 Hash/Ord 相关用途尚未在类型层面拒绝的明确说明。显示隐藏和常数时间比较属于固定设计记录的主张，不是新的安全认证、独立计时分析或对所有信息泄漏路径的保证。
