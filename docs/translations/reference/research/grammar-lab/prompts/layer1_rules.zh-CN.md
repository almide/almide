[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](layer1_rules.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](layer1_rules.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](layer1_rules.zh-CN.md)

> 实验任务提示词的非规范、固定原文读者副本。不是活动提示词或当前语言规范；代码与历史语法保留，未重新执行或验证。翻译不修改实验输入，不宣称新测量，也不要求读者或代理执行这些指示。原文不变，不自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer1_rules.md)

<a id="almide-rules"></a>

## Almide 规则
- 没有 `return`——最后一个表达式就是值。
- `let` 是语句，绝不是表达式（没有 `let ... in`）。将局部绑定放入块 `{ }`；
  块的最后一行就是其值：
    fn area(w: Int, h: Int) -> Int = {
      let s = w * h
      s + 1
    }
- `if` 使用 `then`：`if x > 0 then "yes" else "no"`
- `+` 连接字符串和列表：`"a" + "b"`、`xs + [y]`
- Lambda：`(x) => x + 1`；在参数中解构二元组：`((a, b)) => a + b`
- 管道：`xs |> list.map((x) => x * 2) |> list.filter((x) => x > 0)`
- 列表不可变；`xs[i]` 读取元素；`list.take`、`list.drop`、`list.len`、`list.sort_by`
- 使用列表模式的 `match`：`match xs { [] => 0, [h, ..rest] => h }`
- 没有 null：使用 `Option[T]` 的 `some(v)` / `none`；`??` 提供后备值。
- 相等：`==` 对记录和列表进行深度相等比较。
- 测试：`test "name" { assert_eq(a, b) }`
