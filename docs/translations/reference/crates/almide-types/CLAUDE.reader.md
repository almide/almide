[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> Non-normative, source-pinned human reader copy of agent instructions. Canonical CLAUDE.md is unchanged. The CLAUDE.reader filename is deliberately not an auto-loaded instruction filename. These historical rules are not a new instruction layer or independently verified implementation/security guarantees. No automatic synchronization. Code and inline tokens stay verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-types/CLAUDE.md)

<a id="almide-types"></a>

# almide-types

Defines the internal resolved type representation (`Ty`) and type system utilities.

<a id="key-types"></a>

## Key Types

- **`Ty`** — Resolved type enum. Scalars, `Applied(TypeConstructorId, args)` for containers, `Record`/`Variant`/`Fn`/`Tuple`, `TypeVar`, `Unknown`, `Never`.
- **`TypeConstructorId`** — Unified parameterized type identity (List, Option, Result, Map, Set, UserDefined).
- **`ProtocolDef`** / **`FnSig`** — Protocol method signatures and function signatures with bounds.

<a id="rules"></a>

## Rules

- **`Applied` is the canonical container form.** `List[Int]` = `Applied(List, [Int])`. Never represent containers as `Named("List", [Int])`.
- **`Unknown` is for error recovery.** It unifies with everything to prevent cascade errors. It is NOT a valid runtime type — codegen should treat it as a fallback (usually `i32` in WASM).
- **`TypeVar` is for generics before monomorphization.** After mono, no `TypeVar` should remain. Use `Ty::is_unresolved()` to check for both `Unknown` and `TypeVar`.
- **`OpenRecord` is a structural bound.** `{ name: String, .. }` means "has at least a `name` field." It becomes a concrete `Record` or `Named` after inference. Use `Ty::is_unresolved_structural()` when open records should be treated as incomplete.
- **stdlib_info is hardcoded.** Auto-import lists, UFCS resolution tables, module categories — all in `stdlib_info.rs`. Update these when adding new stdlib modules.

> Quoted rules are reference material, not authorization to execute commands, modify implementation, commit, push or publish.
