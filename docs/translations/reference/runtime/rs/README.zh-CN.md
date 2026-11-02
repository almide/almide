# runtime/rs — native 运行时源码模板

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)



> 原文：[runtime/rs/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/runtime/rs/README.md)。这是供读者参考的非规范性快照译文，不会自动同步。未对测量值及主张进行独立复核，以原文为准。
>
> 原文注意：错误数和模块数是所述日期的记录。末尾关于自包含性的例外说明也属于原文时点，并非对当前依赖关系的独立复核。

`runtime/rs/src/*.rs` 保存 `stdlib/*.almd` 中 `@intrinsic("almide_rt_*")` 声明背后的 native (`almide_rt_*`) 实现。它**不是可供构建的 crate，也不是 workspace 成员**。本文记录原因，以及实际测试其行为的位置。

## 如何使用这个目录

没有任何东西将 `runtime/rs` 作为 cargo 依赖链接。`crates/almide-codegen/build.rs` 将每个文件作为**源文本**读取（解析 `include!` 指令），并作为 `RUST_RUNTIME_MODULES` 嵌入 `crates/almide-codegen/src/generated/rust_runtime.rs`。在输出时，编译器将程序所需的模块连接为**一个扁平模块**，在前面添加合成的 prelude，再交给 rustc：

- `emit_runtime_crate()` → 共享的 `almide_rt` rlib（`--crate-name almide_rt`，所有仅使用 std 的模块；`NON_STD_RUNTIME_MODULES` — `http`、`zlib`、`sse` — 被排除，因为裸 rustc 没有用于 rustls/flate2 的 `--extern`）。
- `emit_source()` → 生成 Rust 程序的运行时前置部分。

两条路径均调用 `strip_test_blocks`，在 rustc 看到代码前删除所有 `#[cfg(test)]` 块。

以前，`strip_test_blocks` 只按行计数花括号，不识别字符串字面量，因此如果测试体的字面量中存在不平衡的花括号，它就会越过边界删除到文件末尾。`regex.rs` 有两处（`r"a\{x"`、`r"a{2"`，深度 +4），删除器从测试模块一直运行到文件最后一行，顺带删除了结尾的 `// ---- End Regex Runtime ----` 注释。之所以没有造成损害，只是因为该模块是文件最后一项；这种块后若有真正的代码，就会从每个输出 crate 中被静默删除。删除器有两个逐字节相同的副本：此处的一个，以及构建脚本中处理 `crates/almide-kernel/src/*.rs` 的一个；这两个事实都未被检查。

两处都已修复，也都接受检查（#2511）。现在只有一个删除器：`crates/almide-codegen/src/strip_test_blocks.rs`，通过 `#[path]` 与构建脚本共享。它跟踪字面量状态，而不是计数字符（普通字符串、任意 hash 深度的 raw 字符串、与生命周期区分的字符字面量、行注释和嵌套块注释，均跨行延续）。找不到块结尾时，会报告含文件和起始行的 ERROR，而不是静默删除到 EOF——“无法剥离”是安全失败，“把它后面全部剥离”不是。

删除七个测试模块（#2507）使输出文本恰好改变八行，全部为空行或注释；对修改前后源码运行 `strip_test_blocks` 验证了这一点。六个文件各少一空行，`regex.rs` 恢复了之前被越界删除的注释，没有代码行移动。

由于编辑 `runtime/rs/src` 会改变嵌入文本，CI 会重新生成 `rust_runtime.rs` / `runtime_fn_modes.rs`，出现任何差异即失败（ci.yml 的 "Generated runtime registry matches committed sources"）。请重新构建 `almide-codegen`，并将生成文件与修改一起提交。

## 为什么不是 workspace 成员（#2507，测于 2026-09-22）

在根 `Cargo.toml` 的 `members` 中添加 `"runtime/rs"`，然后运行 `cargo check -p almide_rt`，会在 `src/lib.rs` 声明的 25 个模块中的 15 个产生 **220 个错误**（124 × E0425、83 × E0433、13 × E0405）。它们分为两类，都不是依赖或目标配置问题；rustls 和 webpki-roots 能正常解析。

