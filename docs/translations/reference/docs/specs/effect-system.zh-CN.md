> 最后更新：2026-10-25

<a id="effect-system"></a>

# 效应系统

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](effect-system.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](effect-system.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](effect-system.zh-CN.md)

> 日英混合原文的非规范性固定读者译本。以固定原文为准，不自动同步，也不表示已独立执行验证或安全审计。保留历史形式与计划中的 fan 形式之区分。原文差异保持不变：回调示例仍将文件系统操作标为 IO，而叶节点表使用 FS.read；§1/§2.4 与 §8 对路径检查范围的描述不同；原生依赖列举的细节也与 package-system §6.4 不同。文件系统约束和跨目标一致性是原文主张，不是新验证的保证。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/effect-system.md)


Almide 的效应系统在纯计算与有副作用的操作之间强制设置严格边界。编译器在函数层面跟踪效应上下文，并在编译时拒绝违规行为。

<a id="1-fn-vs-effect-fn"></a>

## 1. `fn` 与 `effect fn`

Almide 的每个函数要么是**纯函数**（`fn`），要么是**有效应函数**（`effect fn`）。`effect` 修饰符属于函数签名的一部分，并沿调用图传播。

```almide
import fs

fn add(a: Int, b: Int) -> Int = a + b

effect fn read_config(path: String) -> Result[String, String] =
  fs.read_text(path)
```

纯 `fn` 保证不进行 I/O、并发操作或环境访问，但有一个例外：它可以通过 §2.1 的六个内置函数写入 stdout/stderr 或中止。`effect fn` 可以进行上述任何操作。

检查器进入 `effect fn` 本体时设置 `can_call_effect = true`，进入普通 `fn` 时设置 `can_call_effect = false`。此标志控制所有效应相关操作。

测试：`spec/lang/effect_fn_test.almd`

<a id="2-effect-isolation-e006"></a>

## 2. 效应隔离（E006）

纯函数不能调用效应函数。编译器在每个调用点将被调用者的 `is_effect` 标志与调用者的 `can_call_effect` 上下文比较，以强制执行此规则。

```almide check-fail=E006
import http

effect fn fetch() -> Result[String, String] = http.get("https://example.com")

// Compile error E006: cannot call effect function 'fetch' from a pure function
fn bad() -> String = fetch()!
```

诊断包含一个指向效应函数声明位置的次级 span。

该规则统一适用于用户定义函数、stdlib 效应函数（如 `fs.read_text`、`http.get`）和跨模块效应函数调用。

测试：`spec/integration/modules/vis_effect_test.almd`

<a id="21-output-and-abort-builtins-are-admissible-in-a-pure-fn"></a>

### 2.1 纯 fn 可以调用输出与中止内置函数

*于 2026-09-27 添加（[ADR-0022](../../../../adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)）。这里只记录检查器一直具有的行为，不改变任何行为。*

纯 `fn` 可调用六个内置函数：**`println`、`eprintln`、`panic`、`assert`、`assert_eq`、`assert_ne`**。该集合是封闭的。其他所有输出路径均为 stdlib effect fn，从纯 fn 调用仍是 E006，包括 `io.print`、`io.eprint` 和 `io.write`（作用于 `io.stdout()` 或其他任何句柄）。

1. **它们写入或中止，绝不读取。** `println` 追加到 stdout，`eprintln` 追加到 stderr。`panic` 和失败的 assert 结束进程（stderr 块，退出码 1；assert 系列的形式为 C-153）。它们都不返回程序外部的信息，因此纯 fn 的结果仍是其参数的函数。
2. **它们不会使调用者带有效应，也不进行传播。** 返回值是 `Unit`（`println`、`eprintln`、assert 系列）或 `Never`（`panic`），绝不是 `Result`。`println(s)!` 在任何上下文中都是 E034。打印的纯 fn 在模块接口中保持纯签名，effect fn 也把它作为普通值调用，不使用 `!`。
3. **服务处理器（ADR-0020 §5.5 / §7.2）。** 处理器调用的纯辅助函数，其输出遵循处理器规则：每次调用的字节连续到达其流（一行是原子的），同一请求的行保持程序顺序，不同在途请求的行顺序未指定。
4. **`fan` 本体。** 从 `fan` 分支调用的纯 fn 作为该分支的一部分打印。此豁免不增加超出 §5 确定性契约的顺序承诺：C-004 固定 `fan.map` 的列表/分支顺序（`??` 链按定义顺序执行），而 `fan.settle` 各形式按 §5 所述在两个目标上都顺序展开。`fan` 本身仍要求效应上下文（E007）。
5. **参考求值器。** 进程内解释器和 judge 的参考求值器把这六个名称视为不带效应上下文的 prelude 内置函数。无论在哪里调用都以相同方式求值，因此三路 oracle 比较纯 fn 输出的方式与比较 effect fn 输出相同。只有检查器强制区分纯函数与效应函数。
6. **缓冲（C-162，以及 C-367 的流相关表述）。** 纯 fn 输出使用与 effect fn 相同的流。原生 stdout 是一个缓冲区，在每个行尾刷新（若 stdout 是终端，则每次写入也刷新）；stderr 不缓冲。跨目标承诺是**逐流**的：原生与 wasm 的 stdout 字节、stderr 字节和退出码逐字节相同。不承诺两个流重定向到同一文件时如何交错。中止会刷新此前写入的 stdout。
7. **`scoped` 更严格。** `scoped fn` 中的 `println` 仍为 E087。`@bounded` fn 接受输出内置函数（C-316）。

