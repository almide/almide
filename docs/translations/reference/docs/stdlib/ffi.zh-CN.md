# ffi

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ffi.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ffi.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](ffi.zh-CN.md)



> 原文：[docs/stdlib/ffi.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/ffi.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。面向读者的非规范性快照译文，不自动同步，未进行独立测量或实现验证。API 签名和代码内注释保留原文，在代码外附说明译文。这是生成索引的固定副本，本译文不是生成目标。 边界检查以信任绑定声明的长度和外部代码为前提，不保证所有外部代码安全。原文开头称 Region 仅有“三处”创建来源，但后文还描述借给 export 的区域；保留原文结构，不擅自统一。

外部函数边界上的内存。`import ffi`。

程序可通过其读取的指针携带来源信息（[ADR-0031](../../../../adr/0031-raw-pointers-carry-provenance.md)）：

- **`Handle`** 是不透明的外部令牌（`FILE*`、`sqlite3*`），由 `@extern` 函数返回，只能再传给外部函数；不通过它读写，也没有 `Int` 形式。
- **`Region`** 是带基址、长度、权限和所有者的视图，字段不透明。两个目标上的 `ffi.read` 和 `ffi.write` 都在接触内存前检查 `0 <= off` 和 `off + n <= ffi.len(r)`；区域外访问返回 `err`，绝不读写其他内存。通过只读区域写入也返回 `err`。

Region 恰有三种创建来源，每一种都知道长度：

| 创建来源 | 长度 | 是否可写 |
|---|---|---|
| `ffi.region(b)` | `bytes.len(b)`；区域持有 `b` | 否 |
| `ffi.alloc(n)!` | `n`，新分配且置零，不与任何 Almide 值形成别名 | 是 |
| 带 `@region(len = …, release = "…")` 的 `@extern(c)` 函数 | 声明的长度 | 否 |

```almide
import ffi

@extern(c, "z", "zbuf_new")
@region(len = n, release = "zbuf_free")
fn zbuf_new(n: Int) -> Region

@extern(c, "z", "zfill")
fn zfill(buf: Region) -> Unit
```

`@extern(c)` 函数的 `Region` 参数变成两个 C 参数：指针和 `size_t` 长度（`void zfill(uint8_t *buf, size_t len)`）；`@extern(rust)` 函数接收同样一对 `(base: *mut u8, len: usize)`；`@extern(wasm)` 函数接收基址和长度两个数字。被调用方总是收到运行时持有的长度，宿主不使用运行时的 Region 类型名。因此同时带有 `@extern(wasm)` 和 `@extern(rust)` 的绑定在两种宿主上具有相同参数形状（#4353）：

```almide
import ffi

@extern(wasm, "sys", "log")
@extern(rust, "crate::sys", "log")
fn log(buf: Region) -> Unit = _
```

```rust
// native/sys.rs
pub fn log(base: *mut u8, len: usize) {
    let bytes = unsafe { std::slice::from_raw_parts(base, len) };
    println!("{}", String::from_utf8_lossy(bytes));
}
```

指针只在此次调用期间有效，只有 Region 可写（`ffi.alloc`）时宿主才通过它写入。外部区域的释放函数在该区域最后一份副本消失时运行一次；Almide 代码不进行释放。

`import ffi` 是显式启用方式：在 stdlib 之外，未写 `import ffi` 的文件调用任何 `ffi` 函数都会产生 E100（#4043）。`almide check
--profile critical` 即使有导入，也拒绝所有 `ffi.*` 调用（E100，#4008）。`Built` 行列出每个 `@extern` 函数和每个 `@region` 长度子句：绑定声明的长度以及外部代码都是受信任的。

**两个目标。** 基址在原生目标为 `*mut u8`，在 wasm 为线性内存偏移；它不被观察。移动的字节与 `err` 文本在两者上相同（`spec/wasm_cross/bytes_rawptr.almd`，C-062）。

```almide check
import ffi

effect fn main() -> Unit = {
  let r = ffi.alloc(4)!
  ffi.write(r, 0, bytes.from_string("hi"))!
  let back = ffi.read(r, 0, 2)!
  println(bytes.to_string(back) ?? "")
  let past = match ffi.read(r, 2, 4) {
    ok(_) => "read",
    err(e) => e,
  }
  println(past)
}
```

**切片。** `ffi.slice(r, off, n)` 是 `r` 内从 `off` 开始的 `n` 字节视图。除非 `0 <= off` 且 `off + n <= ffi.len(r)`，否则返回 `err`，因此不会越过父区域。即使父区域可写，切片仍只读。它读取父区域的内存，所以之后对父区域的写入会在切片中可见；它持有父块，使内存寿命长于视图。向 `@extern` 传递切片会无复制地传递子范围。

```almide check
import ffi

effect fn main() -> Unit = {
  let r = ffi.alloc(4)!
  let tail = ffi.slice(r, 2, 2)!
  ffi.write(r, 2, bytes.from_list([7, 8]))!
  println(bytes.to_list(ffi.read(tail, 0, 2)!) |> list.map((x) => int.to_string(x)) |> list.join(","))
  let far = match ffi.slice(r, 3, 2) {
    ok(_) => "sliced",
    err(e) => e,
  }
  println(far)
}
```

**无复制的宿主缓冲区**（ADR-0031 修订，2026-10-29）。长度始终来自 Region。

- *Push，wasm。* `@export(wasm)` 函数可以返回 Region，通过多值返回以 `(ptr: i32, len: i32)` 跨越边界。宿主持有 Region 块的一份共享所有权：无论程序如何处理自己的副本，字节位置都保持不变，直到宿主将 `ptr` 传给模块的 `__region_release` 导出。宿主可直接写入导出返回的 `ffi.alloc` Region，再调用读取它的下一个导出（顶层 `var` 持有程序自己的副本）。空 Region 以 `(0, 0)` 传递。切片以其自身地址无复制地传递；这份共享所有权使父区域保持存活，直到宿主释放。重复释放同一地址会触发陷阱。`effect fn` 导出的 `err` 会打印并触发陷阱。这一对值是视图的数据地址和 Region 自身长度，绝非 Region 记录的地址（#4354）。使用 `--host js` 时，胶水代码将导出包装为 `f(…) → [ptr, len]`，并添加 `regionRelease(ptr)`。
- *借给 wasm 导出。* `@export(wasm)` 函数可接收 Region 参数。宿主以 `(base, len)` 传递，与导入接收的相同，使用内存地址宽度。函数得到模块内存该范围上的只读 Region，仅借用于本次调用：不拥有任何内容，每次访问都按宿主给出的长度检查。若函数体将副本存入顶层 `var`、返回它或捕获进闭包，会产生 E119。宿主通常传回从 push 导出获得的范围，其共享所有权使该范围保持存活。
- *Pull。* 传给 `@extern(wasm)` 或 `@extern(c)` 函数的可写 `ffi.alloc` Region 仅在该次调用中借出，以内存地址宽度的 `(base, len)`（wasm32 为 `i32`）或 `(uint8_t*, size_t)` 传递，外部代码原地填充。
- *C 调用方。* `@export(c)` 函数可接收 Region 参数：调用方的 `(const uint8_t*, size_t)` 为只读；声明用 `@region(out = p)` 指明的参数则为可写的 `(uint8_t*, size_t)`。两者都只借用于一次调用。每次访问仍根据调用方传入的长度检查。若函数体将副本存入顶层 `var`、返回它或捕获进闭包，检查时产生 E119。以其他方式保留副本会在调用返回时停止进程，而非让它比调用方缓冲区存活更久。

```almide
import ffi

var inbox: Region = ffi.region(bytes.new())

@export(wasm, "inbox_open")
effect fn inbox_open(n: Int) -> Region = {
  inbox = ffi.alloc(n)!
  inbox
}

@export(wasm, "inbox_sum")
effect fn inbox_sum(part: Region) -> Int =
  bytes.to_list(ffi.read(part, 0, ffi.len(part))!) |> list.fold(0, (a, x) => a + x)

@export(c, "fill")
@region(out = out)
effect fn fill(out: Region, byte: Int) -> Int = {
  ffi.write(out, 0, bytes.from_list(list.repeat(byte, ffi.len(out))))!
  ffi.len(out)
}
```

已删除的写法（dialect epoch 15；`almide fix` 重命名前三项）：

| 已删除 | 应改写为 |
|---|---|
| `ffi.address(b)`, `bytes.as_ptr(b)` | `ffi.region(b)` |
| 传给 wasm `@extern` 的 `bytes.data_ptr(b)` | `@extern` 接收 `Region`：`ffi.region(b)` |
| `ffi.to_int(p)` | 无替代：地址没有 `Int` 形式 |
| `bytes.as_mut_ptr(b)` + `bytes.copy_to_ptr` | `ffi.alloc(n)!` + `ffi.write(r, off, b)!` |
| `bytes.from_raw_ptr(p, n)`, `ffi.read(p, n)` | `ffi.read(r, 0, n)!` |
| 类型 `RawPtr` | `Handle`，或带 `@region(…)` 的 `Region` |



## 签名索引（8 个函数）

```
// A read-only view of b; b lives as long as the view.
// @since unreleased
ffi.region(b: Bytes) -> Region

// A fresh writable region of n zero bytes; err if n is negative.
// @since unreleased
effect ffi.alloc(n: Int) -> Region

// The region's length in bytes.
// @since unreleased
ffi.len(r: Region) -> Int

// True if write may change the region (ffi.alloc and writable foreign regions).
// @since unreleased
ffi.is_writable(r: Region) -> Bool

// Copy of n bytes at off; err unless 0 <= off and off + n <= len(r).
// @since unreleased
effect ffi.read(r: Region, off: Int, n: Int) -> Bytes

// Writes b at off; err if the region is read-only or b does not fit.
// @since unreleased
effect ffi.write(r: Region, off: Int, b: Bytes) -> Unit

// A copy of the whole region as Bytes.
// @since unreleased
effect ffi.to_bytes(r: Region) -> Bytes

// A read-only view of n bytes at off inside r; err unless 0 <= off and off + n <= len(r).
// @since unreleased
ffi.slice(r: Region, off: Int, n: Int) -> Result[Region, String]
```



## 索引注释的说明译文

- `ffi.region(b: Bytes) -> Region` — b 的只读视图；至少在视图存在期间 b 保持存活。 (@since unreleased)
- `effect ffi.alloc(n: Int) -> Region` — 含 n 个零字节的新可写区域；n 为负则 err。 (@since unreleased)
- `ffi.len(r: Region) -> Int` — 区域的字节长度。 (@since unreleased)
- `ffi.is_writable(r: Region) -> Bool` — 若 write 可改变区域则 true（ffi.alloc 和可写外部区域）。 (@since unreleased)
- `effect ffi.read(r: Region, off: Int, n: Int) -> Bytes` — off 处 n 字节的副本；除非 0 <= off 且 off + n <= len(r)，否则 err。 (@since unreleased)
- `effect ffi.write(r: Region, off: Int, b: Bytes) -> Unit` — 在 off 写入 b；区域只读或放不下 b 则 err。 (@since unreleased)
- `effect ffi.to_bytes(r: Region) -> Bytes` — 整个区域的 Bytes 副本。 (@since unreleased)
- `ffi.slice(r: Region, off: Int, n: Int) -> Result[Region, String]` — r 内 off 处 n 字节的只读视图；除非 0 <= off 且 off + n <= len(r)，否则 err。 (@since unreleased)
