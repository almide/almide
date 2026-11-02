<a id="adr-0029-testingassert_abortsexpected-closure-runs-only-its-closure-in-a-child-process-or-a-fresh-instance-the-closure-captures-nothing-every-abort-counts-and-testingassert_throws-leaves-at-dialect-epoch-14"></a>

# ADR-0029：`testing.assert_aborts(expected, closure)`仅在子进程或新实例运行Closure；不允许Capture，所有Abort均计入，`testing.assert_throws`于Dialect Epoch14退出

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](0029-assert-aborts-runs-its-closure-in-a-child.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](0029-assert-aborts-runs-its-closure-in-a-child.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](0029-assert-aborts-runs-its-closure-in-a-child.zh-CN.md)

> 非规范性历史读者快照，固定原本未改且为权威。不自动同步、未独立测试，不授权新进程执行或保证当前release。保留原文矛盾：已接受/九问已决定与前文仍待裁定并存。D3估四个spec调用与D7新增case不擅自重算。定义abort限定status1加指定stderr header，不是所有终止；保留raw panic/trap/signal排除及刻意header+exit假阳性。子effect可影响继承环境/文件，program状态隔离不是安全sandbox。明确stock-wasmtime fallback和interp弃权。诊断/契约ID是历史预留，实现时须重查。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0029-assert-aborts-runs-its-closure-in-a-child.md)

