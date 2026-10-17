# ADR-0029: A test declares the abort it ends in — `@aborts("…")` above `test`, one process or instance per such test, and `testing.assert_throws` leaves at dialect epoch 14

- **Status**: Proposed (draft, 2026-10-17). The direction is the owner's ruling of 2026-10-17 (○): replace `testing.assert_throws(f, expected)` with a test-level declaration in the shape of Rust's `#[should_panic(expected = "...")]`, the test run in its own process (native) or instance (wasm), every abort counting on every leg, landing in dialect epoch 14. This ADR proposes the spelling, the runner semantics and the migration. The questions in [Open](#open) still need a ruling.
- **Date**: 2026-10-17
- **Scope**: the surface spelling of an expected-abort test; what `almide test` (native and `--target wasm`) does with it; removing `testing.assert_throws`; the contract and dialect-ledger changes.
- **Number**: 0028 is taken by `fan-semantics` (ADR-0028, fan selection names), which is not merged yet, so this is 0029.
- **Related**: [ADR-0022](./0022-output-and-abort-builtins-are-admissible-in-a-pure-fn.md) (abort builtins), [ADR-0027](./0027-effect-functions-are-inspected-in-a-fixed-host-world.md) Open item 3 (`assert_throws` on wasm, which this ADR supersedes), C-067 / C-153 / C-196 / C-219 / C-350 / C-381 / ALS-T6 / ALS-T18, #3747, `proofs/dialect-epochs.toml` epoch 14 (branch `remove-33`).

## Context

`testing.assert_throws(f, expected)` runs the thunk `f` and passes when `f` panics with a message that contains `expected`. It works in exactly one place: a native test build. Even there it catches less than it seems to.

**Native.** `runtime/rs/src/testing.rs` wraps `f` in `std::panic::catch_unwind`. A `--test` build keeps `panic(msg)` as an unwinding Rust `panic!` (`almide_panic!`'s `cfg!(test)` arm, `crates/almide-codegen/src/lib.rs:414-422`), so a `panic` is caught. Every other abort goes through `almide_abort`, which does a once-guarded `process::exit(1)`: index out of bounds (C-067), division by zero and overflow (ALS-T6), stack overflow (C-196), the domain errors. `catch_unwind` cannot see an exit. One of these aborts inside `assert_throws` ends the whole test binary. The test does not pass, and every later test in the file does not run either (`src/cli/test_output.rs`, `a_process_ending_abort_leaves_the_failing_test_unfinished`).

**Wasm.** Wasm cannot unwind. Every abort is a stderr line plus `proc_exit(1)` from inside the module. `crates/almide-mir/src/host_ops.rs:243` declines `testing.assert_throws` ("needs an unwinding catch the MIR rung has no form for"). Both files that use it carry `// wasm:skip`. They are listed as `HARNESS_LIMITATION` in `proofs/wasm-fallback-baseline.txt` and as `NoUnwinding` in `tests/wasm_skip_ledger_test.rs`, so their tests run only on the native fallback.

**Outside a test block.** A direct call is desugared to the T18 abort (C-381, `crates/almide-frontend/src/lower/calls_testing_assert.rs`). Nothing can catch there, so it can only fail.

So the function promises "catch an abort and check its message", and no leg keeps that promise for the general case. The owner's ruling resolves the gap the only way that works when an abort cannot be caught in-process: the abort ends the test's own process or instance, and the runner, which lives outside it, reads the result.

### Every current caller

| Where | What | Repo |
|---|---|---|
| `spec/stdlib/coverage_assert_throws_test.almd` | 2 tests (`panic("oh no")` / `"oh no"`, `panic("something went wrong")` / `"went wrong"`), `// wasm:skip` | almide, als |
| `spec/stdlib/testing_polarity_test.almd` | 1 test with **two** calls (`assert_none(some(1))`, `assert_err(ok(0))`), `// wasm:skip` | almide, als |
| `spec/wasm_cross/testing_assert_abort_throws.almd` | C-381 evidence: the call outside a test, which always fails | almide, als |
| `stdlib/testing.almd:10-11` | the `@intrinsic` declaration | almide |
| `runtime/rs/src/testing.rs:35-49` | `almide_rt_test_assert_throws` | almide |
| `crates/almide-frontend/src/lower/calls_testing_assert.rs` | T18 desugar arm (`"assert_throws" => Some(2)`, thunk handling) | almide |
| `crates/almide-mir/src/host_ops.rs:243`, `crates/almide-mir/examples/classify_corpus_parts/classify_corpus_b.rs:103-139` | wasm decline row, corpus classifier | almide |
| `crates/almide-wasm/src/testing.rs:19` | comment | almide |
| generated: `crates/almide-types/src/generated/embedded.rs`, `crates/almide-codegen/src/generated/{rust_runtime.rs,runtime_fn_modes.rs}` | regenerated from the above | almide |
| `proofs/stdlib-inspection.toml:11101`, `proofs/impl-count-ledger.toml:5938`, `proofs/target-availability.toml:126,280`, `proofs/wasm-fallback-baseline.txt:17,34,35` | ledger rows | almide |
| `tests/wasm_skip_ledger_test.rs:50,80`, `tests/wasm_dispatch_coverage_test.rs:28` | skip ledger / comment | almide |
| `crates/almide-spine/tests/golden/spec-check-manifest.txt:612`, `crates/almide-syntax/tests/golden/spec-ast-manifest.txt:612` | fixture manifests | almide |
| `docs/stdlib/testing.md:5-13,131`, `docs/stdlib/since.toml:941`, `docs/stdlib/semantics-manifest.toml:2351`, `docs/diagnostics/E081.md:29`, `docs/specs/als/text-and-numbers.md:301-309` (mirror), `docs/CHEATSHEET.md:871` (testing count 7) | docs | almide |
| `ref/src/stdlib_testing.rs:22` | reference evaluator's testing list | als |
| `docs/contracts/contracts.toml` C-381, `docs/specs/als/text-and-numbers.md` ALS-T18, `docs/contracts/conformance.md:140` | contract and spec | als (normative), almide (mirror) |

`CHANGELOG.md:492` and `llms.txt:380` mention it only as history and stay as they are. The other repos found by `almide-downstream-syntax-sweep` are swept when the implementation lands, not by this ADR.

## Survey: how languages say "this test must abort with X"

| Language | Spelling | Level | Message match | Uncatchable aborts (`exit`, fatal error) |
|---|---|---|---|---|
| Rust | `#[test] #[should_panic(expected = "msg")]` (also `#[should_panic = "msg"]`, bare `#[should_panic]`) | **test** (attribute) | "the given string must appear within the panic message", i.e. a substring | not covered: in-process unwinding only, so `panic = "abort"` or `process::exit` kills the harness |
| Go | none built in. Idiom: `defer func(){ if recover() == nil { t.Error(..) } }()`; for `os.Exit`, re-run the test binary as a child: `exec.Command(os.Args[0], "-test.run=TestCrasher")` with `BE_CRASHER=1` | body (idiom) | hand-written | **subprocess**, by hand |
| Swift (XCTest) | `XCTAssertThrowsError(expr)` | body | the error value, via a closure | not covered (`fatalError` cannot be caught) |
| Swift (Testing, 6.2) | `await #expect(processExitsWith: .failure) { … }` | body (macro + closure) | exit condition; output can be inspected | **child process**: "starts a new process with the same executable … `expression` is not called in the parent process". The closure "cannot capture any state originating in the parent process or from the enclosing lexical context." |
| Zig | `try std.testing.expectError(error.X, f())` | body | error-set tag | not covered: "Proposal: Add a way for a test to expect a panic" (ziglang/zig#1356) is **open** |
| pytest | `with pytest.raises(E, match=r"…"):` | body (context manager) | regex, `re.search` (substring-like) | not covered (`os._exit`) |
| Jest | `expect(() => f()).toThrow("msg")` | body (closure) | "error message **includes** the substring" | not covered |
| Kotlin (kotlin.test) | `assertFailsWith<T> { … }` | body (closure) | type, then read the returned exception | not covered |
| JUnit | 4: `@Test(expected = X.class)`; 5: `assertThrows(X.class, () -> …)` | 4: **test**; 5: body | type; message checked separately | not covered |
| Elixir ExUnit | `assert_raise ArgumentError, "msg", fn -> … end` | body (fn) | "a Regex or an exact String" | `catch_exit` for exits |
| Gleam (gleeunit) | none: `should.be_error` covers `Result`; a `panic` fails the test | — | — | — |
| Roc | none: `expect` only; a `crash` fails the test | — | — | — |

What the survey shows:

1. **Exceptions are caught in-process, with a closure in the body.** The closure-based forms (Jest, pytest, Kotlin, Elixir, JUnit 5, XCTest) all depend on a catch that runs in the same process: an exception or an unwinding panic. Almide has no such catch on any leg. Every abort is a process or instance exit (ALS-T6, C-219, C-196, C-350), and the one native exception, the `cfg!(test)` unwinding `panic`, is exactly the asymmetry this ruling removes.
2. **Where the abort cannot be caught, the answer is a child process.** Go does it by hand, Swift 6.2 builds it in, and Zig and Roc offer nothing yet. Swift shows what a body-level closure costs here: the closure moves to another process, so it may not capture anything. In Almide that would be a new capture rule on a lambda that looks like any other lambda.
3. **Only one precedent is test-level, and it is the closest to this ruling.** Rust's `#[should_panic(expected = …)]` is an attribute on the test, checks the message by substring, and covers the whole test body. It is also among the most-written test idioms in a model's Rust training data. JUnit 4's `@Test(expected=)` was dropped by JUnit 5 for `assertThrows`, and NUnit 3 dropped `[ExpectedException]` the same way. Their stated reason: a test-level declaration cannot say *which statement* must throw, so a test can pass because setup threw. That cost is real, and D2 bounds it. In those languages the catch exists, so the closure form was available to them. In Almide it is not.

## Decision

### D1. Spelling: an `@aborts("…")` attribute on the line above `test`

```almide
@aborts("index out of bounds")
test "reading past the end aborts" {
  let xs = [1, 2, 3]
  println(xs[5])
}
```

- Grammar (`docs/GRAMMAR.md`): `test_decl = test_attr? "test" STRING where_clause* block` and `test_attr = "@aborts" "(" STRING ")"`. The attribute grammar (`attr = "@" IDENT ("(" attr_arg ("," attr_arg)* ")")?`) and `parse_attribute` exist already. `parse_keyword_top_decl` (`crates/almide-syntax/src/parser/declarations.rs:267`) sends every leading `@` to the fn parser today. It now looks past the collected attributes: if `test` follows, it calls `parse_test_decl` with them. The `@dialect` stamp is still claimed first, in `parser/entry.rs`.
- AST: `Decl::Test` gets `aborts: Option<String>`, which reaches `--emit-ast` (spec-ast manifest) and `almide fmt`. Fmt prints the attribute alone on the line above `test`.
- Exactly one argument, a positional string literal, **required**. `""` is the explicit "any abort". A bare `@aborts`, a non-string argument, a second argument, a named `expected:` argument, two `@aborts` on one test, or any other attribute on a `test` is a check-time error with a fix hint. `@aborts` on a `fn` is also an error.
- Near-miss attribute names that a model trained on other languages will write — `@should_panic`, `@should_abort`, `@expect_abort`, `@expected_abort`, `@panics` — get a "did you mean `@aborts(…)`" diagnostic with the rewritten line as `try:`. They are not accepted.
- No `import testing` is needed. This is a language form, not a library call.

Why this spelling (the mission metric is the spelling a model writes correctly the first time):

- **It is Rust's shape, one-to-one.** An attribute above the test, one string, substring semantics, the whole body covered. A model that has learned `#[should_panic(expected = "…")]` only has to change the punctuation, and Almide's `@name(...)` attribute is already in the cheatsheet (`@deprecated(since = 3, use = "...")`, `@dialect(3)`).
- **It names Almide's own concept.** Almide's word is *abort*. The cheatsheet glosses `panic(msg)` as "abort: message on stderr, exit 1", and the contracts say abort for index, division, stack and assert failures alike. `@should_panic` would teach that only `panic(…)` counts, which is exactly the misreading the ruling ("every abort counts") rules out.
- **It does not grow the test-header grammar.** The space between the test name and the body already holds `where` clauses (`where_clause*`). Putting a modifier there (alternative B) means deciding how it orders against `where`, and adds a contextual keyword no surveyed language has.
- **It cannot be mistaken for a call.** A body-level form (alternative C) looks like `testing.assert_throws`. That invites the closure spelling that every non-Rust precedent uses, and implies a scope the runtime cannot honour.

### D2. What counts as an abort, and what is matched

An `@aborts(s)` test **passes** iff its run ends in a *defined abort* whose *abort text* contains `s` (byte substring, case-sensitive).

- **Defined abort**: the test's process (native) or instance (wasm) ended **with exit status 1**, before the test returned, and its stderr contains an abort header: a line that starts with `Error: ` or `PANIC: `. Every abort the contracts define has exactly this form:
  - `PANIC: <msg>` (C-219, `panic`);
  - `Error: index out of bounds` (C-067);
  - `Error: division by zero` / `Error: integer overflow` and the rest of ALS-T6;
  - `Error: stack overflow` (C-196);
  - `Error: exit code must be in 0..=255` (C-350) and the other ALS-R1 domain errors;
  - the `Error: assertion failed` block (C-153 / C-381 / C-339).
- **Abort text**: stderr from the last abort-header line to the end. Only the abort's own text is matched, never what the test printed before it. An `eprintln` earlier in the test cannot satisfy the match.
- **The text is the same on every leg.** The abort contracts pin it byte-identically on native and wasm, so the verdict cannot depend on the target.
- **Not an abort** (each one fails the test, see D5):
  - the body returned normally;
  - the body failed through an `err` / `!` (a test failure, not an abort);
  - any other exit status: `process.exit(n)` with `n ≠ 1`, a raw Rust panic (exit 101, itself a C-153-class defect), a wasm trap (exit 134, an undefined abort and therefore an I-divergence), a signal or a timeout;
  - exit status 1 with no abort header.

### D3. An `@aborts` test runs its body as a program body

Inside an `@aborts` test, every abort takes its **program-mode** form, the one the contracts pin outside test blocks:

- `assert` / `assert_eq` / `assert_ne` / the `testing.assert_*` calls lower to the T18 block (C-153 / C-381). The frontend desugar treats the body of an `@aborts` test as "outside a test block" (`calls_testing_assert.rs`, `almide-driver/src/test_runner.rs::desugar_test_asserts`).
- On native, `panic` takes the C-219 `PANIC: <msg>` path, not the unwinding `panic!`. The check in `almide_panic!` changes from compile-time `cfg!(test)` to `cfg!(test) && !almide_abort_probe()`. `almide_abort_probe()` reads a once-initialised flag that the harness sets on the child (`ALMIDE_TEST_ABORT_PROBE=1`). The macro is used inside helper fns the test calls, so a per-fn codegen switch would not reach them.
- With all of this, an `@aborts` test's abort text is the same bytes the same code prints as a program. That is what makes D2's verdict target-independent: `@aborts("expected: none")` means the same thing on native, wasm and the interp.

### D4. Isolation: one process (native) or instance (wasm) per `@aborts` test; the rest of the file still runs

The ordinary run of a file **excludes** its `@aborts` tests. Each `@aborts` test then runs once, alone, in a fresh process or instance. One aborting test therefore never stops the others. Whatever the `--run` filter selects is what runs, and `where`-table cases expand first: each case of an `@aborts` table test is its own isolated run.

- **Native** (`src/cli/commands.rs::cmd_test` / `run_test_binaries_parallel`, `src/cli/test_report.rs::test_harness_args`):
  - The `test_block` template emits an `@aborts` test as `#[test] #[ignore = "almide:aborts"]`, so the ordinary libtest run (`--nocapture --test-threads=1`) skips it.
  - For each such test the harness re-runs **the same binary** as `<bin> --ignored --exact <emitted path> --nocapture --test-threads=1`, with `ALMIDE_TEST_ABORT_PROBE=1`. This is Go's subprocess-test pattern, done by the harness instead of by hand.
  - No second build.
- **Wasm** (`src/cli/test_wasm.rs::compile_and_run_wasm_test`, `crates/almide-driver/src/test_runner.rs::synthesize`, the embedded host in `crates/almide-wasm-run`):
  - The synthesized runner `main` leaves `@aborts` tests out of its sequence.
  - It gains one selector: when argv carries `--almide-abort-test=<i>`, it runs only the module re-init and test `i`.
  - The lane compiles the module **once** and instantiates it once for the ordinary run, plus once per `@aborts` test with that argv, through the same `WasmTestRunner`.
  - The renderer's decline of `testing.assert_throws` (`host_ops.rs:243`) goes away with the function. Nothing new is declined, because the selector is ordinary IR.
- **Interp leg (3-way oracle)**: runs the body as a program and reports the same abort text. Where it abstains on native aborts today (`docs/specs/als/expressions.md`, ruling 3), it abstains here too.
- **Cost**: one extra process spawn (native) or instantiation (wasm) per `@aborts` test, with no extra compile. Spec has 3 such tests after migration (D7), 4 runs counting the split. This is an estimate, not a measurement. The implementation PR measures `almide test spec/stdlib/` wall time before and after, and the number goes in the PR.

### D5. The report: an expected/found block, like T18

The runner (harness side, not program side) reports a failing `@aborts` test in the C-153 field order. `at:` is the `@aborts` line.

```
Error: assertion failed
  at: line 7
  expected: abort containing "index out of bounds"
  found: returned normally
```

`found:` is one of:
- `returned normally`
- `failed: <err display>`
- `abort "<abort text>"` (it aborted, but the text did not match)
- `exit code <n>`
- `exit code 101 (raw panic)`
- `trap (exit 134)`
- `timeout`

The relation text mirrors C-381's `panic containing <s>`, renamed to *abort* to match D2. Stdout the test wrote before ending is attributed to it as in #2538 (`src/cli/test_output.rs`). A passing `@aborts` test is silent unless `--show-output`. It counts as one test in `TestCounts` and in `--json`.

### D6. Removal: `testing.assert_throws` leaves at dialect epoch 14, with no deprecation window

The removal goes into epoch 14 (`proofs/dialect-epochs.toml`), which is being cut now for the 33 removed stdlib functions (branch `remove-33`). It matches their treatment: no `[[deprecation]]` window. The function never worked for the general case on any leg (Context), and the callers are three spec files.

**`breaks` entry** (appended to epoch 14's `breaks`):

```
"`testing.assert_throws(f, expected)` → rewrite as `@aborts(\"<expected>\")` on the line above `test \"…\"`, and put `f`'s body in the test body as plain statements; one expected abort per test, so a test with two `assert_throws` calls becomes two tests; every abort counts (panic, index out of bounds, division by zero, a failed assert), not only `panic`. Outside a test block there is nothing to rewrite to: the call could only fail, so delete it",
```

**The diagnostic.** `crates/almide-frontend/src/removed_spellings.rs` (on `remove-33`) reads a removal's repair from `breaks`, but its `parse_break` recognises only two fates: `→ \`import x\` + \`x.f(…)\` (package …)` and `→ delete the call`. The entry above matches neither, so it would be read as prose. The call would get a bare unknown-function E002 with no repair, which is exactly the silent miss the ledger-derived table exists to prevent.

So the implementation adds a third fixed shape: `→ rewrite as …`, which becomes `Fate::Rewritten { repair }`. The parse test is extended in the same PR. A call to `testing.assert_throws` anywhere then gets:

```
error[E002]: `testing.assert_throws` was removed at dialect epoch 14
  hint: declare the expected abort on the test instead —
          @aborts("oh no")
          test "…" { …the thunk's body… }
        every abort counts, not only `panic`; one expected abort per test
  try: @aborts("oh no")
```

`"oh no"` is the call's second argument when it is a string literal, and `"…"` otherwise. A call outside a test block gets the "delete it" half of the repair. The deletion leaves `stdlib/testing.almd` with 9 functions and its `@intrinsic`, and the rest of the table in Context moves in the same PR.

### D7. Migration of the callers (almide and als, the same text)

```almide
// spec/stdlib/coverage_test_aborts_test.almd  (replaces coverage_assert_throws_test.almd)
@aborts("oh no")
test "panic is an abort" { panic("oh no") }

@aborts("went wrong")
test "the expected text is a substring" { panic("something went wrong") }

@aborts("index out of bounds")
test "an index past the end is an abort" {
  let xs = [1]
  println(xs[3])
}

@aborts("division by zero")
test "division by zero is an abort" {
  let z = 0
  println(10 / z)
}
```

`spec/stdlib/testing_polarity_test.almd`: the test with two calls becomes two tests. Under D3 they match the T18 block text:

```almide
@aborts("expected: none")
test "assert_none on some fails" { testing.assert_none(some(1)) }

@aborts("expected: err(_)")
test "assert_err on ok fails" { testing.assert_err((ok(0): Result[Int, String])) }
```

Both files drop `// wasm:skip`. `proofs/wasm-fallback-baseline.txt` loses its two `HARNESS_LIMITATION` lines (the ratchet goes down by 2), and `tests/wasm_skip_ledger_test.rs` loses its `NoUnwinding` entries.

`spec/wasm_cross/testing_assert_abort_throws.almd` is deleted, together with its rows in the six fixture ledgers (run/check/ast/alloc/size/gauntlet).

### D8. Contracts: als first

These change in **almide/als first**. Then `proofs/als-pin.txt` advances in its own commit, and then the implementation PR lands (the als-pin order).

1. **C-381** (ALS-T18), statement edits:
   - In the list of asserts, delete `` or `assert_throws` `` (the list ends `` … `assert_ok`, or `assert_err` ``).
   - Delete the relation `` `assert_throws(f, s)` -> `panic containing <s>` / found `returned normally` ``.
   - Delete the sentence `` `assert_throws` outside a test block can only fail: a `panic` there is the C-219 abort, which nothing catches, so the thunk runs once and its panic aborts the program, or it returns and the block follows. ``
   - In the history sentence, replace `` , or refused `assert_throws` at build time (E081) `` with nothing, and append: `` `testing.assert_throws` was removed at dialect epoch 14 (ADR-0029); an expected abort is declared on the test with `@aborts` (the new ALS-T30 contract). ``
   - Remove the evidence row `spec/wasm_cross/testing_assert_abort_throws.almd`.
2. **ALS-T18** (`docs/specs/als/text-and-numbers.md`):
   - Delete the bullet `` `testing.assert_throws(f, s)` → … ``.
   - Delete the paragraph that starts `` `test` ブロック外の `panic` は C-219 の abort であり ``. Keep its last sentence (関数値として渡した呼び出しは…対象外) by moving it to the end of the previous paragraph.
   - Drop `_throws` from the fixture list.
   - Regenerate `docs/contracts/conformance.md`.
3. **ALS-DL1** (`docs/specs/als/expressions.md`), 受理形: `` `test "name" { … }` `` becomes `` `test "name" { … }` (任意で直前の行に `@aborts("…")`、ALS-T30) ``.
4. **New section ALS-T30, 「test の abort 宣言」** (next free T number at als `origin/main` bc4ab695). It states D2 to D5 normatively: what a defined abort is, the abort text, substring matching, program-mode lowering inside the test, isolation and the other tests still running, and the failure block.
5. **New contract, the next free id** (382 at als `origin/main` bc4ab695, written here without its prefix so the citation gate does not read it as a ledger id; re-check at PR time, since ids are a shared resource). Proposed statement:

   > ALS-T30: a test declared `@aborts(s)` passes iff its run ends in a defined abort (exit status 1 with an `Error: ` / `PANIC: ` header on stderr) whose text, from the last header to the end of stderr, contains `s`. This holds identically under `almide test` on native and on `--target wasm`. Every abort counts (`panic`, index out of bounds, division by zero, stack overflow, a failed assert in its T18 block form). Returning normally, failing with `err`, any other exit status, a raw panic, a trap or a timeout fails the test with an `expected: abort containing <s>` / `found: …` block. The test runs in its own process (native) or instance (wasm), so the file's other tests run whatever it does.

   Evidence (fixture class, run on both legs by the test lane, each with a `// @contract:` header naming it):
   - `spec/stdlib/coverage_test_aborts_test.almd`;
   - `spec/stdlib/testing_polarity_test.almd`;
   - `spec/lang/test_aborts_isolation_test.almd`: an `@aborts` test between two ordinary tests, which must still both run.

   The failing cases (D5's `found:` variants) are pinned by a Rust harness test, `tests/test_aborts_runner_test.rs`, that runs `almide test` on must-fail inputs on both legs.
6. **als `ref/src/stdlib_testing.rs:22`**: remove `"testing.assert_throws"`. The reference evaluator's parser accepts `@aborts` on `test`.
7. **als spec files**: the same D7 edits to `spec/stdlib/coverage_assert_throws_test.almd`, `spec/stdlib/testing_polarity_test.almd` and `spec/wasm_cross/testing_assert_abort_throws.almd`.

The als PR does **not** touch dialect epochs. Epoch 14 is this repo's ledger.

## Alternatives considered

- **B. A modifier between the name and the body: `test "…" aborts("index") { }`.** Rejected (D1). No surveyed language has this position. It needs a contextual keyword and an ordering rule against `where_clause*`. It also puts the expectation in the middle of the test header, where a model writing a table test (`where [ … ]`) is most likely to misplace it.
- **C1. A body-level statement: `test "…" { expect_abort("index"); … }`.** Rejected. It reads as a call that takes effect from where it stands, but the expectation covers the whole test. It is order-sensitive in a way the runner cannot check (a statement *after* the abort never runs), and it puts a magic call back in the body, the same shape as the function being removed.
- **C2. A body-level closure run in a child, in Swift 6.2's shape: `testing.expect_abort("index", () => …)`.** Rejected. It keeps the closure spelling every non-Rust precedent uses, which helps writability. But Swift shows the price: the closure cannot capture anything from the test, because it runs in another process. In Almide that would be a new capture rule on a lambda indistinguishable from any other, and on wasm it needs a function-pointer entry per call site. It is also the shape the ruling replaced.
- **`@should_panic("…")` as the canonical name.** Rejected, but kept as a diagnosed near-miss (D1). It matches Rust's spelling exactly, but in Almide's vocabulary a panic is one abort of many. A name that suggests only `panic(…)` counts would teach the opposite of the ruling.
- **Bare `@aborts` (any abort).** Rejected. A test that aborts for the wrong reason, for example a typo'd index in setup, would pass. JUnit's and NUnit's reason for leaving test-level expectations is exactly a test passing because the wrong statement threw. The required substring is the bound on that; `""` stays available and visible.
- **Matching all of stderr instead of the abort text.** Rejected. An `eprintln` of the expected string earlier in the test would satisfy it, so the test would pass for a reason nobody checked.
- **A side channel that marks abort sites, so `process.exit(1)` without a header can never count.** Not needed. D2's status-1-plus-header rule already excludes it. The only false positive left is a program that prints `Error: …` itself and then calls `process.exit(1)`, which deliberately imitates an abort. The T18 desugar is itself `eprintln` + `process.exit(1)`, so a channel would have to reroute that desugar too.
- **Keep `assert_throws` for `panic` only, native only.** Rejected by the ruling: an assertion that silently means less on one leg is the divergence class the contract ledger exists to prevent.

## Consequences

- An expected abort is testable on **both** legs for **every** abort, with the same verdict. The last `HARNESS_LIMITATION` reason for `testing` is gone: 2 fewer native-fallback files and one fewer wasm decline row (`host_ops.rs`).
- An aborting `@aborts` test no longer takes the rest of its file down. An *unexpected* abort in an ordinary test still does (`test_output.rs`); isolating those is out of scope here.
- Cost: one spawn or instantiation per `@aborts` test (D4).
- `stdlib-inspection.toml` loses one uninspected row (the stamp goal counts it as removed, not as inspected).
- A test can check at most one abort. The polarity file splits one test into two.
- Writability depends on the near-miss diagnostics (D1) and the removed-spelling repair (D6). Without them, a model that writes Jest's or pytest's closure form gets a bare E002.

## Falsifier

- A Dojo measurement (owner-run; no API keys in CI) of first-try correctness on "write a test that this function aborts with X", across the D1 spelling and alternatives B and C2. If B or C2 scores clearly higher than `@aborts` over enough tasks, D1 is revisited.
- If a defined abort turns out to differ in text or status between legs inside an `@aborts` test, D2's target-independence fails. That is filed as an I-divergence against the abort contract, not patched in the runner.
- If the measured isolation cost (D4) makes `almide test spec/` measurably slower than noise, the native leg batches `@aborts` tests per binary differently.

## Open

Each needs a ○× ruling before implementation:

1. Is the spelling `@aborts("…")` on the line above `test` (D1, not a modifier or a body form)?
2. Is the expected string required, with `""` as the explicit "any abort" (no bare `@aborts`)?
3. Are `@should_panic` / `@should_abort` / `@expect_abort` / `@panics` rejected with a "did you mean `@aborts`" fix, rather than accepted as aliases?
4. Does an abort mean exit status 1 plus an `Error: ` / `PANIC: ` header, with the match against the text from the last header (D2), so that `process.exit(n≠1)`, a raw panic and a trap all fail the test?
5. Is the body of an `@aborts` test lowered as a program body (D3), so its asserts abort in the T18 block form and native `panic` prints `PANIC: …` there?
6. Is a `where`-table test with `@aborts` allowed, with every case isolated and required to abort?
7. Does `testing.assert_throws` leave at epoch 14 with no deprecation window (D6)?
8. Is a direct `testing.assert_throws` call outside a test block (today's C-381 fixture) repaired by "delete the call", with the fixture deleted?
9. Does `removed_spellings.rs` gain a third fixed shape, `→ rewrite as …`, for this entry (D6)?

## References

- Rust Reference, the `should_panic` attribute: https://doc.rust-lang.org/reference/attributes/testing.html#the-should_panic-attribute ("the given string must appear within the panic message for the test to pass")
- Go, subprocess tests (`BE_CRASHER`, `exec.Command(os.Args[0], "-test.run=TestCrasher")`): https://go.dev/talks/2014/testing.slide#23
- Swift Testing ST-0008, exit tests (implemented in Swift 6.2): https://github.com/swiftlang/swift-evolution/blob/main/proposals/testing/0008-exit-tests.md ("`expression` is not called in the parent process"; "cannot capture any state originating in the parent process or from the enclosing lexical context")
- XCTest `XCTAssertThrowsError`: https://developer.apple.com/documentation/xctest/xctassertthrowserror(_:_:file:line:_:)
- Zig, "Proposal: Add a way for a test to expect a panic" (open): https://github.com/ziglang/zig/issues/1356
- pytest `raises(match=…)`, matched "using `re.search()`": https://docs.pytest.org/en/stable/reference/reference.html#pytest-raises
- Jest `.toThrow(string)`, "error message **includes** the substring": https://jestjs.io/docs/expect#tothrowerror
- kotlin.test `assertFailsWith`: https://kotlinlang.org/api/core/kotlin-test/kotlin.test/assert-fails-with.html
- JUnit 5 exception testing (`assertThrows`): https://junit.org/junit5/docs/current/user-guide/#writing-tests-assertions
- ExUnit `assert_raise/3`, message "a Regex or an exact String": https://hexdocs.pm/ex_unit/ExUnit.Assertions.html#assert_raise/3
- gleeunit `should`: https://hexdocs.pm/gleeunit/gleeunit/should.html
- Roc tests and expectations: https://www.roc-lang.org/tutorial#tests-and-expectations
