# ADR-0022: The output and abort builtins are admissible in a pure fn — writes and aborts, never reads, and they do not make the caller effectful

- **Status**: Accepted (records existing behaviour; no checker or codegen change)
- **Date**: 2026-09-27
- **Scope**: whether `println`, `eprintln`, `panic`, `assert`, `assert_eq` and
  `assert_ne` may be called from a plain `fn`; what that call means for the
  caller's effect, for propagation, for `fan`, for served handlers, for the
  reference evaluators, and for native vs wasm output buffering.
- **Related**: [effect-system.md §2.1](../specs/effect-system.md) (the normative
  rule), [ADR-0002](./0002-fallibility-effect-orthogonal.md) (the effect axis),
  [ADR-0008](./0008-explicit-propagation-only.md) (propagation is `!` only),
  [ADR-0011](./0011-execution-substrate-is-a-free-variable.md) D1 (fan arm
  output), [ADR-0015](./0015-reference-evaluator-is-fresh-source-level-python.md)
  (reference evaluator), [ADR-0020](./0020-http-serving-and-handler-state.md)
  §5.5 / §7.2 (concurrent serving and log lines), contracts C-004, C-153,
  C-162, C-367.
- **History**: 2026-09-27. The rule was never written down, so the
  docs taught both spellings of `main` (`effect fn main() -> Unit = println(…)` and
  `fn main() -> Unit = println(…)`) and a reader could not tell which was
  required. The user was offered three options: A (admit the output and abort
  builtins in a pure fn and write the rule down), B (require `effect` for
  them), and C (admit only aborts). The user chose **A**.

## Context

### What the checker does today (almide 0.64.0, measured)

- `check_named_call_with_type_args` resolves a builtin before it looks up a
  signature (`crates/almide-frontend/src/check/calls.rs:437-440`). The builtin
  arm (`check_builtin_output`, `crates/almide-frontend/src/check/builtin_calls.rs:62`)
  constrains the argument types and returns `Unit` (`println`, `eprintln`, the
  assert family) or `Never` (`panic`). It never reads `can_call_effect`, so the
  E006 test at `calls.rs:514` (`sig.is_effect && !self.env.can_call_effect`)
  is never reached for these names.
- So `fn show(p: Int) -> Unit = println("p=${p}")` checks and runs, and so does
  `fn main() -> Unit = println(…)`. `fs.read_text` in the same position is
  E006, and so is `io.print`: `io.print` is a stdlib *effect fn*, not a builtin.
- The corpus already relies on this. `spec/`, `stdlib/` and
  `research/benchmark/exercises/` contain 535 files with a pure `fn main` and
  386 with `effect fn main`. In `spec/wasm_cross/`, 486 of the 792 fixtures use
  a pure `fn main`.
- `crates/almide-frontend/src/stdlib.rs:305` still has a function
  `builtin_effect_fns()` returning `["println", "eprintln", "panic"]`. Its doc
  comment says "built-in effect functions". Nothing calls it, and the name
  contradicts this ADR.

### Survey: how peers treat print and abort in pure code

Sources are primary: the local clones under `almide-references/` (Koka, Roc,
Gleam, Swift, Rust) and the official language documentation (Go, Haskell).

| Language | Effects tracked in types? | Real output from pure code | Debug-output exemption | Abort from pure code | Primary source |
|---|---|---|---|---|---|
| Rust | No | Yes: `println!` anywhere | `dbg!` (stderr, returns its value) | Yes: `panic!` | `library/std/src/macros.rs` (`println` :143, `eprintln` :221, `dbg` :358; the `dbg!` doc example at :296 is a plain `fn factorial`) |
| Go | No | Yes: `fmt.Println` or the builtin `println` anywhere | The builtin `print`/`println` are for bootstrapping: "not guaranteed to stay in the language" | Yes: `panic` | go.dev/ref/spec §Bootstrapping |
| Haskell | Yes (`IO`) | No: `putStrLn :: String -> IO ()` | `Debug.Trace.trace :: String -> a -> a`, pure type, "should *not* be used in production code" | Yes: `error :: HasCallStack => [Char] -> a` is a pure bottom | hackage `base` Prelude, `Debug.Trace` |
| Koka | Yes (effect rows) | No: `println(s) : console ()` | `trace(message) : ()` is total (via `unsafe-total`); `notrace()` turns it off | `throw : exn a` is tracked. `assert` and `impossible` are typed total (via `unsafe-abort`) | `lib/std/core/console.kk:78`, `debug.kk:58-60, 81, 84`, `exn.kk:49`, `core.kk:55, 79` |
| Gleam | No | Yes: `io.println(string) -> Nil` anywhere | `echo` (1.9) prints to stderr with module and line; `gleam publish` refuses a package that still contains `echo` | Yes: `panic`, `todo`, `assert` | `changelog/v1.9.md:32, 99`, `compiler-cli/src/publish.rs:489-508`, stdlib `gleam/io.gleam:42` |
| Roc | Yes (`->` pure vs `=>` effectful, inferred) | No: "Effectful functions can only be called by other effectful functions" | `dbg` and `expect` are allowed in pure functions: "not part of a program's semantics, and so the side effect is allowed outside effectful functions" | Yes: `crash`; pure functions are "not guaranteed to be *total*" | `docs/langref/functions.md:25-42`, `expressions.md:113-117`, `statements.md:138-151` |
| Swift | No (only `throws`/`async`) | Yes: `print(_:separator:terminator:)` anywhere | none separate (`print`, `debugPrint`) | Yes: `fatalError(…) -> Never`, `precondition`, `assert` | `stdlib/public/core/Print.swift:54`, `Assert.swift:275-278` |

