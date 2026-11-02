[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](layer2_match.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](layer2_match.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](layer2_match.zh-CN.md)

> Non-normative, source-pinned reader copy of an experimental task prompt. Not an active prompt or current language specification; code and historical syntax are preserved, not newly executed or validated. Translation does not change the experiment input, claim new measurements, or ask the reader/agent to carry out these instructions. Canonical source remains unchanged; no automatic synchronization. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer2_match.md)

## Almide Examples — Option Handling (match style)

```almide
// Record + function
type User = { name: String, age: Int }

// Option handling with match
fn find_age(users: List[User], target: String) -> Int =
  match list.find(users, (u) => u.name == target) {
    some(u) => u.age,
    none => -1,
  }

// Pipe into match
fn youngest_name(users: List[User]) -> String =
  users
    |> list.sort_by((u) => u.age)
    |> list.first()
    |> match {
      some(u) => u.name,
      none => "nobody",
    }

// Nested match for chained lookups
fn greet_oldest(users: List[User]) -> String =
  match list.first(users |> list.sort_by((u) => u.age) |> list.reverse()) {
    some(u) => "Hello, ${u.name}!",
    none => "No users",
  }
```
