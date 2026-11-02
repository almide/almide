<a id="adr-0026-effect-categories-ride-the-transparent-fn-type--sets-flow-through-callbacks-unannotated-categories-form-a-two-level-hierarchy-and-inferred--manifest--host-is-one-chain"></a>

# ADR-0026：effect category 搭载于透明 fn 类型 — 集合不加注释流经 callback，category 构成两级层次，inferred ⊆ manifest ⊆ host 为一条链

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0026-effect-categories-ride-the-transparent-fn-type.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0026-effect-categories-ride-the-transparent-fn-type.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0026-effect-categories-ride-the-transparent-fn-type.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md)
> 保留初始2026-10-03状态和Open节，并列后续2026-10-25实现注记；它们是不同阶段，不是单一当前完成结论。static chain不是runtime授权grant。不合并历史6/13category词汇与14leaf实现，保留plain extern未决effect-bit/ABI边界。示意E0xx不是已交付diagnostic ID。

- **状态**：Accepted（D1–D4，2026-10-03）。除 inference fixpoint（#3238）外均未实现。§Open 问题未决定。
- **日期**：2026-10-03
- **范围**：effect *category* 是什么（相对于 effect bit）；category set 如何经过 function value、callback 和 closure；category 如何命名与分组；compiler 视图如何关联 `almide.toml` 的 `[permissions]` 以及 host 授予的 capability。
- **基础**：[ADR-0002](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0002-fallibility-effect-orthogonal.md)（§D6：effect 蕴含 fallible，此处不变）、[ADR-0009](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0009-fn-type-quadrant-transparency.md)（D2：裸 fn-type parameter 透明；本 ADR 将透明 bit 扩展为集合）、[ADR-0022](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)（pure `fn` 中的 output/abort builtin）。
- **相关**：#3235（进行中的 ADR-0025：wasm `process` 作为 private host capability，D2/D3 中 `Proc` leaf 的 host 侧）。
- **章程**：[effect-system-capability.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/effect-system-capability.md)（Phase 4“EffectSet in Ty::Fn”是本 ADR 设计的类型级工作；其 `IO` 简写由 D2 替代）。
- **历史**：2026-10-03。外部设计提案“Almide effect仕様案 v0.1”（baseline 902e58d23）要求用封闭 category set 作为 function type 上界。裁定前进行下方跨语言比较，向 maintainer 逐一提出三个问题，全部答○。第四个诊断问题在跨工具比较 query command 形态后答○（D4）。

<a id="context"></a>

## 背景

develop（7f2f03cca）已有：

- **effect bit** 由 type checker 检查：`fn` 不能调用 `effect fn`，fn 类型参数按 ADR-0009 携带 bit。
- **category** 是单独、更粗的分析：`crates/almide-ir/src/effect.rs` 有六个（`IO`、`Net`、`Env`、`Time`、`Rand`、`Fan`），`pass_effect_inference.rs` **按 module 名**赋予它们（`path` 和 `url` 虽仅做纯 string 工作，仍算 `IO`/`Net`）。它们不出现在任何类型里。
- `src/cli/mod.rs::check_permissions` 将传递集合与 `[permissions].allow` 比较。空列表允许一切（`src/project.rs:81`），未知名称静默丢弃。
- 传递闭包20轮后停止，因此深 call chain 把 effectful root 报为 clean。#3238 已修复（worklist 到最小 fixpoint）。
- `docs/wasm/capability-system.md` 和 roadmap 描述13 category；代码只有6。

因此 compiler 能说某函数*触及*外界，却不能说*哪一部分*；关于哪部分的信息也无法穿过 callback。

<a id="comparison-2026"></a>

### 比较（2026）

| 语言 | 表示 | 多态、负担 | 粒度 | static ↔ runtime |
|---|---|---|---|---|
| Koka | row `<console,exn\|e>` | 隐式 open row；row variable 泄漏到 error | user-defined + handler | 无 |
| Effekt | 隐式传递 capability | 上下文相关，不写 effect variable；second-class function | user-defined + handler | 无 |
| Flix | set `\ {FsRead, Net}`、Boolean/set 合一 | `\ ef`、`ef1 + ef2`、`ef - Block` | 固定 base set + user-defined | per-package effect lock |
| Scala 3 capture checking | capability 为值，`T^{io}` | capture set，重，实验性 | capability | 仅语言内部 |
| Roc | `->` pure / `=>` effectful | 无 | 二值 | platform 提供 effect |
| Wado | `with (Stdout, FileSystem)` | 每 fn 一个 effect variable，从 HOF 实参推断；local 推断，`pub` 显式 | 与 WASI interface 1:1 | component import |
| Aver | `! [Disk.readText]`、`! [Disk]` | — | per-method，带 namespace | runtime 也强制 |
| Jacquard | `->{net}` | row | handler | `--allow net`；`why-effect` command |
| Deno | 不在类型中 | — | `--allow-read=path` | 仅 runtime，不示原因 |

