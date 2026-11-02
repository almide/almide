说明：通过机械化所有权纪律，清理引用计数释放的积压问题
<a id="wasm-reference-count-frees-the-ownership-discipline-drain"></a>

# WASM 引用计数释放：所有权纪律的问题清理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-frees-ownership-discipline.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-frees-ownership-discipline.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](wasm-frees-ownership-discipline.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/wasm-frees-ownership-discipline.md)

> 原文注记：6月7–11日各条是按时间记录的历史报告，不是当前测量或新执行。后文 Stage C/D 完成更新早期剩余工作及最初断言的 M2 根因。原文称 ptr < heap_start 且 heap_start 为0会始终 true，保留该矛盾，不推测修正。双 gate 标题实际列四个完成条件。leak 安全侧／bounded 是相对 double-free crash，并非绝对安全或资源上界；增长 accumulator 和过量计数限制仍保留。RC certificate 是编译器证据，不是第三方安全认证。默认 flag、命令及 kill process 指令都是历史资料，未执行。


**状态：** 进行中（2026-06-07）。分支 `perceus-belt-hard-error`，worktree `xtarget-compound-display`。全部工作均为 LOCAL／未提交，等待完整双 gate 通过。

<a id="mission"></a>

## 使命

WASM 是内存方面的前沿。native Rust 自带借用检查器，而 WASM 后端历史上**按设计泄漏全部 heap**：`compile_rc_inc` / `compile_rc_dec`（`emit_wasm/runtime.rs`）故意为空操作（`if ptr <
heap_start return` 读取始终为 `0` 的 legacy `heap_start_global` 字段，因此 guard 始终为 true）。整个 Perceus belt 一直在验证空操作的 RC 平衡。目标是**启用释放**，让 WASM 回收 heap（churn O(1)，而不是 O(n)），**不引入任何 double-free**，并保持 native↔WASM 字节一致。

这正是“机器编写软件的信任层”的核心：同一程序运行于 native 或 WASM sandbox，都正确释放内存并产生相同字节。

<a id="the-discovery"></a>

## 发现

把 rc guard 切到正确的 `HEAP_START_GLOBAL_IDX`（=4），就会启用已构建好的 free-list 机制（alloc 遍历和复用、rc_dec push）。这立即暴露出空操作模型掩盖的一类**既有潜在 double-free**：heap 值被*借用／别名引用*或 *move 到容器*时，简化 Perceus 会插入作用域末尾 `Dec`，释放仍被其他地方拥有的值。启用释放后，会形成 free-list 环（无声挂起）；或者由加入 `rc_dec` 的 **double-free sentinel guard**（把已释放 block 的 rc 标为 0，第二次 Dec 时触发 `unreachable`）转为明显的 WASM trap，被 cross-target gate 捕获。

<a id="root-cause-the-simplified-perceus-has-no-ownershipmove-discipline"></a>

## 根因：简化 Perceus 没有所有权／move 纪律

真正的 Perceus（Koka）追踪*所有权*：创建新 owner 时 dup（Inc），owner 最后一次使用时 drop（Dec），constructor／return 消费值时 **move**（转移所有权，不另行 drop）。Almide WASM Perceus 是简化版：作用域末尾 Dec *每个* heap local，而*仅*对 `Var` / `Clone` / `Deref` 绑定 Inc。这在两个方向都不正确：

1. **借用／别名值绑定到会被 Dec 的 local 时计数不足**：local 的作用域末尾 Dec 会释放源仍然持有的值（提取别名、env-load、直接元素 accessor）。
2. **move 到容器的值被过多 Dec**：容器按引用接收值（没有 copy／Inc），随后 Perceus 又 Dec 源 local，深度释放容器内容。

本轮清理的每个 bug 都是这一缺口的一个侧面。

<a id="the-completion-bar--both-gates-the-key-lesson"></a>

## 完成标准＝两个 gate 都通过（关键教训）

`almide test spec/ --target wasm`（test block assertion corpus）**必要但不充分**。test block 检验值断言，却跳过整类形状。**`cargo test --release --test wasm_runtime_cross_target`** gate（约 100 个 `spec/wasm_cross/*.almd` `@contract` fixture，stdout／stderr／exit 字节一致）检验 corpus 从未覆盖的形状，例如 **compound set/map key**（`compound_eq.almd`，contract C-015）。真正完成标准是：

