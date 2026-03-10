# Type System Extensions [PLANNED]

## User-Defined Generics
Currently only built-in types like List[T] and Option[T] are generic.

```almide
// proposed
type Stack[T] = { items: List[T] }

fn push[T](stack: Stack[T], item: T) -> Stack[T] = {
  { items: stack.items ++ [item] }
}
```

## Full Trait Implementation
Keywords exist in lexer/parser, but type checking and code generation are incomplete.

```almide
trait Show {
  fn show(self) -> String
}

impl Show for Point {
  fn show(self) -> String = "${self.x}, ${self.y}"
}
```

## Tuple Types
Records require names, which can be verbose.

```almide
// proposed
let pair: (Int, String) = (42, "hello")
let (a, b) = pair
```

## Structured Error Types
Currently Result[T, String] uses a fixed String error type, making it hard to distinguish error kinds.

```almide
// proposed
type AppError = NotFound(String) | Unauthorized | Internal(String)
type AppResult[T] = Result[T, AppError]
```
Enables branching by error type in match arms.

## Type Aliases
```almide
type UserId = Int
type Config = Map[String, String]
```
Newtype exists currently but is limited in scope.

## Priority
User-defined generics > structured error types > trait implementation > tuples > type aliases