```almide check
fn show(label: String, n: Int) -> Unit = println("${label}=${n}")

fn half(n: Int) -> Int = {
  assert_eq(n % 2, 0)
  eprintln("halving ${n}")
  n / 2
}

fn main() -> Unit = show("half", half(8))
```

从同一位置读取仍是 E006：

```almide check-fail=E006
import fs

fn load(p: String) -> String = fs.read_text(p) ?? ""
```

`main` 遵循相同规则：若 `main` 不进行真实效应，普通的 `fn main() -> Unit` 就是正确写法。仅在读取、写文件、使用网络、使用 `fan` 或通过 `!` 传播时，才写 `effect fn main()`。

测试：`tests/checker_test.rs`（`pure_fn_admits_output_and_abort_builtins`、`pure_fn_still_rejects_a_real_read_with_e006`）、`spec/lang/pure_fn_output_builtins_test.almd`、`spec/wasm_cross/pure_fn_output_and_assert_abort.almd`（C-153）。

<a id="22-stdlib-readers-are-effect-fns"></a>

### 2.2 stdlib 读取器是 effect fn

*Dialect epoch 8（#3248）。* `io.read_byte`、`io.read_n_bytes` 和 `process` 的 argv 读取器读取 stdin 或 argv，因此像 `io.read_line` 和 `env.args` 一样成为 `effect fn`（argv 现在只存在于 `env.args`）。它们过去是普通 `fn`，纯 fn 可以通过它们读取外界；现在从纯 fn 调用是 E006。字节读取器后来被作用于 `Read` 句柄（`io.stdin()`、`net.TcpStream`、`unix.Fd`）的 `io.read`、`io.read_exact`、`io.read_to_end` 替代，它们同样是 `effect fn`。

*Dialect epoch 13。* `args` 模块的读取器（`args.has_flag`、`args.option`、`args.positional`）同样读取 argv，因此也是 `effect fn`（#848 的裁定：argv 属于效应）。将调用者标为 `effect fn`，或在 `effect fn main` 中读取参数后传入值。与上面通过模板分派的读取器不同，它们具有 Almide 本体，因此调用像用户 effect fn 一样具有 `Result[T, String]` 类型，并用 `!` 传播（`args.has_flag("verbose")!`）。

```almide check-fail=E006
import io

fn first_byte(h: io.Stdin) -> Int = match io.read(h, 1) {
  ok(b) => bytes.get(b, 0) ?? -1,
  err(_) => -1,
}
```

```almide check-fail=E006
import args

fn verbose() -> Bool = args.has_flag("verbose")
```

测试：`tests/diagnostics/e006-io-read-in-pure-fn/`、`tests/diagnostics/e006-io-read-exact-in-pure-fn/`、`tests/diagnostics/e006-process-args-in-pure-fn/`、`tests/diagnostics/e006-args-*-in-pure-fn/`（每个 `args` 读取器一个）。

<a id="23-pure-the-empty-effect-set-e092"></a>

### 2.3 `@pure`：空效应集合（E092）

*随 #3250 添加。* fn 上的 `@pure` 会被检查：该 fn 及其调用的所有内容没有效应类别（§8）、输出或声明的中止。当 fn 是 `effect fn` 或 `@extern`，或者它可达的任何内容调用了 §2.1 的六个内置函数、带类别的 stdlib 函数或 `@extern` fn 时，产生 E092。错误位于通向该操作的调用点，并给出路径。

经由 fn 类型参数的调用在实参书写处判定，因此 `@pure fn apply(f: (Int) -> Int, x: Int) -> Int = f(x)` 是纯的。语言定义的 trap——除零、索引越界——不属于声明的中止，不计入其中。

```almide check
@pure
fn area(w: Int, h: Int) -> Int = w * h

fn main() -> Unit = println("${area(2, 3)}")
```

```almide check-fail=E092
@pure
fn area(w: Int, h: Int) -> Int = {
  println("w=${w}")
  w * h
}
```

ADR-0027（草案）提议采用具有此含义的 `pure fn` 修饰符，并弃用属性以支持该修饰符；`@pure` 表示同一集合，因此两者从不冲突。

测试：`tests/diagnostics/e092-pure-calls-println/`、`tests/diagnostics/e092-pure-reaches-assert-via-helper/`、`tests/diagnostics/e092-pure-calls-categorised-stdlib/`、`tests/diagnostics/e092-pure-on-effect-fn/`。

<a id="24-effect-fns-reached-as-values-e006-e109"></a>

### 2.4 作为值到达的 effect fn（E006、E109）

