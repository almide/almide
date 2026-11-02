> 原始说明（非活动元数据）：标准库重设计 L2/L3——ffi（新增，#3853）：五个原始地址函数从始终可见的 bytes 模块移至需显式导入的 ffi，统一为一个在两个目标上遇空均返回 null 的取地址操作。
<a id="l2l3-ffi新しいモジュール"></a>

# L2/L3：ffi（新模块）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-ffi.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-ffi.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-ffi.zh-CN.md)

> 非规范性、固定原文的读者快照；未修改原本仍为权威来源。不自动同步，未独立执行、测量或验证安全性。10月27日追加已取代10月21日RawPtr提案；下文旧签名、采纳标记、pure/effect分类、参数顺序及epoch16迁移计划均为历史记录，不是当前API指示。九语言比较和实测差异属于原文当时证据，未在此重新核实。显式导入本身不是内存安全或权限边界：限制权限设计被排除在该提案外。保留Region/Handle追加，但不声称整个产物内存安全或受信任extern正确。代码注释和占位符保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-ffi.md)

> 最后更新：2026-10-21
> 前提：[L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md)、[L1](../../../../../roadmap/stdlib-redesign/L1-categories.md)（R-L1-26：将原始指针移至 `ffi`，#3853）
> 来源：Rust https://doc.rust-lang.org/std/primitive.slice.html#method.as_ptr、https://doc.rust-lang.org/std/slice/fn.from_raw_parts.html；Go https://pkg.go.dev/unsafe；Python https://docs.python.org/3/library/ctypes.html；TS/JS https://nodejs.org/api/n-api.html（JS 侧为 `WebAssembly.Memory.buffer`）；Gleam —（无，外部函数用 `@external`）；Elixir —（NIF 位于 C 侧）；OCaml https://ocaml.org/manual/latest/intfc.html；Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlinx.cinterop/（Native）；Swift https://developer.apple.com/documentation/swift/unsafemutablerawpointer

> **2026-10-27 追加**：本文所述 `RawPtr` 公开面（`ffi.address`、`ffi.to_int`、传递地址及长度的 `ffi.read` / `ffi.write`，以及 `bytes.as_ptr` 等旧写法）已由 [ADR-0031](../../../../../adr/0031-raw-pointers-carry-provenance.md)（Accepted 2026-10-27，dialect epoch 15）取代。当前公开面：`RawPtr` 改名为不透明外部 token `Handle`（标准库函数既不接收也不返回它）。读写经过带长度的 `Region`——`ffi.region(b)`（只读）、`ffi.alloc(n)`（effect fn，可写，零填充）、`ffi.len`、`ffi.is_writable`、`ffi.read(r, off, n)`、`ffi.write(r, off, b)`、`ffi.to_bytes(r)`。越界返回 `err`。`ffi.address`、`ffi.to_int`、`bytes.as_ptr`、`as_mut_ptr`、`data_ptr`、`from_raw_ptr`、`copy_to_ptr` 已移除。`@extern(c)` 的 `Region` 参数以 (ptr, size_t len) 两个参数传递；返回 `Region` 必须有 `@region(len = …, release = "…")`（缺失则 E090）。以下保留为 2026-10-21 的记录。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide（当前 → 提案） | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 字节序列首地址 | `bytes.as_ptr`、`as_mut_ptr`（空时 null）、`data_ptr`（`Int`，空时也非 null）→ `ffi.address` | `as_ptr`、`as_mut_ptr` | `unsafe.SliceData` | `ctypes.addressof` | `Memory.buffer` 内 offset | — | — | `Bigarray` | `addressOf`、`refTo` | `withUnsafeBytes` |
| 整数形式的地址 | `bytes.data_ptr` → `ffi.to_int` | `as usize` | `uintptr` | `ctypes.cast(…, c_void_p).value` | — | — | — | — | `toLong` | `Int(bitPattern:)` |
| 从地址读取（复制） | `bytes.from_raw_ptr` → `ffi.read` | `slice::from_raw_parts` | `unsafe.Slice` | `ctypes.string_at` | `new Uint8Array(buf, off, len)` | — | — | — | `readBytes` | `UnsafeRawBufferPointer` |
| 向地址写入 | `bytes.copy_to_ptr` → `ffi.write` | `ptr::copy_nonoverlapping` | `copy(unsafe.Slice…)` | `ctypes.memmove` | `Uint8Array.set` | — | — | — | `memcpy` | `copyMemory` |

