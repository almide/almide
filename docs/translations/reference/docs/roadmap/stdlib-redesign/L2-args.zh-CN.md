<!-- description: 标准库重新设计 L2/L3 — args：基于 env.args 的最小标志读取器；has_flag 替换 flag（Bool 命名及首字母冲突），移除 raw 和 option_or -->
<a id="l2l3-args"></a>

# L2/L3: args

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-args.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-args.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-args.zh-CN.md)

> 非规范性历史阅读快照，以固定的日文正本为准，不自动同步。采纳的提案、测量、跨语言比较及日期均按原文保留，不代表新验证或当前已实现。页首日期早于后文安全修复，二者均保留。原文简写 positional_at(i) 与表中带 options 的声明也未擅自统一。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-args.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-25：argv 本体归 `env`，标志读取归 `args`。§3 排除完整的标志解析器。）
> 来源：Rust https://doc.rust-lang.org/std/env/fn.args.html （标准库没有标志解析）；Go https://pkg.go.dev/flag；Python https://docs.python.org/3/library/argparse.html；TS/JS https://nodejs.org/api/util.html#utilparseargsconfig；Gleam https://gleam-stdlib.hexdocs.pm/ （没有）；Elixir https://hexdocs.pm/elixir/OptionParser.html；OCaml https://ocaml.org/manual/latest/api/Arg.html；Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/ （没有）；Swift https://github.com/apple/swift-argument-parser （官方包）。
> 九种语言的事实来自 [facts/categories.md §3.4](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md)。

<a id="l2-役割の比較"></a>

## L2：职责比较

| 职责 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 原始 argv | `args.raw`、`env.args` | `env::args` | `os.Args` | `sys.argv` | `process.argv` | — | `System.argv` | `Sys.argv` | `main(args)` | `CommandLine.arguments` |
| Bool 标志 | `args.flag` | — | `flag.Bool` | `store_true` | `parseArgs` `type: 'boolean'` | — | `OptionParser` `:boolean` | `Arg.Set` | — | `@Flag` |
| 带值选项 | `args.option`、`args.option_or` | — | `flag.String` | `add_argument("--x")` | `parseArgs` `type: 'string'` | — | `OptionParser` `:string` | `Arg.String` | — | `@Option` |
| 位置参数 | `args.positional`、`args.positional_at` | — | `flag.Args`、`flag.Arg(i)` | `add_argument("x")` | `parseArgs` `positionals` | — | `OptionParser` 的剩余内容 | `Arg.parse` 的 anon_fun | — | `@Argument` |
| 生成帮助文本 | — | — | ● | ● | — | — | — | ● | — | ● |

<a id="完全性の規則"></a>

### 完备性规则

- `args` 是“不带模式定义的最小读取器”（L1 §3 排除完整版）。职责限定为四种：Bool 标志、值选项、位置参数（全部及第 i 个）。
- 原始 argv 仅由 `env.args` 提供（R-L1-25）。
- 不提供默认值版本，写作 `args.option(n)! ?? d`（R-L0-06）。
- 有意不提供：帮助文本生成、带类型的值（转换为 `Int` 使用 `int.parse`）。

<a id="l2-の判定"></a>

### L2 判定

- 重复：`args.raw` 只是调用一次 `env.args()` 的函数（R-L0-01 所述转发，视为同一职责）→ R-args-02。
- 违反规则：`args.flag` 返回 Bool，却不以 `is_`/`has_` 命名（R-L0-09 明确点名的例外）→ R-args-01。
- 违反规则：`args.option_or` 是 `_or` 版本（R-L0-06）→ R-args-02。
- 文档不一致：`docs/stdlib/args.md` 将 `args.raw` 描述为包含 argv[0]，实测却排除它（`raw=-v,--out,x,in.txt`，native 与 wasm 均如此；probe-args）。在移除窗口内更正 → R-args-02。

<a id="案"></a>

## 提案

<a id="r-args-01-argsflag-を-argshas_flag-にし頭文字の自動の短縮形をやめる---採用"></a>

### R-args-01 将 `args.flag` 改名为 `args.has_flag`，停止自动取首字母作短形式 — ○ 采纳

