<a id="l2l3-compute"></a>

# L2/L3：compute

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-compute.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-compute.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-compute.zh-CN.md)

> 这是固定于 35297a612fb29e3f78f6587aba253aa91d51f466 的[正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-compute.md)的非规范性阅读译本，不会自动同步。原本仍具权威性。路线图决策、日期、比较、实现及测量主张未经独立执行或重新验证而保留，并非对当前版本行为的断言。原本元数据以阅读注记显示，不作为生成器的有效输入。

> 原本说明（已停用元数据）: 标准库重设计 L2/L3 — compute：6 个确定性计算时间构造器按 ADR-0001 的决定保持不变；添加 compare；9 种语言中没有对应项。

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-21：不与 `duration` 合并）、[ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md)、[ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md)
> 相关：[L2-duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md)
> 来源： Rust https://doc.rust-lang.org/std/time/struct.Duration.html ・ Go https://pkg.go.dev/time#Duration ・ Python https://docs.python.org/3/library/time.html#time.process_time ・ TS/JS https://tc39.es/proposal-temporal/docs/duration.html ・ Gleam https://hexdocs.pm/gleam_time/ ・ Elixir https://hexdocs.pm/elixir/Process.html#info/2（reductions） ・ OCaml https://ocaml.org/manual/latest/api/Sys.html（`Sys.time` 是 CPU 时间） ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.time/-duration/ ・ Swift https://developer.apple.com/documentation/swift/clock

`Compute` 是“确定性计算时间”的量，以冻结的抽象机的 charge（函数入口与循环头部）计数。预算判定仅由程序和输入决定，在任何 target、任何主机上都相同。它是 `fan.bounded` 读取、`fan.min(compute…)` 选择的时钟。9 种语言中没有相同的概念（[facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md) 第 3.3 节：较接近的是 BEAM 的 reductions 数与 CPU 时间，但两者都不是预算类型）。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 确定性计算时间的量 | `ns` `us` `ms` `s` `min` `h` | — | — | —（`process_time` 是 CPU 时间，非确定性） | — | — | —（reductions 是数量而非类型） | —（`Sys.time` 是 CPU 时间） | — | — |
| 加、减、整数倍、比较 | `+` `-` `* Int` `<`（`Ord`） | — | — | — | — | — | — | — | — | — |
| 用作预算 | `fan.bounded(c)`、`fan.min(c, …)` | — | — | — | — | — | — | — | — | — |
| 与实际时间混用 | 类型错误（S3） | — | — | — | — | — | — | — | — | 按 `Clock` 区分 `Instant` |
| 以数值读取 | —（S7） | — | — | — | — | — | — | — | — | — |

<a id="完全性の規則"></a>

### 完整性规则

与 [L2-duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md) 的规则 1–4 相同（ADR-0001 的 S2、S3、S7 要求两个时钟具备相同的接口面）。`compare` 也与 `duration` 同时添加。

<a id="l2-の判定"></a>

### L2 判定

- 对于 9 种语言中没有对应项这一点，L1 R-L1-21 已裁定“存在 Almide 特有的理由（确定性承诺），因此不随多数”。
- 缺失单元格：`compare`（R-L0-16）→ R-compute-01。
- 重复：无。

<a id="案"></a>

## 提案

<a id="r-compute-01-computecompare-を足す---採用"></a>

### R-compute-01 添加 `compute.compare` — ○ 采纳

- 内容：`compute.compare(a: Compute, b: Compute) -> Int`。
- 依据的 L0 规则：R-L0-16（有序类型具有 `compare`）。根据 ADR-0001 S1 的“两种类型具有相同的操作接口面”，与 `duration.compare` 成对添加。
- (b) 纯计算。charge 数是确定的，因此比较结果在两个 target 上也相同。
- (c) 为预算列表排序时可写作 `list.sort_with(cs, compute.compare)`。
- (d) 新增。使用次数 0。
- 门禁提案：与 R-duration-01 相同的门禁（为派生 `Ord` 的标准类型提供 `compare`）。
- issue：#3902

<a id="r-compute-02-duration-と統合するまたは読み出し関数を足す---不採用"></a>

### R-compute-02 与 `duration` 合并或添加读取函数 — × 不采纳

- 内容：两者都不做。遵循 ADR-0001 的决定（两个时钟是不能互相转换的不同类型，S7 省略读取）以及 L1 R-L1-21。UFCS 的 `n.ms()` 产生歧义错误，也是为了维护这种分离（S2）。
- 保持类型独立，同时通过共同协议 `TimeAmount` 声明相同的形状 → [L2-duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md) R-duration-03。
- issue：无

<a id="r-compute-03-compute-を共通のプロトコル-timeamount-に従わせる---採用"></a>

### R-compute-03 使 `Compute` 遵循共同协议 `TimeAmount` — ○ 采纳

- 内容：这是 [L2-duration](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md) R-duration-03 的另一半。`Compute` 和 `Duration` 都遵循 `TimeAmount`（6 个单位的构造器与 `compare`）。区别仅在时钟：`Duration` 是墙钟时间，`Compute` 是确定性计算时间（不是 OS 的 CPU 时间，而是在任何主机上都相同的 charge 数）。
- issue：#3964

<a id="l3-関数表"></a>

## L3：函数表

`compute` 的现有 6 个函数与将新增的 1 个函数。

| 函数 | 签名（当前，显示 mut） | 其他语言的主要写法 | 判定 | 提案 |
|---|---|---|---|---|
| `ns` | `compute.ns(n: Int) -> Compute` | —（9 种语言中无对应项；实际时间则有 Rust `Duration::from_nanos`） | 保持 | — |
| `us` | `compute.us(n: Int) -> Compute` | —（实际时间则有 Rust `from_micros`） | 保持 | — |
| `ms` | `compute.ms(n: Int) -> Compute` | —（实际时间则有 Rust `from_millis`、Go `time.Millisecond`） | 保持 | — |
| `s` | `compute.s(n: Int) -> Compute` | —（实际时间则有 Rust `from_secs`） | 保持 | — |
| `min` | `compute.min(n: Int) -> Compute` | —（实际时间则有 Rust `from_mins`） | 保持 | — |
| `h` | `compute.h(n: Int) -> Compute` | —（实际时间则有 Rust `from_hours`） | 保持 | — |
| `compare` | （新增）`compute.compare(a: Compute, b: Compute) -> Int` | Rust `cmp` / Kotlin `compareTo` | 添加 | R-compute-01 |

## 原本注意事项

现有 6 个函数与新增 1 个函数的表、采纳提案、协议遵循、使用次数及跨目标确定性主张，均描述固定的路线图，而非独立检查过的当前 API 或实现。计算时间仍与墙钟 duration 以及 OS CPU 时间分别区分。
