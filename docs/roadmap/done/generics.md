<!-- description: Generic functions, records, variants, and recursive generics -->
# Generics

## Status: Phase 1 Complete + Recursive Variants

Generic functions, generic record types, generic variant types, call-site type arguments, and recursive generic variants are fully implemented across all compiler layers.

## Working Syntax

```almide
// generic function
fn identity[T](x: T) -> T = x
fn pair[A, B](a: A, b: B) -> (A, B) = (a, b)
fn wrap_list[T](x: T) -> List[T] = [x]

// generic record type
type Stack[T] = { items: List[T], size: Int }
type Pair[A, B] = { fst: A, snd: B }

// functions using generic types
fn stack_push[T](s: Stack[T], item: T) -> Stack[T] = {
  { items: s.items ++ [item], size: s.size + 1 }
}

fn swap_pair[A, B](p: Pair[A, B]) -> Pair[B, A] = {
  { fst: p.snd, snd: p.fst }
}
```

## Implementation Details

| Layer | Status | Details |
|-------|--------|---------|
| AST | Done | `generics: Option<Vec<GenericParam>>` on Fn, Type, Trait, Impl |
| Parser | Done | `try_parse_generic_params()` extracts `[T, U]` |
| Type System | Done | `Ty::TypeVar(name)`, `FnSig.generics`, TypeVar compatible with everything |
| Type Checker | Done | TypeVars registered/cleaned per decl, `resolve_named` for return type compat |
| Rust Emitter | Done | `<T: Clone + Debug + PartialEq>` on fns, structs, enums, impl blocks |
| TS Emitter | Done | `<T, U>` on functions, interfaces, type aliases (erased in JS mode) |
| Record Resolution | Done | Anonymous record literals auto-resolve to named struct types |

## Design Decisions

- **No trait bounds yet** — `T` accepts anything. AI-friendliness > type safety rigor
- **Rust bounds are auto-derived**: `Clone + Debug + PartialEq` (minimum for Almide runtime)
- **Type inference only** — no explicit type args at call sites (see roadmap below)
- **`use Enum::*`** skipped for generic enums (Rust doesn't allow it)

## Roadmap

### Call-site Type Arguments — Done

```almide
// All working:
identity[Int](42)
stack_new[Int]()
pair[Int, String](1, "hello")
```

- Parser: `peek_type_args_call()` lookahead distinguishes `f[Type](args)` from list indexing
- Rust emitter: turbofish `f::<i64>(args)`
- TS emitter: type args erased (TS infers from usage)
- Formatter: roundtrips `f[Type](args)` correctly

### Generic Variant Types — Done

```almide
// Working:
type Maybe[T] =
  | Just(T)
  | Nothing

fn from_option[T](opt: Option[T]) -> Maybe[T] =
  match opt {
    some(v) => Just(v)
    none => Nothing
  }
```

- Rust: wrapper constructor functions generated for each case (since `use Enum::*` doesn't work with generics)
- Unit constructors (e.g. `Nothing`) auto-call as `Nothing()` in expression context
- Pattern matching uses qualified `Maybe::Just(v)` / `Maybe::Nothing`
- Comparison operators (`>`, `<`, etc.) work on generic types via `PartialOrd` bound

### Recursive Generic Variants — Done

```almide
// Working:
type Tree[T] =
  | Leaf(T)
  | Node(Tree[T], Tree[T])

fn tree_sum(t: Tree[Int]) -> Int =
  match t {
    Leaf(v) => v
    Node(left, right) => tree_sum(left) + tree_sum(right)
  }
```

- Rust: auto-detects self-referencing fields and wraps with `Box<>`
- Constructor wrappers accept unboxed values, insert `Box::new()` internally
- Pattern matching auto-derefs: `Node(__boxed_left, __boxed_right)` + `let left = *__boxed_left;`
- `tree_map` with closures works correctly across recursive structures

### Stdlib Generics: map.new, map.from_entries, map.from_list ✅

`map.new`, `map.from_entries`, and `map.from_list` previously used `Ty::Unknown` — no type checking on keys/values. Now fully generic:

```almide
let m = map.new[String, Int]()          // explicit type args
let m = map.new()                       // inferred from usage
let m = map.from_entries([("a", 1)])    // K=String, V=Int inferred
```

Call-site type arguments (`[String, Int]`) are emitted as Rust turbofish (`::<String, i64>`) via `gen_module_call` propagation through generated codegen.

### Trait Bounds (Future, post-trait system)

```almide
// Future syntax:
fn sort[T: Ord](xs: List[T]) -> List[T] = ...
```

Depends on trait system maturation. Low priority for AI proliferation since most LLM-generated code uses concrete types.

---
