Source description metadata: Take Nushell's data model for scripts; do not build an interactive shell
<a id="shell-scripting-stance-structured-pipelines-no-interactive-shell"></a>

# Shell scripting stance: structured pipelines, no interactive shell

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](shell-scripting-surface.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](shell-scripting-surface.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](shell-scripting-surface.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/shell-scripting-surface.md)

> Source caveat: measurements belong to 2026-09-12, v0.62.0 and Darwin 24.4.0, not universal or independently rerun performance. Claims about competing shells, MSR and usability are the source design rationale, not independently validated rankings. Proposed table/interpreter REPL/one-step parsing and deferred streaming are not assertions of shipped APIs; the withdrawn dynamic-row sugar remains withdrawn. Error-result handling is not mandatory checking of every process exit code, and argv passing is not protection when a shell is explicitly invoked. The recorded 50-to-124 change and increment of 81 are arithmetically inconsistent and are retained; the old self_host_registry addition procedure is historical, not executed current guidance. Requiring handling of dynamic lookup failures is not itself compile-time validation of a dynamic column name; the static-name claim applies to the proposed typed-record boundary, not arbitrary dynamic lookup.

> The starting question was “Could Almide offer Nushell-like usability?” The answer is **half yes, half something we should not pursue**; this document draws that line.
> Adjacent item: [zero-committed-shell.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/zero-committed-shell.md) (inward-facing). This item is outward-facing.

<a id="決定"></a>

## Decision

**Take Nushell's data model, not its terminal.**

The writing experience of structured data flowing through pipes — pursue that. **Do not build** the interactive-shell layer (command resolution, job control, redirection, line editing, prompt).

Aim to “replace bash scripts,” not “replace zsh.”

<a id="なぜ対話シェルを作らないのか"></a>

## Why not build an interactive shell?

Three reasons, each sufficient on its own.

**1. It does nothing for MSR.** Job control, command resolution and glob expansion belong to the terminal, not the language. This language's mission is modification survival rate ([llm-first-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/llm-first-language.md)); interactive UI is not even part of that measurement. Implementation cost is high and MSR delta is zero. Since that is what the decision criterion says, we do not adopt it.

**2. This approach has repeatedly lost.** xonsh, Elvish, Ammonite — attempts to “make your language an interactive shell” have existed for over a decade, and none has replaced bash/zsh. What Nushell won, on the other hand, was not due to its interactive UI but **because it redesigned the data model**. That is what to imitate. Take only the winning part.

**3. Interactive-shell requirements directly conflict with static types.** The interactive-shell experience rests on “columns determined at runtime.” Almide could only accommodate that by loosening typing, directly cutting MSR (see “No dynamic rows” below).

<a id="いま実際どこにいるか2026-09-12-実測v0620darwin-2440"></a>

## Where we actually are (measured 2026-09-12, v0.62.0, Darwin 24.4.0)

| | Measured |
|---|---|
| `almide run script.almd` (warm) | **20ms** |
| `almide run --target wasm` (warm) | 38ms |
| 1 REPL expression (warm, wasm path) | About 40ms |
| REPL session with 80 lines of definitions | 1.57s (about 19ms/line — flat through 80 lines) |
| REPL expression containing `process.exec` | 0.82s (wasm path falls back to the cargo route) |

**20ms is in the same class as python3 and fully practical for a scripting language.** This is the premise of this item; every subsequent decision rests on this number.

<a id="既にある材料api-名は現行リファレンスに追従上の時間計測は-2026-09-12-の記録"></a>

### Materials already available (API names follow the current reference; timings above were recorded on 2026-09-12)

