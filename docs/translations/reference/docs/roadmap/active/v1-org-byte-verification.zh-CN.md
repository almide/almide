说明：组织范围 v0==wasm 字节验证扫描及其暴露的 wasm 缺陷类别
<a id="org-byte-verification--every-repos-own-vectors-on-both-targets"></a>

# 组织字节验证 — 在两种目标上运行各仓库自身的向量

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](v1-org-byte-verification.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-org-byte-verification.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](v1-org-byte-verification.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-org-byte-verification.md)

> 原文注意事项：这是 7 月 2–15 日的扫描记录，不是新的组织全面验证。开头方法区分独立通过套件与显式字节比较；BYTE_VERIFIED 标签及旧标题不取消该区别。production emit_wasm、MIR/render_program、native-host/FFI 例外、browser-host 构建、per-file 分类、resolved-strict 测量范围不同。早期未解决项、后续修复和陈旧余项表并存（含 grammar/TCO 及重复的第六／七轮），不将其重新协调为当前状态。零 wall 是所述 lowering／分类结果，不是普遍运行正确性或无泄漏。有限测试数、泄漏循环、kernel 检查和 native oracle 比较保留各自边界。host 环境继承及 snapshot 行为是历史实现细节，不是执行授权。保留原文数字而不修正，包括最后称 3 个缺口却列两项具名条目及一项前轮结转的记载。

会话记录（2026-07-02，续 `v1-porta-read-message-handoff.md`）。目标为交接步骤 1–3：解除 porta 的 native 构建阻塞，然后在整个组织将验证从 `wall=0`（能 lowering）扩大为**仓库自身的测试向量在 native 和 `--target wasm` 上按字节一致运行**。

<a id="method"></a>

## 方法

对每个有测试的组织仓库：`almide test --target native` 和 `almide test --target wasm` 必须都通过所有测试。仓库自身套件的断言就是向量。两目标通过只确立这些断言，不确立任意 stdout/stderr/exit 字节相等。仅在记录明确比较这些观测的地方主张字节一致，并与两边独立通过套件区分。扫描脚本模式记在本会话历史中；没有测试的仓库（almide-web、almide-sqlite）不能用此法验证，仍为 🟡。

<a id="porta-native-build-handoff-step-1--fixed-52--0-errors"></a>

## porta native 构建（交接步骤 1）— 已修复，52 → 0 错误

交接把 porta native 阻塞归因于“toml-dep borrow/clone codegen”。实际分解为：

1. **22× E0308 双重包裹** — ResultPropagation Phase 2b（81840f8d）对调用声明为 Result 的 effect fn 的 match-tail arm 包 Ok（从未 sig-lift，故不在 `lifted_fns`，但已有 Result 类型）。修复：类型已为 Result 的 tail 绝不再包裹（`b03d71e7`）。
2. **28× E0425 extern-fn 不匹配** — module `@extern(rs, ...)` fn 输出 `use bridge::f as f;`（裸名），而调用处输出 flatten 前缀 `almide_rt_<mod>_<f>`。修复：alias 带前缀名（`71b22b08`）。
3. **Capability E0425 + CapabilitySet E0308** — cherry-pick develop 侧 #697/#698/#699 修复（loop-body Bind ty mangle、TCO shared-mut、TCO 预制 owned param）。
4. auto-? skip-set 漏掉 `ok(match parsed { ok/err })`（value wrapper 后面的 match）以及顶层以下嵌套的所有 Bind。通过每个 Bind 深度上的穷尽 visitor 扫描修复（`d5794a86`），同时应用于 checker（infer_p5）与 lowering（auto_try）。

回归 harness：3 个新 crossmod-matrix cell + module-extern native gate（`6d6adf05`）。porta：native 构建干净，`almide test` 8/8，wasm 侧 7/7（+1 个 FFI 文件按设计跳过）。

<a id="the-wasm-bug-classes-the-sweep-flushed-out-all-pre-existing-also-on-develop"></a>

