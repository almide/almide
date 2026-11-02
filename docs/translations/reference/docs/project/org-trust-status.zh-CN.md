<a id="almide-org--legacy-v1-trust-spine-report"></a>

# Almide org — legacy v1 trust-spine 报告

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](org-trust-status.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](org-trust-status.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](org-trust-status.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/org-trust-status.md)
> 此 reader 不是原 generator 的输入或输出目标，不自动更新。保留原文截断诊断、列不齐及历史状态差异，不修补或重测。尤其表格与后段注记／spot-check 的 porta 数量及多个 repo verdict 不同。旧 host 的无 wasm 排除是历史资料，不是当前 capability 指引。

> 原 generator：`scripts/org-trust-status.sh`。原 org 目录：`/Users/o6lvl4/workspace/github.com/almide`。

> **历史证据边界。** 保留的 verdict 及 2026 年 7 月 spot-check 是历史观察，不是当前组织状态。script 将 legacy MIR classifier 与 2026-07-02 固定的 `BYTE_VERIFIED` 集合结合；再次运行不会重新测试那些 label。如今 Wasm 路线为 structural，host-operation 支持自下列排除项写成后已有变化。独立 test-suite 通过确立的是相应 assertion，不是任意 stdout/stderr/exit 字节一致。
>
> 新的 org 整体 verdict 需要固定 compiler 与 sibling revision、当前 classifier 及明确行为比较。保留原始行，不以未测量的当前 verdict 替代。

**14/28 repo 在 RESOLVED v1 pipeline 下完整 render**（每个 `src/` module 在 canonical sibling + dep 解消下无 wall 地 lowering，与 `almide build --target wasm` 路径相同；26 个 module 仍有 wall，5 个被 frontend 拒绝）。次级逐函数 classify sweep：19/27 repo 为 wall=0，real wall = 21 / native-FFI = 46（排除），2433 个函数 lowering。

> **标题使用 resolved 指标。** classify_corpus 读取单文件，不做跨 module 解消，因此逐文件数值会计入真实 pipeline 已解消的 artifact（`lay.*`/`v.*` 引用）。仅将它作为逐函数 wall-reason 的处理线索，而非 verdict。

> **wall=0 指标 = 仅 REAL lowering wall。** NATIVE-FFI wall 指传递调用 `@extern(rust/rs)` 声明（没有 wasm 形式），或永久无 wasm 的 stdlib effect（`process.exec/exit/run`、`http.request`、`net.*`）的函数。它们需要 native host，永远不能 lowering 到 wasm，所以像既有 `@extern(wasm)` WASI import 一样排除。它们不是 lowering bug。只有 REAL wall 驱动 wall=0 目标。

⚠ **wall=0 ≠ 正确。** 它表示每个函数都在 v1 lowering subset 内，不表示输出行为正确。权威的 ② gate 是 **native==wasm byte-match**（在 native 与 `--target wasm` 两边运行 repo 自己的 vector/test）。`🟡 lowers, byte-match TODO` 标记已达 wall=0 但尚未 byte-verified 的 repo，正是静默误编译可能藏身之处（例如 `var v=w` scalar-aliasing bug 曾伪造早期 sha1=0，直到 RFC vector 抓到它）。

<a id="per-repo-sorted-by-walls"></a>

## 逐 repo（按 wall 排序）

