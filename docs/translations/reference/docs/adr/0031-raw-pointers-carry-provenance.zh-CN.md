<a id="adr-0031-a-pointer-the-program-can-read-through-carries-its-provenance--rawptr-splits-into-an-opaque-handle-and-a-bounds-checked-region-base-length-owner-and-a-foreign-pointer-becomes-a-region-only-where-its-extern-declaration-states-the-length"></a>

# ADR-0031：程序可读取的指针携带来源信息 — `RawPtr` 分为不透明 `Handle` 与边界检查的 `Region`（base、length、owner），外部指针仅在 `@extern` 声明长度时成为 `Region`

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0031-raw-pointers-carry-provenance.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0031-raw-pointers-carry-provenance.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0031-raw-pointers-carry-provenance.zh-CN.md)

> 固定原文的非规范性历史读者快照，不是新的安全认证或当前实现审计。原本未改且为权威来源，不自动同步、未独立执行测试。保留10月27日初始ABI限制与29日修订的历史分层。component-model复制lowering明确未在此落地。边界检查依赖声明/host提供长度，native外部代码与wasm host仍需信任；host只读仅属契约，不是capability强制。C借用有runtime逃逸兜底，wasm inbound借用没有。静态E119新增与旧调查/替代方案“checker无非逃逸规则”并存，作为原文历史保留、不擅自统一。zero-copy和消除UAF主张限于描述的所有权路径及信任前提，不是普遍无漏洞保证。D与所有者决定编号差异保留。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0031-raw-pointers-carry-provenance.md)