最接近 Wado 设计（local inference、显式 `pub`、无 handler、直接映射 component import）；基础词汇是 Flix 去掉 handler。无 handler 的封闭集合并不落后于前沿。缺口是 package 边界 effect diff（Flix）、静态 effect 到 host grant 的一条链（WASI、Jacquard）及携带路径的诊断（Jacquard）。

<a id="decision"></a>

## 决定

<a id="d1-a-bare-fn-type-parameter-is-transparent-for-the-category-set-too--2026-10-03"></a>

### D1。裸 fn-type parameter 对 category set 也透明（○ 2026-10-03）

ADR-0009 D2 已让裸 `f: (A) -> B` 向 HOF call 传递实参的 effect *bit*。同一 parameter 现在也传递 category *set*：

```almide
fn apply(xs: List[String], f: (String) -> Int) -> List[Int] = xs |> list.map(f)
// apply(paths, (p) => count_lines(p)!)  — the call carries FS.read
// apply(names, (s) => string.len(s))     — the call carries nothing
```

- 编写者从不命名 effect variable。surface language 无 `effects E`，**diagnostic 绝不打印 variable**，只打印 `{FS.read}` 等具体集合。
- 内部 checker 解集合（surface 仅 union；solver 可用 Flix 风格集合合一）。裸 parameter 与 lambda 包裹实参产生相同集合。
- **显式** slot（ADR-0009 D3，例如 `effect (Req) -> Resp!`）固定上界；存储或返回的 function value 保持所存值的集合，任何 adapter 不得丢掉它。

<a id="d2-categories-are-a-closed-two-level-hierarchy--2026-10-03"></a>

### D2。category 是封闭两级层次（○ 2026-10-03）

| Parent | Leaf |
|---|---|
| `FS` | `FS.read`、`FS.write` |
| `Net` | `Net.fetch`、`Net.listen` |
| `IO` | `IO.stdin`、`IO.stdout`、`IO.stderr` |
| `Env` | `Env.read`、`Env.write` |
| — | `Proc`、`Time`、`Rand`、`Fan`、`Abort` |

- **仅有这四个 parent。** 其他都是 leaf。
- inference 和 `[permissions]` 按 leaf 工作。`pub` 边界可声明 parent（`{FS}`）作为粗上界，因此后来内部出现 `FS.write` 不算 interface change。
- parent 意味着编写时所针对 registry version 的 leaf。**后来增加的 leaf 不自动加入已有 parent**，必须显式 grant。
- category **按 operation**而非 module 赋予：`url.parse` 及 path 操作为空，`http.get` 为 `Net.fetch`。
- `Abort` 仅指 `panic` / 失败 `assert*` / `process.exit`。`Result` error 不是 `Abort`，fallibility 仍是 ADR-0002 独立轴。`panic` 同时带 `Abort` 与 `IO.stderr`。
- 替代 roadmap 的 `IO = FS.read + FS.write + IO.std*` 简写：`IO` 是三个标准 stream，file access 是 `FS`。

<a id="d3-inferred--manifest--host-is-one-chain--2026-10-03"></a>

### D3。inferred ⊆ manifest ⊆ host 是一条链（○ 2026-10-03）

```
inferred set   ⊆   [permissions].allow   ⊆   capabilities the host grants
(compiler)         (almide.toml)              (wasm imports / Porta)
```

- **wasm**：从 artifact 剪除 inferred set 外的 import，因此未获 grant 的 operation 不只是禁止，而是不存在。
- **public API**：扩大 `pub` function effect 上界由 `scripts/check-interface-diff.sh` 分类为 **breaking**，走既有 `@deprecated` / epoch 流程。
- **依赖**（后续阶段）：每个 dependency 的 effect summary 记录于 lock；迁往 summary 增长的版本需显式 acknowledgment。
- 顺序：先 pruning 和 interface diff，最后 lock。
- static chain 不是 runtime grant。一次 run 可触达哪些 file、host、process 仍由 host 决定；effect summary 绝不是授权 token。

