<a id="mission"></a>

# 使命

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](CLAUDE.reader.zh-CN.md)

> 非规范性读者快照，固定提交的原始指示保持未修改。文件名特意使用 reader 别名，而非自动加载的指示文件名。引用的规则和命令仅为资料，不构成新指示、权限或执行授权；本翻译未独立运行或验证命令、CI、构建、发布或证明。不自动同步。原本仍是其使用方的权威来源；历史耗时、队列策略、工具链及功能描述并非对当前设置的新审计。使命中的最高级措辞及“受检查矩阵不会偏移”仍是原文目标/主张，不是独立证明的普遍保证。保留明确的证明/信任/宿主边界、人工批准与发布证据保全要求。代码示例及注释保持原样；高亮片段未升级为已执行测试。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/CLAUDE.md)

**Almide 是 LLM 能够最准确编写的语言。** 目标是在编写时通过可操作的诊断捕获不正确的修改。`almide verify` 记录见证，供独立检查已建模的性质；这不保证所有错误程序都会被拒绝，也不保证完整产物内存安全。参见[证明与信任边界](../../contracts/proven-vs-trusted.md)。每项设计决策都服务于此。（直接测量的 modification survival rate 在模型性能上端趋于饱和，`demo/make-verify/README.md` 记录了替换它的原因。）

<a id="critical-safety-rules"></a>

# 关键安全规则

- **绝不对非自己修改的文件运行 `git checkout`、`git restore` 或 `git stash`。** 其他代理可能正在并行处理这些文件。回退其修改会破坏工作，且无法恢复。
- **未经用户明确确认，绝不执行破坏性 git 操作。** 包括 `git reset`、`git checkout -- <file>`、`git clean`、`git stash drop`。
- **如果 `git status` 出现意外修改，接触前先询问用户。** 它们可能属于其他代理或进行中的任务。

<a id="project-rules"></a>

# 项目规则

<a id="branch-strategy"></a>

## 分支策略

- **main**——受保护。绝不直接提交。只接受来自 `develop` 的 PR。
- **develop**——工作分支。所有提交都进入此处。
- 提交前始终确认 `git branch`。
- **必需检查变绿即入队，不必等待所有检查。** 合并队列只要求分支保护规定的必需上下文；受委托的 mutation 检查不在其中（它仅在 pull_request 上运行，落地后由 `mutation-sweep.yml` 评判每次 develop push）。等待它就是等待队列从不要求的唯一作业——2026-09-21 测得它需 56 分钟，而最慢必需 shard 为 27 分钟。使用 `scripts/enqueue-when-required-green.sh <pr> [--wait]`：从 API 读取必需集合，仅对该集合比较 PR 汇总状态，再通过 GraphQL mutation 入队。以 `gh pr merge --auto` 启用自动合并也会自行入队，但要等所有检查变绿（其 method 在 REBASE 队列上显示 MERGE；GitHub 会规范化该字段，无害）——脚本能在更早时点入队。PR 的 mutation 检查为红，事后仍应作为 develop sweep 发现的问题阅读。绿色检查如果打印 `PARTIAL: ran N of M
  in-scope mutants`，实际只运行了 N 个（28 分钟预算，#2814）；列名的延期项由 develop sweep 而非 PR 评判。

<a id="git-commit-rules"></a>

## Git 提交规则

- 提交消息**仅用英语**。
- 不加前缀（feat:、fix: 等）。
- 保持为简洁的一行。
- 聚焦改了什么，而非为什么。
- 提交消息必须**仅用英语**（由 `english-only` commit-msg hook 强制）。

<a id="release-procedure"></a>

## 发布流程

发布 workflow（`.github/workflows/release.yml`，由 `v*` 标签 push 触发）**负责创建 release**。推送标签后不要手动 `gh release create`，否则会与 workflow 竞争，导致其步骤以“a release with the same tag name already exists”失败。

正确流程：

