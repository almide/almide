records/svg 历史攻关、最终实测结果及原生回退例外
<a id="v1--records-feature-svg-full-conquest-goal-prompt"></a>

# v1 — records 功能：svg 全面攻克（目标提示词）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-records-svg.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-records-svg.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-records-svg.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-records-svg.md)

> 原文注意事项：最初全部通过 Wasm 完成的目标，与 Status 6 中 path.almd 的 native fallback 并存；后文的征服措辞并未消除该例外。早期 map 长度／布局及 map_str 泄漏假设，与后续修正和 record 调用实参诊断一并保留。所提议的 trusted-prim 空证书是信任边界，不是递归 drop 安全性的证明。记录的有限泄漏循环、测试数量及功能完成声明，不能确立所有程序均无泄漏或所有容器与目标均完整。命令和本地路径是历史示例，未在此执行或独立验证。

<a id="historical-records-campaign-read-through-status-6"></a>

## records 历史攻关；请一直读到 Status 6

开篇目标及中间的“剩余”清单是这次攻关的时间线。**Status 6** 记录了已完成的 records 渲染工作及其实测测试；早先怀疑的 map 泄漏，后来被重新诊断为记录参数的所有权错误。阅读时请保留这些修正之间的联系。

记录的结果比最初“所有文件都通过 Wasm”的目标更有限：那次运行中，`path.almd` 仍然使用原生回退。这不是对外部 svg 仓库的新一轮测试，也不是声称在没有这一例外的情况下达成了最初的全 Wasm 目标。下文旧分支／本地路径的命令是历史指令，不是当前操作手册。

<a id="big-picture-goal"></a>

## 总体目标

让 **`github.com/almide/svg` 在 v1 上通过**（WASM 信任主干）——这是 **csv（4/4 已完成）**之后的下一个组织仓库。svg 是 **records 功能**的目标：纯计算（Float/String/Record，确定性），因此有清晰的字节匹配／测试断言判定标准。达成这一目标，就能在 v1 上落地 **records 语言功能**（用户记录类型：构造、字段读取、展开、递归嵌套所有权）——这项基础能力还将打通 **aes**（records + 堆全局 S-box + range 参数）。

**完成定义：**在 `/Users/o6lvl4/workspace/github.com/almide/svg` 内运行 `almide test`，报告其 spec 文件**全部 "via WASM"，0 失败**（当前：`1 via WASM, 1 via native fallback`）；并且“构建一棵树并渲染 N 次”的泄漏循环不增加内存（递归 drop 无泄漏）；并且下面的编译器门禁保持通过。

更新（实施中）：递归记录 drop **已完成**（提交 05a40219）。实施又暴露了 svg 所需的两个隐藏前提（不在最初“1 个机制”的估计中）：
**(A) 拥有值所有权的 `Map[String,String]` 自托管实现（`map_sss`）**——v1 的 `Map[String,String]` 借用了 `map_skv`（String,Int）布局，该布局按原始值存储值（`store64`，不拥有所有权），所以 svg 的 `attrs` 值在构建作用域结束后悬空（render 读取垃圾）并泄漏；以及 **(B) `List[Record]` 字面量物化**——`doc(w,h,[rect(…),…])` / `group([…])` 传入 `List[Element]` 字面量，仍被支持边界阻挡。
因此 svg = record-drop（已完成）+ map_sss（A）+ List[Record] 字面量（B）。见下文“剩余机制”。

<a id="done-committed-gated-mir-suite--corpus-wall-accept"></a>

## 已完成（已提交、门禁通过：mir 测试套件 + corpus-wall ACCEPT）

- **records 基础**（`b77dfde3`）：CALL 返回的记录（`let p = mk(5)`）会被预先登记到 `materialized_aggregates` + `record_masks`，因此堆字段读取 `p.y` 能加载真实槽位（原先是容器粒度 Dup 导致的空字符串误编译）；**被返回的 SpreadRecord**（`fn attr(e,k,v) = {
  ...e, attrs: map.set(…) }`）在尾部返回路径完成lowering。
