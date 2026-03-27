<!-- description: Stdlib module additions (set expanded to 20 functions) -->
<!-- done: 2026-03-20 -->
# Stdlib Additions — Complete

**Priority:** 1.x — incremental additions after 1.0
**Research:** [stdlib-module-matrix.md](../../research/stdlib-module-matrix.md)

## Complete: set module (20 functions)

Expanded from 11 to 20 functions. On par with Gleam set (20 functions).

### Added Functions (9)

| Function | Description | Other Languages |
|----------|-------------|-----------------|
| `symmetric_difference(a, b)` | Symmetric difference | Rust, Gleam, Python, Elixir |
| `is_subset(a, b)` | a ⊆ b | Rust, Gleam, Python, Elixir |
| `is_disjoint(a, b)` | No common elements | Rust, Gleam, Python, Elixir |
| `filter(s, f)` | Keep only predicate matches | Gleam, Elm, Kotlin, Elixir |
| `map(s, f)` | Transform elements | Gleam, Elm, Kotlin |
| `fold(s, init, f)` | Accumulate with initial value | Gleam, Elm, Kotlin, Elixir |
| `each(s, f)` | Iterate with side effects | Consistent with list/map |
| `any(s, f)` | Any element is true | Consistent with list/map |
| `all(s, f)` | All elements are true | Consistent with list/map |

## Deferred Candidates (2.x+)

| Module | Rationale |
|--------|-----------|
| **net** (TCP/UDP) | Rust✅ Go✅. Lower layer than http. On demand |
| **encoding** (base64 etc.) | Go✅. Currently provided as .almd. TOML promotion on demand |
| **channel** | Go✅ Rust✅. Under consideration as fan extension |

## Design Principles

- **Add conservatively** — once in stdlib, it is frozen
- **Try in packages before promoting to stdlib** — Deno model
- **Only things that work multi-target** — must be meaningful for both Rust + TS
