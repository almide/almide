# 用 Almide 编写的项目

[English](./written-in-almide.md) | [日本語](./written-in-almide.ja.md) | [简体中文](./written-in-almide.zh-CN.md)

标准库是本仓库中最大的 Almide 程序。以下项目展示其他地方的用途。这是一份目录，并不声称所有列出的平台、宿主或包都已使用当前编译器构建和测试。下方仓库数与代码行数统计来自 2026-09-30 的观测。

2026-10-31 进行的针对性源码检查：
- [Bindgen 的目标注册表](https://github.com/almide/almide-bindgen/blob/b563e8b9076ff511b9b20b03e2c2e475af2c4c0b/src/mod.almd)列出了包括 Almide 在内的 21 种语言。这是源码清单，不代表 21 种分别测试过的绑定。
- [Porta 的架构](https://github.com/almide/porta/blob/123bef319c99610dd94fb73c28f6443c5a03fc73/README.md)区分了原生进程沙箱与独立的 Wasm 模式。本目录并不确立部署的安全性，也不将这两种模式合并为一项通用保证。
其他描述仍只是项目索引；此次针对性检查没有重复所有平台的构建，也没有重新统计这 144 个仓库。

- **编程智能体** — [comide](https://github.com/O6lvl4/comide)，目录中的对话 · [golemide](https://github.com/O6lvl4/golemide)，单个静态二进制文件，先观察再编辑，每次写入都经过语法关卡 · [homullus](https://github.com/almide-ai/homullus)，模块化智能体运行时 · [manus](https://github.com/almide-ai/manus)，在 macOS 上操作计算机
- **智能体基础设施** — [porta](https://github.com/almide/porta)，原生进程沙箱（macOS Seatbelt / Linux Landlock、seccomp 和 namespaces），另有独立的 Wasm 执行模式 · [ctxgate](https://github.com/O6lvl4/ctxgate)，位于工具钩子中的上下文防火墙 · [hew](https://github.com/O6lvl4/hew)，供智能体使用、能够理解结构的 `cat`/`grep`/`sed` · [almai](https://github.com/almide-ai/almai)，面向所有提供商的统一 LLM 接口
- **解析** — [gramide](https://github.com/O6lvl4/gramide)，供编程智能体使用的语法树，提供 [TypeScript](https://github.com/O6lvl4/gramide-typescript)、[Python](https://github.com/O6lvl4/gramide-python)、[Rust](https://github.com/O6lvl4/gramide-rust)、[Go](https://github.com/O6lvl4/gramide-go)、[JavaScript](https://github.com/O6lvl4/gramide-javascript) 和 [Almide](https://github.com/O6lvl4/gramide-almide) 的语法包，每个包都与相应语言自身的解析器进行核对 · [parsegen](https://github.com/almide/parsegen)，兼容 tree-sitter、无需 C 和动态加载的解析器生成器
- **图形** — [ceangal2](https://github.com/almide-graphics/ceangal2)，以计算着色器为核心的 snaidhm 2D/3D 渲染器，以及建立在其上的 ceangal 布局与控件；同一个 Almide 程序可用于 Web、iOS、macOS、Android、Windows 和 Linux（从 [ceangal-hello](https://github.com/almide-graphics/ceangal-hello) 开始） · [obsid](https://github.com/almide-graphics/obsid)，通过 WASM 提供 Canvas 2D / WebGL / 3D · [nn](https://github.com/almide-graphics/nn) 和 [slabhra](https://github.com/almide-graphics/slabhra)，神经网络基础算子（从标准库 `matrix` 移出的内核位于 `nn.kernels`）与反向模式自动微分 · [cruth](https://github.com/almide-graphics/cruth)，VRM/glTF 角色 · [lumen](https://github.com/almide-graphics/lumen)，无需 `@extern` 的图形数学库
- **桌面** — [almide-shell](https://github.com/almide-graphics/almide-shell)，Hyprland shell：状态栏、启动器、通知、屏幕显示 · [sprid](https://github.com/O6lvl4/sprid)，在严格内存预算内提供回滚查看功能的终端模拟器
- **绑定与打包** — [almide-bindgen](https://github.com/almide/almide-bindgen)，为包含 Almide 在内的 21 个已列明语言目标生成 FFI · [almide-wasm-bindgen](https://github.com/almide/almide-wasm-bindgen) · [almide-lander](https://github.com/almide/almide-lander)，将 Almide 用作原生共享库 · [almide-web](https://github.com/almide/almide-web)，浏览器 API
- **库** — [toml](https://github.com/almide/toml) · [yaml](https://github.com/almide/yaml) · [csv](https://github.com/almide/csv) · [svg](https://github.com/almide/svg) · [dfa](https://github.com/almide/dfa) · [bigint](https://github.com/almide/bigint) · [aes](https://github.com/almide/aes) · [rsa](https://github.com/almide/rsa) · [sha1](https://github.com/almide/sha1) · [base64](https://github.com/almide/base64) · [almide-sqlite](https://github.com/almide/almide-sqlite) · [llm](https://github.com/almide/llm)，LLM 提供商的流式客户端
- **符合性与测量** — [als](https://github.com/almide/als)，具有独立判定器的规范性规格，可针对任意 `almide` 二进制文件运行 · [almide-dojo](https://github.com/almide/almide-dojo)，测量 LLM 编写便利性的场地，测试框架用 Almide 编写 · [bonsai-almide](https://github.com/almide/bonsai-almide)，浏览器中的 1-bit LLM

2026-09-30 的测量：除本仓库外，144 个公开仓库包含 Almide 源码。排除生成产物后的手写代码：[nn](https://github.com/almide-graphics/nn) 21,815 行，[gramide](https://github.com/O6lvl4/gramide) 系列八个仓库共 19,780 行，[almide-shell](https://github.com/almide-graphics/almide-shell) 8,502 行，[snaidhm](https://github.com/almide-graphics/snaidhm) 6,800 行，[almide-bindgen](https://github.com/almide/almide-bindgen) 5,871 行，[ceangal](https://github.com/almide-graphics/ceangal) 5,099 行，[sprid](https://github.com/O6lvl4/sprid) 4,565 行，[porta](https://github.com/almide/porta) 4,419 行。snaidhm 和 ceangal 当时按各自独立仓库统计，之后这两个仓库已归档并迁入 ceangal2。

## 工具

- [almide-grammar](https://github.com/almide/almide-grammar) — 用 Almide 编写的语法描述（关键字、运算符、优先级、TextMate 作用域），编辑器和 tree-sitter 工具由此生成；它反映编译器的词法分析器，而非作为其输入。当两者的 token 集合不同时，每周运行的 `downstream-sync` 工作流（`scripts/check-downstream-grammar.sh`）会失败
- [vscode-almide](https://github.com/almide/vscode-almide) · [tree-sitter-almide](https://github.com/almide/tree-sitter-almide)（Neovim、Helix、Zed） · [playground](https://github.com/almide/playground)
