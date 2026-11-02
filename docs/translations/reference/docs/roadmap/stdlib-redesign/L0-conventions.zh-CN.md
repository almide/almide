读者说明：标准库重设计第0层——所有L1/L2/L3判定所依据的跨模块命名、归属、修改、失败及转换规则。
<a id="l0-全モジュールに共通する規則"></a>

# L0：所有模块共用规则

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L0-conventions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L0-conventions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L0-conventions.zh-CN.md)

> 非规范性固定原文历史重设计读者版。原本固定不变，不自动同步、重新测量、修改实现或执行门禁。采纳、撤销、暂缓及带日期实现状态属于原文当时记录，不是独立确认当前保证。保留改订层次：源侧to与目标from、R20撤销、R21none与后续abort、负数截边与R29abort、R41撤销与R43保留组合、设想协议层级与当时已实现公开泛型API。原统计、三种机制却有四行表及其他矛盾不作统一。安全、线性时间、其他语言漏洞断言是原文主张，不是新证明或重新核验安全公告。F标记属于原调查。保留占位符含义：型为类型、元为源、先の型为目标类型、値为值、次の状態为下一状态、残り为剩余、何为何物、表現为表示、変種为变种。历史命令不是新执行指令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)

> 最后更新：2026-10-21
> 依据表：[facts/naming-conventions.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/naming-conventions.md)（来自9种语言官方文档的事实；(F)为此次获取核验处）
> 状态索引：[README.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/README.md)

重设计999个标准函数（44个模块），最优先整体一致性。
本页规则是“上层裁定”，L1（类别）、L2（模块内容）、L3（单个函数）的判定全部由此推导。
不按名称对单个函数特殊处理。规则应可从签名、返回类型、`mut`声明等单一声明处机械判断。

阅读约定：
- `mut`（读作mute）=原地改写调用方`var`的参数
- in-place（原地改写）/复制（返回新值）
- `??`（读作null coalescing，空合并）=左侧为none / err时使用右侧的运算符
- (a)–(d)=判定4轴：(a)与他语言的缺失/多余，(b)是否同时成立于native/wasm，(c)LLM是否容易写对，(d)改动类型及兼容性

---

<a id="改名してよい条件全規則の前提"></a>

## 允许改名的条件（全部规则的前提）

**R-L0-00——○采纳**

只有满足以下之一才可改拼写：

1. **Almide内部冲突。** 同一角色有2种拼写，或同一动词在不同模块含义不同。
2. **9种语言的多数（至少5种）一致采用另一拼写**，且Almide拼写无语言特有理由。

无多数的角色（如长度`len`/`length`/`size`/`count`，大写转换`to_upper`/`uppercase`）保持现有拼写。

- (a) 不与9种语言任何一种完全一致。但规定“靠拢多数，分歧则不动”，即可将改名压到必要最低限度。
- (c) LLM训练数据中的已有拼写，每次改变都会产生新旧混合期。不必要改名本身成为误写原因。
- (d) 此规则自身不是改动。

---

<a id="r-l0-01-同じ役割には綴りを-1-つモジュールをまたいでも---採用"></a>

## R-L0-01 同角色一种拼写（即使跨模块）——○采纳

将CHEATSHEET的“No synonyms”跨模块应用。当前门禁`tests/stdlib_synonym_gate_test.rs`仅看单模块内。
保留“位于主体类型模块者”（R-L0-02）。

- (a) 他语言也有重复（如Python `math.fabs`与`abs`）。即便如此，没有语言设计方针推荐在标准库并列同一计算两次。
- (b) 无影响，两拼写调用同一运行时函数体。
- (c) 存在两个时，若仅一者边界行为不同，容易混用（如`env.temp_dir`与`fs.temp_dir`）。
- (d) 移除，兼容按R-L0-14。
- “同角色”也包含仅调用一次其他公开函数的函数（转发），如`error.context`仅经`result.map_err`加前缀。
- 类型矩阵单元格（如`bytes.append_i16_le`与`append_u16_le`，仅类型轴不同），即使运行时函数体相同也不算同义词，因为类型轴本身有意义。但矩阵形态须已声明并受门禁保护（CLAUDE.md“API族按矩阵扩展”）。
- 同形角色用同词性。每n个分组函数统一为单数动词`chunk`（`list.chunk`，`bytes.chunks`改名）。
- 门禁：扩充同义词门禁，检测不同模块相同签名/运行时函数体，同模块不同intrinsic共享函数体，以及仅调用一次另一公开函数的函数（扩展#3851 F）。允许列表只可缩减。
- issue：#3851（已有）。

<a id="r-l0-02-関数は主語の型のモジュールに置く---採用"></a>

## R-L0-02 函数放在主体类型模块——○采纳

放在第1参数（主体）类型所属模块。无主体构造器（`map.new`、`list.range`）放返回类型模块。
`math`仅放类型模块无归属的数学函数。边界为：“`Int`也有同角色的（`abs`、`min`、`max`、`clamp`、`sign`、`pow`）放类型模块；仅实数数学（`sqrt`、`cbrt`、`hypot`、三角/指数/对数/伽马）及`factorial`、`choose`放`math`；舍入/判断/常量/位模式/转换放`float`”（R-math-03重审`sqrt`归属后形成此规则）。
容器元素类型具有意义的函数（接受`List[Option[T]]`的`option.collect`，接受`List[Result[T, E]]`的`result.partition`）放元素类型模块。

- (a) 多数7/9放主体类型模块或方法（Rust/Kotlin/Swift为方法，Gleam/Elixir/OCaml为类型模块）。Go和Python `math`为例外。
- (c) 如`int.abs`与`math.abs`有两处候选位置，LLM无法猜中实际归属。
- (d) 移动是移除+添加，按R-L0-14。
- 门禁：检查公开函数第1参数或返回类型为该模块主体类型。例外允许列表只可缩减。
- issue：#3949。

<a id="r-l0-03-引数の順序は主語を先頭にする---据え置きゲート"></a>

## R-L0-03 参数顺序以主体为首——○保留+门禁

`xs |> list.map(f)`的`|>`（pipe，管道）插入第1参数。当前999函数全部遵循此顺序。

- (a) 数据优先的语言7/9（Rust、Go、JS、Gleam、Elixir、Kotlin、Swift）。仅OCaml Stdlib和Python `map`/`filter`函数优先。
- 门禁：与R-L0-02同测试，检查主体类型参数在首位。
- issue：#3949。

<a id="r-l0-04-コピーが既定その場書き換えは決まった動詞だけ---採用"></a>

## R-L0-04 默认复制，原地改写仅限固定动词——○采纳

**规则：**
1. 函数默认返回复制（`list.sort`、`list.set`、`map.set`、`set.insert`）。更新变量写`xs = list.set(xs, i, v)`。带`@reuse`的函数即使此写法也不复制而原地执行（map 200万项时，`map.set`、`map.insert`、索引赋值均约0.07秒，已实测）。
2. 接受`mut`参数的函数（原地改写）仅限以下动词：无法用复制版达到同速的缓冲区增长/截短/按位置写入操作。
   - `push`（尾部加1个）、`pop`（尾部取1个）、`clear`（清空）、`fill`（全设同值）
   - 双端容器在名称加前端限定的`push_front`/`pop_front`（`deque`，尾部`push`/`pop`，R-deque-03），计为`push`/`pop`同族
   - `append_<型>`（字节序列尾部追加）、`put_<型>`（字节指定位置覆盖）
   - `copy_from`、`copy_within`（复制字节范围）
3. 同动词在所有模块代表同模式（复制或原地）。
4. 所有容器类型提供`clear`（list/map/set/deque/heap/string/bytes）。保留分配区而清空，复制无法同速（向set/deque/heap添加在第二轮重审决定，R-deque-02、R-heap-02）。`set`、`insert`、`remove`、`delete`、`update`为复制侧动词。

**由此决定的事项（L2分别判定）：**
- 移除`map.insert(mut m, k, v)`与`map.delete(mut m, k)`。`set.insert`、`list.insert`为复制，仅`map.insert`原地。`m[k] = v`和`m = map.remove(m, k)`可达同速。
- `bytes.set_<型>(mut b, pos, v)`改名`bytes.put_<型>`。`list.set`、`map.set`、`bytes.set`为复制。`set_at`与`b[i] = v`重复。

**理由：**
- (a) 9种语言分3种风格：名称区分（Kotlin `sort`/`sorted`、Swift `-ed`/`form`），类型/返回值区分（Rust、Go、OCaml），根本不改写（Gleam、Elixir）。Almide为值语义（赋值复制）和函数式风格。默认复制、`sort`返回复制，与Gleam/Elixir/OCaml List同风格。在此之上，通过“封闭动词集合”吸收Swift/Kotlin按名称区分的做法。
  - `append_<型>`与`put_<型>`对应Go `encoding/binary`的`AppendUint32`（增长）和`PutUint32`（按位置覆盖）。https://pkg.go.dev/encoding/binary
- (b) 无影响，原地改写已有native `&mut`及wasm写回两条路径。
- (c) 当前`insert`在map原地，在set/list复制，无法从动词判断模式；且如下R-L0-05，混淆也不报错。
- (d) 改名与移除，按R-L0-14。
- 门禁：从声明检查有`mut`参数的公开函数名称属于上述动词集，且这些动词不用于无`mut`函数（interface JSON的`mut`栏，#3889）。
- issue：#3928。

<a id="r-l0-05-unit-以外の値を文として捨てたらエラーにする---採用"></a>

## R-L0-05 将非Unit值作为语句丢弃时报错——○采纳

**实测（2026-10-21，v0.68.0-rc2之后的develop）：**

```almide
fn main() -> Unit = {
  var s = set.new()
  set.insert(s, 1)        // エラーにならない。何も起きない
  println(int.to_string(set.len(s)))   // 0 と表示される
}
```

将`list.sort(xs)`或`string.trim(s)`写作语句也会默默通过。
当前E042仅拒绝语句中丢弃的`Result`，扩展为“所有非Unit值”。有意丢弃仍写`let _ = f()`。

- (a) Swift默认警告未使用返回值（可用`@discardableResult`关闭）。Rust对带`#[must_use]`的类型/函数警告。Gleam警告未使用纯表达式。
- (b) 仅检查器变化，两目标相同。
- (c) 即使R-L0-04统一动词，LLM仍会因他语言习惯把复制函数当原地写。本规则在检查时阻止，形成名称规则+检查保险两层。
- (d) 语言变化，计入dialect epoch 15的`breaks`。加入前统计spec/examples/stdlib/research/tests/diagnostics及下游44项目的影响数。
- 门禁：新E042 fixture（broken/fixed）及诊断harness确认。
- issue：#3906。

<a id="r-l0-06-無印の名前は安全版既定値版-_or-は--に一本化する---採用"></a>

## R-L0-06 无后缀名称为安全版，默认值版`_or`统一为`??`——○采纳

1. 可失败提取用无后缀名称返回`Option`（`list.get`、`map.get`、`list.first`）。有原因的失败返回`Result[_, String]`（`int.parse`、`fs.read_text`）。
2. 接受默认值的`_or`版（`list.get_or`、`map.get_or`、`bytes.get_or`、`args.option_or`）等同`xs |> list.get(i) ?? d`，故移除。延续ADR-0005将`option.unwrap_or`、`result.unwrap_or`转为`??`。
3. 仅调用方契约违反时停止（abort），如`list.chunk(xs, 0)`。
4. 可失败构造/转换不返回失败哨兵（空串、0、空列表）。无原因返回`Option`，有原因返回`Result[_, String]`（如当前`string.from_codepoint`无效值返回`""`违反此规则）。
5. 数值转换无后缀`to_<型>`对结果类型无法表示的值abort（R-L0-32）。这是第3项“可检查前提违反则abort”的实例，而非例外。其他处理用后缀选择（`_checked`、`_saturating`、`_wrapping`，R-L0-34）。（第六轮重写，原先称“无后缀回绕为例外”。）

