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

```
PkgId { name: String, major: u64 }
```

- Derived from `almide.toml`'s `[package] version` or the git tag.
- `0.x.y` uses minor as major (pre-1.0 breaking changes).
- `mod_name()` returns `"{name}_v{major}"` for codegen symbol namespacing.

## 3. Version Resolution: MVS

When multiple dependents request the same package (same name, same major):

```
B requires D >= 1.2.0
C requires D >= 1.5.0
→ resolve to D 1.5.0 (maximum of minimums)
```

When majors differ:

```
B requires D v1.x (major=1)
C requires D v2.x (major=2)
→ both coexist as separate modules (D_v1, D_v2)
→ D_v1.Logger ≠ D_v2.Logger (different types)
```

## 4. Module Boundaries

### 4.1 Direct vs Transitive

```
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

```almide
// B/mod.almd
import D
type Logger = D.Logger  // re-export (future: explicit pub use)
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

Short form (defaults to github.com/almide/):
```bash
almide add bindgen@v0.1.0
# → git = "https://github.com/almide/almide-bindgen", tag = "v0.1.0"
```

## 7. Lock File

```toml
# almide.lock (auto-generated, commit to VCS)
[bindgen]
git = "https://github.com/almide/almide-bindgen"
ref = "v0.1.0"
commit = "a629eded8d20..."

[json]
git = "https://github.com/almide/json"
ref = "v2.0.0"
commit = "b8f3a1..."
```

## 8. Resolution Algorithm

```
1. Parse almide.toml → direct dependencies
2. For each dep:
   a. If almide.lock has commit → use exact commit (reproducible)
   b. Else fetch tag/branch → record commit in lock
3. Parse dep's almide.toml → transitive dependencies
4. Recurse (depth-first, leaves first)
5. Dedup by PkgId(name, major):
   - Same (name, major): keep maximum requested version (MVS)
   - Different major: both coexist with versioned names
6. Detect impossible constraints → error with explanation
```

## 9. Error Messages

```
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
