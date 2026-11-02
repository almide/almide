[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> Non-normative human reader snapshot of canonical agent instructions, fixed to the linked source. CLAUDE.reader is deliberately not an auto-loaded instruction filename. Original files stay unchanged. No new instruction layer, execution authority, automatic synchronization, or independently verified implementation guarantee. Source rules and qualifications are preserved; code remains verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/CLAUDE.md)

<a id="spec-rules"></a>

# Spec Rules

<a id="仕様の権威と実装の証拠"></a>

## Specification authority and implementation evidence

- Update specs together with implementation. When changing code, update the corresponding spec in the same commit.
- Follow [SPEC normative status](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md). If ALS and the implementation disagree, treat it as implementation nonconformance. Do not rewrite normative contracts merely from observations of the implementation.
- State verification-test paths and the scope checked in specs. A passing test is evidence for that case, not proof of the entire specification's correctness.
- Explicitly state unverified areas and contradictions between specs; distinguish adopted rules from current implementation status.
- Correct outdated descriptions of current behavior. Do not delete ADRs, history, or normative contracts merely because they differ from the implementation.
- Sections describing implementation status must state implemented behavior and the checked scope; do not write guesses or plans as current guarantees.
- Put unadopted future proposals in `docs/roadmap/`. Preserve adopted ALS norms even where unimplemented or nonconformant, explicitly identifying that status.

<a id="フォーマット"></a>

## Format

- Put `> Last updated: YYYY-MM-DD` at the start.
- State verification-test paths in each section: “Tests: `spec/integration/modules/diamond_test.almd`”.
- Accompany specifications with code examples that can be pasted unchanged into an `.almd` file and compiled.

<a id="現在の-spec"></a>

## Current specs

| File | Contents |
|---|---|
| `language.md` | Language specification: types, declarations, expressions, statements, patterns, operators, visibility, comments |
| `type-system.md` | Type system: inference, generics, records, variants, protocols, type aliases |
| `effect-system.md` | Effects: fn vs effect fn, explicit propagation (`!` only — ADR-0008), fan, permissions, E006/E007/E008 |
| `codegen.md` | Code generation: Nanopass pipeline, templates, Rust/WASM output, module naming |
| `cli.md` | CLI: all commands, options, error codes, Legacy Mode |
| `module-system.md` | Modules: import, submodules, diamond dependencies, visibility, @extern |
| `edit-locality.md` | Edit locality: L1–L3 invariants, L4–L5 declaration frame and composition, enforcement map, counterexample hunt (2026-08-15), side conditions, feature gates |
| `package-system.md` | Packages: dependency management, MVS, version coexistence, module boundaries |
| `result-option-effect.md` | Context-specific Result/Option and `!` rules, lambda boundaries |
| `effect-fn-call-semantics.md` | Effect calls and explicit propagation |
| `scoped.md` | Scoped borrowing and verification scope |
| `perceus.md` | RC optimization model, implementation, and evidence boundaries |
| `als/` | ALS normative descriptions and known consistency notes |