*随 #4151 添加。* §1 的保证也适用于作为值到达的 effect fn：回调、记录字段、列表元素、其他函数构建的闭包。两条规则承担这一保证（ADR-0009 D2、ADR-0026 D1）：

1. **将 effect fn 的名称作为实参就是效应调用。** 无修饰的 fn 类型参数是透明的：`apply(fs.read_text, p)` 执行 `apply` 所执行的内容，因此此调用是效应调用，从纯 fn 调用则为 E006——无论是简单名称（`list.map(xs, eff)`）还是模块路径（`list.map(xs,
   fs.read_text)`）都一样。显式的 `effect (A) -> B` 槽位豁免：它要求 effect fn，而只有效应上下文才能调用它。
2. **效应位随值携带。** 函数值若为 `effect fn`，或为调用它的闭包，就携带此位，并在任何存储位置保留。纯 fn 的本体运行携带此位的值时产生 E109，并显示从纯 fn 到 effect fn 的路径；但经由自身 fn 类型参数的情况除外，规则 1 在各调用点计入这种情况。

```almide check-fail=E109
import fs

type Reader = { read: (String) -> Result[String, String] }

// E109: `main` stored fs.read_text in the field; this pure fn runs it.
fn pure_looking(r: Reader) -> Result[String, String] = (r.read)("s.txt")

effect fn main() -> Unit = {
  let r = Reader { read: fs.read_text }
  println(pure_looking(r) ?? "none")
}
```

```almide check-fail=E006
import fs

fn apply(f: (String) -> Result[String, String], p: String) -> Result[String, String] = f(p)

// E006: `apply` is transparent, so this call reads the file.
fn sneaky(p: String) -> Result[String, String] = apply(fs.read_text, p)
```

从 `effect fn` 中两种程序都被接受：`effect fn reads(r: Reader)` 可以运行该字段，effect fn 中的 `apply(fs.read_text, p)` 也是普通的效应调用。`process.exit` 返回 `Never`，但仍是效应函数：纯调用者会以 E006 被拒绝。它不属于 ADR-0022 豁免的六个 prelude 内置函数，能力台账会计入其中止。

分析在类型检查后针对整个程序进行，覆盖每条路径（`check`、`build`、`run`、`test`，两个目标），并采取保守方式：离开跟踪位置的值与其签名相同的所有其他值一起汇入池中（§8，*回调*）。因此，纯 fn 调用从列表读取的函数时，只要程序中任何位置的列表曾放入同签名且运行 effect fn 的函数，就会被拒绝。`almide check` 判定入口文件自身的函数；模块函数由会 lower 所有模块的构建路径判定。

测试：`tests/diagnostics/e109-*`、`tests/diagnostics/e006-effect-fn-path-to-user-hof/`、`tests/diagnostics/e006-effect-fs-remove-as-value/`、`tests/effect_value_laundering_test.rs`。

<a id="3-return-type-wrapping"></a>

## 3. 返回类型包装

经检查的源接口已经将效应调用视为可能失败（不会出错的 intrinsic 声明除外）。源码中的 `!` 是显式的，共享前端 lowering 将该显式操作记录到带类型的 IR。在原生表示中，`effect fn` 返回类型若尚非 `Result`，会在代码生成期间提升为 `Result[T, String]`。`ResultPropagationPass` 执行此转换：

1. 若 effect fn 声明 `-> T` 且 T 不是 Result，代码生成将其改写为 `-> Result[T, String]`
2. 本体尾表达式包装在 `ok(...)` 中
3. 已为 Result 的返回类型（如 `-> Result[Int, String]`）保持不变

```almide
// Source: returns String
effect fn greet(name: String) -> String = "hello ${name}"

// After ResultPropagationPass (Rust codegen): returns Result<String, String>
// Body becomes: Ok("hello ${name}".to_string())
```

类型检查器通过 `constrain_effect_body` 处理这种灵活性，它接受：

- `Unit` 本体（经 guard 进行控制流返回）
- 未包装的 `T`（自动包装成 `ok(T)`）
- 完整的 `Result[T, E]`（原样传递）

测试：`spec/lang/effect_fn_test.almd` — `safe_div`、`require_positive`

<a id="4-explicit-propagation-the--operator"></a>

## 4. 显式传播：`!` 运算符

Almide 使用 `!` 运算符显式解包 Result/Option，并传播错误。在 `effect fn` 内，`expr!` 解包值并将错误传播给调用者。

```almide
effect fn add_strings(a: String, b: String) -> Result[Int, String] = {
  let x = int.parse(a)!   // unwrap or propagate err
  let y = int.parse(b)!
  ok(x + y)
}
```

<a id="how-it-works"></a>

### 工作方式

**类型检查器（`infer.rs`）：** `Unwrap` 表达式提取内部类型：`Result[T, E]` 变为 `T`，`Option[T]` 变为 `T`。未标注类型的 `let` 绑定的隐式收窄已在 ADR-0008（v0.55.0）中移除：可能失败的调用在所有位置都产生 Result 值，以前隐式的地方现在是硬错误 E041/E042，而 `let _ = f()` 是允许的丢弃方式（C-217）。见 docs/specs/result-option-effect.md §3。

