<a id="v1-backlog--trust-spine-の残件一覧優先度付き"></a>

# V1 Backlog — remaining trust-spine work, prioritized

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](v1-backlog.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-backlog.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-backlog.zh-CN.md)

> Non-normative source-pinned historical reader snapshot. The original path remains authoritative and unchanged for its consumers. No automatic synchronization or independent execution. Accepted proposals, completed checkboxes and remaining work retain their source-time status, not a newly checked current implementation status. The July 4 summary says Tier 2 is closed while listing deliberate Tier 2 deferrals below; both statements are retained. “Medical-grade” and byte-identity claims are source language, not medical certification or an independent safety proof. Old v0-native blockers and handoff directions are not asserted to remain current or instructions executed here. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-backlog.md)

> Original metadata summary (inactive reader note): The v1 trust-spine backlog — soundness, drop completeness, self-host surface, walls.

> Updated 2026-07-04. Tier 1 (soundness), Tier 2 (drop completeness), and Tier 3's lightweight self-host work are closed. Two additional pre-existing soundness bugs were caught and corrected (cert double move / silent miscompile of guard early exit). The remaining heavyweight work (fast-exp, spec wall buckets, contracts depending on v0-native) is left below with precise resumption points.

<a id="-クローズ済み2026-07-04"></a>

## ✅ Closed (2026-07-04)

- **T1-1 mir>ir breach** (wav.almd::find_chunk_at): corrected lowering the opt-tuple fold body 3 times, once per component, to 1 time/iteration. BUG gate 0.
- **T1-2 `f0` type error in anon-record drop**: bind the List-element drop field with a structural type (`List[{…}]`). Unannotated anonymous records pass render.
- **T2-3 Matrix record-field leak**: add Matrix / List[Matrix] arms to `record_drop_field_frees` (generate `__drop_matrix` / `__drop_list_matrix`, sweep rows with rc_dec).
- **T3-5 Lightweight self-host**: list.enumerate/zip (scalar + flat-heap rc), map.entries/from_list (skv repr), list.get_or_hshare/take_hshare, fs.read_bytes, bytes.read_length_prefixed_strings_le. nn unlinked sites 29→12.
- **T4-10 List-field extraction in variant statement match**: extend ctor patterns to all heap fields, including List[…], plus record-ctor patterns (`Data { seq, .. }`), execution of for/while loops at arm tails, and a wall for deferred-subject match. Also includes tagged construction for record-ctor values (fixing a miscompile from treating them as plain records). Spec walls 6→2.
- **(Caught) cert double move**: in `let base=…; if c then …+base else base`, the `EndIf` val-move double-counted the already-Dup'ed value in the else arm, causing ownership REJECT (`iammd`). Runtime was already sound (`Consume` is a move marker and does not rc_dec). Fixed the cert by excluding consumed values from val-move. T4-10 expanded the in-profile surface and exposed it.
- **(Caught) silent miscompile of guard early exit**: `guard cond else E; …` was deferred as always-continue, making the `!cond` path wrong (`guard len>0 else err();
  ok(x)` returned ok on empty input). V1 has no early-return control, so this became an honest wall. Zero parity-baseline regressions (the byte-verified set had no guard functions).

<a id="tier-2-残--drop-完全性"></a>

## Tier 2 remaining — drop completeness

1. **Option[Matrix] (deliberately deferred)**. Defunc find intentionally walls matrix-shaped payloads (two-level option drop is not ready). No actual use. Before opening it, add the equivalent of `__drop_opt_matrix` to the generated side, then remove the gate.
2. (Record) **List[flat variant] ctor fields** wall on the construction side, and the generated side emits no free either—currently consistent (never built). Open both sides together.

<a id="tier-3-残--実行面の拡張重量級"></a>

## Tier 3 remaining — expansion of the execution surface (heavyweight)

3. **The 7-function fast-exp family (heavy; accuracy verification required)**: `matrix.softmax_rows` / `gelu` / `swiglu_gate` / `multi_head_attention` / `masked_multi_head_attention` / `rope_rotate` / `from_q1_0_bytes`. V0 **sums in lane order** using almide-kernel SIMD fast-exp, so bit-exact transcription requires faithfully reproducing the exp_pd_{wasm,neon} polynomial (6 Horner terms), 2-lane partial sums, and libm exp for the odd tail (reference implementation: crates/almide-kernel/src/silu.rs::exp_pd_neon; follow the math_exp.almd precedent). **Do not rush and get ULP wrong, because this is medical-grade work.** A prerequisite for the day nn runs end-to-end on v1. Currently 8 sites are honest walls.
4. **list.enumerate_h / zip_h (rich elements)**: enumerating `Complex=(Float,Float)` elements leads into a heap-tuple pipeline that extracts `e.1` from `(Int, tuple)` via filter→map (4 fft helper sites, honest walls). Low ROI, outside the inference path.

<a id="tier-4-残--spec-壁バケット既存-roadmap-路線"></a>

## Tier 4 remaining — spec wall buckets (existing roadmap tracks)

5. **Model guard-else early return** (39 sites, currently honest walls): introduce early-return control to v1 and structure `guard cond else E; rest` as `if cond then rest else E` to restore in-profile status. Recovers the cases turned from miscompiles into walls in this work.
6. About 32 `fan.*` capability cases → effectful-27-blueprint.
7. About 15 ResultOk/Err/OptionSome argument-materialization cases. **Note**: a `Result[String,String]` `ok(payload)` tail ctor is easy, but functions containing guards (the majority) first need T4-5 early return. Guard-free if/else forms already lower.
8. Effectful stdlib (`datetime.parse_iso` / `env.os` / `env.temp_dir` / `fs.stat` / `http.*` / `net.*` / `process.*`): the floor of capability declarations. As with fs.read_bytes, start with what can be opened through the admitted-effectful list and corresponding prim.
9. About 10 other materializations: EmptyMap/MapLiteral/List arguments, capturing lambdas in call-arg positions, and HOFs over opaque fn-values.

<a id="tier-5--契約計測"></a>

## Tier 5 — contracts and measurement

10. **The matrix.mul / from_bytes OOB contract ledger (blocked by v0-native dependency)**: v1 already conforms to the native contract (mul in ascending k order, OOB produces an all-zero matrix), but matrix fixtures cannot be byte-verified by the output-parity gate because `almide run` (native) is broken (resulting in v0fail). **Fixing v0-native matrix codegen (handoff below) is prerequisite.**
11. **Coverage**: latest 66.30%. The largest uncovered surface is control_p5 (the defunc engine), followed by tail.rs / binds_p2 / binds_p4. Measure with `bash proofs/coverage.sh`.

<a id="v1-レーン外引き継ぎ事項--ここでは直さない"></a>

## Outside the v1 lane (handoff — do not fix here)

- **Broken v0-native matrix codegen (pre-existing on develop-v1)**: generated Rust's `bridge::AlmideMatrix` became an enum while vendored glue (transpose and others) still uses the old `Vec<Vec<f64>>` API. This mismatch makes matrix code under `almide run` report “codegen produced invalid Rust.” It appears to be a transition state in the flat ABI / burn rework (a parallel session's area), so **hand off to the owning session**. For now, the v1 matrix parity oracle uses `almide run --target wasm` (byte-identical guarantee). The Tier 5-10 contract fixture waits for this to be resolved.