- `almide test spec/ --target wasm` clean（assertion），且
- `cargo test --release --test wasm_runtime_cross_target` clean（字节一致），且
- churn benchmark O(1)（leak gate，见下文），且
- native `almide test spec/` 无回退。

“test corpus 240/0”只是早期的假终点；cargo gate 发现 `compound_eq` 仍有分歧。

<a id="mechanisms-implemented-each-a-facet-of-the-one-discipline"></a>

## 已实现机制（同一纪律的各个侧面）

全部复用一个分类器：`pass_perceus.rs` 中的 `yields_borrowed_alias(e)`（穷尽 `match`、无 wildcard＝全覆盖；新的 `IrExprKind` 必须明确分类）。ALIAS 形式获取引用，FRESH 形式已拥有自身引用。leak／crash 的不对称性决定安全侧默认：别名**少一次** Inc＝double-free（crash）；fresh 值**多一次** Inc＝leak（安全侧）。所以未知形式仅在证明成立处默认 FRESH，否则 ALIAS。

| # | 机制 | 位置 | 修复对象 |
|---|-----------|-------|-------|
| 1 | `yields_borrowed_alias` + **VDecl 绑定后 alias-Inc**（涵盖旧 Var/Clone/Deref Rule-1；覆盖 Member/Index/TupleIndex/MapAccess/OptionalChain、Match/If/Block tail、Unwrap/Try/ToOption/UnwrapOr 剥离） | `pass_perceus.rs` ChainHead::VDecl | hamming, default_fields, result_option_matrix |
| 2 | **`emit_stored_field`** — constructor／value builder 对 alias 参数 dup | value str/array/object（`calls_value.rs`）、Record/List/Tuple（`collections.rs`）、OptionSome/ResultOk/ResultErr（`expressions.rs`） | codec/variant 群（11）、capture_clone, protocol_advanced, try_parse_list |
| 3 | **fold accumulator dup** — `fold(xs,seed,f)` 以 move 返回 seed；对 alias seed dup | `calls_list_closure2.rs:1046,1155` | option.collect-empty, is_balanced-empty |
| 4 | **排除 EnvLoad 借用** — closure body 的 `EnvLoad` 绑定 local 借用 env 拥有的 capture，因此排除作用域末尾 Dec | `collect_heap_vdecls` | `>>` 组合链（compose_test） |
| 5 | **`is_alias_returning_runtime_call`** — `list.get_or` / `map.get_or` 直接返回元素指针（alias） | `pass_perceus.rs` | protocol_extreme decode |

机制 #2/#3/#5 **直接**发出 `call rt.rc_inc`（不是 IR `RcInc` node）。

<a id="verification-status-2026-06-07"></a>

## 验证状态（2026-06-07）

- ✅ `almide test spec/ --target wasm`：**240 passed / 0 failed**（8 wasm:skip）。
- ✅ native `almide test spec/`：**248/248**。
- ✅ churn（{构建 Codec record + encode + value.stringify}，2M 次）：peak RSS **7.46 MB**＝O(1)，无泄漏。
- ❌ cargo `wasm_runtime_cross_target` gate：**`compound_eq.almd`（C-015）分歧**。`set.contains(sr, {record})` 返回 F（应为 T），并在 `main` 退出时触发 `rc_dec` sentinel trap。（完整分歧列表待 gate 运行。）

<a id="second-layer-the-collection-runtime-found-by-the-cargo-gate-2026-06-08"></a>

## 第二层：COLLECTION RUNTIME（cargo gate 于 2026-06-08 发现）

上述 expression／constructor／closure 清理通过 test corpus 和 2M 次 churn **双方**验证。但 `cargo wasm_runtime_cross_target` gate（字节一致，`almide test` 无法触及的部分）暴露出**第二层**：WASM collection runtime 把 heap element／key／value 的指针复制到新结构，却不 dup，因此源在作用域末尾的 Dec 会深度释放新结构持有的内容。完整流式扫描 100 个 fixture 的字节（仅 stdout；`control_flow` / `cross_module_spread` 是 md5 误报，即 native compiler 在 stderr 上的 *warning*，不是分歧），发现 **4 个真实分歧**，全是 frees-on 下的 WASM `rc_dec` sentinel trap／破坏：

