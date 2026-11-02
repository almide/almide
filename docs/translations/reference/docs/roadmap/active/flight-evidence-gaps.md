> Source description: Hands-on internal audit findings (2026-07-03) — the measured distance between DAL-A philosophy and DAL-A evidence, as 7 findings with corrective work items and acceptance criteria
<a id="flight-evidence-gaps--実地監査所見台帳"></a>

# Flight Evidence Gaps — Hands-on audit findings register

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](flight-evidence-gaps.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-evidence-gaps.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](flight-evidence-gaps.zh-CN.md)

> Non-normative reader snapshot of the pinned source; original remains authoritative. No automatic synchronization, independent implementation validation or execution of quoted instructions. Historical status, plans and comparative claims retain their source scope.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-evidence-gaps.md)

> Source caveat: this is the pinned historical internal audit and its dated follow-ups, not new measurements or external DAL/DO-178C/DO-330 qualification. Initial seven findings coexist with later F8. Preserve hypotheses and later corrections: F4 load → locale, F6 tarball suspicion → shared-working-copy make install. Initial parity counts do not sum to the stated total; four target headers and six updated files differ. The August table says baseline decreases only and simultaneously lists TOR as uncompiled and compiled; the final paragraph still lists compilation as remaining. These source inconsistencies are not silently corrected. Structural-impossibility claims concern the described stamped gates, not universal security; proof/agent independence is not qualified human approval. Precision ranges proposed before closure are not fresh guarantees. No quoted commands, CI edits or measurements executed.


> **Position**: Whereas [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) handles decomposition of standards into five mechanisms and ALS design, and [flight-qualification](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md) handles DO-330/333 mapping, this document records **holes in the evidence system observed from inside as a worker during hands-on work on 2026-07-02–03 (about 40 compiler/MIR commits, parity 126→163, elimination of silent-miscompile classes)** in the form of audit findings. Each finding cites facts that actually occurred in this session, with their meaning for standards, corrective work items and mechanically verifiable acceptance criteria.
>
> **Overall assessment**: The design-assurance philosophy (the fail-safe principle, traceability and ratchets) points toward DAL-A, but the evidence system is equivalent to DAL-D. Closing that distance requires these seven findings, not individual bug fixes.

---

<a id="所見一覧深刻度順"></a>

## Findings (in severity order)

| # | Finding | Severity | Corresponding standards mechanism |
|---|---|---|---|
| F1 | The specification is the implementation (v0) itself — circular oracle | Most severe | ① Independent normative specification |
| F2 | Illusion of verification coverage — green gate ≠ correct output | Most severe | ③ Verification ranking |
| F3 | Gap between proof and implementation — lowering is trusted code | Severe | ③ DO-333 formal credit |
| F4 | Nondeterministic verification — tolerating flakiness as “known” | Severe | ③ Robust verification procedures |
| F5 | Lack of independence — the author moves the ratchet | Severe | ⑤ Process and responsibility |
| F6 | Unresolved configuration-management incident — binary replacement | Medium | ⑤ Configuration management |
| F7 | Known numerical incompatibility — float.parse boundary rounding | Medium | ① Numerical precision requirements |

---

<a id="f1--仕様が実装v0そのものoracle-循環"></a>

## F1 — The specification is the implementation (v0) itself (circular oracle)

**Evidence (actual example, 2026-07-03)**: The MIR-path `json.parse` self-host was implemented by **copying clauses** from `runtime/rs/src/json.rs`, including surrogate composition, permissive separators and error wording. Unicode support for `string.trim` also reverse-looked-up the 25 codepoints of `char::is_whitespace` from the oracle. The float.parse error wording ("cannot parse float from empty string") follows Rust’s `ParseFloatError` display. **The definition of “correct” is always the v0 implementation**, so v0 bugs become fixed as specification bugs (indeed, `list.chunk(-1)` copied the Rust implementation detail of usize reinterpretation in `chunks(n as usize)` as “specification”).

**Meaning for standards**: DO-178C requirements must exist independently of implementation. In a specification=implementation cycle, verification of compliance with High-Level Requirements cannot be defined.

