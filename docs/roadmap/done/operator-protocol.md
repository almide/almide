<!-- description: Convention-based operator dispatch (==, repr, sort, hash) -->
<!-- done: 2026-03-15 -->
# Operator Protocol

Operator and language feature dispatch based on convention declarations.
Built on top of Derive Conventions Phase 1-2 (convention declaration + method resolution).

## Scope

| Situation | Transformation | Prerequisite |
|------|------|------|
| `a == b` where a: Dog | `Dog.eq(a, b)` | `type Dog: Eq` |
| `"${d}"` where d: Dog | `Dog.repr(d)` | `type Dog: Repr` |
| `list.sort(dogs)` | `Dog.ord` を comparator に | `type Dog: Ord` |
| `map[dog]` | `Dog.hash(dog)` をキーに | `type Dog: Hash` |

## Implementation

### `==` / `!=` dispatch
- checker: when `a == b` and `a`'s type has `deriving Eq`, use `Dog.eq(a, b)` if it exists
- Currently `almide_eq!` macro makes `==` work for all types, so **dispatch only when a custom eq is defined**
- codegen: switch from `almide_eq!(a, b)` to `Dog_eq(a.clone(), b.clone())`

### String interpolation dispatch
- lower: when `"${d}"` string interp and `d`'s type has `deriving Repr`, insert `Dog.repr(d)`
- Currently outputs with `format!("{:?}", d)` (Debug); if custom repr exists, switch to `format!("{}", Dog_repr(d))`

### Sort dispatch
- Auto-insert `Dog.ord` as the comparator argument for stdlib `list.sort`
- codegen generates `dogs.sort_by(|a, b| Dog_ord(a, b))`

## Priority
String interpolation > `==` dispatch > sort. Auto-derive (below) may need to come first.

---

# Auto Derive

When a convention function is undefined, the compiler auto-generates it.

| Convention | Auto-derive Content |
|-----------|---------------------|
| `Eq` | Compare all fields with `==` |
| `Repr` | `"TypeName { field1: value1, ... }"` format |
| `Ord` | Lexicographic comparison in field order |
| `Hash` | Combine hash of all fields |

## Implementation
- In the IR lowering pass, auto-generate `IrFunction` when `deriving Eq` but `Dog.eq` is undefined
- Field list obtained from `IrTypeDecl`
- Rust codegen already emits `#[derive(PartialEq)]`, so auto-derive is unnecessary for the Rust target
- Needed for TS/IR interpreter

## Files
```
src/lower.rs       — auto-derive function generation
src/check/mod.rs   — operator dispatch resolution
src/optimize.rs    — string interp rewrite (optional)
```
