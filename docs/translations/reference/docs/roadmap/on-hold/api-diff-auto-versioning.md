# API Diff & Automatic Versioning

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](api-diff-auto-versioning.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](api-diff-auto-versioning.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](api-diff-auto-versioning.zh-CN.md)



> Non-normative reader translation of the [pinned source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/api-diff-auto-versioning.md). This is a fixed historical snapshot, not an automatically synchronized document. Status, plans, and counts are those of the source; they do not establish current implementation or completion. This translation did not independently execute or verify the claims.

> Source description: Package-facing API diff and version advice beyond the shipped stdlib interface gate.

<a id="実装済みの土台"></a>

## Implemented foundation

Changes to the standard library's public API are already checked by [`scripts/check-interface-diff.sh`](../../../../../../scripts/check-interface-diff.sh). This release gate compares generated signature indexes between git refs, classifies changes as `identical` / `additive` / `breaking`, and fails on breaking changes that have not been explicitly allowed.

[`check-interface-diff-negative.sh`](../../../../../../scripts/check-interface-diff-negative.sh) provides negative controls using removals and additions of pure / effect functions. This mechanism already exists, but it is neither a general package-facing CLI nor automatic version updating.

<a id="残る提案"></a>

## Remaining proposal

- Compare a package's public API through a user-facing CLI.
- Define structural comparison rules that account for matters such as type-variable renaming.
- Recommend MAJOR/MINOR/PATCH from the diff. Behavior can change even when signatures remain the same, so API diffs alone do not guarantee compatibility as a whole.

The following is a design example for an unimplemented CLI. Do not present it as an available command.

```text
almide diff v0.1.0..v0.2.0
# Added: yaml.parse_all (MINOR)
# Changed: yaml.stringify return type String → Result[String, String] (MAJOR)
# Suggested version: v1.0.0
```

<a id="設計参考"></a>

## Design reference

Elm's `Deps/Diff.hs` / `elm diff`: structural comparison of type signatures, equivalence accounting for type-variable names, and classification of module/function additions and type changes. Treat the existing stdlib gate and this package-facing proposal as having separate scopes and completion criteria.
