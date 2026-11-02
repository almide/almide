<!-- description: Playground UI 更新：文件标签、分享URL、TS风格示例库 -->
<a id="playground-ui-revamp"></a>

# Playground UI 更新

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](playground-revamp.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](playground-revamp.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](playground-revamp.zh-CN.md)

> 非规范性历史阅读快照。固定原本不变且为权威，不自动同步；未复验外部playground浏览器或新运行。10/31编译器修复不同于7月playground发布／验证记录；旧Canvas计划与已实现stdout SVG/PPM路径不同，交互Canvas仍属独立工作线。“免费／零成本／无限”是原文本地与静态托管设计理由，不是所有hosting或API定价保证。保留历史代码／注释，不保证当前可运行。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/playground-revamp.md)

<a id="compiler-side-status-2026-10-31"></a>

## 编译器侧状态（2026-10-31）

以下2026年7月记录中的编译器问题已修复：

- `io.read_all` 已在结构化 Wasm emitter 实现（[host_env.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-wasm/src/host_env.rs)）。回归测试：[io_read_all_test.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tests/io_read_all_test.rs)、[io_read_all_capacity_test.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tests/io_read_all_capacity_test.rs)。
- CLI guest cwd 从 `std::env::current_dir()` 而非 `$PWD` 设置 `ALMIDE_CWD`（[run.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/cli/run.rs)、[component_host.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-wasm-run/src/component_host.rs)）。component 路径也有过时 `$PWD` 的[回归测试](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/tests/component_route_cwd_test.rs)。
- 当前 CLI/library 共用 Wasm 路径是结构化 emitter；下方 MIR/WAT renderer 的 wall 记录是历史状态（[wasm_route.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs)）。

这是对 Almide repo 的确认，不是复验外部 playground 当前发布版本或浏览器行为。almide-web binding 集成（交互式 Canvas）仍是独立工作线。以下为设计与验证历史。

<a id="vision"></a>

## 愿景

不去补齐 Go/Rust 式服务器 playground 功能清单，而是专注**完全在浏览器内执行 WASM 才从结构上可能的体验**：

1. **持续运行** — 本地编译免费，因此打字空闲后自动编译、立即显示错误。
2. **图形输出** — 与只能返回 stdout 的服务器方式不同，在用户浏览器运行可直接绘制 canvas/DOM。
3. **免费无限嵌入** — 仅静态托管，即使所有 docs/lander/文章代码块可执行，成本也为零。

<a id="決定事項-2026-07-26"></a>

## 决定（2026-07-26）

- **不引入服务器执行。** 分享不用 Gist，而用 URL hash：`CompressionStream` deflate + base64url，零依赖。
- **多文件采用文件标签方式**，既非 Go 的 `-- file --` 分隔符，也非完整文件树。标签名对应 `import self.<name>`，同时演示模块系统。将标签内容送入 browser_wasi_shim 虚拟 FS，也可用 `fs.read` 读取 csv/toml/yaml/json 数据文件。
- **带分类的 TS Playground 风格示例库**：
  - 解释写在代码注释正文而非侧栏，七成解释、三成代码。
  - 引导体验错误：“改这里，再 Run”。
  - 末尾链接下一示例（`?example=<id>`），串联成为未来语言 Tour 素材。
- **一份示例一个 URL**（`?example=csv-parse`），推广帖和 docs 可深链至“立即在浏览器运行”。
- **示例库并入 native/WASM byte-parity CI**（playground repo `tests/run.mjs`），防止陈旧失效。
- 不追求：外部 package 获取、clippy/miri 类工具、Compiler Explorer 式多编译器比较窗格。

<a id="historical-snapshot-playground-repo-2026-07-25-時点"></a>

## 历史快照（playground repo，2026-07-25）