## 扫描暴露的 wasm 缺陷类别（均为既有，develop 上也有）

每项均由仓库套件在 wasm 上 trap／产生差异发现，最小化为纯 stdlib 复现，定位根因、修复，并用 `spec/wasm_cross` fixture + contract 固定：

| 修复 | 类别 | contract |
|-----|-------|----------|
| `eb0a0fc3` | string 直通 fast path（replace/replace_first/pad_start/pad_end/capitalize）未 +1 就返回输入，使无匹配环节组成的 pipe chain 的 rc 下溢（svg escape_attr trap） | C-121 |
| `b78fda19` | record spread 未 +1 就字节复制 heap field，override 绕过 `emit_stored_field`（svg doc 丢失 attrs Map） | C-123 |
| `b17593d2` | value.merge/pick/omit/json.keys 分配 pair list 少 4 字节（无 cap word），cap 未初始化，借用复制 pair/key；value.get/field 的 ok payload 借用（toml aot 静默丢 field） | C-122 |
| `d4de9c5e` | `Value == Value` 比较指针（无 deep-eq runtime），使 `json.get(f,"import") ?? json.null() != json.null()` 把每个 fn 错分为 JS import（almide-wasm-bindgen）；另有 as_array 的 ok payload 借用 | C-124 |
| `9e5927aa` | value.merge 丢 a 的 key 位置，非 object 处理与 native oracle 不同；重写为保持位置，移除两项陈旧 `@xt-allow` 差异（value_eq、value_merge） | C-103 |
| `86480293` | `bytes.set` 无条件原地存储（oracle 会 clone），经 param 的 set 覆盖调用者 buffer（aes cfb8 NIST 向量错误）；现为值语义，带由 AliasCowPass 行使否决权的 `x = bytes.set(x, …)` 原地 fast path | C-125 |

另修复 v1 文件拆分带来的 rt-oracle registry 漂移（65 项重指向，`f121b1ff`），gate 绿色：verified 137/137，grandfathered=0。

<a id="result"></a>

## 结果

所有有测试套件的组织仓库均通过两目标：yaml、sha1、toml、svg、rsa、porta、csv、bigint、base64、aes、almide-wasm-bindgen、almide-lander、almide-grammar。`scripts/org-trust-status.sh` 的 `BYTE_VERIFIED` 与 dashboard 记录新状态。排除：almide-web / almide-sqlite（无测试，先需向量）、almide-dojo（task-bank fixture，非可编译套件）、almide-bindgen（见 dashboard）。

<a id="graphics--ai-stack-spot-check-same-day-follow-up"></a>

## Graphics / AI stack 抽查（同日后续）

记于 dashboard 的“Graphics / AI stack”：svg/lumen/homullus 已 byte-verified（套件，两目标）；canvas/wasm-canvas/wasm-webgl/obsid wasm 构建干净（browser-hosted，headless run 不适用）。发现 3 项编译器问题：

- **almide-aituber**：develop-v1 的 wasm emit 结构验证失败（`type mismatch: expected i32, found i64`），但 develop v0.27.13 构建干净。这是唯一发现的 v1 对 develop 差异，早于 2026-07-02 会话（59dfd762 复现），需 v1 branch bisect。
- **almai**：两 branch 均出现 `[COMPILER BUG] unresolvable bare type name(s) reached codegen`。8 个 provider module 定义同名类型，裸 ref 无法解析（#433 类）。
- **nn**：两 branch 均有未解析 `__tco_tmp_data`（ty=Unknown）。TCO temp 缺类型解析，构建诚实拒绝。

<a id="second-pass--the-v0-leftovers-rescued-onto-v1-2026-07-02-same-day"></a>

## 第二轮 — 将 v0 遗留项救到 v1（2026-07-02，同日）

指令：不回移 v0，让 v1 成为这些项目能工作的 branch。