- **状态**：已接受（2026-10-27）。owner对全部八项[所有者决定](#owner-decisions-)回答○；已落地范围及其狭义解读见[实现](#implementation-2026-10-27)。2026-10-29修订：[host边界的Region](#amendment-2026-10-29-regions-at-the-host-boundary)。
- **日期**：2026-10-27
- **范围**：`RawPtr` 类型、`ffi` module、指针的 `@extern` 签名ABI（`crates/almide-types/src/types/extern_abi.rs`）、已deprecated的 `bytes` raw-pointer写法、安全表第5行。
- **相关**：
  - `docs/roadmap/active/security-by-construction.md` 第5行（FFI/raw pointer内存错误，状态partly）。
  - #4043（`import ffi` 为opt-in，E100）、#4008（`@memory_unsafe`、critical profile、`Built` 行）、#3054（E090、`@extern(c)` ABI表）、#3420（地址借用其block）、#3966（经 `bytes.as_mut_ptr` 写入改变 `let`）。
  - `tests/raw_ptr_surface_test.rs`：无需裁定、随本ADR落地的部分。

<a id="context"></a>

## 背景

第5行自身所述残余缺口：写了 `import ffi` 的file内，没有检查地址和长度是否描述程序拥有的内存，也无法检查，因为两target的 `RawPtr` 都只是裸地址。

当时的 `RawPtr`：

| | native | wasm |
|---|---|---|
| 表示 | `*mut u8` | 线性内存的 `i32` offset（MIR为 `i64`） |
| 附带长度 | 无 | 无 |
| 附带owner | 无（#3420通过borrow分析而非值保持源block存活） | 相同 |

用户程序获取途径（`tests/raw_ptr_surface_test.rs` 从声明签名导出并固定）：

1. **自己拥有的 `Bytes` 地址**：`ffi.address(b)` 及deprecated的 `bytes.as_ptr` / `as_mut_ptr`。生成时知道owner和长度，随后丢弃。
2. **外部函数返回**：`@extern(c, …) fn f(…) -> RawPtr`。唯一分配记录在外部库。

没有其他生成途径。用户file无法命名 `Int → RawPtr` 路径（`prim.int_to_ptr` 在E085之后），也没有 `RawPtr` 算术、literal或Codec field。地址还通过 `ffi.to_int` 和 `bytes.data_ptr` 作为 `Int` 传递，wasm `@extern` 以 `(ptr: Int, len: Int)` 接受（`spec/wasm_host_js/extern_data_ptr_lifetime.almd`）。

长度恰在两处丢失，且编译器在两处都知道一些事实：(1)runtime知道block长度；(2)声明是binding作者说明外部函数事实的唯一位置。本ADR在两个生成点附加来源信息，之后每次访问检查它。

<a id="survey-what-other-languages-put-in-a-pointer-measured-2026-10"></a>

## 调查：其他语言在指针中放什么（2026-10测量）

| 语言 | 可读取指针 | 长度来源 | Owner / lifetime | 仍需信任 |
|---|---|---|---|---|
| **Rust**（strict provenance，1.84起stable，2025-01） | `&[T]` 为(ptr,len)；`*const T` 在抽象机器携带provenance，`addr()` / `with_addr()` / `without_provenance()` 分离整数与指针；`with_exposed_provenance` 为显式逃生口 | `slice::from_raw_parts(ptr, len)` 是 `unsafe` 调用，转换时一次断言长度 | `&[T]` 有borrow checker，`*const T` 无 | 每个 `unsafe` block；Miri仅在测试检查provenance |
| **CHERI**（Arm Morello、CHERIoT） | capability：地址+base+bounds+permission+hardware validity tag | 分配时设定，只能缩小（单调） | 整数不能变capability，上覆写入清除tag | 设初始bounds的allocator |
| **Zig** | `[]T` slice(ptr,len)，Debug/ReleaseSafe边界检查；`[*:0]T` sentinel终止；`[*]T` 未知长many-pointer；`[*c]T` 仅来自翻译C | `ptr[0..n]`：切片时断言长度 | 无 | 切片点；`@ptrFromInt` |
| **Swift** 6.2 | `UnsafeBufferPointer`（base,count，仅debug检查）；`Span` / `MutableSpan` / `RawSpan` 不可逃逸（`~Escapable`），总边界检查且lifetime系于owner；SE-0458 `-strict-memory-safety` 标出所有unsafe用法 | Clang `-fbounds-safety` 的 `__counted_by(n)` / `__sized_by(n)` C声明import为接收 `Span` 的safe overload | view依赖的owner | `__counted_by` 注解 |
| **Go**（cgo） | `unsafe.Pointer`；`unsafe.Slice(ptr, len)`；`C.GoBytes(p, n)` copy | 在 `unsafe.Slice` / `C.GoBytes` 断言 | 未pin（`runtime.Pinner`，1.21）时C不能在return后保留Go pointer；`GODEBUG=cgocheck=1` 动态检查 | 转换长度；C行为 |
| **Wasm component model**（WASI 0.2 / 0.3） | 无指针跨component边界 | canonical ABI复制 `list<u8>`（经 `cabi_realloc` lift/lower），接收方得runtime测出的长度 | `resource` 以 `own<T>` / `borrow<T>` handle越界，每次检查table index；`borrow` 随call结束，drop `own` 运行destructor | component自身代码 |

共识：

- **两类而非一类。** 各设计分离仅传回的token（CM `resource`、Rust `*mut Opaque`、Swift `OpaquePointer`）与可读view（slice、`Span`、capability）。Almide `RawPtr`兼任两者。
- **长度在指针进入时声明一次，之后每次访问检查**：`from_raw_parts`、`ptr[0..n]`、`__counted_by`、`unsafe.Slice`。声明点就是小且可计数的可信基。
- **整数不是指针。** CHERI用hardware强制，Rust strict provenance用API，CM则根本无指针。
- **Owner是view的一部分**：Swift `Span`依赖owner，CM `own`运行destructor，Go pin。

<a id="decision"></a>

## 决定

<a id="d1-rawptr-splits-into-two-types"></a>

### D1. `RawPtr` 分成两个类型

- **`Handle`**：不透明外部token（`FILE*`、`sqlite3*`、context pointer），只能传给 `@extern` fn。没有 `ffi` 函数通过它读写，也没有 `Int` 形式。当时大多数 `@extern(c) … -> RawPtr` 返回属于此类。相当于CM `resource`。
- **`Region`**：可读写view，携带 **(base,length,permission,owner)**。`ffi.read(r, off, n)` / `ffi.write(r, off, b)` 在两target触碰内存前检查 `0 ≤ off` 和 `off + n ≤ len`，并拒绝写read-only region。

表示：native `struct { ptr: *mut u8, len: usize, perm: u8, owner: Owner }`；wasm三个 `i32` 加owner slot。`Region` 像 `Bytes` 一样是Perceus heap值，因此owner引用计数。

<a id="d2-a-region-is-minted-at-exactly-three-places-each-knowing-the-length"></a>

### D2. `Region` 恰在三个知道长度的地方生成

1. `ffi.region(b: Bytes) -> Region`：**read-only**，长度 `bytes.len(b)`，owner=`b`（一个RC share令block活过所有view，用所有权替代#3420 borrow分析维护lifetime）。
2. `ffi.alloc(n: Int) -> Region`：**writable**、fresh、unique；`ffi.to_bytes(r) -> Bytes` 复制输出。writable region绝不alias Almide值，#3966经指针写入改变 `let` 的形态无法表达。
3. 声明长度的外部返回：

   ```almide
   @extern(c, "z", "zbuf_new")
   @region(len = n, release = "zbuf_free")
   fn zbuf_new(n: Int) -> Region
   ```

   `len` 指parameter或constant；`release` 指释放它的C函数，在最后一个 `Region` share drop时运行。同一声明若无 `@region` 必须返回 `Handle`（否则E090）。这是把Clang `__counted_by` / Swift safe import移至binding。

<a id="d3-a-region-argument-crosses-the-c-abi-as-two-arguments"></a>

### D3. `Region` 参数以两个参数跨C ABI

`fn zfill(buf: Region)` 绑定C prototype `void zfill(uint8_t *buf, size_t len)`。callee得到runtime持有长度，而非caller手写长度，因此传region到C不能夸大长度。wasm中 `Region` 以 `(i32 ptr, i32 len)` 跨 `@extern(wasm)`；component(p2/p3)由canonical ABI降为 `list<u8>`（复制），`Handle` 为 `own<resource>`。

<a id="d4-addresses-as-integers-leave"></a>

### D4. 移除整数地址

删除 `ffi.to_int`、`bytes.data_ptr` 和deprecated `bytes.as_ptr` / `as_mut_ptr` / `from_raw_ptr` / `copy_to_ptr`。原接收 `(ptr: Int, len: Int)` 的wasm host改收 `Region`（D3）。此后用户不可达任何方向的 `Int` 与pointer转换。

<a id="d5-landing"></a>

### D5. 落地

一个dialect epoch，无弃用窗口（project对不匹配形态的裁定）。`almide fix` 将 `ffi.address(b)` 改为 `ffi.region(b)`，无长度的 `@extern … -> RawPtr` 改为 `-> Handle`。`Built` 行继续列出每个 `@extern` fn，并增加所有 `@region` 长度条款。

<a id="what-stays-trusted-after-d1d5"></a>

### D1–D5后仍需信任

1. 每个 `@region(len = …)` 条款：binding作者表明外部函数返回这么多byte。每声明一项，在 `Built` 计数，`--profile critical` 拒绝。
2. 外部代码本身：native C能写进程任意位置。无process/sandbox隔离，语言无法封堵（CHERI hardware或wasm，其host即外部代码）。
3. `ffi.read` / `ffi.write` runtime本体，由 `scripts/check-runtime-unsafe.sh` 计数。

第5行随后可按自身定义关闭：机制是边界检查；gate为每生成类（owned、allocated、foreign）的差分fixture，验证 `len` 与 `-1` 访问在两target均为 `Err`，并将 `tests/raw_ptr_surface_test.rs` 从 `RawPtr` 扩至 `Region`。

<a id="implemented-now-needs-no-ruling"></a>

## 当时已实现（无需裁定）

`tests/raw_ptr_surface_test.rs` 固定D2前提：provenance只能在生成点附加，因此生成点须保持既有两个。来自声明签名，而非名称列表：

- 所有用户可达、签名含 `RawPtr` 的stdlib fn为 `@memory_unsafe`；
- 所有用户可达、返回 `RawPtr` 的stdlib fn接收 `Bytes`；
- 从 `Int` 生成的module（`prim`）被E085所读的同一声明（`stdlib::is_user_nameable_module`）排除，并检查排除非空虚；
- 写了 `import ffi` 的file中，`RawPtr` 算术、类型为 `RawPtr` 的整数字面量、`RawPtr` Codec field和 `prim.int_to_ptr` 均拒绝，附clean control。

<a id="implementation-2026-10-27"></a>

## 实现（2026-10-27）

八项决定均在dialect epoch15落地，无弃用窗口（`proofs/dialect-epochs.toml` 每个删除写法有 `breaks` entry；`almide fix` 改名保留实参的调用：`ffi.address(b)` / `bytes.as_ptr(b)` / `bytes.data_ptr(b)` → `ffi.region(b)`，类型 `RawPtr` → `Handle`，其余给hint）。

- **D1。** `Handle` 是原 `RawPtr` builtin head。`Region` 是stdlib/ffi.almd的 `mod type Region = { owner, base, len, writable, keep }`，module外不透明（E033），程序不能读 `base` 或伪造 `len`。三个raw操作 `__address`、`__load`、`__store` 是该file的 `local`（其他位置E420），仅在 `read` / `write` 完成bounds检查后运行。native实现在runtime/rs/src/ffi.rs，wasm在stdlib/ffi_bridge.almd。
- **D2。** `ffi.region(b)`（read-only、owner `b`）、`ffi.alloc(n)`（`effect fn`，两次call不会合为共享buffer），以及声明 `@region(len = …, release = "…")` 的 `@extern(c)` fn。check/extern_abi_check.rs验证条款，缺失或 `len` 未指整数parameter时E090；在 `Built` 计数。
- **D3。** `@extern(c)` Region parameter在glue为 `(*mut u8, usize)`（walker/extern_c.rs）。`@extern(wasm)` Region parameter在structural emitter读程序前拆为 `(base, len)`（crates/almide-wasm/src/extern_region.rs），JS glue看到同样两个数。
- **D4。** foreign Region也read-only：按字面理解“writable只来自 `ffi.alloc(n)`”。foreign library写自身buffer，程序传Region（D3）让它填充。
- **D5。** `ffi.read` / `ffi.write` 返回包含offset/count/length的 `err`；list索引仍abort（C-067）。
- **D6。** glue将release guard存入Region的 `keep` closure；每copy共享一个 `Rc`，最后copy drop时release函数运行一次。
- **D7。** `ffi.to_int`、`ffi.address`、`bytes.data_ptr` 和其他四个 `bytes` raw-pointer写法消失。`prim.int_to_ptr` / `ptr_to_int` 留作stdlib自身底层（外部E085）。

狭义解读；若owner需要其他行为，明确列为决定项：

- D3 wasm `(i32 ptr, i32 len)`：两值以emitter整数scalar越界（import为 `i64`，JS hook为number），与其他 `Int` import参数相同。缩为 `i32` 需wasm import ABI新scalar class。
- D3 component-model lowering（`list<u8>` / `own<resource>`）不属本次更改：当时p2/p3 component不接收 `@extern(wasm)` hook。

Gate：tests/raw_ptr_surface_test.rs（签名导出：用户可达stdlib fn不含Handle；Region fn全为 `@memory_unsafe`；拒绝伪造；native/wasm bounds及permission检查回 `err`），spec/wasm_cross/bytes_rawptr.almd（C-062、C-376），tests/extern_rust_abi_test.rs（C ABI pair及单次release），scripts/check-rawptr-surface.py。

<a id="amendment-2026-10-29-regions-at-the-host-boundary"></a>

## 修订（2026-10-29）：Host边界的Region

epoch15迁移使约30个下游repo变红，因为Region无法与host交换。约20个fizz repo的JS driver及nendo-ceangal-web pool向export要buffer地址、写入、再调用下一export；ptcg-abc及bindgen scaffolding将C buffer传入 `@export(c)` fn。owner于2026-10-29逐项回答缺口。约束性要求为回答5：inbound host data必须zero-copy且bounded，不能为理想付性能代价。长度总来自Region，边界不发生 `Bytes` copy。

<a id="a1-an-exportwasm-fn-may-return-a-region"></a>

### A1. `@export(wasm)` Fn可返回Region

通过wasm multi-value return以 `(ptr: i32, len: i32)` 越界，即Region参数跨import的 `(base, length)` pair（D3）的wasm32形式。

**所有权。** 返回Region活到host把 `ptr` 交回module的 `__region_release` export。具体：

1. emitter导出的adapter为host保留Region block的一份share。程序自己的Region copy按普通规则drop，但host share存在时都不能free block。因此 `ptr` 不悬空，host写入byte跨后续call仍原地保留。
2. `__region_release(ptr)` 归还share。不调用的host保留block直到instance结束，是leak而非悬空指针。
3. 空Region以 `(0, 0)` 越界，不持有内容；release 0为no-op。
4. sub-region（`ffi.slice`）也可越界（owner○，2026-10-29），使用parent block内自己的地址，零copy。adapter把每个交出的 `(ptr, block)` 记入module小table（其他global后追加三个：table block、count、capacity）。`__region_release(ptr)`查地址、release block share、删entry。因此parent活到host release slice。table不持有的地址，无论已release或从未交出，都trap。
5. `effect fn` export的 `err` 无法放入 `(ptr, len)`，adapter写stderr并trap。

机制：IR rewrite（crates/almide-wasm/src/extern_region.rs，`wrap_region_exports`）将export移到compiler生成wrapper，返回 `ffi.__export_view(f(…))`：一份share的Region block及其中view offset/length。Region record随后按通常所有权规则release。wrapper stamp `region_export` 告诉emitter以adapter替代导出。adapter（crates/almide-wasm/src/region_export.rs）调用wrapper，free view record外壳，把地址登记table，返回view data地址/长度，为host保留share。

**宽度**（owner○，2026-10-29）。双向使用target memory地址宽：wasm32为 `(i32, i32)`，memory64为 `i64`。emitter以 `Int` scalar算地址。module声明32bit memory时，`imports::declare`将Region import的base/length参数设 `i32`（stub用 `address_params` 标位置），以包装它们的shim替代stub。memory64不建shim，仍为 `i64`。

<a id="a1b-an-exportwasm-fn-may-take-region-parameters-4354"></a>

### A1b. `@export(wasm)` Fn可接收Region参数（#4354）

host按 `(base, len)`传每项，即Region import（D3）及#4353 Rust host接收的pair，使用memory地址宽。IR rewrite（`extern_region::lend_region_params`）为fn生成 `__region_lend_<name>` wrapper，以两个值替代export的Region位置（stamp `address_params`；emitter adapter在32bit memory接 `i32`并扩宽），用 `ffi.__lend(base, len)` 调fn：该module内存范围的read-only Region，不拥有内容，仅call中借用。长度是host所言，如A2 C caller，每次访问都对其检查。E119按A2拒绝本体可在call后保留的copy（top-level `var`、return、closure）；wasm没有call后runtime backstop。host通常借回push export给出的范围，由share保持存活。范围是host表明，故read-only；writable inbound buffer用push形式。

<a id="a2-an-exportc-fn-may-take-region-parameters"></a>

### A2. `@export(c)` Fn可接收Region参数

Region参数以C caller的 `(const uint8_t *ptr, size_t len)` 越界，read-only。声明用 `@region(out = p)` 指名者以 `(uint8_t *ptr, size_t len)` 越界，writable，为caller提供buffer形式。两者都**仅借用一个call**，因memory属于caller，不拥有内容。每次访问按caller长度检查（D5，越界 `err`）。call后若仍持有任何Region copy，例如top-level `var`，wrapper停止process，防止Region比caller恢复控制后可立即free的buffer更长寿。无法从所有copy撤销值，故采用Go `cgocheck` 思路且总是应用。这是backstop；本体可写出的escape在check时拒绝（owner○，2026-10-29，**E119**）。借用Region或持有它的local `let` / `var`、slice、container，若存到top-level `var`、返回或closure capture则error。规则读取checker类型并追踪local binding；交给另一fn的Region留给runtime检查。`check`以E090拒绝 `@export(c)` fn返回Region，因caller不能free。也拒绝指向非Region参数的 `out`，以及不bind/export C的fn上的 `@region`。`Built`行计数每个借用Region（`@export(c) Region `p`
of `f``），因为长度是caller所言，如 `@region(len = …)` 条款。

<a id="a3-ffislicer-off-n---resultregion-string"></a>

### A3. `ffi.slice(r, off, n) -> Result[Region, String]`

`r`内从 `off` 开始 `n` byte的read-only view。除非 `0 <= off` 且 `off + n <= ffi.len(r)`，否则 `err`；负count也 `err`。view读parent memory，所以后续writable parent写入可见。持有parent owner及release guard，使memory活过view。parent writable时view也不可写。不生成新provenance、不做地址算术：Region携带block内view offset（`off`，所有生成点为0），`read` / `write`在自身bounds检查后加它，`@extern`得到 `base + off`。传slice给 `@extern` 是无copy子范围，消除nendo/ceangal逐frame outbound sub-range copy。修订C-062说明。

<a id="a4-zero-copy-inbound-host-data"></a>

### A4. Zero-Copy Inbound Host Data

- **Push。** host直接写A1 export返回的writable Region，然后调下一export，经程序自身copy（top-level `var`）读取。
- **Pull。** 将writable `ffi.alloc` Region传给 `@extern(wasm)` / `@extern(c)` fn，仅该call借用（D3已有），foreign code原地填充。

<a id="how-others-cross-a-buffer-surveyed-2026-10"></a>

### 其他语言如何跨边界传Buffer（2026-10调查）

| | Outbound view | Inbound，caller buffer | 跨边界lifetime |
|---|---|---|---|
| **Rust** | `&[u8]`经 `as_ptr` / `len` → `(ptr, len)`；`Box<[u8]>::into_raw`交出所有权 | `extern "C" fn`内 `slice::from_raw_parts(_mut)(ptr, len)`，长度为caller所言、断言一次 | call中借用（`'a`实际上无界）；`into_raw` / `from_raw`交出/收回所有权 |
| **Swift** 6.2 | `Span` / `RawSpan`（不可逃逸view） | caller buffer用 `MutableSpan` / `OutputSpan`；`withUnsafeMutableBytes` | `~Escapable`：compiler禁止view活过call；Almide改在runtime检查（A2） |
| **Wasm component model** | canonical ABI将 `list<u8>`复制入callee内存（`cabi_realloc`） | 同样copy | `borrow<T>`随call结束，`own<T>`运行destructor；形如A1 share/A2 lend，但byte非zero-copy |
| **Zig** | `[]u8` = `(ptr, len)`；`export fn f(p: [*]u8, n: usize)` 与 `p[0..n]` | `[*]u8` + `usize`，切片一次 | 无检查，slice可活过memory |

Almide采用Rust/Zig的 `(ptr, len)` 越界，因为回答5排除component model复制。采用CM `own` / `borrow` lifetime区分：A1交出share如 `own`，A2借一个call如 `borrow`。Swift静态检查非逃逸，Almide改为runtime，因为checker没有非逃逸规则（见下文scoped borrow替代）。

<a id="what-stays-trusted-added"></a>

### 仍需信任（追加）

- `@export(c)` Region的C caller `(ptr, len)`：每call一项声明，按declaration在 `Built`计数。
- wasm host可以写线性内存任意位置。A1给它share和长度，不是capability。交给host的read-only Region仅在契约上read-only。

<a id="read-narrowly-named-for-a-decision"></a>

### 狭义解读，明确为决定项

- `--host js`将A1 export包装为 `f(…) → [ptr, len]`（两宽度都为number），增加 `regionRelease(ptr)`（#4354）。driver也可直接读 `instance.exports.f()` 两个数。
- 无C header generator。A2 prototype手写：`int32_t f(const uint8_t *buf, size_t buf_len)`，`out`参数用 `uint8_t *`。

证据：crates/almide-wasm/tests/ffi_host_boundary.rs（wasmtime驱动wasm A1、A4 push/pull），tests/ffi_host_boundary_test.rs（native A2：read-only及 `out`借用、bounds `err`、escape停止、E090），tests/extern_rust_abi_test.rs（native A4 pull，`(uint8_t*, size_t)`），tests/raw_ptr_surface_test.rs和spec/wasm_cross/bytes_rawptr.almd（两target A3、C-062）。

<a id="alternatives-considered"></a>

## 考虑过的替代方案

- **保留 `RawPtr`，与allocation table检查。** runtime记录交出的每个block，`ffi.read`按地址查找。只覆盖生成(1)，foreign pointer不在table，而它才关键。每访问还付lookup成本、扩大可信runtime。否决。
- **`ffi.read`长度参数不与任何内容比对。** 就是当时设计，长度是caller所言。
- **Scoped borrow（`ffi.with_region(b, (r) => …)`、Swift `withUnsafeBytes`）。** 对生成(1)健全，但需checker没有的非逃逸closure规则，对foreign pointer无解。D2 RC share无需新规则给同样lifetime。
- **Hardware capability（CHERI）。** 最强答案，但Almide交付target无此能力。
- **完全移除raw memory，C仅通过 `@extern(rust)` shim绑定。** 只是把同一未检查pointer移进checker不读的 `native/*.rs`；可信基扩大而非缩小。

<a id="consequences"></a>

## 后果

- 收益：经 `ffi`越界读写在两target变 `Err`。可信基从每个 `ffi.read` / `ffi.write` call缩至每foreign declaration一个长度条款。#3420/#3966不再有写法。
- 成本：类型拆分及ABI改变（`Region`为两个C参数）。所有带pointer返回的既有 `@extern` binding须改写（由 `almide fix`机械处理）。每访问付一次bounds检查。
- `Handle` / `Region`拆分让CM映射直接（`own<resource>` / `list<u8>`）。

<a id="falsifier"></a>

## 可证伪条件

- 实际binding的foreign函数返回buffer，其长度既非parameter、constant也非返回out-value，且无法用 `Handle`加accessor绑定。则 `@region(len = …)`需更丰富形式，或D2.3有误。
- 使用 `ffi` 的 `research/benchmark/perf` workload上bounds检查成本超过5%。

<a id="owner-decisions-"></a>

## 所有者决定（○×）

一问一决定，各有建议。

1. **将 `RawPtr`拆为 `Handle`（不透明、仅传回foreign）与 `Region`（base,length,permission,owner，每访问bounds检查）。** 建议○。全部调查设计分离二者（CM resource与 `list`，Swift `OpaquePointer`与 `Span`，Rust opaque pointer与slice）。
2. **foreign pointer返回仅在 `@extern`声明长度 `@region(len = …)`时为 `Region`，否则必须 `Handle`。** 建议○。把Clang `__counted_by` / Swift safe C import移至binding，令每长度主张成为可计数trusted行。
3. **`Region`以pointer和 `size_t`长度两个C ABI参数越界，callee总得runtime长度。** 建议○。若另用caller手写长度，D1检查会止于边界。
4. **由 `Bytes`产生的 `Region`只读，writable `Region`仅来自 `ffi.alloc(n)`。** 建议○。保持值语义，不能写入 `let`（#3966）；Rust `&[u8]`与 `&mut [u8]`、CHERI permission bit同样区分。
5. **越界 `ffi.read` / `ffi.write`返回 `Err`而非abort。** 建议○。两者已有error channel的effect fn，错误foreign长度是caller可处理runtime事实。list索引仍abort（C-067），仅ffi表面如此。
6. **foreign `Region`在声明用 `release = "…"`命名release函数，runtime在最后share drop时调用，Almide代码不free。** 建议○。即CM `own` destructor，消除Almide侧double-free/use-after-free。
7. **删除地址为 `Int`的写法（`ffi.to_int`、`bytes.data_ptr`），wasm `@extern`的 `Region`用 `(i32, i32)`传递。** 建议○。否则 `Int`仍可作为地址到host并绕过长度检查。
8. **D1–D7以一个dialect epoch落地，无弃用窗口，`almide fix`改写旧形式。** 建议○。遵循不匹配形态直接删除的project裁定；表面位于 `import ffi`之后，受影响file集合小且可见。

<a id="references"></a>

## 参考资料

- Rust strict provenance（1.84稳定化）: https://doc.rust-lang.org/std/ptr/index.html#strict-provenance
- `slice::from_raw_parts`: https://doc.rust-lang.org/std/slice/fn.from_raw_parts.html
- CHERI ISA (UCAM-CL-TR-987): https://www.cl.cam.ac.uk/techreports/UCAM-CL-TR-987.pdf ; CHERIoT: https://cheriot.org/
- Zig指针与切片: https://ziglang.org/documentation/master/#Pointers , https://ziglang.org/documentation/master/#Slices
- Swift SE-0447 `Span`: https://github.com/swiftlang/swift-evolution/blob/main/proposals/0447-span-access-shared-contiguous-storage.md ; SE-0458 严格内存安全: https://github.com/swiftlang/swift-evolution/blob/main/proposals/0458-strict-memory-safety.md
- Clang `-fbounds-safety`: https://clang.llvm.org/docs/BoundsSafety.html
- Go cgo指针传递规则: https://pkg.go.dev/cmd/cgo#hdr-Passing_pointers ; `runtime.Pinner`: https://pkg.go.dev/runtime#Pinner
- Component model的canonical ABI与resource: https://github.com/WebAssembly/component-model/blob/main/design/mvp/CanonicalABI.md , https://github.com/WebAssembly/component-model/blob/main/design/mvp/Explainer.md#handle-types
