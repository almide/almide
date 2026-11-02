<!-- description: Remaining dependency-range semantics and available-version selection beyond the shipped MVS resolver -->
# Dependency Version Constraints

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](package-version-resolution.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](package-version-resolution.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](package-version-resolution.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-version-resolution.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


## Shipped foundation

MVS, semver ordering, lock files and `almide update` are already implemented.
The current behavior belongs in the [package-system specification](../../../../../specs/package-system.md),
not a second implementation plan:

- For the same package identity, the maximum requested version wins;
  different majors coexist. `src/project_fetch.rs::semver_of` and the
  dependency walk implement this. `tests/mvs_resolution_test.rs` pins
  selection and declaration-order independence.
- `almide.lock` records the selected checkout's source, ref and commit for
  each direct dependency. `tests/dep_lock_pairing_test.rs` and
  `tests/lock_roundtrip_test.rs` cover the selected-version pairing and format.
- `almide update [dep]` advances floating refs; a tag pin is not advanced.
  See the [CLI reference](../../../../../specs/cli.md#almide-update) and
  `src/project_fetch.rs::update_locked_deps`.

## Remaining proposal — on hold

The shipped maximum-requested-version rule is **not** a general dependency
constraint solver. `resolve_dep_version` strips leading non-digits and pads
version components; it does not evaluate caret, tilde or compound ranges.
Fetching still selects `tag`, `branch`, or `main` through `dep_ref_name`.
A `version` field must not be presented as a promise that a matching tag is
searched for and selected.

- Define supported range syntax and its interaction with MVS, pre-1.0
  identity, prereleases and incompatible upper bounds.
- Discover available versions from git tags (or a future registry) and
  select a version satisfying the declared constraints; reject invalid or
  unsatisfiable requests with a diagnostic naming the dependency paths.
- Specify how range-aware `add` and `update` interact with existing locks
  and exact tag pins, then cover those rules with reproducibility tests.
- Decide the integrity metadata required for newly selected versions;
  [central-registry work](../../../../../roadmap/on-hold/package-registry.md) owns registry provenance.

Do not add a second semver parser or replace the existing lock format merely
to implement this proposal. Reuse the existing foundation and extend its
executable tests.

## Design references

- [Go MVS](https://research.swtch.com/vgo-mvs) — maximum of requested minimums
- [Cargo resolver](https://doc.rust-lang.org/cargo/reference/resolver.html) — range-resolution comparison