- **nn：0/13 → 13/13，两目标**（现列入 `BYTE_VERIFIED`）。5 项编译器修复，均先最小化失败形状：(1) TCO borrow-preserved Bytes temp 携带真实类型；Rust 侧“不注解”意图移到 `codegen_annotations.infer_binding_tys`（旧 `Ty::Unknown` 偷渡被 ConcretizeTypes gate 正确拒绝）。(2) #653 lambda-param pin 不再把 callee 自身未绑定 generic 写进 param（nested-HOF 元素类型静默默认为 Int — C-126），但作用域内 rigid generic（`names[T: Labelled]`）仍 pin。(3) `AlmideMatrix` 增加 `AlmideRepr` impl（constructor 形式，沿用 Set 先例）。(4) SIMD fast-exp（avx2/neon/simd128）把输入钳到 ±708。softmax `-1e9` mask 使 `(k+1023) << 52` exponent bit trick 回绕，破坏整行（masked attention 返回错误 NIST 向量）。(5) chain 类型未解析时，`unwrap_or` 从 default 参数确定 payload 大小（`list.find |> option.map |> option.unwrap_or` 发出含 `i64.const` arm 的 `if (result i32)` block — C-127）。
- **almide-aituber：已修复** — v1 对 develop 差异源于缺 develop 侧 #717（auto-? 解包 effect branch 后重算 if/match/block 类型），已 cherry-pick。
- **almai：关闭（2026-07-03）**。nominal/structural 分叉通过 STRUCTURAL-TWIN merge 解决：checker 确实跨 module unify 同 base-name、同 shape record 声明（某处落在哪个 nominal name 上取决于 constraint 顺序），因此 codegen 实现该语义。flatten pass 按（base name、shape fingerprint）分组声明，把全部 twin 映到一个 canonical struct；bare-ref repair 接受全 twin owner 集合。同名但不同 shape 类型保留不同 mangle（checker 已在单 package 内以 E020 拒绝）。almai native suite：56 测试绿色。guard cell：`structural_twin_records_flow_both_directions`。
  （保留被取代叙述供记录：）E0063 类已修复（flatten mangle 现 remap 名称索引的 codegen annotation — default/boxed field、ctor_to_enum — 使 module 类型 field default 填入）。约 37 个剩余错误是根本 `LLMResponse`/`ToolCall` 对 `openai.LLMResponse` 等 NOMINAL/STRUCTURAL 分叉：checker 接受跨 module 同 shape record，codegen 却发出不同 Rust struct。解决需要语言决策（check 拒绝或插入转换），develop 也红。
- 调试辅助：结构验证失败时，`ALMIDE_DUMP_INVALID_WASM=<path>` 输出无效 module（保留 name section），让 `wasm-tools
  validate/print` 能命名出错函数。

<a id="third-pass--the-last-two-known-leftovers-2026-07-03"></a>

## 第三轮 — 最后两项已知遗留（2026-07-03）

- **svg 跨 module render stack overflow：已修复**。溢出发生在编译器：`unify_structural` 对不同名 nominal pair（`El` 与 module twin `lib.El`）展开双方并递归 field；递归类型在 `children: List[El]` 中再次遇同 pair，永远循环。等递归 guard：checker 内维护处理中 pair 集合，重新遇见时按余归纳 unify。svg 现在两目标 render 字节一致。guard cell：`recursive_record_type_cross_module`。调试辅助：`ALMIDE_TRACE_PASSES=1` 在每个 pass 运行前报名字。
- **跨 module `@inline_rust`：两侧均修复**。wasm 把每个 `@inline_rust` fn 作为 dispatch-only 跳过，但 user package fn 有真实 Almide body 作为 portable 实现（attr 只是 native 优化）。现仅跳过 Hole body 声明，wasm 编译其 body。native 将 template 裸 struct token（`Cfb8State { .. }`）贴入 mangle 后环境（E0422）。StdlibLowering 现把 package template 自身类型 token 重新限定为 canonical dotted name，flatten pass 将 template 内 dotted token 改为 flat struct 名。aes cfb8 NIST F.3.7 跨 module 通过，两目标字节一致。guard cell：`inline_rust_with_fallback_body_cross_module`。