- `|>` and [`list`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/list.md) operations — `map` / `filter` / `fold` / `sort_by` / `group_by` / `partition` / `window` / `unique_by` / `take_while`. Nushell's `each` / `where` / `reduce` / `sort-by` / `group-by` largely have counterparts.
- If a callback includes `!`, it **switches directly to the fallible form** (ADR-0006, resolved in #1041). Pipelines containing effects do not fall back to `var` + `for`.
- [`process.*`](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/process.md): `exec` / `output` / `run`, `ProcessCommand`'s `dir` / `stdin` / `timeout_ms`, and native `start` / `poll` / `wait` / `stop` / `read_new_stdout` / `read_new_stderr`. Do not use the old names `exec_in` / `exec_with_stdin` / `exec_status` / `spawn`.
- `fs.*`: `glob` (matcher gaps resolved in #1805) / `walk` / `stat` / `list_dir` / `fold_lines` / `read_lines`.
- `io.*`: `read_all` / `read_line` / `read(io.stdin()!, n)!` / `read_exact` / `write`.
- Shebang: `#!/usr/bin/env -S almide run` (`spec/cli/shebang.almd`).
- REPL: `almide` without a subcommand (`src/cli/repl.rs`).
- Rows themselves already return as named records — `FileStat { size, is_dir, is_file, modified }`, `ProcessStatus { code, stdout, stderr }`.

`process.exec(cmd, args)` passes an argv list, so it does not perform shell-string word splitting or expansion (unless the user explicitly launches `sh -c`, etc.). `Result` and E042 require handling error results, but do not guarantee checking child-process exit codes. `exec` turns nonzero exit into `err`, whereas `output` returns `ok(ProcessStatus { code, stdout, stderr })` even for nonzero exit, and `run` also returns an exit code; callers check those code values.

<a id="無いもの"></a>

### What is missing

1. **Table renderer** — no corresponding item in `stdlib/`. This accounts for a subjective half of the Nushell experience.
2. **One-step boundary parsing** — `exec` → `json.parse` → Codec decode takes three steps.
3. **Lazy pipeline composition** — no streaming `A | B`; intermediate output resides in memory.
4. **Line editing** — REPL uses raw `read_line`. Arrow keys, history recall, completion and multiline editing are all absent.
5. Interprocess pipe syntax, command resolution, job control (= the things we decided not to build).

<a id="動的行は入れない--前案の撤回"></a>

## No dynamic rows — withdrawing the earlier proposal

Early in the discussion, we treated “sugar for writing `value.field(v, "name")!` as `v.name`” as the main objective. **That was wrong, so we withdraw it.**

`value.field` returns `Result`, and E042 forces handling. Sugaring this into `v.name` is equivalent to **replacing a checked failure with a runtime failure**. Misspelled column names would no longer fail at compile time — rather than raising MSR, this silences the very kind of error LLMs most easily make.

Almide already explicitly states the correct path in [CHEATSHEET.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md) §JSON & the wire:

> Typed Codec is the default path for ALL JSON work. The dynamic `json.*` API
> is for exploration and schemaless passthrough only — if you know the shape, declare a type.

External-command output is the same “wire.” **Convert to records once at the boundary, then run a statically typed pipeline.** This is stronger for modification survival than Nushell's dynamic tables: when a column name changes, Nushell notices at runtime; Almide points to all references at compile time.

Therefore Phase 3 is not “make dynamic access easier” but “**make conversion to records at the boundary a single step**.”

<a id="the-gate-language-boundary-との関係"></a>

## Relationship to the gate language boundary

They do not overlap.

| | [the gate language boundary](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/zero-committed-shell.md) | This item |
|---|---|---|
| Direction | Inward — what to write this repository's own gates in | Outward — make it a language users and LLMs write |
| Target | Existing and new gates | Newly written scripts |
| Success criterion | Structured gates are written in Almide and enforced by the ledger | Writing experience and adoption |

The two blockers named there are **both closed** (#1040 timeout, #1041 effectful combinator). That item demonstrated feasibility by measurement — byte-matching migration of 7 gates and about 1,300 lines of bash, discovering **8 defects** in the process. This item builds on that.

That item changed its goal on 2026-09-12 (old: “zero committed shell” → “gates reading structured data use Almide; gates that just grep use bash”). This followed the measurement that committed shell grew from 50 → 124 scripts in six weeks, with 50 of the increment of 81 being `scripts/check-*.sh` — in other words, new gates were all being written in bash. The history is recorded there.

<a id="phase"></a>

## Phases

<a id="phase-1--table-モジュール"></a>

### Phase 1 — `table` module

Render `List[A]` as a ruled table. This alone most improves the feel of “Nushell-likeness” without touching the language surface at all.

The implementation mechanism already exists: if `A: Codec`, `A.encode` produces `Value`, and `value.keys` obtains column names. Thus it can be written in pure Almide as `stdlib/table.almd`.

```
table.print(rows)            // 罫線つき
table.render(rows) -> String // 文字列を返す形
table.print_with(rows, cols) // 列を選ぶ
```

Translation of code comments: table.print uses borders; table.render returns a string; table.print_with selects columns.

The MSR significance is not appearance. Without this, **LLMs write hand-rolled padding loops every time** — code extremely brittle under modification, including digit calculations and full-width character handling. Eliminate the entire family.

Follow Testing Rules in [CLAUDE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/CLAUDE.md) for `stdlib/` additions (self_host_registry registration, `spec/stdlib/` tests, 3-way oracle bridge).

<a id="phase-2--repl-を-almide-interp-に載せ替える"></a>

### Phase 2 — Move the REPL to `almide-interp`

[#1490](https://github.com/almide/almide/issues/1490) already independently ranks this second as “a major UX improvement, a small change, and a way to dogfood the oracle with real input.” `crates/almide-interp` already contains a 12,005-line tree-walk evaluator running on linked IR.

The current REPL runs the entire session through the pipeline for each input, an O(n²) structure. However, **measurements are flat at about 19ms/line through 80 definitions**; the quadratic term is not visible while fixed overhead dominates. So this is not a case of “currently slow and painful”: **the absence of line editing dominates the experience more**. When starting, ship the interpreter migration and line editing (rustyline equivalent) as one unit.

The jump to 0.82s when an expression includes `process.exec` and the wasm path falls back would also disappear on the interpreter route.

<a id="phase-3--境界パースを一手にする"></a>

### Phase 3 — One-step boundary parsing

Turn external-command output into records in one step.

```
process.exec_json[T](cmd, args) -> Result[T, String]   // T: Codec
text.columns(s) -> List[List[String]]                   // JSON を話さないコマンド向け
```

Translation of code comments: the first requires T: Codec; the second is for commands that do not speak JSON.

The former merely composes `exec` → `json.parse` → `T.decode`, but **these three steps are precisely what make typing feel troublesome and drive people to dynamic access**. One step makes Codec the default path. This is the part of this item with the greatest MSR effect.

<a id="phase-4--遅延パイプライン合成保留"></a>

### Phase 4 — Lazy pipeline composition (on hold)

Streaming `A | B`. `ProcessCommand.stdin` can pass an input string, but that alone is not lazy interprocess pipeline composition, and large intermediate output resides in memory. This was never needed in the repository's gate migration (Unit 0.46 record), so this item also waits for demand. Reactivate it when one real example handling large intermediate output appears.

<a id="採らないもの記録として"></a>

## What we do not adopt (for the record)

- **Interactive-shell layer** — for the reasons above.
- **Sugar for dynamic row access** — per “No dynamic rows” above. Anti-MSR.
- **Top-level statements** (eliminating `fn main`) — it is true that a 5-line script requires 3 lines of ceremony, but **LLMs do not mind boilerplate**. This concerns human writing comfort, not MSR, and is not worth changing the language surface. Reassess if human dissatisfaction is measured.

<a id="done-criteria"></a>

## Done criteria

- `table` exists in stdlib, with `spec/stdlib/` tests and a 3-way oracle bridge.
- The REPL never goes through rustc/cargo and has line editing (history, completion, multiline).
- JSON output of an external command run with `exec` becomes a typed record **in one step**.
- A full-size script example using these appears in [CHEATSHEET.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md) with ```` ```almide check ```` (per #1483's rule, posting it makes it a promise).

<a id="risks"></a>

## Risks

- **R1 — Scope creep into an interactive shell.** Once tables appear, “isn't only the prompt left?” inevitably follows. Mitigation: this item's decision is the boundary; crossing it requires rewriting this document with an MSR-delta estimate.
- **R2 — `table` distorts the type surface for display convenience.** Mitigation: `table` stands only on `Codec`; add no type-side requirements for rendering.
- **R3 — Phase 3 sugar becomes a bypass around Codec.** Mitigation: introduce `exec_json[T]` only in a form requiring `T: Codec`. Do not create a version returning `Value`.