<a id="d4-a-violation-names-its-path-the-query-is-almide-check---effects--2026-10-03"></a>

### D4。违例指出路径；query 为 `almide check --effects`（○ 2026-10-03）

```
error[E0xx]: `validate` may write files, but its bound is {FS.read}
  --> src/check.almd:12:1
   = added: FS.write
   = path:  validate → normalize → save_cache → fs.write_text   (src/cache.almd:40:3)
   = hint:  1. return the data and let the caller save it
            2. move `save_cache(...)` out of `normalize` into the caller
            3. if writing is intended, widen the bound to {FS.read, FS.write}
```

（代码和措辞仅示意；error code 在实现时分配。）

- 第一行指出边界及新增 category。`path:` 为每个 category 给出一条代表路径，以 operation 位置结束。
- 经过 callback 时，若已知则指出具体 callee，否则指出 parameter（`apply → f (arg 2 of apply)`），绝不编造 callee。
- fix-it 顺序为*作为值接收／移到 caller／扩大 bound*。扩大排最后；没有 `almide fix` 会向 bound 或 `[permissions]` 添加 category，diagnostic 不给 agent 自行授权的捷径。
- **query 扩展已有 view，而不加 verb**：

  ```
  almide check --effects              # every function's set (today's output)
  almide check --effects FS.write     # functions carrying FS.write, with every path
  almide check --effects validate     # validate's categories, with a path for each
  almide check --effects FS.write --json
  ```

  category（`FS.write`，首字母大写并含点）与 function 名不可能同写，因此参数无歧义。`--json` 可组合 `--effects`（当时冲突），输出顺序稳定，同一 report 也是 MCP tool。diagnostic 的 `path:` 行与 `--effects` 输出形态相同，读者可直接从 error 转到 query。
- 形态先例为 `cargo tree -i`（对已有 view 的新问法），而非新 top-level verb。已拒绝：带连字符 top-level `why-effect`（Jacquard 的 `jac why-effect`；Almide verb 为一个词）、通用 `almide why`（一种用途不足以支持通用 verb）、名词 command `almide effects`（两处列相同集合）。其他考虑过的先例：`go mod why`、`npm explain` / `yarn why`、`nix why-depends A B`、Gradle `dependencyInsight`、Bazel `somepath`。

<a id="rejected"></a>

## 拒绝项

1. **user-defined effect 与 handler**（Koka、Effekt、Unison、OCaml 5、Flix）。multi-shot handler 与 reference counting 和 region 冲突，并向目标读者隐藏 control flow。若将来加入，只能 one-shot，另立 ADR。
2. **surface effect variable**（`(A -> B) effects E`、Koka row、Rust `?effect`）。ADR-0009 已因 error message 拒绝 row polymorphism；D1 无须这些语法即可获得相同表达力。
3. **second-class function**（Effekt）。破坏既有代码的 closure 存储和返回。
4. **surface 中把 capability 作为值**（Scala capture checking）。注释负担重且仍实验；D3 改在 host 边界采纳思路。
5. **每个 function/closure 强制 effect annotation**（Vera）及 **per-method 粒度**（Aver）。负担过大／过细；选定四 parent 下14 leaf。
6. **effect 不在类型中**（OCaml 5）与 **runtime-only permission**（Deno）。两者都无法说明原因或路径。

<a id="open--not-decided-by-this-adr"></a>

## Open — 本 ADR 未决定

- **统一 category 词汇**：`almide check --profile critical --allow` 接受 `IO, Net, Env, Time, Rand, Process`，这是 `effect.rs` 六个和 roadmap 十三个之外的第三写法（`Process` 对 `Proc`）。D2 versioned registry 必须替换三者。
- 声明上界及显式空集合（提案的 `effects {}`）的 **surface syntax**。
- **plain `fn` 的兼容 bound**：ADR-0022 允许 output/abort builtin，因此 plain `fn` 至多 `{IO.stdout, IO.stderr, Abort}`，不是 `{}`。是否提供严格空 bound，如何拼写。
- **manifest versioning**：当时空 `allow` 意味着“一切”。versioned manifest 必须区分省略／空／全部，拒绝未知名（提案 EFF 03、EFF 18），不能不通知就翻转当时含义。
- **语言定义 trap**（overflow、bounds）是否属于 `Abort`。
- **extern 和 native-shim contract**，以及如何报告 trusted dependency（提案 EFF 14、EFF 15）。

