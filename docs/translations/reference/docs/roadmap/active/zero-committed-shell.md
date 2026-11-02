Source description metadata: Which gates belong in Almide and which stay bash, and how the line is held
<a id="the-gate-language-boundary"></a>

# The gate language boundary

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](zero-committed-shell.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](zero-committed-shell.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](zero-committed-shell.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/zero-committed-shell.md)

> Source caveat: this records a changed goal and historical measurements, not instructions executed by this translation. Local byte-identity acceptance during the port is distinct from the missing continuous CI twin/original/generated-artifact comparison; the latter is not claimed complete. Implemented reads classification and subcommand inventory are distinct from still-required twin promotion. Counts, costs, defects and old CI locations are source-time observations, not a fresh inventory. Here 50 + 81 - 7 = 124; the additions are not the net increase. Historic timeout names and guarantees are read with the current-reference qualification; matching behavior when a timeout fires does not guarantee equal firing decisions across hosts.

> **The goal was replaced on 2026-09-12.** The old goal was “not a single `.sh` file in this repository.” Six weeks of measurements refuted it, so we redrew the line. The history and old goal remain below under “Why the goal changed.” The filename stays to preserve references.
>
> Outward-facing counterpart: [shell-scripting-surface.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/shell-scripting-surface.md) (shell scripting as a language users and LLMs write). This item faces inward — which language to use for this repository's own gates.

<a id="決定"></a>

## Decision

**Write gates that read structured data in Almide. Gates that just grep and exit may remain bash.**

Not “zero committed shell.” Zero was the wrong goal.

<a id="なぜこれ以外にないのか"></a>

## Why this is the only choice

The decisive factor is not preference but **asymmetry in marginal cost and reuse**.

A bash gate is cheap per file. A new file, 1 CI line, 1 line in `proofs/gate-verification.toml` — **3 places to touch**, and the file is wholly independent. In exchange, **reuse is zero**: 60 `scripts/check-*.sh` scripts independently rewrite `grep`, `awk` and `sed`.

Almide initially cost more. `tools/almide-gates/src/toml/mod.almd` is a 291-line subset TOML reader; `src/mdmeta/mod.almd` is 68 lines for markdown front matter. Something bash handles with three `grep` calls is written out as a typed parser.

**That investment is already paid.** The next person writing a gate that reads TOML, markdown or a ledger gets 359 lines of parser for free. Thus, specifically for structured-data gates, **Almide is now cheaper**. All 8 defects found by the Unit 0.46 migration occurred in precisely these “decision rules” (below).

Conversely, that investment does nothing for a gate shaped like `grep -q foo file || exit 1`. Touch points increase from 3 to 5–6, a shared 245-line file must be edited, and nothing is gained. **That is the line.**

<a id="線の定義"></a>

## Defining the line

| What the gate reads | Language | Examples |
|---|---|---|
| Structured data (TOML / markdown / ledgers / comparison of generated artifacts) | **Almide** | contracts.toml consistency, roadmap README regeneration, ledger counts |
| String presence / simple shapes (grep and fail) | **bash** | Forbidden spellings, reappearance of retired APIs, file existence |

When uncertain, classify it as structured. If you are writing a parser, it is structured.

<a id="どう守るか"></a>

## How to enforce it

A rule alone is not obeyed. That is precisely what the six weeks measured (below).

Use `proofs/gate-verification.toml`. The ledger is authoritative for row counts; each row has `path` / `class` (KERNEL_PROVEN / MUTATION_TESTED / NEGATIVE_TESTED / EXERCISED / UNVERIFIED) / `evidence`, and `scripts/check-gate-verification.sh` rejects enumeration gaps, omissions and staleness. UNVERIFIED burns down under a **decrease-only ceiling**.

The classification field `reads` is implemented, taking `structured` / `scalar` / `UNCLASSIFIED`. **Checks reject `.sh` declared `reads = "structured"`.**

- `UNCLASSIFIED` is reduced under the ledger ceiling. Do not copy the obsolete fixed value of 79 rows.
- New gates must declare; checks cover omissions, unknown classifications and insufficient evidence.
- Subcommands of `tools/almide-gates/src/main.almd` are already enumerated as `tools/almide-gates:<cmd>`. Their introduction itself is not remaining work.