<a id="fourth-pass--the-final-two-threads-2026-07-03"></a>

## 第四轮 — 最后两条线（2026-07-03）

- **almide-sqlite：已验证**。根因：带连字符 package 名在 `parse_toml` 报错，却被静默吞掉，丢失 `[native-deps]` rusqlite 注入（下游不透明 E0433，run.rs/build.rs 现警告）；native binding 也偏离 runtime 的 `AlmideMap` Map repr。两者修复，并写 `spec/sqlite_test.almd`：10 测试覆盖针对 `:memory:` 的全部 API + 文件持久化往返。native host package（像 porta）。
- **almide-web：已验证**。`runtime/headless.mjs` 为实现整个 import surface 的 Node reference host（virtual-DOM handle table、捕获 console、确定性 timer/fetch event queue），运行 `spec/host_app.almd` 并与固定 expected output 做 byte-diff（`spec/run_host_test.sh`）。覆盖每个 binding、string-intern protocol、两种 callback 重入；`runtime/web.js` 的 DOM TODO 得到同语义的真实 handle-table 实现。
- **交接步骤 4：完成 — read_message 在 VERIFIED render_program 路径字节一致运行**（stdin Content-Length framing → json.parse → record → response build → write_message，与 `almide run` diff）。所需工作：
  1. `stdlib/json_parse.almd` — 逐条从 native oracle 转写的纯 Almide 递归下降 JSON parser（char position、\u surrogate pair、宽松尾 separator、Int/Float 分离），另有 `json_ctor.almd`（object/array/stringify 委托 value_core）。注册到 self-host registry；`json.parse` 加入 str-result-module predicate，使 `let v = json.parse(s)!` 与 `match` 都追踪。
  2. **关闭健全性漏洞**：对未追踪 Result subject 的 `match` 即使 arm 带 call 也落入两 arm 线性化，两 println arm 都执行（静默误编译）。带 call arm 现 wall；纯 module-call subject（json.parse）像 Named call 一样追踪。
  3. **never-err strip 两方面修复**：`can_err` 现看得到 module call 上的 `!`（json.parse 会 err，parse_and_wrap 曾被错分 never-err）；bind 位置 strip 限 LIFTED（或 self-TCO）callee。声明 Result 的 fn 构建真实 Result block，去除其 `!` 会让 consumer 从 Result handle 读取 record field。
  4. 在 statement 与 value Result-match bind 中追踪 nested-variant payload（`ok(m)`，m: Option[record]），使 porta 的 `match m { some(req)/none }` 按 tag 分支。
  5. 被丢弃的 heap-result call（作为 statement 的 `write_message(..)!`）现接收并 scope-drop 返回 block；裸 void call 将其留在 wasm stack（无效 wasm）。
  gate：corpus-wall 对 4556 fn TOTAL；output-parity baseline 126 → **151**（+25，JSON-codec cascade）；almide-mir 测试 501/6-known；spec 273/273。

<a id="fifth-pass--the-output-parity-frontier-2026-07-03-same-day"></a>

## 第五轮 — output-parity 前沿（2026-07-03，同日）

指令：继续救援。正面攻击 render_program 路径的 MISMATCH（静默误编译）与 RUNERR（无效 wasm）类。**MISMATCH 6 → 0、RUNERR 8 → 2、parity baseline 151 → 162**。所得：

