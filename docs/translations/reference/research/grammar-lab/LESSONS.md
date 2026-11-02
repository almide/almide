<a id="grammar-lab-初期開発で学んだこと履歴"></a>

# Grammar Lab: Lessons from Early Development (Historical)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](LESSONS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](LESSONS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](LESSONS.zh-CN.md)

> Non-normative historical source-pinned reader snapshot. The unchanged original remains authoritative; no automatic synchronization, independent runs or measurements. Historical workarounds and commands are not current instructions. Code comments are verbatim. LESSONS expressly corrects old auto-unwrap, API names and argv[0] assumptions in its opening; do not apply the old recommendations as current API guidance. REPORT’s equivalence/significance claims, general recommendations and bug status are the original authors’ dated interpretations, not universal guarantees. Its duplicate section 5, four-improvements/three-row mismatch, Sonnet-rule attribution versus run table, and later t05 difficulty/pattern-enumeration descriptions are retained without silently reconciling them. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/grammar-lab/LESSONS.md)

A historical record of problems and workarounds from when the initial runner was written.
**This is not a guide to the current API or error propagation.** In particular, the descriptions of auto-unwrap, `++`, `fn(x) =>`, `process.exec_status`, and the absence of `println` describe the old state. Current usage includes explicit postfix `!`, `+`, `(x) =>`, `process.output`, and builtin `println`. The current result of `env.args()` contains user arguments, not argv[0]. See the current [CHEATSHEET](../../../../CHEATSHEET.md) and [Result/Option context table](../../../../specs/result-option-effect.md). The following failures and workarounds from that time are preserved unchanged as research history.

---

<a id="1-stdlib-api-の名前"></a>

## 1. stdlib API Names

API names that LLMs (including myself) easily get wrong:

| Often written | Correct API | Reason |
|---|---|---|
| `io.println(x)` | `io.print(x ++ "\n")` | `println` does not exist |
| `args.collect()` | `env.args()` | There is no `args` module; use the `env` module |
| `time.now()` | `env.unix_timestamp()` | There is no `time` module; it is in `env` |
| `fs.exists(path)` | `fs.exists?(path)` | Includes `?` |
| `fs.create_dir(dir)` | `fs.mkdir_p(dir)` | POSIX-style naming |
| `process.run(cmd, args)` | `process.exec_status(cmd, args)` | `run` does not exist |

**Lesson:** Almide stdlib names differ subtly from other languages. Points where LLMs are likely to err should go into Layer 1 of the Language Context.

---

<a id="2-effect-fn-内の-auto-unwrap"></a>

## 2. Auto-Unwrap Inside effect fn

Calling a function returning `Result` inside an effect fn automatically unwraps it, equivalent to `?`.

```almide
// effect fn 内
let config = json.parse(config_text)  // Result[Json, String] → Json に auto-unwrap
json.get_string(config, "name")       // config は Json 型として使える
```

**Pitfall:** Manually unwrapping with `match` leaves the variable's type as `Result`, producing a type mismatch in subsequent code.

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

## 3. Match Branches and Type Unification in effect fn

Mixing `Result` and non-`Result` types in branches of `match` or `if` inside an effect fn produces a codegen error.

```almide
// ❌ コンパイルは通るが Rust codegen で失敗
effect fn call_llm(provider: String, ...) -> Result[String, String] = {
  match provider {
    "anthropic" => call_anthropic(...),  // auto-unwrap → String
    _ => err("Unknown"),                  // Result[String, String]
  }
}
```

**Solution:** Return early with `guard`, then branch.

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

## 4. Ownership Moves in Nested Loops (Rust codegen)

In four nested `for` loops (models × variants × tasks × trials), passing an outer-loop variable to `json.object` inside causes a Rust ownership move.

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

**Solution:** Separate JSON construction into a function and pass values as arguments (Almide borrow inference inserts clones at function boundaries).

```almide
fn make_result_json(experiment: String, variant: String, ...) -> Json =
  json.object([("experiment", json.s(experiment)), ...])

// ループ内
let result = make_result_json(experiment_name, variant_name, ...)
```

---

<a id="5-processexec_status-の戻り値"></a>

## 5. Return Value of `process.exec_status`

`process.exec_status` returns `Result[{code: Int, stdout: String, stderr: String}, String]`.

- Inside effect fn, auto-unwrap turns it into `{code: Int, stdout: String, stderr: String}`
- Manually unwrapping with `match ok(r) => ... / err(e) => ...` causes a codegen error

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

## 6. No `println`

Only `io.print` exists. Write newline-terminated output yourself.

```almide
effect fn println(s: String) -> Unit = io.print(s ++ "\n")
```

Using `fn` instead of `effect fn` causes a compile error (because `io.print` is an effect function).

---

<a id="7-jsonparse-の-auto-unwrap-と後続の-jsonget_"></a>

