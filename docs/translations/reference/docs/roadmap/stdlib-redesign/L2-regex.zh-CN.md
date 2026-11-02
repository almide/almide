> 说明（固定历史）：标准库重设计 L2/L3 — regex：编译后的 Regex 类型阻止无效模式及 pipe 错位，采用 is_full_match，补齐 escape／位置／全部捕获／replace_with 的缺口。
<a id="l2l3-regex"></a>

# L2/L3: regex

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-regex.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-regex.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-regex.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变。不自动同步、重新获取、独立probe/benchmark/安全测试或核实现行API。(F)、输出、语言比较均为原作者主张。保留早期String捕获元素与后来可选元素、parse/compile时间差、初始纯Almide共用引擎与后文native/wasm各自回溯叙述。末尾8+7数量及所有新增首参数均为Regex的说法与表不符，仍原样保留。Pike VM/线性时间与限时门禁是采纳的提案，不是此次实测的ReDoS免疫保证。保留日文代码占位符分别表示原因、[整个匹配、组1、...]。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-regex.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> 来源：Rust https://docs.rs/regex/latest/regex/ （标准库外的 crate）・ Go https://pkg.go.dev/regexp (F) ・ Python https://docs.python.org/3/library/re.html (F) ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/RegExp (F) ・ Gleam https://hexdocs.pm/gleam_regexp/ （标准库外的 package）・ Elixir https://elixir.hexdocs.pm/Regex.html (F) ・ OCaml https://ocaml.org/manual/latest/api/Str.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/-regex/ (F) ・ Swift https://developer.apple.com/documentation/swift/regex

`regex` 有 8 个函数，需要 `import regex`。目前直接传入字符串模式，没有编译后的类型（R-regex-02 将引入 `Regex`）。引擎是纯 Almide（`stdlib/regex_engine.almd`），因此 native 与 wasm 结果相同（两 target 运行 probe-regex/p.almd，输出一致）。

<a id="l2-役割の比較"></a>

## L2：职责比较

| 职责 | Almide | Rust (regex) | Go | Python | TS/JS | Gleam (regexp) | Elixir | OCaml (Str) | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 是否在任意位置匹配 | `regex.is_match` | `is_match` | `MatchString` (F) | `search` (F) | `test` (F) | `check` | `match?` (F) | `search_forward` (F) | `containsMatchIn` (F) | `contains` |
| 是否完整匹配 | `regex.full_match` | `^…$` 用 `is_match` | `^…$` | `fullmatch` (F) | `^…$` 用 `test` | — | — | `string_match` + 长度 | `matches` (F) | `wholeMatch` |
| 首次匹配字符串 | `regex.find` | `find` | `FindString` (F) | `search().group()` | `match` (F) | `scan` | `run` (F) | `matched_string` (F) | `find` (F) | `firstMatch` |
| 全部匹配 | `regex.find_all` | `find_iter` | `FindAllString` (F) | `findall` (F) | `matchAll` (F) | `scan` | `scan` (F) | — | `findAll` (F) | `matches` |
| 全部替换（字面） | `regex.replace` | `replace_all` + `NoExpand` | `ReplaceAllLiteralString` (F) | `sub`（`\1` 被解释） | `replaceAll` | `replace` | `replace` (F) | `global_replace` (F) | `replace` (F) | `replacing` |
| 仅替换首次 | `regex.replace_first` | `replace` | — | `sub(count=1)` | `replace` | — | `replace(global: false)` | `replace_first` (F) | `replaceFirst` (F) | — |
| 按匹配分割 | `regex.split` | `split` | `Split` (F) | `split` (F) | `split` (F) | `split` | `split` (F) | `split` (F) | `split` (F) | `split` |
| 首次匹配的组 | `regex.captures` | `captures` | `FindStringSubmatch` (F) | `search().groups()` | `exec` (F) | `scan` | `run` (F) | `matched_group` (F) | `find().groupValues` | `firstMatch().output` |
| 全部匹配的组 | — | `captures_iter` | `FindAllStringSubmatch` (F) | `finditer` (F) | `matchAll` (F) | `scan` | `scan` (F) | — | `findAll().groupValues` | `matches` |
| 匹配位置 | — | `find().start()` | `FindStringIndex` (F) | `search().start()` | `search` (F) | — | `run(return: :index)` | `search_forward` 的返回值 (F) | `find().range` | `firstMatch().range` |
| 转义特殊字符 | — | `escape` | `QuoteMeta` (F) | `escape` (F) | `RegExp.escape`（ES2025） | — | `escape` (F) | `quote` (F) | `Regex.escape` | — |
| 使用组替换（函数） | — | `replace_all(closure)` | `ReplaceAllStringFunc` (F) | `sub(fn)` (F) | `replace(fn)` | `match_map` | `replace(fn)` (F) | `global_substitute` (F) | `replace(transform)` (F) | `replacing(with: closure)` |
| 替换字符串的 `$1` | — | `$1` | `$1` (F) | `\1` | `$1` | — | `\1` | `\1` | `$1` | — |
| 不区分大小写 | — | `(?i)` | `(?i)` | `re.I` / `(?i)` | `i` 标志 | `case_insensitive` | `i` (F) | `regexp_case_fold` (F) | `IGNORE_CASE` | `ignoresCase()` |
| 命名组 | — | `(?P<n>)` / `(?<n>)` | `(?P<n>)` / `SubexpNames` (F) | `(?P<n>)` 仅此 (F) | `(?<n>)` | — | `named_captures` (F) | — (F) | `groups["n"]` | `Capture` |
| 报告无效模式 | — （悄悄通过） | `Regex::new` 返回 `Result` | `Compile` 返回 error (F) | `re.error` | `SyntaxError` | `compile` 返回 `Result` | `compile` 返回 `{:error, _}` (F) | 异常 | 异常 | `throws` |
| 参数顺序 | 模式在前 | 模式为主体 | 模式为主体 | 模式在前 (F) | 模式为主体 | 模式在前 | 模式在前 (F) | 模式在前 (F) | 模式为主体 | 字符串为主体 |

