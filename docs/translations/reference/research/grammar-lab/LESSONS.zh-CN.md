<a id="grammar-lab-初期開発で学んだこと履歴"></a>

# Grammar Lab：早期开发的经验（历史记录）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](LESSONS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](LESSONS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](LESSONS.zh-CN.md)

> 非规范性、固定原文的历史读者快照。未修改原本仍为权威来源；不自动同步，未独立运行或重新测量。当时的应对方法和命令不是当前指示。代码注释保持原样。LESSONS 开头明确更正了旧 auto-unwrap、API 名称和 argv[0] 假设，不应把后文旧建议作为当前 API 指南。REPORT 的等同／显著性主张、一般建议及 bug 状态属于原作者当时的解释，不是普遍保证。保留重复的第5节、四次改进与三行表格的差异、Sonnet 规则效果与运行表的叙述差异，以及后文 t05 难度／模式枚举描述，不擅自统一。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/LESSONS.md)

保留编写早期 runner 时遇到的问题与应对方法的历史资料。
**这不是当前 API／错误传播指南。** 尤其是 auto-unwrap、`++`、`fn(x) =>`、`process.exec_status` 以及不存在 `println` 的说明均属旧状态；当前使用显式后缀 `!`、`+`、`(x) =>`、`process.output`、内建 `println` 等。`env.args()` 当前返回用户参数，不包含 argv[0]。参阅当前 [CHEATSHEET](../../../../CHEATSHEET.md) 与 [Result/Option 上下文表](../../../../specs/result-option-effect.md)。下列失败示例及当时的 workaround 作为研究过程原样保留。

---

<a id="1-stdlib-api-の名前"></a>

## 1. stdlib API 名称

LLM（包括我自己）容易弄错的 API 名称：

| 容易写成 | 正确 API | 原因 |
|---|---|---|
| `io.println(x)` | `io.print(x ++ "\n")` | 不存在 `println` |
| `args.collect()` | `env.args()` | 不存在 `args` 模块，应为 `env` 模块 |
| `time.now()` | `env.unix_timestamp()` | 不存在 `time` 模块，在 `env` 中 |
| `fs.exists(path)` | `fs.exists?(path)` | 带 `?` |
| `fs.create_dir(dir)` | `fs.mkdir_p(dir)` | POSIX 风格命名 |
| `process.run(cmd, args)` | `process.exec_status(cmd, args)` | 不存在 `run` |

**经验：** Almide 的 stdlib 命名与其他语言有细微差别。LLM 容易出错的要点应放入 Language Context 的 Layer 1。

---

<a id="2-effect-fn-内の-auto-unwrap"></a>

## 2. effect fn 内的 auto-unwrap

在 effect fn 内调用返回 `Result` 的函数时，会自动进行相当于 `?` 的 unwrap。

```almide
// effect fn 内
let config = json.parse(config_text)  // Result[Json, String] → Json に auto-unwrap
json.get_string(config, "name")       // config は Json 型として使える
```

**踩坑模式：** 用 `match` 手动 unwrap 时，变量类型仍为 `Result`，导致后续代码类型不匹配。

```almide
// ❌ 冗長で型が合わなくなる
let config = match json.parse(config_text) {
  ok(j) => j,
  err(e) => err("Failed: ${e}"),
}
// config の型が曖昧になる

// ✅ effect fn 内なら直接代入
let config = json.parse(config_text)
// auto-unwrap で config は Json 型
```

---

<a id="3-effect-fn-の-match-分岐と型の統一"></a>

## 3. effect fn 中的 match 分支与类型统一

在 effect fn 内，`match` 或 `if` 的分支混用 `Result` 类型与非 `Result` 类型会产生 codegen 错误。

```almide
// ❌ コンパイルは通るが Rust codegen で失敗
effect fn call_llm(provider: String, ...) -> Result[String, String] = {
  match provider {
    "anthropic" => call_anthropic(...),  // auto-unwrap → String
    _ => err("Unknown"),                  // Result[String, String]
  }
}
```

