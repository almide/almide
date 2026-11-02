<a id="the-published-almide-grammar"></a>

# 已发布的 Almide 文法

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)



> 原文：[docs/grammar/README.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/grammar/README.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。这是面向读者的非规范性快照译文，不会自动同步。保留原文的时间、适用范围与主张；翻译时未进行独立测量或实现验证。 原文中的研究评价按原文主张翻译，并非翻译时独立验证的比较结果。

`almide.lark` 是机器可读的 Almide 文法。它只为一个目的而存在：**解码时约束引擎**——[llguidance](https://github.com/guidance-ai/llguidance)、[XGrammar](https://github.com/mlc-ai/xgrammar)、OpenAI custom tools（Lark CFG 输入）——只有存在可用的 CFG，才能在 token 采样时强制语法有效。该产物可以在生成前排除一些无效 token 序列，但它有意成为编译器语法的超集。它并不会让所有无效 Almide 程序都无法表示；限制与已测试的反例列于下文。

这种方法的依据：CangjieBench（arXiv 2603.14501）——对于低资源语言，语法约束生成提供最佳的准确率/成本权衡。CRANE（arXiv 2502.09061）——约束的是*输出*，绝不是推理。

<a id="files"></a>

## 文件

| 文件 | 内容 |
|---|---|
| `almide.lark` | 文法产物（Lark 语法，Earley + basic lexer） |
| `negative-fixtures.txt` | 文法必须拒绝的无效程序 |
| `../../scripts/check-lark-grammar.sh` | 防止过时的门禁 |
| `../../scripts/lib/lark-gate.py` | 门禁的实现 |

<a id="using-it"></a>

## 使用方法

```python
from lark import Lark
parser = Lark(open("docs/grammar/almide.lark").read(),
              start="start", parser="earley", lexer="basic")
parser.parse(open("app.almd").read())
```

对于 llguidance / XGrammar，将同一个文件交给引擎的 Lark 前端。

<a id="the-anti-rot-gate--exactly-what-it-checks"></a>

## 防止过时的门禁——它确切检查什么

手写文法在解析器改变的那一刻就会过时，而过时文法*比没有更糟*：它会把解码器限制到编译器已不再接受的语言中。`scripts/check-lark-grammar.sh` 正是用来阻止这种情况的。它断言三个独立事项。

**1. 词法一致。** 将 `almide.lark` 中的 `BEGIN-KEYWORDS` / `BEGIN-OPERATORS` 标记块与 `crates/almide-syntax/src/lexer.rs` 中的 `KEYWORDS`、`OPERATORS` 表逐 token 比较。向 lexer 添加关键字而没有在这里添加，会使门禁失败，反之亦然。`EXCLUDED` 列表也进行双向检查：被排除的运算符必须仍是真正的 lexer token（否则排除项已过时），且不能同时在文法中声明。

**2. 语料超集。** 使用公开文法解析 `spec/`、`examples/` 和 `stdlib/` 下的每个 `.almd`。对文法*拒绝*的每个文件，通过 `almide <file> --emit-ast` 查询编译器——它只运行 lexer 和 parser，不做其他处理（出现类型错误仍以 0 退出）。如果编译器能够解析而文法不能，则直接失败。

只要求这一方向。文法有意作为**超集**：过度接受不会给解码器造成损失（只是没能阻止一个坏 token），而接受不足会让合法程序无法被采样。

**3. 区分能力。** `negative-fixtures.txt` 中的每个测试夹具都必须被文法*和*编译器拒绝。没有这一条，退化成 `.*` 的文法也能轻松通过检查 2。门禁还强制要求测试夹具数量达到下限。

<a id="what-the-gate-does-not-prove"></a>

## 门禁没有证明什么

- **不是健全性。** 它没有证明文法*只*接受有效 Almide。它按设计就是超集；检查 2 只能在接受不足的方向失败，检查 3 只固定 `negative-fixtures.txt` 中列明的反例。
- **不是语料之外的覆盖率。** `spec/`、`examples/` 或 `stdlib/` 中没有文件使用的语法特性不会得到测试。如果新结构落地时*没有*附带 spec 测试夹具，此处就捕捉不到——这也是新语法总要带上 `spec/` 测试的另一个理由。
- **不涉及任何语义。** `--emit-ast` 仅做解析：类型、effect、模块解析和整个 checker 都不在范围内。
- **不是引擎可移植性。** 门禁使用 Python `lark`（Earley + basic lexer）。它没有证明 llguidance 或 XGrammar 能编译这一精确方言；这些引擎接受具有各自限制的*类* Lark 语法。

<a id="known-gaps--what-the-grammar-cannot-express"></a>

## 已知差距——文法无法表达什么

这些是如实说明的限制，不是 bug。在产物比编译器*宽松*的地方，解码器只是没有约束；在它*更严格*的地方，语料检查只确立：经过测试、被编译器接受的文件没有被排除。它不证明覆盖所有合法程序。

| # | 差距 | 方向 |
|---|---|---|
| G1 | **嵌套块注释。** `/* … */` 是非贪婪正则表达式；lexer 会计算嵌套深度。`/* /* */ */` 会留下一个多余的 `*/`。 | 更严格 |
| G2 | **Heredoc 去缩进。** `"""…"""` 是一个不透明终结符。`strip_heredoc_indent` 是*值*转换，而不是语法转换，因此没有损失语法信息——但该产物无法表达闭合分隔符的缩进规则。 | 不适用 |
| G3 | **插值孔。** `${…}` 按*词法*匹配，支持嵌套的 `"…"`/`'…'` 字面量以及至多三层 `{ }` 嵌套。编译器会重新词法分析插值孔，并用子解析器重新解析，因此内容是完整表达式。所以解码器在**插值孔内部不受约束**，而花括号嵌套超过三层的插值孔会被拒绝。这是单一基于文本的 CFG 无法镜像编译器两阶段词法分析的唯一位置。 | 双向 |
| G4 | **对空白敏感的一元负号。** `a⏎- b` 延续减法；`a⏎-b`（紧贴）开始新语句。token 级文法看不到间隔，因此两种解释都会被接受。 | 更宽松 |
| G5 | **仅由诊断施加的限制。** `??` 的跨行规则（E038/#1112）、末尾 `??`（E038）、命名参数之后的位置参数，以及“第一个默认参数之后的每个参数也必须有默认值”的规则，是解析器*诊断*，不是文法。该产物接受这些情况。 | 更宽松 |
| G6 | **保留的上下文标识符。** `as`、`where` 和类型名 `Fn` 在编译器中按拼写匹配，但对 lexer 而言是普通标识符。该产物将它们保留，因此合法 Almide `let as = 1` 会被拒绝。语料中没有文件这样做。`self` 有意*不被*保留（它在每个 convention 方法体中都是一个值），所以无类型的接收者参数被写成“第一个参数可省略 `: Type`”——该产物接受编译器拒绝的 `fn f(x) -> Int = 1`。 | 双向 |
| G6b | **`scoped` 是上下文相关的，并非保留字**（#1997、ALS-E31）。与 G6 的三个标识符不同，`scoped` 在产物中保留标识符含义：`SCOPED` 终结符可出现在每个普通名称位置（`_ident`），因此 `let scoped = 41` 和 `match scoped { … }` 仍可解析。产物仅在头部比编译器*宽松*——只要后面跟着块或 `fn`，它就把 `scoped` 读成限定符，而编译器还要求 `{` 在同一行，并处于 `match`/`while`/`for` 头部之外。 | 更宽松 |
| G7 | **`fan.<head>`。** head 名称不受限制（任意标识符）；当前块 head 包括 `bounded`/`timeout`/`min`/`settle`；`race` 和 `any` 是编译器会诊断的已退役拼写。 | 更宽松 |
| G8 | **必须有语句分隔符**（换行或 `;`）。编译器允许同一行两个语句无分隔符相邻。该产物有意要求分隔符：它消除了较大的歧义，没有格式化后的 Almide 依赖这种写法。 | 更严格 |
| G9 | **算术优先级被展平。** `\|>`、`>>`、`+ -`、`* / %`、`^` 共享一个层级。优先级塑造解析*树*，不影响可接受 token 串的集合，而 Earley 在每一列都要支付深度成本。结合强度仍以 `grammar/precedence.toml` 和 `Parser::infix_bp` 为准。布尔、比较和范围层级仍然分开——它们的非结合性是语言事实（`a < b < c` 和 `0..<1..<2` 是错误）。 | 相同 |
| G10 | **排除退役或拒绝的拼写。** `&&`、`\|\|`、`++` 和作为范围的 `..`/`..=` 在编译器中可解析，*只是*为了给出迁移提示。该产物排除这些写法：解码器绝不应采样它们。 | 更严格 |
| G11 | **没有深度限制。** 解析器将表达式嵌套上限设为 500（`MAX_DEPTH`）；该产物不设此限制。 | 更宽松 |
| G12 | **带模块限定的记录字面量**（`mod.TypeName { … }`）可以出现在任意后缀表达式之后，而不只是解析器要求的单独标识符之后。 | 更宽松 |
| G13 | **非 ASCII 名称**（#1456）。编译器接受 UAX #31 标识符（XID_Start 后接 XID_Continue，NFC），拒绝兼容字符（`ａ`、`１`、`ﬁ`），并通过声明判断无大小写名称的类别：`住所` 只在声明 `type 住所` 或 case `| 住所` 的模块中是类型名。产物的 `UNAME` 是任何以非 ASCII 字母开头的名称，在值位置和类型位置都可接受，后续的 `\w` 还允许兼容字符。 | 更宽松 |

<a id="relationship-to-the-grammar-submodule"></a>

## 与 `grammar/` 子模块的关系

[`almide/almide-grammar`](https://github.com/almide/almide-grammar)（`grammar/` 子模块）保存 tree-sitter 和 VS Code 生成器使用的*描述性*关键字、优先级和 TextMate 数据。它没有可用的 CFG，CI 也从不获取它——因此该产物位于编译器仓库中，让门禁能够访问它及其镜像的 lexer。`almide.lark` 的优先级注释指向 `grammar/precedence.toml`；两者都指向同一个权威来源 `Parser::infix_bp`，从而保持一致。
