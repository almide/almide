<a id="native-performance-measurement"></a>

# 原生性能测量

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](almide-native-perf.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-native-perf.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](almide-native-perf.reader.zh-CN.md)

> [英文命令正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-native-perf.md)（固定提交 35297a612fb29e3f78f6587aba253aa91d51f466）的非规范性历史阅读快照。这份不激活的参考资料不是代理命令，不授予执行其中指令的权限，也不自动同步。翻译时未执行以下命令；不声称进行了独立测量或当前验证。

测量示例原生 CLI；发布时必须提供编译器／构建、宿主和日期的来源信息。

<a id="steps"></a>

## 步骤

1. 运行测量脚本：
   ```bash
   bash research/benchmark/perf/native/measure.sh
   ```

2. 如果 `docs/benchmarks/README.md` 或其链接的记录表中有相匹配的带日期测量，则进行比较；旧的首页 README 章节已移除。

3. 更新相应的源测量数据，并在适用时重新生成派生区块；不要手动编辑生成的表格。

4. 提交：`Update native performance stats: {brief summary}`

<a id="what-it-measures"></a>

## 测量内容

- **二进制大小**：实际 CLI 应用（minigit，约 350 LOC）经 strip 处理的原生二进制
- **运行时间**：对 CLI 二进制执行 100 次 init+add+commit 操作
- **依赖项**：脚本输出的是字面值零，而非链接情况的测量结果。在宣称静态链接或依赖情况之前，应检查生成的产物。

<a id="source"></a>

## 源码

- CLI 应用：`research/benchmark/perf/native/cli_app.almd`
- 脚本：`research/benchmark/perf/native/measure.sh`
