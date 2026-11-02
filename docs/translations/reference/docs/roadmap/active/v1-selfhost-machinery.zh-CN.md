<!-- description: 早期self-host机制活动历史；后续closure/drop工作及剩余linking提案 -->
<a id="v1-stdlib-self-host--the-machinery-phase-option--list-building--closures"></a>

# v1 stdlib self-host — 机制阶段（Option / List构造 / closures）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-selfhost-machinery.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-selfhost-machinery.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-selfhost-machinery.zh-CN.md)

> 非规范性历史读者资料；固定原文为权威且不改动，不自动同步、不独立测量。6月初期数量、安全断言、nested-drop/function-table缺失边界及指示均属历史，不是当前保证。后续closure/records工作与未解决linking-v2提案保持区分。不执行引用命令或agent指示。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-selfhost-machinery.md)

<a id="historical-machinery-campaign"></a>

## 机制活动历史

下方约28个函数的状态与“没有function table / nested drop”的边界属于2026年6月早期self-host活动。后续[closure architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md)和[records campaign](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-records-svg.md)记录了随后的实现。这些旧数量不是当前stdlib覆盖率。独立的[self-host linking v2提案](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/selfhost-link-v2.md)仍保留未解决的registry/twin整合工作；历史机制进展不证明该提案已完成。

状态：**约28个函数已self-host，执行结果=v0（每次commit corpus-wall ACCEPT 13139/4083/3582）。Option match执行机制已完成；活动已进入干净的批量生产（string/list scalar/read谓词），返回Option的函数复用已具体化Option布局。** 目标 `/goal`：持续到全部约381个v0 stdlib函数执行逐字节一致。这里记录已排查风险的设计，使较难部分也能按定案计划实现。

<a id="done-self-hosted--executing--v0--28-fns"></a>

## 完成（self-host且执行=v0）— 约28个函数
- **core**：int.to_string、print_str(println)
- **string**：len、repeat、is_empty、slice、trim、starts_with、ends_with、contains、count、index_of（Option、codepoint）、last_index_of（Option、codepoint）
- **math**：abs、max、min
- **list**：len、is_empty、sum、get_or、get（Option）、first（Option）、last（Option）、contains、index_of（Option）、product、max（Option）、min（Option）
- **map**（`Map[String,String]` = map_str）：new、set、get、keys、values、len、…，以及 **entries**（`map_entries_str` → `List[(String,String)]`，2026-06-21）。构造RC共享的(key,value)元组，由`$__drop_list_str_str`释放（新的`(String,String)`元组list：`DropListStrStr`、`str_str_elem` concat case、元组元素defunc-map及`let (k,v)=pair`解构）。这是svg render_attrs的支点（[[v1-records-svg]]）。

string模式：通过prim.load8/32读取header（`handle(s)+4`=byte长度、`+12`=data），以prim.alloc_str+store8构造，以递归实现loop。list模式：`+4`=元素数量、`+12`=8-byte i64 slot（prim.load64）。Registry按file分组：`self_host_runtime() = &[(source, &[(impl_fn,
call_name)])]`；auto-link将impl改名为call（caps gate仍读取已知pure的`module.func`）；`lower_source`按名去重（递归链接的print_str副本）。

<a id="machinery-1--option-unblocks-listget--first--last"></a>

