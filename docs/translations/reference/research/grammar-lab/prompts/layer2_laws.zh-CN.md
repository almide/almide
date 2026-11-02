[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](layer2_laws.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](layer2_laws.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](layer2_laws.zh-CN.md)

> 实验任务提示词的非规范、固定原文读者副本。不是活动提示词或当前语言规范；代码与历史语法保留，未重新执行或验证。翻译不修改实验输入，不宣称新测量，也不要求读者或代理执行这些指示。原文不变，不自动同步。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer2_laws.md)

<a id="laws"></a>

## 法则
`law` 块陈述必须对每个输入都成立的性质：

    law "reverse twice is identity" for (xs: List[Int]) {
      list.reverse(list.reverse(xs)) == xs
    }

检查器生成许多输入（包括空列表、零和负数），若性质为假，则报告找到的最小反例。`where it >= 1` 限制输入。
法则由任务固定：不要修改或复制法则，而要修改代码使其成立。
