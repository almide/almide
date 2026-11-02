[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](layer2_combinator.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](layer2_combinator.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](layer2_combinator.zh-CN.md)

> 实验任务提示词的非规范、固定原文读者副本。不是活动提示词或当前语言规范；代码与历史语法保留，未重新执行或验证。翻译不修改实验输入，不宣称新测量，也不要求读者或代理执行这些指示。原文不变，不自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer2_combinator.md)

<a id="almide-examples--option-handling-combinator-style"></a>

## Almide 示例——Option 处理（combinator 风格）

```almide
// Record + function
type User = { name: String, age: Int }

// Option handling with combinators
fn find_age(users: List[User], target: String) -> Int =
  list.find(users, (u) => u.name == target)
    |> option.map((u) => u.age)
    |> option.unwrap_or(-1)

// Pipeline with option.map
fn youngest_name(users: List[User]) -> String =
  users
    |> list.sort_by((u) => u.age)
    |> list.first()
    |> option.map((u) => u.name)
    |> option.unwrap_or("nobody")

// Chained lookups with option.map
fn greet_oldest(users: List[User]) -> String =
  users
    |> list.sort_by((u) => u.age)
    |> list.reverse()
    |> list.first()
    |> option.map((u) => "Hello, ${u.name}!")
    |> option.unwrap_or("No users")
```

代码注释：记录与函数、用 combinator 处理 Option、option.map 管道、用 option.map 串联查找。
