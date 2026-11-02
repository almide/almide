<!-- description: Central package publishing, ownership and provenance beyond the existing git-based package system -->
# Central Package Registry

## Existing foundation

Almide already has git-based package fetching, `almide add <name>` shorthand,
MVS and commit-pinned lock files. Shorthand resolves through the bundled
package-name map or the default GitHub owner; it is **not** a central registry.
Current behavior and source-trust rules are documented in the
[package-system specification](../../specs/package-system.md) and
[CLI reference](../../specs/cli.md#almide-add).

Implementation evidence: `src/project_fetch.rs::resolve_add_target`,
`resolve_package_spec`, `update_lock_file` and `update_locked_deps`;
`src/almide_packages.txt`; `tests/mvs_resolution_test.rs`,
`tests/dep_lock_pairing_test.rs` and `tests/lock_roundtrip_test.rs`.

Lock-file generation and basic version selection are no longer backlog
items. Remaining dependency-range semantics are tracked separately in
[Dependency Version Constraints](package-version-resolution.md).

## Remaining proposal — on hold

- A registry service with package/version metadata and a defined mapping
  from a package name to its published content.
- A publishing workflow (`almide publish` is a proposed command, not a
  shipped interface), including immutable versions and publication validation.
- Name reservation, ownership, transfers and recovery rules.
- Integrity and provenance verification for registry content, plus a clear
  trust boundary distinct from the current git-owner and native-code grants.
- Migration from explicit git sources without changing the identity or
  reproducibility of already-locked dependencies.

Hosting and distribution format remain design choices. Do not advertise a
registry endpoint or treat today's name shorthand as registry publication.