**Correction**: Certification-grade CG-1 (making ALS normative) is the corrective action itself. Additional requirements from this document:
- Write each ALS section as a **normative decision**, not “because v0 does it.” ALS must **explicitly rule on adoption or rejection** of behavior derived from v0 implementation details (chunk’s usize reinterpretation, parse_string permissiveness), recording the rationale.
- Fix the copied self-hosts (json_parse / string_trim / float_parse / list_chunk) in this finding as the list to **relink to specification references** after ALS is complete.

**Acceptance criteria**: In addition to the three-layer traceability in `check-contracts.sh` (CG-1 exit criteria), the header comments of these four self-hosts refer to ALS section numbers rather than oracle implementation files.

**Progress (2026-07-04)**: Normative sections ALS-T1–T5 established in `docs/specs/als/text-and-numbers.md` (trim’s White_Space set; float.parse’s exactly rounded value norm plus error wording; json.parse’s RFC 8259 plus rulings; list.chunk’s negative-size ruling — **explicitly stating promotion from a v0 implementation detail**; and case mapping’s Unicode norm plus Final_Sigma and locale-independent rulings). Headers of six copied self-host files now refer to ALS sections (the latter half of the oracle cycle — CG-1 spec-keying / three-layer traceability — remains unfinished).

**Progress 2 (2026-07-04)**: **Spec-keying mechanism implemented** — the contract `spec = "ALS-xx"` field and resolution checks in check-contracts.sh (a contract referencing a nonexistent section makes the gate FAIL; mutation-tested). Six initial contracts keyed (C-001/C-002→ALS-T6 termination conventions; C-007→ALS-T7 top-let evaluation timing — both sections newly established; C-020→T5, C-021→T1, C-024→T2). Three-layer spec↔contract↔fixture machinery is operational. CG-1 remainder = rollout to all active contracts (writing ALS sections is the bottleneck: a documentation project).

<a id="f2--検証カバレッジの錯覚ゲート-green--出力正"></a>

## F2 — Illusion of verification coverage (green gate ≠ correct output)

**Evidence (actual example, 2026-07-03)**: Behind a green corpus-wall advertising "zero silent miscompiles", `match` linearization (executing both arms sequentially) survived as **output corruption executing both println calls**. The gate’s claim concerns certificate soundness (ownership/caps), not output correctness. Output-parity, which actually verifies output, covers **163** out of 573 spec files (wall 75 + v0fail 23 + skip 297 are unverified). Moreover, **structural coverage of the compiler itself had never been measured** — not even statement coverage, let alone MC/DC. Nobody knows which code lines have never been exercised by a test.

**Meaning for standards**: DAL-A is MC/DC at object-code level. Without even its prerequisite statement coverage, there is no way to claim completeness of verification activities. Surviving linearization is a concrete specimen of a bug living in a coverage hole.

**Corrective work items**:
1. Introduce `cargo llvm-cov` (or grcov) into CI and **measure and publish almide-mir / almide-codegen line coverage** under `almide test spec/` + corpus-wall + output-parity (first face current numbers; targets come afterward).
2. Register output-affecting branches among uncovered regions as gaps in the evidence ladder.
3. Classify output-parity’s 297 skips (such as missing `fn main`), separating those **not yet verified, rather than unverifiable**, from wall/v0fail.
4. Align gate wording with what is checked (corpus-wall’s “zero silent miscompiles” → “zero certificate violations” — matching claims to evidence is the first audit step).

**Acceptance criteria**: CI emits coverage numbers and a “compiler’s own coverage” row exists on the dashboard. Gate output wording matches what is actually verified.

**Progress (2026-07-03)**: First measurement with `proofs/coverage.sh` (manual llvm-cov pipeline — cargo-llvm-cov orchestration rejected because it reported the wrong target as 0.00%): **almide-mir line 65.76% / function 72.20%** (test suite + rendering load from 232 spec/wasm_cross fixtures). Wording in four places — corpus-wall / receipt / TRUSTED_BASE / classify_corpus — changed to “totality + certificate claim, NOT output correctness.” The same day demonstrated the correction in practice: behind green corpus-wall, a silent miscompile (the sixth class) was found and fixed in which `prim.handle(<literal>)` fell into deferred-Const 0. Retiring the deferred-Const fallback was the next step (recorded in TRUSTED_BASE’s boundary diagram) → **already retired** (strict value mode, 2026-07-03).