<a id="完全性の規則"></a>

### 完备性规则

- 与字符串模块成对：`regex.replace`/`replace_first`/`split`/`find_all` 与 `string.replace`/`replace_first`/`split` 使用相同动词、相同语义（`replace` 替换全部，保留空片段）。不破坏这种对应。
- 位置与 `string.index_of` 一样返回码点索引。
- 判定（Bool）以 `is_` 开头（R-L0-09）。

<a id="l2-の判定"></a>

### L2 判定

- R-L0-09 例外：`regex.full_match`（R-regex-01）。
- 两处悄悄出错的情况（实测）：
  1. 无效模式不报错。`regex.is_match("(", "abc")` 返回 `true`，`regex.find("[", "abc")` 返回 `none`，`(?i)ABC` 不匹配 `abc`，返回 `false`，命名组 `(?P<n>a)` / `(?<n>a)` 返回 `none`（probe-regex/p.almd、q.almd，native 与 wasm 相同）。9/9 语言都会报告无效模式（R-regex-02）。
  2. 通过 `|>` 管道传递时，字符串进入模式位置。`"order 42" |> regex.is_match("[0-9]+")` 返回 `false`，`|> regex.find(..)` 返回 `none`（probe-regex/swap.almd）。`"  a \t b\nc  " |> string.trim |> regex.split("\\s+")` 也悄悄得到不同结果（probe-string/e.almd）。两个参数都是 String，现有类型检查不能阻止。用 `Regex` 类型阻止（R-regex-02）。
- 至少 5/9 提供的缺失职责：转义（7）、匹配位置（8）、所有匹配的组（8）、不区分大小写（9）、函数替换（9）、命名组（7）。新增前四项（R-regex-04）。函数替换通过 `replace_with` 新增（R-regex-05）。不新增命名组（R-regex-06）。

<a id="案"></a>

## 提案

<a id="r-regex-01-regexfull_match-を-regexis_full_match-に改名する---採用"></a>

### R-regex-01 将 `regex.full_match` 改名为 `regex.is_full_match` — ○ 采纳

- 内容：新增 `regex.is_full_match(pat: String, s: String) -> Bool`，给 `regex.full_match` 加上 `@deprecated(since=15, use="regex.is_full_match")`。`[[deprecation]]` 块中设 `removal_epoch = 16`。
- 依据的 L0 规则：R-L0-09（L0 点名的例外）、R-L0-14。
- (a) 拼写各异（Python `fullmatch`、Kotlin `matches`、Swift `wholeMatch`）。与 Almide 的 `regex.is_match` 并列，`is_full_match` 读起来自然。
- (b) 无影响，调用相同运行时实现。
- (c) `is_match` 与 `full_match` 一对中，只有一方没有 `is_`，导致编写者写成 `regex.is_full_match` 并产生 E002。
- (d) 改名。`almide fix` 一对一重写。使用数：spec 69 / tests 1 / docs 7 / stdlib 0。
- 门禁：从 R-L0-09 门禁允许列表移除 `regex.full_match`。
- issue：#3940