| Fixture | 操作 | 子类 | 状态 |
|---------|-------|-----------|--------|
| `compound_eq`（C-015） | 对 record/tuple key 执行 `set.from_list`/`set.insert`/`map.from_list`/`map.insert` | heap element/key RC | **set.from_list 已修复**（stdout 现已一致）；exit 仍为 134（set.insert/map 操作仍待处理） |
| `list_float_total_order` | 对 `List[R]` 执行 `list.sort_by` | heap element RC | 未解决 |
| `alias_cow` | `var b = a; a[0]=…` list COW | list BACKING／COW RC | 未解决（所有值正确，仅 main 退出清理时 double-free） |
| `list_count_index_truncation` | `list.take/drop/slice` 的巨大 index（2³²…） | 数值（可能 u32 index 截断 → OOB → 破坏），可能与 frees 无关 | 未解决／分类中 |

<a id="the-fix-tool--why-a-piecemeal-drain-corrupts-attempted--reverted-2026-06-08"></a>

### 修复工具，以及为何零碎清理会破坏内存（2026-06-08 尝试后撤回）
`emit_elem_copy_owned(ty)`＝`emit_elem_copy` 加上 heap 元素的 stack-neutral `rc_inc`（`i32_load; call rc_inc; i32_store`），**仅用于 SHARE 点**（从仍存活的源向外 copy），不用于结构内 MOVE 点（grow 并放弃旧结构，额外 Inc 会 leak）。对 `set.from_list`（`calls_set.rs` 的 append-from-xs）应用后，确实修复了 `compound_eq` 的 stdout 值（set.contains(record) F→T），且 corpus 仍为 240/0。**但是** `set.from_list` + `set.insert` churn loop 揭示零碎修复的陷阱：10 次时 native `sink=30`，而 **wasm `sink=31`**（一个 set 的去重长度错误），随后 **1.5M 次时 OOB**。set 操作有多个 SHARE/MOVE 点，只修部分会残留 double-free，复用已释放内存，破坏*下一次*操作去重依赖的元素相等性，导致错误长度，最终 OOB。**教训：必须原子地修完一个结构的全部 SHARE/MOVE 点，并用每结构 CHURN loop（1M+ 次）把关，而不只看 stdout/corpus。** 零碎改动已**撤回**到验证过的良好状态（240/0 + records churn 7.43 MB）。工具／方法没错，错的是*增量单位*。

<a id="the-remaining-collection-drain-mechanical-but-per-site-sharemove-judgement"></a>

### 剩余 collection 清理（机械化，但每点需判断 SHARE/MOVE）
`calls_set.rs`（行 240,310,355,392,470,536,609，insert/union/intersect/diff/…）和 `calls_map.rs` 中有约 15–20 个 `emit_elem_copy` / `emit_elem_copy_sized` 点。dict path：`emit_dict_put_entry` + 行 102/510/519 的 `emit_elem_copy_sized`。注意 `emit_elem_copy_sized` 接收 SIZE 而非 Ty，需要传递是否为 heap，或增加以 `key_ty`／val-ty 为依据的 owned variant。还有 `list.sort_by`（`calls_list_closure.rs:941`）和 slice/take/drop 群。每处先把 copy 分类为 SHARE（源存活 → owned）或 MOVE（源放弃 → plain），再替换。其 leak gate 是 **cargo gate／2M churn**，不是 test corpus；每批之后都验证两者。

<a id="cow-alias_cow-is-its-own-sub-problem"></a>

### COW（`alias_cow`）是独立子问题
`var b = a`（list alias）+ `a[0]=…`（copy-on-write）：所有值已字节一致，只有 main 退出清理会 double-free COW 后的 backing。问题位于 `AliasCowPass` 及 IndexAssign／var-alias RC，而非 element-copy helper。需单独谨慎修复。

<a id="remaining-work"></a>