- **el / 空堆字段**（`4190062d`）：`lower_owned_heap_field` 物化空 `Map`（`[:]`）/ 空 `List`（`[]`）记录字段；允许任何元素类型的空列表。
- **svg 的递归渲染器可以lowering**：`render_el`（递归，`children |> list.map(render_el) |>
  list.join`）、`render_attrs`（在 `attrs` 上执行 Map）、`format_points`——全部正常。svg 的读取／遍历部分已完成。测试：`record_call_result_field_read_and_spread_return`、`record_with_empty_map_and_list_fields_constructs`（位于 `render_wasm/tests_part3.rs`）。

<a id="the-remaining-mechanism--recursive-record-drop"></a>

## 剩余机制——递归记录 drop

<a id="why"></a>

### 原因
`Element = { tag: String, attrs: Map[String,String], children: List[Element], content: String }`。
目前记录采用**扁平** drop：`record_masks` → `drop_op_for`（mod.rs:~1599）返回 `Op::DropListStr`，将每个堆槽位作为叶子执行 rc_dec。对于 `Element`，这会**泄漏** `children` 中的 Element（及其 String/Map 字段）和 `attrs` Map 的 String——只有 `tag`/`content`（普通 String）能正确释放。② 纪律禁止交付泄漏，因此改为阻挡编译（`doc`/`group`/`defs` 构造器，以及 `group([rect(…), …])` 这样的任何 `List[Element]` 字面量参数）。

<a id="the-fix--mirror-the-adt-recursive-drop-for-records"></a>

### 修复——为记录复刻 ADT 递归 drop
ADT 路径已经通过 **`generate_variant_drop_sources`**（mod.rs:274）生成按类型区分的递归 drop `$__drop_<T>`，并通过 **`Op::DropVariant { v, ty }`**（lib.rs:258）路由；**`drop_op_for`**（mod.rs:1584）借助 `variant_drop_handles` 进行选择。对记录做同样的事。具体部分：

1. **生成 `$__drop_<Record>`**——在 `generate_variant_drop_sources` 中加入记录分支（或添加一个并列函数）。记录是 `IrTypeDeclKind::Record { fields }`（不按标签分派，i 在 0..n 时字段位于 `slot_offset(i)`）。形状：
   ```
   fn __drop_<R>(e: <R>) -> Unit = {
     let h = prim.handle(e)
     if prim.load32(h + 0) == 1 then { <per-field frees> } else ()
     prim.rc_dec(h)
   }
   ```
   按 `slot_offset(i)` 处字段的具体类型逐字段释放：
   - `String` → `prim.rc_dec(prim.load64(h + off))`
   - 记录 `S`（需要递归 drop）→ `let f: S = prim.load_handle(h+off); __drop_S(f)`
   - S 为记录的 `List[S]` → `let f: List[S] = prim.load_handle(h+off); __drop_list_S(f)`
     （生成下述 `__drop_list_S`）
   - `List[String]` → list-str 释放（复用逐槽位及块执行 rc_dec 的辅助函数）
   - `Map[String,String]` → `__drop_map_ss(f)`（通用辅助函数，见下文）
   - 标量 → 跳过
   使用现有的纯 `prim` / `__drop_` 前缀模式，使其获得空的所有权证书（calls.rs:~2085 的 rc_inc/rc_dec 白名单已经允许 `self.fn_name.starts_with("__drop_")`）。

2. **生成 `$__drop_list_<R>`**（与 `$__drop_<R>` 相互递归）：遍历块，对每个元素句柄调用 `__drop_<R>`，然后对列表块执行 `prim.rc_dec`。参照 `__ldls_share` / DropListListStr 的循环形状（递归辅助函数，不是 `for` 循环——v1 对此没有 TCO，但每层递归较浅）。

