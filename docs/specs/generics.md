# Generics Specification

The generics specification is part of the [Type System Specification](./type-system.md).

See **Section 4 (Generics)** in `docs/specs/type-system.md`, which comprehensively covers:

- **4.1** Syntax — `[]` notation for type parameters
- **4.2** Generic functions — `fn identity[T](x: T) -> T`
- **4.3** Generic record types — `type Stack[T] = { items: List[T], size: Int }`
- **4.4** Generic variant types — `type Maybe[T] = | Just(T) | Nothing`
- **4.5** Recursive generic variants — `type Tree[T] = | Leaf(T) | Node(Tree[T], Tree[T])` with auto-Boxing
- **4.6** Call-site type arguments — `identity[Int](42)` with turbofish codegen
- **4.7** Type inference — TypeVar compatibility, no full HKM
- **4.8** Auto-derived bounds — `Clone + Debug + PartialEq + PartialOrd` for Rust target

No separate generics spec is needed; `type-system.md` is the single source of truth.