- (a) Rust、OCaml Base、Elixir无后缀为安全版；OCaml Stdlib和Kotlin无后缀为异常版，没有多数。Almide无异常，只能选择无后缀安全版。默认值版有Python `get(k, d)`、Kotlin `getOrDefault`、Elixir `get(m, k, default)`，而Rust/Swift/JS/Gleam用运算符或通用函数。
- (c) `??`为CHEATSHEET教的唯一写法，无需记住`get_or`也能写。
- (d) 移除，按R-L0-14。`removed_spellings.rs`提示“`list.get(xs, i) ?? d`”。
- issue：#3913。

<a id="r-l0-07-変換の向き-値の変換は元の型の-to_型入れ物からの構築は先の型の-from_元---採用"></a>

## R-L0-07 转换方向：值转换放源类型`to_<型>`，从容器构造放目标类型`from_<元>`——○采纳

1. 标量（数/字符）转换放源类型模块，用`to_<先の型>`（`int.to_float`、`int8.to_int`）。
2. 从容器/表示构造值的函数放目标类型模块，用`from_<元>`（`map.from_list`、`set.from_list`、`bytes.from_list`、`string.from_codepoint`）。
3. 相同转换两侧都有时，仅保留第1项一侧。`float.from_int`等同`int.to_float`，故移除。
4. 方向对也需类型一致。消解`string.to_bytes`返回`List[Int]`而`bytes.from_string`返回`Bytes`的分歧（L2 string/bytes判定）。

- (a) 源类型侧放`to_X`为5/9（Rust、Gleam、Elixir、OCaml新API、Kotlin）。容器构造用`from_list`的有Gleam、Rust（`from_iter`）、JS（`Array.from`）。此组合与Gleam风格完全相同。
- (c) `x |> int.to_float`形式，从管道起点类型决定函数位置。
- (d) 移除`int.from_int8`系列，添加`int8.to_int`系列。移除`float.from_int`。按R-L0-14。
- 门禁：对照公开`to_<T>`/`from_<T>`对，检查相同转换不在两侧重复。
- issue：#3935。

<a id="r-l0-08-数値変換の-4-種は接尾辞の行列---据え置き穴埋め"></a>

## R-L0-08 数值转换4种为后缀矩阵——○保留+补缺

- `to_<T>`：结果类型不能表示的值（越界、NaN转整数）abort（R-L0-32），舍入位置见R-L0-35。
- `to_<T>_checked`：仅精确可表示时some（R-L0-28）。
- `to_<T>_saturating`：越界则固定到端点值。
- `to_<T>_wrapping`：整数转整数时保留低位并回绕（R-L0-34）。
- 回绕运算`int.wrap_add`、`wrap_sub`、`wrap_mul`及`int.wrap(x, bits)`在`Int`内计算，不用于类型转换（R-L0-34）。
- （第六轮按R-L0-32/R-L0-34/R-L0-35重写。）

