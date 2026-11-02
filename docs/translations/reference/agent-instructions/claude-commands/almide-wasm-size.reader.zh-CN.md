<a id="wasm-binary-size-measurement"></a>

# WASM 二进制大小测量

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-wasm-size.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-wasm-size.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](almide-wasm-size.reader.zh-CN.md)

> [英文命令正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-wasm-size.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范性历史阅读快照。这份不激活的参考资料不是代理命令，不授予执行其中指令的权限，也不自动同步。翻译时未执行以下命令；不声称进行了独立测量或当前验证。

测量这些示例的 WASM 二进制大小；将测量结果与生成的 Hello, world 基线区分开。

<a id="steps"></a>

## 步骤

1. 运行测量脚本：
   ```bash
   bash research/benchmark/perf/wasm-size/measure.sh
   ```

2. 与 `docs/benchmarks/README.md` 及其链接的记录表中相匹配的带日期记录进行比较；旧的首页 README 章节已移除。

3. 对于发布的 Hello, world 基线，使用 `scripts/gen-readme-stats.sh --measure`；其他示例结果应单独标注日期，不要覆盖生成的区块。

4. 提交：`Update WASM binary sizes: {brief summary}`

<a id="source-files"></a>

## 源文件

位于 `research/benchmark/perf/wasm-size/`：
- `hello.almd` — Hello World
- `fizzbuzz.almd` — FizzBuzz
- `fibonacci.almd` — 斐波那契
- `closure.almd` — 闭包（高阶函数）
- `variant.almd` — 变体（代数数据类型＋模式匹配）

<a id="notes"></a>

## 说明

- 所有二进制均自包含（包括分配器、字符串处理和运行时）
- 客体的所有权管理不需要外部 GC；执行宿主操作仍需要兼容的 Wasm 运行时以及产物的导入项。
- 使用 `almide build --target wasm` 构建