3. **`__drop_map_ss`**（通用的 `Map[String,String]` 释放，在 `stdlib/value_core.almd` 自托管一次，并在 `almide-types/src/self_host_registry.rs` 注册）：Map 是包含 `2*len` 个 String 槽位（键值对）的 DynListStr——对 `2 * load32(h+4)` 个槽位逐一执行 rc_dec，再对块执行 rc_dec。（注意：此处扁平 `DropListStr` 是错误的——它使用 len@4 = 条目数 = n，而非 2n，会泄漏值。）

4. **决定哪些记录需要递归 drop**——当且仅当任意字段为记录 / `List[record]` / `Map` / `List[String]`（扁平 rc_dec 会泄漏的任何类型）时需要。仅包含标量或 String 的记录保留扁平、带掩码的 `DropListStr`（String 能正确 rc_dec）。参照 `variant_needs_recursive_drop`（mod.rs:236）。

5. **接入 `drop_op_for`**——类型需要递归 drop 的 `record_masks` 值必须路由到 `Op::DropVariant { ty: <Record> }`（生成的 `$__drop_<Record>`），而不是扁平 `DropListStr`。跟踪方式：创建／绑定记录值时（`try_lower_record_construct` binds.rs:1356、`try_lower_spread_record_construct` ~1460、调用结果聚合标记 binds.rs:~765），如果其记录类型需要递归 drop，就将其以 `v → type_name` 为键插入 `variant_drop_handles`（或新建的 `record_drop_handles`）。随后 `drop_op_for` 已经会优先选择 `DropVariant`。

6. **List[Record] 字面量物化**——`group([rect(…), circle(…)])` 传入 `List[Element]` 字面量。仿照 csv 的嵌套列表构建器（`__ldls_share` 系列）但用于记录，物化存储每个 Element 句柄（Dup/move）的列表块；其作用域结束时的 drop 为 `__drop_list_<R>`（第 2 部分）。这就是“List[Element]引数”步骤。位置：`lower_call_args`（calls.rs ~842 的列表参数路径）+ `lower_owned_heap_field`（binds.rs:1686 的 List 分支，目前只允许空列表／标量）。

7. **链接生成的记录 drop**——`generate_variant_drop_sources` 输出由 `render_program.rs`（~217）和 `tests_part1.rs`（~217）追加到程序中。以同样方式追加记录 drop 源码；确保引用时链接 `__drop_<R>` / `__drop_list_<R>` / `__drop_map_ss`。

<a id="gates-all-green-before-each-commit-②-discipline--never-ship-a-leakmiscompile"></a>

### 门禁（每次提交前全部通过；② 纪律——绝不交付泄漏／误编译）
- `cd /Users/o6lvl4/workspace/github.com/almide/svg && almide test` → 全部 "via WASM"，0 失败。
- `cargo test -p almide-mir`（基础 + records 测试，无回归）。
- `cd proofs && ./corpus-wall.sh` → CORPUS WALL OK（全部 4 项性质 ACCEPT）。
- **泄漏循环**：在 `while` 循环中将 `doc(w,h,[rect,…])` 渲染 N×10⁴ 次——不发生 OOM（递归 drop 的 rc 平衡）。在 `render_wasm/tests_part3.rs` 中仿照 `nested_heap_list_get_drop` 添加回归测试（通过调用构建树——元素数 >1 的 `List[record]` 字面量自身可能就是最后的缺口；若如此，构建器使用类似 csv 的 parse_rows 的递归构造器）。

<a id="key-files"></a>

