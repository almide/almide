<!-- description: Lock file, semver resolution, and central package registry -->
# Package Registry

## Current State (v0.5.13)

- Dependencies are Git URL only: `almide.toml` references `git = "https://github.com/..."`
- No central registry, no `almide add <name>` shorthand
- No lock file — dependency versions are not pinned
- Git clone cached in `.almide/deps/` but no version tracking
- Diamond dependencies handled via versioned module names (PkgId)

## What's Needed

### 1. Lock File (`almide.lock`)

Pin exact commit hashes for reproducible builds:

```toml
# almide.lock — auto-generated, do not edit
[[package]]
name = "http-utils"
git = "https://github.com/example/http-utils"
rev = "a1b2c3d4e5f6"

[[package]]
name = "json-schema"
git = "https://github.com/example/json-schema"
rev = "f6e5d4c3b2a1"
```

- `almide build` reads lock file if present; creates one if missing
- `almide update` refreshes lock file to latest compatible versions
- Lock file should be committed to version control

### 2. Semver Resolution

`almide.toml` version constraints:

```toml
[dependencies]
http-utils = { git = "...", version = "^1.2.0" }
```

- Read version from dependency's `almide.toml`
- Resolve compatible versions across transitive dependencies
- Error on incompatible version constraints

### 3. Central Registry (Future)

- `almide add fizzbuzz` → fetch from registry
- Package publishing: `almide publish`
- Name reservation, ownership
- Hosted at `registry.almide.dev` or similar

### Priority

Lock file (P1) → semver resolution (P2) → central registry (P3)

Lock file is the most impactful — it enables reproducible builds with zero infrastructure investment.
