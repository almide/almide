<!-- description: Dependency lockfile with git-based resolution and reproducible builds -->
<!-- done: 2026-03-17 -->
# almide.lock [DONE — 1.0 Phase III]

## Implemented

- [x] `[dependencies]` section in `almide.toml`
- [x] `almide.lock` generation (auto-generated on `almide build`)
- [x] Dependency resolution: git clone + tag/branch/commit specification
- [x] `almide add <pkg>`: dependency add command (short specifier: `user/repo@tag`)
- [x] Reproducible builds with locked commits
- [x] Recursive dependency resolution (transitive deps)
- [x] Dependency unification/coexistence by major version

## Not Included in 1.0

- Package registry (on-hold/package-registry.md)
- Version range resolution (semver constraint solving)
- Private registry
- Workspaces (monorepo)