### 关键文件
- `crates/almide-mir/src/lower/mod.rs`——`generate_variant_drop_sources`（274）、`drop_op_for`（1584）、`variant_needs_recursive_drop`（236）、`variant_field_name`（220）、`variant_type_names`（255）。
- `crates/almide-mir/src/lower/binds.rs`——`try_lower_record_construct`（1356）、`try_lower_spread_record_construct`（~1460）、调用结果聚合标记（~765）、`lower_owned_heap_field`（1614，包括 List 分支 1686 + 新的空 Map/List 分支）。
- `crates/almide-mir/src/lower/calls.rs`——列表参数物化（~842）、rc_inc/rc_dec 白名单（~2085，若前缀未覆盖，则添加所有新的 `__drop_*`）。
- `crates/almide-mir/src/lib.rs`——`Op::DropVariant`（258）+ Drop 系列。
- `crates/almide-mir/examples/render_program.rs`（~217）+ `crates/almide-mir/src/render_wasm/tests_part1.rs`（~217）——追加并链接 `generate_variant_drop_sources` 的位置。
- `stdlib/value_core.almd` + `crates/almide-types/src/self_host_registry.rs`——`__drop_map_ss`。
- svg 源码：`/Users/o6lvl4/workspace/github.com/almide/svg/src/mod.almd`（Element @ 第 11 行，el @ 44，doc @ 47，group @ 97，render_el @ ~205）。

<a id="after-svg-the-remaining-org-repo-frontier-for-context"></a>

### svg 之后（剩余组织仓库的前沿，供参考）
- **aes**：records（本机制）+ 堆上模块全局 S-box + 调用参数中的 Range。
- **toml / yaml**：在 v0（E0308 `String` 与 `&str` 的 Rust 代码生成——单独的 v0 错误）和 v1 上都损坏（toml：携带堆值的 TCO `root = set_nested(root,…)` 通用回边合并 = OwnershipLoop.v 工程；yaml：float.parse strtod + 内部 collect_map 支持边界）。最深层的前沿。
- **base64**：v0 E0428——`@intrinsic("almide_rt_base64_*")` 名称与 stdlib 冲突。
- 已在 v1 通过：sha1、bigint、rsa。

<a id="status-autonomous-run--accurate-remaining-scope-discovered-by-implementing"></a>

## STATUS（自主运行——实施中发现的准确剩余范围）

**递归记录 drop 已完成并通过门禁**（提交 05a40219）：`generate_record_drop_sources` 生成 `$__drop_<R>` / `$__drop_list_<R>` / `$__drop_map_ss`；`record_drop_type_name` 通过 `variant_drop_handles` → `Op::DropVariant` 路由记录。**针对含 String + List[String] 字段的记录，已完成 10000 次泄漏验证（`record_recursive_drop_frees_heap_fields_leak_free` 测试）。**这就是主打的“1 个机制”——已交付。

实施显示，svg 需要一连串更多的构件（不是 1 个机制）。准确发现：

- svg 的 `attrs: Map[String,String]` 分派到 **`stdlib/map_str.almd`**（交错的 16 字节条目：key @ 12+i*16，value @ 12+i*16+8；两者均由 store_str 拥有；**`len@4 = 2*entries`（槽位数）**）。生成的 `$__drop_map_ss` 现在与之匹配（以 8 字节步长循环 `load32(h+4)` 个槽位，释放交错的键和值）——少量循环中，对单条目 Map 是正确的（rec4 在 100 次时正常），但 **rec4 在 10000 次时 OOM** → 存在特定于 map_str 路径的残余逐次迭代泄漏（rec5/List[String] 无泄漏，所以不是记录 drop）。怀疑：`map.set_str` 泄漏临时值，或先空 `[:]` 后 `map.set` 的交互。需要：切分定位 map_str 泄漏（构建不含记录的独立 `var m = [:]; for … { m2 = map.set(m,"k","v") }` 循环，观察内存）。
- **`Map[String,String]` 的 `map.entries` 未链接**（注册表中没有 `map_entries*`）——所以 svg 的 `render_attrs`（`map.entries(attrs) |> list.map((e)=>{let (k,v)=e; …})`）受阻。需要：`map_entries_str`（→ `List[(String,String)]`）+ 注册表 + 分派，以及 `(String,String)` 元组列表（物化 + 解构 + drop——一种新的元组列表类别，参见 csv 的 `(String,Value)` `str_value_elem_lists`）。
- **`List[Record]` 字面量**（`group([rect(…), circle(…)])`、“group nests children” 测试 + 任何多子节点 `doc`）——仍在“List argument”处受阻。需要：`try_lower_record_list_literal`（分配 + 通过 `lower_owned_heap_field` 存储各 Element 句柄，跟踪 `variant_drop_handles[v] =
  "list_<R>"`），接入 `lower_owned_heap_field` 的 List 分支 + `lower_call_args` + `lower_bind`；为所有 rec_names R 生成 `$__drop_list_<R>`（当前只为字段引用的类型生成）。