**Lowering/代码生成：** 前端将显式 `!` 记录为 IR `Unwrap` 节点（消去在普通值上允许的 no-op）。目标 lowering/输出实现其依赖通道的控制流。原生 `pass_result_propagation.rs` 将 Result 签名和调用形式适配到 Rust；结构化 Wasm 路径通过自身发射器使用共享带类型 IR。两者都不恢复源码级自动传播。匹配 Result 值会检查 `ok`/`err`，而不隐式解包。

```almide
fn require_positive(n: Int) -> Result[Int, String] =
  if n > 0 then ok(n) else err("must be positive")

effect fn classify(s: String) -> Result[String, String] = {
  let n = int.parse(s)!           // ! propagates parse error
  let valid = require_positive(n)! // ! propagates validation error
  ok(if valid > 100 then "big" else "small")
}

test "each ! is a propagation point" {
  assert_eq(classify("500"), ok("big"))
  assert_eq(classify("-1"), err("must be positive"))
  assert_eq(classify("x"), err("invalid digit found in string"))
}
```

测试：`spec/lang/effect_fn_test.almd` — `parse_num`、`add_strings`、`double_parsed`、`classify_str`

<a id="5-fan-blocks"></a>

## 5. `fan` 块

`fan` 块并发执行表达式。编译时强制执行两项限制：

<a id="e007-fan-requires-effect-context"></a>

### E007：`fan` 要求效应上下文

`fan` 块只能出现在 `effect fn` 或 `test` 块内。在纯 `fn` 中使用 `fan` 会产生 E007。

```almide check-fail=E007
effect fn compute_a() -> Result[Int, String] = ok(1)
effect fn compute_b() -> Result[Int, String] = ok(2)

// Compile error E007: fan block can only be used inside an effect fn
fn bad() -> (Int, Int) = fan { compute_a(), compute_b() }
```

经由 `static_dispatch.rs` 的 `fan.map()` / `fan.settle()` / `fan.min()` 调用也应用同样检查。

<a id="e008-no-mutable-variable-capture"></a>

### E008：禁止捕获可变变量

`fan` 块不能捕获外层作用域的 `var` 绑定。这可防止数据竞争。只允许捕获 `let` 绑定。

```almide check-fail=E008
effect fn use(n: Int) -> Result[Int, String] = ok(n)

effect fn bad() -> Result[Unit, String] = {
  var count = 0
  // Compile error E008: cannot capture mutable variable 'count' inside fan block
  let (a, b) = fan { use(count), use(count) }
  ok(())
}
```

<a id="type-behavior"></a>

### 类型行为

`fan` ALL 块 / `fan.settle` 产生元组（没有 Result 包装，也就没有可解包内容）。产生 Result 的形式（mapper、`fan.min`）遵循 ADR-0008：Result 是值，绑定须显式写出传播（`let v = fan.map(xs, f)!`）。测试：`spec/wasm_cross/fan_any_early_winner.almd` 和在 0.55 切换时迁移的 fan fixture。

```almide
effect fn add(a: Int, b: Int) -> Result[Int, String] = ok(a + b)
effect fn mul(a: Int, b: Int) -> Result[Int, String] = ok(a * b)

effect fn example() -> Result[Unit, String] = {
  let (sum, product) = fan {
    add(3, 4)    // Result[Int, String] -> Int
    mul(3, 4)    // Result[Int, String] -> Int
  }
  // sum: Int, product: Int
  assert_eq(sum, 7)
  assert_eq(product, 12)
  ok(())
}

test "the tuple carries the unwrapped values" {
  assert_eq(example(), ok(()))
}
```

<a id="determinism-contract-c-004"></a>

### 确定性契约（C-004）

`fan` 的并发 API 以**确定性内核**为契约（native ⇄ wasm 逐字节一致，契约台账 C-004，fixture：`spec/wasm_cross/fan_deterministic.almd`、`spec/wasm_cross/fan_pure_thunks.almd`）：

- 接收 thunk 列表的调用形式 `fan.race(thunks)` / `fan.any(thunks)` / `fan.settle(thunks)` 已移除（E027 墓碑诊断）。旧 `fan.race(thunks)` 在确定性模型下就是 `thunks[0]()`，只有名称承诺了语言并不具有的时序语义。
- `fan.min(compute.X(n)) { }` / `fan.min(compute.X(n), xs, f)` —— 当前形式。时钟参数必填（类型说明以什么度量最小值）。各分支为纯计算（E006），按源码顺序逐个执行，选择预算内确定性计算量最小的 Ok（相同则选源码顺序靠前的分支）。这是与 Go 的 select 规范要求 RANDOM 相对照的设计选择。`fan.min(duration.X(n)) { }` —— 按真实时间最先成功的分支（oracle 层，允许 I/O，未获胜的分支在下一分界点停止，并等待它们结束）已决定采用（ADR-0028），将通过 #3788 引入。
- 按顺序尝试并取首个 Ok 不是 fan：使用 `a() ?? b() ?? 既定値`（`??` 的右侧仅在左侧失败时求值），列表则用 `list.find_map(xs, (x) => f(x)?) ?? 既定値`。旧 `fan.any { }` / `fan.any(xs, f)` 已在 dialect epoch 14 移除（E027 给出替代形式）。
- `fan.settle { }` / `fan.settle(xs, f)` —— 所有元素的 Result。两种形式在所有目标上都顺序展开（块为元组，mapper 为 `list.map`）。
- `fan.timeout(duration.X(n)) { }` —— 向纯本体施加墙钟期限的 oracle 层形式（允许本体执行 I/O 的方案已决定，#3788）。旧调用形式 `fan.timeout(ms, f)` 为 E027（C-006）。

