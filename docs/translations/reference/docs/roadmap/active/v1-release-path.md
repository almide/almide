<a id="v1-リリース路線--段階リリース計画"></a>

# v1 Release Route — Staged Release Plan

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](v1-release-path.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](v1-release-path.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](v1-release-path.zh-CN.md)

> Non-normative reader snapshot of the [mixed Japanese/English canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-release-path.md), pinned at `35297a612fb29e3f78f6587aba253aa91d51f466`. The original remains authoritative. This snapshot is not automatically synchronized and is not generator/checker input. Dates, plans, completion status, measurements and comparative claims below are retained as historical source statements, not independently reproduced or validated for a current release. Code blocks and comments remain verbatim. Historical collection metadata is displayed as an inactive reader note.

> Historical description: July 2026 release-path decision with correction of zero-risk and qualification overclaims.

> Source caveat: the original’s opening correction supersedes the categorical safety premises preserved below. “Zero risk,” flight-grade promotion, unique value and 4/8 bundles are historical rationale, not present guarantees, external certification or reproduced proof results. Historical CLI flags and automatic fallback are not current operational guidance. The reader translation does not repair the old decision record by silently changing it.

<a id="correction-to-the-historical-release-rationale"></a>

## Correction to the historical release rationale

This is the July 5, 2026 release-path decision, not current flag or release
guidance. Its categorical “no miscompile risk” / “always correct bytes”
premises were too strong: ownership/capability certificates establish modeled
properties, and byte parity establishes observations on tested programs.
Neither proves full lowering semantics. Byte binding alone also does not
confer flight qualification; that requires a scoped assurance case and the
remaining trust assumptions.

Read [proven versus trusted](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md) for the
boundary and [the Wasm overview](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/README.md) for today's route.
The options and original decision rationale below are retained as history,
not repeated as a zero-risk guarantee.

> Formulated 2026-07-05. In response to “release v1 soon,” this records the choice of the fastest route to a “substantive v1,” based on the stated fact that **correctness (honest-wall + PCC) was already guaranteed**.
> **Related**: [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) (ultimate metrics) / [v1-v0-parity](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-v0-parity.md) (③ coverage) / [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md) (③ certificate) / [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) (beyond Gap 1).

<a id="前提これが全ての土台"></a>

## Premises (the foundation of everything)

- **The v1 trust-spine “never emits incorrect bytes”** (honest-wall). If it can lower, its bytes match v0; otherwise `try_render_wasm_source` returns `Err` and walls off. Keep MISMATCH=0 / RUNERR=0 at all times. PCC (ownership/caps) ACCEPT.
- Therefore **no release route has miscompile risk**. They differ only in “what can be shipped / effort / value delivered to users.”

<a id="選定-①-opt-in-検証-codegenv0-fallback-を-beachhead-に"></a>

## Selection: ① opt-in verified codegen (v0 fallback) as the beachhead

**Reason for adoption**: can ship today, zero risk, makes v1 a real product feature, and automatically turns the results of ②③ into value on the same foundation. Combine ④ (version tag) by releasing a version containing ①.

<a id="4案のメリデメ意思決定の記録"></a>

## Pros and cons of four options (decision record)

<a id="①-opt-in-検証-codegenv0-fallback採用進行中"></a>

### ① Opt-in verified codegen (v0 fallback) ★ Adopted, in progress

`almide build/run --target wasm --verified` tries v1 → on success, PCC-verified v1 output; at a wall, automatically fall back to v0. The default (without the flag) remains v0.

- **Pros**: can ship immediately (integration + flag, no research) / zero risk (a wall uses v0, the same as today) / makes v1 an actually usable product feature / opt-in leaves existing users unaffected / when byte-binding proof (Gap 1) arrives, the same route is elevated to flight-grade.
- **Cons**: v1 activates only when all functions are within the subset (whole-program dispatch) → real programs initially often fall back to v0, so **activation is rare and unremarkable** / per-function mixing is hard because of shared ABI/runtime / **the trust core (certificate + checker) is not yet exposed to users** (the execution side emits byte-identical wasm, but the verification story remains unexposed until ③).
- **Effort/risk**: medium / low correctness risk, medium value risk.

