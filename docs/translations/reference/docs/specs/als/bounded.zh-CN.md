<a id="als-b--the-bounded-profile-normative"></a>

# ALS §B — 有界配置（规范性）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bounded.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](bounded.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](bounded.zh-CN.md)

> 权威原文：[docs/specs/als/bounded.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。本页是非规范性的读者快照，不替代原 ALS 或检查器、台账、生成器输入，不自动同步。需求、历史状态、证据和安全主张均保留原文范围。本版未独立执行、测量、认证或证明。原文语言版为读者副本，其他语言版为翻译。

> 原文范围说明：下述保证是带日期的规范要求，包括明确记载的 0.58.0 实现差距及六项限制，不是对当前构建的独立认证。模块分类及旧 fan 语法按原文保留，未推断当前 API 或效果状态。Float 限制在所述两项修订条件下属于暂行规则。


> 最后更新：2026-10-26

> **状态**：规范性（ADR-0017，2026-08-21 裁定）。本章规定**有界配置**——语言的**子集**，而非方言。对于标有 `@bounded` 的函数，实现**以检查器判定的形式**保证：执行时间和存储**静态有界**，调用图**闭合**，效果受 **capability 限定**。判定可观察（接受／拒绝码）；每节的拒绝规则通过 `tests/diagnostics/e07x-bounded-*/` 中的 broken / fixed 对执行验证。本章先于实现成为规范（CONTRIBUTING 的“需求先行”）：截至 2026-08-21 的 0.58.0 发布版只对未知属性 `@bounded` 警告（E053），没有拒绝规则，因此本章的拒绝测试对该发布版为红。这是判定者描述发布版的状态；实现推进固定版本后会变绿。
>
> “bounded”在本语言中统一表示**有上限**：`fan.bounded(c) { … }`（ALS-DT2）通过**运行时预算**限定计算，`@bounded`（本章）通过**编译时证明**限定函数。机制不同，含义相同。
>
> **本章不能表达的内容（自述限制，避免误当作主张）**：(1) 有界深度递归（全面禁止，B6）；(2) 定点及确定性 Float 运算（Float 运算暂禁，B10；解除条件是 ALS-T19 家族成为规范，且认证席具有 Float 指令集，两者均须成立）；(3) 静态大小数组（所有堆容器的大小均为动态确定，B8 拒绝运行时长度的构造）；(4) 按字节计的内存上限（仅有对象数量上限）；(5) 低于上限（`≤ B`）的提前退出（B5/B11 为严格上限而禁止）；(6) 功能正确性的可追溯性——本配置保证**安全**（内存、名称、capability、上限），不保证控制律**正确**。

<a id="als-b1-bounded-属性と有界プロファイル"></a>

## ALS-B1 `@bounded` 属性与有界配置

`@bounded` 放在 `fn` / `effect fn` 声明上，声明该函数属于有界配置。属性**只能放在函数声明**上（不存在放在模块声明上的语法糖）。`@bounded` 函数只能调用 `@bounded` 函数及纯标准库模块的一阶函数（B7），因此从 `@bounded` 函数可达的调用图闭合。属性不影响类型和值：`@bounded` 函数可直接作为普通函数调用。

违反任一配置规则的 `@bounded` 函数会**在类型检查时拒绝**；诊断码为 **E070–E079**（为本章保留），消息形如 `<construct> is not admissible in a @bounded function`，并带有各节指定的 hint。违规完全不影响未标注 `@bounded` 的函数（子集只收窄属性内部）。

```almide
@bounded
fn scale(x: Int) -> Int = x * 3

test "a @bounded function is an ordinary function" {
  assert_eq(scale(4), 12)
}
```


测试：`spec/wasm_cross/bounded_kernel.almd`、`spec/stdlib/bounded_profile_test.almd`。
Contracts: C-308。

<a id="als-b2-サブセットであって方言ではない"></a>

## ALS-B2 是子集，不是方言

标有 `@bounded` 的程序即使移除所有属性，仍是**同一程序**：可观察行为（stdout、stderr、exit code）不因属性有无而改变（与 SPARK ⊂ Ada 的关系相同）。属性只改变“检查器额外拒绝什么”。`spec/wasm_cross/bounded_kernel.almd` 与 `bounded_kernel_plain.almd` 是仅属性有无不同的同一程序，打印相同输出。

```almide
@bounded
fn grid_sum() -> Int = {
  var acc = 0
  for i in 0..<4 { for j in 0..<3 { acc = acc + i * j } }
  acc
}

fn grid_sum_plain() -> Int = {
  var acc = 0
  for i in 0..<4 { for j in 0..<3 { acc = acc + i * j } }
  acc
}

test "the attribute changes nothing observable" {
  assert_eq(grid_sum(), grid_sum_plain())
  assert_eq(grid_sum(), 18)
}
```


测试：`spec/wasm_cross/bounded_kernel.almd`、`spec/wasm_cross/bounded_kernel_plain.almd`。
Contracts: C-309。

<a id="als-b3-回数付きループのみ"></a>

## ALS-B3 仅允许计数循环

`@bounded` 函数中的循环仅限**计数循环**：`for i in a..<b` / `for i in a...b`，且 `a`、`b` 为**编译时常量**（整数字面量或可从字面量常量折叠的绑定）。迭代次数由范围长度静态确定，允许嵌套（上限为各循环次数的乘积）。`while`、遍历容器的 `for x in xs`、以运行时值为端点的范围，均以 **E070** 拒绝（hint: `counted range`）。

```almide
@bounded
fn triangle() -> Int = {
  var acc = 0
  for i in 1...10 { acc = acc + i }
  acc
}

test "counted loops with literal bounds are admissible" {
  assert_eq(triangle(), 55)
}
```


```almide check-fail=E070
@bounded
fn count_down(n: Int) -> Int = {
  var k = n
  while k > 0 { k = k - 1 }
  k
}

fn main() -> Unit = println("${count_down(3)}")
```


测试：`tests/diagnostics/e070-bounded-unbounded-loop/`、
`tests/diagnostics/e070-bounded-for-over-container/`。Contracts: C-310。

<a id="als-b4-ループ内確保の禁止"></a>

## ALS-B4 禁止循环内分配

计数循环体中**分配新堆对象**的表达式——列表／字符串／Map／Set 的构造、拼接、`push`，以及创建闭包——以 **E071** 拒绝（hint: `allocate outside the loop`）。仅读取、借用或增加现有堆值引用计数的操作（索引读取、字段读取、传递同一值）允许。允许在循环外分配。

```almide
@bounded
fn dot(xs: List[Int], ys: List[Int]) -> Int = {
  var acc = 0
  for i in 0..<3 { acc = acc + (list.get(xs, i) ?? 0) * (list.get(ys, i) ?? 0) }
  acc
}

test "reading heap values inside a counted loop is admissible" {
  assert_eq(dot([1, 2, 3], [4, 5, 6]), 32)
}
```


```almide check-fail=E071
@bounded
fn squares() -> List[Int] = {
  var xs: List[Int] = []
  for i in 0..<8 { xs = xs + [i * i] }
  xs
}

fn main() -> Unit = println("${list.len(squares())}")
```


测试：`tests/diagnostics/e071-bounded-alloc-in-loop/`。Contracts: C-311。

<a id="als-b5-break--continue-の禁止"></a>

## ALS-B5 禁止 `break` / `continue`

`@bounded` 函数的循环体不得包含 `break` / `continue`——**E072**（hint: `single exit`）。迭代次数严格等于范围长度；提前退出会将其弱化为“不超过上限”，因此不允许（与 MISRA 的 single-exit 相同纪律）。

```almide check-fail=E072
@bounded
fn first_cube_over(limit: Int) -> Int = {
  var found = 0
  for i in 0..<100 {
    if i * i * i > limit then break
    found = i
  }
  found
}

fn main() -> Unit = println("${first_cube_over(100)}")
```


测试：`tests/diagnostics/e072-bounded-break/`。Contracts: C-312。

<a id="als-b6-再帰の禁止--呼び出しグラフの非循環"></a>

## ALS-B6 禁止递归——调用图无环

从 `@bounded` 函数可达的调用图必须是 **DAG**：自递归、相互递归（回边）以 **E073** 拒绝（hint: `call graph`）。无法解析的被调函数（其他文件中未解析的函数）会被**保守拒绝**，以免放过隐藏递归边。允许持有递归**类型**（`Boxed`），但不允许递归遍历。

```almide
@bounded
fn factorial_10() -> Int = {
  var acc = 1
  for i in 1...10 { acc = acc * i }
  acc
}

test "iteration replaces recursion" {
  assert_eq(factorial_10(), 3628800)
}
```


```almide check-fail=E073
@bounded
fn fact(n: Int) -> Int = if n <= 1 then 1 else n * fact(n - 1)

fn main() -> Unit = println("${fact(5)}")
```


测试：`tests/diagnostics/e073-bounded-recursion/`。Contracts: C-313。

<a id="als-b7-呼び出し閉包--呼べるもの"></a>

## ALS-B7 调用闭包——可调用的对象

`@bounded` 函数仅可调用：(a) `@bounded` 函数；(b) 纯标准库模块（`int` `float` `string` `list` `map` `set` `math` `option` `result` `value` `json` `bytes` `regex` `matrix` 等）的**一阶**函数。以下以 **E074** 拒绝（hint: `@bounded callee`）：调用非 `@bounded` 用户函数、创建和调用闭包／lambda、高阶函数（传入函数值的调用，如 `list.map(xs, f)`）、函数引用、通过 `any P` 的动态派发。（仅一阶的原因：间接被调对象会失去静态调用图、capability 和上限。）

变体构造器应用（`Circle(r)`）不是调用。它与记录字面量（`P { x: 1 }`）、元组一样，是构造值的表达式，不属于 B7，也不在 B4 列举的循环内分配中。各载荷表达式仍须按所有规则判定。

```almide
type Shape = | Circle(Int) | Dot

@bounded
fn classify(n: Int) -> Shape = if n > 0 then Circle(n) else Dot

@bounded
fn radius(s: Shape) -> Int = match s {
  Circle(r) => r,
  Dot => -1,
}

test "a @bounded function builds variant values" {
  assert_eq(radius(classify(2)), 2)
  assert_eq(radius(classify(0)), -1)
}
```


```almide
@bounded
fn double(x: Int) -> Int = x * 2

@bounded
fn quadruple(x: Int) -> Int = double(double(x))

test "@bounded calls @bounded and first-order stdlib" {
  assert_eq(quadruple(5), 20)
  assert_eq(int.max(quadruple(1), 3), 4)
}
```


```almide check-fail=E074
fn helper(x: Int) -> Int = x + 1

@bounded
fn uses_plain(x: Int) -> Int = helper(x)

fn main() -> Unit = println("${uses_plain(1)}")
```


测试：`tests/diagnostics/e074-bounded-unbounded-callee/`、
`tests/diagnostics/e074-bounded-higher-order/`、`spec/stdlib/bounded_profile_test.almd`
（构造器）。Contracts: C-314。

<a id="als-b8-実行時長のヒープ構築の禁止"></a>

## ALS-B8 禁止运行时长度的堆构造

长度或大小由**运行时值**决定的堆构造——`string.repeat(s, n)`、`list.with_capacity(n)`、`list.range(a, b)` 中 `n`/`a`/`b` 非常量的情况，以及生成运行时长度的字符串／列表——以 **E075** 拒绝（hint: `compile-time size`）。允许字面量（`[1, 2, 3]`、`"abc"`）及常量大小的构造。允许循环外的 `push`（B4 禁止的是循环内）。

```almide check-fail=E075
@bounded
fn pad(n: Int) -> String = string.repeat("x", n)

fn main() -> Unit = println(pad(3))
```


测试：`tests/diagnostics/e075-bounded-runtime-length-heap/`。Contracts: C-315。

<a id="als-b9-効果と-capability"></a>

## ALS-B9 效果与 capability

`@bounded effect fn` 只能在声明的 capability 范围内具有效果，可用效果仅限标准输出（`println` 家族）。以下调用以 **E076** 拒绝（hint: `declared capability`）：效果模块 `env` `fs` `http` `io`（print 除外）`net` `process` `random` `zlib`，不带 `effect` 却可触达宿主的模块 `datetime` `args` `mem` `testing`，以及 `fan.*`（非确定性调度）。

```almide check-fail=E076
import env

@bounded
effect fn which_os() -> String = env.os()

effect fn main() -> Unit = println(which_os() ?? "unknown")
```


测试：`tests/diagnostics/e076-bounded-effect-outside-capability/`。Contracts: C-316。

<a id="als-b10-浮動小数演算の禁止暫定"></a>

## ALS-B10 禁止浮点运算（暂行）

`@bounded` 函数内针对 Float 的运算符（算术、比较）及接受 Float 的标准库函数调用，以 **E077** 拒绝（hint: `Int arithmetic`）。单纯持有和传递 Float 类型的值允许。本规则是**暂行**的；当 ALS-T19 数值确定性家族成为规范，且认证席具有 Float 指令集时将修订——不会悄悄放宽。

```almide check-fail=E077
@bounded
fn scale(x: Float) -> Float = x * 2.0

fn main() -> Unit = println(float.to_string(scale(1.5)))
```


测试：`tests/diagnostics/e077-bounded-float-arithmetic/`。Contracts: C-317。

<a id="als-b11-早期脱出の制限"></a>

## ALS-B11 限制提前退出

计数循环**体内**的提前退出——通过 `!` 传播（effect fn）、`guard … else`——以 **E078** 拒绝（hint: `outside the loop`）。循环外允许传播和 guard（从整个函数提前 return 是有界的）。（`?` 是 Result → Option 转换，不是退出——ADR-0008。）理由与 B5 相同：从循环体中途退出会跳过每次迭代的堆回收，破坏严格次数上限。

```almide
@bounded
fn sum_or_zero(xs: List[Int]) -> Int = {
  var acc = 0
  for i in 0..<3 { acc = acc + (list.get(xs, i) ?? 0) }
  acc
}

test "defaulting inside the loop is admissible" {
  assert_eq(sum_or_zero([4, 5]), 9)
}
```


```almide check-fail=E078
@bounded
effect fn sum_first(xs: List[Int]) -> Int! = {
  var acc = 0
  for i in 0..<3 { acc = acc + list.get(xs, i)! }
  ok(acc)
}

effect fn main() -> Unit = println("${sum_first([1, 2, 3]) ?? -1}")
```


测试：`tests/diagnostics/e078-bounded-early-exit-in-loop/`。Contracts: C-318。

<a id="als-b12-md5sha-1-は-stdlib-にない"></a>

## ALS-B12 stdlib 中没有 MD5、SHA-1

`hash.md5`、`hash.md5_hex`、`hash.sha1`、`hash.sha1_hex` 不是公开的 stdlib 函数。这些摘要不适合用于安全，因此在 dialect epoch 15 移除，没有弃用期，也没有迁移目标包。在任何配置下，调用均为检查时错误 E002；提示来自 `proofs/dialect-epochs.toml` 的移除记录（epoch 15 移除，改写为 `hash.sha256(b)` 或 `hex.encode(hash.sha256(b))`；这是不同的摘要）。被移除的集合从此台账导出，而非从名称列表导出。台账记录为已移除的写法不会返回公开接口。

测试：`tests/diagnostics/e002-epoch15-md5-removed/`、
`tests/removed_spellings_stay_removed_test.rs`。Contracts: C-424。