测试：`tests/fan_acceptance_test.rs`（固定四个 leg 的 stdout 一致）。所有形式的现状与已知差异见 `docs/design/fan-semantics-audit.md`。

<a id="historical-thunk-adapters-not-a-list-of-current-public-forms"></a>

### 历史 thunk 适配器（并非当前公共形式列表）

在旧 thunk API 实现中，`race` / `any` / `settle` 的 thunk 可以是纯函数（`fn() -> T`）或 effect（`fn() -> Result[T, String]`）——非 Result thunk 由 FanLowering 在两个目标共用的处理中包装成 `Ok` 适配器（v0.27.3+、#514）。**只有 `fan.map` 要求 mapper 显式返回 Result**（`(x) => ok(x * 10)`）；因为 mapper 返回类型推断不同于 thunk，较为困难，纯 mapper 在 check 时被拒绝（有意的不对称，诊断改进见 #547）。

测试：`spec/lang/fan_test.almd`、`spec/lang/fan_map_test.almd`、`spec/wasm_cross/fan_pure_thunks.almd`、`spec/wasm_cross/fan_block_err_list_order.almd`（C-199）、`spec/wasm_cross/fan_sibling_trap.almd`（C-200）

<a id="6-test-blocks"></a>

## 6. `test` 块

测试块隐式处于效应上下文中。检查器进入测试本体时设置 `can_call_effect = true`，lowerer 输出带 `is_effect: true` 的测试函数。

这意味着测试块可以：

- 直接调用效应函数
- 使用 `fan` 块
- 使用 `!` 运算符解包

```almide
import fs

effect fn load_config() -> Result[String, String] = ok("port=8080")

test "calls effect functions directly" {
  let content = load_config()!
  assert(string.len(content) > 0)
  assert_eq(fs.exists("/nonexistent/almide-doctest"), false)
}
```

测试函数不返回 `Result`，而返回 `Unit`。测试中调用的 effect fn 产生显式 Result 值：用 `!` 解包（err 时使测试 panic），或用 `??` / ok/err 匹配消费。见 docs/specs/result-option-effect.md §5。

测试：`spec/lang/effect_fn_test.almd` — 所有测试块都直接调用效应函数

<a id="7-cross-module-effects"></a>

## 7. 跨模块效应

效应函数签名跨模块边界保留。当模块 A 导出 `effect fn` 时，任何导入 A 的模块都可以调用它，但仅限效应上下文。

```almide project
// file: effectlib/mod.almd
effect fn read_config() -> Result[String, String] = ok("config_value")
fn pure_() -> String = "pure"
// file: main.almd
import effectlib

effect fn main() -> Unit = {
  let config = effectlib.read_config()!  // ok: effect context — the Result is a value, `!` propagates
  assert_eq(config, "config_value")
  assert_eq(effectlib.pure_(), "pure")   // pure fn also callable
}

test "the imported effect fn keeps its signature" {
  assert_eq(effectlib.read_config(), ok("config_value"))
  assert_eq(effectlib.pure_(), "pure")
}
```

`is_effect` 标志属于存储在 `TypeEnv.functions` 中的函数签名，并经模块接口传播。调用点的 E006 检查对本地函数和导入函数完全相同。

测试：`spec/integration/modules/vis_effect_test.almd`

<a id="8-permissions"></a>

## 8. 权限

`almide.toml` 的 `[permissions]` 部分限制包可以使用的效应类别。这是 Almide 能力系统的安全第 2 层。

<a id="configuration"></a>

### 配置

```toml
[permissions]
allow = ["FS.read", "IO", "Net.fetch"]
```

如果没有 `[permissions]`，或其中没有 `allow` 键，则不检查能力（向后兼容）。显式的 **`allow = []` 不允许任何能力**（#4101）：所有到达能力的函数都是能力违规，因此 `allow = []` 是清单表示“此程序不触及任何内容”的方式。过去它与缺少键的含义相同，会授予一切。输出也计入：`println` 是 `IO.stdout`（下表），所以在 `allow = []` 下，打印的程序会被拒绝，尽管普通 `fn` 可以调用 `println`（ADR-0022）。授予 `IO.stdout` 即可只允许打印，不允许其他能力。

`almide.toml` 按 TOML 读取，因此 `allow` 可以使用任意数组写法：单行、多行、尾随逗号、项之间的注释（#3253）。过去多行数组会被读为空，从而允许一切。