<a id="②-実プログラムを端まで通すカバレッジ拡大-次段"></a>

### ② Carry real programs all the way through (expand coverage) — next stage

Keep opening walls and expand coverage until real repositories lower with v1 alone (expand the region not needing fallback).

- **Pros**: within completed coverage, “v1 alone compiles real programs” is a strong, honest milestone / directly advances Gap 3 (real-program coverage) / automatically becomes “expanded v1 activation” value on ①'s foundation.
- **Cons**: slow / endpoint hard to see (even one walled function causes whole-program v0 fallback) / remaining work includes **capturing closures = a fundamental proof-model limitation** ([closure-architecture-v2](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/closure-architecture-v2.md), proof extension = research) / ships nothing by itself (productization through ①③ is needed).
- **Example acceptance criteria**: all fn in the target repository lower with v1 (zero walls) → v0 fallback never fires with `--verified`. Current remaining walls: Set (`Option/Result[Set]` can self-host; construction of `some(set)` confirmed) / guards / general Map (top-level non-String values and `Map[Int,Int]`/`Map[String,String]` can self-host) / capturing closures (fundamental limitation) / tuple-inner and record-inner interp (open-shape aggregates require per-shape readers). **Progress is consolidated in [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md), “Offensive KPI: language-surface coverage.”**

<a id="③-証明書-emit-を製品化--その次"></a>

### ③ Productize certificate emission — after that

`almide build --certify` emits wasm + certificate, rechecked by a checker of a few hundred lines (the README's v1 core: “building is hard, checking is cheap”). `emit_cert` (the verification-side machinery) already exists in part.

- **Pros**: v1's unique value (the core of the trust model) / third parties can run the checker and experience “acceptance ⟹ safety bundle,” making trust tangible / the certificate can attach directly to ①'s verified output.
- **Cons**: the current certificate covers **4/8 of the safety bundle** (mem/name/cap/type ✅; leak, reuse, call-mode, byte remain), and **byte-binding (Gap 1, the “core”) remains trusted** → the certificate is honest but incomplete (“bytes refine source” = C-FAITHFUL is unproved) / complete KGI-T needs a Coq byte-binding proof, deep research / a design is needed to present “4/8 proved” without misunderstanding (overclaim risk).
- **Example acceptance criteria**: `almide build --certify` outputs `app.wasm` + `app.cert`; the bundled checker (`Print Assumptions` ⊆ trusted base, 0 sorry/0 Axiom) accepts ⟹ rederives the declared safety bundle (initially 4/8, honestly explicit). **Certificate format: [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md).**

<a id="④-現状をバージョンリリース--①-と併用"></a>

### ④ Release the current state as a version — combine with ①

Keep trust-spine as an internal quality improvement, bump `Cargo.toml` and tag (the release workflow creates the release).

- **Pros**: fastest (minutes) / ships recent quality and coverage improvements / zero risk.
- **Cons**: alone it does not give users a newly usable “v1” (pure maintenance release).
- **Operation**: cut a version containing ①'s integration as “the first release with v1 opt-in.” Then bump whenever coverage/certificates advance in ②③.

<a id="横断する正直な限界"></a>

## Honest limitations across all routes

- In every integration route, **v1 activates only within the region it can lower** (② coverage matters).
- **A complete trust claim (KGI-T) requires Gap 1 = Coq byte-binding proof**; even on the fastest route, this is research. ①③ deliver “honestly scoped” value before that.
- For grounding in aviation (flight-grade), see [flight-profile](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md) / [flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md) (keystone (い), avoiding Gap 1's wasm byte-binding by binding through Rust→Ferrocene).

<a id="実行順このドキュメントの結論"></a>

## Execution order (this document's conclusion)

1. **Ship ① immediately as a beachhead** (`--verified` opt-in, v0 fallback) = in progress.
2. **Use ④ to cut a version containing ①** (“first v1 opt-in release”).
3. **Expand coverage through ②** (expand v1's activation region; one remaining wall at a time, explicitly identify fundamental limitations and avoid them or move them to research).
4. **Expose certificates through ③** (honestly state safety bundle 4/8; make the trust model tangible).
5. **Gap 1 (byte-binding proof) / flight keystone** toward flight-grade (research stage, separate ledger).
