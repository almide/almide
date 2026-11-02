# v1 → develop Reflow Strategy

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](v1-develop-reflow.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-develop-reflow.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-develop-reflow.zh-CN.md)



> Source: [docs/roadmap/active/v1-develop-reflow.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-develop-reflow.md), pinned to `35297a612fb29e3f78f6587aba253aa91d51f466`. This is a non-normative reader translation of a historical roadmap snapshot; it is not automatically synchronized. Dates, status, estimates, measurements and plans refer to the source record, not a new implementation or independent verification. Historical approval requests and recommended commands are not authorization to execute them now. The original is authoritative.

> Source description metadata: Historical July 2026 proposal for integrating develop-v1; not a pending execution request.

## Historical branch-integration decision record

This is the July 3, 2026 `develop-v1` integration proposal. Its requests for
an owner's Go and branch-local execution are historical, not pending approval
for the current checkout. MIR, self-host sources and proof infrastructure are
already in this repository; the current shipping Wasm route is
[structural](../../../../../../src/wasm_route.rs). The original sequencing and risk
assessment below are retained, not reissued as instructions.

> **Position**: A proposed policy for when and in what units to bring v1 results—the MIR trust spine, self-host sources and evidence gates, confined to develop-v1—back into develop. **Execution awaits the user's Go decision**. This document organizes material for that decision.

## Inventory of assets to bring back (as of 2026-07-03)

| Asset | Value to develop | Risk | Proposal |
|---|---|---|---|
| **Evidence gates** (toolchain stamp / LC_ALL=C + solo-retry parity / ratchet-separated lefthook / coverage.sh / three-way comparison) | v0 verification has the same gaps (locale collation and binary drift are branch-independent) | Almost zero — gates do not touch implementation | **Wave 1**: can be cherry-picked as-is; immediately useful to v0 alone |
| **flight-evidence-gaps / TRUSTED_BASE boundary diagram / roadmap documents** | Audit ledgers are shared assets across all branches | Zero | **Wave 1** |
| **Compiler fixes also relevant to v0** (равi-recursive unify guard / structural-twin merge / cross-module @inline_rust — frontend/codegen layers already verified in v1) | Fix real v0 bugs | Medium — must reverify with v0's full suite | **Wave 2**: 1 fix = 1 PR, run through develop CI |
| **Entire MIR spine** (almide-mir crate + registry self-host sources + render_program) | v1's core; bringing it into develop means it coexists as a “second backend” | High — doubles develop CI time and maintenance surface | **Wave 3**: together with the decision that “v1 replaces v0's parent”; until then it stays on its branch |
| **stdlib self-host (.almd)** | Unused by the v0 route (registry-only) | Low (no v0 impact), but meaningless alone | Together with Wave 3 |

## Ordering principles

1. **Evidence first** (Wave 1): merge verification infrastructure before implementation. This increases trust in v0's green status and provides a net for subsequent implementation integration.
2. **1 fix = 1 PR** (Wave 2): split fixes for v0 bugs found during v1 work into individually explainable units, independent of v1. The governing directive was “no v0 backports needed”, so explicit approval of this policy change is required.
3. **The spine on replacement day** (Wave 3): bringing back the MIR spine is inseparable from deciding to “retire emit_wasm and make the render path the production route”. Until then, develop-v1 remains its home.

## Unresolved decisions (awaiting user input)

- When to execute Wave 1 (possible at any time — CI additions only).
- Review the target list for Wave 2 (the equi-recursive guard changes v0 checker behavior; v0 revalidation with the full spec set + org sweep is a prerequisite).
- Trigger conditions for Wave 3 (examples: parity baseline exceeds 90% of the executable spec set, org wall=0 in 26/26, Matrix/closure brick complete).