- **Value-model formatting**：stringify Float arm 现输出 Rust 原始 `{}`（去掉 float.to_string 的 ".0"）；float.parse 保留 -0.0 符号（用 `-1.0 *` 而非 `0.0 -`），以 accumulator（尾）递归把 pow10 scale 钳到 f64-inf 边界。"1e99999" 曾耗尽 call stack。
- **Unicode whitespace**：string.trim/trim_start/trim_end 在边界解码 UTF-8（char::is_whitespace 全 25 codepoint 集合）；int.parse / float.parse 经此处理（MIR 路径 C-021）。
- **json.parse 基于 prim byte floor 重写** — char-indexed 初版 O(n²)（string.get + 逐字符 concat），在多 KB glTF 超时；现 byte-addressed，用 write-cursor string decoder 与 \uXXXX UTF-8 encode（含 surrogate pair）。
- **lifted lambda 继承 variant_layouts + global_inits** — `list.filter((t) => match t …)` 中 custom-ADT match 曾对空 registry 解析，过滤掉全部内容（closures_and_variants）。
- **heap-element combinator routing**：非 String heap payload（variant、Value）上的 filter/get_or/unwrap_or 改路由到 rc-sharing self-host（`list.filter_rc`、`list.get_or_value`、option/result value-unwrap_or variant），而非把 block length word 当 byte count 读取的 `_str` deep-copy（UAF 垃圾）。不可 share combinator 路由到未注册名称＝干净 wall。
- **允许 scalar TCO**：针对无 destructure 的尾自递归。self-host byte-walker（`__split_fill`、`__chunk_outer`）不再在大输入耗尽 stack；tuple-destructure body 拒绝优化（loop rewrite 错处置），保留真实递归。
- **`Try` 在所有地方加入 `Unwrap`**：monadic desugar、never-err strip、新 rewrap（never-err call 赋给显式 `Result` 类型 var 时重新包成 `ok(call)`）。effect_assign_unwrap matrix（assign/loop/index/annotated/unannotated）全绿。
- **`_start` 处理显式 Result main**（读 tag、drop Ok block、Err 时 trap）。每个 `fn main() -> Result[Unit, String]` CLI（porta、almide-grammar）曾输出无效 wasm（"values remaining"）。
- **CLI dispatch 形状**（`match list.get(args,1) { some("cmd") => …, _ => usage }`）desugar 为可执行两 arm 形式；裸 Named call 解析到唯一已链接 user module；`string.capitalize` self-host。almide-grammar 现 render + 运行 dispatch matrix（output byte-diff 仍不同，module-record map 侧未解决）。
- top_let_test 暴露 **scalar module-global 被 lowering 成 Const-0**：const（无 call）initializer 现物化实值，含传递 const global（`SOLAR_MASS = 4.0 * PI * PI`）；含 call init wall。
- 全 `ok` lambda 的 `fan.map` 重写为 `list.map`（观测相同，fan lambda 不能捕获 var）并 defunctionalize。

第六轮（2026-07-03，同日）：**almide-grammar wall=0 恢复，作为 CLI 全面字节验证**。全部 4 generator mode 在同 argv 下与 `almide run` 字节匹配。三层：Option-String literal dispatch desugar、裸 Named 解析到唯一 linked module、module type layout alias 到裸 base name（裸 `Named` ref 在无 layout 下读 record，field 静默错位；仅唯一 owner，歧义保持 qualified＝wall）。还确立 v1 的 `env.args` 已与 v0 匹配（跳 argv[0]；早先差异是 `wasmtime -- args` 将字面 `--` 传给 guest）。`Map[Int, String]` / 非 String 值 heap map 现干净 wall，而不链接错误 slot plain/`_str` variant（map_set_eq：无效 wasm → 诚实 wall）。

第七轮（2026-07-03，同日）：系统处理 record-value surface。porta protocol 层现全绿（真实 wall 2 → 1，余项本质上邻接 wasmtime-bridge）。落地内容：monadic `match` 在 heap-result if-arm 内执行（tail-duplicated `let x = if c then f()! else []` — resolve_env）；允许 class 的空 list literal 物化；structural record-list literal 路由到合成 anon-record drop（并补 `$__drop_list_anonrec_<hash>` wrapper 生成），也打开 `f([{…}])` 参数位置；`list.get/first/last` share record 元素（按 elimination 对应到 layout 相同的 `_value` accessor）；record `??` default 经 handle-level value helper 选择。最后的 runtime RUNERR 也解决：append-accumulator TCO 现允许 pure-call 包裹的增长（`string.take(acc + "x", 8)` — tco_deep_recursion_churn 的 2M 次迭代 spin 字节匹配）。parity：match=163、RUNERR 1（负载不稳定）、MISMATCH 1 真实（float.parse 精确舍入）。