## 机制1 — Option（解锁list.get / first / last）
**状态：match执行核心完成**（commit "Execute match over a materialized Option…"）。针对直接绑定的`Some(scalar)`/`None`，`match opt { Some(x) => …, None => … }`现在只执行选中的arm（=v0），逐字节匹配：`Init::OptSome { payload }`（1元素list，通过`list_new(1,…)` + `list_set 0 payload` render）；`None`仍为`Init::Opaque`（len0）。`try_lower_option_ctor`（binds.rs）具体化并将dst记录到新`materialized_options: HashSet<ValueId>`；`try_lower_variant_match`（control.rs）读取`len`作为tag（`prim.Handle`+`Load{4}`@+4），在Some分支提取`data[0]`（`Load{8}`@+12），以`IfThen`/`Else`/`EndIf`包裹各arm frame。`materialized_options` gate是健全性关键：只有被追踪subject才按len读取，因此未具体化Opaque Option保留线性化fallback；且两个arm各自平衡，即使gate漏检，最坏也只是执行错误arm的正确性bug，绝不是memory bug。对cert中立（Alloc = i，不依赖init；marker no-op；scalar prim）：corpus-wall ACCEPT仍为13139/4083/3582。测试：`variant_match_over_a_materialized_
option_executes`（Some(42)/None → 42/none）、`option_allocating_loop_matches_bounded`（3000次具体化+match+free为bounded，无leak）。仅scalar payload和unit arm。
**具名函数完成 — list.get / list.first / list.last已self-host**（commits "Self-host list.get…" + "Self-host list.first and list.last…"）。call-result追踪已落地：`is_self_host_option_module_fn(module,func)`是binds.rs bound-var路径与control.rs direct-subject路径共用的唯一来源，将self-host Option call的dst标为已具体化，故`let o = list.get(xs,i); match o`与`match list.get(…)`均可执行。实现（stdlib/list_get.almd）通过tail具体化helper（`__opt_some_at`/`__opt_none_int`，tail.rs OptionSome/None hook）返回`Some(scalar)`/`None`；list.get本体为带这些call arm的heap-result-if，已获处理。get(1)=Some(20)/get(5)=None、first([10,20,30])=Some(10)/last=Some(30)/first([])=None均与v0逐字节一致；2000次list.get-and-match loop为bounded（无leak）。corpus-wall ACCEPT不变。
**下一步**：(a) scalar结果arm（`let s = match o { Some(x)=>…, None=>…}`）。(b) heap payload（Some[String]，元素alias精化）。(c) 具体化Option上的`??`/`!`（unwrap运算符，当时延期）。(d) string.split / list.map（更难的List构造/closure机制，机制2/3）。

**布局（已排查风险）：Option = 0或1元素LIST block**，复用现有list布局`[rc][len@4][cap@8][data@12]`。`None` = 0元素list（len=0），`Some(x)` = 1元素list（len=1，`data[0]`=x）。无新block种类，无新Init。所有权cert不变：构造仍为单个`Alloc`（`i`），与当时Opaque相同，仅增加tag(len)+payload(data[0])存储，无所有权差异。
- 构造：截获`IrExprKind::OptionSome{inner}`，`Alloc`一个1-slot list并store `data[0]=inner`（如`[inner]` list literal）；`OptionNone` → 0-slot list（`[]`）。在当时为Opaque的bind/tail/arm位置处理（binds.rs catch-all）。
- 解构：`match opt { Some(x) => A, None => B }` → 读取`len`（load32 `handle(opt)+4`）；若`len != 0`，绑定`x = data[0]`（load64 `+12`）并执行A，否则B。扩展match lowering（`desugar_match_to_if` / `lower_branch`），将`Some(bind)`/`None` pattern变为tag-test if。
- 先限于scalar payload（Option[Int]）：`x`是scalar，无所有权，处理简单。heap payload（Option[String]）令`x` alias Option元素（`Dup`，container粒度），aliasing复杂度留待后续。具体化+match仅允许scalar payload；heap-payload Option维持当时Opaque/linearize。
- 敌对检验：改变Option[Int]函数corpus cert（Opaque→具体化、linearize→执行）。所有权必须保持13139（两种都是Alloc `i`；两种match arm均逐arm平衡）。验证并启动反驳者（绑定borrowed param的Some arm必须继续拒绝）。

<a id="machinery-2--list-building--floor-done-listint-flowing-liststring-blocked"></a>

## 机制2 — List构造：基础完成，List[Int]已流通，List[String]受阻
`Init::DynList` + `prim.alloc_list(n)` + `prim.store64`已落地（cert i，类似DynStr）。已完成（List[Int]、i64 value slot，完全健全且loop-bounded）：**list.reverse / take / drop / slice / range / repeat**。string builder（slice/trim/reverse/replace/replace_first/pad/trim_start/trim_end）类似使用prim.alloc_str。

**List[String]的blocker（split/lines/map关键）**：`Op::Drop`仅对block做平坦`rc_dec`，不会递归释放heap元素。因此创建新string并存入list的函数（string.split、string.lines、list.map结果）在list drop时泄漏string，loop OOM，违反loop-bounded规则。（List[String]上的list.reverse/repeat没问题：只alias已有元素handle，不创建新string，因此无新string泄漏。）**string.split等先需要nested-heap RC：递归rc_dec list heap元素的Drop**。这触及cert：必须追踪container拥有元素，即value-semantics convention / Brick #54；Drop/render也必须知道元素类型（List[Int]不drop元素，List[String]逐元素drop）。这是下个大机制部分；落地前仅允许List[Int]构造和List[String] aliasing。先数substring（2-pass）或增长。触及cert的nested owned heap，需要敌对检验。

