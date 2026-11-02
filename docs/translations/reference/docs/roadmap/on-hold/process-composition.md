> Source description: Build a command pipeline as a value and make running it one explicit step; parked until a real need appears
<a id="process-composition-a-pipeline-is-a-value-running-it-is-a-separate-step"></a>

# Process composition: a pipeline is a value, running it is a separate step

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](process-composition.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](process-composition.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](process-composition.zh-CN.md)

> Non-normative pinned reader snapshot. The original remains authoritative; this copy is not automatically synchronized and has not independently revalidated implementations, measurements, or safety.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/process-composition.md)
> This held proposal does not authorize activation or execution. Existing ProcessCommand APIs are separate from the unimplemented pipeline syntax and claimed future memory/effect properties. Demand, static checks, must-use, terminal forms and fan interaction remain unresolved as stated. The source’s absolute design arguments and ten-year prior-work claims are reproduced, not independently established here.


> Split out from Phase 4 (lazy pipeline composition) of [shell-scripting-surface.md](../../../../../roadmap/active/shell-scripting-surface.md). **That item's hold decision has not been reversed** — this item gives the design a home and records its activation conditions.
>
> Design reference: `../almide-references/RESEARCH-shell-composition.md`
> (notes from reading plumbum / sh / xonsh; clones are in the same directory).

<a id="現行-api-との境界-2026-10-31"></a>

## Boundary with the current API (2026-10-31)

The existing API executes a `ProcessCommand` configuration value through `process.output` / `process.start` / `process.run`. Supply stdin through the record's `stdin` field. [`stdlib/process.almd`](../../../../../../stdlib/process.almd) is the current definition. The `process.cmd` / `pipe_to` / `capture` below are an unimplemented composition API proposal; the mechanism for representing a single process as a configuration value already exists.

<a id="状態-保留"></a>

## Status: on hold

Quoting the parent's reason for holding it:

> It was never needed when porting this repository's gates (Unit 0.46 record), so this item also stays unstarted until demand appears. Activate it when one real example handling large intermediate output emerges.

**The existence of a prior implementation is not evidence of demand.** A library having operated for ten years can accelerate design, but not the decision to start.

<a id="起こす条件"></a>

## Activation conditions

Move to active when any one of these is observed.

1. **One real example handling large intermediate output appears.** Currently `ProcessCommand.stdin` passes an entire string, putting intermediate output in memory. Even one gate streaming logs or large dumps meets the condition.
2. **A pipeline needs early termination equivalent to `head`.** If upstream must stop when downstream finishes first, this cannot be expressed without lazy composition.
3. **There is demand to run the same pipeline in two places.** It cannot be reused unless it is a value.

<a id="何を作るのか"></a>

## What to build

Make commands values, compose them, then run them in one final step.

```almide
// 構築 — 純粋。プロセスは起きない
let chain = process.cmd("grep", ["-v", "\\.py"])
  |> process.pipe_to(process.cmd("wc", ["-l"]))

// 実行 — ここだけが effect
let n = chain |> process.capture()!
```

There is no user-defined operator overloading (CHEATSHEET: “No macros, no operator overloading, no implicit conversions”), so composition uses named functions. `|>` is function application and can be used as-is.

<a id="なぜ値にするが-almide-に効くのか"></a>

## Why making it a value matters for Almide

Three reasons, none merely cosmetic.

**1. The effect boundary matches the composition structure.** `process.cmd` and `process.pipe_to` are `fn`; only terminals `capture` / `status` / `spawn` become `effect fn`. In today's manual handoff from the result of `process.output` to the next `ProcessCommand.stdin`, each intermediate step is effectful, scattering `!` throughout composition. A value **concentrates the effect at one point**.

**2. Pipelines can be inspected.** Being values, they can be bound with `let`, compared, and tested by asserting “this input constructs this tree.” Testing pipeline correctness without starting processes has practical value for people porting gates.

**3. Intermediate output stays out of memory.** Actual wiring uses OS pipes, so upstream output is not held as a string. This is expected to be the leading reason to lift the hold.

<a id="先行例から取るもの"></a>

## What to borrow from prior examples

`plumbum` (tomerfiliba/plumbum, `plumbum/commands/base.py` line 725) has operated the same design for ten years. Borrow two points.

**Separate construction from execution.** `__or__` / `__gt__` / `__getitem__` only build nodes; only `__call__` calls `run()`. There is just one entry to side effects.

**Wiring that preserves SIGPIPE.** After connecting upstream stdout to downstream stdin, `Pipeline.popen` **explicitly closes the parent's duplicate**:

```python
srcproc.stdout.close()   # allow p1 to receive a SIGPIPE if p2 exits
```

Forgetting this lets upstream run forever even after downstream dies. Almide already handles SIGPIPE (`MAIN_SIGPIPE_PRELUDE`), so integration looks possible, but record this as **a pitfall that must be addressed during implementation**. The same applies to cleanup where `wait()` waits for upstream too.

<a id="採らないもの"></a>

## What not to adopt

- **Mode inference** (`xonsh`) — interpreting a line as language if valid, otherwise as a command. A mistake does not become a syntax error but **another valid meaning**, so a diagnostic is impossible in principle. The parent rejected this strategically, and the mechanism supplies an independent reason.
- **Attribute dispatch** (`sh`) — dynamically resolving command names, as in `sh.git.log(...)`. Typos fail at runtime, throwing away the benefit earned through diagnostics.

<a id="未解決の設計論点"></a>

## Unresolved design questions

Decide these before starting.

1. **Whether to statically check command names.** Nobody currently stops `process.cmd("gre", ...)`. Resolving `PATH` during check would introduce environment dependence and is likely rejected, but **explicitly choosing to do nothing** is necessary.
2. **Whether E042 (must-use) should apply to `Pipeline` values.** Pure construction makes it possible to build a chain and **forget to run it**. Because the type has no effect, current E042 does not catch this.
3. **How many terminals to provide.** Current main execution forms are `output` / `run` / `start` (`exec` is shorthand). Gate ports in [zero-committed-shell.md](../../../../../roadmap/active/zero-committed-shell.md) provide measurement material for whether capture while also streaming to the terminal (plumbum's `run_tee`) is needed.
4. **Relationship to `fan`.** Pipelines compose sequentially; `fan` is parallel. `A | B` means “B waits for A's output,” whereas `fan` has independent branches. Can readers distinguish them when both use `|>`?

<a id="done-criteria"></a>

## Done-criteria

(Completion conditions once work starts; not evaluated while on hold.)

- Pipelines can be built as values; construction is `fn`, only terminals are `effect fn`.
- Intermediate output stays out of memory (measure with large output).
- Early downstream exit propagates upstream (measure with a `head` equivalent).
- A realistic example using the above appears in [CHEATSHEET.md](../../../../../CHEATSHEET.md) with ```` ```almide check ````.

<a id="risks"></a>

## Risks

- **R1 — Design grows while the item remains on hold.** Mitigation: activation conditions are defined; do not start until observed. More design questions do not justify starting.
- **R2 — Scope creeps into an interactive shell.** Once pipeline composition exists, it can seem “only the prompt remains.” Mitigation: the parent's decision is the boundary. Crossing it requires rewriting that decision with an MSR-delta estimate.
- **R3 — Missing SIGPIPE wiring.** Mitigation: the `stdout.close()` and `wait()` cleanup above are fixed here as implementation checklist items.

> The retained Almide comments say construction is pure and starts no process; execution is the only effectful step. The Python comment says closing upstream stdout allows p1 to receive SIGPIPE if p2 exits.
