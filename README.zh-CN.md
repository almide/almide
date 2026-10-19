<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./docs/assets/brand/almide-cover-dark.png">
    <img src="./docs/assets/brand/almide-cover.png" alt="Almide" width="720">
  </picture>
</p>

<p align="center"><strong>当模型出错时，编译器会点名该怎么改。</strong></p>

<p align="center">
  <a href="https://github.com/almide/almide/actions/workflows/ci.yml"><img src="https://github.com/almide/almide/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg" alt="License: MIT / Apache-2.0"></a>
  <a href="https://deepwiki.com/almide/almide"><img src="https://img.shields.io/badge/Ask-DeepWiki-blue.svg" alt="Ask DeepWiki"></a>
</p>

<p align="center">
  <a href="https://almide.github.io/playground/">Playground</a> ·
  <a href="./docs/CHEATSHEET.md">速查表</a> ·
  <a href="./docs/SPEC.md">规范</a> ·
  <a href="#快速开始">快速开始</a> ·
  <a href="./docs/benchmarks/README.md">证据</a> ·
  <a href="./docs/how-it-works.md">工作原理</a> ·
  <a href="./docs/status.md">状态</a>
</p>

<p align="center">
  <a href="README.md"><img alt="English" src="https://img.shields.io/badge/English-d0d7de?style=flat-square"></a>
  <a href="README.ja.md"><img alt="日本語" src="https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square"></a>
  <a href="README.zh-CN.md"><img alt="简体中文" src="https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square"></a>
</p>

---

Almide 是一门面向「由 AI 编写、由 AI 修改的代码」的静态类型语言。它的主张不是「模型能写对」，而是：**当模型写错时，编译器在编写阶段就指出来，并点名该怎么改；当模型写对时，一个在 Rocq 中证明过的检查器会证明该程序内存安全。** `almide verify` 把程序的所有权、名称与能力见证交给独立的 `almide-verify`（针对所发布 wasm 字节本身的证书仍在推进中，#2755–#2760）。它同时编译为原生二进制（经由 Rust）和 WebAssembly，两者产生逐字节相同的输出。

## 活下来的一次修改

模型给一个类型加了一个分支，并且像模型常做的那样，别的什么都没动：

```almd
type Shape =
  | Circle(Float)
  | Square(Float)
  | Triangle(Float, Float)   // 这次修改

fn area(s: Shape) -> Float =
  match s {
    Circle(r) => 3.14159 * r * r
    Square(w) => w * w
  }
```

```text
error[E010]: non-exhaustive match: missing Triangle(_, _)
  --> shape.almd:7:9
  in match
  here: match s {
  hint: add arms for Triangle(_, _):
  Triangle(arg1, arg2) => _
Or use `_ => todo()` to compile incrementally.
```

诊断本身就是修复：模型的下一步就是 `Triangle(b, h) => 0.5 * b * h`，程序随后在原生和 wasm 上打印相同的字节。把同一个问题拿去问 Python，用八处注入的修改错误：[demo/make-verify](./demo/make-verify/)。

## 快速开始

**[在浏览器里试 →](https://almide.github.io/playground/)** —— 无需安装。

```bash
curl -fsSL https://raw.githubusercontent.com/almide/almide/main/tools/install.sh | sh   # macOS / Linux
irm https://raw.githubusercontent.com/almide/almide/main/tools/install.ps1 | iex        # Windows (PowerShell)
```

安装脚本在解包前会用发布的 `almide-checksums.sha256` 校验压缩包；每个发布资产都有 Sigstore 证明（验证方法见 [SECURITY.md](./SECURITY.md#release-integrity)）。每个压缩包还带有 `almide-verify`，即 `almide verify` 交付见证的证书检查器。从源码构建需要 [Rust](https://rustup.rs/) 1.96+：`make install`。

```almd
fn main() -> Unit = {
  println("Hello, world!")
}
```

```bash
almide run hello.almd                 # 原生
almide run hello.almd --target wasm   # 相同的字节，在 wasmtime 上
```

## 特性

- **多目标** —— 同一份源码编译为原生二进制（经由 Rust）或 WebAssembly（直接产出，不经 LLVM），输出逐字节相同（[契约](./docs/contracts/)）
- **点名修复的诊断** —— 每条错误都带有位置、上下文和一个具体的修改（[错误码](./docs/diagnostics/)）
- **显式 effect** —— `effect fn` 与 `expr!` 传播；裸的可失败调用是错误，绝不静默（[速查表](./docs/CHEATSHEET.md)）
- **类型** —— 泛型、记录、带穷尽 `match` 的 variant、双向类型推导、`Codec` 自动派生（[规范](./docs/SPEC.md)）
- **无需注解的内存管理** —— Perceus 式所有权推导，没有 GC，没有生命周期（[内存安全](./docs/design/MEMORY-SAFETY.md)）
- **Fan** —— 结构化并发：原生上用线程，在 wasm 上输出顺序相同（[ADR-0024](./docs/adr/0024-fan-effect-callbacks-run-concurrently-as-if-sequential.md)）
- **标准库** —— 自举的 `.almd` 模块，每个公开函数都在两个目标上检验过（[参考](./docs/stdlib/)）
- **内建工具** —— `almide test`、`almide fmt`、`almide check`，以及解析菱形依赖的模块系统（[工作原理](./docs/how-it-works.md)）

## 延伸阅读

以下页面为英文。

- [速查表](./docs/CHEATSHEET.md) —— 语法、stdlib 与惯用法；模型应当最先读的一页（另见 [`llms.txt`](./llms.txt)）
- [规范](./docs/SPEC.md) · [文法](./docs/GRAMMAR.md) · [标准库](./docs/stdlib/) · [设计哲学](./docs/design/DESIGN.md) · [稳定性策略](./docs/STABILITY.md)
- [测量了什么](./docs/benchmarks/README.md) —— LLM 可写性、逐字节相同、内存安全、体积与速度，均带日期并有门禁 · [基准方法](./docs/project/BENCHMARKS.md)
- [行为契约](./docs/contracts/) —— 原生 ⇄ wasm 的每一项承诺及其证据 · [证明与信任的边界](./docs/contracts/proven-vs-trusted.md)
- [工作原理](./docs/how-it-works.md) —— 流水线与信任主干 · [架构](./docs/ARCHITECTURE.md)
- [项目状态](./docs/status.md) —— 1.0 之前；面向 LLM 的表面已冻结 · [路线图](./docs/roadmap/README.md)
- [用 Almide 写成的](./docs/written-in-almide.md) —— 本仓库之外的智能体、解析器、图形与库 · 编辑器：[vscode-almide](https://github.com/almide/vscode-almide)、[tree-sitter-almide](https://github.com/almide/tree-sitter-almide)

## 参与贡献

欢迎提 Issue 和 Pull Request。克隆后请运行 `brew install lefthook && lefthook install`；提交信息使用英文（由 commit-msg 钩子强制）。约定见 [CLAUDE.md](./CLAUDE.md)。

## 许可证

可任选 [MIT](./LICENSE-MIT) 或 [Apache 2.0](./LICENSE-APACHE)。