<a id="effect-categories"></a>

### 效应类别

类别是 ADR-0026 D2 的封闭两层层级（#4155）。推断与 `[permissions]` 都以**叶节点**工作；清单可以授予**父节点**，表示此处列在其下的叶节点——以后添加的叶节点不会自动加入父节点。

| 父节点 | 叶节点 | 携带它们的操作 |
|--------|--------|-------------------|
| `FS` | `FS.read`, `FS.write` | `fs`：读取和列举为 `FS.read`；写入、移除、重命名、创建为 `FS.write`（`fs.copy` 两者都有） |
| `Net` | `Net.fetch`, `Net.listen` | `http` 的 `effect fn` 为 `Net.fetch`，`http.serve` 为 `Net.listen`；`net.tcp_connect` / `unix_connect` 为 `Net.fetch`，listen/accept 函数为 `Net.listen`；对已打开的流或共享内存区域的操作同时具有两者（无法判断是哪一端打开的） |
| `IO` | `IO.stdin`, `IO.stdout`, `IO.stderr` | `io` 读取器为 `IO.stdin`；`io.print` / `write` / `write_bytes` 和 `println` 为 `IO.stdout`；`eprintln` 为 `IO.stderr` |
| `Env` | `Env.read`, `Env.write` | `env` 和 `args` 读取（`Env.read`）；`env.set` 写入（`Env.write`）；`process.pid` 读取 |
| — | `Proc` | 启动进程与发送信号：`process.exec` 等、`kill`。没有父节点授予它，因为启动的命令可到达其他所有能力 |
| — | `Time` | 时钟：`datetime.now` / `now_ms` / `monotonic_ns`、`env.sleep_ms` |
| — | `Rand` | `random` |
| — | `Fan` | `fan` |
| — | `Abort` | `panic`、失败的 `assert*`、`process.exit`（`panic` 和失败的 assert 也写入 `IO.stderr`） |

模块在 `STDLIB_MODULE_EFFECTS`（`crates/almide-ir/src/effect.rs`）中的行是其默认值；操作不同的函数在声明上写出自身叶节点，例如 `@category("FS.write")`。表旁的 gate 拒绝没有行的 stdlib 模块、运行时符号分类不同于声明它的函数的 `@intrinsic`（#3246）、未指定有效叶节点的 `@category`，以及已分类模块中没有类别的 `effect fn`。

`path`、`url`、`json`、`regex`、`zlib`、集合和数值操作不携带类别。**调用 `@extern` fn 等于所有叶节点**（⊤、#3245）：其本体是外部代码，因此只有 `allow` 授予全部十四个叶节点时才通过 `[permissions]`。**stdlib 之外的 `@inline_rust` fn 也等于所有叶节点**（#4100）：原生构建将 Rust 模板插入以替代 Almide 本体，因此推断读取的本体不是实际执行的代码。stdlib 自身模板按上述调用表分类。

**从 dialect epoch 15 起**，`IO` 不再涵盖文件（属于 `FS`），`Env` 不再允许启动进程（属于 `Proc`），输出与中止内置函数携带 `IO.stdout` / `IO.stderr` / `Abort`。如果被拒绝的叶节点原本会由旧名称覆盖，会输出 `note:` 指明新名称。

`[permissions].allow` 授予 `Proc` 却没有 `proc = [...]` 命令列表（ADR-0025）时，每条路径都会警告——程序可以启动任意命令——在 `almide check --profile critical` 下则是错误。

测试：`tests/effect_permissions_test.rs`、`tests/proc_category_test.rs`；表的 gate 是 `crates/almide-ir/src/effect.rs` 中的 `classification_gate` 模块。

<a id="unknown-names"></a>

### 未知名称

`allow` 只接受四个父节点和十四个叶节点。其他任何名称，包括大小写不同的名称（`io`），都会在任何命令运行前，在清单书写该名称的行报错（#3247）：

```
error: almide.toml:6: unknown capability `Fil` in [permissions].allow — grantable capabilities are FS, Net, IO, Env, FS.read, FS.write, Net.fetch, Net.listen, IO.stdin, IO.stdout, IO.stderr, Env.read, Env.write, Proc, Time, Rand, Fan, Abort
  hint: did you mean `FS`?
```

提示给出最接近的能力名，与 `almide check --profile
critical --allow` 报告未知名称的方式相同（`docs/specs/cli.md`）。

测试：`tests/manifest_permissions_test.rs`、`tests/manifest_toml_reader_test.rs`、`tests/diagnostics/permissions-unknown-capability/`、`tests/diagnostics/permissions-unknown-capability-multiline/`

<a id="enforcement"></a>

### 强制执行

代码生成后，编译器运行 `EffectInferencePass`，通过不动点迭代计算每个函数的传递效应（直接 stdlib 调用及被调用函数的效应）。如果任何函数使用未列入 `allow` 的能力，构建以能力违规错误失败。