1. 在 `develop` 提升 `Cargo.toml` 版本，提交、推送。
2. 等待 `develop` CI 变绿。
3. PR `develop → main` 并合并（要求绿色 CI，不要强制合并发布）。
4. `git tag vX.Y.Z <merge-commit>` 及 `git push origin vX.Y.Z`。
   - **发布阻碍项限制最终标签**（#1482）：若带 `I-unsound` / `I-miscompile` / `I-divergence` / `regression` 标签的 issue 仍开放，workflow 拒绝正式发布；闭合标签集合见 [docs/project/ISSUE-TAXONOMY.md](../../project/ISSUE-TAXONOMY.md)。提前用 `bash scripts/count-release-blockers.sh` 检查。
   - **RC 渠道**（#1484）：包含语言公开功能或编译器行为变化的发布，先打 `vX.Y.Z-rc1` 标签。它作为 GitHub PRERELEASE 发布（不纳入“latest”；阻碍检查打印但不失败），提供稳定性观察窗口。观察无问题后再打正式 `vX.Y.Z`；窗口长度由每次发布的人工作出判断。
   - **下游 canary**（#2839）：打 `vX.Y.Z-rc1` 以及正式标签之前，都在候选提交上运行 `gh workflow run downstream-canary.yml -f candidate_ref=<sha>`（baseline 默认为最新非 rc 发布）。它以两套编译器构建约 45 个真实公开下游项目的 HEAD（`scripts/downstream-canary.toml`）。**任意 REGRESSION 行都会阻止打标签，直到完成分诊**：创建带 `regression` 标签的 issue；若破坏是有意的，则记录为 `proofs/dialect-epochs.toml` 条目。`infra` 行不是判定，应重跑该路径。两边都失败、但在更早发布成功的命令（已知正常台账 `proofs/downstream-canary-known-good.toml`）也属于 REGRESSION（`good/FAIL`）——已经发布的破坏仍是破坏。仅在 canary 为 CLEAN 后推进台账并提交：`bash scripts/downstream-canary-record.sh <run-id>`。其他情况不推进。
   - **接口差异**（#1488）：正式标签前运行 `bash scripts/check-interface-diff.sh vPREV vX.Y.Z`。它根据已提交的签名索引，将标准库公开面分类为 identical / additive / breaking，除非以 `--allow-breaking` 声明，否则拒绝破坏性差异（移除需要 `@deprecated` 窗口；可能破坏已有代码时，还需 `proofs/dialect-epochs.toml` 条目）。
5. **打标签前写发布说明**——`docs/releases/vX.Y.Z.md`（每个 `-rc` 标签也一样），经 `develop` → `main` 落地，使标签提交包含它。第 1 行 `# vX.Y.Z — <headline>` 是发布标题，其余为按主题组织的正文，绝不使用自动生成的“What's Changed”PR 列表。格式见 [docs/releases/README.md](../../releases/README.md)。用 `bash scripts/check-release-notes.sh vX.Y.Z` 检查。
6. **由 workflow 创建 release。** 它从该文件发布标题和正文，文件缺失或格式不正确则在构建前拒绝标签。之后的更正先修改文件，再运行 `bash scripts/check-release-notes.sh vX.Y.Z out && gh release edit vX.Y.Z --title "$(cat out/title)" --notes-file out/body.md`。
7. **刷新固定记录的台账计数**——运行 `bash scripts/gen-ledger-counts.sh`，提交 `proofs/ledger-counts.toml` 及其重新渲染的四篇文档。fixture/契约 PR 不刷新这些计数；另一个运行时机是夜间 `scripts/check-ledger-counts.sh` 变红时。
8. **封存发布证据**（审计冻结）：运行 `bash scripts/release-seal.sh gen vX.Y.Z`，填写 `[recorded]` 字段（release-gate fuzz 运行、asset 清单），在 `develop` 提交 `proofs/releases/vX.Y.Z.toml`。此后 CI 持续针对标签重新测量所有 `[derived]` 字段——seal 是该发布的不可变证据记录。

发布失败时，首先区分从未发布的构建与已经公开的 release。构建失败本身不授权删除已公开 release 或标签。已提交的 seal 要求原标签保持可获取；删除会破坏证据检查。

旧的“删除后发下一补丁”方案没有解决这一约束。在撤回或破坏性恢复前，须取得所有者明确批准的计划，涵盖已发布标签、asset、attestation 和 seal。本清单不决定证据保全的例外。在 `develop` 准备修复，采用正常的带版本发布流程；不要静默复用已发布版本。

<a id="development-setup"></a>

## 开发环境设置

克隆后获取子模块并安装 git hooks：

```bash
git submodule update --init --recursive
brew install lefthook  # or: https://github.com/evilmartians/lefthook
lefthook install
```

pre-push clippy 棘轮仅在 CI 固定工具链（1.96.0）下给出判定；非固定版本会打印 `NO VERDICT` 并跳过，因为不同 clippy 发布的计数不可比。在 rustup 管理工具链的环境，`rustup toolchain install 1.96.0 --component
clippy` 可使其给出判定；qusp 管理的机器（无 rustup）则仍只有 CI 给出判定——那里绿色 push 绝不代表 clippy 判定。同一 hook 的 run_parity 在所有环境都给出判定。

