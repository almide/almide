<a id="almide-grand-plan"></a>

# Almide Grand Plan

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](GRAND_PLAN.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](GRAND_PLAN.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](GRAND_PLAN.zh-CN.md)

> Non-normative pinned reader snapshot. The original remains authoritative; this copy is not automatically synchronized and has not independently revalidated implementations, measurements, or safety.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/GRAND_PLAN.md)
> The checked and unchecked items are the pinned plan’s historical records, not a fresh implementation audit. In particular the older unchecked build-cache item is not a claim that current CLI caching is absent; the 1.0 checklist is explicitly not a present readiness or safety verdict. Later runtime/platform items remain plans.


> “Win decisively as a language at 1.0. Then build the foundations for also becoming an app runtime, before UI.”
>
> Version-by-version execution plan: [ROAD_TO_1_0.md](../../../../roadmap/ROAD_TO_1_0.md) — a ledger assigning major features to every minor from 0.65 → 0.99 (redrawn on 2026-09-27; 0.41–0.64 are shipping records)
>
> Paths to winning by evaluation axis: [ALL_AXES.md](../../../../roadmap/ALL_AXES.md) — current values, victory conditions, and axes to leave across the 6 axes of speed / performance / correctness / LLM writability / DX / ecosystem

---

<a id="phase-1-original-10-readiness-assessment-historical"></a>

## Phase 1: original 1.0 readiness assessment (historical)

The checklist below records an earlier assessment, not a current release readiness verdict or an API freeze. Its counts and excluded modules are historical. For current language/API behavior use [SPEC](../../../../SPEC.md), the [stdlib reference](../../../../stdlib/README.md) and the actual release/version metadata. A 1.0 release decision is not implied by this document.

Earn trust as a language. Do not add features; fix the definition of completion.

- [x] Freeze language specification (syntax, operators, keywords settled; protocol added)
- [x] Stabilize compiler behavior (ICE=0, warning=0, cross-target 106/106)
- [x] Set stdlib boundaries (22 modules, 390 functions; set added; uuid/crypto/toml/compress/term excluded)
- [x] Error-message quality (E001-E010, --json, hint system)
- [x] Breaking-change policy (edition, FROZEN_API, REJECTED_PATTERNS)
- [x] Showcases (5: CLI/API/Data/DevTool/Script)
- [x] 2,500+ tests (2,681 achieved)
- [x] Protocol System (user-defined conventions, generic bounds)

Continued improvements after 1.0:
- examples / cookbook / migration story
- LLM measurement (MSR, first-attempt correctness) — nonblocking

**Original assessment goal:** make adoption decisions possible with explicit evidence of stability. The old checks above alone do not guarantee safety, compatibility, or 1.0 readiness for all present uses.

<a id="phase-2-production-language"></a>

## Phase 2: Production Language

Strengthen distribution units. Create a module economy.

- [x] Package manifest (almide.toml)
- [x] Lockfile (almide.lock)
- [ ] Document semver conventions
- [ ] Public API stability rules
- [ ] Build cache (incremental compilation)
- [ ] Reproducible builds
- [ ] Monorepo / polyrepo support
- [ ] Target compatibility contract (make multi-target a “promise”)

<a id="phase-3-runtime-foundation"></a>

## Phase 3: Runtime Foundation

Establish Almide's distinctiveness. **Grow effects into capabilities.**

- [x] **[HKT Foundation Phase 1-4](../../../../roadmap/done/hkt-foundation-phase1.md)** — TypeConstructor/Kind/algebraic laws, Stream Fusion, Ty::Applied unification (within the completed record's scope)
- [ ] **Old remaining HKT plan, Phase 5-6** — Effect-type integration, Trait integration (Phase 4 Ty unification is included in the completed record)
- [x] **[Effect System Phase 1-2](../../../../roadmap/done/effect-system-phase1-2.md)** — Automatically inferred capabilities (7 categories, transitive), almide check --effects, Security Layer 2
- [ ] **[Effect System Phase 3-4](../../../../specs/effect-system.md)** — Dependency restrictions, type-level integration
- [ ] **Typed host bindings** — IDL/schema-first, codegen-first, no handwritten FFI needed
- [ ] **Resource/task model** — task/scheduler, resource lifetime
- [ ] **Persistence/sync primitives** — local-first, CRDT/merge, sync queue
- [ ] **Observability** — effect trace, IR dump, pass inspector, runtime event timeline
- [ ] **[Compile-Time Contracts](../../../../roadmap/on-hold/compile-time-contracts.md)** — General function preconditions and type/value invariants are proposals. They are separate from existing generic bounds / test-context where; old newtype/where sketches are not current syntax

**Directive: “Make effects the core of the runtime permission model.”**

<a id="phase-4-app-runtime-layer"></a>

## Phase 4: App Runtime Layer

Build an execution model that keeps applications running, before UI.

- [ ] Host lifecycle model
- [ ] Storage/sync runtime
- [ ] App manifest / permission manifest
- [ ] Module hot-upgrade (signed, ABI checked, rollback possible)
- [ ] Optional UI abstraction (state/effect/sync/host integration before buttons)

<a id="phase-5-platform-expansion"></a>

## Phase 5: Platform Expansion

- [ ] Mobile runtime (iOS/Android)
- [ ] Desktop runtime
- [ ] Web embedding
- [ ] Plugin marketplace
- [ ] Managed distribution/update model

---

<a id="やらないことリスト-今は"></a>

## Not doing (for now)

| Item | Reason |
|---|---|
| Dedicated UI framework | Runtime primitives and host boundary come first |
| Own renderer | Lean on host UI or use a thin abstraction |
| Huge stdlib | Freezing costs too much; keep it small and strong |
| “Can do everything” positioning | Clarify what it does well and what it does not do yet |

---

<a id="3つの最重要命令"></a>

## The 3 most important directives

1. **Release 1.0** — Earn trust as a language before talking about the future
2. **Grow effects into capabilities** — This is Almide's distinctiveness
3. **Build runtime primitives before UI** — Becoming an app runtime is fundamentally about the execution model, not rendering

---

<a id="一文で"></a>

## In one statement

> Complete 1.0 as a general-purpose language.
> Then transform the effect system into the core of a capability runtime,
> and establish typed host boundaries and runtime primitives.
> The UI framework can come later.
