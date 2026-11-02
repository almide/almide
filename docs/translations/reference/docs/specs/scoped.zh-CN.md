# scoped——可声明的回收边界

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](scoped.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](scoped.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](scoped.zh-CN.md)



> 权威原文：[docs/specs/scoped.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/scoped.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是非规范性的读者快照译文，不自动同步。原规格及其声明的规范地位保持不变。模型证明、实现主张、历史状态和证据边界均保留原文范围；本译文不提供独立执行、测量、符合性或安全性证明。代码块内代码及注释逐字保留。

> 最后更新：2026-09-22

`scoped` 是修饰符，可写在**两个位置**。块位置的 `scoped { … }` 声明内存何时结束，声明位置的 `scoped fn` 则把“可在区域内运行”变成**签名的一部分**（即使编辑函数体或跨模块，资格也不会悄然丢失）。规范是 [docs/specs/als/expressions.md](../../../../specs/als/expressions.md) 中的 ALS-E31，契约为 C-362 / C-363 / C-364。

```almide
type Chain = Nil | Cons(Int, Chain)

scoped fn build(n: Int, acc: Chain) -> Chain =
  if n == 0 then acc else build(n - 1, Cons(n, acc))

scoped fn total(c: Chain, acc: Int) -> Int =
  match c {
    Nil => acc,
    Cons(h, t) => total(t, acc + h),
  }

fn sum_to(n: Int) -> Int = scoped { total(build(n, Nil), 0) }
```

测试：`spec/wasm_cross/scoped_region_value.almd`（C-362）、`spec/wasm_cross/scoped_worker_in_and_out.almd`（C-363）、`spec/lang/scoped_test.almd`、`tests/scoped_region_test.rs`、`tests/diagnostics/e08{6,7,8}-scope-*`（C-364）。

## 语法——`scoped` 是上下文关键字

```ebnf
scoped_block := "scoped" block
fn_decl      := [visibility] ["scoped"] ["effect"] "fn" name …
```

只有两种形式会将 `scoped` 读作修饰符：

- **表达式位置**：`scoped` 后紧接着**同一行**的 `{`。
- **声明位置**：其后紧接 `fn` 或 `effect`。

其他情况下，`scoped` 仍是标识符。在 `match` / `while` / `for … in` 的**头部**，后面的 `{` 打开语法本身的主体，因此始终把 `scoped` 读作标识符——`match scoped { … }` 在 #1997 前后含义相同。因此，已有程序中的变量、函数、模块即使名为 `scoped` 也不会被破坏，此次引入本身没有提高方言 epoch（引入时为 epoch 4，当前值及后续变更见 `proofs/dialect-epochs.toml`）。若确实要在头部写作用域块，用括号包围：`while (scoped { step(i) }) > 0 { … }`。

`almide fmt` 按 `[visibility] scoped [effect] fn` 顺序往返。

## scoped 块承诺什么

**`scoped { body }` 的值就是 `body` 的值**，删除 `scoped` 不改变 stdout、stderr 或退出码（C-362）。修饰符声明的是**内存回收边界**：为块分配的内存至迟在该边界回收，指向区域的引用不跨越边界。

各执行路径的实现不同。

| 路径 | 实现 |
|---|---|
| 结构化 wasm | 块入口用 `RegionSave` 将 bump 指针及 16 个大小类别链表头保存到一个保存块中，并清零链表头；出口由 `RegionRestore` 回退。块内分配的所有块一次消失（`crates/almide-wasm/src/region.rs`） |
| native | 调用点变为 `almide_region_window(\|\| …)`；`__rgn_` 双生体（带有 `Copy` 句柄 `AlmideRgn<T>` 的双生 enum）在前奏 arena 中运行。不能双生化的闭包在**相同的闭合点**释放所有权（`crates/almide-codegen/src/pass_region_window.rs`） |

**不作的主张**：窗口内峰值内存上界、终止性、边界前分配量上界。这些需要其他契约和基准测试 oracle。

## scoped fn 承诺什么

`scoped fn` **在区域外是普通函数**。无论直接调用还是从 `scoped { … }` 内调用，答案都相同，且两个目标一致（C-363）。修饰符只要求**检查函数体**，也就是将“该函数可在区域内运行”的承诺变为**义务**，而不是靠发现。

stage 1 接受的片段：

- **值**：标量（`Int` / `Float` / `Bool` / `Unit`）、同文件声明的非泛型变体类型（每个 case 是 unit 或位置字段，字段为标量或满足相同条件的变体）、只有标量字段的记录，以及它们的 tuple 和 `Option`。
- **worker**：普通 worker（包括树遍历形状）及**直接尾递归** worker。
- **可调用及使用的形式**：其他 `scoped fn`、构造器、`int` / `float` / `math` / `bool` 的标量 stdlib 调用、标量运算符、`if` / `match` / 块 / 局部绑定 / 范围 `for` / `while`。

## 拒绝什么，为什么

片段之外的形状在**检查时**拒绝。检查先于目标选择，因此 `almide check` 与 `almide check --target wasm` 判定相同（C-364）。后端不决定是否接受：结构化 wasm 路径若与检查不一致，那是 E083（编译器缺陷）而不是 wall。包含 `scoped` 的程序若进入该路径之外的路线，会被拒绝，而不是没有边界地交付。

| 代码 | 拒绝内容 | 文档 |
|---|---|---|
| E086 | 区域值跨越边界的形状（块的值、向外写回） | [E086](../../../../diagnostics/E086.md) |
| E087 | 区域内不允许的操作（非 `scoped` 调用、全局变量、宿主资源、未知间接调用、外部堆捕获、片段外类型、`!` / `guard` / `?`） | [E087](../../../../diagnostics/E087.md) |
| E088 | 保留续体的递归形状（单一非尾自递归、相互递归） | [E088](../../../../diagnostics/E088.md) |

E086 有 `= allocated at <file:line:col>` 与 `= scope ends at <file:line:col>` 两行；E087 指出“可能保留什么”和“哪个声明提出了要求”；E088 指出“调用后还剩什么”。

树遍历形状（一个分支中有两次及以上自身调用）无法改写为累加器，因此**作为普通 worker 接受**。E088 捕获的是可通过累加器化修复的单一非尾递归，以及往返中保留激活记录的相互递归。

## stage 1 尚未完成的内容

- `scoped` 仅限入口程序。导入模块的 `scoped fn` / `scoped` 块以 E087 拒绝（因为 native 区域 pass 将根的函数和类型声明双生化）。
- `String` 和 `T!` 在片段外。设计（#1997）冻结了“由分离字节创建的新 String 可离开”和“`T!` 保持 `Result[T, String]`”的决定，但结构化 wasm 路径边界处的分离（回退前复制到区域外）不含在 stage 1。二者均以 E087 / E086 拒绝，不存在悄然通过的形式。
- `scoped effect fn` 在语法上接受，在检查时产生 E087。