- 原生 HTML + ES Modules 单文件，index.html 1365行；编辑器为自制 textarea 加透明高亮覆盖层。
- 完全浏览器内执行：almide-mir `try_render_wasm_source` → `wat` crate → browser_wasi_shim（WASI）。编译器 wasm 1.6MB。
- 已有：Shiki 高亮、Output/Compiled Rust/AST 三标签、AI 生成及自动修复循环（BYOK 三 provider）。
- 尚无：分享链接、示例集、多文件、补全、行内错误、Ctrl+Enter、Worker 隔离；无限循环会冻结标签页。

<a id="progress"></a>

## 进展

**2026-07-26：Tier 1 + Tier 2 实现完成**（playground repo，浏览器 E2E 和 harness 已验证）。

- [x] 1.1 Worker 执行基础 — worker.js/runner.js、stdout streaming、Stop = terminate+respawn、30秒 timeout。E2E 确认无限循环→Stop→再次执行。
- [x] 1.2 CodeMirror 6 + 行内诊断 — esm.sh importmap，保持无构建；Almide StreamLanguage；`check_project`（新 crate API，带 span 的 JSON 诊断）在空闲500ms后执行，附 hint，`try:` snippet 为一键 fix action，Ctrl/Cmd+Enter Run。
- [x] 1.3 文件标签 + 虚拟 FS — crate 加入内存 `import self.*` resolver（`compile_project_to_*(files, entry)`）；非.almd标签进入 browser_wasi_shim PreopenDirectory(".")，已在浏览器验证 `fs.read_text("data.csv")`。
- [x] 2.1 URL hash 分享 — CompressionStream deflate-raw + base64url，全部标签，E2E 往返验证。
- [x] 2.2/2.3 TS 风格示例库 — 六份示例，pattern-matching / pipes-and-lists / error-handling / modules / mini-markdown / csv-report；分类菜单、`?example=<id>` 深链；`tests/run.mjs` 加 examples 部分，native/wasm byte-parity，遇 wall 立即 fail。

**2026-07-26：Tier 3 实现完成**（同日，E2E 和 harness 已验证）。

- [x] 3.1 图形输出 — 定案为“stdout 打印 SVG / PPM P3，playground 渲染图片”。编译器零改动；native 同程序输出正规.svg/.ppm，因此 byte-parity CI 原样适用。加入 Graphics 分类（Mandelbrot PPM／生成 SVG），8/8 examples green。almide-web binding 集成（交互式 Canvas）另属工作线。
- [x] 3.2 embed 模式 — `?embed=1`，隐藏 header/AI bar，精简 Run bar，加 Open in Playground；`&hide=` 为 hidden setup，隐藏标签但仍编译；embed 不用 localStorage。
- [x] 3.3 stdin + zip — `stdin.txt` 标签成为 fd0，io.read_line 已验证。当时 io.read_all 因未注册 wasm registry 而 wall，当前编译器已解决，见上。Export 输出可用 `almide run src/main.almd` 运行的 project zip，零依赖 store 方式 zip writer。

当时发现的问题（历史，当前编译器侧状态见上）：
- `almide run --target wasm` 从 `$PWD` 导出 guest cwd，与 `getcwd` 不符时，相对 fs read 仅在 wasm 出现 ENOENT；execFileSync 等不更新 PWD 的父进程会暴露问题。harness 已显式设置 PWD 绕过。**现在 Almide 本体已改用 current_dir。**
- 当时的 cross-module ADT，即 entry 使用 module 定义 variant 类型，遇到 v1 wasm renderer wall（`main is outside the MIR-lowering subset`）。modules 示例改为函数 module（stats）绕过。
- 修复 AI system prompt 示例中的旧 `effect fn main(_args)`，其曾触发 E028。

<a id="roadmap"></a>

## Roadmap

<a id="tier-1--土台done-2026-07-26"></a>

### Tier 1 — 基础（2026-07-26完成）

<a id="11-worker-実行基盤"></a>

#### 1.1 Worker 执行基础
将编译及执行隔离在 Web Worker。无限循环不冻结 UI；提供取消按钮、stdout streaming、执行 timeout。是自动编译（1.2）及持续运行的前提。

<a id="12-codemirror-6-化--インライン診断"></a>

