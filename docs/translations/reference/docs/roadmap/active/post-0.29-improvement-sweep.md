# Post-0.29 Improvement Sweep

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](post-0.29-improvement-sweep.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](post-0.29-improvement-sweep.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](post-0.29-improvement-sweep.zh-CN.md)

> Non-normative source-pinned historical reader snapshot. The original path remains authoritative and unchanged for its consumers. No automatic synchronization or independent execution. Accepted proposals, completed checkboxes and remaining work retain their source-time status, not a newly checked current implementation status. This is a post-0.29.0 ledger, not current issue tracking. Preserve the original mixed status: #785 remains unchecked while #782 calls it done; the ceangal heading says 3/6 while a later row says 4/6. A checked row can still say full gates are running. Publication, CI and gate instructions are historical text, not actions performed here. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/post-0.29-improvement-sweep.md)

> Original metadata summary (inactive reader note): Post-0.29.0 improvement sweep — every open gap made into an issue and ordered, from presentation fixes to v0 wasm retirement.

An all-around inventory immediately after shipping 0.29.0 (verified by default). Turn every improvement candidate into an issue and work through them by priority. The issues are authoritative for each item's details and acceptance criteria; this document is the ledger of ordering and current position.

<a id="すぐ直す顔の修繕衛生"></a>

## Fix immediately (presentation and hygiene)