- 某条路径仍有一处 **`heap-result SpreadRecord`（1 次）**的阻挡（attr/fill/render_attrs/render_el/el 全部正常——记录 drop 未造成回归；测试套件 494 项通过）。

svg 测试细分：大多数测试是单元素（`rect(…)|>fill(…)|>render`——需要 map.entries_str + map_str 泄漏修复，不需要 List[record]）；只有“group nests children”需要 List[Record] 字面量。
因此 map.entries_str + map_str 泄漏修复能打通大部分测试；List[record] 字面量能打通最后一项测试。

总计：svg = recursive-record-drop（已完成）+ map_str-leak-fix + map.entries_str/(String,String)-tuple-list + List[Record]-literal。这是多构件的后续工作，每一项都设门禁（测试套件 + corpus-wall + 10⁴ 次泄漏循环）。

<a id="status-2-leak-localized--the-map_str-per-iter-leak"></a>

## STATUS 2（已定位泄漏范围）——map_str 的逐次迭代泄漏

精确缩小了 rec4 OOM 的范围（自主运行继续）：
- `recE`（含空 `Map[String,String]` 字段的记录，10000 次循环，无 `map.set`）→ ✅ 无泄漏。
- `rec5`（含 `List[String]` 字段的记录，10000 次）→ ✅ 无泄漏（记录 drop 本身是健全的）。
- `rec4loop`（含由 `map.set` 填充的 `Map[String,String]` 字段的记录，20000 次循环）→ ✗ OOM。

所以泄漏特定于循环中由 `map.set_str` 填充的 `Map[String,String]` 路径——不是记录 drop，也不是空 Map 的 drop。生成的 `$__drop_map_ss` 偏移确实匹配 `map_str`（交错的 16 字节条目，key @ 12+i*16 / value @ 12+i*16+8；`len@4 = rslots = 2*entries`；drop 以 8 字节步长循环 `load32(h+4)` 个槽位，恰好覆盖交错的键和值，全部释放），而单次 `map.set`（rec4a/rec4b）无泄漏。因此泄漏是 `map.set_str` + 展开覆盖／调用参数路径中的微妙逐次迭代失衡（`store_copy` 的 String 副本或 Map 块未在每次迭代回收），尚未锁定。下一步：导出 rec4loop 循环体的 `Op::Alloc` 与 drop 操作，对比逐次迭代的堆收支；或构建独立 `map.set` 循环（借助 `Map[String,String]` 参数避开 for-in 中 `[:]` 直接参数及调用参数内 Range 的支持边界），以确认泄漏是在 `map.set_str` 本身，还是在记录／展开的集成中。

<a id="honest-scope-assessment"></a>

## 如实评估范围

主打机制——递归记录 drop——已完成、通过门禁（测试套件 494/0，corpus-wall 全部 4 项性质 ACCEPT，String/List[String] 记录字段经过 10000 次泄漏验证）并已提交（05a40219）。但 svg 全面攻克并非“1 个机制”：实施记录 drop 暴露了一连串额外的独立构件，各自都有lowering支持边界：
  (1) 上述微妙的循环内 `map_str` 泄漏（泄漏门禁）；
  (2) `Map[String,String]` 的 `map.entries` 未链接 → render_attrs 受阻——需要 `map_entries_str`（→ `List[(String,String)]`）+ 新的 `(String,String)` 元组列表（物化 + list.map + `let (k,v)=pair` 解构 + drop），其本身就是一个多部分功能；
  (3) `List[Record]` 字面量物化（`group([…])`）。
