<!-- description: Effect inference engine with 7 categories and checker integration -->
<!-- done: 2026-03-19 -->
# Effect System — Phase 1-2

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](effect-system-phase1-2.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](effect-system-phase1-2.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](effect-system-phase1-2.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/effect-system-phase1-2.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


**Completed:** 2026-03-19
**PR:** #49

## Implementation

### Phase 1: Effect inference engine
- `EffectInferencePass` Nanopass — 7 effect categories (IO, Net, Env, Time, Rand, Fan, Log)
- stdlib module → effect mapping (fs→IO, http→Net, env→Env, etc.)
- Direct effect collection (Module call + Named call + fan expression)
- Call graph construction + transitive effect inference via fixpoint iteration
- Added `effect_map` field to IrProgram
- `almide check --effects <file>` CLI command
- ALMIDE_DEBUG_EFFECTS=1 for analysis output

### Phase 2: Self-package restriction (Security Layer 2)
- Parse `almide.toml [permissions] allow = ["IO", "Net"]`
- Violation detection + hint display with `almide check --effects`
- Integrated into regular `almide check` — auto-detects violations when [permissions] present
- Added `permissions` field to `Project` struct in `project.rs`

## Remaining at completion (2026-03-19)

The original `active/effect-system.md` plan is no longer present. For the
current capability roadmap, see [Effect System & Capability](../../../../../roadmap/active/effect-system-capability.md).
The phase numbering below belongs to this completion record.

- Phase 3: Dependency restriction (`[dependencies.X].allow`) → 2.x
- Phase 4: Type-level integration (merges with HKT Foundation Phase 4) → 2.x

## Tests
- 5 internal tests (module_to_effect, runtime_name_to_effect, format_effects)
- 110/110 almide tests