| repo | 已解消（render） | lowering 数 | real wall | native-FFI | classify 状态 | 最常见 wall 原因 |
|------|--------------------|-------:|-----------:|-----------:|-----------------|-----------------|
| `homullus` | 🔴 4/11 — frontend: type errors: [\"check() expects 3 argument(s) but got 2\" | 77 | 6 | 4 | 🔴 6 +1 xmod, 4 native-FFI 排除 | REAL heap-result X outside the executable subset cannot  |
| `ceangal` | 🔴 8/10 — [render_program] Unsupported("parse error: error: Expect | 250 | 5 | 0 | 🔴 5 | REAL use of unbound var VarId(N) |
| `toml` | 🔴 0/1 —   parse_octal: Unsupported("guard-else early return cann | 55 | 3 | 0 | 🔴 3 | REAL guard-else early return cannot be faithfully lowere |
| `porta` | 🔴 10/18 —   dedup_env: Unsupported("while body with a heap-accumul | 165 | 2 | 24 | 🔴 2 +2 xmod, 24 native-FFI 排除 | NATIVE-FFI heap move-out from <other> (only a bound var, |
| `almide-dojo` | 🔴 5/7 —   compile_and_test: Unsupported("heap-result X outside t | 84 | 2 | 1 | 🔴 2, 1 native-FFI 排除 | REAL heap-result X outside the executable subset cannot  |
| `wasm-canvas` | 🔴 1/2 —   main: Unsupported("non-empty List[heap] literal with n | 46 | 1 | 0 | 🔴 1 | REAL non-empty List[heap] literal with nested-ownership  |
| `canvas` | 🔴 1/2 —   main: Unsupported("non-empty List[heap] literal with n | 46 | 1 | 0 | 🔴 1 | REAL non-empty List[heap] literal with nested-ownership  |
| `almai` | 🔴 8/10 —   retry_loop: Unsupported("heap-result X outside the exe | 183 | 1 | 2 | 🔴 1, 2 native-FFI 排除 | NATIVE-FFI heap-result X outside the executable subset c |
| `yaml` | ✅ 1/1 | 74 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `wasm-webgl` | ✅ 3/3 | 85 | 0 | 0 | 🟡 0 — 可 lowering，byte-match TODO | — |
| `vscode-almide` | — | — | — | ▫ 不是 Almide `src/` library | — |
| `tree-sitter-almide` | — | — | — | ▫ 不是 Almide `src/` library | — |
| `three` | — | — | — | ▫ 不是 Almide `src/` library | — |
| `tech-article` | — | — | — | ▫ 不是 Almide `src/` library | — |
| `syschord` | — | — | — | ▫ 不是 Almide `src/` library | — |
| `svg` | ✅ 2/2 | 80 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `sha1` | ✅ 1/1 | 14 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `rsa` | ✅ 2/2 | 19 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `prism-ml-llama.cpp` | — | — | — | ▫ 不是 Almide `src/` library | — |
| `playground` | — | — | — | ▫ 不是 Almide `src/` library | — |
| `obsid` | 🔴 3/4 — [render_program] Unsupported("unlinked stdlib/runtime ca | 177 | 0 | 0 | 🟡 0 — 可 lowering，byte-match TODO | — |
| `nn` | 🔴 13/15 —   approx_row: Unsupported("scalar tail outside the value | 289 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `lumen` | ✅ 4/4 | 75 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `docs` | — | — | — | ▫ 不是 Almide `src/` library | — |
| `csv` | ✅ 1/1 | 10 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `bonsai-almide` | — | — | — | ▫ 不是 Almide `src/` library | — |
| `bigint` | ✅ 1/1 | 35 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `base64` | 🟠 0/1 — frontend: type errors: [\"cannot call effect function 'decode_chunk | 0 | 0 | 0 | ▫ 全部跨 module | — |
| `audio-poc` | ✅ 1/1 | 4 | 0 | 0 | 🟡 0 — 可 lowering，byte-match TODO | — |
| `almide-web` | ✅ 5/5 | 23 | 0 | 0 | 🟡 0 — 可 lowering，byte-match TODO | — |
| `almide-wasm-bindgen` | 🔴 0/1 —   tuple_supported: Unsupported("scalar tail outside the  | 97 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `almide-sqlite` | 🔴 0/1 —   open: Unsupported("scalar tail outside the value subse | 14 | 0 | 14 | 🟡 0 — 可 lowering，byte-match TODO, 14 native-FFI 排除 | — |
| `almide-lander` | 🔴 1/2 —   main: Unsupported("effectful/impure stdlib Module call | 10 | 0 | 1 | ✅ 0 — 已 byte 验证, 1 native-FFI 排除 | — |
| `almide-js` | — | — | — | ▫ 不是 Almide `src/` library | — |
| `almide-grammar` | ✅ 2/2 | 17 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `almide-bindgen` | ✅ 23/23 | 405 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `almide-aituber` | ✅ 8/8 | 72 | 0 | 0 | 🟡 0 — 可 lowering，byte-match TODO | — |
| `aes` | ✅ 1/1 | 27 | 0 | 0 | ✅ 0 — 已 byte 验证 | — |
| `_references` | — | — | — | ▫ 不是 Almide `src/` library | — |

<a id="cross-repo-wall-lever-most-frequent-reasons"></a>

## 跨 repo 的 wall 处理杠杆（最常见原因）

清除一个高频原因的 brick 可以同时推进多个 repo。

| 数量 | wall 原因（规范化、原诊断片段） |
|------:|--------------------------|
| 29 | NATIVE-FFI heap move-out from <other> (only a bound var, fresh literal, or call) not in this 
| 8 | REAL heap-result X outside the executable subset cannot be faithfully returned in this brick (
| 7 | NATIVE-FFI heap-result X outside the executable subset cannot be faithfully returned in this b
| 3 | NATIVE-FFI heap-result Try cannot be faithfully returned in this brick (would move out an empt
| 3 | NATIVE-FFI effectful/impure stdlib Module call process.exec needs a declared capability not in
| 2 | REAL use of unbound var VarId(N) |
| 2 | REAL non-empty List[heap] literal with nested-ownership elements (a heap-field record/tuple, a
| 2 | REAL List argument cannot be faithfully materialized in this brick (would borrow an empty defe
| 2 | REAL guard-else early return cannot be faithfully lowered (vN has no early-return             
| 2 | NATIVE-FFI match over an UNTRACKED subject with a call-bearing arm cannot take the both-arms l
| 1 | REAL while body with a heap-accumulator reassignment cannot be faithfully lowered (the model-o
| 1 | REAL if over an unresolvable condition (Gt over Float; Var vs LitFloat) with a call-bearing ar

<a id="notes"></a>

## 注记

- sweep **每个** `src/*.almd` module，不只是 entry。**resolved 列**经真实 pipeline（sibling + dep 解消）render 每个 module，所以跨 module importer 也在该列测量。classify_corpus 单文件读取、不解消跨 module import，因此 sweep 跳过 sibling importer，逐 repo 显示为 `+N xmod`；仅用于逐函数 wall reason。
- `almide` repo 本身是参考 corpus（自己的 `proofs/corpus-wall.sh` gate），不是这里的目标。
- `porta` 是 NATIVE HOST（`almide.toml`：wasmtime + reqwest/Net）；完整 MCP server 按设计仅 native（WASI preview1 无 net，也不能 embed wasmtime），所以 25 个 native-FFI 是其 host call，只有 PORTABLE protocol layer（jsonrpc/config）属于 v1 subset。其 byte-verification = 两 target 的完整 test suite（7/7 个 wasm 可运行 file + 1 个按设计 native-only 的 FFI file），加完整 MCP server 的 native build，两者都 green。
- ✅ **byte-verified** = repo 自己的 test suite 在 `almide test --target native` 和 `--target wasm` 两边全数通过（assertion 就是 vector）。2026-07-02 sweep 修复其自身发现的六类 wasm bug 后，把覆盖从 2 repo（yaml、sha1）扩展到所有拥有 test suite 的 repo（pass-through/copied-pair 路径的 share/+1、list-layout 分配不足、缺失 Value deep-eq、value.merge oracle mismatch、bytes.set value semantics；contract C-121..C-125，见 docs/roadmap/active/v1-org-byte-verification.md）。
- 尚不能 byte-verified 的 repo：`almide-web` / `almide-sqlite`（没有 test vector，先写 suite），`almide-dojo`（task-bank fixture，不是可编译 suite）。
- byte-verify 一个 repo：在两 target（`almide test --target native` 与 `--target wasm`）运行其自身测试，要求每边全数通过，再加入 script 的 `BYTE_VERIFIED`。

<a id="graphics--ai-stack--production-target-spot-check-2026-07-02"></a>

## Graphics / AI stack — 产品 target spot-check（2026-07-02）

graphics 与 AI repo 大多为 BROWSER-HOSTED wasm app（JS host 加载 module），所以其“在 v1 上运行”表示当时 compiler 生成有效 wasm artifact；headless byte-run 比较不适用于 rendering host。验证状态：

| repo | 类型 | 状态 |
|------|------|-------|
| `svg` | graphics（pure lib） | ✅ 已 byte 验证 （suite，两个 target）; 跨 module 的 `render(group(..))` compiler stack overflow 已修复（2026-07-03 equi-recursive unify guard） |
| `lumen` | graphics（pure lib） | ✅ 已 byte 验证 （suite，两个 target） |
| `canvas` | browser-hosted app | ✅ wasm 构建正常 |
| `wasm-canvas` | browser-hosted app | ✅ wasm 构建正常 |
| `wasm-webgl` | browser-hosted app | ✅ wasm 构建正常 |
| `obsid` | browser+native-hosted app | ✅ wasm 构建正常 （尚无 test suite） |
| `almide-aituber` | browser-hosted app | ✅ wasm 构建正常 （v1 分歧源于缺少 develop 侧 #717 auto-?-branch-retype 修复，2026-07-02 cherry-pick） |
| `homullus` | AI agent | ✅ 已 byte 验证 （suite，两个 target） |
| `almide-sqlite` | native host lib | ✅ suite green（10 tests，:memory: + file 持久化）。native host package（rusqlite FFI），2026-07-03 验证 |
| `almide-web` | browser bindings | ✅ headless vector green（17 行 byte-match：完整 import surface + intern protocol + runtime/headless.mjs 下 callback），2026-07-03 验证 |
| `nn` | AI (neural nets) | ✅ 已 byte 验证 （suite：13/13 files，两个 target）。2026-07-02 由 TCO-temp type 修复、nested-HOF lambda-pin 修复（C-126）、Matrix repr、SIMD fast-exp clamp 及 unwrap_or chain 修复（C-127）解除阻碍 |
| `almai` | AI (LLM client lib) | ✅ native suite green（56 tests，2 files）。2026-07-03 由 STRUCTURAL-TWIN merge 解决：checker 跨 module 统一同 base-name、同 shape 的 record 声明，codegen 因而合并为一个 canonical struct（flatten twin-merge；E0063 default-fill 和 bare-ref 修复也用同一机制）。wasm leg：native fallback（`env.get`/http 仅 native；LLM client 需要 network host） |

在这份记录中，上述每个 graphics/AI repo 均已在 v1 上构建或验证。

> 表中诊断栏原样保留被截断的实际输出。诊断词句译文：frontend: type errors 为“前端类型错误”；check() expects 3 argument(s) but got 2 为“check() 要求3个参数但收到2个”；parse error 为“解析错误”；unlinked stdlib/runtime call 为“未链接的标准库／运行时调用”；scalar tail outside the value subset 为“值 subset 外的 scalar tail”；cannot call effect function 为“不能调用 effect 函数”。未补写缺失尾部。
>
> wall reason 译文（按行）：29项＝允许形式（已绑定变量、新 literal、call）之外的 heap move-out；8项＝不能忠实返回可执行 subset 外的 heap-result X；7项＝同类 heap-result X 的 NATIVE-FFI 分类；3项＝不能忠实返回 heap-result Try，后续提及移出空对象但已截断；3项＝process.exec 的 effectful/impure stdlib Module call 需要未收录的声明 capability；2项＝使用未绑定 VarId(N)；2项＝具有嵌套所有权元素的非空 List[heap] literal；2项＝无法忠实物化 List 参数，后续提及借用空对象但已截断；2项＝无法忠实 lowering guard-else early return，vN 没有 early-return；2项＝对 UNTRACKED 对象的 match 含有调用的 arm，无法采用某个双 arm 方案，尾部已截断；1项＝含 heap-accumulator 重赋值的 while 本体无法忠实 lowering；1项＝条件不可解消（Float 上 Gt，Var 对 LitFloat）的 if 含调用 arm，尾部已截断。REAL/NATIVE-FFI 分类及数字以原表为准。