```almide project build-fail=capability
// file: almide.toml
[package]
name = "permdemo"
version = "0.1.0"

[permissions]
allow = ["IO"]
// file: main.almd
import http

// This function uses Net.fetch (http.get) -> build error
effect fn fetch() -> Result[String, String] = http.get("https://example.com")
//   error: capability violation in `fetch`
//     Net.fetch is not in [permissions].allow
```

每条路径都应用 gate：`almide check`、`build`、`run`、`emit` 及两个 `almide test` lane，覆盖两个目标。清单来自入口文件的项目——入口文件所在目录或其上层最近的 `almide.toml`，否则使用工作目录中的清单——而 `check` 判定所有用户模块，不只是入口文件。

测试：`tests/permissions_routes_test.rs`。

<a id="directory-scope-fs"></a>

### 目录范围：`fs`

`[permissions] fs` 列出程序可以触及的目录（#4022）：

```toml
[permissions]
fs = ["./data", "/tmp"]
```

相对项是项目目录，以 `almide.toml` 的目录为基准（如同相对 `path` 依赖），因此范围不随启动程序的目录变化。

- **缺省**（无 `fs` 键）：像以前一样允许所有路径。这是默认值，因此当前可运行的程序不会停止工作。`fs = []` 不允许任何路径。
- **看解析结果，而非写法。** 路径按 OS 实际到达的位置判定：相对工作目录绝对化，由 OS 解析最长已存在前缀中的所有符号链接和 `..`，其余部分按词法应用。若路径落在所有授权目录之外，或经过悬空符号链接，会在触及前被拒绝：`fs.exists`、`fs.is_dir`、`fs.is_file`、`fs.is_symlink` 返回 `false`，其他所有 `fs` 调用返回 `err("fs: \"<path>\" is outside [permissions] fs")`。no-follow 调用也按路径指向的位置判定，所以授权目录中指向外部的链接既不能查询，也不能删除。`copy` 和 `rename` 检查两个路径；temp 调用检查创建所在的临时目录；`glob` 检查模式的字面基目录。
- **Native**（`almide run`、`almide build`、`almide test`）：构建用检查包装程序对 fs 运行时的每次调用，根目录嵌入产物，并在第一次 fs 调用时解析，随后调用运行时函数的范围受限 twin（`src/cli/fs_scope_twin.rs`：相同本体，但 `std::fs` 调用经由范围机制）；运行时自身不变。测试 harness 构建没有 `main`，也以同样方式约束（#4084）。
- **Wasm，嵌入式宿主**（`almide run --target wasm`、`almide test` 的 wasm leg）：来自同一源码（`crates/almide-rt-core/src/fs_scope.rs`）的同一检查位于宿主的单个 fs import 之前，宿主 fs 调用也经过同一受限 core，因此两个目标用相同字节拒绝相同路径。以 p3 component（full-response http 系列）运行的程序，将授权目录作为 WASI preopen；没有键时则是工作目录和临时目录，不再是 `/`，因此这里的拒绝是 WASI 自身的错误。
- **Wasm，标准运行时**：`almide build --target wasm` 的产物可以访问其运维者预打开的范围（`wasmtime --dir`）；清单不会随产物携带。

检查只决定路径落在哪里；在原生与嵌入式宿主上，随后操作在所到达的授权目录的描述符之下运行，该描述符在首次使用时打开一次（`crates/almide-rt-core/src/fs_scoped_core.rs`，#4084）。从该描述符遍历已决定的路径分量，不跟随符号链接：Linux 的 64 位和 32 位使用 openat2 `RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS`，macOS 使用 `O_NOFOLLOW` 的 openat 遍历，Windows 使用相对于持有目录句柄的 `NtCreateFile` 并带 `FILE_OPEN_REPARSE_POINT`，任何 reparse point——符号链接、junction——都像符号链接一样被拒绝（#4293）。最后一个分量从其父描述符打开、创建、列举、删除或重命名。检查与调用之间重写的目录树（目录被替换为符号链接）会被调用用相同 `err` 拒绝，绝不跟随。对授权目录本身的操作在其持有描述符上运行，或确认父目录中的项就是所持目录后，从父描述符运行；进程不能打开的项，其元数据也按描述符取得。在 Linux、macOS、Windows 之外的平台，所有受范围约束的调用都被拒绝，工具链在构建前拒绝该键。p3 component 的边界是 preopen，由运行时（基于 cap-std 的 wasmtime-wasi）在调用本身时于预打开描述符之下解析。程序内，`fs.open_root`（`docs/stdlib/fs.md`）在此范围之上，把由不可信输入构建的路径约束到一个目录；其调用在每个 leg 上都通过描述符拒绝符号链接（#4084），因此重写不能与它们形成竞态。

反转默认值——缺少 `fs` 键仅表示项目目录——会拒绝目前所有读取项目外部内容的程序（绝对配置路径、`$HOME` 文件、`/dev/stdin`）。这需要 dialect epoch 条目、拒绝中的 `fs = ["/"]` 提示，以及 `almide init` 写入该键；不属于本次变更。

测试：`tests/fs_scope_test.rs`

<a id="callbacks-the-set-rides-the-value-adr-0026-d1"></a>