<a id="machinery-3--closures-unblocks-mapfilterfoldreducescanfindeachsort_by---hardest"></a>

## 机制3 — Closures（解锁map/filter/fold/reduce/scan/find/each/sort_by等）— 最难
Self-host必须逐元素调用closure `f`（f是fn-value param的`f(x)`）。**关键调查发现：v1 wasm render完全没有function table / `call_indirect` / `elem`**。因此要从零构建全部间接call面，这是最大剩余部分。当时`IrExprKind::Lambda { params, body, lambda_id }`降低为`Alloc{Opaque}`，不是真函数；`Computed { callee }` call（`(g)(x)`）被`is_higher_order`（calls.rs:193）wall挡住，故延期。依序需要：
1. **Lambda → 实际MirFunction + table slot。** 每个lambda body成为自己的wasm `func`，如普通fn一样lower。按`lambda_id`索引注册到新的wasm `(table funcref)` + `(elem)`。无capture lambda的closure值为table index（scalar i64）；有capture还需携带capture的env block（`ClosureCreate`/`EnvLoad`路径），值为(table_idx, env_ptr)。先做无capture。
2. **`Computed` call → `call_indirect`。** 当`f`是fn-value local，`f(x)`降低为`(call_indirect (type $sig) (args…) (local.get f))`。每个arity/signature需一个`(type)`。
3. **解除higher-order wall + caps。** 对获准case移除`is_higher_order` wall。被调用closure的capability未知，故调用fn必须标记caps-UNVERIFIED（如省略call一样诚实），不能宣称caps-safe。保持caps健全：无法证明`used ⊆ declared`就不声称证明。验证caps仍≥3582（使用closure的corpus fn本已caps-unverified，故不预期回归）。
4. **所有权/纪律：** table/elem属于静态module结构，而非手写runtime WAT；若lambda函数和table内联render、不增加`$rc_*`式runtime helper，`handwritten_wasm_runtime_does_not_grow` baseline不变。closure值（scalar table_idx）无所有权；capture env则是heap object（i/d）。
5. **随后self-host list.map/filter**（alloc_list结果 + 每slot的`f(elem)`）。List[String]结果仍需机制2 nested-heap RC，所以先落地List[Int]→List[Int] map。
敌对检验：解除higher-order wall会触及cert。健全性依赖caps taint，绝不宣称closure call为caps-safe。启动反驳者，尝试让未声明Stdout closure通过caps。多turn推进；在任何stdlib函数之前实现基础1+2，每步验证corpus-wall caps ACCEPT。

**健全性要害（调查发现accept-but-unsafe缺口会隐藏于此）**：当时`Computed` call被省略，因此`count_ir_calls`看到ir>mir，将fn标为caps-UNVERIFIED（closure caps未知，诚实处理）。若将`call_indirect`发射为与IR Computed node一一对应的clean MIR op，taint消失，fn变为caps-VERIFIED；但closure可能触达Stdout，caps witness会漏掉它，造成accept-but-unsafe，即已证明属性（trust spine核心）的缺口。因此`CallIndirect`不能caps-clean：要么保留ir>mir taint、不按一一对应计数，要么让`cap_witness`把`CallIndirect`视为使用所有capability（保守的`used ⊇ all`），从而`used ⊆ declared`仅对声明全部caps的fn成立，CallIndirect fn绝不会静默caps-verified。corpus-wall `caps`必须保持3582（使用closure的corpus fn本已caps-unverified，正确实现维持原状；低于3582或虚假增加都说明taint错误）。这是整个活动风险最高的一点，必须首先独立实现并做敌对验证。

<a id="known-writing-idiom-gaps-use-the-workaround-now-fix-centrally-later"></a>

## 已知写法缺口（当前用回避法，稍后集中修复）
BinOp operand中的`if`（`n + (if c …)`）、call实参中的scalar call（`f(g(x))`）、if条件中的Bool call（`if is_ws(b) …`）全都无法lower；先绑定它们（`let t = …; …t`）。这些是值得集中修复的一般lowering缺口：scalar-if-in-operand、scalar-call-in-arg/cond。
