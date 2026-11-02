[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 供人阅读代理指示的非规范、固定原文副本。权威 CLAUDE.md 不变。CLAUDE.reader 名刻意避免自动加载的指示文件名。这些历史规则不是新指示层，也不是独立验证的实现或安全保证。不自动同步；代码和内联 token 保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-optimize/CLAUDE.md)

> 引用指示是历史参考资料，不授权执行命令、修改实现、提交、推送或发布。

<a id="almide-optimize"></a>

# almide-optimize

IR 层优化 pass：单态化、死代码消除、常量传播。

<a id="monomorphization-mono"></a>

## 单态化（`mono/`）

针对具体类型参数专门化泛型函数。

- **基于 frontier 的发现**——首轮扫描全部函数中的泛型调用点；后续轮次只扫描新建的专门化。没有新实例时收敛。
- **结构约束**——将 `T: { name: String, .. }` 约束解析为具体类型（Dog、Person 等）。
- **失控保护**——生成的专门化超过1000时警告并停止（可能无限展开）。

<a id="optimization-optimize"></a>

## 优化（`optimize/`）

- **DCE**——移除零调用点的函数和未使用变量绑定。
- **常量传播**——折叠常量表达式并传播已知值。

<a id="rules"></a>

## 规则

- **Mono 在 codegen 之前运行。** mono 后 IR 中不应残留 `TypeVar`（不可达路径除外）。
- **Mono 克隆函数。** 专门化函数是替换了类型的副本。原泛型函数若有非泛型调用点则保留。
- **DCE 必须保守。** 仅移除已证明不可达的函数／变量。带副作用的表达式（effect 函数、赋值）即使结果未用也必须保留。
- **优化不得改变语义。** 每个优化 pass 必须保持可观察行为，为边界情况添加回归测试。
