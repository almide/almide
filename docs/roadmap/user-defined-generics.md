# User-Defined Generics

Currently `List[T]`, `Option[T]`, `Result[T, E]` etc. are compiler built-ins, but users cannot define their own generic types or functions.

### Proposed Syntax

```almide
// generic type
type Stack[T] =
  | Empty
  | Push(T, Stack[T])

// generic function
fn map[A, B](xs: List[A], f: fn(A) -> B) -> List[B] =
  match xs {
    [] => []
    [head, ...tail] => [f(head)] ++ map(tail, f)
  }

fn identity[T](x: T) -> T = x
```

### Implementation Steps

- [ ] Parser: parse generic parameters in `fn name[T, U](...) -> ...` (partial support exists in `try_parse_generic_params`)
- [ ] Type checker: introduce type variables, type inference (unification-based)
- [ ] Rust emitter: convert to `fn name<T, U>(...) -> ...`
- [ ] TS emitter: convert to `function name<T, U>(...): ...` (type erasure in JS mode)

### Design Decisions

- Type parameters use `[T]` notation (consistent with existing `List[T]` in Almide)
- Type inference is the primary approach; explicit type arguments at call sites are not required
- Type constraints will be introduced after trait implementation, e.g. `fn sort[T: Ord](xs: List[T])`

---