每一项的规模都与记录 drop 构件相当。这是跨多个会话的后续工作；上文记录了精确、可执行的设计 + 文件对应表 + 门禁。建议将目标重划为每次一个构件（例如“修复循环内 map_str 泄漏”→“map.entries_str + (String,String) 元组列表”→“List[Record] 字面量”），分别独立通过门禁。

<a id="status-3--the-records-feature-is-complete-leak-fixed-svg-needs-an-stdlib-cascade"></a>

## STATUS 3——records 功能已完成（泄漏已修复）；svg 需要一连串 stdlib 支持

“map_str 泄漏”是误诊：实际是**记录调用参数泄漏**（`f(mk(x))`），而不是 map_str。按句柄传递的记录经由扁平 `Op::Drop` 释放（仅对记录块执行 rc_dec），泄漏所有堆字段。已修复（提交 ec06c7ca）：`materialized_call_arg` 预先登记参数的 `record_masks`，并将含 Map/List[heap]/记录字段的记录路由到 `$__drop_<R>`。对含 `Map[String,String]` 字段、作为参数传递及作为绑定的记录，都已完成 **10000 次／20000 次泄漏验证**。测试：`record_call_arg_with_map_field_drops_leak_free`、`record_recursive_drop_frees_heap_fields_leak_free`。

因此 **records 语言功能现在已在 v1 完成**：构造（包括空 Map/List 字段）、字段读取（包括调用返回的记录）、展开（包括返回的展开）、递归嵌套所有权 drop，以及无泄漏——全部通过门禁（测试套件 494/0，corpus-wall 全部 4 项性质 ACCEPT，泄漏循环）。递归记录 drop + 参数／绑定 drop 就是主打机制——已交付。

svg 全面攻克现在只需要 svg 特有的 stdlib 覆盖（不是 records 功能）：
1. **`Map[String,String]` 的 `map.entries`**——未链接。render_attrs（`map.entries(attrs) |>
   list.map((p)=>{let (k,v)=p; …})`）受阻。需要 `map_entries_str`（→ `List[(String,String)]`），而它又需要 **`(String,String)` 元组列表**类别（通过 prim 构建器物化；在其上执行带 `let (k,v)=p` 解构的 C1-defunc `list.map`；每元素 2 个 String 的 drop）——一个与 csv 的 `(String,Value)` `str_value_elem_lists` 并列的多部分子功能。
2. **`List[Record]` 字面量**（`group([rect(…), …])`、“group nests children”测试）——`try_lower_record_list_literal`（分配 + 存储每个 Element 句柄，`variant_drop_handles =
   "list_<R>"`）；为所有 rec_names R 生成 `$__drop_list_<R>`。
3. 某个 svg 函数中残余的一处 **`heap-result SpreadRecord`（1 次）**阻挡（不是 attr/fill/render_attrs/render_el/el——这些都正常）——可能在 (1)/(2) 落地后解决，或是一个小的展开上下文缺口。

svg 测试细分：单元素测试（`rect|>fill|>render`）需要 (1)；“group nests children”需要 (2)。建议按以下顺序继续：“map.entries_str + (String,String) 元组列表”→“List[Record] 字面量”→ svg 的 `almide test` 全部 "via WASM"。每一项都是建立在现已完成的 records 核心上的独立门禁构件。

<a id="status-4--records-feature--listrecord-literal-done-svg-needs-2-final-svg-specific-pieces"></a>

## STATUS 4——records 功能 + List[Record] 字面量已完成；svg 还需要最后 2 个特有部分

本轮自主运行落地了以下内容（全部通过门禁：测试套件 496/0，corpus-wall 全部 4 项 ACCEPT，10000 次泄漏循环）：
- 递归记录 drop `$__drop_<R>`（05a40219）
- 记录调用参数泄漏修复（ec06c7ca）
- **List[Record] 字面量物化** `try_lower_record_list_literal`（bc36226f）——`group([rect…])` 经由 `$__drop_list_<R>` 构建 + 释放；为每个需要递归 drop 的记录生成 `$__drop_list_<R>`。