**Progress 2 (2026-07-04)**: Remeasurement with expanded workload — almide-mir + almide-codegen test suites, v1 render of all executable spec fixtures, and the **v0 production path** (the full `almide test spec/` pipeline). TOTAL line 65.89%. Per-file register fixed in `proofs/COVERAGE.md` (lowest is the defunc engine’s control_p5 at 43%, with a list of next tests to write).

<a id="f3--証明と実装の間隙lowering-は信頼コード"></a>

## F3 — Gap between proof and implementation (lowering is trusted code)

**Evidence (actual example, 2026-07-03)**: Coq proofs exist for the checker kernel and Coown patterns (CoownLoop.v / CoownCompose.v). But the lowering body that **generates** certificates is trusted Rust code, and this session alone fixed five output corruptions (linearization, representation mismatch from never-err stripping, Const-0 globals, missing lambda-ctx layout, and residual _start stack values) — **all outside the proof**. “Proven” in “kernel-proven path” is not end-to-end.

**Meaning for standards**: DO-333 formal credit applies only to proven properties. Lowering correctness is currently supported only by differential testing, whose net is F2.

**Corrective work items**:
1. Document a single **boundary diagram** in docs/contracts/ showing what is proven and what is trusted code (currently scattered across file comments). This is the first diagram an auditor asks for.
2. Shrink the trusted-code side: mechanize lowering output invariants (such as “a bound Result enters the tracking set” and “materialize and drop classification use the same type decision”, categories violated by all these fixes) as **post-pass checks, not runtime assertions**. Add a tracking-set consistency gate analogous to existing `assert_names_resolvable` / ConcretizeTypes gates.
3. Long term, join per-function certificates in certificate-format-v1 / value-rc-cert.

**Acceptance criteria**: A boundary diagram exists, the tracking-set consistency gate runs in CI, and a recurrence-prevention gate or fixture can be identified for each of the five bug classes.

<a id="f4--検証の非決定性flaky-を既知で容認"></a>

## F4 — Nondeterministic verification (tolerating flakiness as “known”)

**Evidence (actual example, 2026-07-03)**: In full output-parity runs, two or three files among append_accumulator(_heap) / list_eq(_float_bool) / string_codepoint(_index) fail **only under machine load**, but always byte-match alone. The cause is the harness’s 20-second `alarm` timeout and CPU contention from parallel cargo execution. Twice I “manually restored the baseline” and moved on — reasonable for research speed, but **the verification procedure itself remains nondeterministic**, and tolerating this damages evidentiary value.

**Meaning for standards**: A nonreproducible verification result is not a result. “Passes alone” is an additional claim, not a substitute for evidence.

**Corrective work items**:
1. Make output-parity timeouts **retry-based**, not just wall-clock-based (on timeout, rerun alone once and use that result; a second failure is real).
2. Isolate full-run load (serialization equivalent to `--test-threads`, or script-enforced prohibition of cargo build during gates — flock is already a precedent).
3. Abolish manual baseline editing; only `--update` output is authoritative (linked to F5).

**Acceptance criteria**: Five consecutive full runs with zero REGRESSION output. No subsequent manual-edit commits in baseline file git history.

**Closed (2026-07-03)**: The true cause was not load but **sort/comm collation locale** (Python’s byte-ordered sorted and the shell’s locale-ordered sort reverse `.` and `_`; decisive evidence was self-contradictory logs listing the same file as both new and regressed). Fixed `LC_ALL=C` in the gate, with solo-once retry (all non-match verdicts rechecked during quiet time with 60s). Acceptance achieved: five consecutive green full runs. Misdiagnosed as machine load for two days — exactly this finding’s lesson: do not tolerate nondeterminism; remove its root cause.

<a id="f5--独立性の不在変更者がラチェットを動かす"></a>

## F5 — Lack of independence (the author moves the ratchet)

**Evidence (actual example, 2026-07-03)**: In this session I alone (a) changed lowering, (b) ran verification, (c) updated the parity baseline with `--update` plus manual restoration, and (d) rewrote one test expectation (`match_arm_heap_payload_binding_aliases_the_subject` changed to expect a wall). (d) was legitimate (the old test fixed miscompile behavior in place), but **the person deciding legitimacy was also the author**.