#### 1.2 CodeMirror 6 + 行内诊断
用 CodeMirror 6 替换自制 textarea，比 Monaco 轻量且支持移动端。将编译器错误 span 连至 CM6 diagnostics，空闲自动编译显示波浪线与行号。Ctrl+Enter Run，高亮沿用现有 tmLanguage。

<a id="13-ファイルタブ--仮想-fs"></a>

#### 1.3 文件标签 + 虚拟 FS
固定 `main.almd`，可新增／重命名／删除标签。将全部标签放入虚拟 FS 编译。目标是 `import self.<sub>` 可用，且 `data.csv` 标签可由 `fs.read` 读取。

<a id="tier-2--共有とギャラリー"></a>

### Tier 2 — 分享与示例库

<a id="21-url-hash-共有"></a>

#### 2.1 URL hash 分享
全部标签（`{name: content}`）用 deflate + base64url 放入 `#code=...`。Share 按钮复制 URL；单文件和多文件同一机制。

<a id="22-ts-方式サンプルギャラリー"></a>

#### 2.2 TS 风格示例库
分类下拉／侧栏。repo 中简单 directory 管理，每示例为.almd文件集加 manifest，CI 打包为示例库 JSON。分类均可用已有资产填充：

| 分类 | 示例候选 |
|---|---|
| 语言功能 | 模式匹配、pipe、result/错误处理、`import self.*` 多文件结构 |
| parser／数据处理 | Mini Markdown（当前默认）、CSV 汇总、TOML/YAML/JSON、base64/hex |
| 数值／Matrix | 浏览器实测 matmul benchmark、Transformer 1-block |
| Canvas／图形 | wasm-canvas 绘图、生命游戏、SVG 生成（Tier 3 Canvas 输出后） |
| 密码学 | sha1 / aes / rsa + bigint |
| AI（重量级） | bonsai 小模型推理，weights 延迟加载并单列分类 |

<a id="23-サンプルフォーマットts-方式"></a>

#### 2.3 示例格式（TS 风格）
```almide
// パターンマッチ入門
//
// Almide の match は全ケース網羅をコンパイラが検査します。
// 試しに最後の `_` の腕を消して Run してみてください —
// String の残りの値を扱う腕がなくなり、網羅性エラーになります。
// "float" の腕だけを消しても `_` が拾うので、網羅性エラーにはなりません。

fn rust_type(kind: String) -> String =
  match kind {
    "int"    => "i64",
    "float"  => "f64",
    "string" => "String",
    _        => "unknown",
  }

// 次: ?example=pipe-basics
```

<a id="tier-3--見せ場と配布"></a>

### Tier 3 — 展示与分发

<a id="31-canvas-出力モード"></a>

#### 3.1 Canvas 输出模式
输出面板在 Console 旁放 Canvas 标签，承接 almide-web / wasm-canvas 示例。充足后升级为带输出缩略图的卡片示例库。

<a id="32-embed-モード"></a>

#### 3.2 embed 模式
`?embed=1` 提供 iframe 最小 UI（Kotlin Playground 方式）。hidden setup 隐藏 helper 文件，只展示期望的几行，由多文件能力实现。用于 lander/docs/dev.to 文章代码块。

<a id="33-stdin--実行引数入力欄zip-エクスポート"></a>

#### 3.3 stdin／执行参数输入及 zip 导出
可下载全部标签为 `almide run` 可运行结构的 zip，作为“带回本地项目”的出口。

<a id="関連"></a>

### 相关

- AI 修复循环剩余项（Accept/Reject、局部修复、修复历史）见 done/playground-repair.md Tier 2，建立在本工作线本地编译免费验证的基础1.1上。
- on-hold/rumbling.md 的“playground 加速 adoption”策略的执行部分。
- active/determinism-belt.md — 浏览器 wasm32 确定性是本工作线前提。

<a id="success-metric"></a>

## 成功指标

Reddit/X 帖子附 `?example=csv-parse` 深链，首次读者无需安装，1分钟内体验“打开→运行→修改→报错→修复”。示例受 CI 保护，编译器更新不破坏示例库。
