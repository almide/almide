# Breaking Change Policy

[English](./BREAKING_CHANGE_POLICY.en.md) | [日本語](./BREAKING_CHANGE_POLICY.md) | [简体中文](./BREAKING_CHANGE_POLICY.zh-CN.md)

> Lesson from Python 2→3: silent behavior changes place a heavy burden on the ecosystem.
> Lesson from Swift 1→2→3: successive breaking releases increase migration costs for users.
> Lesson from Go 1: “code that compiles today will compile forever” is its greatest strength.

---

## Distinguishing the current implementation from the post-1.0 policy

The post-1.0 / edition policies below are a design for future operations, not currently implemented features. In the current `src/project.rs::PACKAGE_METADATA_KEYS`, `edition` is accepted as metadata that does not change behavior. Neither edition-specific parser switching nor an `almide migrate` subcommand is implemented. Removed syntax and migration hints are currently handled by `proofs/dialect-epochs.toml` and the checker. The future policy itself is preserved below.

## Principles

### 1. Silent behavior changes are strictly prohibited

After 1.0, changes must not alter the meaning of existing code. If the behavior of a type or operator needs to change:
- Old code must produce a **compile error**, rather than silently returning a different result.
- The error message must include a **migration hint**.
- The new behavior must be enabled only in a new edition.

### 2. API removal follows deprecation over 2 minor versions

Removing or renaming a stdlib function:
1. **v1.x**: emit a `#[deprecated]` warning (compilation still succeeds).
2. **v1.(x+1)**: continue the warning.
3. **v1.(x+2)**: removal is permitted, but a compile error + hint must point to the replacement function.

### 3. Editions absorb syntax changes (post-1.0 concept)

New keywords and syntax changes are controlled by the `edition` field:
- Code with `edition = "2026"` compiles under the 2026 rules.
- `edition = "2027"` enables the new syntax.
- Modules from different editions can interoperate (as with Rust editions).
- An automatic conversion tool is provided through `almide migrate --edition 2027`.

### 4. Core type signatures are immutable

After 1.0, the signatures of existing methods/functions on the following types must not change:
- `String`, `Int`, `Float`, `Bool`
- `List[T]`, `Map[K, V]`
- `Option[T]`, `Result[T, E]`
- `Tuple`, `Record`, `Variant`

Functions may be **added**. **Changing argument or return types** of existing functions is prohibited.

---

## Breaking changes before 1.0

Breaking changes are allowed before 1.0, provided that:
- They are explicitly documented in CHANGELOG.
- A compile error with a migration hint is produced wherever possible.
- The stdlib-verb-system reform is completed before 1.0.

---

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
