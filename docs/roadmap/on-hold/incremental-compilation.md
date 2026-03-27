<!-- description: Skip redundant rustc invocations by hashing generated Rust source -->
# Incremental Compilation [ON HOLD]

## Problem

Every `almide run` / `almide build` performs the full pipeline from scratch:
```
.almd → lex → parse → check → lower → IR → emit Rust → rustc → binary
```

For a 500-line program, the Almide compiler itself is fast (~50ms), but `rustc` invocation dominates (~1-3s). Re-running `rustc` on identical generated Rust is pure waste.

## Design

### Level 1: Skip rustc When Unchanged

**Cheapest win.** Hash the generated Rust source and skip `rustc` if the hash matches the last compilation.

```
almide run app.almd
  → generate Rust source
  → hash(generated_source) == hash(cached_source)?
    → yes: run cached binary directly
    → no:  invoke rustc, cache binary + hash
```

**Cache location:** `.almide/cache/` in project root (or `~/.cache/almide/` for global)

**Cache key:** SHA-256 of generated Rust source + rustc flags + target

**Implementation:** ~30 lines in `src/cli.rs`. Check hash before `rustc` invocation.

### Level 2: Module-Level Caching

For multi-module projects, only recompile modules whose source changed.

```
import utils    # unchanged since last build
import config   # changed

→ reuse cached utils.rs, recompile config.rs only
```

**Requires:** Dependency tracking between modules (already available from `resolve.rs`).

### Level 3: Incremental Checking

Cache type-check results per module. If a module's source hasn't changed and its dependencies' signatures haven't changed, skip re-checking.

**Requires:** Serializable `TypeEnv` state per module. Higher complexity.

## Priority

| Level | Impact | Difficulty | Priority |
|-------|--------|------------|----------|
| 1. Skip rustc | High (1-3s saved) | Very Low | P0 |
| 2. Module cache | Medium | Medium | P2 |
| 3. Incremental check | Low (compiler is fast) | High | P3 |

## Affected Files

| File | Change |
|------|--------|
| `src/cli.rs` | Hash check before rustc, cache management |
| `src/main.rs` | Cache directory initialization |
