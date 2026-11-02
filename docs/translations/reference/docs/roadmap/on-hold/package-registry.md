<!-- description: Central package publishing, ownership and provenance beyond the existing git-based package system -->
# Central Package Registry

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](package-registry.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](package-registry.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](package-registry.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/package-registry.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


## Existing foundation

Almide already has git-based package fetching, `almide add <name>` shorthand,
MVS and commit-pinned lock files. Shorthand resolves through the bundled
package-name map or the default GitHub owner; it is **not** a central registry.
Current behavior and source-trust rules are documented in the
[package-system specification](../../../../../specs/package-system.md) and
[CLI reference](../../../../../specs/cli.md#almide-add).

Implementation evidence: `src/project_fetch.rs::resolve_add_target`,
`resolve_package_spec`, `update_lock_file` and `update_locked_deps`;
`src/almide_packages.txt`; `tests/mvs_resolution_test.rs`,
`tests/dep_lock_pairing_test.rs` and `tests/lock_roundtrip_test.rs`.

Lock-file generation and basic version selection are no longer backlog
items. Remaining dependency-range semantics are tracked separately in
[Dependency Version Constraints](../../../../../roadmap/on-hold/package-version-resolution.md).

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
