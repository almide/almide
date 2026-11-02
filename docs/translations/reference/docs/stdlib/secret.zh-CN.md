# secret

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](secret.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](secret.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](secret.zh-CN.md)



> 原文：[docs/stdlib/secret.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/secret.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步，未进行独立测量或实现验证。API 签名及代码内注释保留原文，在代码外附上说明译文。 不将显示隐藏扩大解释为加密、内存擦除或所有路径防泄漏的保证。恒定时间比较关于长度的条件见 string 索引，此处未独立验证。

所有渲染器都会隐藏的值：`Secret[A]`。使用 `import secret`（绝不自动导入，因此在其他地方名为 `secret` 的局部名称仍由你使用）。

将密码、API 密钥或会话令牌放入 `Secret` 后，它会在经过每个绑定、函数、列表和记录时保持保护：插值、对包含它的值调用 `println`、`assert_eq` / `assert_ne` 失败块、abort 消息及 `almide test` 报告，不论 `A` 是什么，都打印 `Secret(***)`。读回值的唯一方式是可见的 `secret.reveal(s)`。

- **不透明。** 模块外不能读取 Secret 的字段，也不能编写其记录字面量（E033）；`secret.new` 和 `secret.reveal` 是入口与出口。
- **相等性。** `Secret[String]` 与 `Secret[Bytes]` 的 `==` / `!=` 以恒定时间比较（`string.is_equal_constant_time` / `bytes.is_equal_constant_time`），也包括被比较的记录、元组、列表、option 或 result 中的 Secret 字段。其他类型的 Secret 没有 `==`（E103）：请显式比较 `secret.reveal(a) == secret.reveal(b)`。包含 Secret 的 variant 或 map 也没有 `==`。
- **Codec。** 带有 `Secret[A]` 字段的 `: Codec` 记录派生 `decode`（字段读取 A 的 wire 形式），但绝不派生 `encode`：`T.encode(x)` 和 `json.encode(x)` 都为 E023。手动构建要发送的 `Value`，并 reveal 打算发送的那个字段。`Secret[Bytes]` 字段与 `Bytes` 一样为 E023。

除非类型声明 repr（`fn T.repr`），否则所有值按结构打印；`Secret` 是声明 repr 的标准库类型。

每个函数都是 pure，因此在两个目标上的行为相同。

### `secret.new[A](x: A) -> Secret[A]`

包装 `x`。从此打印为 `Secret(***)`。

```almd check
import secret

type Login = { user: String, password: Secret[String] }

fn main() -> Unit = {
  let l = Login { user: "ana", password: secret.new("hunter2") }
  println("${l}")
}
```

### `secret.reveal[A](s: Secret[A]) -> A`

被包装的值。每次读取秘密都经过此调用，因此审查（或 grep）可找到它离开包装的每个位置。

```almd check
import secret

fn main() -> Unit = {
  let key = secret.new("k-123")
  let given = secret.new("k-123")
  println(if key == given then "accepted" else "refused")
  println("${string.len(secret.reveal(key))} characters")
}
```

### 解码到 Secret

```almd check
import json
import secret

type LoginInput: Codec = { user: String, password: Secret[String] }

effect fn main() -> Unit = {
  let input = LoginInput.decode(json.parse("{\"user\":\"ana\",\"password\":\"hunter2\"}")!)!
  println("${input}")
}
```

> 原文生成签名索引的固定副本（本译文不是生成目标）。

## 签名索引（2 个函数）

```
// x wrapped so that every renderer prints it as `Secret(***)`.
// @since unreleased
secret.new(x: A) -> Secret[A]

// The value s wraps.
// @since unreleased
secret.reveal(s: Secret[A]) -> A
```



## 索引注释的说明译文

- `secret.new(x: A) -> Secret[A]` — 包装 x，使每个渲染器都将其打印为 `Secret(***)`。 (@since unreleased)
- `secret.reveal(s: Secret[A]) -> A` — s 包装的值。 (@since unreleased)