- 内容：添加 `effect fn has_flag(name: String) -> Bool`。存在 `--name` 时为 true；`name` 为单字符时，`-n` 也为 true。不再从首字母生成短形式。移除 `args.flag`。
- 实测（probe-args）：执行 `almide run a.almd -- -v`，`args.flag("verbose")` 和 `args.flag("version")` 都为 true。`-v` 匹配两个名称。
- 依据的 L0 规则：R-L0-09（Bool 使用 `is_`/`has_`；L0 明确将 `args.flag` 列为例外）。同时改变语义，因此遵循 R-L0-14 第 4 项（改变语义时采用新名称）迁移到新名称。
- (a) 没有语言会从长名称自动生成短形式。Go 的 `flag` 同等处理 `-name` 与 `--name`，短形式单独定义。Python、Node、Elixir 也显式指定短形式（通过 `alias`/`short` 指定 `-v`）。
- (b) 这是基于 `env.args` 的纯计算。native 与 wasm 相同（`wasi:cli/environment`）。
- (c) 从 `has_flag` 的名称即可看出 Bool。将短形式写作 `args.has_flag("verbose") or args.has_flag("v")`，不与任何语言的习惯冲突。首字母冲突是 LLM 不写测试就无法察觉的错误。
- (d) 改名及语义变更。兼容：给 `args.flag` 标注 `@deprecated(since=15, use="args.has_flag")`，`removal_epoch = 16`。`almide fix` 将 `args.flag("verbose")` 改为 `(args.has_flag("verbose")! or args.has_flag("v")!)`，完整保留当前含义。使用数 7/9/13/0。
- 检查提案：在 R-L0-G 的 Bool 名称检查中，从允许列表移除 `args.flag`。增加首字母冲突 fixture（`-v` 下 `has_flag("version")` 为 false）。
- issue：#3893。

<a id="r-args-02-argsraw-と-argsoption_or-を撤去する---採用"></a>

### R-args-02 移除 `args.raw` 与 `args.option_or` — ○ 采纳

- 内容：`args.raw()` → `env.args()`；`args.option_or(n, d)` → `args.option(n)! ?? d`。移除前将 `docs/stdlib/args.md` 的“argv[0] included”改为“excluded”。
- 依据的 L0 规则：R-L0-01（只调用一次其他公开函数的转发也视为同一职责）、R-L0-06（将 `_or` 版本统一为 `??`）、R-L1-25。
- (a) 只有 Almide 的标志解析模块也返回原始 argv（facts §3.4）。Python 的 `default=` 等默认值参数确实存在，但没有语言为此拆分函数。
- (b) 无变化。
- (c) README 与 args.md 对 `args.raw` 的说明不同，无论 LLM 阅读哪一个，都会有一半出错。
- (d) 移除。`@deprecated(since=15, use="env.args")` 和 `use="args.option(name)! ?? fallback"`，`removal_epoch = 16`。`almide fix` 改写两者。`option_or` 的替换形式（是否需要 `!`）按照 R-L0-17 决定的调用类型确定。使用数：`args.raw` 1/4/11/0，`args.option_or` 6/4/8/0。
- 检查提案：同义词检查规则 5（#3851 F）。R-L0-G 拒绝以 `_or` 结尾的公开函数。
- issue：#3851 B（raw）、R-L0-06 的新 issue（option_or）。

<a id="r-args-03-positional_at-を撤去する---採用"></a>

### R-args-03 移除 `positional_at` — ○ 采纳

- 当前：`args.positional_at(i)` 与 `list.get(args.positional(options)!, i)` 的值相同。R-args-02 以相同理由（可组合其他函数表达）移除了 `option_or`。
- 提案：移除它，写作 `list.get(args.positional(options)!, i)`。
- 依据的 L0 规则：R-L0-01（不在两处设置相同职责）。
- (d) 移除 1 项。`@deprecated(since=15, use="list.get(args.positional(options)!, i)")`，`removal_epoch = 16`，通过 `almide fix` 改写。
- issue：#3979。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，显示 mut） | 其他语言的主要写法 | 判定 | 提案 |
|---|---|---|---|---|
| `args.raw` | `effect fn raw() -> List[String]` | Rust `env::args` / Go `os.Args` / Python `sys.argv` | 移除（→ `env.args`） | R-args-02 |
| `args.flag` | `effect fn flag(name: String) -> Bool` | Go `flag.Bool` / Python `store_true` / Elixir `:boolean` | 移除（→ `has_flag`） | R-args-01 |
| `args.has_flag` | `effect fn has_flag(name: String) -> Bool`（提案） | 同上 | 添加 | R-args-01 |
| `args.option` | `effect fn option(name: String) -> String?` | Go `flag.String` / Python `add_argument("--x")` / Node `parseArgs` | 保持 | — |
| `args.option_or` | `effect fn option_or(name: String, fallback: String) -> String` | Python `default=` | 移除（→ `option(n)! ?? d`） | R-args-02 |
| `args.positional` | `effect fn positional(options: List[String]) -> List[String]` | Go `flag.Args` / Node `positionals` | 保持（2026-10-26 的安全修复 #4144、C-181 使它接收要读取的选项名称 `options`，不再将 `--name` 后的值放入位置参数。撤回此前“也包括选项值”的描述。） | — |
| `args.positional_at` | `effect fn positional_at(i: Int, options: List[String]) -> String?` | Go `flag.Arg(i)` | 移除（→ `list.get(args.positional(options)!, i)`） | R-args-03 |

<a id="l0-への差し戻し処理済み"></a>

## 退回 L0（已处理）

- `args.option` 与 `env.get` 都是相同的 `effect fn … -> String?`，但 `??` 的写法不同。L0 的 R-L0-17（调用类型仅由声明决定）解决了这一点。二者声明相同，因此调用方式相同。统一为何种形式在 R-L0-17 的新 issue 中设计，本页不改变函数名称及声明（L2-env R-env-03）。