<a id="完全性の規則"></a>

### 完备性规则

- 角色闭合为 4 个：{取地址、将地址转为整数、从地址读取、向地址写入}。
- 取地址函数仅 1 个，空字节序列返回 null（native 和 wasm 答案相同）。不设只读与写入两种写法（native 的 `as_ptr` 也返回 `*mut u8`，区别只有名称。#3853）。
- 必须 `import ffi`。`bytes` 在所有文件中无需 import 即可见，因此不放置内存不安全函数。
- 取地址及转整数函数为纯函数（值会变化，但不读取外部世界）。通过 ADR-0027 D3 的“往返”判定检验。

<a id="l2-の判定"></a>

### L2 判定

- 重复：返回地址的写法有 3 个（`as_ptr`、`as_mut_ptr`、`data_ptr`），空时答案不同 → R-ffi-01。
- 位置不当：5 个函数位于自动可见的 `bytes` → R-ffi-01。

<a id="案"></a>

## 提案

<a id="r-ffi-01-ffi-を作りbytes-の生ポインタ-5-関数を移す---採用"></a>

### R-ffi-01 创建 `ffi`，迁移 `bytes` 的 5 个原始指针函数 — ○ 采纳

- 内容：
  ```almide
  fn address(b: Bytes) -> RawPtr                     // 先頭の番地。空なら null
  fn to_int(p: RawPtr) -> Int                        // 番地の整数（JS host に渡す用。null は 0）
  fn read(ptr: RawPtr, len: Int) -> Bytes            // len バイトのコピー。null か len <= 0 なら空
  fn write(b: Bytes, ptr: RawPtr, cap: Int) -> Int   // 最大 cap バイトを書き、書いた数。null なら 0
  ```
  改写：`bytes.as_ptr(b)`、`bytes.as_mut_ptr(b)` → `ffi.address(b)`；`bytes.data_ptr(b)` → `ffi.to_int(ffi.address(b))`；`bytes.from_raw_ptr(p, n)` → `ffi.read(p, n)`；`bytes.copy_to_ptr(b, p, c)` → `ffi.write(b, p, c)`。
- 语义差异：`data_ptr` 即使字节序列为空也返回 `handle + 12`。`ffi.to_int(ffi.address(b))` 是 0。因为只发生长度为 0 的读取，JS host 路径（#907、#3420 的生命周期规则 `spec/wasm_host_js/extern_data_ptr_lifetime.almd`）没有差异。变化仅发生在新名称一侧（R-L0-14 第 4 项）。
- 写法：`to_int` 遵循 R-L0-07（源类型模块中的 `to_<先>`）。`RawPtr` 归属 `ffi`（R-L0-02）。
- 限制可调用文件的权限（仿照 `prim` E085，拒绝无 `@extern` 文件调用 `ffi.*` 的提案）不包含在本案中。`[permissions]` 键的设计将与 `proc`（ADR-0025）一起确定，由 #3853 跟踪。本案即使没有权限也成立（只需 `import ffi` 即可从普通代码的补全候选中消失）。
- 支撑的 L0 规则：R-L0-01（取地址写法统一为一个）、R-L0-02、R-L0-07、R-L1-26。
- (a) 9 种语言均将原始地址放在 `unsafe`、`ctypes`、`cinterop` 等名称明确的区域。只有 Rust 将其放在普通字节序列类型旁（`as_ptr` 安全，使用方需要 `unsafe`）。
- (b) native：`*mut u8`。wasm：线性内存地址（#440 / C-062，`stdlib/bytes_rawptr.almd`）。两者使用同一函数体，统一空时答案。不需要 host 功能。
- (c) LLM 不再在普通代码补全候选中看到 `bytes.as_mut_ptr`。
- (d) 新增，并撤除 `bytes` 一侧（L2-bytes 同样判定）。`@deprecated(since=15, use="ffi.…")`、`removal_epoch = 16`。`almide fix` 一对一改写（也添加 `import ffi`）。使用数（spec/tests/docs，#3853）：`as_ptr` 10/1/7、`as_mut_ptr` 9/0/5、`data_ptr` 12/0/10、`from_raw_ptr` 6/0/4、`copy_to_ptr` 11/0/5。
- 检查提案：`bytes` 中不存在以 `RawPtr` 为参数或返回值的公开函数。用 cross fixture 比较 `ffi.address` 空时在 native 与 wasm 的答案。
- issue：#3853（已有）

