<a id="outside-review-audit-2026-07--証拠層の負債バーンダウン"></a>

# Outside-Review Audit 2026-07 — Burning Down Evidence-Layer Debt

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](outside-review-audit-2026-07.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](outside-review-audit-2026-07.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](outside-review-audit-2026-07.zh-CN.md)

> Non-normative reader snapshot of the [mixed Japanese/English canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/outside-review-audit-2026-07.md), pinned at `35297a612fb29e3f78f6587aba253aa91d51f466`. The original remains authoritative. This snapshot is not automatically synchronized and is not generator/checker input. Dates, plans, completion status, measurements and comparative claims below are retained as historical source statements, not independently reproduced or validated for a current release. Code blocks and comments remain verbatim. Historical collection metadata is displayed as an inactive reader note.

> Historical description: the 2026-07-27 five-lens outside-reviewer audit found an evidence layer lagging v0→v1 and inverted honesty (honest internal docs, false outward claims); layered burn-down toward “zero false claims, every gate real,” with issues #913–#932.

> Source caveat: this is the dated July audit record, not a fresh audit or proof that every finding remains open today. Strong comparative evaluations, defect counts and proposed durations are source statements, not independently validated endorsement or scheduling commitments. Proposed fixes and issue actions are quoted history; this translation executes none of them. Preserving failure evidence does not confer general compiler or certification guarantees.

> Conducted 2026-07-27. Consolidates parallel scans from five perspectives — architecture/IR, performance, DX, proof/certification, ecosystem/appearance — asking “what would discerning outside reviewers from Rust/MLIR/Koka/CompCert circles criticize when assessing this as a top compiler of 2026?” All findings were filed as issues #913–#932 + bonsai-almide#1 with file:line evidence and structural closure conditions. This document is their map.

<a id="核心の構図"></a>

## The central pattern

**The compiler itself advanced in the v0→v1 transition, but the surrounding evidence layer — claims, benchmarks, gates, documentation — failed to follow, inverting the honesty gradient.**

- Internal documents (TRUSTED_BASE.md / proven-vs-trusted.md / flight-*.md) are strikingly honest to reviewers — assessed as “more honest than academic verified-compiler papers.”
- Factually false claims concentrate on outward-facing surfaces (README / TRUST-SPINE.md / BENCHMARKS.md / docs-site).
- For a project presenting itself as evidence-first, this is its deepest vulnerability: **once an outside reviewer finds one false claim, they discount even genuine strengths (contract ledger, coqchk, skip ledger).**

The same pattern appeared in all five reports: performance scoreboards measured the deleted v0 emitter; roadmap/active documents described vanished directories; some CI gates were only nominal (fmt --check, axiom-clean, LEDGERED examples); fuzz succeeded on only 1 of 20 nights. The root cause: **the compiler improved while evidence stayed old**.

<a id="第1層--外向き主張の虚偽-数時間1日級最優先"></a>

## Layer 1 — False outward claims (hours to one day, highest priority)

| Issue | Content |
|---|---|
| #913 | README Lean claims are false in three ways: `allHeapFreed` = “Dec ≥1” (not leak elimination); `perceusTransform` is a 12-line toy; “Formally Verified” covers the native leg with zero proof |
| #914 | Four false claims in TRUST-SPINE.md: V = substring search; ALS normative semantics absent (80 lines / 2 theorems); unused CompCert/CertiCoq in the TCB; checker said to be “a few hundred lines” is actually 1,144 |
| #915 | fan concurrency: `fan.map` sequential, `fan.race` runs only the first thunk. docs-site lists nonexistent tokio/TS backends and deleted fan.timeout |
| #916 | Performance claims: README “faster than Gleam/MoonBit” refers to LLM work time; BENCHMARKS “100 ops 1.1s” has no baseline; vs-Rust table is a v0 relic; wasm-opt flags differ between docs and code |
| #918 | Stale-doc cleanup: PRODUCTION_READY frozen at v0.8.0; wasm-engine-redesign.md describes vanished emit_wasm/; 847 vs 834 |
| bonsai#1 | PERF_ROADMAP “724 tok/s” measured as 0.725 — a 1000-fold error (in the direction that makes it appear to win) |

<a id="第2層--動かないゲート-アイデンティティの傷"></a>

## Layer 2 — Nonfunctional gates (a wound to identity)