<a id="r-regex-02-コンパイル済みの-regex-型を入れ不正なパターンを作る時点で止める---採用"></a>

### R-regex-02 引入编译后的 `Regex` 类型，在构造时阻止无效模式 — ○ 采纳

- 内容：
  1. 新增不透明类型 `Regex`，提供两种构造方式。
     - `regex.new(pat: String) -> Regex`：若模式是字符串字面量，在 `almide check` 阶段检查语法，无效时以新 E 代码停止（不支持的语法如命名组，也明确报告“不支持”并停止）。运行时拼装的无效模式视为调用者违反契约而 abort（R-L0-06 第 3 条，与 Go 的 `regexp.MustCompile` 相同）。
     - `regex.compile(pat: String) -> Result[Regex, String]`：对运行时拼装的模式进行附带原因的检查后构造（R-L0-06 第 1 条；Go `regexp.Compile`、Rust `Regex::new`、Gleam `regexp.from_string`、Elixir `Regex.compile`）。
  2. 匹配、搜索、替换、分割函数首参数为 `Regex`（例如 `regex.is_match(re: Regex, s: String) -> Bool`）。名称不变。
  3. abort 消息为单行 `Error: invalid regex: <理由>`，退出码 1，native 与 wasm 相同（新契约 C-NNN 与 fixture，先通过 als 台账）。
- 依据的 L0 规则：R-L0-00（9/9 有正则表达式类型）、R-L0-02（主体＝`Regex`）、R-L0-03、R-L0-06、R-L0-14。
- (a) 9 种语言都有编译后的正则表达式类型（Rust `Regex`、Go `*regexp.Regexp`、Python `re.Pattern`、JS `RegExp`、Gleam `Regexp`、Elixir `Regex`（`~r` 标记）、OCaml `Str.regexp`、Kotlin `Regex`、Swift `Regex`）。只有 Python 的 `re` 模块函数能直接接收字符串模式，而 Python 也有 `re.compile`。
- (b) 引擎是纯 Almide（`stdlib/regex_engine.almd`），因此 `Regex` 作为保存已编译状态的值，以相同形式适用于两 target。不需要 host 功能。
- (c) 实测的两处“悄悄出错”都由类型阻止。
  1. 无效模式（`regex.is_match("(", "abc")` 得到 `true`、`(?i)ABC` 不匹配等）在 `regex.new` 处报错。
  2. 用 `|>` 管道时的错位（`"order 42" |> regex.is_match("[0-9]+")` 得到 `false`）变成将 `String` 传到 `Regex` 位置的类型错误。不需要给声明加标记让检查器读取的机制（旧 R-regex-03 提案）。
- (d) 参数类型变化。旧调用 `regex.find("[0-9]+", s)` 不会悄悄改变含义，而是产生类型错误。提示为 `regex.find(regex.new("[0-9]+"), s)`（R-L0-14 第 4 条禁止的是“类型仍匹配而语义改变”，这里不属于该情况）。`almide fix` 机械地将字符串字面量模式包在 `regex.new(...)` 中。不引入 `String` 自动转换为 `Regex` 的机制（否则产生两种写法）。列入 epoch 15 的 `breaks`，在 `summary` 写出 spec（约 500 处）、tests、下游数量。
- 速度：目前每次调用都解析模式。只构造一次 `let re = regex.new(p)` 并在循环中使用，便只解析一次。
- 门禁：无效模式矩阵 fixture（`(`、`[`、`*`、`a{`、`(?P<n>a)`）核对 check 阶段错误、`regex.new` 的 abort、`regex.compile` 的 err 三种结果。通过 `spec/wasm_cross/` fixture 核对 native 与 wasm 一致。在 `tests/diagnostics/` 放入把字符串管道传至模式位置的 broken，以及使用 `regex.new` 的 fixed。
- issue：#3942

<a id="r-regex-03-パイプの取り違えを宣言の印で止める---不採用r-regex-02-の型で止まるため"></a>