- (a) 全部4种独立命名的只有Rust（前缀）与Swift（参数标签），无多数。现有后缀矩阵已有门禁保护，故保留。
  - 也参考9语言外的Zig。Zig不提供单词统一转换，而按含义分`@intCast`（越界为非法行为，安全模式会停止）和`@truncate`（回绕）等（[语言参考](https://ziglang.org/documentation/master/#intCast)）。Rust也有禁止/警告`as`的lint（Clippy `as_conversions`、`cast_possible_truncation`）。要求把意图写在名称上的方向与Almide后缀矩阵一致。
- (c) 矩阵缺口（没有`wrap_sub`）会使LLM按规则猜写时必定失误。
- 不加转换运算符`as`的判定见R-L0-18，从他语言转换形式引导的诊断见R-L0-19。
- issue：#3854（已有）。

<a id="r-l0-09-bool-を返す関数は-is_has_または決まった動詞---採用"></a>

## R-L0-09 返回Bool的函数用`is_`/`has_`或固定动词——○采纳

公开Bool返回函数名以`is_`或`has_`开头，否则用以下封闭动词集之一：`contains`、`exists`、`starts_with`、`ends_with`、`any`、`all`。

- (a) `is`前缀8/9（仅Elixir尾加`?`）。`contains`为5/9，`starts_with`、`ends_with`也为多数。
- (c) 名称即可读出返回Bool。
- 当前例外（L2判定）：`args.flag`、`bytes.eof`、`regex.full_match`、`net.unix_poll`。`bytes.read_bool`是读出Bool的函数，不属范围。
- 族前缀：模块按前缀区分函数族（`net.tcp_*`、`net.udp_*`）时，`is_`放族前缀后（`net.tcp_is_open`）。名称开头为族，其余遵本规则。
- 门禁：检查返回Bool的公开函数名。
- issue：#3895。

<a id="r-l0-10-高階関数の接尾辞の意味を-1-つに決める---採用"></a>

## R-L0-10 高阶函数后缀固定为单一含义——○采纳

| 后缀 | 接受的函数 | 例 |
|---|---|---|
| `_by` | 键函数`(A) -> K` | `sort_by`、`unique_by`、`group_by` |
| `_with` | 代替值接收的函数（合并/比较/生成值） | `zip_with`、`sort_with`（比较）、`regex.replace_with`（生成替换字符串） |
| `_while` | 从开头连续成立的谓词 | `take_while`、`drop_while` |
| `_map` | 返回`Option`的转换，排除none | `filter_map`、`find_map` |
| `flat_map` | 连接返回列表的转换 | `list.flat_map`、`option.flat_map` |

- (a) `_by`在各语言含义相反。Elixir/Kotlin为键函数，Rust `sort_by`和Swift/Gleam `by:`为比较函数。Almide已统一键函数，固定为规则。比较版用Kotlin `sortWith`式`_with`。
- 门禁：从类型检查公开`_by`最后参数为一元键返回函数，`_with`最后参数为函数。
- issue：#3949。

<a id="r-l0-11-長さは-len-だけ---採用既存の撤去を日付に載せる"></a>

## R-L0-11 长度仅`len`——○采纳（安排既有移除日期）

`list.length`、`string.length`已`@deprecated`，但移除epoch未定。在R-L0-14窗口移除。

- (a) 拼写分散（length 4、len 3），按R-L0-00保留当前`len`。
- issue：#3851 A（已有）。

<a id="r-l0-12-範囲は半開区間はみ出しは丸めabort-は契約違反だけ---据え置きゲート"></a>

## R-L0-12 半开区间，超出截边，abort仅契约违反——○保留+门禁

`slice(start, end)`含`start`不含`end`，越界索引截到端点。`take(n)`在`n`超过长度时也返回全部。负索引视为越界，不表示从尾计数。

- (a) 9种语言均半开区间。Python/JS/Gleam/Elixir截边，Rust/Swift panic/trap。Almide无异常，统一截边。
- 门禁：对具有`slice`、`take`、`drop`、`take_end`、`drop_end`的全部类型（list/string/bytes）设置矩阵测试，对比相同边界输入结果。

**指定位置写入：** 越界写入（`list.set`、`list.update`、`list.swap`、`list.remove_at`、`bytes.put_<型>`等）按调用方契约违反abort，与索引赋值`xs[i] = v`已有abort一致。`list.insert`接受`0..=len`，范围外abort。

- (a) Rust/Go/Kotlin/Swift用panic/trap停止越界写入，Python/OCaml用异常。仅Elixir `List.replace_at`默默无操作。
- (c) 当前函数“无操作”、索引赋值“abort”，同一写入两种行为，无操作掩盖错误。
- (d) 语义变化（缩小定义域）。正确位置结果不变，不计入epoch `breaks`（现有规则“缩小定义域不设epoch”）。
- issue：#3944。

<a id="r-l0-13-環境を読む関数は-effect---据え置き穴を塞ぐ"></a>

## R-L0-13 读取环境的函数为effect——○保留（补洞）

按ADR-0022，纯函数可输出和abort，但不能读环境。当前仅`process.env`仍以纯`fn`读环境，归并到`env.get`补洞（#3851 B）。

<a id="r-l0-14-互換の扱い全案で共通---採用"></a>

## R-L0-14 兼容处理（全部提案共用）——○采纳

1. **窗口：** 改名/移除在0.68.0下一发布加`@deprecated(since=15, use="…")`。在`proofs/dialect-epochs.toml`加`[[deprecation]]`块，设`removal_epoch = 16`。使用时E052警告并指替代。
2. **移除：** 在epoch 16 `breaks`移除。之后调用变为带`removed_spellings.rs`提示的错误。
3. **机械改写：** 一对一改名（`bytes.set_u8`→`bytes.put_u8`等）可由`almide fix`改写。形态改变（`list.get_or(xs, i, d)`→`list.get(xs, i) ?? d`）也提供提示与fix。
4. **禁止重赋语义：** 不做同名且旧写法通过类型检查却产生不同结果的改变，因为旧意义会留在LLM训练数据。变义用新名。旧写法必然类型错误的改动（参数`String`→`Regex`，返回`String`→`Option[String]`等），附提示和`almide fix`可允许，因为不会静默变结果。因此返回失败哨兵的无后缀版也保留无后缀名改为安全版（`io.read_line`返回`Option[String]`，移除`read_line_opt`），不以`_opt`等后缀逃避。容器类型改变同理：`fs.read_bytes`、`io.write_bytes`保名由`List[Int]`改为`Bytes`，移除另名存续的`Bytes`版，如`fs.read_bytes_raw`。
5. **语言改变**（R-L0-05）计入epoch 15 `breaks`，影响数写`summary`。
6. **数量：** 各提案写spec/tests/docs/stdlib使用数。

- 先例：epoch 14未设弃用窗口直接移除。本次改名多，因此设置窗口。
- issue：#3904。

<a id="r-l0-15-標準関数は-native-と-wasm-の両方で動く---採用"></a>

## R-L0-15 标准函数同时运行于native与wasm——○采纳

标准函数以native（Rust）和wasm（WASI）行为相同为前提。需要宿主功能的函数，提案写明通过哪条路径支持wasm：
- 标准WASI接口（`wasi:filesystem`、`wasi:sockets`、`wasi:clocks`、`wasi:random`、`wasi:http`、`wasi:cli`）
- 暂定嵌入式host op（与已裁定TCP/unix/shm同形，ADR-0023 step 7退役）
- 移出标准库作为package

不以“保持native专用”为终点（ADR-0027 D8）。
- 第4条路径（第二轮重审追加）：private host capability（ADR-0025/ADR-0029）。`process`全部函数及`testing.assert_aborts`通过嵌入式宿主提供的私有功能支持wasm。标准wasm宿主在加载时拒绝，而非静默返回不同结果。无退役计划，与“暂定host op”分别计数。各L2的(b)明确指定4类之一：标准WASI、暂定host op、private host capability、标准库外（package）。

<a id="r-l0-16-順序のある型には-compare-を置く---採用"></a>

## R-L0-16 有序类型提供`compare`——○采纳

有序类型（`int`、`float`、定宽数、`bigint`、`bool`、`string`、`bytes`、`duration`、`compute`、`uuid`）提供`<mod>.compare(a, b) -> Int`。较小返负，相等0，较大正。`list.sort_with`等比较函数接受此形态（R-L0-10）。

- (a) 多数提供三路比较：Rust `cmp`（`Ordering`）、Go `cmp.Compare`、Kotlin `compareTo`、OCaml `compare`、Gleam `int.compare`（`Order`）、Elixir `compare`。
- (c) 可传`list.sort_with(xs, int.compare)`，无需自写比较函数。
- (d) 添加/改名（`bytes.cmp`改`bytes.compare`）。浮点顺序与`list.sort`同为IEEE totalOrder。
- 门禁：R-L0-G检查全部有序类型模块有`compare`。
- issue：#3902。

<a id="r-l0-17-呼び出しの型は宣言だけで決まる---採用"></a>

## R-L0-17 调用类型仅由声明决定——○采纳

调用effect fn时的类型（是否需`!`、能否直接`??`）只由函数声明（参数/返回类型/`effect`）决定，不得因函数体是intrinsic还是Almide而异。

- 当前分歧（实测）：可写`env.get(k) ?? d`，但Almide函数体的`args.option(k) ?? d`报错，需`args.option(k)! ?? d`。interface JSON均`effect: true, error: string`，声明无法区分。
- 影响标准函数：`env.get`、`args.option`、`args.positional_at`、`io.read_line`（[L2-env](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-env.md) R-env-03）及用户自写返回`Option`的effect fn。
- (c) 同形声明因函数体写法改变调用方式，LLM即使读声明也无法确定正确调用。
- (d) 语言改变。按ADR-0002（失败与effect正交）及ADR-0008（仅显式传播），在issue设计统一哪种形式。若统一后改变已有调用，计入epoch 15 `breaks`。
- 门禁：每声明fixture确认全部标准effect fn的intrinsic版/Almide版接受`??`、`!`方式相同。
- issue：#3900。

<a id="r-l0-18-型を変換する演算子-as-は足さない---不採用"></a>

## R-L0-18 不增加类型转换运算符`as`——×不采纳

判定是否增加如`x as Int8`的单符号类型转换运算符。他语言`as`分以下3种机制：

| 机制 | 例 | Almide承担方式 |
|---|---|---|
| 数值转换 | Rust `x as i8`（回绕，float→int饱和）、Go `int8(x)`、Swift `Int8(x)`（停止） | `to_<型>`、`_checked`、`_saturating`（R-L0-08） |
| 运行时类型检查（向下转换） | Kotlin `as?`、Swift `as?`/`as!`、Go `x.(T)`、Rust `downcast` | 不需要，无继承或`Any`；未知内容值（`Value`等）用`match`分类 |
| 压制类型检查 | TypeScript `x as Foo`、Python `typing.cast` | 不引入，运行时不检查任何东西 |
| 表达式中确定类型 | Swift `[] as [Int]`、`1 as Double` | 类型注解`[]: List[Int]`（实测E018提示也教此形式） |

- (a) 仅Rust用单一`as`做数值转换。Swift `as`不能转数值（`x as Int8`编译错，转换写初始化器`Int8(x)`，[Type Casting](https://docs.swift.org/swift-book/documentation/the-swift-programming-language/typecasting/)）。Rust自身也可用lint禁`as`（[参考](https://doc.rust-lang.org/reference/expressions/operator-expr.html#type-cast-expressions)）。Zig无对应`as`的单词转换，按语义分别命名。
- (b) 不适用（不加）。
- (c) 一个符号按类型对切换回绕/饱和/停止，代码无法看出发生哪种。压制类型检查的`as`是LLM为消除类型错常写的失败形式。
- (d) 不加，无兼容影响。`as`继续仅用于import别名（`import x as y`）及JSON字段重命名。
- 理由总结：数值转换已有R-L0-08命名函数，违反R-L0-01（每角色一种拼写）。其他3用途在Almide类型结构下无需或违背目标。他语言习惯写的`as`由R-L0-19诊断承接。

<a id="r-l0-19-他言語の変換の書き方から変換の関数へ案内する---採用"></a>

## R-L0-19 从他语言转换形式引导到转换函数——○采纳

- 内容：
  1. 发现`x as Int8`、`Int8(x)`、`x.into()`/`x.try_into()`时，按源/目标类型给一个诊断，列函数及越界行为（如`int.to_int8(x)`回绕，`int.to_int8_checked(x)`为`Option`，`int.to_int8_saturating(x)`饱和）。
  2. 从L3“其他语言主要拼写”列生成未定义名称建议表（`ALIASES`），覆盖全部模块（list/string在#3905手工修部分）。
- 实测（develop `9352c73e3`）：`x as Int8`报“无变量`as`”并提示“是否`x`？”，随后对`Int8`另报错。`Int8(x)`称“类型`Int8`不可调用”，`x.into()`称“无方法`into`”，均不指正确函数。
- (a) 他语言写法见R-L0-18表。
- (b) 仅检查器改变。
- (c) R-L0-18不加`as`，Rust/TypeScript/Kotlin/Swift习惯仍会写`as`。首个错误能否到正确函数决定修改存活率。
- (d) 仅新增诊断，无兼容影响。
- 完整性规则：L3他语言列每个拼写，要么为指向现存函数/正确表达式的`ALIASES`键，要么写明排除理由（如Almide已有同名不同义函数）。转换矩阵全部类型对均有`as`、`T(x)`、`into`引导。
- 门禁提案：#3905的`tests/stdlib_alias_table_test.rs`对照生成行与L3列。`tests/diagnostics/`放4种形式例，确认仅一个诊断且指函数名。
- issue：#3958。
- 第二轮追加（issue #3967）：
  3. 不放函数而用语法/表达式的角色也有引导行。另于L3维护一张“角色→正确表达式”表。例：`list.for_each`、`each`、`iter`→`for x in xs { … }`；`result.collect`→`rs |> list.map((r) => r!)`；`option.unwrap`→`o ?? panic("…")`；`result.ok`→`result.to_option`；`string.char_at`→`string.get`；`list.concat`→`xs + ys`；`bigint.add`→`a + b`；`float32.is_nan`→`float32.to_float(x) |> float.is_nan`。
  4. 矩阵有意不设的单元格名称（`bytes.get_u8`→`bytes.get`、`bytes.append_u8`→`bytes.push`、`bytes.get_u64_le`→`get_i64_le`、`json.encode`→`json.stringify`、`value.to_string`→`json.stringify`、`random.next_bytes`→重复`next_int`）必有引导行。
  5. 编辑距离候选若指向不同义的现存函数则不输出。实测错误引导：`list.find_last`→`find_map`、`list.last_index_of`→`index_of`、`list.iter`→`filter`、`bytes.get_u8`→`set_u8`（写函数）、`value.to_string`→`as_string`、`print`→`println`（加换行改变输出）。
  6. `String`/`Bytes`混淆（E005）提示`bytes.from_string(s)`/`bytes.to_string(b)`。`fs.read_bytes`、`hash.fnv1a32`、`io.write_bytes`改为`Bytes`后此混淆会增加。
  7. 如`5 & 3`的他语言位运算符，当前报语法错“缺`)`”。引导（#3905计划）放语法错误路径而非词法路径。

<a id="r-l0-20-critical-プロファイルで黙って折り返す-to_型-を拒む---取り消し-r-l0-32-で解消"></a>

## R-L0-20 critical配置拒绝静默回绕`to_<型>`——×取消（由R-L0-32解决）

- 内容：仅`almide check --profile critical`时，将有损类型对的`to_<型>`报错，要求`_checked`或`_saturating`，与Rust Clippy `cast_possible_truncation`同思路。普通代码不变。
- 暂缓理由：当前critical配置（ALS bounded规则）保证安全/资源上界，不含计算正确性。越界`xs[i]`及除0也未被critical拒绝（2026-10-01实测）。向critical加入正确性（全域性）规则延期至[certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md)的CG-3。先仅拒静默回绕会使critical规则性质不一致，故随全域性规则一同判断。

- 取消（第六轮）：R-L0-32将无后缀窄化改abort，消除静默回绕无后缀项。critical已无拒绝目标，关闭此案。仅名称写`_wrapping`才回绕（R-L0-34）。
- 决定顺序：在ALS（almide/als）加规则→推进pin→实现检查器（`check/bounded.rs`）。
- issue：无（CG-3重启时共同建项）。

<a id="r-l0-21-無印の-to_型-をはみ出したら失敗する版に変える--保留--r-l0-32r-l0-34-で解消"></a>

## R-L0-21 无后缀`to_<型>`改为越界失败——暂缓→由R-L0-32/R-L0-34解决

- 内容：有损类型对`to_<型>`改返回`Option`（如当前`_checked`），回绕版用`to_<型>_wrapping`。Zig/Swift形式，消除R-L0-06第5项例外。

- 解决（第六轮）：无后缀越界失败被R-L0-32采纳（不是none而是abort），回绕用`_wrapping`被R-L0-34采纳。下文保留当时暂缓记录。
- (a) 有单词有损转换的语言中，回绕4种：Rust `as`、Go、Kotlin、OCaml；停止1种：Swift。未达R-L0-00改名条件5/9。
- (b) 纯计算，两目标相同。
- (c) `int.to_int8(300)`默默变`44`的bug可通过类型检查和运行，此形式可避免，但每次转换需`??`或`!`。
- (d) 语义变化。返回类型改变使旧代码类型错，满足R-L0-14.4。`almide fix`可改为`to_<型>_wrapping`。影响全部有损类型对的大改。
- 暂缓理由：无多数支持，先在almide-dojo测LLM实际写静默回绕bug的频度再决定。
- issue：[almide-dojo#63](https://github.com/almide/almide-dojo/issues/63)（测量）。

<a id="r-l0-22-組み込みの-ord-プロトコルのメソッド名を-compare-にする---採用"></a>

## R-L0-22 内置`Ord`协议方法改名`compare`——○采纳

- 当前：内置`Ord`方法为`fn cmp(a: Self, b: Self) -> Int`（[type-system.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/type-system.md) §10）。而R-L0-16在有序类型模块设`<mod>.compare(a, b) -> Int`。`Duration`派生`Ord`，同一比较产生`d.cmp(e)`与`duration.compare(d, e)`两种拼写。
- 提案：`Ord`方法改`compare`。用户`fn T.cmp(a: T, b: T) -> Int`变`fn T.compare(…)`。内置类型`Ord`方法和模块`compare`函数同名，成为一种拼写。
- 依据L0规则：R-L0-01（每角色一种拼写）、R-L0-16、R-L0-00（内部冲突可改名）。
- (a) 三路比较叫`compare`的有Go（`cmp.Compare`）、Gleam、Elixir、OCaml、Kotlin（`compareTo`）；仅Rust叫`cmp`。
- (b) 仅改检查器及按名查方法处。
- (c) `list.sort_with(xs, duration.compare)`与`a.compare(b)`同词，从任一侧学习均可写。
- (d) 改名。epoch 15将`cmp`标`@deprecated(since=15, use="compare")`，epoch 16移除。`almide fix`改写`fn T.cmp`声明及`.cmp(`调用。
- 实测（develop `437b8f127`）：`duration.ms(5).cmp(duration.s(1))`通过`almide check`，但构建内部错误（`[COMPILER BUG] internal type resolution failed`）。此bug应独立于拼写修复，另建issue #3960。
- 完整性规则：所有有`Ord`的标准类型都有`<mod>.compare`，且它本身即`Ord`方法（不另设转发函数）。
- 门禁提案：R-L0-G确认每个标准`Ord`类型的`compare`可解析且check/build均通过。
- issue：#3961。

<a id="r-l0-23-状態を進める関数は-next_-で始め値-次の状態-を返す---採用"></a>

## R-L0-23 推进状态的函数以 `next_` 开头，返回 `(値, 次の状態)` — ○ 采纳

- 当前：有两个接收状态、返回值与下一状态的函数族，名称和返回顺序不一致。
  - `random.next_int(rng, min, max) -> (Int, Rng)`：名称为 `next_`，值在前。
  - `bytes.read_i32_le_at(b, pos) -> (Int, Option[Int])`、`bytes.take_at(b, pos, n) -> (Int, Option[Bytes])`：名称为 `read_…_at`，下一位置在前。另有只按指定位置读取的 `bytes.get_i32_le(b, pos)`。
- 提案：推进状态的函数命名为 `next_<何>`，按 `(値, 次の状態)`（值、下一状态）顺序返回。bytes 推进读取位置的函数族改名为 `bytes.next_<型>(b, pos) -> (Option[T], Int)` 和 `bytes.next_bytes(b, pos, n) -> (Option[Bytes], Int)`（详情见 [L2-bytes](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bytes.md) R-bytes-13）。
- 依据的 L0 规则：R-L0-01（相同形态的职责使用相同拼写）。R-L0-03 的“主语在前”是参数规则，未规定返回顺序，因此在此决定。
- (a) 显式传递状态的随机 API 中，Haskell 的 `random :: g -> (a, g)`、Elixir 的 `:rand.uniform_s` 使用 `{値, 状態}`（值、状态）；解析器惯例也是 `(値, 残り)`（值、剩余部分），值在前。Rust 常用的 bytes 库中，`get_i32_le` 是推进读取位置的名称，和 Almide 的 `get_<型>`（只按指定位置读取）相反。采用 `next_` 也能避免此混淆。
- (b) 纯计算。
- (c) 所有模块统一为“`next_` 推进状态，`get_` 只按位置读取”。`let (v, pos) = …` 的解构顺序也只有一种。
- (d) 改名及语义变化。返回元组的类型改变，旧代码将以类型错误停止（满足 R-L0-14.4）。epoch 15 将旧名标为 `@deprecated`，epoch 16 撤除。`almide fix` 改写名称，但无法改写元组解构顺序，因此给出提示。
- 完整性规则：返回类型为含状态类型的元组的公开函数，以 `next_` 开头，状态位于元组最后。
- 门禁提案：向 R-L0-G 测试加入此形态检查（状态类型为 `Rng` 和返回读取位置 `Int` 的 bytes 函数族）。
- issue: #3962

<a id="r-l0-24-標準の面の型は正規の綴りで書くt---採用"></a>

## R-L0-24 标准接口的类型使用规范拼写（`T?`）— ○ 采纳

- 当前：ADR-0010 已将 `T?` 定为 `Option[T]` 的规范拼写，`almide fmt` 也改写为 `T?`。然而标准函数声明混有 199 个 `Option[T]` 和 23 个 `T?`。LLM 阅读的 docs/stdlib 和 interface JSON 也原样呈现这些声明。
- 提案：标准 `.almd` 的声明统一为 `T?`，docs/stdlib、interface JSON 及本记录的 [target-surface.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/target-surface.md) 也显示规范拼写。
- 依据的 L0 规则：R-L0-01（同一类型只有一种拼写）、ADR-0010。
- (b)、(d) 仅拼写变化，类型和行为不变，无兼容影响。
- (c) LLM 在文档中见到两种拼写会犹豫哪种正确。只展示规范拼写，也与 `almide fmt` 结果一致。
- 完整性规则：标准公开函数声明不出现 `Option[`。
- 门禁提案：针对 `stdlib/*.almd` 的公开函数声明，应用与 `almide fmt --check --no-import-edit` 相同的规范化，确认无差异。
- issue: #3963


<a id="r-l0-25-文字列との変換の動詞を表で決める---採用"></a>

## R-L0-25 用表规定字符串转换动词 — ○ 采纳

| 类别 | 转为字符串 | 从字符串转换 |
|---|---|---|
| 有类型的值（`Url`、`Uuid`、`Int`、`BigInt`、`Regex` 等） | `to_string` | `parse`（Result） |
| 同一值的不同表示（`Bytes` 等） | `to_<表現>` | `from_<表現>` |
| 保持内容、只改变字符排列的编码（base64、hex、URL component、form） | `encode_<変種>`（无变种则为 `encode`） | `decode_<変種>`（Result） |
| 例外 | `json.stringify`（训练数据中压倒性的拼写；R-json-03）、`url.build_query`（`parse_query` 的逆操作；无多数派） | — |

- 依据的 L0 规则：R-L0-07、R-L0-01、R-L0-00。第二轮复查发现“拼写已统一，但未写成规则”，因此加入此表（`path.from_string` 将由 R-path-02 删除）。`build_query` 无多数派，故改名提案不予采纳（×）。
- (c) 新增模块时拼写不再摇摆（uuid 的 `parse`、`to_string`、`from_bytes`、`to_bytes` 已遵循此表）。
- 门禁提案：向 R-L0-G 加入“从 `String` 返回 `Result[T, _]` 的公开函数名称须为 `parse`、`parse_<x>`、`decode_<x>` 或 `from_<x>`”。允许列表从上述两个例外开始，只允许缩减。
- issue: #3971

<a id="r-l0-26-演算子を持つ型は演算子と脱糖先の表を-1-か所で宣言する---採用"></a>

## R-L0-26 有运算符的类型在一处声明运算符及其脱糖目标表 — ○ 采纳

- 当前：哪些类型支持哪些运算符，分别写在检查器与 lowering 中。不一致实例：`-m`（Matrix 一元负号）能通过 `almide check`，却在 lowering 产生内部错误（“NegInt expects Int operand”）（#3965）。bigint 和 matrix 还将运算符脱糖目标（`bigint.add`、`matrix.add` 等）作为公开函数，同一计算有两种拼写。
- 提案：
  1. 在一处声明类型 × 运算符（`+ - * / %`、一元 `-`、比较）表，检查器和 lowering 都读取它。表中不存在的单元格产生带诊断的类型错误，而非内部错误。
  2. 不公开运算符脱糖目标（使用 `__add` 等内部名称）。公开写法只有运算符。例外仅限其他类型也有同名、且语义不同于运算符的函数（`floor_div`、`modulo`）（R-bigint-04、R-matrix-14）。
- 依据的 L0 规则：R-L0-01、R-L0-15、R-L0-17（调用形式由声明决定）。
- (c) LLM 看到能用 `+` 就也会写 `-m`。可避免此处产生用户无法修复的内部错误。
- 完整性规则：每个有运算符的类型，表中每个单元格要么有脱糖目标，要么产生带诊断的类型错误。
- 门禁提案：为所有单元格放置固定 `almide check` 和构建结果的 fixture。
- issue: #3973

---

<a id="r-l0-27-next_-が-none-を返すときは状態を進めない---採用"></a>

## R-L0-27 `next_` 返回 none 时不推进状态 — ○ 采纳

- 当前：R-L0-23 已规定 `next_<何>` 返回 `(値, 次の状態)`，但未规定值为 none 时下一状态如何。`bytes.next_u32_le(b, pos)` 字节不足，以及 `random.next_choice(rng, [])` 时，第二个值尚未确定。
- 提案：值为 none 时原样返回接收的状态（不移动读取位置，不推进随机状态）。不读取值的位置移动（`bytes.skip(b, pos, n) -> Int`）没有值，因此不返回 R-L0-23 的二元组，只返回下一状态。
- 依据的 L0 规则：R-L0-23、R-L0-06（可能失败的提取返回 none，不改变其他值）。
- (a) Rust 的 `Iterator::next` 即使返回 None，也不改变下一次调用的含义。解析器惯例（失败不消耗输入）也相同。
- (b) 纯计算。
- (c) 不足时改用另一类型重读，无须恢复位置。
- (d) 只是写明新函数的约定，现有函数不变。
- 门禁提案：向 `next_` 函数族的矩阵测试加入“none 时第二个值等于参数”。
- issue: #3979

<a id="r-l0-28-_checked-の変換は正確に表せるときだけ-some---採用記録の訂正"></a>

## R-L0-28 `_checked` 转换“仅精确可表示时返回 some” — ○ 采纳（记录订正）

- 当前：R-L0-08 描述为“`to_<T>_checked`：越界时以 `Option` 返回 none”。实现更严格，无法精确表示时返回 none（实测 `almide 0.68.0 dev 437b8f127`：`float.to_int32_checked(1.5)` 为 none，`int.to_float32_checked(16777217)` 为 none，`int.to_float32_checked(16777216)` 为 `some(16777216)`）。
- 提案：使规则措辞与实现一致：“`to_<T>_checked`：仅结果与原值精确相等时返回 some。越界、截去小数部分、精度损失均返回 none”。与 Swift `init?(exactly:)` 语义相同。
- (d) 仅订正文档。
- issue: 无

<a id="r-l0-29-負の開始位置を丸めずに-abort-にする---採用保留から変更"></a>

## R-L0-29 负的起始位置不再截到边界，而是 abort — ○ 采纳（由暂缓改为采纳）

- 当前：R-L0-12 为“范围是半开区间；越界截到边界；abort 仅用于违反契约”。`list.slice(xs, -3, len)` 将起点截到 0，不停止而返回另一值。按 Python、JS“负索引从末尾计数”的习惯编写的代码会静默得到不同结果。
- 提案：起止位置为负时（`slice`、`take`、`drop` 的数量为负也包括在内）abort。欲从末尾计数则使用 `take_end`、`drop_end`。正方向越界（大于 `len`）仍按原规则截到边界。
- 暂缓理由：R-L0-12 是带门禁决定的规则，与正方向截到边界配套。先在 Dojo 测量负索引误写频率再决定（与 R-L0-21 相同处理）。
- (a) Rust 和 Go panic，Python 和 JS 从末尾计数。所比较语言中无静默截到 0 的语言。
- 判定变更（第五轮）：3 次外部评审连续将其列为最严重指摘之一，且比较语言中无任何语言静默截到 0，因此不等待测量，直接采纳。与 R-L0-32（无标记窄化转换）、R-L0-33（按职责区分越界处理）一致，采用“违反调用方可检查的前提则 abort”（R-L0-06 第 3 项）。
- (d) 语义变化。目前截到边界返回的调用将 abort，纳入 epoch 15 的 `breaks`。保留 Dojo 测量（almide-dojo#63），作为采纳后统计误写数量的项目。
- issue: #3990

<a id="r-l0-30-引数名は役割ごとに-1-つ---採用"></a>

## R-L0-30 每种职责只有一个参数名 — ○ 采纳

- 当前：L0 已统一函数名，但未统一参数名。同一职责有两种以上拼写。
  - `Bytes` 主语：`b` 149 处、`data` 12 处（`zlib.*`、`hash.fnv1a32`、`matrix.from_bytes_*`）。向外写出的字节串（`fs.write_bytes`、`net.tcp_write` 等）统一为 `data`。
  - 数值主语：`int`、`float` 用 `n`，定宽类型用 `x`。
  - 元素：`x`（`list.push`）、`val`（`list.insert`、`bytes.*`）、`value`（`set.insert`）。
  - 索引：`bytes.get` 用 `i`，`bytes.insert` 用 `pos`。
  - 函数参数：大多为 `f`，`result.filter` 用 `pred`，`heap.new_by` 用 `key`。
  - 模式：`regex.new` 用 `pat`，`fs.glob`、`http.route` 用 `pattern`。
- 参数名是公开契约。Almide 有命名参数（`list.len(xs: [1, 2])` 可通过，实测 `almide 0.68.0 dev`），签名出现在 `llms.txt` 和 cheatsheet 中。
- 提案：每种职责确定一个名字。

| 职责 | 名称 |
|---|---|
| 主语 `List`、`Bytes`、`Map`、`Set`、`Matrix`、`Value`、`Result`、`Option` | `xs`、`b`、`m`、`s`、`m`、`v`、`r`、`o` |
| 向外写出的字节串（`io`、`fs`、`net`、`unix`、`http`、`process`） | `data` |
| `int`、`float`、`bigint`、定宽数值模块的主语 | `x`（二元运算用 `a`、`b`） |
| `path` 主语 | `path`（与 `fs` 相同） |
| 元素值（list、set、deque、heap、bytes） | `x`（只有 `map` 用成对的 `key`、`value`。并非容器元素的值，如 `env.set(name, value)`，仍用职责名称） |
| 元素索引 | `i` |
| 有类型读写（`bytes.get_*`、`put_*`、`next_*`）的字节位置 | `pos` |
| 函数参数 | `f`；比较函数为 `cmp` |
| 命名函数类型的参数（`HttpHandler`、`HttpMiddleware`） | 类型职责名（`handler`、`middleware`） |
| 对等接收两个同类型值的函数（`compare`、`min`、`max`、`merge`、`union`、`xor`、`zip`） | `a`、`b`（优先于主语命名规则） |
| 模式字符串（`regex`、`fs.glob`、`http.route`） | `pattern` |
| 数量（`duration.ms(n)`、`random.bytes(n)`、`list.take(xs, n)`） | `n` |

- (c) 命名参数写旧名会出现 E004“no parameter named …”，提示正确名称，不会静默产生不同值。
- (d) 改名。`spec/`、`stdlib/`、`examples/`、exercise 中，以命名参数调用标准函数的地方为 0。纳入 epoch 15 的 `breaks`，用 `almide fix` 改写。
- 完整性规则：按职责分类所有公开函数参数，同一职责有第二种拼写则门禁失败。同一签名中出现两个相同参数名也失败（第四轮改写产生了 `bytes.compare(b, b)`，评审发现）。
- 列表（[target-surface.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/target-surface.md)）的签名使用本表名称。
- issue: #3987

<a id="r-l0-31-同じ形を持つ型の組はプロトコルで宣言する---採用"></a>

## R-L0-31 通过协议声明具有相同形态的类型组 — ○ 采纳

- 当前：R-duration-03 用 `TimeAmount` 协议声明了 `Duration` 与 `Compute` 形态相同。还有其他组为每个类型排列相同函数，用手写矩阵门禁或文字维护一致性。
- 提案：用标准协议声明“具有相同职责、同名同形函数的类型组”。协议检查通过类型检查阻止仅一方缺少函数的偏差。只写一个泛型函数即可用于组内任意类型。
- 调查的组及判定：

| 组 | 协议 | 判定 | 内容 |
|---|---|---|---|
| `Duration`、`Compute` | `TimeAmount` | ○（R-duration-03） | 6 个单位构造器和 `compare` |
| `HttpRequest`、`HttpResponse` | `HttpMessage` | ○（R-http-12） | `header`、`header_values`、`headers`、`body`、`body_bytes`、`set_header`、`set_body`、`set_body_bytes`。带 `req_` 的 6 个函数和 `get_header` 合并为单一泛型函数 |
| `int` 和 8 种定宽整数 | `Integer` | ○（R-sized-08） | `compare`、`min`、`max`、`clamp`、`parse`、`parse_radix`、`to_string`、`to_string_radix`、`min_value`、`max_value` |
| `float`、`float32`、`float64` | `FloatingPoint` | ○（R-sized-08） | 与 `Integer` 相同的选择、比较族，加值分类（`is_nan`、`is_infinite`、`is_finite`）和特殊值（`nan`、`infinity`、`epsilon`） |
| list、string、bytes、map、set、deque、heap | `Sized` | ○（本提案） | `len`、`is_empty`；7 个容器均已具备 |
| list、string、bytes、map、set、deque、heap 的 `contains`、`count` | — | × | `map.contains` 接收键，`map.count` 接收 `(K, V)`，元素类型无法唯一确定 |
| 有 `parse` 的类型（`int`、`bool`、`uuid`、`url` 等） | `Parse` | × | 泛型函数内部不能写 `T.parse(s)`（实测 `almide 0.68.0 dev`：协议可声明无参数函数，但 `T.parse` 为 E002）。语言加入“按类型选择的函数调用”后再提出 |
| `option`、`result` 的 `map`、`flat_map` | — | × | 语言没有抽象 `Option[_]` 这种“接收类型的类型”的机制（高阶类型） |
| `base64`、`hex` 等编码 | — | × | 是模块组而非类型组，没有承载协议的值。通过 R-L0-25 表统一拼写 |

- 协议仅保证表中“内容”列列出的函数一致。仅一方存在的函数（`float.floor`、`round`、`signum` 等；`float32`、`float64` 先 `to_float` 再用 `float.*`）不在协议范围内。
- 运算符（`+`、`<` 等）不纳入协议（与 R-duration-03 相同；在 R-L0-26 运算符表中声明）。
- 无参数函数（`min_value`、`nan`、6 个单位构造器）也写入协议以检查“具备它们”。当前语言不能从泛型函数内部调用。
- 依据的 L0 规则：R-L0-01（同一职责不在两处手写）、R-L0-12（用矩阵统一函数族）；CLAUDE.md 的“从单处声明推导性质”。
- (a) Swift `BinaryInteger`、`BinaryFloatingPoint`、`Collection`，Rust `num-traits` 的 `PrimInt`、`Float`，Go 的 `constraints.Integer`、`constraints.Float`。
- (b) 检查器变化，值表示不变。
- (c) 将 `Float` 换为 `Float32` 的代码仍可使用相同函数名，这由类型声明保证。
- (d) 增加声明。补齐缺少函数的部分（R-sized-08）为新增。
- 门禁提案：针对每个声明类型，在 `spec/stdlib/` 放置协议泛型函数测试。替换 R-L0-12 手写矩阵门禁中协议覆盖的部分。
- issue: #3989

<a id="r-l0-32-無印の狭める変換は範囲外と-nan-で-abort-にする入力の型によらない---採用"></a>

## R-L0-32 无标记窄化转换在越界或 NaN 时 abort，不依赖输入类型 — ○ 采纳

- 当前：R-L0-08 无标记的 `to_<T>` 随输入类型不同而采用不同越界处理。整数到整数回绕，浮点到整数饱和，NaN 为 0（实测 `almide 0.68.0 dev 437b8f127`，native 与 wasm 相同：`int.to_int8(300)` = 44、`float.to_int8(300.0)` = 127、`int.to_uint8(-1)` = 255、`float.to_uint8(-1.0)` = 0）。仅改写输入类型的代码便不停止而返回不同值。
- 提案：无标记窄化转换在结果类型无法表示时（越界、NaN）abort。浮点小数部分截断（`float.to_int(1.5)` = 1）维持现状。其他处理按名称选择。
  - `_checked`：不能精确表示则 none（R-L0-28）。
  - `_saturating`：饱和到端点。也为浮点输入增加此单元（`float`、`float32`、`float64` → 9 种整数，共 27 函数）。当前无标记行为移到这里。
  - 回绕使用 `_wrapping` 单元（R-L0-34；第六轮从引导至 `int.wrap` 改为此方案）。
- 依据的 L0 规则：R-L0-06 第 3 项（违反调用方可检查的前提则 abort）；R-L0-08 矩阵（有损转换具有无标记、`_checked`、`_saturating` 三种）。
- (a) Swift `Int8(x)` 越界 trap，通过 `init(exactly:)`、`init(clamping:)`、`init(truncatingIfNeeded:)` 选择处理。Rust `as` 随类型区分回绕与饱和，正是评审指出的不一致。
- (b) 纯计算，两种 target 判定相同。
- (c) 误写在当场 abort，而非静默得到另一值。
- (d) 语义变化。当前回绕返回的调用将 abort，纳入 epoch 15 的 `breaks`。`almide fix` 将浮点输入的无标记转换改为 `_saturating`（保持行为），整数输入的无标记转换改为 `_wrapping`（保持行为；R-L0-34）。
- 完整性规则（替换 R-L0-08）：任何输入类型的有损转换都有无标记（abort）、`_checked`、`_saturating` 三种。整数到整数的有损转换再加 `_wrapping`，共四种（R-L0-34）。浮点目标类型遵循 R-L0-35。
- issue: #3990

<a id="r-l0-33-範囲外の扱いを関数の役割で分ける---採用"></a>

## R-L0-33 按函数职责区分越界处理 — ○ 采纳

- 当前：R-L0-12 仅一句“范围是半开区间；越界截到边界；abort 仅用于违反契约”。各页又写明，按位置读一个值返回 none，写一个值则 abort。必须逐函数推测该用哪条。
- 提案：统一为一张职责表。

| 职责 | 例子 | 正方向越界 | 负位置、负数量 |
|---|---|---|---|
| 提取范围 | `slice`、`take`、`drop`、`take_end`、`drop_end`、`byte_slice`、`bytes.slice` | 截到长度 | abort（R-L0-29） |
| 读取一个位置 | `get`、`first`、`last`、`bytes.get_*`、`next_*` | none（`next_` 不推进状态；R-L0-27） | none |
| 写入、插入、删除一个位置 | `set`、`update`、`swap`、`insert`、`remove_at`、`bytes.put_*` | abort | abort |
| 按键更新现有元素 | `map.update` | 无键则 abort（R-map-08） | — |
| 以形状为前提 | `matrix` 运算、`bytes.xor`、`chunk(n <= 0)` | abort | abort |
| 创建指定数量 | `list.repeat(x, n)`、`string.repeat` | — | abort |

- 依据的 L0 规则：R-L0-06（可能失败的提取为 none，可检查的前提被违反则 abort）、R-L0-12。
- (a) Rust `get` 为 None，`v[i]` 和 `split_at` panic，`iter().take(n)` 截到边界。Kotlin `getOrNull` 为 null，`take(-1)` 抛异常。
- (d) 表中大部分重述现状。变化为 R-L0-29（负位置）、R-map-08（`map.update`），以及原先对负数截到边界的 `repeat`。
- issue: #3990

<a id="r-l0-34-整数から整数への損失のある変換に-_wrapping-を置く---採用"></a>

## R-L0-34 为整数到整数的有损转换提供 `_wrapping` — ○ 采纳

- 当前：R-L0-32 原先规定用 `int.to_int8(int.wrap(x, 8))` 回绕。`int.wrap(x: Int, bits: Int) -> Int` 是 `Int` 内的计算，无法接收超出 `Int` 的 `UInt64` 上半区（2^63 以上），也无法返回回绕后的 `UInt64`。R-L0-32 使 `uint64.to_int(uint64.max_value())` abort，因此不存在经过 `UInt64` 的回绕路径。
- 提案：为整数（`Int` 与 8 种定宽整数）到整数的有损转换提供 `to_<T>_wrapping`（46 个函数）。保留低位，按结果类型范围回绕（二进制补码）。`int.wrap`、`wrap_add`、`wrap_sub`、`wrap_mul` 保留为 `Int` 内计算，不用于类型转换引导。
  - 例：`int.to_uint8_wrapping(300)` = 44、`int.to_uint64_wrapping(-1)` = `uint64.max_value()`、`uint64.to_int_wrapping(uint64.max_value())` = -1。
  - `UInt64` 计算使用 `uint64` 之间的运算符（R-L0-26）。转到 `Int` 时选择 `to_int`（越界 abort）、`to_int_checked`、`to_int_saturating`、`to_int_wrapping`。
  - 不为 `BigInt` 输入提供（提取任意长度整数低位可用 `bigint` 取余表达）。
- 依据的 L0 规则：R-L0-08（用后缀矩阵选择转换处理）、R-L0-32 完整性规则、CLAUDE.md 的“函数族用矩阵统一”。
- (a) Swift 通过名称 `init(truncatingIfNeeded:)` 选择回绕（F）。Rust `as`、Go `T(x)`、Kotlin `toByte()` 的单词转换直接回绕。`_wrapping` 拼写对齐 Rust 的 `wrapping_add` 等前缀及 Almide 的 `wrap_add`。
- (b) 纯计算，两种 target 相同。
- (c) 其他语言用 `as` 表达的意图（回绕）可用唯一名称表达。`almide fix` 将整数输入无标记转换改为 `_wrapping` 即可保持当前行为（以此替换 R-L0-32(d)）。
- (d) 新增（46 个函数）。
- 完整性规则：有损整数类型对具备无标记、`_checked`、`_saturating`、`_wrapping` 四种。向矩阵门禁（R-sized-01）加入此列。
- issue: #3995

<a id="r-l0-35-無印の変換の丸めと-nan-を変換先が整数か浮動小数点かで分けて書く---採用"></a>

## R-L0-35 按整数或浮点目标分别规定无标记转换的舍入与 NaN — ○ 采纳

- 当前：R-L0-32“结果类型无法表示的值则 abort”可能被理解为也适用于小数截断、转 `Float32` 的精度损失、浮点类型间 NaN。
- 提案：采用下表，保持当前实现行为，只确定措辞。

| 目标 | 小数部分、精度 | 越界 | NaN |
|---|---|---|---|
| 整数（`float.to_int` 等） | 向 0 截断（`float.to_int(1.5)` = 1、`float.to_int(-1.5)` = -1） | abort | abort |
| 整数（从整数） | — | abort | — |
| 浮点（`float.to_float32`、`int.to_float32`、`int.to_float` 等） | 舍入到最近值，中点取偶（`int.to_float32(16777217)` = 16777216.0） | 无穷大（`float.to_float32(1e300)` = 无穷大） | 仍为 NaN |

- 所有行的 `_checked` 都是“仅精确可表示时 some”（R-L0-28）。截断、舍入、越界、NaN 均为 none。与无标记转换的差别是后者允许上表的舍入。
- 依据的 L0 规则：R-L0-28、R-L0-32。
- (a) IEEE 754 默认转换（舍入到最近值），以及 Rust、Swift、Go、Kotlin 浮点转整数时向 0 截断。
- (d) 仅确定文档。
- issue: #3995

<a id="r-l0-36-間違えても型エラーになるはそれだけでは不採用の理由にしない---採用"></a>

## R-L0-36 不能仅以“误写也会成为类型错误”作为不采纳理由 — ○ 采纳

- 当前：部分不采纳理由为“误写也会成为类型错误，不会静默通过”。Almide 的指标是修改存活率，首次代码产生类型错误也计为失败。虽优于静默返回不同值，却降低“无需修改即正确运行的比例”。
- 提案：将类型错误写为减轻危害的因素。不采纳本身的理由应是变更会丧失的具体性质（现有代码静默产生不同值、同一职责出现两种拼写等），或测量结果。两者都没有则暂缓，在 Dojo 测量各候选的存活率。
- 复查的提案：R-map-04 改为暂缓（Dojo 测量）。R-datetime-04、R-duration-04、R-result-02、R-option-03、R-regex-06 有类型错误以外的理由（同职责两种拼写、由语义决定的形态、ADR 决定），故保持。
- (d) 增加判定规则，函数不变。
- issue: 无
<a id="r-l0-37-標準の具体型はprotocol-の組み合わせとその型だけの関数でできている---採用方向"></a>

## R-L0-37 标准具体类型由协议组合和该类型特有函数构成 — ○ 采纳（方向）

- 当前：各类型重复排列同职责函数，通过规则（R-L0-16 等）与手写矩阵门禁维护名称、形态一致。R-L0-31 将部分组（`Integer`、`FloatingPoint`、`Sized`、`TimeAmount`、`HttpMessage`）做成协议。
- 提案：将此作为所有标准类型的构造方式。
  1. 两个以上类型拥有的同职责函数，必须作为协议方法仅声明一次。
  2. 类型模块函数（`int.compare`、`list.len`）就是协议方法本身，不另设转发函数（将 R-L0-22 扩展到所有协议）。
  3. 类型公开面可由“遵循的协议列表”加“该类型特有函数”描述。
  4. 协议组成层级，参照 Swift 标准库（`Equatable`→`Comparable`、`AdditiveArithmetic`→`Numeric`→`BinaryInteger`→`FixedWidthInteger`、`Sequence`→`Collection`→`BidirectionalCollection`→`RandomAccessCollection` 等）。Swift 没有的层需说明理由再加入。[protocol-view.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) 包含层级、各类型遵循关系、空缺及由层级决定的事项（由协议定义与遵循关系对照 target-surface.md 机械生成）。R-L0-31 命名的协议（`Integer`、`FloatingPoint`、`Sized`、`TimeAmount`、`HttpMessage`）重新放入层级，内容遵循层级定义。
- 不“全用协议”的理由：将仅一个类型拥有的 `string.split`、`regex.find` 等函数做成只有一个成员的协议，只增加名字，无法维护一致性。Swift 也有具体类型特有方法。
- 实测（重设计后接口）：确定层级时，31 种值类型的 1,041 个函数中，663 个为协议要求或数值转换族，尚缺要求函数 209 个（其中 168 个是 8 种定宽整数缺少 `int` 已有的算术、位运算）。第七轮增加 164 个函数补缺，第九轮将转换归并到目标 `from`（R-L0-40），第十轮将序列算法归并为 `Sequence`、`Ordered` 默认实现（R-L0-42）。位运算仍为函数，是 `Integer` 要求（R-int-10、R-L0-43）。重设计后共 1,291 函数，协议函数只计一次时，需记忆名称约 650 个。
- 语言缺少的能力（实测 `almide 0.68.0 dev`，目前均无法表达）：
  - ~~通过类型参数调用无接收者方法~~ → 已加入（wave 2 后，2026-10-24）。在 `[M: Monoid]` 中写 `M.empty()`，单态化时成为具体类型函数（声明类型为 `fn T.empty`，内置类型为标准库模块函数）。`list.sum` 使用此能力（`spec/lang/protocol_static_requirement_test.almd`）。
  - 协议继承（`protocol Integer: Ord`）。
  - 关联类型（`type Elem`）。容器 `contains`、`count` 协议需要它（这正是 R-L0-31 判 × 的理由）。
  - 默认实现（由 `compare` 推导 `min`、`max`、`clamp`）。
  - 条件遵循（仅 `A: Hash` 时 `List[A]` 才为 `Hash`）。
  - 不需要以容器本身为参数的类型（`F[_]`）。与 Swift 一样，`map` 不纳入协议，保持各类型函数。
- 推进方式：先写 ADR 再加入 5 项能力。加入前用 Dojo 确认协议泛型函数不会降低 LLM 编写代码的存活率。类型专用写法（`int.compare(a, b)`）是同一函数，因此两种都可用。
- 定位（第十五轮补充）：[协议层级](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md)是**设想设计**。当前实现需遵守[重设计后的函数列表](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/target-surface.md)中的名称、形态，即用户可见公开面。内部可仍为各类型具体函数。层级是未来目标，随着上述 5 项语言能力逐项加入，再将内部向其靠拢。
  - 列表中泛型函数（`list.sum[A: Numeric]`、`list.join[T: Text]`、数值类型的 `from[S: Numeric]` 等）已按列表形态实现（2026-10-24）。`Numeric`、`Integer`、`Text` 为内置协议，遵循它们的内置类型表与要求在编译器一处（`BUILTIN_PROTOCOLS`）声明。要求包含不公开的内部函数（`__trunc`、`__int_exact`、`__sum`、`__join` 等，`__` 开头且不出现在列表）。泛型函数体通过类型参数调用要求，单态化替换为具体类型模块函数。`almide docs-gen --check` 的矩阵门禁确认遵循类型具备全部要求。`scripts/check-stdlib-target.py` 的 `lang_wait` 为 0。
  - 仍为设想设计的是层级本身（继承、关联类型、默认实现、条件遵循），以及让内置类型遵循用户声明的协议。公开函数中没有仍仅为设想设计的函数。
  - 用户可声明协议能表达的部分（`io.read[H: Read]`、`http.header[M: HttpMessage]`），现在按列表形态实现。
- (d) 声明方向。第七轮提案（R-L0-38、R-sized-10/11、R-bigint-07、R-bool-03、R-list-20、R-string-16、R-deque-03/04）增加补齐层级要求的函数。R-L0-31 判 × 的行（`contains`、`count`、`Parse`）重新置于该层级的 `Membership`、`Container`、`Parse`。
- issue: #4000

<a id="r-l0-38-組み込みの-repr-のメソッド名を-to_string-にする---採用"></a>

## R-L0-38 将内置 `Repr` 方法命名为 `to_string` — ○ 采纳

- 当前：内置 `Repr` 方法是 `fn repr(v: Self) -> String`。另一方面，数值类型、`bigint`、`uuid`、`url` 模块有 `<mod>.to_string`，`bool` 没有。同职责两种拼写，某些类型只有其一。
- 提案：与 R-L0-22（`Ord` 的 `cmp` → `compare`）一样，将 `Repr` 方法改为 `to_string`。根据 R-L0-37，模块函数就是协议函数本身，因此所有值类型都有 `<mod>.to_string`，得到与 `"${x}"` 相同字符串。有 `Parse` 的类型满足 `parse(to_string(x)) == ok(x)`（Swift `LosslessStringConvertible`）。
- (a) Swift 让所有类型拥有 `CustomStringConvertible` 的 `description`，并在 `LosslessStringConvertible` 中与 `init?(_ description:)` 配对。Rust `Display`（`to_string`）和 `FromStr` 也成对。
- (c) 从 `int.to_string` 类推的 `bool.to_string(b)` 可通过，无须逐类型记住是否有 `to_string`。
- (d) 改名和新增。epoch 15 将用户编写的 `fn T.repr` 标为 `@deprecated(since=15, use="to_string")`，epoch 16 撤除，`almide fix` 改写。列表只列类型页有行的类型（有 `Parse` 的类型），其他类型记入“不列在列表中的共同约定”。
- issue: #4000

<a id="r-l0-39--を累乗の演算子から外し累乗は--だけにする---採用"></a>

## R-L0-39 撤去 `^` 的乘方含义，乘方只保留 `**` — ○ 采纳

- 当前：`^` 是乘方（`**` 别名）。实测（`almide 0.68.0 dev`）：`6 ^ 3` 返回 `216`。
- 问题：Rust、Go、Python、TS/JS、Swift 五种语言中，`^` 为异或（XOR）。LLM 欲写 XOR 而写 `a ^ b` 时，不产生类型错误，静默得到乘方值，违反 R-L0-32“不静默返回不同值”。9 种语言无一用 `^` 乘方（Python、JS 用 `**`，其余用函数）。
- 提案：从运算符表移除 `^`，使之为词法错误。提示“乘方用 `**`，按位异或用 `int.bxor`”。保留 `**`。
- (b) 语法变化，两种 target 相同。
- (d) 撤除语法。epoch 15 对 `^` 警告，epoch 16 报错（R-L0-14）。`almide fix` 将 `a ^ b` 改为 `a ** b`（保持当前含义）。向 `proofs/dialect-epochs.toml` 加一行。
- 空出的 `^` 不以其他含义复用（R-L0-43），永久保持提示“乘方用 `**`，按位异或用 `int.bxor`”的错误。
- issue: #4002

<a id="r-l0-40-数の変換は変換先の型に-from-で置く---採用"></a>

## R-L0-40 数值转换放在目标类型上，命名为 `from` — ○ 采纳

- 当前：转换在源模块按目标分别命名（`int.to_float(n)`、`int8.to_uint16_checked(x)`）。13 种数值类型组合有 347 函数，占标准库列表四分之一。
- 提案：目标类型模块提供能接收任意数值类型的泛型函数。
  - `fn <T>.from[S: Numeric](x: S) -> T`（无标记；结果类型无法表示的值则 abort）
  - `fn <T>.from_checked[S: Numeric](x: S) -> T?`
  - `fn <T>.from_saturating[S: Numeric](x: S) -> T`（仅整数类型）
  - `fn <T>.from_wrapping[S: Integer](x: S) -> T`（仅整数类型，仅整数输入）
  - 语义保持 R-L0-32、R-L0-34、R-L0-35 不变（仅拼写变化）。无损类型对中，无标记不 abort，`_checked` 始终返回值。
- (a) 多数放在目标侧：Rust `i8::from(x)`、`i8::try_from(x)`，Swift `Int8(x)`、`Int8(exactly:)`、`Int8(clamping:)`、`Int8(truncatingIfNeeded:)`，Python `float(n)`，Go `float64(n)`，JS `Number(x)`，OCaml `Float.of_int`。仅 Kotlin（`toDouble()`）、Gleam（`int.to_float`）放在源侧。Swift 四个初始化器与 Almide 四个函数一一对应。
- (c) 需记忆名称从 13 类型 × 12 目标 × 后缀矩阵缩为四个后缀。管道也一样写（`n |> float.from`）。
- (d) 改名与撤除。epoch 15 将 `to_<T>…` 标为 `@deprecated(since=15, use="<T>.from…")`，epoch 16 撤除。`almide fix` 将 `int.to_float(n)` 机械改为 `float.from(n)`。数值类型的 `to_string`、`parse` 不在本提案内。
- 语言所需：带类型参数的泛型函数可用协议边界（`S: Numeric`）接收，并调用无接收者要求（`T.from`）（#4000）。
- 实现状态（2026-10-24）：已实现。9 种整数、3 种浮点、`bigint` 的 `from`、`from_checked`、`from_saturating`、`from_wrapping` 共 44 函数可按列表形态使用（如 `fn int8.from[S: Numeric](x: S) -> Int8`）。函数体为一个泛型函数，通过类型参数调用源类型内部要求（`__trunc`、`__int_exact`、`__uint64_exact`、`__int_sat`、`__to_float` 等，转换的“轴”值）。不需要每对源目标类型的函数。语义遵循 R-L0-32/34/35：无标记向 0 截断，越界、NaN、无穷大 abort；`_checked` 无法精确表示（小数部分、NaN、越界）则 none；`_saturating` 饱和至端点，NaN 为 0；`_wrapping` 仅整数（`Integer`，浮点输入检查报错），将低位按补码解释。保留 `UInt64` ≥2^63 以及转 `Float32` 只舍入一次（不经过 `Float`）的处理。当前 `to_<T>` 列仍保留（从 epoch 15 的 `@deprecated` 开始撤除；R-L0-40(d)）。`spec/stdlib/numeric_from_test.almd` 在两个 target 验证。
- issue: #4003

<a id="r-l0-41-ビット演算を演算子にし-を関数の合成から外す---取り消し十二巡目記号の意味を入れ替えないため--r-l0-43"></a>

## R-L0-41 位运算改为运算符，并取消 `>>` 函数组合 — × 撤销（第十二轮：不替换符号含义 → R-L0-43）

- 当前：位运算是 `int.band(a, b)` 等函数，无运算符。`>>` 为函数组合（`f >> g`），`^` 原为乘方（R-L0-39 撤除）。
- 提案：为整数类型（`int`、8 种定宽类型、`bigint`）提供 `&`、`|`、`^`（XOR）、`~`（取反）、`<<`、`>>`。`>>` 对有符号类型保留符号，对无符号类型补 0（与 Swift、Rust、Go 相同）。忽略符号的右移转为无符号类型后写（`uint64.from_wrapping(x) >> n`）。
- 优先级跟随 Swift，避免 C 陷阱（`a & b == c` 变为 `a & (b == c)`）：`**` ＞ `<<` `>>` ＞ `*` `/` `%` `&` ＞ `+` `-` `|` `^` ＞ 范围 ＞ `|>` ＞ 比较。
- 取消函数组合 `f >> g`，改用 `(x) => g(f(x))` 或 `x |> f |> g`。9 种语言中只有少数有组合运算符（F#、Haskell 系），Rust、Go、Python、JS、Swift 五种中 `>>` 是右移。实测：Almide 代码中组合 `>>` 仅用于测试组合本身的 7 行 spec。
- (a) R-int-05（×）只比较函数名而判“无多数派”。连运算符一起比较时，`&`、`|`、`^`、`<<`、`>>` 为 Rust、Go、Python、JS、Swift 五种的拼写，构成多数派（R-L0-00）。
- (d) 新增语法并改变语义。`&`、`|`、`~`、`<<` 从 epoch 15 可用。`^`（乘方）、`>>`（组合）在 epoch 15 警告、16 报错、17 赋予新义（中间插入一个错误 epoch，避免静默替换同符号含义）。`almide fix` 将 `int.band(a, b)` 改为 `a & b`，`f >> g` 改为 `(x) => g(f(x))`。撤除 `band`、`bor`、`bxor`、`bnot`、`bshl`、`bshr`、`bshr_logical`。
- issue: #4004

<a id="r-l0-42-列のアルゴリズムは-protocol-の既定の実装として-1-回だけ書く---採用"></a>

## R-L0-42 序列算法只写一次，作为协议默认实现 — ○ 采纳

- 当前：`enumerate`、`zip`、`sort`、`min`、`take_while`、`chunk` 等 30 个序列算法只在 `list` 中。`deque`、`set`、`string`、`bytes`、`map` 只能先 `to_list` 再 `list.*`。`list.sum`、`product` 有 `Int` 与 `Float` 两种形态，`list.join` 有 `String` 与 `Bytes` 两种。
- 提案：在[协议层级](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md)的 `Sequence`、`Ordered` 中作为默认实现只放一次（与 Swift `Sequence`、`Collection` 扩展相同）。按 R-L0-37，所有遵循的序列模块出现同名、同形函数。
  - `Sequence`（可按固定顺序遍历的序列）：`enumerate`、`zip`、`zip_with`、`filter_map`、`find_map`、`reduce`、`scan`、`partition`、`group_by`、`sort`、`sort_by`、`sort_with`、`min`、`max`、`min_by`、`max_by`、`min_with`、`max_with`、`sum`、`product`。元素类型变化或排序后的结果返回 `List`（如 Swift `sorted()`、`map` 返回 `Array`）。
  - `Ordered`（可从两端计数的序列）：`find_index`、`take_while`、`drop_while`、`chunk`、`window`、`split_at`、`dedup`、`intersperse`、`unique`、`unique_by`。切片结果返回同一类型（Swift `SubSequence`）。
  - 条件：`sum`、`product` 仅用于元素为 `Numeric`；`sort`、`min`、`max` 仅用于元素为 `Ord`。`string` 无 `sum`、`product`；`map` 无 `sort`、`min`、`max`、`sum`、`product`。
  - 将 `list.sum`、`product` 统一为 `sum[A: Numeric](xs: List[A]) -> A`，`list.join` 统一为 `join[T: Text](xs: List[T], sep: T) -> T`。
- 从 `Ord` 默认实现移除 `min`、`max`、`clamp`，恢复为 `Integer`、`FloatingPoint` 要求（R-sized-08 形态）。放在 `Ord` 会与 `duration.min(n)`（n 分钟）、`string.min(s)`（最小字符）重名。
- (a) Swift（`Sequence`、`Collection` 扩展）、Rust（`Iterator` 提供的方法）、Kotlin（`Iterable` 扩展函数）均在一处定义序列算法，让所有序列可用。
- (d) 新增（`deque` 30、`set` 20、`string` 28、`bytes` 29、`map` 15），并扩展 `list.sum`、`product`、`join` 类型。实现只有一个默认实现，不逐类型编写。
- issue: #4012

<a id="r-l0-43-撤去した記号を別の意味で使い直さない---採用"></a>

## R-L0-43 不以新含义复用撤除的符号 — ○ 采纳

- 规则：
  1. 运算符、符号即使撤除，也不以其他含义复用。撤除符号永久保持说明“已撤除”的错误。
  2. 仅当旧写法必定类型错误或 abort 时，才可用其他含义复用函数名（如 `string.count(s, sub)` → 谓词 `count(s, f)`；传字符串的旧写法类型错误；R-string-18）。
- 理由：即使两个 epoch 间插入一次错误，旧代码、旧资料、LLM 记忆的写法仍存在。同一符号随读者和编写时代含义不同。将整数 `a ^ b` 从乘方改为 XOR，会通过类型检查却产生不同值，违反 R-L0-32“不静默返回不同值”。
- 依此撤销第九轮 R-L0-41（将位运算改为运算符并取消 `>>` 函数组合）。`^` 只取消乘方别名并报错（R-L0-39）；`>>` 保持函数组合；位运算保持函数（R-int-10）。
- (d) 增加判定规则。
- issue: #4004

<a id="r-l0-44-入出力の-protocol-readwriteclose-を置き読み書きと閉じるを-io-の総称関数-1-組にする---採用"></a>

## R-L0-44 引入 I/O 协议 `Read`、`Write`、`Close`，将读写关闭统一为一组 `io` 泛型函数 — ○ 采纳

- 当前：读、写、关闭随句柄种类使用不同名称。TCP 为 `net.tcp_read`、`tcp_write`、`tcp_close`、`tcp_close_listener`（句柄是裸 `Int`），Unix socket 为 `unix.read`、`unix.write`、`unix.close`（`unix.Fd`），UDP 为 `net.udp_close`（`UdpSocket`），标准 I/O 为 `io.read_bytes`、`io.write_bytes`（无句柄）。[协议层级](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md)中没有 I/O 层。
- 提案：
  ```almide
  protocol Read  { effect fn read(h: Self, max: Int) -> Bytes }      // 最大 max バイト。相手が閉じたら空
  protocol Write { effect fn write(h: Self, data: Bytes) -> Unit }   // 全部書く（Rust の write_all）
  protocol Close { effect fn close(h: Self) -> Unit }                // 2 回目も ok

  effect fn io.read[H: Read](h: H, max: Int) -> Bytes
  effect fn io.read_exact[H: Read](h: H, n: Int) -> Bytes            // 既定の実装。足りなければ err
  effect fn io.read_to_end[H: Read](h: H) -> Bytes                   // 既定の実装。閉じるまで全部
  effect fn io.write[H: Write](h: H, data: Bytes) -> Unit
  effect fn io.close[H: Close](h: H) -> Unit
  ```
  - 遵循类型：`net.TcpStream`（`Read`、`Write`、`Close`），`net.TcpListener`、`net.UdpSocket`（`Close`），`unix.Fd`（`Read`、`Write`、`Close`），`io.Stdin`（`Read`），`io.Stdout`、`io.Stderr`（`Write`）。
  - 泛型函数只在 `io` 放一次，不放在各类型模块（与 `http.header[M: HttpMessage]` 同形，R-http-12）。
- (a) 6 种语言跨句柄种类统一读写：Rust（`Read`、`Write` trait，`read_exact`、`read_to_end` 为默认实现），Go（`io.Reader`、`Writer`、`Closer`、`io.ReadFull`、`io.ReadAll`），Python（file-like object），Elixir（IO device），OCaml（channel），Kotlin（`InputStream`、`OutputStream`、`Closeable`）。Swift 标准库无 I/O 协议（Foundation 的 `FileHandle` 为具体类型）。层级骨架参照 Swift，但 Swift 没有的层按多数派加入（与 `Container` 相同）。
- 拼写：`read`、`write`、`close` 是 9 种语言的多数派。`read_exact`、`read_to_end` 遵循 Rust（Go 是 `ReadFull`、`ReadAll`，但 `io.read_all()` 已是按文本读取全部标准输入的函数，不能同名用于两种形态）。
- 名称复用（R-L0-43 第 2 项）：`io.write` 当前是向标准输出写 `Bytes` 的单参数函数，R-io-01 已决定撤除。新 `io.write(h, data)` 有两个参数，因此旧 `io.write(data)` 类型错误，不会静默变义。
- (d) 撤除与新增，详情为 L2-net R-net-07、L2-unix R-unix-04、L2-io R-io-05。
- issue: #4015

<a id="r-l0-45-メモリ安全でない標準の関数は-ffiread-と-ffiwrite-の-2-つだけにする---採用"></a>

## R-L0-45 内存不安全的标准函数仅限 `ffi.read` 和 `ffi.write` — ○ 采纳

- 当前：读写原始地址的函数放在无需 import 可见的 `bytes`（`bytes.from_raw_ptr`、`copy_to_ptr`）。尚未清点标准库中哪些函数触及 Almide 值以外的内存。内存安全证明（#4011）中，这是语言保证与外部世界的边界。
- 提案：
  1. 重设计后标准库中，读写 Almide 值之外内存的函数仅 `ffi.read`、`ffi.write` 两个（R-ffi-01/02）。`ffi.address`、`ffi.to_int` 仅将地址作为值构造，不访问内存。（2026-10-29 补充：ADR-0031 补遗引入的 `ffi.slice` 也仅在父范围内构造只读 `Region`，不访问内存。）
  2. 参数或返回值有 `RawPtr` 的公开函数不放在 `ffi` 外。
  3. `ffi.read`、`ffi.write` 是 effect fn，要求 `import ffi`。critical profile 拒绝它们、构建记录标明使用 `ffi`，由 #4008 决定。
  4. 其他所有标准函数按 R-L0-33 职责处理越界（R-L0-46），无破坏内存的手段。
- 实测（重设计后 1,289 函数）：`RawPtr` 仅出现在 `ffi` 的 4 个函数。
- (a) 9 种语言都将原始地址读写限制在名称明确的区域（Rust `unsafe`、Go `unsafe`、Python `ctypes`、Kotlin `cinterop`、Swift `UnsafeRawPointer`）。
- 门禁提案：检查公开函数类型中的 `RawPtr` 仅出现在 `ffi`。向 `ffi` 加函数时，必须在同一 PR 更新本规则第 1 项列表。
- issue: #4008、#4013

<a id="r-l0-46-位置を受け取る関数を全数で表にし役割の規則r-l0-33をゲートにする---採用"></a>

## R-L0-46 全量列出接收位置的函数，将职责规则（R-L0-33）设为门禁 — ○ 采纳

- 当前：R-L0-33 规定按职责处理越界，但未清点并应用于所有位置参数函数。内存安全空间项（#4005）的检查对象正是此列表。
- 总数（重设计后 1,289 函数中接收 `Int` 位置、端点、偏移的函数）：108 个。读一个 60，写、插、删一个 33，提取范围 5，其他 10（`list.range` 是值而非位置；`value.index` 只构造路径；`bytes.skip`、`is_eof` 移动、检查读取位置；`matrix.from_bytes_*` 返回 none；`matrix.slice_rows`、`fs.fold_lines_range` 为范围）。
- 60 个单项读取函数都能由类型返回 none（`get_*` 为 `T?`，`next_*` 为 `(T?, Int)`）。
- 偏离项（实测 `almide 0.68.0 dev 437b8f127`）：
  - `fs.fold_lines_range(path, -2, 2, …)` 将起点截到 0 后折叠。
  - `matrix.slice_rows(m, -1, 2)` 返回 0 行矩阵。
  - 两者均是提取范围职责，按 R-L0-29 对负位置 abort（正方向越界仍截到边界）。
  - `bytes.skip(b, 0, -5)` 返回负读取位置 `-5`，后续 `bytes.read_u8(b, -5)` 返回 `0`。R-bytes-13、R-bytes-21 已决定“`n < 0` 则 abort”，实测确认该判定正确。
- 门禁提案：生成接收位置的公开函数列表，拒绝职责列为空、单项读取却不能返回 none、范围或写入缺少负位置 abort fixture 的行。
- issue: #4005

<a id="r-l0-47-信頼しない入力を読む関数は時間と大きさに上限を持つ---採用"></a>

## R-L0-47 读取不可信输入的函数具备时间与大小上限 — ○ 采纳

- 当前：读取外部字节串或字符串的函数中，可能扩张超出输入大小、深层嵌套、耗时爆炸者没有上限（#4019、#4020）。
  - `zlib.decompress`、`inflate`、`gunzip` 无输出上限，压缩炸弹会持续展开，直到内存耗尽而 abort。
  - `json.parse` 无嵌套深度上限，深层嵌套导致栈溢出 abort（C-196）。服务器可被一个请求终止进程。
  - 正则通过回溯匹配，`(a+)+$` 等模式配合长输入会指数增长（ReDoS）。
- 提案（规则）：读取不可信输入的标准函数满足以下之一。
  1. 时间和输出大小都与输入大小成正比（`base64`、`hex` 的 decode、`url.parse`、`string.split` 等，保持现状即可）。
  2. 不成正比者具备默认上限，达到时返回 err，不 abort。函数说明以数字写明上限。
  3. 提供一个接收上限参数的函数以便修改上限（语言没有默认参数）。
- 应用：R-regex-12（线性时间匹配）、R-zlib-04（展开输出上限）、R-json-04（嵌套深度上限）。HTTP 服务端、客户端已有上限（`http_server_core.rs`、`http_route_core.rs`）。
- (a) Go（`regexp` 线性时间，`encoding/json` 深度 10000 停止）、Rust（`regex` 线性时间，`serde_json` 深度 128）、Python（`json` 递归上限，`zlib` 的 `max_length`）、Node（`zlib` 的 `maxOutputLength`）。2026 年 Go 的资源耗尽漏洞（`archive/tar`、`encoding/asn1`、`net/mail`）发生在本规则之外。
- (c) 直接传入外部输入是 LLM 最容易写出的形态。要求显式写上限会漏写，默认保护则不会漏写。
- 门禁提案：为 3 个应用函数设置恰好达到上限和超出 1 的 cross fixture，两个 target 连 err 文本也相同。
- issue: #4019、#4020

<a id="r-l0-48-codec-の入力-フィールドを読まない属性decode_strictフィールドの属性の語彙---採用"></a>

## R-L0-48 Codec 输入：跳过读取字段的属性、`decode_strict`、字段属性词汇 — ○ 采纳

- 当前：派生的 `T.decode` 从输入读取所有声明字段。将请求体 `{"name": "m", "is_admin": true}` 传给 `type User: Codec = { name: String, is_admin: Bool = false }` 会令 `is_admin` 为 true（mass assignment，#4108）。字段写 `@skip` 或 `@serde(skip)`，属性也会未经读取而丢弃，毫无作用。
- 提案（规则）：
  1. 记录字段可用 `@skip`（decode、encode 均不处理）和 `@skip_decode`（decode 不读，encode 写）。两者均要求默认值（`= 値`），没有则 E104。即使输入含不读取字段的键，也视为未知键。可与别名语法组合（`@skip_decode role as "role_name": String = "user"`）。
  2. `T.decode` 仍忽略未知键。需要拒绝时，为所有 Codec 记录类型派生 `fn T.decode_strict(v: Value) -> Result[T, String]`。读取字段前寻找 decode 会读的字段键之外的键，以文档顺序返回第一个，形态为 `err("unknown key '<k>'")`。不为 variant 派生。
  3. 记录字段允许的属性仅 `@builtin`、`@location`、`@skip`、`@skip_decode`。其他为 E053 错误（函数属性 E053 仍为警告）。对其他语言拼写提示 Almide 拼写（`@serde(skip)` → `@skip`、`@skip_deserializing` → `@skip_decode`、`@redact` → 类型 `Secret[String]`）。
- 函数族：{decode, encode} × {读, 不读} 四格中，`@skip`、`@skip_decode` 占两格，无标记字段占一格。有意不提供只从输出隐藏的一格（`@skip_encode`）；输出隐藏应由 `Secret[A]` 类型或单独输出类型承担。表仅在 `crates/almide-frontend/src/attr_vocab.rs` 的 `CODEC_FIELD_SKIPS` 声明一次，派生两半和门禁均读取。
- (a) 8 个中 6 个默认忽略未知键（serde、Go、pydantic、Rails、Ecto、Swift）。拒绝开关为 serde `deny_unknown_fields`、Go `DisallowUnknownFields`、pydantic `extra='forbid'`。排除输入字段为 serde `skip`、`skip_deserializing`，Jackson `READ_ONLY`，kotlinx `@Transient`，均要求默认值。
- (c) 拒绝未知键无法防止 mass assignment，因为 `is_admin` 是已知键。防范方式是“先 decode 到输入类型，再构造领域类型”（Rails strong parameters、Ecto changeset、FastAPI `UserIn`、OWASP），以及为需要双向 Codec 的类型加 `@skip_decode`。CHEATSHEET 教此写法。
- (d) `@skip`、`@skip_decode`、`decode_strict` 为新增。未知字段属性变为错误会拒绝原先接受的代码，因此 epoch 15 breaks 有一行。`decode_strict` 是派生方法而非标准模块函数，故不放 target-surface.json，写在“不列在列表中的共同约定”。
- 门禁：`spec/wasm_cross/codec_field_skip_matrix.almd`（{无标记, `@skip`, `@skip_decode`} × 输入 {无键, 有键, 未知键} × {`decode`, `decode_strict`} 及 `encode`，C-428、C-429）；`tests/codec_field_skip_test.rs`（从函数族表检查两个 leg，用忽略 `@skip_decode` 的对照确认 admin=false 变成 admin=true）；`tests/diagnostics/e104-*`、`e053-unknown-field-attribute-*`（C-430）。
- 拼写：`decode_strict` 遵循 R-L0-25 的“`decode_<変種>`（Result）”。拒绝未知键的开关无多数拼写（serde `deny_unknown_fields`、Go `DisallowUnknownFields`、pydantic `extra='forbid'`，kotlinx 默认拒绝并以 `ignoreUnknownKeys` 放宽）。`@skip` 是 serde、Go（`json:"-"`）、kotlinx（`@Transient`）、Jackson（`@JsonIgnore`）共有的“双向排除”语义的最短拼写；`@skip_decode` 将 serde `skip_deserializing` 改用 Almide 动词 `decode`（R-L0-25）。
- issue: #4108

<a id="ゲートの束r-l0-g---採用"></a>

## 门禁集合（R-L0-G）— ○ 采纳

将 R-L0-02、03、04、07、09、10、12、16 检查汇总为一个测试 `tests/stdlib_surface_conventions_test.rs`。输入为 interface JSON（包含 `mut` 字段）。每条规则的允许列表随 L2 提案消除例外而缩减，不允许扩张。

- issue: #3949

> 保留日文代码注释的读者说明：set 示例中插入不报错且无效果，打印长度为 0。I/O 提案中 Read 最多读取 max 字节，对端关闭则为空；Write 写完全部（Rust write_all）；Close 第二次也 ok。read_exact 是默认实现，不足则 err；read_to_end 是默认实现，读到关闭为止。这些是历史提案注释，不是新执行结果。
