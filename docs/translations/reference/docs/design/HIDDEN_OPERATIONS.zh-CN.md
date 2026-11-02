# 隐藏操作

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](HIDDEN_OPERATIONS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](HIDDEN_OPERATIONS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](HIDDEN_OPERATIONS.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/HIDDEN_OPERATIONS.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> Zig 的原则："no hidden control flow, no hidden memory allocations"
> Almide 有意隐藏某些操作。本文记录所有这些操作。

---

## 1. 自动插入 Clone（Rust target）

### 会发生什么

非 Copy 类型（String、List、Map、Record、Variant）的变量被多次使用时，编译器会自动插入 `.clone()`。

### 条件

- `use_count > 1` — IR 的 use-count 分析统计变量的使用次数。
- `!is_copy` — 不对 Int、Float、Bool 等 Copy 类型插入。
- 最后一次使用时不 clone（以 move 传递）。

### 文件

- `crates/almide-codegen/src/pass_clone.rs` — Clone 插入 pass
- `crates/almide-ir/src/use_count.rs` — use-count 分析

### 对用户的影响

- 性能：可能插入不必要的 clone（正确性优先的设计）。
- 优化：borrow inference（`pass_borrow_inference*.rs`）会减少 clone（不改变已有代码的行为）。v1 spine 侧的别名分析位于 `crates/almide-mir/src/alias_safety.rs`。

---

## 2. 错误传播——**不隐藏**（auto-`?` 已废止）

本节曾记录 auto-`?` 插入（在 `effect fn` 内为可失败调用隐式添加 `?`）。**已由 ADR-0008 废止**。传播只有一种写法：只有后缀 `!` 才会降为 `?`。

```almide
effect fn load() -> String = {
  let text = fs.read_text("file.txt")   // E041 — Result 値のまま、伝搬しない
  let text = fs.read_text("file.txt")!  // これが `?` に落ちる
  text
}
```

不写 `!` 的可失败调用是 Result **值**，不是控制流：

- 无类型注解的绑定 → **E041**（`let x = f()`）
- 在语句位置丢弃 → **E042**（must-use。使用 `f()!` 或 `let _ = f()`）
- `list.try_*` 写法 → **E043**（以回调的 `!` 作为策略）

### 文件

- `crates/almide-frontend/src/lower/auto_try.rs` — 名称源于历史。目前根据 `!` 标记插入 `Try`，不进行隐式插入。
- `crates/almide-codegen/src/pass_result_propagation.rs` — 签名提升与调用位置的 `Try` 处理
- 详细规范：[specs/effect-fn-call-semantics.md](../../../../specs/effect-fn-call-semantics.md)、[ADR-0008](../../../../adr/0008-explicit-propagation-only.md)

---

## 3. 嵌入 Runtime

### 会发生什么

生成代码所需的 Almide 运行时会被嵌入。根据所用功能，可能还需要外部 Rust crate 或宿主功能。

### Rust target

`runtime/rs/src/*.rs` 的内容被嵌入生成的 `.rs` 文件（由 `crates/almide-codegen/build.rs` 生成 `generated/rust_runtime.rs`）。

包含：
- `almide_eq!` / `almide_ne!` 宏（深度相等比较）
- `AlmideConcat` trait（String + List 拼接）
- 通过 `@intrinsic` 声明的 stdlib 运行时函数

### WASM target

stdlib 中自举的 Almide 实现与用户代码一起链接，由 `almide-wasm` 的 structural emitter 直接生成 Wasm 字节。旧 MIR→WAT renderer 与手写 WAT preamble 路径均已废止。新的自举函数体通过 `@implements` 声明，并反映到生成的 registry 中。I/O 等宿主操作需要所选执行路径的 import 实现。详见 [WASM-OUTPUT.md](../../../../wasm/WASM-OUTPUT.md)。

---

## 4. Perceus RC 插入（native MIR / structural Wasm）

### 会发生什么

v1 MIR（`crates/almide-mir`）采用 Perceus 方式，自动插入引用计数操作（dup / drop）及用于原地修改的唯一性检查（MakeUnique）。该 MIR 用于 native renderer 和 MIR witness producer 路径；当前 Wasm 由独立的 structural emitter 处理 RC/layout。

### 对用户的影响

- 必须保持语言语义（native / wasm / interp 差分关卡对所覆盖的 corpus 进行检查）。
- 仅影响性能特征。MakeUnique elision 等优化会随各次发布得到增强。
- 证明：`crates/almide-perceus-belt/`（Lean 4）验证 RC 规则。

---

## 5. fan 的并行化（Rust target）

### 会发生什么

在 native 中，符合并行条件的 `fan { a(), b() }` 通过 `std::thread::scope` + `spawn` 执行。包含 `@ordered` 操作的 block 等会按 arm 顺序执行，因此并非每个 `fan` 都一定为每个 arm 创建一个线程。以下是并行路径的概略。

```rust
std::thread::scope(|__s| -> Result<_, String> {
    let __fan_h0 = __s.spawn(move || { a() });
    let __fan_h1 = __s.spawn(move || { b() });
    Ok((__fan_h0.join().unwrap()?, __fan_h1.join().unwrap()?))
})?
```

### 对用户的影响

- 外部变量通过 `move` 捕获（可能被 clone）。
- 捕获 `var` 会产生编译错误（E008）。
- 执行基础设施及是否可以并行由 compiler/host 决定。普通 `fan` 会 join 全部 sibling，并返回按 arm 顺序遇到的第一个错误。

### 文件

- `crates/almide-codegen/src/pass_fan_lowering.rs`、`pass_rust_lowering_fan.rs` — fan lowering 与 native parallel routing

---

## 不隐藏的操作

| 操作 | 语言中的表达 | 不隐藏的原因 |
|------|-------------|-------------|
| I/O | `effect fn` | 在类型签名中明确体现 |
| 错误传播 | `Result[T, E]` | 在类型中可见 |
| 可变性 | `var` vs `let` | 用关键字明确表示 |
| 并行化 | `fan { }` | 用语法明确表示 |