未执行 `lefthook install` 时，pre-commit 和 pre-push 检查完全不会运行；hook 本可数秒发现的每个失败，都会消耗一轮 CI（30–56 分钟）——2026-09-21 测得一晚三轮。会话首次 push 前执行 `which lefthook`。

子模块（`actions/checkout` 不获取它们，因此 CI 看不到——仅为本地便利设施，构建不依赖其中任何一个）：

| 路径 | 仓库 | 使用方 |
|---|---|---|
| `grammar/` | [almide/almide-grammar](https://github.com/almide/almide-grammar) | 编辑器 / tree-sitter 仓库使用的语法定义 |
| `research/benchmark/lang-bench/upstream/` | [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) | `/almide-lang-bench` |

<a id="project-overview"></a>

## 项目概述

Almide 是一种编程语言（.almd 文件），使用纯 Rust 编译器编译，具有双目标代码生成（Rust、WASM）。

- **架构**：[docs/ARCHITECTURE.md](../../ARCHITECTURE.md)——编译器流水线、模块映射。
- **语言参考**：[docs/CHEATSHEET.md](../../CHEATSHEET.md)——语法、标准库、惯用法（供 AI 代码生成）。
- **标准库**：`stdlib/` 中自托管的 `.almd` 模块；注册表位于 `crates/almide-types/src/stdlib_info.rs`，逐模块文档见 [docs/stdlib/](../../stdlib/)。
- **模块系统**：[docs/specs/module-system.md](../../specs/module-system.md)——导入、子模块、菱形依赖。
- **包系统**：[docs/specs/package-system.md](../../specs/package-system.md)——依赖管理、MVS、版本共存。

<a id="building--installing"></a>

## 构建与安装

修改编译器源码后始终重新构建并安装，以更新 PATH 中的二进制：

```bash
make install   # cargo build --release + install to ~/.local/bin/almide
```

<a id="usage"></a>

## 用法

跨目标输出比较适用于受支持的契约及匹配的宿主/输入前提；参见[等价性范围](../../design/EQUIVALENCE.md)。

```bash
cargo build --release

almide run app.almd              # Compile + execute
almide run app.almd --target wasm  # Execute on embedded Wasmtime; parity has contract/host conditions
almide run app.almd -- arg1 arg2 # Program args go after --
almide build app.almd -o app     # Build binary
almide build app.almd --target wasm  # Build WASM
almide test                      # Find all .almd with test blocks (recursive)
almide test spec/lang/           # Run tests in a directory
almide test spec/lang/expr_test.almd  # Run a single test file
almide test --run "pattern"      # Filter tests by name
almide bench app.almd            # Verify-then-time (median; --target wasm for the embedded host)
almide compile                    # Module interface (project)
almide compile parser             # Module interface (by name)
almide compile app.almd --json    # Module interface (JSON)
almide check app.almd             # Type check only
almide check app.almd --target wasm# + the wasm build route: E081/E082 at check time (#1922)
almide check app.almd --profile critical --allow IO  # Critical profile (#567): bounded rules on every fn, capabilities deny-all
almide fmt app.almd               # Format source
almide fmt --check spec/          # Formatting gate (non-zero on drift)
almide fmt --no-import-edit stdlib/  # Format WITHOUT touching imports (splice-context sources)
almide clean                     # Clear dependency cache + native build scratch (#2500)
almide add almide/pkg@v0.1.0    # Add dependency (github.com/almide/ default)
almide update [dep]              # Advance a locked git dep to its ref's remote head (tags never move)
almide deps                      # List dependencies
almide dep-path bindgen          # Print cached source dir of a dependency
almide app.almd --target rust    # Emit Rust source
almide app.almd --target rust --repr-c  # Emit with #[repr(C)]
almide app.almd --emit-ast       # Emit AST as JSON
```

<a id="test-structure"></a>

## 测试结构

`almide test` 递归寻找包含 `test` 块的全部 `.almd` 文件。

- **内联测试**：在任意 `.almd` 文件写 `test "name" { }`。
- **测试文件**：专用测试文件使用 `*_test.almd` 后缀（惯例）。

```
spec/
├── lang/            Language feature tests (*_test.almd)
├── stdlib/          Stdlib tests (*_test.almd)
└── integration/     Multi-module / integration tests
tests/               Rust compiler unit tests (.rs, Cargo auto-discovery)
```

运行测试：
```bash
almide test                      # All .almd with test blocks (recursive)
almide test spec/lang/           # Language tests only
almide test spec/stdlib/         # Stdlib tests only
```

<a id="testing-rules"></a>

## 测试规则

编译器变更必须通过**全部练习及测试**验证：

```bash
almide test
```

添加或修改标准库函数时（标准库已自托管——`stdlib/defs/*.toml` 和 `runtime/rust/` 不再存在）：
- 在 `stdlib/<module>[_<part>].almd` 中以纯 Almide 实现。
- 为获得 WASM/v1 覆盖，在 `stdlib/<module>_<part>.almd` 的 `fn` 正上方声明 `@implements("module.fn")`。构建生成注册表行（`crates/almide-types/src/generated/implements.rs`）；其所在文件从不触及 `prim.` 的函数体，无需列表条目即可获准进入 wasm 路径。不要向 `self_host_registry.rs` 添加行，也不要向 `crates/almide-wasm/src/whitelist.rs` 添加名称：两者都是只允许缩减的旧名称列表（`tests/name_keyed_sites_test.rs`）。必要时调整 `crates/almide-mir/src/purity.rs`。未链接标准库调用在 wasm 渲染器中是拒绝错误。
- 仅在需要原生 intrinsic 时，在 `runtime/rs/src/<module>.rs` 实现 `almide_rt_*`，并在模块 `.almd` 中以 `@intrinsic("almide_rt_*")` 声明。
- 新模块：更新 `crates/almide-types/src/stdlib_info.rs` 中的 `STDLIB_MODULES` / `BUNDLED_MODULES` / `bundled_source()`（若自动导入，也更新 `AUTO_IMPORT_BUNDLED`）。
- 在 `spec/stdlib/` 写测试（`*_test.almd` 或内联 `test` 块）；添加 almide-interp 桥接代码，使三方 oracle 覆盖它。
- **API 家族按矩阵扩展，绝不逐点处理。** 若函数属于某个家族（数值转换三件套 `to_T`/`to_T_saturating`/`to_T_checked`、逐类型边界、Codec 覆盖等），说明家族完备性规则（哪些单元必须存在，哪些省略是有意的，例如有损⇒三件套、无损⇒仅普通版），并在同一 PR 落地或扩展断言该规则的可执行矩阵检查。手工维护形状的公开面会偏移，受检查的矩阵不会。

修改代码生成时：
- 测试所有权：在 `for...in` 后使用的变量必须仍然可用。
- 测试 effect 函数（ADR-0008）：传播是显式的——`fs.read_text(p)!` 编译并传播；裸 `fs.read_text(p)` 语句是 E042（must-use），无标注的 `let x = fs.read_text(p)` 是 E041；`let _ = f()` 丢弃且不传播。
- 测试生成的 Rust 无警告编译。

<a id="writing-idiomatic-almide"></a>

## 编写惯用 Almide

编写 `.almd` 代码（标准库、包、示例）时遵循这些惯用法：

<a id="prefer-match-over-ifelse-chains"></a>

### 优先 match，而非 if/else 链
```almide
// ✗ avoid
if kind == "int" then "i64"
else if kind == "float" then "f64"
else if kind == "string" then "String"
else "unknown"

// ✓ use match
match kind {
  "int"    => "i64",
  "float"  => "f64",
  "string" => "String",
  _        => "unknown",
}
```

<a id="prefer-list-combinators-over-var--for"></a>

### 优先列表组合器，而非 var + for
```almide
// ✗ avoid
var result: List[String] = []
for item in items {
  result = result + [transform(item)]
}
result

// ✓ use map / flat_map / filter_map
items |> list.map((item) => transform(item))

// ✓ with index: list.enumerate — destructure the pair in the parameter
cases |> list.enumerate |> list.map(((idx, case)) => "${idx}: ${case}")

// ✓ FALLIBLE body: the callback's `!` instantiates the fallible form
//   (first-err short-circuit, ADR-0006) — never var + for
files |> list.map((f) => read_meta(f)!)!
```

<a id="prefer-listfind-over-var--for-search"></a>

### 优先 list.find，而非 var + for 搜索
```almide
// ✗ avoid
var result = json.null()
for t in types {
  if get_str(t, "name") == name then result = t else result = result
}
result

// ✓ use list.find
types |> list.find((t) => get_str(t, "name") == name) ?? json.null()
```

<a id="prefer-recursion-over-var--while--flag"></a>

### 优先递归，而非 var + while + 标志
预先知道迭代次数时，写范围 `for`（`for _ in 0..<n`），通过 `var` 传递状态。递归用于提前退出（`break`/`continue` 形式）及事先不知道边界的循环，而非计数型 `f(n - 1)` 循环。
```almide
// ✗ avoid
var i = p
var go = true
while i < len and go {
  let c = peek(t, i)
  if is_ws(c) then { i = i + 1 }
  else { go = false }
}
i

// ✓ use recursion
fn skip_ws(t: String, p: Int) -> Int =
  if p < string.len(t) and is_ws(peek(t, p)) then skip_ws(t, p + 1)
  else p

// ✓ or use scan_while for common patterns
fn scan_while(t: String, p: Int, pred: (String) -> Bool) -> Int =
  if p < string.len(t) and pred(peek(t, p)) then scan_while(t, p + 1, pred)
  else p
```

<a id="use-heredoc-for-static-text-blocks"></a>

### 静态文本块使用 heredoc
```almide
// ✗ avoid: array of strings joined
let code = [
  "#[no_mangle]",
  "pub extern \"C\" fn alloc(len: i32) -> *mut u8 {",
  "    let buf = Vec::<u8>::with_capacity(len as usize);",
  "    buf.as_mut_ptr()",
  "}",
] |> list.join("\n")

// ✓ use heredoc: no escapes, reads like actual code
let code = """
  #[no_mangle]
  pub extern "C" fn alloc(len: i32) -> *mut u8 {
      let buf = Vec::<u8>::with_capacity(len as usize);
      buf.as_mut_ptr()
  }
  """
```

<a id="use-pipe-for-data-transformation-chains"></a>

### 数据变换链使用 pipe
```almide
// ✓ pipe chains
fields
  |> list.map((f) => "${get_str(f, "name")}: ${go_type(get_type(f))}")
  |> list.join(", ")
```

<a id="use--for-fallback--for-resultoption--for-unwrap"></a>

### ?? 用于回退，? 用于 Result→Option，! 用于 unwrap
```almide
value.get(v, key) ?? default_val      // Result/Option fallback
json.get(v, "field")?                  // Result → Option
parse_int(s)!                          // unwrap and propagate into the current fallible fn or lambda
```

<a id="imports"></a>

### 导入
- 标准库模块（`string`、`int`、`float`、`list`、`value`、`map`、`set`、`math`、`datetime`、`bytes`、定宽整数等）自动导入——不要写 `import string`。权威列表：`import_table.rs` 的 seed + `stdlib_info.rs` 中的 `AUTO_IMPORT_BUNDLED`。
- `json`、`fs`、`http`、`env`、`io`、`random`、`regex`、`process`、`testing`、`url` 需要显式 `import`。
- 外部包需要 `import pkg_name`。
- 包自引用：`import self as pkg_name`。

<a id="key-design-decisions"></a>

## 关键设计决策

- **多目标**：同一 IR 通过 `--target rust|wasm` 生成 Rust 或 WASM（TS 代码生成已移除）。
- **Codegen v3**：Nanopass 流水线（语义改写）+ TOML 模板渲染器（语法）。
- **Effect fn（Rust）**：调用 `effect fn f() -> T` 默认得到 `Result<T, String>`。显式 `Result[T, E]` 返回类型保留其错误类型。纯可失败函数为 `-> T!` / `-> T!E`。传播**仅显式**（ADR-0008）——`!` 在 Rust 侧的实现取决于上下文（Result/Option 传播、test 中的 unwrap 等）。包括 lambda 边界的详情见[结果、Option 与 effect 规格](../../specs/result-option-effect.md)中的上下文表。隐式传播是 E041，在 statement 位置静默丢弃是 E042。
- **`==`/`!=`**：普通二元相等比较生成 Rust 的 `almide_eq!`。convention `eq` 和 `Secret` 比较在此之前由 lowerer 解析为函数调用（`crates/almide-frontend/src/lower/expressions_access.rs`）。
- **`+`**：字符串与列表拼接（与加法重载）。
- **诊断**：每个错误都包含 file:line、上下文和可操作提示。

<a id="repo-boundary-almide-vs-almide-dojo"></a>

## 仓库边界：almide 与 almide-dojo

- **本仓库**＝编译器正确性。`spec/` 测试、`cargo test`、grammar-lab 实验、lang-bench。
- **[almide/almide-dojo](https://github.com/almide/almide-dojo)**＝LLM 编写便利性。日常 MSR 测量、任务库、恶意提示检测、诊断反馈循环。
- 所有 MSR 工作进入 Dojo。`research/benchmark/msr/` 和 `research/benchmark/framework/` 已从本仓库移除（2026-08-08）——自 2026 年 4 月交接以来，本地框架的 `results/` 为空，运行它会用本地测量值覆盖 Dojo 公开数值。可通过 `git log --diff-filter=D -- research/benchmark/msr/` 恢复。
- 仍保留且活跃：`research/benchmark/perf/`（由 `scripts/check-perf-ratio.sh` 检查）、`research/benchmark/lang-bench/`、`research/benchmark/exercises/`（供 `tools/v1_gap_measure.py` 的语料）、`research/grammar-lab/`、`research/spike/`。
- 桥接：Dojo 的 PR 检查将作为本仓库 CI 的一部分运行任务子集（未来）。

<a id="behavior-contracts"></a>

## 行为契约

每项可观察的跨目标承诺（原生 Rust ⇄ wasm32：stdout、stderr、退出码）都是 [docs/contracts/contracts.toml](../../contracts/contracts.toml) 中具名的 `[[contract]]`，可追溯到可执行证据：`spec/wasm_cross/*.almd` fixture、差分 fuzz、生成期 Σ-probe 或 Lean 定理。索引是 [docs/contracts/README.md](../../contracts/README.md)（自动生成）。

**修改可观察行为＝在同一 PR 更新契约台账。**

- 新行为＝新的 `C-NNN` + 至少 1 个 fixture，fixture 以 `// @contract: C-NNN` 头部行声明它（反向链接强制且对称）。
- 移除差异＝在同一 PR 将 `status` 改为 `active`、移除标志、降低棘轮。`flagged-for-revision` 计数只允许下降。
- **哪些已证明，哪些依赖信任**：[docs/contracts/proven-vs-trusted.md](../../contracts/proven-vs-trusted.md)——边界图，以及每个检查主张和不主张什么。
- `scripts/check-contracts.sh`（CI `checks` 作业 + lefthook pre-commit hook）强制每个契约具有级别不低于 fixture 的证据、每个 fixture 指明其契约、链接双向。证据类别词汇通过 `scripts/lib/contract-classes.txt` 与 rt-oracle-registry 共享。
- **新的 `C-NNN` 首先进入 [almide/als](https://github.com/almide/als)。** judge 持有规范台账；`scripts/check-als-pin.sh`（CI `checks`）拒绝任何未出现在 `proofs/als-pin.txt` 固定提交所对应 judge 台账中的契约 ID。顺序为：als PR（契约 + fixture）合并 → 单独提交推进 pin → 实现 PR。
- **汇总计数是固定记录的，不在每个 PR 重生。** 生成文档引用的所有“N contracts / N fixtures”总数，都从 `proofs/ledger-counts.toml` 渲染到带日期的 `counts:generated` 块。fixture/契约 PR 重新生成结构台账（行、manifest、索引），但不动该块；在发布时，或夜间 `scripts/check-ledger-counts.sh` 报告偏差时，由 `bash scripts/gen-ledger-counts.sh` 刷新。

<a id="the-llm-facing-surface-is-the-stable-surface-1483"></a>

## 面向 LLM 的公开面就是稳定公开面（#1483）

`docs/CHEATSHEET.md` 和 `llms.txt` 是模型作为训练与上下文数据读取的内容——**凡出现于其中的都是承诺**，无意承诺的内容不得出现。主要读者无法察觉文档与现实的差距，因此以检查约束：

- 标为 ```` ```almide check ```` 的 fence 断言“这是完整、可检查的程序”，`scripts/check-llm-surface.sh`（CI）对每段运行 `almide check`——失效示例应导致 CI 失败，而不是成为训练语料缺陷。普通 ```` ```almide ```` 仍仅是片段与有意 ✗ 示例的高亮标签。
- 扩大覆盖意味着补全示例并提升其标记，每次处理一个 fence。检查的允许列表只能缩减。
- cheatsheet 中新记录的语言公开功能，在同一变更中落地 spec 覆盖——将上述契约台账规则应用于文字。

<a id="documentation"></a>

## 文档

- 语言规格：`docs/specs/`——规则见 [docs/specs/CLAUDE.md](../../specs/CLAUDE.md)。
- 编译器设计：`docs/ARCHITECTURE.md`。
- 语言参考：`docs/CHEATSHEET.md`。
- 路线图：`docs/roadmap/`。
- 行为契约：`docs/contracts/`——跨目标等价性的契约台账。