## 剩余工作

1. **Compound set/map key**（`compound_eq`、C-015）。`set.from_list` / `map.from_list` 按引用将 record/tuple 元素 move 到结构，输入 list 的作用域末尾 Dec 随后深度释放移入的元素（以及去重掉的 duplicate）。与 #2 相同的 move 纪律缺口，位于 set/map runtime（`calls_set.rs`、`calls_map.rs`）。在那里应用 dup-on-store／move 排除。
2. **运行完整 cargo gate**，清理每个分歧 fixture（test corpus 按其构造就会漏掉这些形状）。
3. **Verifier 不可见性（后续，对 belt 至关重要）：** #2/#3/#5 在 emitter 内发出 `rc_inc`，不是 IR `RcInc` node，perceus-belt verifier **看不到**。目前 churn benchmark 是它们唯一的 leak gate。扩展 verifier，对 constructor／builder／HOF dup 建模（将 `value_object` / `Record` / `fold` / …视为会 dup alias 参数），使其获得*证书验证*，而不只运行时检查。此前 belt 的“证明 RC 平衡”主张仍有缺口。
4. **重新落地** M1（rc guard 切换）+ M2（TCO flatten + `tco_managed_params`）+ sentinel guard + #1–#5 + compound 修复，作为一个一致的 commit，且须在双 gate 与 churn 都 green 之后。

<a id="trap-log-environment"></a>

## 陷阱日志（环境）

- `almide test spec/ --target wasm` 在 wasmtime 上运行每个 test block；cargo `wasm_runtime_cross_target` 是字节一致 contract gate。**单独任何一个都不足，必须两个。**
- sentinel guard 将 double-free 挂起变为快速 trap，corpus run 因此会结束而不空转。
- **孤儿 `rustc` 风险：** cargo gate 通过 `rustc` 编译每个 fixture 的 native 侧。此前 session 留下 3 天前的孤儿 `rustc`（PPID=1），在共享 build scratch `/tmp/claude-501/almide-build/` 以 99% CPU 空转，令 gate 饥饿 47h。gate 计时前检查 `pgrep rustc`，kill 过期 PPID=1 rustc。用 `perl -e 'alarm N'` 包装 `cargo test` 会 kill cargo，却让 test binary 脱离并继续运行；应改为后台运行 gate。

<a id="2026-06-1011--third-campaign-full-drain-under-the-flag"></a>

## 2026-06-10/11 — 第三轮：flag 下全部清理

分支 `true-perceus`。启用现在受 emit 时的环境变量控制（`ALMIDE_WASM_FREES=1`），**flag 下四重标准全部 green**：native corpus 264/264 · wasm corpus（flag off）264/264 · wasm corpus（flag ON）264/264 · cargo byte gate（flag ON）67/67 · 2M 次 record churn 正确，RSS 平坦（13.3 MB ≈ leak-mode baseline）。

全部 14 个 frees-ON 分歧已清理（机制 → 修复）：
- record typed-drop glue 按名字排序 field，而 layout 按声明顺序 → 去掉排序（2 行）。
- heap_restore 现在也重置 free list；rc_inc/rc_dec 增加 dead-zone guard（ptr ≥ heap_ptr）。
- double-free sentinel（标记 rc=0 → 第二次 dec trap）+ rc_inc resurrection trap + 绝对遍历上限（1M step）。
- string.join len==1 原样返回元素指针 → 条件 inc。
- tuple payload 的 variant constructor store → emit_stored_field。
- 机制 #6：insert_rc_ops 中 return-alias dup（callee 侧 owned return）；unwrap_or runtime call 分类为 alias。
- **顺序修复**：Block 值上的 VDecl alias-Inc 现在 hoist 到 block 内（tail bind 后、temp dec 前）。过晚的 Inc 会复活 temp typed dec 已释放的 payload。
- frees 下停用 emitter-level ownership（is_single_use_var → false）：in-place list 复用／raw rc_dec 与 IR dec 二重拥有 block。
- list 群：15 处 emit_elem_copy→owned，以及 min/max、get-box、remove_at OOB inc、set/insert/push stored-field、sort 构建后 dup walk、filter loop 后 dup、enumerate、update 替换值 release；list.repeat 每 slot inc（for+concat-push → repeat rewrite 暴露了它）。
- concat：call-site 对 fresh result 元素进行 SHARE dup walk。
- map 群：emit_elem_copy_sized(size, dup)，dict_recap dup Option<(K,V)>（set/merge=Some，grow=None），remove survivor dup，update 全 table dup 与 old-value capture-release／not-found input inc，map.map key inc。
- json/Value：value_get／get_typed(get_string|get_array)／as_type(tag 4|5) 的 interior-pointer inc；set_path kept-key／unchanged-pair／path-key／no-op cur_built／unchanged-elem inc；remove_path 7 个 no-op alias return 和 survivor 点；set_path new_val 经 stored-field。
- emit_elem_copy_owned 由 is_heap_type 控制（排除 Int32/UInt32/Float32 的 4-byte scalar 陷阱）。
- 移除 free-list size-sanity bound（对合法已释放 node 误报；cap／sentinel／resurrection trap 覆盖相关类别）。

