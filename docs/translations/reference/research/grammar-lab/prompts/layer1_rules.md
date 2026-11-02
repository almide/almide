[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](layer1_rules.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](layer1_rules.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](layer1_rules.zh-CN.md)

> Non-normative, source-pinned reader copy of an experimental task prompt. Not an active prompt or current language specification; code and historical syntax are preserved, not newly executed or validated. Translation does not change the experiment input, claim new measurements, or ask the reader/agent to carry out these instructions. Canonical source remains unchanged; no automatic synchronization. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer1_rules.md)

## Almide Rules
- No `return` — the last expression is the value
- `let` is a statement, never an expression (no `let ... in`). Put local bindings in a block `{ }`;
  the block's last line is its value:
    fn area(w: Int, h: Int) -> Int = {
      let s = w * h
      s + 1
    }
- `if` uses `then`: `if x > 0 then "yes" else "no"`
- `+` concatenates strings and lists: `"a" + "b"`, `xs + [y]`
- Lambdas: `(x) => x + 1`; destructure a pair in the parameter: `((a, b)) => a + b`
- Pipes: `xs |> list.map((x) => x * 2) |> list.filter((x) => x > 0)`
- Lists are immutable; `xs[i]` reads an element; `list.take`, `list.drop`, `list.len`, `list.sort_by`
- `match` with list patterns: `match xs { [] => 0, [h, ..rest] => h }`
- No null: `some(v)` / `none` with `Option[T]`; `??` gives a fallback
- Equality: `==` is deep equality on records and lists
- Tests: `test "name" { assert_eq(a, b) }`