What the table shows:

1. **Every language lets pure code abort.** Even the three that track effects
   do: Haskell's `error` is a pure bottom, Roc's `crash` is allowed in pure
   code, and Koka types `assert` and `impossible` as total. Koka tracks only
   the recoverable `throw` (`exn`). An abort ends the program's observations
   and does not add a new one, so none of these languages treats it as I/O.
2. **Every language that tracks effects exempts debug output** (`trace`,
   `dbg`), and gives the same reason: the output is for the programmer and is
   not program semantics. Those exemptions are write-only. None of them lets
   pure code *read* anything.
3. **The languages that do not track effects put no limit on output.**

Almide's position is between the two families. `println` is real program
output, not a trace channel, so Almide's exemption is wider than Haskell's
`trace` or Roc's `dbg`. But reads (`fs`, `env`, `io.read_line`, `http`,
clocks, `random`) stay effect-gated, which the untracked languages do not do.

## Decision

**`println`, `eprintln`, `panic`, `assert`, `assert_eq` and `assert_ne` are
admissible in a pure `fn`. They are writes to stdout or stderr, or program
aborts, and never reads. Calling one does not make the caller effectful and
creates nothing to propagate.** The normative text is
[effect-system.md §2.1](../specs/effect-system.md). This ADR records why.

### D1. The exempt set is closed and names builtins, not modules

The six names above are the whole set. `io.print`, `io.write`,
`io.write_bytes` and the `log` module are stdlib effect fns and stay E006 from
a pure fn. Adding a name to the set requires amending this ADR. Any candidate
must be a write-only or aborting builtin that returns `Unit` or `Never`.

### D2. No effect, no propagation

The builtins return `Unit` (`println`, `eprintln`, the assert family) or
`Never` (`panic`), never a `Result`. There is nothing for `!` to unwrap:
`println(s)!` is E034 in every context. A pure fn that calls them stays pure
in its signature, in its module interface and at every call site. An effect fn
calls it as a plain value, with no `!`.

### D3. Reads remain the boundary

A pure fn still cannot observe the outside world. None of the exempt builtins
returns information from outside the program. So a pure fn's *result* is still
a function of its arguments. What the exemption adds are *observations*: bytes
on stdout or stderr, and possibly a terminated process.

### D4. Interactions (each is stated normatively in effect-system.md §2.1)

- **Served handlers (ADR-0020).** A handler that prints through a pure helper
  follows the same rule as one that prints directly: each call's bytes reach
  their stream contiguously (a line is atomic), lines from one request keep
  program order, and the order of lines from different in-flight requests is
  unspecified.
- **`fan` bodies.** A pure fn called from a `fan` arm prints as part of that
  arm. The exemption adds no ordering promise beyond the fan contracts (C-004;
  `fan.settle` thunk side-effect interleaving is outside the contract on
  native). `fan` itself still requires an effect context (E007), and ADR-0011
  D1 (arm-ordered output), once implemented, covers these bytes like any
  others.
- **Reference evaluators.** Neither evaluator has an effect context for these
  names. The in-process interpreter (`almide-interp`) dispatches them by name
  (`crates/almide-interp/src/dispatch.rs:393`). The judge's reference evaluator
  (ADR-0015) lists them as its `PRELUDE` (almide/als `ref/src/eval.rs:217-224`),
  and that list is exactly the six names in D1. So the three-way oracle
  compares a pure fn's output the same way it compares an effect fn's. Only the
  checker enforces the pure/effect distinction.