**解决：** 先用 `guard` 提前 return，再分支。

```almide
// ✅
effect fn call_llm(provider: String, ...) -> Result[String, String] = {
  guard provider == "anthropic" or provider == "openai" else err("Unknown provider")
  if provider == "anthropic" then {
    ok(call_anthropic(...))
  } else {
    ok(call_openai(...))
  }
}
```

---

<a id="4-nested-loop-での-ownership-move-rust-codegen"></a>

## 4. 嵌套循环中的 ownership move（Rust codegen）

四重 `for` 循环（models × variants × tasks × trials）中，将外层循环变量在内层传给 `json.object` 会发生 Rust ownership move。

```almide
// ❌ Rust codegen で move error
for model in models {
  for variant in variants {
    for task in tasks {
      for trial in 1..=n {
        json.object([("model", json.s(model_name)), ...])
        // ↑ model_name が move されて次の trial で使えない
      }
    }
  }
}
```

**解决：** 将 JSON 构建拆成独立函数，通过参数传入（Almide 的借用推断在函数边界插入 clone）。

```almide
fn make_result_json(experiment: String, variant: String, ...) -> Json =
  json.object([("experiment", json.s(experiment)), ...])

// ループ内
let result = make_result_json(experiment_name, variant_name, ...)
```

---

<a id="5-processexec_status-の戻り値"></a>

## 5. `process.exec_status` 的返回值

`process.exec_status` 返回 `Result[{code: Int, stdout: String, stderr: String}, String]`。

- 在 effect fn 内自动 unwrap，成为 `{code: Int, stdout: String, stderr: String}`
- 用 `match ok(r) => ... / err(e) => ...` 手动 unwrap 会产生 codegen 错误

```almide
// ❌
let result = process.exec_status(cmd, args)
match result {
  ok(r) => r.code,    // Rust codegen: expected record, found Result
  err(e) => ...,
}

// ✅ effect fn 内なら直接フィールドアクセス
let r = process.exec_status(cmd, args)
if r.code != 0 then { ... } else { ... }
```

---

<a id="6-println-がない"></a>

## 6. 没有 `println`

只有 `io.print`。带换行的输出需自己编写。

```almide
effect fn println(s: String) -> Unit = io.print(s ++ "\n")
```

必须用 `effect fn` 而非 `fn`，否则编译出错（因为 `io.print` 是 effect function）。

---

<a id="7-jsonparse-の-auto-unwrap-と後続の-jsonget_"></a>

## 7. `json.parse` 的 auto-unwrap 及后续 `json.get_*`

`json.parse` 返回 `Result[Json, String]`。在 effect fn 内自动 unwrap 后变为 `Json`。不过 `json.get_*` 系列有时也返回 `Result`（直接传入 `json.parse` 结果的上下文中）。

查看测试代码（`json_test.almd`）：
```almide
let obj = json.parse("{\"n\": 42}")
assert_eq(json.get_int(obj, "n").unwrap_or(0), 42)
```

test block 中同样会应用 auto-unwrap。

---

<a id="8-stringto_int-は-resultint-string"></a>

## 8. `string.to_int` 返回 `Result[Int, String]`

在 pure fn 内使用时，需要用 `match` unwrap。

```almide
// ❌ pure fn 内では auto-unwrap されない
fn parse_int(s: String, default: Int) -> Int =
  if s == "" then default else string.to_int(s)

// ✅
fn parse_int(s: String, default: Int) -> Int =
  if s == "" then { default }
  else { match string.to_int(s) { ok(n) => n, err(_) => default } }
```

---

<a id="9-envargs-は-argv0-を含む"></a>

## 9. `env.args()` 包含 argv[0]

`env.args()` 直接使用 Rust 的 `std::env::args()`，因此首个元素是二进制路径。

```almide
let raw_args = env.args()         // ["/tmp/mybin", "arg1", "arg2"]
let args = list.drop(raw_args, 1) // ["arg1", "arg2"]
```

通过 `almide run` 时 argv[0] 是临时二进制路径，不知情地使用会破坏路径。