### R-regex-03 通过声明标记阻止管道错位 — × 不采纳（R-regex-02 的类型已能阻止）

- 理由：加标记和区分类型都会使错位调用产生编译错误，生存率不变，没有理由为此新增语言机制。R-regex-02 使首参数为 `Regex` 类型，因此 `s |> regex.find(re)` 形式的错位会成为类型错误。不新增给参数标记“禁止管道”供检查器读取的语言机制（其他模块也可通过区分类型阻止同类错位）。

<a id="r-regex-04-エスケープ位置全一致のグループ大文字小文字の区別なしを足す---採用"></a>

### R-regex-04 新增转义、位置、全部匹配组、不区分大小写 — ○ 采纳

- 内容：
  - `regex.escape(s: String) -> String`：在特殊字符前加 `\`。
  - `regex.index_of(re: Regex, s: String) -> Option[Int]`：首次匹配的起始位置（码点索引）。拼写和单位与 `string.index_of` 相同。
  - `regex.captures_all(re: Regex, s: String) -> List[List[String]]`：每次匹配对应 `[全体, グループ1, ...]`。
  - 接受模式开头的 `(?i)` 作为忽略大小写标记（不增加函数）。
- 依据的 L0 规则：R-L0-01（`index_of` 与 string 同拼写）、R-L0-03、R-L0-12（位置单位）。
- (a) 转义 7/9、位置 8/9、全部匹配组 8/9、大小写 9/9（多数带 (F)）。
- (b) 纯计算。
- (c) 若无 `regex.escape`，嵌入用户输入的模式会悄悄产生不同含义。
- (d) 新增。使用数 0。
- 门禁：性质测试确认 `captures_all` 与 `captures`、`find_all` 一致。`index_of` 加入 R-L0-12 边界输入矩阵。
- issue：#3941

<a id="r-regex-05-一致ごとに関数で置換文字列を作る-regexreplace_with---採用"></a>

### R-regex-05 每次匹配通过函数生成替换字符串的 `regex.replace_with` — ○ 采纳

- 内容：`regex.replace_with(re: Regex, s: String, f: (List[String]) -> String) -> String`。`f` 接收 `[全体, グループ1, ...]`。与 `replace` 一样替换全部匹配。
- 依据的 L0 规则：R-L0-10（`_with`＝接收函数而非值的版本）。
- (a) 9/9 语言提供（Rust `replace_all(closure)`、Go `ReplaceAllStringFunc`、Python `sub(fn)`、JS `replace(fn)`、Elixir `replace(fn)`、Kotlin `replace(transform)` 等）。
- (b) 纯计算。
- (d) 新增。
- 不解释替换字符串中的 `$1`（× 不采纳）。目前 `regex.replace` 将 `$1` 按字面处理（probe：`replace("(a)", "xax", "$1$1")` → `x$1$1x`）。改变解释会让包含 `$` 的既有替换悄悄产生不同结果，违反 R-L0-14 第 4 条。使用组的替换通过 `replace_with` 编写。
- issue：#3941

<a id="r-regex-06-名前つきグループを足さない---不採用"></a>

### R-regex-06 不新增命名组 — × 不采纳

- 内容：不新增 `(?<name>...)` 或按名称提取的 API。
- (a) 7/9 提供，但语法分歧（Python 只有 `(?P<n>)` (F)，JS 使用 `(?<n>)`）。
- (c) 这会为与位置提取 `captures` 相同的职责引入第二种形式（R-L0-01）。写命名语法时，R-regex-02 的 `regex.new` 会报告“不支持”并停止，不会悄悄出错。
- issue：无

<a id="r-regex-07-replace_first_with-を足して置換の-22-を閉じる---採用"></a>

### R-regex-07 新增 `replace_first_with`，补齐替换的 2×2 — ○ 采纳

- 当前：`replace`（全部、字符串）、`replace_first`（首次、字符串）、`replace_with`（全部、函数）。缺少仅首次用函数替换的格子。
- 提案：`fn regex.replace_first_with(re: Regex, s: String, f: (List[String]) -> String) -> String`。
- 依据的 L0 规则：CLAUDE.md“按矩阵扩展族”、R-L0-10（`_with`）。
- (a) 五种语言：Rust regex 库 `replace`（函数）、Python `sub(fn, s, count=1)`、JS `replace(re, fn)`、Elixir `Regex.replace(..., global: false)`、OCaml `Str.substitute_first`。
- (c) 知道 `replace_first` 与 `replace_with` 的 LLM 会按规则写出 `replace_first_with`。
- (d) 新增 1。
- 完备性规则：替换为 {全部、首次} × {字符串、函数} 四格。
- 门禁提案：`replace_first_with(re, s, (g) => rep) == replace_first(re, s, rep)` 性质测试。
- issue：#3971

<a id="r-regex-08-捕捉グループを-string-で返す---採用"></a>

### R-regex-08 捕获组以 `String?` 返回 — ○ 采纳

- 当前：`captures` 是 `Option[List[String]]`，未参与匹配的组设为 `""`（L3 表注）。用 `(a)?(b)` 匹配 `"b"` 的第一组，与 `(a*)(b)` 的空匹配无法区分。
- 提案：组元素改为 `String?`。
  - `fn captures(re: Regex, s: String) -> List[String?]?`
  - `fn captures_all(re: Regex, s: String) -> List[List[String?]]`
  - `replace_with`、`replace_first_with` 回调也改为 `(List[String?]) -> String`。
  - 元素 0（整个匹配）必为 some。
- 依据的 L0 规则：R-L0-06（不返回失败标记值 `""`）、R-L0-24。
- (a) Rust `Captures::get` → `Option<Match>`、Python `groups()` → `None`、JS `exec` → `undefined`、Kotlin `groups[i]` → `null`、Swift `Regex` 输出 → `Substring?`。只有 Go 返回 `""`（位置函数可用 -1 区分）。
- (b) native 与 wasm 两侧正则实现都原样传递未参与组的标记。
- (d) 语义变化（元素类型改变，当字符串使用时通过类型错误察觉）。`captures` 与 R-regex-0x 的首参数变化在同一 epoch 引入。
- issue：#3977

<a id="r-regex-09-対象の文字列を第-1-引数にする---不採用"></a>

### R-regex-09 将目标字符串置于首参数 — × 不采纳

- 理由：`regex` 模块的主体是 `Regex`，符合 R-L0-02、R-L0-03（函数放在主体类型所属模块、主体在前）。Rust `re.replace_all(s, …)`、Go `re.ReplaceAllString(s, …)`、Elixir `Regex.replace(re, s, …)` 也将正则表达式放在前面。想从字符串侧写时使用 `string.replace`。
- issue：无

<a id="r-regex-11-パターンを確かめて作る関数を-regexcompile-と綴る---採用"></a>

### R-regex-11 验证并构造模式的函数拼为 `regex.compile` — ○ 采纳

- 内容：将 R-regex-02 新增的 `regex.parse(pat) -> Result[Regex, String]` 拼为 `regex.compile`（其余保持 R-regex-02）。
- 理由：protocol 层级中，`parse` 是“读取能与 `to_string` 往返的值的字符串表示”的函数（`Parse`、Swift `LosslessStringConvertible`）。拼装模式的函数职责不同。Go `regexp.Compile`、Python `re.compile`、Elixir `Regex.compile` 使用 `compile`。
- (d) 本轮重设计新增函数的名称变化。
- issue：#4004

<a id="r-regex-12-一致の照合を線形時間にする---採用"></a>

### R-regex-12 使匹配为线性时间 — ○ 采纳

- 当前：native（`runtime/rs/src/regex.rs`）与 wasm（`stdlib/regex_engine.almd`）都用回溯匹配，没有步数上限。将 `"aaaa…a!"` 传给 `(a+)+$`，耗时随输入长度指数增长。
- 提案：改用 Pike VM（带捕获组同时推进 Thompson NFA），时间与模式大小 × 输入长度成正比。Almide 正则既无反向引用，也无前瞻、后顾（`docs/stdlib/regex.md`），因此与 RE2、Go、Rust 一样，可在不改变结果（最左、带优先级的匹配与捕获组）的情况下替换。
- 依据的 L0 规则：R-L0-47 第 1 条。
- (a) Go `regexp`、Rust `regex`、RE2 是线性时间。Python、JS、Kotlin、Swift 使用回溯，许多 ReDoS 漏洞由此产生。
- (b) native 与 wasm 采用相同主体（self-host Pike VM），或两个语义相同的实现，并确认现有全部 regex cross fixture 返回相同答案。
- (d) 语义不变（匹配结果相同，仅时间改变）。
- 门禁提案：病态模式（`(a+)+$`、`(a|aa)*b`）和 10 万字符输入在固定时间内结束的 fixture。
- issue：#4019

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，标明 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `regex.is_match` | `is_match(pat: String, s: String) -> Bool` | Rust is_match / Go MatchString / Python search / JS test / Kotlin containsMatchIn | 语义变化（首参数改为 `Regex`） | R-regex-02 |
| `regex.full_match` | `full_match(pat: String, s: String) -> Bool` | Python fullmatch / Kotlin matches / Swift wholeMatch | 改名（`is_full_match`；首参数为 `Regex`） | R-regex-01、R-regex-02 |
| `regex.find` | `find(pat: String, s: String) -> Option[String]` | Rust find / Go FindString / Kotlin find / Swift firstMatch | 语义变化（首参数改为 `Regex`） | R-regex-02 |
| `regex.find_all` | `find_all(pat: String, s: String) -> List[String]` | Python findall / Go FindAllString / Kotlin findAll / Elixir scan | 语义变化（首参数改为 `Regex`） | R-regex-02 |
| `regex.replace` | `replace(pat: String, s: String, rep: String) -> String` | Rust replace_all / Go ReplaceAllLiteralString / JS replaceAll / Kotlin replace | 语义变化（首参数改为 `Regex`） | R-regex-02、R-regex-05（`$1` 不解释） |
| `regex.replace_first` | `replace_first(pat: String, s: String, rep: String) -> String` | Rust replace / Kotlin replaceFirst / OCaml replace_first | 语义变化（首参数改为 `Regex`） | R-regex-02 |
| `regex.split` | `split(pat: String, s: String) -> List[String]` | 全部语言 split | 语义变化（首参数改为 `Regex`） | R-regex-02 |
| `regex.captures` | `captures(re: Regex, s: String) -> Option[List[Option[String]]]` | Rust captures / Go FindStringSubmatch / JS exec / Elixir run | 语义变化（首参数改为 `Regex`；组为 `String?`） | R-regex-08 |
| `regex.is_full_match` | `is_full_match(re: Regex, s: String) -> Bool` | （同上方 full_match） | 新增 | R-regex-01 |
| `regex.new` | `new(pat: String) -> Regex` | Go MustCompile / Rust Regex::new / Kotlin Regex() / JS RegExp / Swift Regex | 新增 | R-regex-02 |
| `regex.compile` | `compile(pat: String) -> Result[Regex, String]` | Go Compile / Rust Regex::new / Gleam from_string / Elixir compile | 新增 | R-regex-02 |
| `regex.replace_with` | `replace_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String` | Rust replace_all(closure) / Go ReplaceAllStringFunc / Python sub(fn) / JS replace(fn) | 新增 | R-regex-05、R-regex-08 |
| `regex.escape` | `escape(s: String) -> String` | Rust escape / Go QuoteMeta / Python escape / Elixir escape / OCaml quote | 新增 | R-regex-04 |
| `regex.index_of` | `index_of(re: Regex, s: String) -> Option[Int]` | Go FindStringIndex / JS search / Rust find().start() | 新增 | R-regex-04 |
| `regex.captures_all` | `captures_all(re: Regex, s: String) -> List[List[Option[String]]]` | Rust captures_iter / Go FindAllStringSubmatch / Elixir scan | 新增 | R-regex-04、R-regex-08 |
| `regex.replace_first_with` | `replace_first_with(re: Regex, s: String, f: (List[Option[String]]) -> String) -> String`（提案） | Python `sub(fn, s, count=1)` / JS `replace(re, fn)` / OCaml `substitute_first` | 新增 | R-regex-07、R-regex-08 |

（当前 8 个函数 + 新增 7 行。新增函数的首参数全为 `Regex`。）

与 R-L0-05 的关系：regex 函数都是返回值的纯函数，误写成语句的情况很少。无特别事项。

<a id="l0-への差し戻し処理済み"></a>

## 退回 L0 的事项（已处理）

- “通过一个参数的函数构造值”的后缀：R-L0-10 将 `_with` 定义为“接收函数而非值的版本”，所以采用 `regex.replace_with`（R-regex-05）。
- 通过声明标记阻止管道错位的机制：不新增。用区分类型阻止（R-regex-02、R-regex-03）。
