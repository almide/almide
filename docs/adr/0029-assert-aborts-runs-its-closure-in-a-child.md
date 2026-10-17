# ADR-0029: `testing.assert_aborts(expected, closure)` runs only its closure, in a child process or a fresh instance; the closure captures nothing, every abort counts, and `testing.assert_throws` leaves at dialect epoch 14

- **Status**: Proposed (draft, 2026-10-17).
  - The direction is the owner's ruling of 2026-10-17 (○): replace `testing.assert_throws(f, expected)` and land the change in dialect epoch 14.
  - The spelling is the owner's ruling Q15 (2026-10-17). It is a body-level call, `testing.assert_aborts("expected substring", () => { … })`, executed the way Swift 6.2's exit tests are: the runner runs *only the closure*, in a child process (native) or a fresh instance (wasm). The closure may capture nothing, and every abort counts.
  - Q15 rejected the first draft of this ADR (a test-level `@aborts("…")` attribute, then a test-header modifier). That draft is kept below as rejected alternatives.
  - This ADR fills in the rest: the runner, the capture check, the migration. The questions in [Open](#open) still need a ruling.
- **Date**: 2026-10-17
- **Scope**:
  - the surface and type of `testing.assert_aborts`;
  - the check-time rules for it: where it may be called, and that its closure captures nothing;
  - how the native test binary and the wasm test lane run a closure in isolation, and what the parent compares;
  - the removal of `testing.assert_throws`;
  - the contract and dialect-ledger changes.
- **Number**: ADR-0028 is taken on `fan-semantics` (fan selection names, not merged yet), so this is 0029.
- **Related**: [ADR-0020](./0020-http-serving-and-handler-state.md) §5.2 (E095: the served app may not capture a local, for the same reason — it runs in another instance), [ADR-0022](./0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md), [ADR-0025](./0025-wasm-process-is-a-private-host-capability.md) (a private embedded-host import), [ADR-0026](./0026-effect-categories-ride-the-transparent-fn-type.md), [ADR-0027](./0027-effect-functions-are-inspected-in-a-fixed-host-world.md) Open item 3 (superseded here), C-067 / C-153 / C-196 / C-219 / C-350 / C-381 / ALS-T6 / ALS-T18, #3747, `proofs/dialect-epochs.toml` epoch 14 (branch `remove-33`).

## Context

`testing.assert_throws(f, expected)` runs the thunk `f` and passes when `f` panics with a message containing `expected`. It works in only one place, a native test build, and even there it catches less than it seems to.

**Native.**
- `runtime/rs/src/testing.rs` wraps `f` in `std::panic::catch_unwind`.
- A `--test` build keeps `panic(msg)` as an unwinding Rust `panic!` (the `cfg!(test)` arm of `almide_panic!`, `crates/almide-codegen/src/lib.rs:414-422`), so a `panic` is caught.
- Every other abort goes through `almide_abort`, which does a once-guarded `process::exit(1)`. That covers index out of bounds (C-067), division by zero and overflow (ALS-T6), stack overflow (C-196) and the domain errors. `catch_unwind` cannot see an exit.
- So one of those aborts inside `assert_throws` ends the whole test binary. The test does not pass, and every later test in the file does not run either (`src/cli/test_output.rs`, `a_process_ending_abort_leaves_the_failing_test_unfinished`).

**Wasm.**
- Wasm cannot unwind. Every abort is a stderr line plus `proc_exit(1)` from inside the module.
- `crates/almide-mir/src/host_ops.rs:243` declines `testing.assert_throws` ("needs an unwinding catch the MIR rung has no form for").
- Both test files that use it carry `// wasm:skip`. They are listed in `proofs/wasm-fallback-baseline.txt` (`HARNESS_LIMITATION`) and `tests/wasm_skip_ledger_test.rs` (`NoUnwinding`), and they run only on the native fallback.

**Outside a test block.** A direct call is desugared to the T18 abort (C-381, `crates/almide-frontend/src/lower/calls_testing_assert.rs`). Nothing can catch there, so it can only fail.

So the function promises to catch an abort and check its message, and no leg keeps that promise for the general case. When an abort cannot be caught in-process, the only working answer is for the abort to end *another* process or instance, with the observer outside it. Swift 6.2 builds exactly this into its test library. The owner chose Swift's shape.

### Every current caller

| Where | What | Repo |
|---|---|---|
| `spec/stdlib/coverage_assert_throws_test.almd` | 2 tests: `panic("oh no")` / `"oh no"` and `panic("something went wrong")` / `"went wrong"`; `// wasm:skip` | almide, als |
| `spec/stdlib/testing_polarity_test.almd` | 1 test with **two** calls: `assert_none(some(1))` and `assert_err(ok(0))`; `// wasm:skip` | almide, als |
| `spec/wasm_cross/testing_assert_abort_throws.almd` | C-381 evidence: the call outside a test, which always fails | almide, als |
| `stdlib/testing.almd:10-11` | the `@intrinsic` declaration | almide |
| `runtime/rs/src/testing.rs:35-49` | `almide_rt_test_assert_throws` | almide |
| `crates/almide-frontend/src/lower/calls_testing_assert.rs` | the T18 desugar arm (`"assert_throws" => Some(2)`, thunk handling) | almide |
| `crates/almide-mir/src/host_ops.rs:243`, `crates/almide-mir/examples/classify_corpus_parts/classify_corpus_b.rs:103-139` | the wasm decline row and the corpus classifier | almide |
| `crates/almide-wasm/src/testing.rs:19` | a comment | almide |
| generated: `crates/almide-types/src/generated/embedded.rs`, `crates/almide-codegen/src/generated/{rust_runtime.rs,runtime_fn_modes.rs}` | regenerated from the above | almide |
| `proofs/stdlib-inspection.toml:11101`, `proofs/impl-count-ledger.toml:5938`, `proofs/target-availability.toml:126,280`, `proofs/wasm-fallback-baseline.txt:17,34,35` | ledger rows | almide |
| `tests/wasm_skip_ledger_test.rs:50,80`, `tests/wasm_dispatch_coverage_test.rs:28` | the skip ledger and a comment | almide |
| `crates/almide-spine/tests/golden/spec-check-manifest.txt:612`, `crates/almide-syntax/tests/golden/spec-ast-manifest.txt:612` | fixture manifests | almide |
| `docs/stdlib/testing.md:5-13,131`, `docs/stdlib/since.toml:941`, `docs/stdlib/semantics-manifest.toml:2351`, `docs/diagnostics/E081.md:29`, `docs/specs/als/text-and-numbers.md:301-309` (mirror) | docs | almide |
| `ref/src/stdlib_testing.rs:22` | the reference evaluator's testing list | als |
| `docs/contracts/contracts.toml` C-381, `docs/specs/als/text-and-numbers.md` ALS-T18, `docs/contracts/conformance.md:140` | contract and spec | als (normative), almide (mirror) |

`CHANGELOG.md:492` and `llms.txt:380` mention it only as history and stay as they are.

## Survey: how languages say "this code must abort with X"

| Language | Spelling | Level | Message match | Aborts that cannot be caught (`exit`, fatal error) |
|---|---|---|---|---|
| **Swift (Testing, 6.2)** | `await #expect(processExitsWith: .failure) { … }` | body, a closure | the exit condition; the child's output can be inspected | **child process**: "starts a new process with the same executable as the current process … `expression` is not called in the parent process". The closure "cannot capture any state originating in the parent process or from the enclosing lexical context" (compile-time). |
| Go | none built in. The idiom re-runs the test binary as a child: `exec.Command(os.Args[0], "-test.run=TestCrasher")` with `BE_CRASHER=1` | body, by hand | hand-written | **subprocess**, by hand |
| Rust | `#[test] #[should_panic(expected = "msg")]` | test (attribute) | "the given string must appear within the panic message", a substring | not covered: in-process unwinding only, so `panic = "abort"` or `process::exit` kills the harness |
| Swift (XCTest) | `XCTAssertThrowsError(expr)` | body | the error value | not covered (`fatalError`) |
| Zig | `try std.testing.expectError(error.X, f())` | body | the error-set tag | not covered: "Proposal: Add a way for a test to expect a panic" (ziglang/zig#1356) is **open** |
| pytest | `with pytest.raises(E, match=r"…"):` | body (context manager) | regex, `re.search` | not covered |
| Jest | `expect(() => f()).toThrow("msg")` | body, a closure | "error message **includes** the substring" | not covered |
| Kotlin (kotlin.test) | `assertFailsWith<T> { … }` | body, a closure | the exception type; the returned exception is inspected after | not covered |
| JUnit | 4: `@Test(expected = X.class)`; 5: `assertThrows(X.class, () -> …)` | 4: test; 5: body, a closure | the type | not covered |
| Elixir ExUnit | `assert_raise ArgumentError, "msg", fn -> … end` | body, a closure | "a Regex or an exact String" | `catch_exit` |
| Gleam (gleeunit) | none: `should.be_error` covers `Result`; a `panic` fails the test | — | — | — |
| Roc | none: `expect` only; a `crash` fails the test | — | — | — |

What the survey shows:

1. **The closure-taking assertion is the dominant spelling.** Jest, Kotlin, Elixir, JUnit 5 and Swift all use it, and pytest is the context-manager variant of the same idea. JUnit 4's test-level `@Test(expected=)` was dropped by JUnit 5 for `assertThrows`, and NUnit 3 dropped `[ExpectedException]` the same way. A test-level declaration cannot say *which statement* must fail, so a test passes when its setup throws. The closure names the statement.
2. **Everywhere except Swift and Go, that closure is caught in-process.** Almide has no in-process catch on any leg (Context).
3. **Swift's exit tests are the precedent that fits Almide exactly.** They keep the closure-taking shape, run only the closure in a child process, and forbid captures because the closure runs in another process. Go's `BE_CRASHER` pattern is the same mechanism written by hand.

## Decision

### D1. The surface: `testing.assert_aborts(expected: String, f: () -> T) -> Unit`

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

- **Arguments**: the expected substring first and the closure last, as Q15 ruled. This is the order of Elixir's `assert_raise(E, msg, fn)` and Kotlin's trailing `block`.
- **Type**: `[T] (String, () -> T) -> Unit`. The closure's value is discarded, so `() => xs[5]` type-checks without a `println` around it.
- **Effects**: the closure may be an effect closure (the transparent fn type, ADR-0026). It may read files or env in the child, which runs in the same working directory and environment as the test.
- **Import**: `testing` already requires `import testing`, and `assert_aborts` sits beside the other `testing.assert_*`.
- **Why it is writable**: it is the shape a model already writes for "this must fail" in five languages. The one Almide-specific rule, "no captures", is enforced at check time with a fix hint (D2) instead of failing at run time.

### D2. Check-time rules (one new check, two diagnostics)

All of these live in the checker. They reuse the closure-reach machinery that already enforces E095 (ADR-0020 §5.2: the app passed to `http.serve` may not capture a local, because it runs in another instance) in `crates/almide-frontend/src/concurrent_reach.rs` and `check/concurrent_reach_check.rs`. That gives `testing.assert_aborts`'s second argument a new slot kind.

1. **The closure captures nothing** — the next free code, E098 at `insp-all`; ids are shared, so re-check at implementation time. Message: `the closure passed to testing.assert_aborts captures \`xs\`, a local of the test`.
   - The closure must be a lambda literal. Locals, parameters, `where`-bindings of the test, and outer lambda parameters are captures.
   - It may reference top-level `fn`s, top-level `let`s, imported modules, and top-level `var`s. The child starts a fresh program, so a `var` holds its initializer value there (D3), the same state the test runner's per-test re-init gives.
   - A fn *value* that is not a lambda literal (`testing.assert_aborts("x", f)` with `f` a local) is also E098: its captures cannot be seen.
   - Hint: `the closure runs in a separate process (native) or instance (wasm), where the test's locals do not exist. Inline the value into the closure, or make it a top-level \`let\`.` The `try:` shows the closure with the captured local's binding moved inside it, when that binding is a literal.
2. **Where it may be called** — the next free code after that, E099 at `insp-all`. The call must be a direct call that sits *lexically inside a `test` block*, though it may be nested in `if`, `match`, `for` or a lambda there. These are errors:
   - a call in a `fn` (including a helper called from a test) or in `main`. Hint: `assert_aborts runs only in a test: move the call into a \`test "…" { }\` block`. A program that wants to abort just aborts.
   - a first-class use (`let check = testing.assert_aborts`).
   - an `assert_aborts` *inside* another `assert_aborts`'s closure, which would need a grandchild.

### D3. Execution: the runner runs only the closure, in a child

**Lowering (frontend, once for every leg).**
- The capture-free closure of each call site is lifted to a top-level fn `__almd_abort_site_<k>`. `k` is a per-program site index in declaration order, so it is stable between the parent and a child of the same build.
- The call becomes `testing.__abort_site_run(k)`. That is a per-leg intrinsic returning `(Int, String)`: the child's exit status and its stderr.
- The pass/fail comparison (D4) follows the call as ordinary Almide code in `stdlib/testing.almd`, so the comparison is written once and every leg inherits it.
- The lifted site fn is lowered as a **program body**, not a test body:
  - `assert*` and `testing.assert_*` calls in it take their T18 block form (C-153 / C-381);
  - `panic` takes C-219's `PANIC: <msg>` path on native too.

  This is what makes the abort text the same bytes on every leg (D4).

**Native** (the test binary is libtest; `src/cli/commands.rs::cmd_test`, `runtime/rs/src/testing.rs`):
- The codegen emits one hidden `#[test] #[ignore = "almide:abort-site"] fn __almd_abort_site_entry()`. It reads `ALMIDE_ABORT_SITE=<k>` and calls site `k`.
- `__abort_site_run(k)` spawns `std::env::current_exe()` with `--ignored --exact <path>::__almd_abort_site_entry --nocapture --test-threads=1`, and sets `ALMIDE_ABORT_SITE=k` and `ALMIDE_TEST_ABORT_PROBE=1`. It waits and returns `(status, stderr)`.
- `ALMIDE_TEST_ABORT_PROBE=1` flips `almide_panic!` from `cfg!(test)` (unwind) to the C-219 abort path. The flag is read at run time, so it reaches helper fns the closure calls.
- This is Go's `BE_CRASHER` pattern, done by the runtime instead of by hand. There is no second build, and the ordinary run skips the ignored entry.

**Wasm** (`src/cli/test_wasm.rs`, `crates/almide-driver/src/test_runner.rs::synthesize`, `crates/almide-wasm-run`):
- The synthesized runner `main` gains one selector. When argv carries `--almide-abort-site=<k>`, it runs the global init and then site `k` only. Otherwise it runs the tests as now.
- `__abort_site_run(k)` lowers to a private embedded-host import, `almide:test/abort-site`, in the manner of ADR-0025's `almide:process/spawn`. The host instantiates **the same compiled module** fresh, with that argv, runs it to completion, and returns `(status, stderr)` to the parent instance. The parent instance then continues.
- The import exists only in test builds and only on the embedded host, which is the default `WasmTestRunner`.
- Under `ALMIDE_TEST_WASM_RUNNER=wasmtime` (stock runtime), a file that uses `assert_aborts` is a declared wall that falls back to native, with the reason printed. It is never a silent decline.
- The `host_ops.rs:243` decline row leaves with `assert_throws`.

**Interp** (the 3-way oracle): it runs the lifted site fn on a fresh interpreter state and reports the abort text. Where it abstains on native aborts today (`docs/specs/als/expressions.md`, ruling 3), it abstains here.

**Several calls in one test**: each call runs its own child, one after another, and the parent test continues after each. A test can therefore check several aborts, which the old two-call polarity test needs. A failing `assert_aborts` fails its test the way any other `testing.assert_*` does inside a test block (the harness reporting form). The rest of the file runs as usual.

**What the child sees**: a fresh program with top-level lets and vars at their initial values, the same cwd, env and argv as the test, and none of the parent's state. The child's stdout and stderr are captured and not forwarded. They appear only in the failure report (D5).

**Cost**: one process spawn (native) or one instantiation of an already-compiled module (wasm) per call. After the migration there are 4 calls in spec (D7). This is an estimate; the implementation PR measures `almide test spec/stdlib/` wall time before and after.

### D4. What counts as an abort, and what is matched

`testing.assert_aborts(s, f)` passes iff the child ended in a *defined abort* whose *abort text* contains `s` (byte substring, case-sensitive).

- **Defined abort**: the child's exit status is **1** and its stderr has an abort header, a line starting with `Error: ` or `PANIC: `. Every abort the contracts define has exactly this form:
  - `PANIC: <msg>` (C-219);
  - `Error: index out of bounds` (C-067);
  - `Error: division by zero` / `Error: integer overflow` and the rest of ALS-T6;
  - `Error: stack overflow` (C-196);
  - `Error: exit code must be in 0..=255` (C-350) and the other ALS-R1 domain errors;
  - the `Error: assertion failed` block (C-153 / C-381 / C-339).
- **Abort text**: the child's stderr from the **last** header line to the end. An earlier `eprintln` in the closure cannot satisfy the match.
- **Target-independent**: the abort contracts pin these bytes identically on native and wasm, and D3 makes the closure run in program mode, so the verdict does not depend on the target.
- **Not an abort** (each one fails the assertion):
  - the closure returned (exit 0);
  - any other exit status: `process.exit(n)` with `n ≠ 1`, a raw Rust panic (101, itself a C-153-class defect), a wasm trap (134, an undefined abort and therefore an I-divergence), a signal;
  - exit 1 with no header.
- **The one false positive left**: a closure that prints `Error: …` itself and then calls `process.exit(1)`, which deliberately imitates an abort. The T18 desugar is itself `eprintln` + `process.exit(1)`, so ruling this out would need an abort side channel that reroutes the desugar too. That is not worth it.

### D5. The failure report: an expected/found block, like T18

The comparison in `stdlib/testing.almd` fails with the C-153 field order:

```
Error: assertion failed
  at: line 7
  expected: abort containing "index out of bounds"
  found: returned normally
```

`found:` is one of:
- `returned normally`
- `abort "<abort text>"` (it aborted, but the text did not match)
- `exit code <n>`
- `exit code 101 (raw panic)`
- `trap (exit 134)`

Then come `child stdout:` / `child stderr:` sections when the child printed anything.

Inside a test block this rides the harness reporting form, exactly as the other `testing.assert_*` do. `at:` is the `assert_aborts` call's line.

### D6. Removal: `testing.assert_throws` leaves at dialect epoch 14, with no deprecation window (ruled)

The epoch-14 `breaks` entry, appended on `remove-33`:

```
"`testing.assert_throws(f, expected)` → rewrite as `testing.assert_aborts(expected, f)`: the expected text comes first, and `f` must be a lambda that captures nothing (it runs in a child process or instance); every abort counts — `panic`, index out of bounds, division by zero, a failed assert — not only `panic`. Outside a test block there is nothing to rewrite to: the call could only fail, so delete it",
```

**The diagnostic.** `crates/almide-frontend/src/removed_spellings.rs` (on `remove-33`) reads a removal's repair from `breaks`, but its `parse_break` recognises only two shapes: `→ \`import x\` + \`x.f(…)\` (package …)` and `→ delete the call`. Read as is, the entry above would be prose, and a call would get a bare unknown-function E002 with no repair.

The implementation therefore adds a third fixed shape, `→ rewrite as \`m.g(…)\` …`, parsed into `Fate::Rewritten { to, repair }`, and extends the `parse_break` tests in the same PR. A call then gets:

```
error[E002]: `testing.assert_throws` was removed at dialect epoch 14
  hint: rewrite as testing.assert_aborts(expected, f) — the expected text first;
        f runs in a child and must capture nothing; every abort counts, not only panic
  try: testing.assert_aborts("oh no", () => panic("oh no"))
```

The `try:` is built from the call's own arguments, swapped. A call outside a test block gets the "delete it" half of the repair instead.

### D7. Migration of the callers (almide and als, the same text)

`spec/stdlib/coverage_assert_throws_test.almd` becomes `spec/stdlib/coverage_assert_aborts_test.almd`:

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

`spec/stdlib/testing_polarity_test.almd` keeps its one test with two calls. Under D3 both match their T18 block text:

```almide
test "wrong polarity fails" {
  testing.assert_aborts("expected: none", () => testing.assert_none(some(1)))
  testing.assert_aborts("expected: err(_)", () => testing.assert_err((ok(0): Result[Int, String])))
}
```

Both files drop `// wasm:skip`. `proofs/wasm-fallback-baseline.txt` loses its two `HARNESS_LIMITATION` lines (the ratchet goes down by 2), and `tests/wasm_skip_ledger_test.rs` loses its `NoUnwinding` entries.

`spec/wasm_cross/testing_assert_abort_throws.almd` is deleted, together with its rows in the six fixture ledgers (run/check/ast/alloc/size/gauntlet).

`testing.assert_aborts` is a new stdlib function, and it goes through the stdlib routine in the same PR:
- `stdlib/testing.almd`;
- `self_host_registry.rs` (the comparison), plus the two per-leg intrinsics;
- `docs/stdlib/testing.md` with an `almide check` example;
- `docs/stdlib/since.toml`;
- a `proofs/stdlib-inspection.toml` row, inspected on both legs on its success path, per #3671;
- `proofs/target-availability.toml`: served by native and the embedded host; a stock runtime walls it, as in D3.

### D8. Contracts: als first

These change in **almide/als first**. Then `proofs/als-pin.txt` advances in its own commit, and then the implementation PR lands (the als-pin order).

1. **C-381** (ALS-T18), statement edits:
   - In the list of asserts, delete `` or `assert_throws` `` (the list ends `` … `assert_ok`, or `assert_err` ``).
   - Delete the relation `` `assert_throws(f, s)` -> `panic containing <s>` / found `returned normally` ``.
   - Delete the sentence `` `assert_throws` outside a test block can only fail: a `panic` there is the C-219 abort, which nothing catches, so the thunk runs once and its panic aborts the program, or it returns and the block follows. ``
   - In the history sentence, delete `` , or refused `assert_throws` at build time (E081) `` and append: `` `testing.assert_throws` was removed at dialect epoch 14 (ADR-0029); `testing.assert_aborts` replaces it (ALS-T30), and a call to it outside a test block is a check-time error, so it never reaches this desugar. ``
   - Remove the evidence row `spec/wasm_cross/testing_assert_abort_throws.almd`.
2. **ALS-T18** (`docs/specs/als/text-and-numbers.md`):
   - Delete the bullet `` `testing.assert_throws(f, s)` → … ``.
   - Delete the paragraph that starts `` `test` ブロック外の `panic` は C-219 の abort であり ``. Keep its last sentence (関数値として渡した呼び出しは…対象外) by moving it to the end of the previous paragraph.
   - Drop `_throws` from the fixture list.
   - Regenerate `docs/contracts/conformance.md`.
3. **New section ALS-T30, 「testing.assert_aborts — 子で走る abort 表明」** (the next free T number at als `origin/main` bc4ab695). It states D1 to D5 normatively:
   - the closure runs in a child process or instance, only the closure, with fresh program state;
   - captures are a check-time error, and so is any call outside a test block;
   - what a defined abort is, the abort text, and substring matching;
   - program-mode lowering inside the closure;
   - the parent continues after each call;
   - the failure block.
4. **New contract, the next free id** (382 at als `origin/main` bc4ab695, written here without its prefix so the citation gate does not read it as a ledger id; ids are a shared resource, so re-check at PR time). Proposed statement:

   > ALS-T30: inside a test block, `testing.assert_aborts(s, f)` runs only `f` — a lambda that captures nothing, checked at compile time — in a fresh child process (native) or a fresh instance of the same module (wasm), and passes iff that child ends in a defined abort (exit status 1 with an `Error: ` / `PANIC: ` header on stderr) whose text, from the last header to the end of stderr, contains `s`, identically under `almide test` on native and on `--target wasm`. Every abort counts (`panic`, index out of bounds, division by zero, stack overflow, a failed assert in its T18 block form, which the closure takes because it runs as a program body). Returning normally, any other exit status, a raw panic or a trap fails it with an `expected: abort containing <s>` / `found: …` block. The test continues after each call, and several calls in one test each get their own child.

   Evidence (fixture class, run on both legs by the test lane, each with a `// @contract:` header naming it):
   - `spec/stdlib/coverage_assert_aborts_test.almd`;
   - `spec/stdlib/testing_polarity_test.almd`;
   - `spec/lang/assert_aborts_continues_test.almd`: a test that runs two `assert_aborts`, then asserts it is still running.

   The failing cases (D5's `found:` variants) and the E098/E099 rejections are pinned by a Rust harness test, `tests/assert_aborts_runner_test.rs`, on both legs.
5. **als `ref/src/stdlib_testing.rs:22`**: replace `"testing.assert_throws"` with `"testing.assert_aborts"`, giving the reference evaluator the child semantics (a fresh evaluator state per call).
6. **als spec files**: the same D7 edits to `spec/stdlib/coverage_assert_throws_test.almd`, `spec/stdlib/testing_polarity_test.almd` and `spec/wasm_cross/testing_assert_abort_throws.almd`.

The als PR does not touch dialect epochs. Epoch 14 is this repo's ledger.

## Alternatives considered

- **A. A test-level attribute, `@aborts("…") test "…" { }`** (the first draft of this ADR, Rust's `#[should_panic]` shape). Rejected by Q15: the owner does not want more `@` attributes. It also covers the whole test, so it cannot say which statement must abort; that is the reason JUnit 5 and NUnit 3 left the form. And it allows only one abort per test.
- **B. A test-header modifier, `test "…" aborts("…") { }`.** Rejected by Q15. No surveyed language has it, and it competes with `where_clause*` for the same slot.
- **C. Keep `assert_throws`'s in-process catch and make wasm unwind.** Rejected. Wasm has no unwinding on any leg Almide targets, and native catches only `panic`. The ruling requires every abort to count on every leg.
- **D. Allow captures by serialising the captured values into the child.** Rejected. It would need a wire format for every type, including closures and handles, and it would make the child's input differ from what the source shows. Swift rejected it for the same reason (captures were forbidden in its first version). The check-time error has a mechanical fix.
- **E. Run the comparison in the host or harness instead of in Almide.** Rejected. Comparing in `stdlib/testing.almd` keeps the verdict and the failure block in one desugar that every leg inherits (the C-153 "desugar once" pattern). The host only reports `(status, stderr)`.
- **F. Allow calls outside test blocks (in `main`).** Rejected. A program has no harness to report to, and an expected abort in a program is just an abort. It also keeps the wasm import test-only.
- **G. Match all of the child's stderr.** Rejected. An earlier `eprintln` of the expected text would satisfy it, and the test would pass for a reason nobody checked.

## Consequences

- An expected abort is testable on **both** legs for **every** abort, with the same verdict, and several per test.
  - The `HARNESS_LIMITATION` reason for `testing` is gone: 2 fewer native-fallback files and one fewer wasm decline row (`host_ops.rs`).
  - An aborting closure never takes the rest of the test or the file down. An *unexpected* abort in an ordinary test still does (`test_output.rs`); that is out of scope here.
- The cost is one spawn or instantiation per call (D3).
- There is one new private embedded-host import (`almide:test/abort-site`), test builds only.
- There are two new check-time diagnostics (E098/E099 as numbered at `insp-all`). The capture rule is the one LLM-facing rule a model has not seen elsewhere except in Swift. E098's hint and `try:` carry the fix.
- `stdlib-inspection.toml` swaps one row: `assert_throws` leaves, and `assert_aborts` arrives and needs a both-leg stamp.

## Falsifier

- A Dojo measurement (owner-run; no API keys in CI) of first-try correctness on "test that this function aborts with X". If models capture a local in a large share of first attempts and E098's repair does not bring the second attempt to parity, revisit D2. The capture-serialising alternative D, limited to literal scalars, would be the next candidate.
- If a defined abort turns out to differ in text or status between legs inside an `assert_aborts` closure, D4's target-independence fails. That is filed as an I-divergence against the abort contract, not patched in the comparison.
- If the measured per-call cost (D3) makes `almide test spec/` measurably slower than noise, the native leg batches sites.

## Open

Each needs a ○× ruling. Already ruled: the spelling and argument order (Q15), the child-process / fresh-instance execution, no captures, every abort counting, and the epoch-14 removal with no deprecation window and "delete" outside a test. The `removed_spellings.rs` third shape is an implementation choice, done in D6.

1. Is the closure's type `() -> T` with the value discarded, rather than `() -> Unit` only?
2. May the closure be an effect closure (fs/env/argv in the child, same cwd/env/argv as the test)?
3. May the closure reference top-level `var`s, seeing their initial values in the child?
4. Does an abort mean exit 1 plus an `Error: ` / `PANIC: ` line, matched from the last such line, so `process.exit(n≠1)`, a raw panic and a trap all fail the assertion?
5. Does the closure run as a program body, so its asserts abort in T18 form and native `panic` prints `PANIC: …` there?
6. Is a call outside a test block (in a `fn` or helper called from a test, or in `main`) a check-time error?
7. Are a first-class use and a nested `assert_aborts` inside the closure check-time errors?
8. Under the stock-wasmtime test runner, does a file using it fall back to native as a declared wall, the private `almide:test/abort-site` import being served only by the embedded host?
9. Are the child's stdout/stderr shown only in the failure report, never forwarded on a pass?

## References

- Swift Testing ST-0008, exit tests (implemented in Swift 6.2): https://github.com/swiftlang/swift-evolution/blob/main/proposals/testing/0008-exit-tests.md ("`expression` is not called in the parent process"; "cannot capture any state originating in the parent process or from the enclosing lexical context")
- Go, subprocess tests (`BE_CRASHER`, `exec.Command(os.Args[0], "-test.run=TestCrasher")`): https://go.dev/talks/2014/testing.slide#23
- Rust Reference, the `should_panic` attribute: https://doc.rust-lang.org/reference/attributes/testing.html#the-should_panic-attribute ("the given string must appear within the panic message for the test to pass")
- XCTest `XCTAssertThrowsError`: https://developer.apple.com/documentation/xctest/xctassertthrowserror(_:_:file:line:_:)
- Zig, "Proposal: Add a way for a test to expect a panic" (open): https://github.com/ziglang/zig/issues/1356
- pytest `raises(match=…)`, matched "using `re.search()`": https://docs.pytest.org/en/stable/reference/reference.html#pytest-raises
- Jest `.toThrow(string)`, "error message **includes** the substring"; "You must wrap the code in a function": https://jestjs.io/docs/expect#tothrowerror
- kotlin.test `assertFailsWith(message, block)`: https://kotlinlang.org/api/core/kotlin-test/kotlin.test/assert-fails-with.html
- JUnit 5 exception testing (`assertThrows`): https://junit.org/junit5/docs/current/user-guide/#writing-tests-assertions
- ExUnit `assert_raise/3`, message "a Regex or an exact String": https://hexdocs.pm/ex_unit/ExUnit.Assertions.html#assert_raise/3
- gleeunit `should`: https://hexdocs.pm/gleeunit/gleeunit/should.html
- Roc tests and expectations: https://www.roc-lang.org/tutorial#tests-and-expectations