## 7. Auto-Unwrap of `json.parse` and Subsequent `json.get_*`

`json.parse` returns `Result[Json, String]`. Auto-unwrap inside effect fn turns it into `Json`. However, functions in the `json.get_*` family can also return `Result` (in a context that directly passes the result of `json.parse`).

Looking at the test code (`json_test.almd`):
```almide
let obj = json.parse("{\"n\": 42}")
assert_eq(json.get_int(obj, "n").unwrap_or(0), 42)
```

The same auto-unwrap applies inside test blocks.

---

<a id="8-stringto_int-は-resultint-string"></a>

## 8. `string.to_int` Returns `Result[Int, String]`

Inside a pure fn, unwrap it with `match`.

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

## 9. `env.args()` Includes argv[0]

`env.args()` directly uses Rust's `std::env::args()`, so the first element is the binary path.

```almide
let raw_args = env.args()         // ["/tmp/mybin", "arg1", "arg2"]
let args = list.drop(raw_args, 1) // ["arg1", "arg2"]
```

Via `almide run`, argv[0] is the temporary binary's path; using it unknowingly breaks paths.

---

<a id="10-effect-fn-main-の-err-が-silent-exit1-になる"></a>

## 10. Err from effect fn main Becomes Silent exit(1)

When `effect fn main() -> Result[Unit, String]` returns `err(msg)`, it exits with only exit code 1, without printing an error message.

**Cause:** codegen generates `let _ = e; std::process::exit(1);` (`src/emit_rust/program.rs:258`).

**Workaround:** Match in main and print manually.

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

**TODO:** The compiler should generate `eprintln!("{}", e)`.

---

<a id="11-stringpad_right-が-ownership-を取る-codegen"></a>

## 11. `string.pad_right` Takes Ownership (codegen)

In Rust codegen, `almide_rt_string_pad_right(s: String, ...)` takes ownership of `s`, so reusing the variable later causes a move error.

**Workaround:** Arrange the order of variable use, or first perform an operation equivalent to clone (such as creating intermediate results with `list.map`).

---

<a id="12-テストファイルの型二重定義"></a>

## 12. Duplicate Type Definitions in Test Files

The runner concatenates with `"${code}\n\n${test_code}"`, so a test file containing the same `type` definition as the source causes a Rust codegen error: `Item is defined multiple times`.

**Solution:** Do not put type definitions in test files. Types are defined on the source-code side.

---

<a id="13-transpile-で未実装構文を実装前にテスト"></a>

## 13. Test Unimplemented Syntax Before Implementation via Transpilation

Have the LLM write unimplemented syntax (`(x) => expr`) and transpile it into current syntax (`fn(x) => expr`) before compilation. This allows syntax experiments without compiler changes.

```
LLM output: (x) => x * 2
  ↓ transpile
fn(x) => x * 2
  ↓ almide check + almide test
PASS/FAIL
```

**Caution:** `regex.replace` does not support capture-group back references (`$1`). Enumerating known patterns with `string.replace` on strings is more reliable.

---

<a id="14-llm-が-if----else---と書く他言語の癖"></a>

## 14. LLMs Write `if ... { } else { }` (Habits from Other Languages)

Almide uses `if ... then ... else ...`, but LLMs write `if ... { } else { }` out of JS/Rust habits.

Avoid this by explicitly including `if` syntax in Layer 1:
```
- `if` uses `then`: `if x > 0 then "yes" else "no"` (not `if (x > 0) { }`)
```

---

<a id="15-transpile-のパターン列挙は脆い"></a>

## 15. Enumerating Transpile Patterns Is Fragile

Pattern-enumeration transpilation (using string.replace for each variable name to turn `(x) =>` into `fn(x) =>`) cannot cover every variable name an LLM uses. Missing `(o) =>` and `(t) =>` dropped paren-lambda to 26%.

**Solution:** Switch to a general algorithm that finds `") =>"` and scans backward for `(`. It does not depend on parameter names.

---

<a id="16-llm-の生出力を保存すべき"></a>

## 16. Save Raw LLM Output

Distinguishing transpile bugs from LLM errors requires the raw output before transpilation. Added a mechanism to save `_raw.almd` (raw output) and `_compiled.almd` (after transpilation) in the `outputs/` directory.

---

<a id="全体の教訓"></a>

## Overall Lessons

1. **Rely on auto-unwrap in effect fn** — manual `match ok/err` easily causes type mismatches
2. **Separate JSON construction into a function in nested loops** — avoid ownership moves
3. **Check exact stdlib API names** — LLMs make mistakes from habits learned in other languages; this itself is a finding to incorporate into Grammar Lab Layer 1
4. **Codegen errors occur after passing Almide's checker** — checker pass ≠ Rust compile pass, especially around ownership