default-ON 前剩余：Stage C（TCO loop 回收，M2 重落地；目前 loop 每次 iteration leak，有界），已提交 churn fixture 群（每结构 1M+ gate），Stage D PIN（从构造保证 fs scratch）+ C-042 解锁，flag-ON 完整标准连续 3 次，然后默认切换 + C-041 修订 + 新回收 contract + perf suite（Stage G）。

<a id="stage-c-finding-2026-06-11-the-b8bbdfad-m2-cannot-be-cherry-picked"></a>

## Stage C 发现（2026-06-11）：b8bbdfad 的 M2 无法直接 cherry-pick

在 Round-3 accounting 上重落地 b8bbdfad 的 flatten_exit_tail_blocks 与 pass_tco managed-param protocol，导致新受理形状 `spec/churn/tco_loop_churn.almd` 在 `__alloc` 内 OOB（free-list `next` 被破坏）。loop-VARIANT TCO 参数阻止 LICM，而旧 probe 是 loop-invariant，被无声 hoist。M2 前相同形状 green（param 每次 iteration leak，有界、安全侧）。根因类别：b8bbdfad 的 pass_perceus diff 共 93 行，其中 flatten 约 80 行；其余 loop exit 周边 Dec 放置规则与旧 alias accounting 共同设计，与 Round-3 的 alias-Inc hoist、#6、作用域末尾规则冲突。部分移植 M2 在结构上不健全。

决定：Stage C 要针对 Round-3 规则**重新设计**逐 iteration TCO 回收，用 `scripts/check-frees-churn.sh` 把关（tco_loop_churn 为验收 test）。暂存 diff：/tmp/m2park（含 flatten 移植的 pass_tco.rs、pass_perceus.rs）。这不阻塞 default-ON：TCO loop 每 iteration leak（原有行为），正确性不受影响，non-TCO churn 已为 O(1)（record_churn）。

<a id="2026-06-11--the-flip-v0270-frees-are-the-default"></a>

## 2026-06-11 — 切换（v0.27.0）：释放成为默认

`wasm_frees_enabled()` 默认 TRUE；`ALMIDE_WASM_FREES=0` 为 opt-out 退路。切换过程证据：

- tree 不作改动，三重标准连续 3 次全部 green：native 264/264 · wasm default（frees）264/264 · wasm opt-out 264/264 · byte gate 67/67 · churn gate（record 2M + tco 200k）· cargo full suite 失败 0。
- Stage G micro-perf：ON 与 OFF 在噪声范围内（string 0.022/0.021 s，list 0.033/0.034 s，map 0.011/0.011 s，wasmtime，3 次中最好值）。
- contract：修订 C-041，添加 C-066（`spec/wasm_cross/rc_reclaim_churn.almd`）。