**Meaning for standards**: DO-178C verification independence (DAL-A/B) means the maker does not decide pass/fail. Automatic gates strongly mitigate this, but three powers — ratchets, expected values and wall classification — are concentrated in the author.

**Corrective work items**:
1. Require baseline / expected-value / wall(KnownBroken) changes in **dedicated commits** (lefthook rejects implementation and baseline changes in one commit).
2. Require the commit message of those commits to give reasons and evidence of no regression (single-run records).
3. Long term: require human or independent-agent review approval (organizational, mechanism ⑤).

**Acceptance criteria**: A lefthook rule exists; zero implementation/baseline mixing in the last 20 commits.

**Closed (2026-07-03)**: `scripts/check-ratchet-separation.sh` + lefthook pre-commit. Verified with negative (implementation+baseline mixing → exit 1) and positive (clean → exit 0) cases. All three float_parse / total_order / case_unicode ratchets that day used separate commits.

**Addendum (2026-09-18, discovered in the v0.63.0-rc1 release range)**: Range mode (#2183) judged every commit against the **latest** rules, retroactively failing commits made before rule introduction (19 in main..develop, 17 retroactive). Corrections:
1. Judge commits on the protected line (develop in CI) by **the gate at their parent commit** — rules bind only changes after introduction. Replay that gate version in pre-commit mode, replacing `git diff --cached` with that commit’s diff (replay, not reimplementation).
2. Judge changeset commits by **both the protected-tip gate and the parent gate** — a changeset cannot relax gates for itself (independent review demonstrated and closed the remove-gate → mixed-commit → restore-gate bypass).
3. Classify the gate itself and disposition register as verification artifacts — they cannot ride along in implementation commits.
4. Real violations already on the protected line (violating contemporaneous rules, not retroactive violations) cannot be split, so close them with **dispositions** in `proofs/ratchet-dispositions.txt`. Records must exactly match the target commit and flagged file set, name a reviewer different from the author, and state evidence. A changeset cannot close its own commits. The initial two were #2076 / #2115.

**Honesty about independence**: Design and implementation were by Claude Opus 5; another agent using a different model (Claude Fable 5.1), with fresh context, reviewed in three rounds, finding one blocker and six minor holes initially, then four further issues after fixes (one a blocker for the disposition register). All became negative controls (`scripts/check-ratchet-separation-negative.sh`, 5 positive + 22 negative cases). This is **independence between agents**, not a substitute for DO-178C’s independent human verification (flight-organization B4). The disposition reviewer field records who rederived evidence, not qualified human approval.

<a id="f6--構成管理インシデント未解明バイナリ置換"></a>

## F6 — Unresolved configuration-management incident (binary replacement)

**Evidence (actual example, 2026-07-03)**: During work, `~/.local/bin/almide` was **inexplicably** replaced by v0.27.13 (Jun 22 build), failing two specs. I restored it with `make install` and moved on, **leaving the cause (what overwrote it, when and why) unresolved**. A parallel agent, cron or another session was possible, but unidentified.

**Meaning for standards**: An actual incident showing verification-environment configuration is uncontrolled. Identity of “evidence taken with that binary” cannot be claimed.

**Recurrence (around 14:00, 2026-07-03, detected by the stamp gate)**: It recurred **the day F6-2 stamping was introduced**, and coverage measurement correctly stopped FATAL at the stamp gate — taking evidence with a wrong binary has already become structurally impossible. On-the-spot investigation cleared `~/.cargo/bin/almide` (0.14.8, Apr 18), cron, LaunchAgents and org scripts. The replacement’s mtime remained Jun 22, a fingerprint of unpacking a release tarball + mv (mtime-preserving transfer) — the strongest hypothesis is **a parallel session on this machine reinstalling GitHub release v0.27.13**. Prevention (uchg etc.) was rejected because it breaks legitimate overwrites by parallel work; permanent mitigation is stamp-based detection and stopping.

**Mechanism identified (2026-07-04, closed)**: A whole-disk scan found almide binaries only in `~/.local/{bin,almide}` and **this repository’s target/release** (no other checkout, worktree or cache). Therefore all replacements go through this repo’s `make install`; the culprit is **a parallel session sharing the same working copy** (consistent with five frontend/codegen files simultaneously marked M and addition of the new ceangal repo to the org). Old 0.27.13 appeared while that session built another branch/commit. Existing stamping completes the mitigation (evidence collection with a wrong binary is structurally impossible), and a resident watcher (/tmp/almide-binwatch.log) was installed to capture the process name in the act.

**Corrective work items**:
1. Keep this finding as the incident record (done — this document).
2. At the beginning of gate scripts, **stamp logs** with `almide --version` and binary mtime/hash, failing immediately if different from the expected build (a one-line defense makes recurrence detectable).
3. Include toolchain stamps (rustc/wasmtime/coqc versions) in verification receipts — connect to certificate-format-v1.

**Acceptance criteria**: Every proofs/*.sh emits a binary stamp and fails on version mismatch.

**Closed (2026-07-03)**: `proofs/lib/stamp.sh` introduced in all gates. On its introduction day it caught two real drifts with FATAL stops (another 0.27.13 replacement and an old workspace build predating embedded-stdlib updates) — taking evidence with the wrong binary became structurally impossible.

<a id="f7--既知の数値非互換floatparse-境界丸め"></a>

## F7 — Known numerical incompatibility (float.parse boundary rounding)

**Evidence (actual example, 2026-07-03)**: Self-host float.parse uses mant×10^k scaling; rounding differences of about 1ULP from v0 (Rust strtod = Eisel-Lemire) remain at denormal (4.9e-324), maximum (1.797e308) boundaries and mantissas over 19 digits. This is parity’s sole actual MISMATCH.

**Meaning for standards**: In systems with numerical-precision requirements (aviation is exactly such a system), known rounding incompatibility alone is a blocker. There is no “almost correct” floating point.

**Corrective work items**:
1. Self-host exact decimal→f64 conversion (pure Almide port of Eisel-Lemire or Simple Decimal Conversion — independent brick, substantial work).
2. Until then, explicitly state the current implementation’s **guaranteed precision range** in ALS (a quantitative statement such as “up to 15 digits within 10^±22 is exact; boundaries are 1ULP”) and pin that range with contract fixtures.

**Acceptance criteria**: `spec/wasm_cross/float_parse.almd` byte-matches, or ALS normatively specifies the precision range and fixtures verify it (either closes the finding).

**Closed (2026-07-03)**: Implemented an 800-digit high-precision decimal (Simple Decimal Conversion) slow path; `float_parse.almd` byte-matches (minimum denormal 5e-324, f64 maximum, >19-digit mantissas, half-even and huge exponents all match Rust). Fast path (≤15 digits & |exp|≤22) retains the existing Clinger safe range.

---

<a id="f8--新規所見2026-07-03-夕-ローカル-pcc-チェーンが暴いた-cert-会計違反"></a>

## F8 — New finding (evening of 2026-07-03): cert accounting violations exposed by the local PCC chain

**Background**: As part of F6-2, Coq (Rocq 9.1.1) was installed locally and the checker phase of `proofs/corpus-wall.sh` (previously delegated to CI, never run locally) was executed in full for the first time. Result: **the kernel-proven checker REJECTed ownership witnesses** (among 20393 objects, the cert line for `spec/integration/codegen_effect_fn_test.almd::parse_positive_even` — err reconstruction in chained monadic `!` — was a bare `m`: move-out without an acquisition event).

**Meaning**: Execution of this function is byte-verified by parity (including green err-path fixtures), with no observed actual harm (double-free/leak). However, rejection of its cert puts it **outside** the “accept ⟹ safe” net — an in-profile function existed without proof protection. Like the mir>ir gate, this exemplifies “it looked green because verification had not run”: additional evidence for F2 (coverage illusion).

**Corrective work items**:
1. Identify the responsible path and fix accounting: pair payload move-out in `err(e) => err(e)` reconstruction (mod_p6 monadic desugar) with an acquisition event (Dup/`a`), or fix a real bug if ownership is genuinely doubled.
2. Make `ownership.names` permanent (cert-line ↔ function-name correspondence, already added during this investigation) — REJECT can immediately be bisected to a function.
3. Make the corpus-wall checker phase **mandatory locally** (coqc already installed) — “delegated to CI” allowed nonexecution as seen here.
4. Check whether CI actually runs this phase (if so, CI should also be red; if not red, it too is not running it = configuration hole).

**Acceptance criteria**: Local corpus-wall green including its checker phase, zero REJECTs.

**Progress (night of 2026-07-03)**: Inventory using a custom balancer listing all violating cert lines reduced **many → 2**.
1. **Accounting for merge move-in** (root cause fixed): a bare `m` when an inner merge dst of nested monadic `!` is Consumed outside — pair a move-in `i` from the arm with the released merge dst (physically, arm −1 and merge +1 transfer the same reference). The backing gate (unbacked +1 check) now uses the same released-merge set. Checker ACCEPT confirmed.
2. **via_if class (eighth miscompile, root cause fixed)**: In `let v = if c then boom(x) else boom(y)` (a scalar-typed if whose effect-fn auto-`?` places `Try` in each arm), the scalar path silently stripped Try, emitting **raw arithmetic adding +100 to a Result-block handle** — the first case where a checker leak line (bare `i`) pointed to a real miscompile. Extended tail-dup to “heap-typed binds or binds with direct Try/Unwrap in an arm” and delegated to existing monadic desugar, fixing the root cause (via_if byte-MATCH, baseline 176 with zero regressions, MIR 507/0).
3. **Remaining classes also fixed (late that night)**: `effect_tco::checked` (declared-Result tail-if emits no Consume and `EndIf {{ val }}` effectively moves — added a val-move rule, excluding loop-mechanism values) and `bytes_set_value_semantics::rotate` (Dup-initialization-slot events split into two lines by of resolution versus direct ValueId — unified slot events on object_of resolution, converging to "a(id)m").

**Closed (2026-07-03)**: `proofs/corpus-wall.sh` **fully green locally including checker phase** — the kernel-proven checker ACCEPTed all witnesses for ownership 20876 objects / names 4248 / caps 3481 / caps-transitive 243. First full local PCC-chain run in project history. Side effects: root fixes for one real miscompile class (via_if) and three cert-accounting classes. Stamping also caught the day’s third binary replacement with FATAL (active parallel sessions — F6’s permanent mitigation works).

<a id="優先順位と依存"></a>

## Priorities and dependencies

```
F6-2 (刻印)         ── 1日級、即効、単独
F4 (flaky 根絶)     ── 数日級、単独 — 以後の全証拠の信頼性の前提
F5 (権限分離)       ── 数日級、単独
F2-1 (coverage 計測) ── 数日級、単独 — 現状の直視
F3-2 (整合ゲート)   ── 週級 — 今回のバグ 5 クラスの再発防止
F1 (ALS)            ── certification-grade CG-1 に合流（最長・最重要）
F7 (strtod)         ── 独立 brick、F1 の精度規範が先でも可
```

Translation of the preserved historical diagram: F6-2 (stamping): about one day, immediate impact, independent. F4 (eradicate flakiness): several days, independent, prerequisite for all subsequent evidence reliability. F5 (separate authority): several days, independent. F2-1 (coverage measurement): several days, independent, facing the current situation. F3-2 (consistency gate): weeks, preventing recurrence of these five bug classes. F1 (ALS): joins certification-grade CG-1, longest and most important. F7 (strtod): independent brick; F1’s precision norm may come first.

Order: small, immediately effective F6-2 → F4/F5 restoring evidence reliability → F2-1 facing the current situation → structural F3/F1. **Until F1 and F2 close, claims of “aviation quality” must be limited with an explicit evidence-ladder rank** (refrain from unqualified "flight-grade").

---

<a id="再測定-2026-08-12--所見の現在地1-ヶ月の保証工事後"></a>

## Remeasurement 2026-08-12 — Finding status after one month of assurance work

**Remeasurement** one month after July’s initial audit, following Stage 1–4 work (scalar-read/WAT-prelude/libm audits, three-way voting gate, classifier repair, release sealing and G-F4 app). This register is append-only — findings above remain unchanged as historical records.

| # | July status | Measured 2026-08-12 | Remainder |
|---|---|---|---|
| F1 | Partial (spec-keying operational, all-contract rollout remaining) | **Mechanism now covers all**: 230/230 contracts `spec =` keyed; check-contracts enforces both directions (nonexistent section reference = FAIL; every normative ALS section cited by ≥1 contract). Copied self-host headers now reference ALS sections (json_parse → ALS-T3 etc.). Moreover, **interp promoted to executable specification**: three-way voting on 254/374 fixtures (native==wasm==interp byte equality), abstentions in classification register (HEAP_BOUNDARY 120 / BRIDGEABLE 0) + shrink-only ratchet; measurements for all 20 semantics-manifest modules | **ALS writing coverage** (syntactic-element completeness = remaining half of Stage 3 gate) and interp heap arc for HEAP_BOUNDARY 120 (#1226) |
| F2 | Partial (coverage 65.89% measured, wording corrected) | Alignment of wording and verification **structuralized**: proven-vs-trusted.md (boundary diagram) collects “what each gate claims / does not claim.” Coverage **ratcheted in CI** (fuzz-nightly coverage-ratchet job #566, only baseline decreases allowed). #1183 class **eradicated as a family** by scalar-read audit (all 63 arms classified, UNGUARDED 0, CI gate). Equivalent audits extended horizontally to 63 WAT-prelude functions / 23 platform-libm calls | **MC/DC still zero** (not even branch coverage measured, let alone DAL-A object-code MC/DC — lines only). Next step: branch coverage |
| F3 | Open (no boundary diagram or consistency gate) | Item 1 **closed**: docs/contracts/proven-vs-trusted.md exists as the auditor’s boundary diagram, referenced from CLAUDE.md. Item 2 partial: gates for these five bug classes exist individually — mir==ir caps gate, rollback lifted_mark check, released-merge backing check etc. (F8 root fixes) — but are not integrated as **one named tracking-set consistency gate** | Register the consistency checks (mapping bug classes to gates). Moving to per-function certificates remains long term |
| F4-F8 | Closed | **Remain closed**, strengthened: F5 ratchet-separation works on all PRs today (baseline changes in separate commits). F6 stamping caught PATH/workspace mismatch with FATAL in gate.sh today (proof of operation). **Slow/Hang separation in the fuzz classifier** (#1235) strengthens verification determinism — known performance debt (#1229) does not pollute correctness metrics | — |

<a id="stage-5-欠損成果物リストgoal-指示の-3-点実測"></a>

### Stage 5 missing-deliverables list (three items from goal instructions, measured)

| Deliverable | Current state | Assessment |
|---|---|---|
| Tool Operational Requirements (TOR) | CLAUDE.md Usage + TRUSTED_BASE.md partially correspond. **Not yet compiled** into DO-330 format (intended operating environment, input constraints, operational workarounds for known limits) | **Compiled (proofs/TOR.md)** — TOR-1–9 operational requirements (each names enforced-by instrumentation; check-tor-refs.sh enforces reference consistency in CI) + operational known-limitations table. Formatting for G-F6 kit remains |
| Verification of verification tools themselves | Checker = Coq proof (qualified-by-proof, 45 theorems + coqchk + cross-version). **Negative-test culture established for gate scripts** (check-contracts mutation tests, three audit gates negative-tested, release-seal forged/missing-input negative tests). Fuzz-harness unit tests (classifier made pure functions) | **Inventory operational** (proofs/gate-verification.toml + check-gate-verification.sh — 33 gates in five classes, confronting UNVERIFIED via a shrink-only ceiling). #1244 burn-down: 18 → **2**. Remaining two (check-host-determinism / check-browser-determinism) are environment gates requiring a host with wasm32 std |
| Formal known-problems register | GitHub issues + release-note known limits (#1229 in 0.57.0) + `fuzz-perf` label + flagged-for-revision mechanism + **release sealing** (proofs/releases/ — since v0.57.0, evidence bound to tags) | **PARTIAL** — scattered. Adding a known-problems section to release seal’s `[recorded]` closes the form |

**Updated overall assessment**: From July’s “DAL-A philosophy, DAL-D-equivalent evidence” to an evidence system with **the skeleton of independent normative specification + executable specification + coverage audit + sealing**. Remaining structural distance: (a) MC/DC (F2 remainder), (b) ALS writing coverage (F1 remainder = Stage 3), (c) TOR compilation (Stage 5), (d) external adoption track record — (d) requires market time, not engineering. Refraining from unqualified "flight-grade" **continues** (keep explicit evidence-ladder ranks).
