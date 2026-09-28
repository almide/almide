# Package System Specification

> Almide's dependency management, version resolution, and module isolation.
> Design informed by Go Modules (MVS), Cargo (type identity), pnpm (boundary enforcement).

## 1. Design Principles

1. **Minimal Version Selection** — Always pick the minimum version that satisfies all constraints. No SAT solver. Deterministic, reproducible, fast. (Go Modules)
2. **Strict module boundaries** — Code can only access modules it directly imports. Transitive dependencies are invisible. No phantom dependencies. (pnpm)
3. **Type identity by (name, major)** — Two packages with the same `(name, major)` produce the same types. Different majors produce incompatible types. (Cargo)
4. **Single source of truth** — `almide.toml` declares intent, `almide.lock` records exact commits. Lock file is always respected.
5. **Future-proof for registry** — Current git-based system will be supplemented by an Almide package registry. Design decisions must not preclude this.

## 2. Package Identity

```text
PkgId { name: String, major: u64 }
```

- Derived from `almide.toml`'s `[package] version` or the git tag.
- `0.x.y` uses minor as major (pre-1.0 breaking changes).
- `mod_name()` returns `"{name}_v{major}"` for codegen symbol namespacing.

## 3. Version Resolution: MVS

When multiple dependents request the same package (same name, same major):

```text
B requires D >= 1.2.0
C requires D >= 1.5.0
→ resolve to D 1.5.0 (maximum of minimums)
```

When majors differ:

```text
B requires D v1.x (major=1)
C requires D v2.x (major=2)
→ both coexist as separate modules (D_v1, D_v2)
→ D_v1.Logger ≠ D_v2.Logger (different types)
```

## 4. Module Boundaries

### 4.1 Direct vs Transitive

```text
A imports B, C
B imports D
C imports D
```

From A's perspective:
- `B.func()` ✓ — direct dependency
- `C.func()` ✓ — direct dependency
- `D.func()` ✗ — transitive only (unless A also imports D)
- `D.Logger` ✗ — type not visible unless A imports D

If A needs D's types, A must declare `import D` explicitly.

### 4.2 Enforcement

The type checker tracks which modules each file has imported via `import` statements.
`resolve_module_call` and `resolve_static_member` check against this set, not the global `user_modules`.

### 4.3 Re-exports

A module can re-export a dependency's types by wrapping:

```almide project
// file: d/mod.almd
type Logger = { name: String }
fn make(n: String) -> Logger = Logger { name: n }
// file: b/mod.almd
import d
type Logger = d.Logger  // re-export (future: explicit pub use)
fn make(n: String) -> Logger = d.make(n)
fn name_of(l: Logger) -> String = l.name
// file: main.almd
import b

test "the consumer sees D's type through B without importing D" {
  let l: b.Logger = b.make("x")
  assert_eq(b.name_of(l), "x")
}
```

## 5. Codegen: Versioned Symbols

When `IrModule.versioned_name` is set (e.g., `"json_v2"`), codegen uses it for function prefixes:

```rust
// Without versioning (same major, no conflict)
pub fn almide_rt_json_parse(...) { ... }

// With versioning (different majors coexist)
pub fn almide_rt_json_v1_parse(...) { ... }
pub fn almide_rt_json_v2_parse(...) { ... }
```

Struct names are also versioned to prevent type collisions:

```rust
pub struct JsonV1_Config { ... }
pub struct JsonV2_Config { ... }
```

## 6. Dependency Declaration

```toml
# almide.toml
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
bindgen = { git = "https://github.com/almide/almide-bindgen", tag = "v0.1.0" }
json = { git = "https://github.com/almide/json", tag = "v2.0.0" }
```

