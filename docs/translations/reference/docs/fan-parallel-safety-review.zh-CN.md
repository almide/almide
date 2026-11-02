<a id="explicit-fan-routing-safety-review"></a>

# 显式 fan 路由安全性审查

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-parallel-safety-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fan-parallel-safety-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](fan-parallel-safety-review.zh-CN.md)

> 固定原文的非规范性历史阅读快照，以原本为准。不自动同步。所述测试、测量、实现状态和保证是原文主张，不是独立重跑或当前验证。未执行引用代码或命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/fan-parallel-safety-review.md)

#2055 的集成保留 StreamFusion 和 DecodeErrFrame，并移除已废弃的 AutoParallel pass。解决流水线冲突后，codegen 单元测试与集成测试通过。

<a id="plain-functions-can-contain-effects"></a>

## 普通函数也可以包含效果

以下程序检查成功，但原路由 pass 会生成 `almide_rt_fan_map_par`，使元素的打印顺序可能变化：

```almide
fn observe(n: Int) -> Int = {
  println(int.to_string(n))
  n
}
effect fn main() -> Unit = {
  let values = fan.map([1, 2, 3], (n) => ok(observe(n)))!
  println(int.to_string(list.len(values)))
}
```

仅凭声明标志 `is_effect` 不能确立纯度。修复后的路由针对自身输入，重新计算与 StreamFusion 相同的保守、传递性纯度分析，不依赖融合是否运行过。既有可变捕获与 Send-safe 类型检查仍然适用。未知调用和不透明 Rust 不能确立纯度。`tests/nanopass_test.rs` 中的回归测试覆盖普通辅助函数到达 random 运行时的情况。

这遵循 `docs/stream-fusion-safety-review.md` 中关于求值顺序的推理，包括固定版 Gleam 源码（`19bf207ebb7d953ea1391f041da48c214ee1440a`）：将非变量对象只绑定一次以保留效果。参考仓库克隆在 `../almide-references` 下；纯度分析是正确性的准入规则，而非从回调参数类型作出的推断。

<a id="invocation-local-mutation-and-integration"></a>

## 调用局部的修改与集成

StreamFusion 保留显式 fan 块直属的操作，同时仍融合回调内部与隐式 list 流水线。这使后续 fan 路由 pass 仍能使用并行 list 调用。

Fan 的证明通过仅用于分析的 IR 视图，允许局部命令式辅助函数体。只有参数为标量的闭合函数才接受此处理；堆参数可能与调用者存储互为别名，因此保留严格证明。对调用局部绑定的写入被替换为其操作数表达式，循环体和条件保留。这些表达式中的调用仍须通过传递性效果检查。参数写入及对外围状态的访问不能获得准入。变换后的视图从不生成，也从不确立全定义性：StreamFusion 保持原证明及终止性要求。一个回归测试把 random 调用放入循环中被消去的局部赋值的 RHS，验证该函数仍被拒绝。

这一所有权边界与固定版 Rust 参考 `0d31508599a7814a7044e9a7a871e3dc5f037753` 的 `library/std/src/thread/scoped.rs` 一致：scoped spawning 仍要求 `F: Send`；作用域生命周期不会复制被捕获的所有权。Fan 在纯度之外继续保留既有 Send 和可变捕获检查。

<a id="remeasured-parallel-execution"></a>

## 重新测量并行执行

修复后的编译器在 fannkuchredux 的用户函数中生成 `almide_rt_list_par_map`。n=10 时，并行版本、`ALMIDE_FAN_SEQUENTIAL=1` 及手写 Rust 参考均打印 `73196` 和 `Pfannkuchen(10) = 38`。
预热 1 次后，使用 `almide build --release` 和基准测试框架的标准 Rust 标志交错测量 7 次，在这台开发机器上得到的中位数为：并行 54.16 ms、顺序 179.27 ms、Rust 172.47 ms。native/Rust 比为 0.314，顺序/并行比为 3.310。
这些是本地验证结果，不代替 CI runner 的性能门禁，也不对更早的基准版本作出主张。
原始测量：`research/benchmark/perf/results/2026-09-09-fan-repair.json`。

含有效果的普通辅助函数复现程序仍顺序执行，在 native 和 structural WASM 上打印 1, 2, 3, 3。Codopsy 2.2.0 在阈值不变的情况下，将路由和分析文件评为 A。