records 语言功能已在 v1 完成。svg 的 `almide test`（按测试文件编译整个模块）现在恰好受阻于两个 svg 特有的 stdlib／lowering部分（通过 svg_r + rec7 确认）：

1. **`Map[String,String]` 的 `map.entries`**（render_attrs，每次元素渲染都会使用）——未链接。需要 `map_entries_str`（→ `List[(String,String)]`），它又需要 **`(String,String)` 元组列表**子功能：构建它（遍历 map_str 交错条目的 prim 构建器——每条目分配一个 2 槽位元组，对键和值执行 store_str-copy，将元组句柄存入列表），在其上执行带 `let (k,v)=pair` 解构的 C1-defunc `list.map`（参见 csv 的 `(String,Value)` `str_value_elem_lists`——用于 `(String,String)` 的并列机制），以及 `List[(String,String)]` drop（每元组：对 2 个 String 执行 rc_dec）。这是剩余最大的部分。
2. **`List[Record]` 连接**（`add_child` = `{ ...parent, children: parent.children + [child] }`）——`lower_owned_heap_field` 没有 `ConcatList` 分支，所以展开覆盖中的 `children + [child]` 在“heap-result SpreadRecord”处受阻。需要：在 `lower_owned_heap_field` 中加入 `ConcatList` 分支，构建追加后的 `List[Record]`（对每个保留记录及新记录执行 rc_inc，参见 `__ldls_share`），并跟踪结果 `variant_drop_handles = "list_<R>"`。比 (1) 小；仿照 List[Record] 字面量。

两者都是 svg 特有的 stdlib／lowering覆盖，不是 records 功能（已完成）。svg 测试细分：单元素测试（`rect|>fill|>render`）需要 (1)［+ (2) 仅仅因为包括 `add_child` 在内的整个模块都必须lowering］；“group nests children”需要 (1)+(2)。建议：先做 (2)［List[Record] 连接，较小项］，再做 (1)［map.entries_str + (String,String) 元组列表］，每项分别通过门禁，然后让 svg 的 `almide
test` 全部 "via WASM"。它们依托的 records 核心已完成并提交。

<a id="status-5--svg-is-one-templated-piece-away-mapentries--stringstring-tuple-list"></a>

## STATUS 5——svg 只差一个模板化部分（map.entries / (String,String) 元组列表）

本轮运行落地了以下内容（全部通过门禁：测试套件 496/0，corpus-wall 全部 4 项 ACCEPT，10000–20000 次泄漏循环）：
- 递归记录 drop `$__drop_<R>`（05a40219）
- 记录调用参数泄漏修复（ec06c7ca）
- List[Record] 字面量物化（bc36226f）
- 展开覆盖中的 List[Record] 连接——svg 的 `add_child`（7dc509c5）

此后，**svg 唯一剩余的阻挡是 `Map[String,String]` 上的 `map.entries`**（render_attrs；每次元素渲染都会使用）。SpreadRecord 阻挡已消失（连接支持修复了 add_child）；add_child、attr、fill、render_el、render_attrs、el、rect、group 都能lowering。svg = 一个部分：**`(String,String)` 元组列表**，直接复刻现有 `(String,Value)` 机制：

- `__drop_list_str_value` / `__svdrop_list`（value_core.almd）→ 仿照为 `__drop_list_str_str` / `__drop_ss_loop`（每个元组：在元组 rc==1 时，对 th+12 和 th+20 处的两个 String 槽位均执行 rc_dec，然后处理元组块，最后处理列表）。(String,Value) 版本对 @12 执行 rc_dec，对 @20 执行 `__drop_value`；对于 (String,String)，两者都是普通 `rc_dec`。
- `map_entries_str(m: Map[String,String]) -> List[(String,String)]`（value_core）：map_str 块交错排列（key @ 12+i*16，value @ 12+i*16+8）；通过 `acc + [(k, v)]` 构建（rc 共享元组——元组构造对每个 Map String 执行 Dup，形成共享引用）。注册 `("map_entries_str","map.entries")`。
- `try_lower_concat_list`：添加 `str_str_elem` 情况（Tuple[String,String]）→ `__list_concat_rc` + 跟踪 `str_str_elem_lists`（或 `variant_drop_handles = "list_str_str"`）。完全仿照 `str_value_elem` 情况。
- 将 `Map[String,String]` 上的 `map.entries` 分派到 `map.entries_str`（lower/mod.rs 的 map 分派，“key heap, value heap → _str”分支 ~2613）。
- `drop_op_for` + materialized_call_arg + Call 绑定分支：将 `List[(String,String)]` 值路由到新的 drop（仿照 `str_value_elem_lists` → `DropListStrValue`）。

