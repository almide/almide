[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](layer2_laws.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](layer2_laws.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](layer2_laws.zh-CN.md)

> Non-normative, source-pinned reader copy of an experimental task prompt. Not an active prompt or current language specification; code and historical syntax are preserved, not newly executed or validated. Translation does not change the experiment input, claim new measurements, or ask the reader/agent to carry out these instructions. Canonical source remains unchanged; no automatic synchronization. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/prompts/layer2_laws.md)

## Laws
A `law` block states a property that must hold for EVERY input:

    law "reverse twice is identity" for (xs: List[Int]) {
      list.reverse(list.reverse(xs)) == xs
    }

The checker generates many inputs (including empty lists, zero and negative numbers) and, when the
property is false, reports the smallest counterexample it finds. `where it >= 1` restricts an input.
Laws are fixed by the task: do not change or copy them, change the code so they hold.