- **状态**：已接受（2026-10-18）。九个Open问题已依Q15选择的Swift exit-test先例决定，见[未决项](#open)。
  - 方向来自owner2026-10-17裁定○：替代 `testing.assert_throws(f, expected)`，在epoch14落地。
  - 写法来自owner Q15（2026-10-17）：本体调用 `testing.assert_aborts("expected substring", () => { … })`，按Swift6.2 exit test运行：runner只在子进程(native)或新instance(wasm)执行closure。不可capture，所有abort均计入。
  - Q15否决初稿test级 `@aborts("…")` 属性及后来的test-header modifier，作为否决方案保留在下文。
  - 本ADR补充runner、capture检查、迁移。[未决项](#open)仍需裁定。
- **日期**：2026-10-17
- **范围**：
  - `testing.assert_aborts` 表面与类型；
  - check时调用位置、closure无capture规则；
  - native test binary与wasm lane如何隔离运行closure及父方比较什么；
  - 删除 `testing.assert_throws`；
  - 契约与dialect台账变化。
- **编号**：ADR-0028被未merge的 `fan-semantics`（fan选择名称）使用，因此为0029。
- **相关**：[ADR-0020](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0020-http-serving-and-handler-state.md) §5.2（E095，served app因在别的instance运行也不可capture local）、[ADR-0022](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md)、[ADR-0025](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0025-wasm-process-is-a-private-host-capability.md)（private embedded-host import）、[ADR-0026](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0026-effect-categories-ride-the-transparent-fn-type.md)、[ADR-0027](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) Open3（本ADR取代）、C-067 / C-153 / C-196 / C-219 / C-350 / C-381 / ALS-T6 / ALS-T18、#3747、`proofs/dialect-epochs.toml` epoch14（branch `remove-33`）。

<a id="context"></a>

## 背景

`testing.assert_throws(f, expected)`运行thunk `f`，若 `f`以包含 `expected` 的消息panic则通过。仅在native test build一处有效，即便那里也比看起来捕获得少。

**Native。**
- `runtime/rs/src/testing.rs` 用 `std::panic::catch_unwind` 包住 `f`。
- `--test` build保留 `panic(msg)` 为可unwind Rust `panic!`（`almide_panic!` 的 `cfg!(test)` 分支，`crates/almide-codegen/src/lib.rs:414-422`），因此捕获 `panic`。
- 其他abort均经 `almide_abort`，执行一次guard保护的 `process::exit(1)`。包括越界索引C-067、除零/overflow ALS-T6、stack overflow C-196及domain错误。`catch_unwind`看不到exit。
- 这些abort在 `assert_throws` 内发生就终止整个test binary；test不通过，同file后续test也不执行（`src/cli/test_output.rs`、`a_process_ending_abort_leaves_the_failing_test_unfinished`）。

**Wasm。**
- 无法unwind；全部abort为module内stderr行加 `proc_exit(1)`。
- `crates/almide-mir/src/host_ops.rs:243`拒绝 `testing.assert_throws`（“需要MIR rung没有形式的unwind catch”）。
- 两个使用它的test file有 `// wasm:skip`，列于 `proofs/wasm-fallback-baseline.txt`（`HARNESS_LIMITATION`）及 `tests/wasm_skip_ledger_test.rs`（`NoUnwinding`），只走native fallback。

**Test block外。** direct call去糖为T18 abort（C-381，`crates/almide-frontend/src/lower/calls_testing_assert.rs`）。无捕获机制，只会失败。

函数承诺捕获abort并检查消息，然而没有leg在一般情形守住承诺。不能进程内捕获时，唯一可行办法是让另一个process/instance终止，从外部观察。Swift6.2将此内建于test库，owner选择其形态。

<a id="every-current-caller"></a>

### 当时全部调用方

| 位置 | 内容 | Repo |
|---|---|---|
| `spec/stdlib/coverage_assert_throws_test.almd` | 2 test：`panic("oh no")` / `"oh no"` 和 `panic("something went wrong")` / `"went wrong"`；`// wasm:skip` | almide, als |
| `spec/stdlib/testing_polarity_test.almd` | 1 test内**两次**调用：`assert_none(some(1))` 与 `assert_err(ok(0))`；`// wasm:skip` | almide, als |
| `spec/wasm_cross/testing_assert_abort_throws.almd` | C-381证据，test外调用永远失败 | almide, als |
| `stdlib/testing.almd:10-11` | `@intrinsic`声明 | almide |
| `runtime/rs/src/testing.rs:35-49` | `almide_rt_test_assert_throws` | almide |
| `crates/almide-frontend/src/lower/calls_testing_assert.rs` | T18去糖分支（`"assert_throws" => Some(2)`，thunk处理） | almide |
| `crates/almide-mir/src/host_ops.rs:243`, `crates/almide-mir/examples/classify_corpus_parts/classify_corpus_b.rs:103-139` | wasm拒绝行及corpus classifier | almide |
| `crates/almide-wasm/src/testing.rs:19` | 注释 | almide |
| generated: `crates/almide-types/src/generated/embedded.rs`, `crates/almide-codegen/src/generated/{rust_runtime.rs,runtime_fn_modes.rs}` | 从上述再生成 | almide |
| `proofs/stdlib-inspection.toml:11101`, `proofs/impl-count-ledger.toml:5938`, `proofs/target-availability.toml:126,280`, `proofs/wasm-fallback-baseline.txt:17,34,35` | 台账行 | almide |
| `tests/wasm_skip_ledger_test.rs:50,80`, `tests/wasm_dispatch_coverage_test.rs:28` | skip台账和注释 | almide |
| `crates/almide-spine/tests/golden/spec-check-manifest.txt:612`, `crates/almide-syntax/tests/golden/spec-ast-manifest.txt:612` | fixture manifest | almide |
| `docs/stdlib/testing.md:5-13,131`, `docs/stdlib/since.toml:941`, `docs/stdlib/semantics-manifest.toml:2351`, `docs/diagnostics/E081.md:29`, `docs/specs/als/text-and-numbers.md:301-309`（mirror） | 文档 | almide |
| `ref/src/stdlib_testing.rs:22` | 参考evaluator的testing列表 | als |
| `docs/contracts/contracts.toml` C-381, `docs/specs/als/text-and-numbers.md` ALS-T18, `docs/contracts/conformance.md:140` | 契约和spec | als（规范）、almide（mirror） |

`CHANGELOG.md:492` 与 `llms.txt:380`只作历史提及，保持不变。

<a id="survey-how-languages-say-this-code-must-abort-with-x"></a>

## 调查：如何表达“代码必须以X Abort”

| 语言 | 写法 | 层级 | 消息匹配 | 无法捕获的Abort（`exit`、fatal error） |
|---|---|---|---|---|
| **Swift（Testing，6.2）** | `await #expect(processExitsWith: .failure) { … }` | 本体closure | exit条件，可检查子输出 | **子进程**：以当前可执行文件启动新进程，`expression`不在父进程调用；closure不可capture父进程或外部词法context状态（编译时） |
| Go | 无内建，惯用子进程重跑test binary：`exec.Command(os.Args[0], "-test.run=TestCrasher")`，`BE_CRASHER=1` | 本体手写 | 手写 | 手写**subprocess** |
| Rust | `#[test] #[should_panic(expected = "msg")]` | test属性 | 指定字符串须出现在panic消息内，substring | 不覆盖，仅进程内unwind；`panic = "abort"`或 `process::exit`结束harness |
| Swift（XCTest） | `XCTAssertThrowsError(expr)` | 本体 | error值 | 不覆盖 `fatalError` |
| Zig | `try std.testing.expectError(error.X, f())` | 本体 | error-set tag | 不覆盖，期待panic测试提案ziglang/zig#1356仍**open** |
| pytest | `with pytest.raises(E, match=r"…"):` | 本体context manager | regex，`re.search` | 不覆盖 |
| Jest | `expect(() => f()).toThrow("msg")` | 本体closure | 消息**包含**substring | 不覆盖 |
| Kotlin（kotlin.test） | `assertFailsWith<T> { … }` | 本体closure | exception类型，随后检查返回exception | 不覆盖 |
| JUnit | 4: `@Test(expected = X.class)`; 5: `assertThrows(X.class, () -> …)` | 4 test，5本体closure | 类型 | 不覆盖 |
| Elixir ExUnit | `assert_raise ArgumentError, "msg", fn -> … end` | 本体closure | Regex或精确String | `catch_exit` |
| Gleam（gleeunit） | 无；`should.be_error`覆盖 `Result`，`panic`令test失败 | — | — | — |
| Roc | 无，只有 `expect`，`crash`令test失败 | — | — | — |

调查表明：

1. **接受closure的assert占主流。** Jest、Kotlin、Elixir、JUnit5、Swift皆用，pytest是同思路context manager变体。JUnit5以 `assertThrows`取代JUnit4 test级 `@Test(expected=)`；NUnit3也删除 `[ExpectedException]`。test级声明无法指明哪个statement应失败，setup抛错也会通过；closure明确statement。
2. **Swift和Go之外都在进程内捕获closure。** Almide所有leg都没有进程内catch（背景）。
3. **Swift exit test恰适合Almide。** 保留closure形式，只在子进程运行closure，因跨进程而禁止capture。Go `BE_CRASHER`是同机制手写版。

<a id="decision"></a>

## 决定

<a id="d1-the-surface-testingassert_abortsexpected-string-f----t---unit"></a>

### D1. 表面：`testing.assert_aborts(expected: String, f: () -> T) -> Unit`

```almide
import testing

fn nth(xs: List[Int], i: Int) -> Int = xs[i]

test "reading past the end aborts" {
  testing.assert_aborts("index out of bounds", () => nth([1, 2, 3], 5))
  testing.assert_aborts("PANIC: oh no", () => panic("oh no"))
  testing.assert_aborts("expected: none", () => testing.assert_none(some(1)))
  assert_eq(nth([1, 2, 3], 0), 1)   // the test goes on after each assert_aborts
}
```

- **参数**：Q15规定期待substring先、closure最后，与Elixir `assert_raise(E, msg, fn)` 和Kotlin尾部 `block`同序。
- **类型**：`[T] (String, () -> T) -> Unit`，丢弃closure值，所以 `() => xs[5]`无需包 `println`即可定型。
- **Effect**：closure可有effect（透明fn型，ADR-0026），可在子中读file/env，cwd和环境与test相同。
- **Import**：`testing`已要求 `import testing`，`assert_aborts`与其他 `testing.assert_*`并列。
- **易写性**：模型已在五种语言用此形表达“必须失败”；Almide唯一特别规则“无capture”在check时以fix hint强制，而非runtime失败（D2）。

<a id="d2-check-time-rules-one-new-check-two-diagnostics"></a>

### D2. Check时规则（一个新Check，两个诊断）

全在checker。复用 `crates/almide-frontend/src/concurrent_reach.rs` 与 `check/concurrent_reach_check.rs` 的closure-reach机制，已有E095（ADR-0020 §5.2，`http.serve` app因别的instance运行不能capture local）。这为 `testing.assert_aborts`第二参数增加slot kind。

1. **Closure不能capture任何内容**：`insp-all`下一个空码E098；ID共享，实现时重查。消息：`the closure passed to testing.assert_aborts captures \`xs\`, a local of the test`。
   - 必须lambda literal。test local、parameter、`where` binding和外lambda parameter都是capture。
   - 可引用top-level `fn`、`let`、import module和top-level `var`。子启动fresh program，`var`为初值（D3），与runner逐test re-init相同。
   - 非lambda literal的fn值（`testing.assert_aborts("x", f)`，`f`是local）也E098，因为看不到capture。
   - hint：`the closure runs in a separate process (native) or instance (wasm), where the test's locals do not exist. Inline the value into the closure, or make it a top-level \`let\`.`。若被capture local的binding为literal，`try:`展示移入closure的写法。
2. **可调用位置**：下一空码E099（`insp-all`）。必须direct call且词法上在 `test` block内，允许嵌入其中 `if`、`match`、`for`或lambda。以下错误：
   - `fn`（包括test调用的helper）或 `main`中。hint：`assert_aborts runs only in a test: move the call into a \`test "…" { }\` block`。想abort的program直接abort。
   - 一等使用（`let check = testing.assert_aborts`）。
   - 在另一个 `assert_aborts` closure内嵌 `assert_aborts`，这会需要孙进程。

<a id="d3-execution-the-runner-runs-only-the-closure-in-a-child"></a>

### D3. 执行：Runner只在子中运行Closure

**Lowering（frontend，全leg共用一次）。**
- 各call site无capture closure提升为top-level fn `__almd_abort_site_<k>`。`k`为按声明顺序的program内site index，同build父子间稳定。
- call变 `testing.__abort_site_run(k)`，leg专属intrinsic返回 `(Int, String)`，即子exit status和stderr。
- 后续pass/fail比较（D4）为 `stdlib/testing.almd`普通Almide代码，写一次、全leg继承。
- 提升site fn作为**program本体**而非test本体lower：
  - 其中 `assert*` / `testing.assert_*`采用T18 block形式（C-153 / C-381）；
  - native `panic`也走C-219 `PANIC: <msg>`路径。
  这让各leg abort文本字节相同（D4）。

**Native**（test binary为libtest，`src/cli/commands.rs::cmd_test`、`runtime/rs/src/testing.rs`）：
- codegen生成一个hidden `#[test] #[ignore = "almide:abort-site"] fn __almd_abort_site_entry()`，读 `ALMIDE_ABORT_SITE=<k>`调用site `k`。
- `__abort_site_run(k)`以 `--ignored --exact <path>::__almd_abort_site_entry --nocapture --test-threads=1`启动 `std::env::current_exe()`，设 `ALMIDE_ABORT_SITE=k`及 `ALMIDE_TEST_ABORT_PROBE=1`，等待返回 `(status, stderr)`。
- `ALMIDE_TEST_ABORT_PROBE=1`令 `almide_panic!`从 `cfg!(test)` unwind切换C-219 abort，runtime读取因此影响closure所调helper。
- 即Go `BE_CRASHER`模式由runtime代劳，无第二次build，普通run跳过ignored entry。

**Wasm**（`src/cli/test_wasm.rs`、`crates/almide-driver/src/test_runner.rs::synthesize`、`crates/almide-wasm-run`）：
- 合成runner `main`加selector。argv带 `--almide-abort-site=<k>`时运行global init后仅site `k`，否则照常跑test。
- `__abort_site_run(k)`降到private embedded-host import `almide:test/abort-site`，同ADR-0025 `almide:process/spawn`。host用该argv fresh实例化**同一已编译module**，运行至完成，返 `(status, stderr)`给父instance，父方继续。
- import只存在test build及embedded host（默认 `WasmTestRunner`）。
- `ALMIDE_TEST_WASM_RUNNER=wasmtime`（stock runtime）下，使用 `assert_aborts`的file以声明wall退回native并打印原因，绝不静默decline。
- `host_ops.rs:243`拒绝行随 `assert_throws`退出。

**Interp**（3-way oracle）：在fresh interpreter状态运行提升site fn，报告abort文本；当时对native abort弃权处（`docs/specs/als/expressions.md`裁定3）这里仍弃权。

**单Test多Call。** 每call依次运行自己的child，父test每次继续。因此可检查多个abort，旧polarity test两call需要如此。失败 `assert_aborts`与test内其他 `testing.assert_*`一样走harness报告，使该test失败，file其余照常。

**子看到什么。** fresh program，top-level let/var初始值，与test相同cwd/env/argv，无父方状态。stdout/stderr捕获不转发，只在失败报告（D5）显示。

**成本。** 每call一次process spawn(native)或已编译module实例化(wasm)。迁移后spec有4call（D7）。这是估计，实装PR测 `almide test spec/stdlib/`前后wall time。

<a id="d4-what-counts-as-an-abort-and-what-is-matched"></a>

### D4. 什么算Abort，匹配什么

`testing.assert_aborts(s, f)`当且仅当子以*定义的abort*结束，*abort文本*含 `s`时通过（大小写敏感byte substring）。

- **定义的abort**：exit status **1**，stderr含以 `Error: `或 `PANIC: `开头的header行。契约定义全abort恰如此：
  - `PANIC: <msg>`（C-219）；
  - `Error: index out of bounds`（C-067）；
  - `Error: division by zero` / `Error: integer overflow`及其余ALS-T6；
  - `Error: stack overflow`（C-196）；
  - `Error: exit code must be in 0..=255`（C-350）及其他ALS-R1 domain error；
  - `Error: assertion failed` block（C-153 / C-381 / C-339）。
- **Abort文本**：stderr从**最后一个**header行到末尾。closure前面的 `eprintln`不能满足匹配。
- **不依target**：契约固定native/wasm相同字节，D3用program mode运行，因此判定与target无关。
- **非abort**（均令assert失败）：
  - closure返回（exit0）；
  - 其他status：`process.exit(n)`且 `n ≠ 1`、raw Rust panic（101，属C-153级缺陷）、wasm trap（134，未定义abort所以I-divergence）、signal；
  - exit1无header。
- **保留一个假阳性**：closure自己打印 `Error: …`后调用 `process.exit(1)`，刻意模仿abort。T18本身是 `eprintln`加 `process.exit(1)`，排除此情形需连去糖都改道的abort side channel，不值得。

<a id="d5-the-failure-report-an-expectedfound-block-like-t18"></a>

### D5. 失败报告：T18式Expected/Found Block

`stdlib/testing.almd`比较按C-153字段顺序失败：

```
Error: assertion failed
  at: line 7
  expected: abort containing "index out of bounds"
  found: returned normally
```

`found:`取其一：
- `returned normally`
- `abort "<abort text>"`（abort但文本不符）
- `exit code <n>`
- `exit code 101 (raw panic)`
- `trap (exit 134)`

子有输出时后接 `child stdout:` / `child stderr:`段。

test block内与其他 `testing.assert_*`相同harness报告形式。`at:`为 `assert_aborts`调用行。

<a id="d6-removal-testingassert_throws-leaves-at-dialect-epoch-14-with-no-deprecation-window-ruled"></a>

### D6. 删除：`testing.assert_throws`于Epoch14退出，无弃用窗口（已裁定）

追加到 `remove-33`的epoch14 `breaks` entry：

```
"`testing.assert_throws(f, expected)` → rewrite as `testing.assert_aborts(expected, f)`: the expected text comes first, and `f` must be a lambda that captures nothing (it runs in a child process or instance); every abort counts — `panic`, index out of bounds, division by zero, a failed assert — not only `panic`. Outside a test block there is nothing to rewrite to: the call could only fail, so delete it",
```

**诊断。** `crates/almide-frontend/src/removed_spellings.rs`（`remove-33`）从 `breaks`读repair，但 `parse_break`只识别两形：`→ \`import x\` + \`x.f(…)\` (package …)`和 `→ delete the call`。原样读取会当prose，调用得到无repair的unknown-function E002。

故实现加第三固定形 `→ rewrite as \`m.g(…)\` …`，解析为 `Fate::Rewritten { to, repair }`，同PR扩 `parse_break`测试，调用得到：

```
error[E002]: `testing.assert_throws` was removed at dialect epoch 14
  hint: rewrite as testing.assert_aborts(expected, f) — the expected text first;
        f runs in a child and must capture nothing; every abort counts, not only panic
  try: testing.assert_aborts("oh no", () => panic("oh no"))
```

`try:`由call自身实参交换构成。test block外则给repair“删除”半部。

<a id="d7-migration-of-the-callers-almide-and-als-the-same-text"></a>

### D7. 调用方迁移（Almide与Als同文本）

`spec/stdlib/coverage_assert_throws_test.almd`改为 `spec/stdlib/coverage_assert_aborts_test.almd`：

```almide
import testing

test "panic is an abort" {
  testing.assert_aborts("oh no", () => panic("oh no"))
}

test "the expected text is a substring" {
  testing.assert_aborts("went wrong", () => panic("something went wrong"))
}

test "an index past the end is an abort" {
  testing.assert_aborts("index out of bounds", () => [1][3])
}

test "division by zero is an abort" {
  testing.assert_aborts("division by zero", () => 10 / (int.parse("0") ?? 1))
}
```

`spec/stdlib/testing_polarity_test.almd`保留一test两call，D3让两者匹配T18 block文本：

```almide
test "wrong polarity fails" {
  testing.assert_aborts("expected: none", () => testing.assert_none(some(1)))
  testing.assert_aborts("expected: err(_)", () => testing.assert_err((ok(0): Result[Int, String])))
}
```

两file删除 `// wasm:skip`。`proofs/wasm-fallback-baseline.txt`删两条 `HARNESS_LIMITATION`（ratchet降2），`tests/wasm_skip_ledger_test.rs`删 `NoUnwinding` entry。

删除 `spec/wasm_cross/testing_assert_abort_throws.almd`及六fixture台账（run/check/ast/alloc/size/gauntlet）对应行。

`testing.assert_aborts`是新stdlib函数，同PR走标准流程：
- `stdlib/testing.almd`；
- `self_host_registry.rs`（比较）及两个leg intrinsic；
- `docs/stdlib/testing.md`加 `almide check`例；
- `docs/stdlib/since.toml`；
- `proofs/stdlib-inspection.toml`行，按#3671在两leg检查success path；
- `proofs/target-availability.toml`，native/embedded host提供，stock runtime为D3 wall。

<a id="d8-contracts-als-first"></a>

### D8. 契约：Als先行

先改 **almide/als**，然后独立commit推进 `proofs/als-pin.txt`，再落地实现PR（als-pin顺序）。

1. **C-381**（ALS-T18）statement编辑：
   - assert列表删 `` or `assert_throws` ``（结尾为 `` … `assert_ok`, or `assert_err` ``）。
   - 删关系 `` `assert_throws(f, s)` -> `panic containing <s>` / found `returned normally` ``。
   - 删句 `` `assert_throws` outside a test block can only fail: a `panic` there is the C-219 abort, which nothing catches, so the thunk runs once and its panic aborts the program, or it returns and the block follows. ``。
   - 历史句删 `` , or refused `assert_throws` at build time (E081) ``，追加 `` `testing.assert_throws` was removed at dialect epoch 14 (ADR-0029); `testing.assert_aborts` replaces it (ALS-T30), and a call to it outside a test block is a check-time error, so it never reaches this desugar. ``。
   - 删evidence行 `spec/wasm_cross/testing_assert_abort_throws.almd`。
2. **ALS-T18**（`docs/specs/als/text-and-numbers.md`）：
   - 删条目 `` `testing.assert_throws(f, s)` → … ``。
   - 删以 `` `test` ブロック外の `panic` は C-219 の abort であり ``起始段，将最后句（作为函数值传入的调用…不在范围）移到上段末保留。
   - fixture列表删 `_throws`。
   - 再生成 `docs/contracts/conformance.md`。
3. **新节ALS-T30“testing.assert_aborts — 在子中运行的abort断言”**（als `origin/main` bc4ab695下一空T号）。规范性陈述D1–D5：
   - 仅closure在fresh program状态子process/instance运行；
   - capture与test外调用均check error；
   - 定义abort、abort文本及substring匹配；
   - closure内program-mode lowering；
   - 每call后父方继续；
   - failure block。
4. **新契约，下一空ID**（als `origin/main` bc4ab695为382，此处不写prefix以免citation gate当台账ID，ID共享故PR时复查）。拟议statement：

   > ALS-T30：test block内，`testing.assert_aborts(s, f)`仅运行 `f`，即编译时检查无capture的lambda，在fresh子process(native)或相同module新instance(wasm)运行。当且仅当子以定义abort（status1、stderr有 `Error: ` / `PANIC: `header）结束，且从最后header至stderr末文本含 `s`时通过；native `almide test`与 `--target wasm`相同。所有abort计入（`panic`、越界、除零、stack overflow、closure作为program本体的T18失败assert）。正常返回、其他status、raw panic、trap均以 `expected: abort containing <s>` / `found: …` block失败。每call后test继续，多call各自获得child。

   证据（test lane两leg运行的fixture class，各有 `// @contract:` header指名契约）：
   - `spec/stdlib/coverage_assert_aborts_test.almd`；
   - `spec/stdlib/testing_polarity_test.almd`；
   - `spec/lang/assert_aborts_continues_test.almd`：运行两次 `assert_aborts`，再断言仍运行。

   失败case（D5 `found:`变体）及E098/E099拒绝由Rust harness `tests/assert_aborts_runner_test.rs`两leg固定。
5. **als `ref/src/stdlib_testing.rs:22`**：`"testing.assert_throws"`改 `"testing.assert_aborts"`，参考evaluator采用child语义（每call fresh状态）。
6. **als spec file**：对 `spec/stdlib/coverage_assert_throws_test.almd`、`spec/stdlib/testing_polarity_test.almd`、`spec/wasm_cross/testing_assert_abort_throws.almd`作D7同修改。

als PR不改dialect epoch，epoch14属于本repo台账。

<a id="alternatives-considered"></a>

## 考虑的替代方案

- **A. Test级属性 `@aborts("…") test "…" { }`**（初稿，Rust `#[should_panic]`式）。Q15否决，owner不愿增加 `@`。且覆盖整个test，不能指定abort statement，是JUnit5/NUnit3放弃原因。每test仅一次abort。
- **B. Test-header modifier `test "…" aborts("…") { }`。** Q15否决；调查语言无此形式，还与 `where_clause*`竞争slot。
- **C. 保留 `assert_throws`进程内catch并让wasm unwind。** 否决，Almide目标wasm leg无unwind，native也仅捕 `panic`，裁定要求各leg全abort。
- **D. Serialize capture值传子来允许capture。** 否决，需closure/handle等每型wire格式，让子输入不同于source显示。Swift初版同理由禁capture。check错误有机械fix。
- **E. 在host/harness而非Almide比较。** 否决，`stdlib/testing.almd`比较让判定/失败block一次去糖、全leg继承（C-153），host只报 `(status, stderr)`。
- **F. 允许test外（`main`）调用。** 否决，program无报告harness，期待abort只是abort，也保持wasm import test-only。
- **G. 匹配全部子stderr。** 否决，前面的 `eprintln`期待文本会满足，test因无人检查的理由通过。

<a id="consequences"></a>

## 后果

- **两leg所有abort**同判定可测，每test可多次。
  - `testing`的 `HARNESS_LIMITATION`消失：少2个native-fallback file、少一wasm decline行（`host_ops.rs`）。
  - abort closure不再拖垮余下test/file；普通test中的意外abort仍会（`test_output.rs`），不在范围。
- 每call一次spawn或实例化（D3）。
- 新private embedded-host import `almide:test/abort-site`一个，仅test build。
- 新check诊断两个（`insp-all`编号E098/E099）。capture规则是模型除Swift外没见过的LLM规则，E098 hint与 `try:`承载修复。
- `stdlib-inspection.toml`替换一行：`assert_throws`退出，`assert_aborts`进入且需两leg stamp。

<a id="falsifier"></a>

## 可证伪条件

- owner运行Dojo测“测试函数以X abort”的首次正确率（CI不放API key）。若大量首次capture local，E098修复也不能令第二次达同等水平，则重审D2；下一候选为仅literal scalar的capture序列化D。
- 若 `assert_aborts` closure内定义abort的文本/status在leg间不同，D4 target无关失败。按abort契约I-divergence起issue，不在比较逻辑patch。
- 若每call实测成本D3令 `almide test spec/`慢过noise，则native批处理site。

<a id="open"></a>

## 未决项

2026-10-18全部○决定（各问题建议答案），依Swift6.2 exit test相关先例：任意本体含effect，子继承环境、global初值，匹配声明exit条件，capture输出仅用于报告。Q8为○，因为stock wasmtime guest无法启动新instance，所以embedded host提供wasm，stock lane声明wall。此前已裁定写法/参数顺序Q15、子process/新instance、无capture、全abort、epoch14无窗口删除及test外“删除”。`removed_spellings.rs`第三形式是D6实现选择。

1. closure类型是否为丢弃值的 `() -> T`，而非仅 `() -> Unit`？
2. 是否允许effect closure（子fs/env/argv，与test同cwd/env/argv）？
3. 是否允许引用top-level `var`，在子中看到初值？
4. abort是否为exit1与 `Error: ` / `PANIC: `行，从最后一个此类header行匹配，令 `process.exit(n≠1)`、raw panic、trap均失败？
5. closure是否program本体运行，assert为T18、native `panic`打印 `PANIC: …`？
6. test外调用（`fn`、test调用helper、`main`）是否check error？
7. 一等使用及closure内嵌 `assert_aborts`是否check error？
8. stock-wasmtime runner使用file是否声明wall并native fallback，private `almide:test/abort-site`仅embedded host提供？
9. 子stdout/stderr是否只在失败报告显示，pass时不转发？

<a id="references"></a>

## 参考资料

- Swift Testing ST-0008，exit test（Swift6.2已实现）: https://github.com/swiftlang/swift-evolution/blob/main/proposals/testing/0008-exit-tests.md （`expression`不在父进程调用，不可capture父进程或外层词法context状态）
- Go，subprocess测试 (`BE_CRASHER`, `exec.Command(os.Args[0], "-test.run=TestCrasher")`): https://go.dev/talks/2014/testing.slide#23
- Rust Reference，`should_panic`属性: https://doc.rust-lang.org/reference/attributes/testing.html#the-should_panic-attribute （指定字符串须在panic消息内才通过）
- XCTest `XCTAssertThrowsError`: https://developer.apple.com/documentation/xctest/xctassertthrowserror(_:_:file:line:_:)
- Zig，期待panic的测试提案（open）: https://github.com/ziglang/zig/issues/1356
- pytest `raises(match=…)`, 用 `re.search()`匹配: https://docs.pytest.org/en/stable/reference/reference.html#pytest-raises
- Jest `.toThrow(string)`, 错误消息**包含**substring，必须用函数包装代码: https://jestjs.io/docs/expect#tothrowerror
- kotlin.test `assertFailsWith(message, block)`: https://kotlinlang.org/api/core/kotlin-test/kotlin.test/assert-fails-with.html
- JUnit5异常测试 (`assertThrows`): https://junit.org/junit5/docs/current/user-guide/#writing-tests-assertions
- ExUnit `assert_raise/3`, 消息为Regex或精确String: https://hexdocs.pm/ex_unit/ExUnit.Assertions.html#assert_raise/3
- gleeunit `should`: https://hexdocs.pm/gleeunit/gleeunit/should.html
- Roc测试与期待: https://www.roc-lang.org/tutorial#tests-and-expectations