### 回调：集合随值携带（ADR-0026 D1）

闭包的类别在其被**调用**的位置执行，集合随值移动。创建闭包不执行任何能力。集合通过求解得出，从不手写：

- **无修饰的 fn 类型参数是透明的。** 调用传入闭包的函数（`f(x)` 或 `xs |> list.map(f)`）以参数报告，绝不猜测被调用者；每个传入闭包的调用点都计入该闭包的集合——`apply(paths, (p) => …fs…)` 携带 `IO`，`apply(names, (s) => string.len(s))` 不携带类别。
- **存储或返回的值保留其集合**：`let`/`var` 局部变量、记录字段（存入该字段的所有闭包的并集）、返回值、顶层 `let`。
- 放到其他任何位置的闭包（列表、option payload、variant、未知被调用者的实参）加入其签名的池（展开别名，`effect (A) -> B` 槽位采用 carrier 形式）；调用从此类位置读回的值会计入该池；若读取的类型使签名保持开放（类型变量），则计入其可能别名到的所有池。传入泛型参数的值在调用点按具体类型入池（#4151）。
- 本体由运行时提供的 stdlib 函数（`@intrinsic`、`= _`）运行每个 fn 类型实参且不保留任何一个；通过类型变量参数接收的值进入池，结果按调用类型从池读取。#4152 之前，这种本体被视为“什么都不做”，所以 `list.map(paths, fs.read_text)` 未计入文件系统读取能力。在当前叶节点词汇中必须计入 `FS.read`；将 stdlib fn 作为值仍保留该类别。

```
  call_box  → {FS.read}
  make  → {} (effect fn); returns a closure doing {FS.read}
  use_it  → {} + whatever f (arg 1) does
  main  → {FS.read, IO.stdout} (effect fn)
```

`[permissions].allow` 检查这些集合，因此违规会指出**运行**闭包的函数，每个类别给出一条路径（D4）。经过回调的一步先指出参数，已知时再指出具体闭包；可能流入多个值时（参数的其他调用点、池化值），下一步加前缀 `e.g.`：

```
error: capability violation in `main`
  FS.read is not in [permissions].allow
  path: main → use_it → f (arg 1 of use_it) → closure (line 3:42 in make) → fs.read_text (line 3:49)
```

集合存在于效应分析（`crates/almide-codegen/src/effect_flow/`）中，而不在 `Ty::Fn` 中：类型检查后计算，随报告丢弃，所以单态化从不看到它们。不涉及表面语法。

测试：`tests/effect_category_flow_test.rs`、`tests/effects_report_callbacks_test.rs`

<a id="native-code-from-dependencies-native-4093"></a>

### 依赖的原生代码：`native`（#4093）

上述类别从 Almide 代码推断。依赖的原生代码（`native/*.rs`、`[native-deps]` crate 及其 `build.rs`、`@extern(rust|c)` fn）在 cargo 构建程序时运行，发生在推断之前且不在其范围内。`[permissions] native` 列出允许带入此类代码的依赖包：

```toml
[permissions]
native = ["nat"]
```

`build`、`run`、`test` 和 `bench` 在每个目标上拒绝未授权的依赖；根包自身的原生代码不需要授权（[package-system.md §6.4](./package-system.zh-CN.md#64-native-code-from-a-dependency-needs-a-grant-permissions-native)）。

<a id="design-layers"></a>

### 设计层次

- **第 1 层**（已实现）：`effect fn` 与 `fn`——由类型检查器强制执行
- **第 2 层**（已实现）：almide.toml 中的 `[permissions].allow`——逐包限制 stdlib 能力
- **第 3 层**（未来）：使用方限制依赖的能力

测试：`crates/almide-codegen/src/pass_effect_inference.rs` 中的效应推断单元测试（`#[cfg(test)]` 模块）

<a id="9-error-codes"></a>

## 9. 错误代码

| 代码 | 名称 | 触发条件 | 修复 |
|------|------|---------|-----|
| E006 | 效应隔离违规 | 纯 `fn` 调用 `effect fn` | 将调用者标为 `effect fn`；若调用的 Result 尚未消费（`!` / `??` / `?` / `match`），还需加 `!`——提示给出两个步骤，同一表达式的 `expected T but got Result[T, String]` 不再单独报告（#3515，测试：`tests/e006_fallible_cascade_test.rs`） |
| E007 | 纯函数中的 fan 块 | 在效应上下文外使用 `fan { ... }` | 将外围函数标为 `effect fn` |
| E008 | fan 捕获可变变量 | `fan` 块引用 `var` 绑定 | 将 `var` 改为 `let`，或在 `fan` 前把值复制到 `let` |
| E109 | 通过值运行 effect fn | 纯 `fn` 运行作为值到达的 `effect fn`（字段、列表元素、保存的闭包） | 将该 fn 标为 `effect fn`，或传给它不做 I/O 的值 |
| E092 | `@pure` fn 不纯 | `@pure` fn 到达输出、中止、带类别的 stdlib 调用或 `@extern` | 将调用移到调用者，或移除 `@pure` |