该前沿仍开放：Map repr variant（`_ivh` scalar-key/heap-val、`_hval` heap-val-non-String — map_set_eq 砖块）、`tco_deep_recursion_churn`（通过 call 构建 heap accumulator — `string.take(acc + "x", 8)` — 需通用 heap back-edge）、denormal/max 边界 float.parse 精确 decimal→f64 舍入（strtod 级课题）、almide-grammar output 差异，以及 porta 的 2 个 native-FFI wall。

<a id="sixth-pass--org-wall-1010-arc-ceangal-7721-2026-07-14"></a>

## 第六轮 — org wall 101→0 弧线、ceangal 77→21（2026-07-14）

- **tail spread/record ctor**（返回 `{ ...n, gap_main: v }` / `text()`）：tail.rs 现把 SpreadRecord+Record 路由到 arm 先例 builder；`lower_owned_heap_field` 增加 nested-spread arm + `value_or_global` Var arm（`_default` 等 record global，含 scalar-only fallback）；spread base 可为 Member（`{ ...v._style, … }` 经 `try_lower_heap_field_borrow` 借用）。ceangal 77→42。
- **mutable module-level `var` 砖块（scroll 类别 + 存在中的误编译）**：read/assign 曾静默物化 init／重绑定 local（`5 3 0` 对 native `5 8 8`，probe 确认；`inline_pure_call_globals` 也内联 mutable call-init，write 不可见）。现每个 `var` 在 `MG_SLOT_BASE + 8i` 有 linear-memory slot（移动 bump base，N=0 module 字节一致）。read = `LoadHandle`+`Dup`（cert `a` = render 的 rc_inc）；assign = build-new → `$__mg_take`（CallFn-owned、忠实证书）→ 按类型 drop → store+Consume；`g[i] = v` = take → MakeUnique(COW) → elem store → store-back；合成 `__mg_init` 在 `__global_init` 前运行（init 无法 lowering 则 program wall）。`__mg_take` 从 mir≤ir call count 排除（prim-class 注入 accessor）。ceangal 42→21（scroll 21→0）；module_var_test + wasm_cross_pkg 现跑 v1（corpus 0 现诚实）。
- **Float export ABI**：带 Float param/ret 的 `pub fn` export 经 `f64.reinterpret_i64` wrapper（`$__export_<fn>`）render；public ABI 提供真实 f64（v0 parity），非 Float param 保留实际 repr（heap 为 i32）。
- **已知 v0-wasm 差异（记录、未修）**：`let snapshot = g` 后 `g[i] = v` — v0-wasm alias（snapshot 看到 write），native clone（`9 9` 对 `9 2.5`）。v1 COW 匹配 native；v0 退役（Phase 3）使差异路径过时。
- **borrow-chain 后续（同日）**：`node.children[item.idx]`（list 为物化 aggregate field 的 index）经 `try_lower_heap_field_borrow` 借用；`line.items[ii].idx`（从 indexed 元素读 scalar）经 IndexAccess container arm 解析。共同打开整个 Yoga layout core（compute_size / do_layout / resolve_line_flex / layout_line / diff_tree），ceangal 21→14。fixture：`spec/lang/member_index_borrow_test.almd`。
- **ceangal 余 14 — 全部跨 module 阻塞（per-file classify 下限）**：逐项读 wall 实际代码，全依赖 per-file classify 无法解析的跨 module state（`lay.MARGIN_AUTO` record-field global、两 arm if 条件中的 `render.list_frame_height` 跨 module mutable global、经 generic Cell API 的 `td.todos.get()`、view_to_node/theme 中 `v.*`/`lay.*` sibling call）。ceangal per-file wall 指标已在跨 module 边界触底；后续经 (a) sibling-resolved 分类工具、(b) generic-module-fn mono/link 修复、(c) v1 pipeline 跨 module mutable global bridge（当前 slot map 同 region）推进。
- **既有 checker bug（阻塞 ceangal 自身套件，非本弧线回归）**：ceangal `almide test` 失败于 #433 name-pinning postcondition（build_lines/resolve_line_flex/layout_line/do_layout 中 `Node` 应 pin `layout.Node`），发布版 v0.28.6 同样复现。单文件／两 module 最小 probe 不复现，需要 ceangal 实际 module graph。独立 checker 工作线。
- **既有 build blocker（ceangal，v0.28.6 也有）**：`almide build
  src/mod.almd --target wasm` IR verify 失败：`call to unknown function
  'cell.get'`（todoapp）。经跨 module UFCS 调用的 generic module fn（`fn get[T](c: Cell[T])`）从未 monomorphize/link。frontend（mono + ir_link）工作线。
