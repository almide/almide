[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](layer2_variant_fn.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](layer2_variant_fn.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](layer2_variant_fn.zh-CN.md)

> 实验任务提示词的非规范、固定原文读者副本。不是活动提示词或当前语言规范；代码与历史语法保留，未重新执行或验证。翻译不修改实验输入，不宣称新测量，也不要求读者或代理执行这些指示。原文不变，不自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer2_variant_fn.md)

<a id="almide-examples"></a>

## Almide 示例

```almide
// Record + function
type User = { name: String, age: Int }

fn greeting(user: User) -> String =
  "Hello, ${user.name}! Age: ${int.to_string(user.age)}."

// List operations with pipe
fn active_names(users: List[User]) -> List[String] =
  users
    |> list.filter(fn(u) => u.age >= 18)
    |> list.map(fn(u) => u.name)
    |> list.sort()

// Pattern matching
fn describe(opt: Option[Int]) -> String = match opt {
  some(n) => "Got ${int.to_string(n)}",
  none => "Nothing",
}
```

代码注释：记录与函数、使用管道的列表操作、模式匹配。