| Issue | Content |
|---|---|
| #919 | `almide fmt --check` always exits 0 — every CI gate using it is a no-op |
| #920 | The axiom-clean “gate” is printf — adding Axiom/Admitted still leaves it green. Fix is 3 grep lines + tamper drill |
| #921 | trust-spine.yml does not run on the release route (PR to main), its paths filter excludes frontend, and check-wasm-exec silently exits 0 when wasmtime is missing |
| #922 | LEDGERED examples are never compiled — 6 of 22 examples have stayed broken for 4 months |
| #923 | `almide explain` fails for 21 of 32 codes in the installed binary (docs not embedded) |
| #924 | fuzz-nightly succeeds on 1 of 20 nights — the instrument for the silent-wrong class (dominant class, 49 of 258 closed cases) is down. Blocks #796 |

<a id="第3層--構造債-arc-級依存関係あり"></a>

## Layer 3 — Structural debt (arc-sized, with dependencies)

| Issue | Content | Dependency |
|---|---|---|
| #925 | Frontend runs twice every build, discarding the first IR; six manually synchronized drivers → unify the driver | Prerequisite for #928 |
| #926 | `is_heap_type` defined divergently in 6 places across 3 crates (missing Ty::Named in pass_anf has actual leak symptoms) → single source | — |
| #927 | LSP correctness: UTF-16 positions treated as bytes (panic on non-ASCII), rename is string replacement, didChange does git fetch + lock writes | — |
| #928 | No incremental/query foundation — LSP invokes the batch compiler on every keystroke | #925 |
| #929 | No nanopass optimizer on wasm leg (almide-mir does not depend on almide-codegen), zero SIMD — optimization quality structurally forks | #917 |
| #930 | Dead-code cleanup: unreachable 26-pass wasm pipeline (including a second Perceus), two dead Rust emitters, unused wasm-encoder | — |
| #931 | Wall diagnostics: `LowerError::Unsupported(String)` has no span; nested `{:?}` dump + “file an issue” → Diagnostic + per-shape rewrite hints | — |
| #917 | Rebenchmark v1 leg against Rust + CI perf ratchet (benchmarks currently run nowhere) | — |
| #932 | Mechanize renderer→proof byte link (replace hand-written heredoc comparison in check-wasm-bytes.sh with actual renderer output) | — |

<a id="第4層--証明のコアギャップ-内部では自認済み対外整合が先"></a>

## Layer 4 — Core proof gaps (internally acknowledged; align outward claims first)

Not new issue candidates (owned by existing arcs / TRUSTED_BASE residual ledger). Once outward documents (#913/#914) are corrected, the following stand as “honestly disclosed research problems”:

- The proof subject is a certificate string written by the untrusted compiler itself — producer = claimant, leaving correlated bugs invisible (five shipments of this class already self-recorded).
- No formal semantics of Almide; zero theorems of the form `⟦s⟧ ≈ ⟦compile(s)⟧` — byte equality comes from differential tests (→ promotion to ALS in #530 is the real solution).
- Zero proof coverage for the native leg (→ #764 native trust spine).
- Perceus reuse `r` is proved sound in Coq, but no path emits `Op::Reuse` — proved but unimplemented.

<a id="守るべき強み-割引かれる前に"></a>

## Strengths to protect (before they are discounted)

CI-enforced ledger of 187 contracts (with README drift detection), independent coqchk rechecking, Lean 0 sorry, wasm skip ledger (30→11, reasons required), org-trust-status.md (publishing our own failures), 770B verified Hello World, wasm-opt differential parity gate, rustc-grade diagnostic data model (`try_replace_span`), playground.
**The verification culture itself exceeds funded projects** — the shared assessment of all five reports. Layers 1–2 stop damage to these assets; they are not new investment.

<a id="着手順"></a>

## Order of work

1. **Sprint 1 = Layer 1 + Layer 2** (all hours-to-one-day items). Completion state: **“zero false claims; every gate real.”** Put numeric claims (theorem count / checker line count / stdlib fn count) into gen-claims machinery to prevent redrift.
2. **#924 restore fuzz** — “zero open silent bugs” is meaningless until the instrument returns.
3. **Fold Layer 3 into existing arcs and normal operation** — only the order #925→#928 is fixed. Scope #929 using #917's measurements (measure before building).

<a id="閉鎖条件"></a>

## Closure conditions

After all Layer 1 + Layer 2 issues close, rerun the same five-perspective audit once and find **zero fatal findings in outward-facing documentation**. Each Layer 3 issue owns its closure condition (this document does not wait for those). On achievement, move this document to done/; the reaudit findings become the new ledger.

<a id="未起票の-serious-次の-dx-arc-でまとめる"></a>

## Unfiled serious items (group into the next DX arc)

REPL is cargo-per-line (almide-interp unused); test failures reported per file (assertions appear as panics in generated Rust); parse errors live in a second-tier path outside the diagnostic format (span reconstructed by string round trip); no error dedup (one missing import gives 8 identical errors); formatter output diverges from the repository's own style and rewrites imports.