<a id="r-ffi-02-ffiread-と-ffiwrite-を-effect-fn-にする---採用"></a>

### R-ffi-02 将 `ffi.read` 与 `ffi.write` 改为 effect fn — ○ 采纳

- 当前：`fn ffi.read(ptr: RawPtr, len: Int) -> Bytes` 和 `fn ffi.write(b: Bytes, ptr: RawPtr, cap: Int) -> Int` 是纯 fn。
- 提案：`effect fn ffi.read(…)`、`effect fn ffi.write(…)`。`address`、`to_int` 保持纯函数。
- 支撑的 L0 规则：R-L0-13（读取环境的函数为 effect）、ADR-0022（纯 fn 仅能输出与 abort）。`write` 改写 Almide 值之外的内存，`read` 读取外部代码写入的内存。
- (a) 9 种语言都将原始地址读写放在 `unsafe`、`ctypes` 等区别于普通函数的区域。
- (b) 实测两个 target 不一致。在纯 fn 中向 `let` 绑定的 `Bytes` 地址写入时，native 会改写调用方不可变值，而 wasm 不会（以当前 `bytes.as_mut_ptr` 和 `copy_to_ptr` 重现。#3966，将作为阻止发布的缺陷另行修复）。只要视为纯 fn，优化还可能进一步改变它。
- (d) 只改变新函数声明。使用迁移源 `bytes.from_raw_ptr`、`copy_to_ptr` 的纯 fn 在改写后会得到 E006，因此给出“将调用方改为 effect fn”的提示。
- 完备性规则：读写 Almide 值外部状态的函数为 effect。在 `ffi` 中，{address, to_int} 为纯函数，{read, write} 为 effect。
- issue：#3974

<a id="r-ffi-03-ffiwrite-の引数を書き込む先が先にする---採用"></a>

### R-ffi-03 将 `ffi.write` 参数改为“写入目标在前” — ○ 采纳

- 当前（提案阶段）：`fn ffi.write(b: Bytes, ptr: RawPtr, cap: Int) -> Int`。其他标准写操作均以目标在前（`fs.write_bytes(path, data)`、`net.tcp_write(stream, data)`、`unix.write(sock, data, fds)`、`bytes.put_<型>(mut b, pos, v)`）。
- 提案：`fn ffi.write(ptr: RawPtr, b: Bytes, cap: Int) -> Int`。也与 `ffi.read(ptr, len)` 对齐，首参数均为指针。
- 支撑的 L0 规则：R-L0-03（主体参数在前）。
- (d) 仅改变尚未发布的函数形式。`almide fix` 从 `bytes.copy_to_ptr(b, p, n)` 改写时交换参数顺序。
- issue：#3979

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（提案） | 其他语言主要写法 | 判定 | 提案 |
|---|---|---|---|---|
| `ffi.address` | `fn address(b: Bytes) -> RawPtr` | Rust `as_ptr` / Go `unsafe.SliceData` / Python `addressof` | 新增（来自 `bytes.as_ptr`、`as_mut_ptr`） | R-ffi-01 |
| `ffi.to_int` | `fn to_int(p: RawPtr) -> Int` | Go `uintptr` / Swift `Int(bitPattern:)` | 新增（`bytes.data_ptr` 的角色） | R-ffi-01 |
| `ffi.read` | `effect fn read(ptr: RawPtr, len: Int) -> Bytes` | Rust `from_raw_parts` / Python `string_at` / Go `unsafe.Slice` | 新增（来自 `bytes.from_raw_ptr`） | R-ffi-01 |
| `ffi.write` | `effect fn write(ptr: RawPtr, b: Bytes, cap: Int) -> Int` | Rust `copy_nonoverlapping` / Python `memmove` | 新增（来自 `bytes.copy_to_ptr`；目标在前） | R-ffi-01、R-ffi-03 |

> 保留的历史代码注释译文：address 返回首地址，空时为 null。to_int 将地址转为供 JS host 使用的整数，null 为 0。read 复制 len 个字节，null 或 len <= 0 时返回空。write 至多写入 cap 个字节并返回写入数量，null 时为 0。保留的日文占位符分别表示 to_<…> 中的“目标”和 bytes.put_<…> 中的“类型”。这些是被取代提案的解释，不是当前 API 保证。