<a id="consequences"></a>

## 后果

- `Ty::Fn` 在 effect bit 旁增加 category set（roadmap Phase 4 设计上已解除阻碍，仍无排期）。意图为 checking 后擦除 category，使 monomorphisation 不按集合成倍增加（尚未验证）。
- category 分配从 module 名迁往单一 versioned registry 的 per-operation contract，由 checker、manifest validation、diagnostic 和 docs 读取。
- 当时 CLI 名（涵盖 file 的 `IO`）需要迁移到 D2 名称。

<a id="falsifier"></a>

## 证伪条件

1. **Dojo 显示透明集合比显式 bound 更常导致 LLM 编辑失败**（如 effect“出现”在未修改 call site）——退回 `pub` HOF 的显式 slot。
2. **编写者处处用 parent，leaf 从不收窄**，`pub` bound 因而失去信息——限制 parent 仅用于 manifest。
3. **import pruning 或 interface-diff 分类对 release 产生错误 breaking verdict**，需要 reviewer override——链在检查错误对象，应重新定义规格。

<a id="references"></a>

## 参考

- Flix, effect 多态: https://doc.flix.dev/effect-polymorphism.html
- Flix, "Fighting Supply Chain Attacks with Effect Systems" (OOPSLA 2026):
  https://2026.splashcon.org/details/oopsla-2026/120/Fighting-Supply-Chain-Attacks-with-Effect-Systems
- Scala 3 capture checking:
  https://scala-lang.org/api/3.x/docs/experimental/capture-checking/overview.html
- Roc functions: https://roc-lang.org/docs/main/langref/functions/
- 本地： `../almide-references/wado/docs/spec.md` (Effect System),
  `../almide-references/aver/docs/language.md`,
  `../almide-references/jacquard/README.md`,
  `../almide-references/research-2026/agent-verify-memory.md`
- 内部： #3238, `crates/almide-ir/src/effect.rs`,
  `crates/almide-codegen/src/pass_effect_inference.rs`,
  `src/cli/mod.rs::check_permissions`, `docs/wasm/capability-system.md`


<a id="implementation-note--categories-through-stdlib-callbacks-and-fn-values-4152-2026-10-25"></a>

## 实现注记 — category 经过 stdlib callback 与 fn value（#4152，2026-10-25）

D1 集合未能穿过 stdlib 高阶调用：`@intrinsic` function（`list.map`、`= _`）按空 body 扫描，因此 `list.map(paths, fs.read_text)` 的 `IO` 未计入任何人，只有 `Time` 权限的 manifest 也能 build 并读 file（#4152）。现在：

- 无 body 的 stdlib function 运行每个 fn 类型实参（像 D1 透明 parameter 一样计入 call site）且不保留任何一个；已对 `stdlib/` 每个 `@intrinsic` HOF 检查，没有谁在返回物中存 callback。type-variable parameter 收到的值按实参具体类型 pooled，结果按 call 类型从 pool 读取。不 lowering stdlib body 的 `almide check` 也从声明读取同一事实（`effect::stdlib_param_uses`）。
- 取为值的 stdlib fn 携带 category（`fs.read_text` → `IO`），若为 `effect fn` 也带 effect bit（ADR-0009、E109）。
- 逃逸值按 signature 而非 arity pooled（alias 已展开，`effect` slot 取 carrier form），generic body 不重新 pool type-variable 值；若读取类型使 signature 开放，则读取可能 alias 的每个 pool。这使 `(Handler) -> Handler` 上的 pure middleware chain 不会被计入所有 handler。
- stdlib function 的 category 计在使用处而非声明处，所以 `[permissions]` 不评判未使用的 bodyless declaration。

§Open 不变：六 category 词汇、声明 bound 语法、plain-fn bound 和 manifest versioning 未在此决定。

<a id="implementation-note--d2s-leaves-4155-2026-10-25"></a>

