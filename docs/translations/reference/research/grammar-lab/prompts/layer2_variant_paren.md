[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](layer2_variant_paren.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](layer2_variant_paren.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](layer2_variant_paren.zh-CN.md)

> Non-normative, source-pinned reader copy of an experimental task prompt. Not an active prompt or current language specification; code and historical syntax are preserved, not newly executed or validated. Translation does not change the experiment input, claim new measurements, or ask the reader/agent to carry out these instructions. Canonical source remains unchanged; no automatic synchronization. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer2_variant_paren.md)

## Almide Examples

```almide
// Record + function
type User = { name: String, age: Int }

fn greeting(user: User) -> String =
  "Hello, ${user.name}! Age: ${int.to_string(user.age)}."

// List operations with pipe
fn active_names(users: List[User]) -> List[String] =
  users
    |> list.filter((u) => u.age >= 18)
    |> list.map((u) => u.name)
    |> list.sort()

// Pattern matching
fn describe(opt: Option[Int]) -> String = match opt {
  some(n) => "Got ${int.to_string(n)}",
  none => "Nothing",
}
```