切换又暴露并修复一类问题：每 iteration 的 LOOP REGION reset（iter_scope，泄漏时代为不逃逸 loop body 提供的回收）回退 bump pointer，却不回退 FREE LIST。已释放 node 位于恢复 frontier 以上，下一次 iteration 的 bump 重新分配 free list 仍引用的地址（表现为 `string.len` 读到 free-list next pointer；写出新 C-066 fixture 后几分钟就发现）。region reset 现在也清空 free list：region invariant“无内容逃逸 iteration”必须覆盖 ALLOCATOR STATE，而不只是值。frees 下 reset 保持启用，它还掩盖回收 loop 内已知的 stored-field 过多计数（2M churn 恢复为平坦约 13 MB；禁用 reset 后，过多计数在 2M iteration 内泄漏约 61 MB，这是剩余 Koka 精度工作的测量规模，不是回退）。

<a id="stage-c-complete-2026-06-11-per-iteration-tco-reclamation"></a>

## Stage C 完成（2026-06-11）：逐 iteration TCO 回收

b8bbdfad M2 已干净地重落地。旧 free-list 破坏从来不是 M2 的错：loop REGION reset 每次 iteration 回退 bump pointer 却不清空 free list（已在 v0.27.0 切换修复），M2 的 Dec 是首个在 region loop 内向该 list 投入内容的机制。在 Round-3 accounting 上重新应用，并作三点细化：

1. `is_fresh_alloc` 扩展到返回所有权的 CALL 参数：user fn 返回 owned（#6）；TCO rewrite 把参数变为 VDecl 后，alias 形状 temp 获得 bind-level Inc，因此 call 值的 self-call 参数可安全接管，其 param 受管理。仅检查 literal 会让 `spin(n-1, string.take(acc + "x", 8))` 不受管理，即无界。
2. inner-hoist alias-Inc 只在 tail temp **自身值**为 alias 时触发。从 block 移出的 fresh tail 拥有自己，无条件 +1 在 TCO loop 中测得 16 B/iteration 泄漏。
3. 该细化移除了一个关键过量 inc，暴露下方真正缺口（历史调查预测的类别）：`?? default` 可作为 `Call { option.unwrap_or }` 到达 Perceus，而 alias 分类器只认识 RuntimeCall 写法，因此 Call 形式被当作 FRESH，map 拥有的 payload 在 teardown 时 double-free（byte gate 数分钟内在 map_insertion_order 捕获）。unwrap_or/get_or 的 Call{Module} 写法现在也分类为 alias。

测量：2M 深度的定大小 TCO 递归，4.27 GB → **平坦 7.3 MB**（新 gate fixture `spec/churn/tco_deep_recursion_churn.almd`）；200k loop-variant TCO churn green；全部标准 green（corpus 3 mode、byte gate 67/67、churn、cargo）。已记录但未修复的已知限制：单调增长 accumulator（完整保留 `acc + "x"`）的 peak 仍是总和；每个已释放 block 比下一次请求小一个单位，first-fit 无法复用。这是 allocator policy（capacity growth／size class）前沿，不是 ownership bug。C-066 的有限例外列表缩小到回收 region 外的 construct-from-temp 过多计数。

<a id="stage-d-complete-2026-06-11-fs-scratch-pinned--the-real-c-042-root"></a>

## Stage D 完成（2026-06-11）：fs scratch pin 与 C-042 真正根因

准备 PIN 时发现，多 fs 操作 trap 的真因从来不只 free-list lifetime。free_list 占用 index 1 后，`register_runtime` 仍保留 PRE-FREES global 编号（`preopen_table_global = 1`，count=2）。**`__init_preopen_dirs` 在启动时把 preopen TABLE POINTER 写入 FREE LIST head**，因此 `__alloc` 的无条件遍历把 table 当成 free node，影响每个使用 fs 的程序，持续数月的每个 release。由于 `__resolve_path` 通过同一过期 index 读回 table，足够自洽而通过 2-op fixture。v0.26.21 的纯 leak mode 下，`fs.exists` 2 次即可复现。

已落地：index 修正（一对常量）+ `__alloc_pinned` / `PINNED_RC`（所有 host 写入的 WASI buffer，包括 fd_out/stat/iov/nread/data scratch、preopen table、resolve result，从构造上永生；rc_inc 和 rc_dec 遇 sentinel 提前返回）+ C-007 init-order assertion。C-042 fixture 现执行 6 次 fs 操作，中间穿插 allocation churn，字节一致。