**Do not build a ratchet counting `.sh` files.** The old goal's done criteria called for one, but once grep-and-exit gates are rightly left in bash, counts may legitimately grow. File count is no longer the metric.

<a id="twin-は検証してから昇格する"></a>

## Verify twins before promotion

Unit 0.46 ported 7 gates to Almide. Those 7 — `contracts-readme` / `conformance` / `roadmap-readme` / `check-contracts` / `output-parity` / `stamp` / `fuzz-track-record` — exist in `tools/almide-gates/`, with unit tests run by `almide test`.

**But comparison against the `.sh` originals does not exist.** CI actually runs only `.sh` (`ci.yml:1132`, `ci.yml:1240`, `ci-cross-target.yml:142`); only 1 twin, `bench`, is used as a live gate. `main.almd`'s byte-match tests cover only 2 files, `docs/contracts/conformance.md` and `docs/contracts/README.md`.

This document formerly said “the byte-diff runs in CI.” **It merely said so; it was not implemented.** Keep that record — declaring a mitigation and not implementing it is worse than not declaring it.

Therefore fix the order:

1. **Build the diff.** For the relevant gate, verify in CI “Almide twin output == `.sh` original output == committed generated artifact.”
2. **Confirm green.**
3. **Only then promote.** Make the Almide side the live gate and delete `.sh`.

Do not delete `.sh` without evidence; otherwise breakage goes unnoticed.

**First is `roadmap-readme`.** It is the smallest and safest of the 7 (only reads markdown and outputs a table, no side effects), yet has no verification despite being `main.almd`'s default subcommand. And `docs/roadmap/README.md` had actually drifted (8 missing items found on 2026-09-12), so this 1 step **resolves both README staleness and the unverified twin**.

<a id="なぜ目標を差し替えたか"></a>

## Why the goal changed

The old goal was “`git ls-files '*.sh'` is 0.” At drafting on 2026-08-01: 50 files / 6,159 lines.

**Measured on 2026-09-12: 124 files / 13,579 lines.**

| | |
|---|---|
| Added | 81 |
| Deleted | 7 |
| **Of the additions, `scripts/check-*.sh`** | **50** |

Over six weeks, **every new CI gate was written in bash**. New inflow was about 7 times the 7 ported gates. Porting was not slow; the tap was still open.

And nobody noticed, because the old done criterion “A gate fails CI when the count goes up” **was unimplemented**.

Two lessons follow. First, **posting a rule does not enforce it** — hence ledger declarations and machine checks under “How to enforce it.” Second, **six weeks of actual choices might reflect correct decisions every time** — there really is no reason to use Almide for a one-grep gate. The zero goal was what failed to match reality.

<a id="残すもの--unit-046-の測定"></a>

## What remains — Unit 0.46 measurements

The goal changed, but the most valuable part of this item is measurement, and that remains.

Ported 7 gates **with byte identity as the acceptance condition**, additionally checking the last one against **15 mutations**.

| | |
|---|---|
| Bash replaced | 7 gates, about 1,300 lines |
| Almide written | About 2,300 lines, 14 modules |
| Tests | 47, covering decision rules reachable only through I/O in bash |
| Defects found by porting | **8** |

Of the 8, **5 had the same shape** — **rules copied because copying was cheaper than abstraction, all agreeing on the day they were written**. Locale pinning scattered over 11 scripts, unquote expressions across 5 extractors, evidence-class enums written in 2 places.

This is not author laziness. Shell has neither modules nor type checking, so **copying genuinely is the cheapest choice**. The port found these not through cleverness but because byte-diff rejected “plausible paraphrases.”

<a id="タイムアウト--当初の経緯と現在の境界"></a>

## Timeouts — original history and current boundary

Current child-process deadlines use `ProcessCommand.timeout_ms` ([process reference](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/process.md)). `fan.timeout(duration.ms(n)) { body }` also exists in the oracle layer; it has not remained removed. Deterministic computation bounds use `fan.bounded(compute.ms(n))`; clock-based selection uses `fan.min` from [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md). Below is design history at #1040, not a table deciding present syntax availability.