- **homullus/almai 前沿 = WASI-env runtime 砖块**：主要 wall（`memory_path`/`memory_dir`/`sessions_dir`/`require_env` …）为 `match env.get(..)` tail。v1 无 `env.get` lowering，v0-wasm 也直接 ICE（`[ICE] emit_wasm: no WASM dispatch for env.get`，calls_env.rs:63；发布 binary 也是 panic，而非 wall）。打开该类需两 engine 通过 WASI `environ_get` 提供 env（+ env 可观测性 C-contract），随后 match-tail 机制原样可用。非 env homullus wall（value_to_tool_call/value_to_message 的 Record/Value shape）为普通 lowering 砖块。
  **同日交付：两 engine 的 WASI-env 砖块**。(1) v0-wasm `emit_env_call("get")` — 仿 `emit_wasi_argv_list` 的 environ scan；some() 为装值 String 的 boxed cell（`unwrap_or`/match 载入 `*opt`，v0-wasm Option 约定；unboxed 初稿 match 成功却打印空）。(2) runner + 全 3 个 test-gate wasmtime spawn 传 `-S inherit-env=y`（否则每次 guest lookup 为 none，正是设计预见的静默差异陷阱）。(3) v1：`PrimKind::EnvGet`（Capability::CliArgs，Env profile canonical cap，无新 id）、`$env_get` preamble WAT（4 对齐 out-pointer — `$alloc` 不保证 alignment，否则 wasmtime trap — 及 `$env_envp`/`$env_cnt` global 中一次性 environ snapshot；每 call 重读会 leak scratch，120k leak-loop 挂起）、`env_get.almd` self-host + `prim.env_get` 声明、cert `i`-alloc + kernel 验证。(4) C-133（ALS-R5）+ `spec/wasm_cross/env_get.almd`（243 equal）。almai 7→0、homullus 9→6、RATCHET 0（fixture 自身经 v1 lowering）。