- [x] [#778](https://github.com/almide/almide/issues/778) Stale and self-contradictory README Project Status table (Tests 240/232→285, MSR 23/25 vs 30/30, verified-default absent).
- [x] [#779](https://github.com/almide/almide/issues/779) Ruling on `almide.lock` (commit or gitignore, consistent across the org).
- [x] [#780](https://github.com/almide/almide/issues/780) Switch the org-trust-status headline to a resolved-strict sweep—honest first measurement: 13/28 repos clean, 24 modules wall, 11 frontend-rejected (#783 class). New real walls in porta/almai/toml are the next lowering targets.
- [x] [#741](https://github.com/almide/almide/issues/741) Add `math.tanh` / `math.atan`: implement the vendored libm transcription on 3 paths, native/v0-wasm/v1-self-host; C-134 + 3000-point differential fuzz. Also found and fixed the trap where `0.0 - t` loses IEEE negative zero (sign-bit bxor).

<a id="正しさnative-側の非対称を塞ぐ"></a>

## Correctness (close native-side asymmetries)

- [x] [#757](https://github.com/almide/almide/issues/757) Nested variant-tag panic—root fix: the `matches!` guard in #610's box rewrite erased non-boxed inner tags/literals into `_`. Preserve all refutable constraints with guard_shape recursion. Extend C-070 + `nested_variant_tag_box.almd` (full gates running).
- [x] [#753](https://github.com/almide/almide/issues/753) Debug-profile ANF trap—not reproducible on current develop (resolved by lowering fixes in the 0.29 cycle). Verified both fixtures with a debug binary and closed. Debug-only postcondition execution is by design (documented in pass.rs).
- [x] [#783](https://github.com/almide/almide/issues/783) Name-pinning recurrence—root fix: repair used `map_children` and skipped Bind ty inside ForIn/While bodies (`Vec<IrStmt>`). Rewrite with canonical `IrMutVisitor`, sharing the checker's traversal family (resolving the enumeration-drift class). Also add location granularity to the gate's where_.
- [x] [#784](https://github.com/almide/almide/issues/784) Unknown in anonymous-record fields—the true cause was an Unknown seed for **unannotated negative-literal module constants** (`infer_literal_type` lacks a Unary branch). The fix lets the ceangal suite compile (remaining issues are #433-class cells and individual test failures). Regression: add 2 tests to cross_module_let_test.
- [ ] [#785](https://github.com/almide/almide/issues/785) Module constants initialized by calls also leak Unknown—the refresh path (re-registering top_lets in check_decl) does not rescue readers. This is the core contract.

<a id="戦略級次の大玉"></a>

## Strategic work (the next major items)

- [ ] [#782](https://github.com/almide/almide/issues/782) **Phase 3: retire the v0 wasm emitter**—major progress on prerequisites: frontend bugs #783/#784/#785 done; **wasm:skip stale sweep complete** (23→15 markers; the wasm path had already caught up for 8: fn_ref/recursive generics/closure-capture/borrow_hoist/mut params×2/random(C-112)/url_decode). Real remaining work: [#786](https://github.com/almide/almide/issues/786) invalid wasm generation for cross-module Unit effect fns, 7 structural-native markers, and choosing the oracle successor.
- [~] [#764](https://github.com/almide/almide/issues/764) Native trust-spine—**Float slab shipped** (real f64, byte agreement at 3 points, differential 4 rows). Remaining: records (DynList discovery drastically shrank the design—reuse ListLit + 1 FieldGetScalar op; 6 emit sites already fixed in the ledger)/variants/closures → rung 6 → default flip.
- [ ] [#617](https://github.com/almide/almide/issues/617) RcCow for Matrix/Bytes (remove the deep-clone tax while preserving value semantics).

<a id="品質基盤コツコツ級"></a>

## Quality foundations (steady work)

- [~] [#566](https://github.com/almide/almide/issues/566) Coverage ratchet—**mechanism shipped**: baseline comparison in `proofs/coverage.sh` (FAIL if TOTAL line % falls; `--update` only raises it) + nightly CI job (coverage-ratchet in fuzz-nightly.yml). Remaining: seed the baseline (run coverage.sh after full gates) and MC/DC (the flight scope in #566 itself).
- [~] [#781](https://github.com/almide/almide/issues/781) cog>100 burndown—**first worst item complete**: split `list_heap_call_name` (cog 324) into a per-module router + 7 helpers, verified by wall-list byte equality. Continue the remaining 14 in the ledger's worst-first order.

<a id="ceangal-suite-burndown783784ufcsrecord-order-で-36-まで前進"></a>

## Ceangal suite burndown (#783→#784→UFCS→record-order brought progress to 3/6)

- [x] Cross-module UFCS could not resolve qualified type names (`count.get()` E001): deriving the defining module in both checker and lowering used a suffix scan assuming bare names. Derive qualified names directly with `rsplit_once('.')`.
- [x] E005 from anonymous-record field order: Record×Record in `Ty::compatible` compared a positional zip (asymmetric with the solver's by-name comparison). Unify on by-name set comparison. Also close the single-file hole in `mix({ g: .., r: .. })`.
- [x] [#787](https://github.com/almide/almide/issues/787) ScratchAllocator overflow—resolved with minimal tier 8/4/4 (two-pass hwm emit remains the endgame).
- [x] [#788](https://github.com/almide/almide/issues/788) Fix mono's 2-layer bug: (1) alpha-renaming free variables (module globals), (2) failure to unify qualified/bare Named (`cell.get` unspecialized—the actual cause of the recorded #433-class cell.get problem). `ALMIDE_MONO_DEBUG` instrumentation in-tree, regression cell added. **ceangal 4/6** (from 0/6 in the morning).
- diff_bench's `view.box` E002 is a stale bench on ceangal's side (not updated for the view API rename), not a compiler bug.

<a id="発信ユーザー手番"></a>

## Communication (the user's turn)

- Post the finalized announcement (#773 closed; full text in `docs/roadmap/done/claim-wording-perceus-byte-identity.md`) together with the 0.29.0 announcement.

<a id="関連"></a>

## Related

- The flight ladder is separate: [#586](https://github.com/almide/almide/issues/586) (#776 reference app / #777 shrinking the lowering trust base / #569 WCET updated).
- [v1-release-path](../../../../../roadmap/active/v1-release-path.md) / [v1-org-byte-verification](../../../../../roadmap/active/v1-org-byte-verification.md) / [code-health-codopsy](../../../../../roadmap/done/code-health-codopsy.md)