- **Buffering.** A pure fn's output goes through the same streams as an effect
  fn's. Native stdout is one buffer, flushed on every write when stdout is a
  terminal and 64 KiB-buffered otherwise; stderr is unbuffered (C-162, and the
  statement in C-367). The contract is per stream: stdout bytes, stderr bytes
  and the exit code are byte-identical between native and wasm. How the two
  streams interleave when both go to one file is not part of the contract
  (measured: `println; eprintln; println` merged with `2>&1` came out as
  `o1 e1 o2` natively and `e1 o1 o2` on wasm). An abort flushes the stdout
  written before it, then writes its stderr block (C-153).
- **`scoped`.** Unchanged. A `scoped fn` is stricter than a pure fn, and
  `println` inside one stays E087. `@bounded` admits the output builtins, as
  C-316 already states.

### D5. Which `main` the docs teach

A plain `fn main() -> Unit` is correct whenever `main` performs no real
effect. Write `effect fn main()` only when `main` reads, writes a file, uses
the network, uses `fan`, or propagates with `!`. The LLM-facing docs
(`docs/CHEATSHEET.md`, `llms.txt`) teach it this way and say so in one line.

## Rationale

- **It is what the language already does.** Option A changes no behaviour, and
  the corpus depends on it in 535 files. B would turn every one of them into an
  E006 and add an `effect` keyword to programs that perform no effect except
  output. Adding a keyword that carries no information makes programs harder
  to modify, which works against modification survival.
- **Aborts are exempt everywhere (Survey point 1).** Rejecting `assert` in a
  pure fn would make Almide the only language in the table that does so. It
  would also block the idiomatic "check an invariant inside a pure helper"
  pattern.
- **The guarantee pure code gives callers is about inputs, not outputs.**
  "A pure fn's result is a function of its arguments" holds under A (D3). The
  checker gains its real value from gating reads and host access, and every
  host-reaching module stays gated.
- **Debug output in pure code is universal (Survey point 2).** Every
  effect-tracking language in the table had to add an escape hatch for it.
  Almide does not need a second, trace-only builtin: `eprintln` already goes to
  stderr and does not mix with stdout.

## Alternatives rejected

- **B: output requires `effect`** (Haskell's and Koka's `console`). It is
  stricter on paper, but it breaks 535 corpus files and every doc example with
  a pure `main`. The tracked languages that chose this still had to add a
  trace exemption, so the result would be option A plus one more builtin.
- **C: aborts only; output requires `effect`.** This has the same cost as B
  for output, and removes the most common way to debug a pure helper.
- **A trace-only builtin (`dbg`/`trace`) instead of admitting `println`.** It
  would add a second output spelling for LLMs to choose between, and every
  existing pure `main` would still need `effect`. Gleam enforces a debug
  printer only at publish time, which shows that such a rule is tooling, not
  typing.

## Consequences

- The rule is now written down and pinned. The check test is
  `tests/checker_test.rs` (`pure_fn_admits_output_and_abort_builtins`,
  `pure_fn_still_rejects_a_real_read_with_e006`). The run fixtures are
  `spec/lang/pure_fn_output_builtins_test.almd` and
  `spec/wasm_cross/pure_fn_output_and_assert_abort.almd`; the latter is
  evidence for C-153 and must be byte-identical on native and wasm.
- The docs teach one rule for `main` (D5).
- Costs: a pure fn's call count and call order become observable through its
  output. So an optimizer may not deduplicate, hoist or reorder a pure call
  that may print or abort. A pure fn is therefore not freely memoizable. That
  was already true, because `panic` could run in any pure fn.

## Falsifier

Withdraw this decision (moving to B, or to a trace-only builtin) if either of
the following is observed:

1. An optimization that the pure/effect split is meant to allow (CSE, hoisting
   or memoization of pure calls) is blocked in practice by the output builtins,
   measured on the perf corpus.
2. Dojo measurements show that LLM-written programs print from pure helpers
   in ways that break modification survival (for example, output order
   changing when a pure call moves). "Stricter would be more principled" is
   not a falsifier on its own.

## References

- Rust: `library/std/src/macros.rs` (clone at `almide-references/rust`)
- Go: <https://go.dev/ref/spec#Bootstrapping>
- Haskell: <https://hackage.haskell.org/package/base/docs/Debug-Trace.html>, <https://hackage.haskell.org/package/base/docs/Prelude.html>
- Koka: `lib/std/core/{console,debug,exn,core}.kk` (clone at `almide-references/koka`)
- Gleam: `changelog/v1.9.md`, `compiler-cli/src/publish.rs` (clone at `almide-references/gleam`)
- Roc: `docs/langref/{functions,expressions,statements}.md` (clone at `almide-references/roc`)
- Swift: `stdlib/public/core/{Print,Assert}.swift` (clone at `almide-references/swift`)
