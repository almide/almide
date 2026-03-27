<!-- description: Auto-inferred effect capabilities with package-level permissions -->
<!-- done: 2026-03-27 -->
# Effect System — Auto-Inferred Capabilities

**Priority:** 1.x (information display) → 2.x (restriction enforcement)
**Prerequisites:** Effect Isolation Layer 1 completed
**Principle:** Users only write `fn` / `effect fn`. The compiler auto-infers capabilities.
**Syntax constraint:** Effect granularity is never exposed in user syntax. No new keywords. `effect fn` is the sole marker.

> "The code you write doesn't change. Only the compiler gets smarter."

---

## Completed (Phase 1-2) → [done/effect-system-phase1-2.md](../done/effect-system-phase1-2.md)

- [x] **Phase 1: Effect Inference Engine** — EffectInferencePass, 7 categories (IO/Net/Env/Time/Rand/Fan/Log), transitive inference, `almide check --effects`
- [x] **Phase 2: Self-package Restrictions** — `almide.toml [permissions]`, integrated into standard `almide check`, Security Layer 2
- [x] **Phase 2b: Permissions Propagation** — Permissions check runs during `almide run`/`almide build` as well (`check_permissions()` extracted as shared function)

---

## Remaining

### Phase 3: Dependency Restrictions (2.x)

```toml
[dependencies.api-client]
git = "https://github.com/example/api-client"
allow = ["Net"]  # IO is forbidden
```

Consumers restrict the capabilities of dependency packages. Security Layer 3.

### Phase 4: Internal Type-Level Integration (2.x, no syntax changes)

Attach effect sets to type information inside the compiler (add EffectSet to FnType).
User syntax remains unchanged — still just `effect fn`.
Explicit effect syntax like vibe-lang's `with {Async}` is **not adopted**.

---

## Effect Categories

| Effect | Modules | Status |
|--------|---------|--------|
| `IO` | fs, path | ✅ |
| `Net` | http, url | ✅ |
| `Env` | env, process | ✅ |
| `Time` | time, datetime | ✅ |
| `Rand` | math.random | ✅ |
| `Fan` | fan | ✅ |
| `Log` | log | ✅ |

## Differentiation from vibe-lang

| | vibe-lang | Almide |
|---|-----------|--------|
| User syntax | Explicit `with {Async, Error}` | `effect fn` only — inferred |
| LLM burden | Must choose effects | No changes |
| Restriction scope | Function level | Package boundary |
| New keywords | `with`, `handle` | None |
| Breaking changes | — | None (additive) |