**Each key appears once per table, and each table once** (#2583). TOML forbids
a repeated key, and every command that reads `almide.toml` (`check`, `run`,
`test`, `build`, …) refuses one before fetching anything or touching the lock,
naming both lines:

```text
error: almide.toml:7: dependency `almai` is declared twice in [dependencies] (first at line 6)
  hint: keep one of lines 6 and 7 and delete the other — TOML allows a key only once per table
```

The rule covers every table the manifest reader reads (`[package]`,
`[dependencies]`, `[permissions]`, `[native-deps]`), including a repeated
table header. Two spellings of the same url (`…/almai` and `…/almai.git`) are
still one dependency declared twice.

Test: `tests/manifest_duplicate_key_test.rs`.

Short form (defaults to github.com/almide/):
```bash
almide add bindgen@v0.1.0
# → git = "https://github.com/almide/almide-bindgen", tag = "v0.1.0"
```

## 7. Lock File

```toml
# almide.lock — auto-generated, do not edit

bindgen = { git = "https://github.com/almide/almide-bindgen", ref = "v0.1.0", commit = "a629eded8d20..." }
json = { git = "https://github.com/almide/json", ref = "v2.0.0", commit = "b8f3a1..." }
```

One entry per direct dependency, one line each; the writer never emits a name
twice. A lock that holds a name twice (written by a compiler before #2583
from a manifest that declared the dependency twice) is refused with the two
lines and the way out — delete one of them, keeping the one whose `git`
matches `almide.toml`, or delete the lock and let the next run rewrite it.

**The lock records the resolved result, not the request** (#2532). Each entry's
`ref` and `commit` are those of the version Minimal Version Selection chose for
that dependency's package — the checkout that was actually built — as
`Cargo.lock` does. The request lives only in `almide.toml`. When nothing raises
a dependency the two coincide. When a transitive requirement raises it (the
manifest asks `foo@v1.0.0`, a dependency asks `foo@v1.1.0`), the entry reads
`ref = "v1.1.0"` with v1.1.0's commit, even though `almide.toml` still says
`v1.0.0`; that is not drift and does not re-resolve. Every entry is one fetch's
own `(git, ref, commit)`, so it is always a triple that was true together.

A pin applies to any request for the same source at the same ref, whoever in
the graph makes it, so the locked commit is exactly what the next build
compiles. A request that was walked but not selected (the `v1.0.0` above) is
resolved from its ref — MVS reads its manifest for requirements, it does not
build it. Changing a request in `almide.toml` re-resolves as before, and the
lock is rewritten from the new selection. A lock written before #2532, which
recorded the root's request, still reads; the next run rewrites the raised
entries to the resolved record.

Test: `tests/manifest_duplicate_key_test.rs`, `tests/lock_roundtrip_test.rs`,
`tests/dep_lock_pairing_test.rs`
(`a_dependency_raised_by_mvs_is_locked_at_the_version_that_was_built`).

## 8. Resolution Algorithm

```text
1. Parse almide.toml → direct dependencies
2. For each dep:
   a. If almide.lock has an entry for this (git, ref) → use its exact commit
   b. Else fetch tag/branch
3. Parse dep's almide.toml → transitive dependencies
4. Recurse (depth-first, leaves first)
5. Dedup by PkgId(name, major):
   - Same (name, major): keep maximum requested version (MVS)
   - Different major: both coexist with versioned names
6. Detect impossible constraints → error with explanation
7. Write almide.lock: per direct dependency, the (git, ref, commit) of the
   version step 5 selected for its package
```

## 9. Error Messages

```text
error: version conflict for package 'json'
  → myapp requires json >= 2.0.0 (via almide.toml)
  → bindgen requires json >= 1.0.0, < 2.0.0 (via bindgen/almide.toml)

  json v1.x and v2.x are different major versions and will coexist.
  However, json.Config from v1 cannot be passed to functions expecting json.Config from v2.

  hint: Update bindgen to a version that supports json v2.x,
        or add json v1.x as a separate dependency.
```

## 10. Future: Almide Package Registry

The registry will:
- Host packages with semver metadata
- Provide `almide add pkg` without git URLs
- Support `almide publish` for package authors
- Use content-addressed storage (like pnpm) for dedup
- Enforce package signing and provenance (like JSR)

The current git-based system is forward-compatible:
- `PkgId` already supports semver
- `almide.lock` already records exact commits
- `versioned_name` already supports coexistence
- Migration: `git = "..."` → `registry = "almide"` (or just name)

## References

- [Go Modules: Minimal Version Selection](https://research.swtch.com/vgo-principles)
- [How Rust Solved Dependency Hell](https://stephencoakley.com/2019/04/24/how-rust-solved-dependency-hell)
- [PubGrub: Next-Generation Version Solving](https://nex3.medium.com/pubgrub-2fb6470504f)
- [pnpm: Flat node_modules is not the only way](https://pnpm.io/blog/2020/05/27/flat-node-modules-is-not-the-only-way)
