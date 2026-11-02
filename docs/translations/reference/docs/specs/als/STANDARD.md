<a id="als-requirements-standard"></a>

# ALS Requirements Standard

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](STANDARD.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](STANDARD.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](STANDARD.zh-CN.md)

> Authoritative source: [docs/specs/als/STANDARD.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/STANDARD.md), pinned at `35297a612fb29e3f78f6587aba253aa91d51f466`. This is a non-normative reader snapshot, not a replacement for the original ALS or checker/ledger/generator input; it is not automatically synchronized. Requirements, historical status, evidence and safety claims keep the source scope. No independent execution, measurement, certification or proof was performed for this edition. The source-language edition is a reader copy; other languages are translations.


> Last updated: 2026-08-21

How a normative section is written, identified, and retired. This is the
DO-178C Table A-1 artifact for the ALS: the standard the requirements are
authored against. `scripts/check-als-style.sh` enforces everything here that
a machine can check; the rest is review discipline.

> **Implementation checkout note (2026-10-31; explanatory, not a new
> requirements policy).** This document retains the 2026-08-21 authoring and
> review discipline below. The named `scripts/check-als-style.sh`,
> `scripts/check-als-validation.sh`, and `proofs/als-validation.toml` are not
> present in this checkout; their described enforcement and the statement
> about every recorded reviewer being the author are not a current validation
> report. The available [pin gate](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/check-als-pin.sh) checks
> contract IDs and statement/title agreement against the pinned judge, and the
> [element-coverage gate](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/check-als-element-coverage.sh)
> checks syntax-element coverage. Neither substitutes for substantive,
> independent review or establishes qualification. This note changes no
> contract, pin, authoring rule, or acceptance criterion.

<a id="section-identity"></a>

## Section identity

- A section heading is `## ALS-<PREFIX><n>[a-z]? <title>` — e.g.
  `## ALS-ST3 式文とコメント`.
- **A prefix belongs to exactly one chapter file** (a chapter may own several
  prefixes; a prefix never spans chapters). This injection is what makes a
  `spec = "ALS-…"` key unambiguous — 12 duplicated ids across chapters were
  found and renamed on 2026-08-20, and one of them was three ways ambiguous
  with its citations already split across all three meanings.
- The registry (the style gate parses this table):

| Prefix | Chapter | Subject |
|--------|---------|---------|
| B  | bounded.md | the bounded profile — `@bounded` functions (ADR-0017) |
| C  | collections.md | list / map / set semantics |
| D  | data-formats.md | JSON, Value, regex, binary decode |
| DT | deterministic-time.md | time algebra, fan budgets, deterministic race |
| DL | expressions.md | declarations (`Decl::*`) |
| E  | expressions.md | expressions (`ExprKind::*`) |
| I  | implementation.md | implementation-defined limits |
| M  | semantics.md | pattern matching, binding, generics, loops |
| R  | runtime.md | process, fs, http, streaming, display |
| S  | strings.md | codepoint semantics of `string.*` |
| ST | expressions.md | statements (`Stmt::*`) |
| T  | text-and-numbers.md | numeric/text conversion, bounds, assert |

- Ids are **permanent**: never renumbered, never reused. Splitting a section
  mints new ids; merging keeps ONE id and retires the other permanently (the
  retired meaning may never come back under another id — 2026-08-20: the
  expressions-chapter `ALS-S4` (`Stmt::Comment`) merged into `ALS-ST3`;
  `ALS-S4` now names only the strings-chapter section it always also named,
  and `Stmt::Comment` is `ALS-ST3` forever).

<a id="writing-a-section"></a>

## Writing a section

- **One observable behaviour per section.** Observable = stdout, stderr,
  exit code, or a checker verdict (accept / reject with a pinned code).
  Nothing an implementation could not be tested against.
- **State what IS, in the indicative.** No plans, no "should", no 「予定」,
  no hedging. Unimplemented design lives in an ADR or a roadmap, not here.
- **Evidence is named in the section**: the citing contract(s) do the
  certifying, and the section names its test paths (`テスト:` line). A
  normative statement without executable evidence does not exist.
- Every section must be cited by ≥1 contract (`spec =` key) and every
  contract must cite a real section — both directions are gated
  (`scripts/check-contracts.sh`).
- Divergence between targets, when intentional, is **committed data** (a
  table of per-target values under a named contract), never avoidance and
  never a prose fudge.
- Chapters open with `> Last updated: YYYY-MM-DD`.

<a id="validation-record-do-178c-a-3"></a>

## Validation record (DO-178C A-3)

The style gate holds what a machine can check. Whether a section is
**accurate, consistent and complete** is a reviewer's judgment, and a
judgment that is not recorded does not exist. `proofs/als-validation.toml`
carries one row per reviewed section — `id`, `hash` (the section text the
review covered; `bash scripts/check-als-validation.sh --stamp ALS-<id>`
prints it), `reviewed` (date), `by`, `independent` (`yes` only for a
reviewer who is not the author), `verdict` (`accurate`, or `revise` with an
`issue = "#N"`). A text change makes the row STALE and the gate red until
the section is re-reviewed. Sections without a row are **unvalidated**, a
shrink-only ceiling: a new section lands with its row, or raises the
ceiling by exactly one with a justification in the PR. The gate is
`scripts/check-als-validation.sh`. Today every review on record is by the
author (QUALIFICATION.md, limitation 1) — the ledger makes that visible
rather than pretending otherwise.

<a id="ratchets-the-four-direction-law"></a>

## Ratchets (the four-direction law)

A shrink-only ledger in this repository must fail in all four directions,
not one: (1) the count may not grow without an authored, named justification;
(2) a row whose reason has stopped being true fails; (3) a row whose subject
no longer exists fails; (4) a measurement of zero measures fails — a broken
instrument is not a pass.

<a id="forbidden-vocabulary-normative-chapters"></a>

## Forbidden vocabulary (normative chapters)

`TODO`, `FIXME`, 「たぶん」 (probably), 「そのうち」 (eventually), 「予定」 (planned) — a normative sentence
that needs them is not ready to be normative.
