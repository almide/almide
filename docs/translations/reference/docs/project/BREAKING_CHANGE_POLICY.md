# Breaking Change Policy

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](BREAKING_CHANGE_POLICY.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](BREAKING_CHANGE_POLICY.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](BREAKING_CHANGE_POLICY.zh-CN.md)

> Non-normative reader translation of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/BREAKING_CHANGE_POLICY.md); the original remains authoritative. This copy is not automatically synchronized.




> Lesson from Python 2→3: silent behavior changes place a heavy burden on the ecosystem.
> Lesson from Swift 1→2→3: successive breaking releases increase migration costs for users.
> Lesson from Go 1: “code that compiles today will compile forever” is its greatest strength.

---

<a id="現行実装と-post-10-方針の区別"></a>

## Distinguishing the current implementation from the post-1.0 policy

The post-1.0 / edition policies below are a design for future operations, not currently implemented features. In the current `src/project.rs::PACKAGE_METADATA_KEYS`, `edition` is accepted as metadata that does not change behavior. Neither edition-specific parser switching nor an `almide migrate` subcommand is implemented. Removed syntax and migration hints are currently handled by `proofs/dialect-epochs.toml` and the checker. The future policy itself is preserved below.

<a id="原則"></a>

## Principles

<a id="1-silent-な挙動変更は絶対禁止"></a>

### 1. Silent behavior changes are strictly prohibited

After 1.0, changes must not alter the meaning of existing code. If the behavior of a type or operator needs to change:
- Old code must produce a **compile error**, rather than silently returning a different result.
- The error message must include a **migration hint**.
- The new behavior must be enabled only in a new edition.

<a id="2-api-削除は-2-minor-version-の-deprecation-後"></a>

### 2. API removal follows deprecation over 2 minor versions

Removing or renaming a stdlib function:
1. **v1.x**: emit a `#[deprecated]` warning (compilation still succeeds).
2. **v1.(x+1)**: continue the warning.
3. **v1.(x+2)**: removal is permitted, but a compile error + hint must point to the replacement function.

<a id="3-構文変更は-edition-で吸収post-10-構想"></a>

### 3. Editions absorb syntax changes (post-1.0 concept)

New keywords and syntax changes are controlled by the `edition` field:
- Code with `edition = "2026"` compiles under the 2026 rules.
- `edition = "2027"` enables the new syntax.
- Modules from different editions can interoperate (as with Rust editions).
- An automatic conversion tool is provided through `almide migrate --edition 2027`.

<a id="4-コア型のシグネチャは不変"></a>

### 4. Core type signatures are immutable

After 1.0, the signatures of existing methods/functions on the following types must not change:
- `String`, `Int`, `Float`, `Bool`
- `List[T]`, `Map[K, V]`
- `Option[T]`, `Result[T, E]`
- `Tuple`, `Record`, `Variant`

Functions may be **added**. **Changing argument or return types** of existing functions is prohibited.

---

<a id="10-前の-breaking-change"></a>

## Breaking changes before 1.0

Breaking changes are allowed before 1.0, provided that:
- They are explicitly documented in CHANGELOG.
- A compile error with a migration hint is produced wherever possible.
- The stdlib-verb-system reform is completed before 1.0.

---

<a id="edition-の仕組み未実装の設計"></a>

## How editions work (unimplemented design)

```toml
# almide.toml
[package]
name = "myapp"
version = "0.1.0"
edition = "2026"
```

- If `edition` is omitted, the compiler's latest edition is the default.
- Dependencies from different editions can coexist without problems.
- Editions are updated every 2-3 years (following Rust's 3-year cycle).