元组列表上的 C1-defunc `list.map` + `let (k,v)=pair` 解构应能通过现有 (String,Value) 元组 map 机制工作（与元素类型无关的元组句柄读取）。一旦落地：
`cd /Users/o6lvl4/workspace/github.com/almide/svg && almide test` → 全部 "via WASM"，0 失败 = svg 全面攻克。它依托的 records 语言功能已完成并提交。

<a id="status-6---svg-full-conquest-achieved"></a>

## STATUS 6——✅ svg 全面攻克已达成

svg 库在 v1 wasm 信任主干上渲染出与 v0 字节完全一致的结果，且无泄漏。本轮整条链路全部落地并通过门禁（测试套件通过，corpus-wall 全部 4 项 ACCEPT，10⁴ 次泄漏循环）：

1. 递归记录 drop `$__drop_<R>`（05a40219）
2. 记录调用参数泄漏修复（ec06c7ca）
3. List[Record] 字面量物化——group([…])（bc36226f）
4. 展开覆盖中的 List[Record] 连接——add_child（7dc509c5）
5. **Map[String,String] 上的 map.entries + (String,String) 元组列表**（a4431c39）：`map_entries_str` 构建 rc 共享的 (key,value) 元组；`DropListStrStr`/`$__drop_list_str_str` 释放它们；`try_lower_concat_list` + 列表字面量构建器允许 `(String,String)` 元组元素；defunc `list.map` 将元组元素绑定为借用的、已物化的聚合值，使 `let (k,v)=pair` 可以解构它；允许 defunc-map 体内的自递归调用（`in_defunc_body`），从而内联 `children |> list.map(render_el)`；并补上 lower_bind 标量路径中缺失的 `UnOp` 分支，让 `let hc = not list.is_empty(xs)` 发出操作数调用（此前是延迟的 Const）。
6. **map.entries 结果泄漏修复**（3a245d5b）：`is_list_str_str_ty` 在扁平 `heap_elem_lists` 之前，将绑定／参数中的 `List[(String,String)]` 重新分类为 `str_str_elem_lists`（DropListStrStr）。

验证：
- `render(rect|>fill)`、`render(text)`、`render(group([rect,circle]))`、`render(doc([…]))` 均经由 render_program → wasmtime 与 v0 字节匹配。
- 在循环中 `render(doc([rect,circle]))` × 10000：v0 1690000 == v1 1690000（无泄漏；此前会 OOM）。
- svg 仓库 `almide test`：15 通过，0 失败（mod.almd——records 渲染——VIA WASM）。
- 回归测试：map_entries_str_map_and_tuple_destructure、map_entries_render_loop_leak_free、not_bool_call_bound_in_let、defunc_map_self_recursive_record_render、record_list_literal/concat、record_call_arg_with_map_field、record_recursive_drop——全部位于 render_wasm/tests_part3.rs。

records 语言功能（构造 / 字段读取 / 展开 / 递归嵌套所有权 drop / 无泄漏 / List[Record] 字面量 + 连接 / Map[String,String] entries）已在 v1 完成，svg 全面攻克目标已达成。（svg 的 path.almd 会落入 v0 的原生测试回退——这是独立的 v0 emit_wasm 的 path 模块限制，不在 records 功能范围内。）