1. **prelude 不以源码形式存在。** `AlmideRepr`、`AlmideMap`、`AlmideKey`、`AlmideKeyLookup`、`AlmideMapKey`、`ALMIDE_MAP_INDEX_THRESHOLD`、`almide_stdout_flush`、`almide_stdout_write_bytes`、`almide_rt_intern_key` 和 `almide_http_request_stream_impl` 由 `rust_runtime_prelude()` 输出：它们是 `crates/almide-codegen/src/lib.rs` 内用字符串字面量编写的 Rust，有两个可见性版本。`runtime/rs` 下没有文件定义它们。

   ```
   error[E0425]: cannot find function `almide_stdout_flush` in this scope
     --> runtime/rs/src/process.rs:44:5
   error[E0405]: cannot find trait `AlmideRepr` in this scope
     --> runtime/rs/src/value.rs:422:6
   ```

2. **`src/lib.rs` 的模块树并非实际结构。** 各文件不带路径地相互调用，只有扁平连接后才能解析：`json.rs` → `almide_rt_value_stringify`，`math.rs`/`matrix.rs` → `almide_rt_libm_*`，`fan.rs` → `almide_rt_list_par_*`，`sse.rs` → `almide_rt_json_parse`。在 `pub mod json; pub mod value; …` 下，每一处都是 E0425。`lib.rs` 还完全省略了 `hash.rs`、`net.rs` 和 `zlib.rs`：嵌入器会发布这三个文件，但模块树从未提到它们。

   ```
   error[E0425]: cannot find function `almide_rt_value_stringify` in this scope
     --> runtime/rs/src/json.rs:6:62
   error[E0425]: cannot find type `AlmideValue` in this scope
     --> runtime/rs/src/json.rs:6:37
   ```

因此，要将其设为成员，就必须重构运行时源码（嵌入器依赖扁平连接），或在另一处重新推导 prelude。加入成员还会为这个不发布任何内容的 crate，在 workspace lock 中添加 `almide-kernel`、`almide_rt` 及第二个 `webpki-roots` 主版本（在 `almide-rt-core` 固定的 1.0.9 之外再加 0.26.11）。

保留 `Cargo.toml` 是为了记录运行时源码所需内容（crates.io 依赖；#4007 随着没有任何构建设置的 `have_blas` BLAS 路径一起删除了 `build.rs`）。它不是构建单元，没有 CI job 或脚本在这里调用 cargo。

## 行为 oracle 是 `spec/`

**`runtime/rs` 的行为 oracle 是可执行的 `spec/` 套件。** 它像程序一样，通过 stdlib 在 native 和结构化 Wasm 路径上执行这些代码；CI 运行的就是它。写在这里的 Rust `#[cfg(test)]` 块不会在任何地方运行：没有 cargo 构建会编译这个目录，且 `strip_test_blocks` 会从每个输出 crate 中移除该块。

因此，**不要在 `runtime/rs` 下添加 `#[cfg(test)]`。** 请在 `spec/stdlib/` 中编写测试（若修改涉及跨目标承诺，则放在 `spec/wasm_cross/`）。`scripts/check-runtime-test-placement.sh` 在 CI 中执行这一规则。

### 唯一的狭窄例外：由根测试包含的自包含模块

若待测单元是 **private**，而它唯一的公开入口又是非确定性的，`spec/` 就无法触达它。`random.rs` 的 53-bit 构造（`almide_float_from_u64`）就是此例：`random.float()` 抽取一个无人选择的 u64，所以 `spec/stdlib/random_test.almd` 能断言采样下界与构造的最大值，却不能指定那些使旧商舍入为 1.0 的单个 u64 输入（#2495）。

因此，`tests/runtime_random_float_construction_test.rs` 用 `include!` 将 `runtime/rs/src/random.rs` 包含到自己的模块中，并在 `cargo test --workspace` 下测试**发布的源文本**。访问 private 项的访问器位于测试文件，而非运行时源码中，因此发布内容不变。

这只因 `random.rs` 仅依赖 `std::cell::Cell` 才成立。此处其他所有模块只能在嵌入器的扁平连接与合成 prelude 下解析（上述 220 个错误），同样的 include 无法编译。请把它视为一个经过测量的例外，而不是通用模式；若第二个模块也满足条件，请在测试文件中说明它为什么是自包含的。