## 实现注记 — D2 的 leaf（#4155，2026-10-25）

`process` 原被分为 `Env`，因此为读取 `$HOME` 授予 `Env` 的 manifest 也能启动任意 command。D2 按所写实现：

- `Effect` 是14 leaf；`[permissions].allow` 接受 leaf 或四 parent 之一（`FS`、`Net`、`IO`、`Env`），授予其列出的 leaf。`Proc` 不在任何 parent 下。
- 按 operation：module 在 `STDLIB_MODULE_EFFECTS` 的行是默认；operation 不同的 function 在自己的声明写 `@category("FS.write")`（`fs.write_text`、`env.set`、`http.serve`、`process.args` 等）。
- output/abort builtin 带其 leaf（`println` 是 `IO.stdout`，`eprintln` 是 `IO.stderr`，`panic` 和失败 `assert*` 为 `Abort` + `IO.stderr`，`process.exit` 是 `Abort`）。D2 规定 inference 和 `[permissions]` 按 leaf 工作；§Open 的“plain fn bound”谈声明 bound，而非 manifest grant。
- `[permissions]` 评判 program 自身 function（entry 和 user module）；stdlib function 经计费到的 caller 判定，所以 `list.range_step` 内未使用 `panic` 不会拒绝任何东西。

D2 或 §Open 沉默之处采用保守选择：

1. **打开 socket 或 shared-memory region 上的 operation**（`net.tcp_read`、`unix_send`、`shm_write` 等）同时带 `Net.fetch` 和 `Net.listen`：handle 不说明哪一侧打开它。
2. **无 `proc = [...]` list 的 `Proc`** 在每条 route 都 warning，在 `--profile critical` 为 error。
3. **旧名继续解析**（`IO`、`Env`、`Net` 为 parent），但涵义缩小；被拒绝且旧义涵盖的 leaf 会打印 migration note。dialect-epoch-15 break 记录它。
4. **此处不变（§Open，统一词汇）**：`--profile critical --allow` 仍接受 `IO, Net, Env, Time, Rand, Process`。

<a id="implementation-note--dispatch-plain-host-fns-routes-4158-4159-2026-10-25"></a>

## 实现注记 — dispatch、plain host fn、route（#4158、#4159，2026-10-25）

- **protocol dispatch（#4158）。** 经 bound 调用 `effect` protocol method，从 pure fn 发出则 E006；每个 protocol 的 implementation effect 性必须匹配其声明，包括 `==` 和 derive 隐式调用的 convention。因此 pure caller 不会 dispatch 到 effect impl，集合无须 dispatch edge。
- **plain host fn（#4159）。** 作用于 host 的 plain `fn` 变为 `effect fn`。gate 拒绝 `All(_)` module 中 public plain fn，除非声明 `@category()`（不触达 host：`random` seeded generator、`env.os`），并拒绝任何 plain fn 上的 host category。`env.millis` / `unix_timestamp` / `sleep_ms` 为 `Time`。
- **不变 — plain-`fn` `@extern`（#4159，需裁定）。** 它对 `[permissions]` 仍携带全部 category（⊤），但 E006 和 effect bit 都不把其调用视为 effect：JS host 同步 import（`spec/wasm_host_js`）是从 pure fn 调用的 plain `@extern` fn，其 exported wrapper 被 host 作为 plain value 调用；把这些调用改为 effect 会改变 export ABI。两个保守选择——拒绝 plain `@extern`，或把 call 视为 effect——均对此模型 breaking，应属 ADR-0027 §5（extern bound）。
- **route。** `[permissions]` 来自 entry file 的 project（该处或祖先最近的 `almide.toml`），`check` 评判每个 user module，两个 `almide test` lane 和 `emit` 都应用它。

> 代码注释与示例译文（保留原代码）：apply(paths, …) 携带FS.read，apply(names, …) 不携带任何项。集合图为compiler推断集合 ⊆ almide.toml许可集合 ⊆ host grant的capability（wasm import / Porta）。诊断示例说明validate在FS.read上界下可能写文件，列出新增FS.write及validate→normalize→save_cache→fs.write_text路径；hint依次建议返回数据由caller保存、把save_cache移到caller、若有意写入再扩大上界至FS.read/FS.write。query示例注释为每个函数的集合（当时输出）、带FS.write的函数和全部路径、validate的category及各自路径。
