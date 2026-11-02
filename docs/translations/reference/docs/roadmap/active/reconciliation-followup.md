# Reconciliation follow-up — develop items deferred from v0.28.0

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](reconciliation-followup.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](reconciliation-followup.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](reconciliation-followup.zh-CN.md)



> Source: [docs/roadmap/active/reconciliation-followup.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/reconciliation-followup.md), pinned to `35297a612fb29e3f78f6587aba253aa91d51f466`. This is a non-normative reader translation of a historical roadmap snapshot; it is not automatically synchronized. Dates, status, estimates, measurements and plans refer to the source record, not a new implementation or independent verification. Historical approval requests and recommended commands are not authorization to execute them now. The original is authoritative.

> Source description metadata: v0.28.0 reconciliation follow-up: deferred develop commits for 0.28.1.

> Recorded 2026-07-06. The v0.28.0 [reconciliation](../../../../../roadmap/active/v1-release-path.md) shipped after **reapplying all capabilities from develop (0.27.13) to the v1 line**. This document records the **develop commits deliberately deferred** at that time (not capabilities), and the policy for incorporating them in 0.28.1 and later.
> **Related**: [v1-release-path](../../../../../roadmap/active/v1-release-path.md) / [v1-v0-parity](../../../../../roadmap/active/v1-v0-parity.md).

## Premises

- v0.28.0 has **already reapplied every substantive fix from develop** (mut-record writeback, wasm aliasing, E026, native cdylib, http, npm removal, dep 0.252, and others). Everything “v0 can do” is included as a capability.
- The remaining items below are **not capabilities** (performance optimizations / docs / CI infrastructure / cosmetic refactoring). They remain in develop's history (`git log origin/main`) and can be cherry-picked at any time.
- Reconciliation used `git merge -s ours` to record history while keeping the v1 tree. These changes are merely “not in the tree”, not “lost”.

## Candidates for 0.28.1 (can be cherry-picked)

### c2f87402 — WASM SIMD performance (q1_0/transpose/fold)

- **Nature**: performance optimization. The **correctness core (variant-eq tail-zero) was demonstrated to be redundant in v1** (covered by develop-v1's iter_scope free-list reset + iter-scope escape; the leak-test passed through WASM + PCC ownership ACCEPT). Thus only **SIMD matrix acceleration** remains.
- **Decision material**: develop-v1 already has its own SIMD implementation (matrix fast-exp, Q1_0 dequant). **Investigate how much it overlaps** c2f87402's q1_0/transpose/fold SIMD. If redundant, it is unnecessary; if differences remain, port them for v0-wasm performance on nn/qwen workloads.
- **Effort/risk**: 12 files, SIMD emission in emit_wasm. Medium priority because this is performance work that does not affect correctness. First measure develop-v1's existing SIMD coverage, then decide.

### c5e13f61 — record the certification-grade roadmap's terminal goal

- **Nature**: docs only. An addition to `docs/roadmap/active/certification-grade.md`.
- **Effort**: minimal. A cherry-pick may conflict, but can be resolved manually.

### 13691202 — nightly fuzz harness child-process group reaping + bounded output

- **Nature**: CI/test infrastructure hardening (reap orphan child processes, cap output).
- **Effort**: small. Fuzz harness scripts only. Worth incorporating for CI stability.

## Permanently skipped

### c3648caa — remove WASM emitter magic numbers (named constants)

- **Nature**: cosmetic refactoring (49 files). Introduces named constants assuming develop's ~26-file structure.
- **Reason**: develop-v1 has its **own structure splitting emit_wasm into 109 files**. develop's named-constant refactoring does not fit v1's structure and was the main cause of emit_wasm conflicts during reconciliation. If v1 needs equivalent readability improvements, do them **separately** to fit v1's structure (do not transplant develop's changes).

## Acceptance policy

- Even when incorporating these items, **maintain honest-wall + all gates green** (parity 210 MISMATCH=0, PCC ACCEPT, mir/spec/cargo/drift/contracts/docs-gen). Performance changes must pass the byte-identical gate.
- At a version bump, **update llms.txt's "Current stable" narrative**, or `docs_gen_check_passes_on_clean_checkout` fails (docs-gen's version-description check).
