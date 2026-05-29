<!-- description: WASM engine v2 correctness bug catalog, by category, with detection and remediation -->
# WASM Engine v2 — Correctness Bug Catalog

Tracks **correctness** defects in the v2 WASM engine (`emit_wasm/engine/`),
grouped by *bug type*. The key lesson driving this doc:

> **"Builds under v2" ≠ "correct under v2".**

The fallback-based gap analysis (does a program lower without hitting
`Op::Unsupported`?) cannot catch a program that lowers to a *valid-but-wrong*
binary. That class is the dangerous one — it ships silent data corruption.

## Detection

`scripts/wasm-v2-diff.sh` — the differential correctness gate. Builds every
`fn main` `.almd` under both v2 and the legacy emitter, runs each under
`wasmtime`, and fails on any output / exit-code mismatch for a program v2 fully
lowered. Re-run after **any** new v2 lowering. Current baseline:
`ran=5 fell-back=27 skipped=10 bugs=0`.

A bug here is only meaningful where v2 *actually ran* (did not fall back) —
expand v2 coverage and the gate exercises more programs.

---

## Category 1 — Silent offset default → memory corruption  *(FIXED)*

**Shape:** a field/element **offset** computation returns `None` for an
unhandled type and the caller does `.unwrap_or(0)` / `.unwrap_or(index*8)`, so a
*wrong offset* is emitted as a valid instruction. Reads/writes hit the wrong
bytes; the binary validates and runs but produces wrong values.

**Found:** named record types (`type Point = {x,y}`) were `Ty::Named`, which
`record_field_offset`/`record_total_size` did not resolve → **every field
collapsed to offset 0** in both construction and access. `records_variants`
printed `4 4 / 32 / 2 6` instead of `3 4 / 25 / 1 5`.

**Fixed:**
- `build_module` threads `program.type_decls` → `LowerCtx.record_types`
  (`RecordLayouts`); `record_fields` resolves `Ty::Named`. (commit 9a0eba3f)
- Root-cause hardening: `Member` / `TupleIndex` / `Record`-literal now emit
  `Op::Unsupported` (→ legacy) when the type can't resolve to concrete offsets,
  instead of guessing. v2 only emits record/tuple code it can prove correct.
  (commit cc022f28)

**Residual:** `lower.rs` Record/Spread loops still hold defensive
`record_field_offset(...).unwrap_or(0)` reached only after the type already
resolved — harmless, candidate for cleanup.

---

## Category 2 — Silent element/payload-type default → wrong load/store width  *(IN PROGRESS: "Unknown sweep")*

**Shape:** an **element or payload type** lookup returns `None` (type leaked as
`Ty::Unknown`) and the caller does `.unwrap_or(Ty::Int)` / `.unwrap_or(Ty::String)`.
The wrong type → wrong WASM width (i64 vs i32) at load/store → corrupted values
or stack-type mismatch. Lower blast radius than Category 1 (width, not offset)
but the same silent class.

**Sites (audit):**
- `lower.rs`: ForIn element type (`unwrap_or(Ty::Int)`); match-destructure
  sub-types (Tuple element / List element / Some/Ok/Err payload); ConcatList
  element width (`unwrap_or(8)`).
- `intrinsics.rs`: `list_contains`, `result_map`, `result_map_err`, `option_map`,
  `option_to_list`, `list_reverse`, `list_filter_map`, `list_flat_map`,
  `list_map` — all default an element/payload type to `Ty::Int`/`Ty::String`.

**Plan:** convert each to **honest rejection** — when the element/payload type is
unresolved (`Unknown`/`TypeVar`), return `Op::Unsupported` (→ legacy) rather than
guessing a width. Guard against over-fallback: verify each site is actually
reachable with an unresolved type before converting (the differential shows 0
current bugs, i.e. real programs carry resolved types).

---

## Category 3 — Coverage gaps  *(honest `Op::Unsupported` → legacy; NOT bugs)*

These already fall back correctly; listed for completeness / coverage tracking.

| Label | Meaning | Blocks |
|-------|---------|--------|
| `runtime-call` | unregistered RuntimeCall intrinsic (list.push/pop/take/drop/slice/enumerate/repeat; json/value) | json_value, list_comprehensive, memory_stress |
| `stdlib-call` | non-intrinsic stdlib fn via `CallTarget::Module`/`Named` | cross_module_spread |
| `lambda-value` / `Lambda` | closures as **values** (call_indirect) | closures_hof, control_flow |
| `MapLiteral` / `EmptyMap` / `MapAccess` | `{}` map-literal syntax, `m[k]` indexing | — |
| `Try` / `ToOption` / `OptionalChain` | `?`, Result→Option, `?.` | — |
| `unresolved-fn` / `unresolved-call` / `unhandled-expr` / `unhandled-stmt` | catch-alls | — |

`SpreadRecord` is implemented (commit 9a0eba3f); its label only triggers on an
unresolvable record type.

---

## Invariant we are converging on

> The v2 engine must emit a binary **only** when it can prove that binary is
> correct for the given types. Whenever a type can't be resolved to a concrete
> layout (offset or width), reject honestly (`Op::Unsupported` → legacy
> fallback) rather than guess. Coverage grows by *adding proven lowerings*, never
> by guessing.