`output-parity` needs subprocess time limits and then outsourced them to `perl -e 'alarm …; exec …'`. At that time `fan.timeout` had been removed in 0.29.0 — timing out **pure computation** makes results a function of the machine, breaking native ⇄ wasm byte identity.

The line then was: **a time limit is allowed only when the enclosed operation is already outside the byte-identity contract**.

| Enclosed operation | Already nondeterministic? | Time limit |
|---|---|---|
| `process.exec_status` (launches a process) | yes | **Allowed** |
| `http.get` (network) | yes | **Allowed** |
| Pipe / socket reads | yes | **Allowed** |
| Pure `fan` siblings | **no** | **Not allowed — this is what 0.29.0 removed** |

The contract has two separate statements:

> **Byte identity guaranteed**: if the deadline fires, both targets produce the same error value, same message and same exit path.
> **Not guaranteed**: whether it fires. That is a function of the host.

**Landed** in [#1040](https://github.com/almide/almide/issues/1040) — at the time it entered the effect surface as `process.exec_status_timeout`. For current naming and reintroduction of `fan.timeout` in the oracle layer, follow the current references at the start of this section.

<a id="解けた障害"></a>

## Resolved blockers

The 2 items the old version called “the two primitives that block the rest” are **both closed**.

- [#1040](https://github.com/almide/almide/issues/1040) — the timeout above. Landed.
- [#1041](https://github.com/almide/almide/issues/1041) — the problem of pure `list.map` callbacks forcing effectful pipelines down to `var` + `for`. Resolved (ADR-0006; `!` in the callback instantiates the fallible form).

However, `tools/almide-gates/` still contains 13 `var` + `for` sites (`parity_sweep.almd:71-79`, etc.). This code predates #1041; it is not a language limitation. Rewrite alongside measurements of friction.

<a id="既知の摩擦almide-側でゲートを書くときのコスト"></a>

## Known friction (costs of writing gates on the Almide side)

Drawing this line means more Almide gates. Record the measured friction.

- **5–6 places for 1 new gate** (bash: 3). One is **the 245-line `main.almd` shared by every gate** — add an import, a `cmd` match arm and a root-default match arm.
- **Trap**: gate-style commands (returning an exit code) must call `process.exit` inside the arm to avoid the generator-oriented `println` at the end of `main.almd`. Easy to hit if unaware.
- **No template** (none on the bash side either).
- **Running Almide gates in CI requires the Rust toolchain.** Bash requires nothing.
- `list.sort_by` takes a key extractor, not a comparator; without `string.compare`, descending order means “sort ascending, then reverse” (recorded at `main.almd:70`).

**Do not fix these speculatively in advance.** Encounter them on the next 1 item (`roadmap-readme`'s diff), then fix them in encounter order.

<a id="done-criteria"></a>

## Done criteria

- `proofs/gate-verification.toml` has a classification field; CI fails if a gate declared structured is written in bash. The `UNCLASSIFIED` ceiling only decreases.
- Ledger enumeration includes `tools/almide-gates/` subcommands.
- Each of the 7 twins has a green CI diff for “Almide output == `.sh` output == committed generated artifact.”
- Promote twins in the order their green status is confirmed, deleting the corresponding `.sh`.
- Newly added structured gates are written in Almide (enforced by the ledger).

<a id="risks"></a>

## Risks

- **R1 — Porting for its own sake.** A script nobody runs is not worth making a program. Mitigation: the line defines scope. Grep-and-exit is outside it, and not porting is correct.
- **R2 — Twins drift from the `.sh` oracle.** Mitigation: **implement the diff** (the old version declared this but never implemented it, which is how R2 became real). Do not delete `.sh` until the diff is green.
- **R3 — Confuse deterministic computation bounds with environmental deadlines.** Mitigation: distinguish `Compute` and `Duration` contracts, and do not claim target invariance for whether wall-clock expiry occurs. Current SPEC / ADR-0028 are authoritative; do not reinterpret the 0.29.0 removal as a current prohibition.
- **R4 — The line becomes undecidable.** If “structured or grep?” is debated every time, the ledger field becomes hollow. Mitigation: the default above says to choose structured when unsure. If writing a parser, it is structured.
