<a id="almide-llm-benchmark-results--historical-typescriptdeno-experiment"></a>

# Almide LLM 基准结果 — 历史 TypeScript/Deno 实验

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](BENCHMARK.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](BENCHMARK.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](BENCHMARK.zh-CN.md)

> 固定原文的非规范性历史阅读快照，以原本为准。不自动同步。所述测试、测量、实现状态和保证是原文主张，不是独立重跑或当前验证。未执行引用代码或命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/exercises/BENCHMARK.md)
> 零知识与充分性主张针对这组历史练习，不是训练数据排除或普遍正确性的证据。旧代码及指令不是当前语法指南；这里把相对 CHEATSHEET 链接固定到源码快照，不声称它就是当时的精确 prompt。

本文记录以前的 TypeScript transpiler 实验，不是当前 Rust/Wasm 编译器的基准。测量结果与旧语法作为证据保留；下述 auto-unwrap、`do`、`++` 和 `src/almide.ts` 说明属于历史资料，不是当前语言指南。

<a id="overview"></a>

## 概述

LLM 能否仅使用[当前 CHEATSHEET.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md)，在**零先验知识**的情况下写出正确的 Almide 代码？

我们让 LLM agent（Claude）仅以 CHEATSHEET 为参考，从零解决 14 道 Exercism 风格练习来检验这一点。同一组练习也用 Python 完成以供比较。

<a id="results"></a>

## 结果

<a id="almide-cheatsheet-only-zero-knowledge"></a>

### Almide（仅 CHEATSHEET，零知识）

| 练习 | 测试 | 通过 | 失败 | 注记 |
|---|---|---|---|---|
| affine-cipher | 16 | 16 | 0 | |
| bob | 25 | 25 | 0 | |
| collatz-conjecture | 6 | 6 | 0 | |
| config-merger | 27 | 27 | 0 | `effect fn` + 文件 I/O 错误传播 |
| grade-report | 30 | 30 | 0 | 跨函数错误传播 |
| hamming | 9 | 9 | 0 | |
| isbn-verifier | 14 | 14 | 0 | |
| isogram | 13 | 13 | 0 | |
| pangram | 10 | 10 | 0 | |
| phone-number | 14 | 14 | 0 | |
| pipeline | 36 | 36 | 0 | `do` block 自动错误传播 |
| raindrops | 18 | 18 | 0 | |
| roman-numerals | 19 | 19 | 0 | |
| scrabble-score | 11 | 11 | 0 | |
| **总计** | **248** | **248** | **0** | **100%** |

<a id="python-baseline-comparison"></a>

### Python（基线比较）

| 练习 | 测试 | 通过 | 注记 |
|---|---|---|---|
| affine-cipher | 16 | 16 | |
| bob | 25 | 25 | |
| collatz-conjecture | 6 | 6 | |
| config-merger | 27 | 27 | |
| grade-report | 30 | 30 | |
| hamming | 9 | 9 | |
| isbn-verifier | 14 | 14 | |
| isogram | 13 | 13 | |
| pangram | 10 | 10 | |
| phone-number | 14 | 14 | |
| pipeline | 36 | 36 | |
| raindrops | 18 | 18 | |
| roman-numerals | 19 | 19 | |
| scrabble-score | 11 | 11 | |
| **总计** | **248** | **248** | **100%** |

<a id="methodology"></a>

## 方法

1. **练习设计**：每道练习提供带 `= _`（hole stub）的函数签名及测试用例，LLM 必须实现所有函数。
2. **Almide agent**：仅提供 `CHEATSHEET.md`，没有其他文档、Almide 代码示例，也不能访问 transpiler 源码。
3. **Python agent**：标准 Python 知识，无特殊约束。
4. **Test runner**：Almide 文件经 `src/almide.ts` 转译为 TypeScript，用 `deno test` 测试。Python 文件用 `pytest` 测试。
5. **单次尝试**：每道练习一次完成，无重试或反馈循环。

<a id="key-findings"></a>

## 主要发现

<a id="the-cheatsheet-is-sufficient"></a>

### CHEATSHEET 已足够
LLM 可以仅凭 340 行速查参考，从零知识写出正确的 Almide 代码。248/248 测试通过（100%）。

<a id="result-erasure--fold-resolved"></a>

### Result erasure + fold：已解决
`let result = list.fold(...); match result { err(_) => ... }` 模式过去会失败，因为 fold 回调中的 `err()` 从 `Array.reduce` 抛出，阻止赋值。修复方法是在 codegen 检测 `let x = expr; match x { err(...) => ... }` 模式，将表达式内联进 match 对象，使 try-catch wrapper 正确捕获抛出。

<a id="almide-advantages-visible-in-complex-exercises"></a>

### 复杂练习中可见的 Almide 优点

**`do` block 自动错误传播**（pipeline 练习）：
```
fn run_pipeline(input: String) -> Result[String, String] = do {
  let pairs = parse_pairs(input)       // auto-unwrap Result
  let pairs2 = validate_keys(pairs)    // error → propagate
  let pairs3 = validate_values(pairs2) // error → propagate
  let pairs4 = transform(pairs3)       // error → propagate
  format_output(pairs4)
}
```
对比 Python（隐式异常传播，没有类型级错误契约）：
```python
def run_pipeline(text):
    pairs = parse_pairs(text)
    pairs = validate_keys(pairs)
    pairs = validate_values(pairs)
    pairs = transform(pairs)
    return format_output(pairs)
```

**带 `err(e)` 模式的 `match`**（grade-report 练习）：
```
match parse_student(line) {
  ok(student) => { ... },
  err(e) => err("line " ++ line_num ++ ": " ++ e),  // catch and re-wrap
}
```

**带文件 I/O 错误传播的 `effect fn`**（config-merger 练习）：
```
effect fn merge_and_save(paths: List[String], output: String) -> Result[Int, String] = {
  match merge_files(paths) {       // file read errors propagate automatically
    err(e) => err(e),
    ok(pairs) => {
      save_config(output, pairs);  // file write errors propagate
      ok(list.len(pairs))
    }
  }
}
```
对比 Python（异常隐式，没有类型级契约）：
```python
def merge_and_save(paths, output):
    merged = merge_files(paths)     # FileNotFoundError? ValueError? Unknown
    save_config(output, merged)     # IOError? Unknown
    return len(merged)
```

<a id="result-erasure-codegen-auto-inlining"></a>

### Result erasure：codegen 自动内联
codegen 现在检测 `let x = expr; match x { ..., err(e) => ... }`，将 `expr` 自动内联进 match 对象，并用 try-catch 包裹。因此直接匹配（`match fn() { err(e) => ... }`）及 let-then-match 模式都正确工作。使用 `assert_eq(expr, err("msg"))` 测试错误情形。

<a id="stdlib-note"></a>

### stdlib 注记
`string.to_int`、`string.to_upper`、`string.to_lower` 现已进入 stdlib，减少早期练习的样板代码。

<a id="historical-runner-commands"></a>

## 历史 runner 命令

```bash
# Run a single exercise
bash exercises/run_exercise.sh exercises/pipeline/pipeline.almd

# Run all exercises
for f in exercises/*/*.almd; do
  echo "=== $(basename $(dirname $f)) ==="
  bash exercises/run_exercise.sh "$f"
done
```

> 代码注释译文（代码本身保持原文）：自动 unwrap Result／错误传播／捕获并重新包装／文件读取错误自动传播／文件写入错误传播。Python 示例注记为“FileNotFoundError 或 ValueError？未知”“IOError？未知”。runner 注记为“运行单道练习”“运行全部练习”。
