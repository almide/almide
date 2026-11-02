# Fan v2 — reference examples

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](fan-v2-examples.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fan-v2-examples.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](fan-v2-examples.zh-CN.md)



> Source: [docs/roadmap/active/fan-v2-examples.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2-examples.md), pinned to `35297a612fb29e3f78f6587aba253aa91d51f466`. This is a non-normative reader translation of a historical roadmap snapshot; it is not automatically synchronized. Dates, status, estimates, measurements and plans refer to the source record, not a new implementation or independent verification. Historical approval requests and recommended commands are not authorization to execute them now. The original is authoritative.

> Source description metadata: Reference examples for the fan v2 surface: every head, edge semantics, diagnostics.

> This index preserves design examples from the introduction of fan v2 and the plan to transcribe them into fixtures.
> The `.almd` files in `fan-v2-examples/` are historical sketches, not the authority for current grammar or diagnostics. Do not copy old `any` / `race` examples into current programs.

<a id="1-現在の参照先"></a>

## 1. Current references

- Grammar and semantics: [SPEC §13](../../../../../SPEC.md#13-concurrency-fan).
- Selection names and clocks: [ADR-0028](../../../../../adr/0028-fan-selection-names-and-cost-contract.md) — `fan.any` becomes `??` / `list.find_map`; `fan.race` becomes `fan.min`, which requires a clock.
- Implemented surface and rejected forms: `tests/fan_surface_matrix_test.rs`.
- Regression examples: `spec/wasm_cross/fuel_bounded_*.almd`, `fuel_race_*.almd`, and others. An old `race` remaining in a filename is not the same as old syntax being usable.
- `fan.timeout(duration.ms(n)) { body }` exists as a body form. The Stage 4/tombstone items below describe the original implementation order, not the current list of unimplemented features.

Code is not recopied here; executable fixtures are authoritative.

<a id="2-歴史的な設計例の索引"></a>

## 2. Index of historical design examples

| File | Contents | Original stage |
|---|---|---|
| [bounded.almd](../../../../../roadmap/active/fan-v2-examples/bounded.almd) | Totalization idiom, single body allowing `let`, `0ms`/divergence edges, min-cap nesting | Stage 2 |
| [race.almd](../../../../../roadmap/active/fan-v2-examples/race.almd) | Portfolio solving, **behavior table for all 8 cases** (ties, trap visibility window, exhaustion), two-grammar parsing, mapper form | Stage 3 |
| [any.almd](../../../../../roadmap/active/fan-v2-examples/any.almd) | Configuration fallback chain, sequential mirror attempts; semantics note: “fallback, not concurrency” | Wave 1 |
| [settle.almd](../../../../../roadmap/active/fan-v2-examples/settle.almd) | Block form for heterogeneous tuples, mapper form collecting all errors | Wave 1 |
| [fan_all_unchanged.almd](../../../../../roadmap/active/fan-v2-examples/fan_all_unchanged.almd) | Control for the unchanged surface at introduction; surviving reference points for C-004/C-199/C-200 | Existing surface at introduction |
| [diagnostics.almd](../../../../../roadmap/active/fan-v2-examples/diagnostics.almd) | 6 examples that **correctly do not compile**, with expected diagnostics (pure constraint, ticks label, thunk tombstone, E008, etc.) | Wave 1–Stage 3 |
| [migration.almd](../../../../../roadmap/active/fan-v2-examples/migration.almd) | v1 → v2 side-by-side translations (any/settle/race/timeout); note on removal of the funcref wall | Wave 1–Stage 4 |

<a id="3-当初の-fixture-転写計画現在の網羅性一覧ではない"></a>

## 3. Original fixture-transcription plan (not a current coverage inventory)

| Example | Destination | Contract to pin |
|---|---|---|
| bounded.almd basic form and edges | `spec/wasm_cross/fuel_bounded_*.almd` | bounded exhaustion equivalence (new C-NNN) + consumed ratchet |
| 8 rows of race.almd's behavior table | `spec/wasm_cross/fuel_race_*.almd` (1 fixture per row) | Winner equivalence, trap visibility window, source order for ties |
| bounded.almd nesting | `spec/wasm_cross/fuel_nest_*.almd` | min-cap and streaming consumption |
| any.almd / settle.almd | `spec/lang/fan_any_block_test.almd`, etc. (Wave 1, no fuel needed) | any/settle form contracts |
| All 6 diagnostics.almd cases | Fixtures in `tests/diagnostics/` (one per E0xx) | Diagnostic text |
| fan_all_unchanged.almd | Existing fixtures remaining unchanged is itself a regression gate | C-004 / C-199 / C-200 |

When adding or updating current examples, update the fixtures in `spec/` and the current SPEC.
Merely appending to these historical sketches is not acceptance evidence for the current grammar or contracts.