---

<a id="10-effect-fn-main-の-err-が-silent-exit1-になる"></a>

## 10. effect fn main 的 Err 变为静默 exit(1)

`effect fn main() -> Result[Unit, String]` 返回 `err(msg)` 时，不显示错误消息，只以退出码1结束。

**原因：** codegen 生成 `let _ = e; std::process::exit(1);`（`src/emit_rust/program.rs:258`）。

**应对方法：** 在 main 中 match，手动输出。

```almide
effect fn main() -> Result[Unit, String] = {
  match run_experiment() {
    ok(_) => ok(()),
    err(e) => {
      println("Error: ${e}")
      process.exit(1)
      ok(())
    },
  }
}
```

**TODO：** 编译器应生成 `eprintln!("{}", e)`。

---

<a id="11-stringpad_right-が-ownership-を取る-codegen"></a>

## 11. `string.pad_right` 取得所有权（codegen）

Rust codegen 中，`almide_rt_string_pad_right(s: String, ...)` 取得 `s` 的所有权，因此之后再次使用同一变量会产生 move 错误。

**应对方法：** 调整变量使用顺序，或预先执行相当于 clone 的操作（如用 `list.map` 创建中间结果）。

---

<a id="12-テストファイルの型二重定義"></a>

## 12. 测试文件中类型重复定义

runner 用 `"${code}\n\n${test_code}"` 拼接代码，所以测试文件若包含与源码相同的 `type` 定义，Rust codegen 会报 `Item is defined multiple times`。

**解决：** 不在测试文件中写类型定义。类型由源码侧定义。

---

<a id="13-transpile-で未実装構文を実装前にテスト"></a>

## 13. 用转译在实现前测试未实现语法

让 LLM 用未实现语法（`(x) => expr`）编写，在编译前转译为当前语法（`fn(x) => expr`）。这样无需修改编译器即可实验语法。

```
LLM output: (x) => x * 2
  ↓ transpile
fn(x) => x * 2
  ↓ almide check + almide test
PASS/FAIL
```

**注意：** `regex.replace` 不支持捕获组反向引用（`$1`）。用字符串的 `string.replace` 枚举已知模式更可靠。

---

<a id="14-llm-が-if----else---と書く他言語の癖"></a>

## 14. LLM 写出 `if ... { } else { }`（其他语言的习惯）

Almide 使用 `if ... then ... else ...`，但 LLM 会因 JS/Rust 习惯写成 `if ... { } else { }`。

在 Layer 1 明确加入 `if` 语法来避免：
```
- `if` uses `then`: `if x > 0 then "yes" else "no"` (not `if (x > 0) { }`)
```

---

<a id="15-transpile-のパターン列挙は脆い"></a>

## 15. 枚举转译模式很脆弱

枚举模式的转译（按变量名逐个 string.replace，将 `(x) =>` 改为 `fn(x) =>`）无法覆盖 LLM 使用的所有变量名。漏掉 `(o) =>`、`(t) =>` 使 paren-lambda 降到26%。

**解决：** 改为找到 `") =>"` 后反向寻找 `(` 的通用算法，不依赖参数名称。

---

<a id="16-llm-の生出力を保存すべき"></a>

## 16. 应保存 LLM 原始输出

要区分转译 bug 和 LLM 错误，需要转译前的原始输出。已加入将 `_raw.almd`（原始输出）和 `_compiled.almd`（转译后）保存到 `outputs/` 目录的机制。

---

<a id="全体の教訓"></a>

## 总体经验

1. **依赖 effect fn 的 auto-unwrap**——手动 `match ok/err` 容易造成类型不匹配
2. **嵌套循环中构建 JSON 时拆出函数**——避免 ownership move
3. **确认 stdlib 的准确 API 名称**——LLM 会因其他语言习惯而出错；这本身就是应反映到 Grammar Lab Layer 1 的发现
4. **codegen 错误在通过 Almide checker 后出现**——checker pass ≠ Rust compile pass，尤其是所有权方面