- **homullus 余 6 = external-type 分类假象 + glob path-model 缺口**：`ok(Resp{...})` tail 单 module lowering 正常（probe 确认）；wall 来自 `almai.LLMResponse`/`almai.ToolCall` 是 per-file classify 无法解析的外部 package 类型（同 ceangal 跨 module 残留的假象类）。真实缺口为 `fs.glob`：native 对绝对 canonical host path 匹配 pattern（runtime/rs fs.rs `glob_recursive`），wasm preopen path model 无法字节一致复现；v0-wasm 也无 glob dispatch（同 env.get 曾有的潜在 ICE 类）。先需 path-model contract；此前按设计排除于 byte-parity subset。
- **测量现为 org 弧线瓶颈**：per-file classify 在 ceangal/homullus 的假象上触底。下个高杠杆砖块为 sibling/package-resolved 分类（采用 render_program 相同 import resolution），使 org 数字只数真实 lowering 缺口。
- **resolved-strict 测量（同日，经 render_program，已运行 canonical driver discovery）**：**homullus = 0 wall，ceangal = 8**（per-file 为 14）。theme 3 — `let gray_50 = v.rgb(…)` module→module global ref 即使 resolved 仍未绑定（跨 module top-let bridge 漏掉 sibling module 引用的 call-init global，是真 pipeline 缺口）；scroll 2 + layout 1 — strict-mode scalar-binding/for-in-var value-subset wall（permissive classify 隐藏）；render 2 — view_to_node/zip_view_rects heap-result-if（真实，现 resolved 测量）。真正 org 前沿 ≈ ceangal 8 + porta 2 + 零星项。org-trust-status.sh 应采用 resolved 扫描作为头条数。
  **8 项精确分成接下来的 3 砖块**：(1) theme 3 = module→module top-let bridge（`bridge_cross_module_toplets` 仅映 main 侧 ref，即带 `module_origin` 的 `ir.var_table`；sibling module ref 在自己的 `m.var_table`，从未 bridge。需 per-module region map，注意共享 union 所容忍的 VarId 冲突）。(2) scroll/layout 3 → scroll 2 同日交付：`let id =
  region_count` — RHS 裸 mutable-global Var 的 bind 同时漏过 alias arm（`value_for` 仅解析 local）与 projection 尝试（无 Var kind）→ strict wall；scalar-value 尝试加入 Var 后，经 `value_or_global` slot Load 路由。ceangal resolved 8→6。layout resolve_line_flex（for-in VAR wall）仍余。(3) render 2 = arm-block tail-helper lift：zip_view_rects else arm 为返回 record 的完整 block（global assign + for-loop + accumulator）；heap-result-if arm 机制 lowering arm value，1792e5d7 dense-chain lift 须泛化，将这类 arm block lift 为 helper fn。

<a id="seventh-pass--ceangal-0-2026-07-15"></a>

## 第七轮 — CEANGAL 0（2026-07-15）

最后两个 render wall（view_to_node / zip_view_rects）经带插桩的 bisect 分成 3 个小缺口，各有 probe 验证 + parity 运行：

- **anon-record concat 准入**：`items + [{ x: …, content: nm, … }]` — checker 将 19-field 元素保留为 structural，concat 准入用 Named-only `record_drop_type_name`。扩为 `record_or_anon_drop_type_name`（已有 `$__drop_list_anonrec_<hash>` wrapper 生成）。
- **module-keyed top-let bridge**：`kind: v.ROW` — view.ROW 与 layout.ROW 名字冲突，bridge 歧义规则删掉两者（未绑定 → 整 loop → arm → tail chain wall）。现按（module_origin、NAME）索引，裸名 map 作 fallback；消除冲突类别，而非绕开。
- （第六轮修复承担其余：alias-chase/ty-repair/pure-call subst、重绑定 list-registration 传播、member-container borrow。）

**ceangal resolved-strict：全 15 src module 为 0 wall**。zip-shape fixture：`spec/wasm_cross/zip_view_shape.almd`；60k-call leak-loop parity 绿色。组织前沿现为 porta 2（native-FFI，结构上排除）+ external-package classify 假象；org 弧线 lowering 侧已完成，待 porta 类别决策。

<a id="remaining-threads"></a>

## 剩余线索

- **wall=0 数 21 → 19（诚实，不是回归）**：linearization guard 与 never-err-strip 修复暴露 porta（2，native-FFI move-out 类）和 almide-grammar（1，未追踪 subject 上含 call arm）的 wall；这些形状此前被 lowering 为静默错误路径（两 arm 都运行／Result block 被当 payload 读）。两仓库自身套件在 production（emit_wasm）路径仍绿／byte-verified。下一砖块是在 MIR 路径忠实支持这些形状。
- output-parity full-run 不稳定：2–3 个 baseline file 仅在 full-gate machine load 下掉落，单独字节匹配（既有，已记录）。
- 6 个 almide-mir record-materialization DEBUG test（另一工作线）。
