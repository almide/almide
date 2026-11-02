[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](layer2_combinator.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](layer2_combinator.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](layer2_combinator.zh-CN.md)

> Non-normative, source-pinned reader copy of an experimental task prompt. Not an active prompt or current language specification; code and historical syntax are preserved, not newly executed or validated. Translation does not change the experiment input, claim new measurements, or ask the reader/agent to carry out these instructions. Canonical source remains unchanged; no automatic synchronization. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer2_combinator.md)

## Almide Examples — Option Handling (combinator style)

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
