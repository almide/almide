# almide-types

Defines the internal resolved type representation (`Ty`) and type system utilities.

## Key Types

- **`Ty`** — Resolved type enum. Scalars, `Applied(TypeConstructorId, args)` for containers, `Record`/`Variant`/`Fn`/`Tuple`, `TypeVar`, `Unknown`, `Never`.
- **`TypeConstructorId`** — Unified parameterized type identity (List, Option, Result, Map, Set, UserDefined).
- **`ProtocolDef`** / **`FnSig`** — Protocol method signatures and function signatures with bounds.

## Rules

- **`Applied` is the canonical container form.** `List[Int]` = `Applied(List, [Int])`. Never represent containers as `Named("List", [Int])`.
- **`Unknown` is for error recovery.** It unifies with everything to prevent cascade errors. It is NOT a valid runtime type — codegen should treat it as a fallback (usually `i32` in WASM).
- **`TypeVar` is for generics before monomorphization.** After mono, no `TypeVar` should remain. Use `Ty::is_unresolved()` to check for both `Unknown` and `TypeVar`.
- **`OpenRecord` is a structural bound.** `{ name: String, .. }` means "has at least a `name` field." It becomes a concrete `Record` or `Named` after inference. Use `Ty::is_unresolved_structural()` when open records should be treated as incomplete.
- **stdlib_info is hardcoded.** Auto-import lists, UFCS resolution tables, module categories — all in `stdlib_info.rs`. Update these when adding new stdlib modules.
