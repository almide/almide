# 分区运行时要求 (#568)

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./PARTITIONED-RUNTIME.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](./PARTITIONED-RUNTIME.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](./PARTITIONED-RUNTIME.zh-CN.md)

本文说明分区式宿主（ARINC 653 式的时间／空间分区，或任何按组件分配内存与 CPU 预算的环境）运行 Critical 配置的 Almide wasm 产物时必须提供什么，以及产物本身保证什么。以下具体产物检查针对 `tests/static_memory_test.rs` 中的 PID 参考应用，并非每个 Critical 配置程序或每条 Wasm 组件路径。必须针对每个部署产物审查授予的能力和实际导入项。每行都列出了对应门禁。

## 产物保证什么（空间）

| 属性 | 机制 | 门禁 |
|---|---|---|
| 固定堆预算 | `almide build --heap-cap <bytes>` 将 bump frontier 上限固化到模块中；超出预算会在确定性位置触发已定义的中止（`Error: out of memory`，退出码 1），绝不会静默增长 | `tests/static_memory_test.rs::fixed_heap_budget_suffices_and_is_enforced` |
| 稳态分配纪律 | `almide check --profile critical`（#567）拒绝计数循环内的分配、运行时长度的堆构造器、闭包、递归和 `while`；通过 critical 检查的程序，其循环体最多只分配有界的临时对象（为声明的 print 能力进行标量到字符串转换） | `tests/static_memory_test.rs::reference_app_is_critical_clean` |
| 无分配的内核算术 | 控制律函数生成的 Rust 不含堆相关词元（无 Vec/Box/String/format!/clone/RcCow） | `tests/static_memory_test.rs::kernel_fns_emit_allocation_free_rust`（Float 内核另见 `tests/wcet_kernel_test.rs`） |
| 单一非共享线性内存，无线程导入 | 参考核心产物有一个非共享内存，不存在名称／模块包含 `thread` 的导入项；此测试并不断言所有产物都只有同样的五种操作导入 | `tests/static_memory_test.rs::artifact_is_partition_shaped` |
| 有界栈 | 在 `--profile critical` 下，调用图为 DAG（递归被拒绝，E073），因此栈深度受静态调用图深度约束；wasm 自身的栈还受宿主限制 | 配置门禁（`tests/critical_profile_test.rs`） |

## 宿主必须提供什么

1. **实现实际产物导入项的宿主**：检查构建出的模块／组件及其获授能力。其他程序可能需要文件系统、网络或私有进程操作；并非都会转换为参考实例的五操作接口。打包选择核心或组件路径（`src/cli/build_wasm.rs`）；serve 形式的 HTTP 构建使用 WASI 0.3 handler 组件。
2. **内存分区**：线性内存声明初始页数；执行每分区内存配额的宿主还可设置 wasmtime `StoreLimits` 上限。当产物自身的 `--heap-cap` 预算更严格时，它会先触发中止，保持既定的失败语义。
3. **时间分区**：宿主必须提供并验证其抢占与宿主调用超时策略。确定性计算计量器（C-320）以协作方式限制模型化的计算开销；它不会抢占阻塞的宿主调用，也不能单独证明硬件时间片。
4. **并发策略**：验证选定路径与宿主服务。参考产物的无线程导入检查，并不能证明每项宿主服务或其他所有 Almide 产物都是单线程的。

## 资格鉴定边界

为这类产物提供经过资格鉴定／最小化的执行环境——使用哪个宿主、如何鉴定——属于 #865 的范围，不属于本文。本文是该契约的产物侧部分：参考产物属性由门禁检查，而实际部署必须确立自己的导入项、资源预算与宿主保证。
