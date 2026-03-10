# Tuple & Record Specification

> Verified by: tuple/record patterns used across exercises (pipeline, config-merger, generics-test); type system spec covers generic record/tuple types.

---

## 1. Tuple Types

### 1.1 Type Syntax

```almide
(Int, String)              -- pair
(Int, String, Bool)        -- triple
(A, B)                     -- generic tuple
```

- Rust: `(i64, String)`, `(i64, String, bool)`
- TypeScript: `[number, string]`, `[number, string, boolean]`

### 1.2 Tuple Construction

```almide
let pair = (1, "hello")
let triple = (1, "two", true)
let nested = ((1, 2), (3, 4))
```

Tuples are constructed with parenthesized, comma-separated expressions. A single-element parenthesized expression is a grouping (not a tuple).

### 1.3 Tuple Index Access

```almide
let t = (1, "hello")
let x = t.0     -- 1
let s = t.1     -- "hello"
```

Index access uses `.N` syntax where N is a zero-based integer literal. The AST represents this as `Expr::TupleIndex { object, index }`.

**Type checking**: The checker validates that the index is within bounds of the tuple type and returns the element type at that position.

**Code generation**:
- Rust: `(expr).0`
- TypeScript: `(expr)[0]`

---

## 2. Tuple Destructuring

### 2.1 Let Destructuring

```almide
let (a, b) = (1, "hello")
-- a: Int = 1
-- b: String = "hello"
```

The `let` statement accepts a `Pattern::Tuple` on the left side. The AST represents this as `Stmt::LetDestructure { pattern, value }`.

### 2.2 Nested Tuple Destructuring

```almide
let ((a, b), (c, d)) = ((1, 2), (3, 4))
-- a = 1, b = 2, c = 3, d = 4
```

Tuple patterns nest recursively — each element of the outer tuple pattern can itself be a tuple pattern.

### 2.3 Match Patterns

```almide
fn describe(pair: (Int, String)) -> String =
  match pair {
    (0, s) => "zero: " ++ s
    (n, s) => int.to_string(n) ++ ": " ++ s
  }
```

Tuple patterns work in `match` arms. Each element is matched individually.

### 2.4 Lambda Tuple Parameters

```almide
let pairs = [(1, "a"), (2, "b")]
let names = list.map(pairs, fn((_, name)) => name)
```

Lambda parameters can destructure tuples directly. The parser recognizes `fn((a, b)) => ...` and stores the destructured names in `LambdaParam.tuple_names`.

**Code generation**:
- Rust: `|(a, b)| ...`
- TypeScript: `([a, b]) => ...`

---

## 3. Record Types

### 3.1 Type Declaration

```almide
type User = { id: Int, name: String }
type Pair[A, B] = { fst: A, snd: B }       -- generic records
```

Records are declared with `type Name = { field: Type, ... }`. They compile to `struct` in Rust and `interface` in TypeScript.

### 3.2 Anonymous Record Construction

```almide
let p = { x: 1, y: 2 }
```

Anonymous record literals auto-resolve to named struct types when field names match a declared type. The emitter maintains a `named_record_types` map for this resolution.

### 3.3 Named Record Construction

```almide
type Point = { x: Int, y: Int }
let p = Point { x: 1, y: 2 }
```

Named construction explicitly specifies the type. The AST stores this as `Expr::Record { name: Some("Point"), fields }`.

**Code generation**:
- Rust: `Point { x: 1i64, y: 2i64 }`
- TypeScript: `{ x: 1, y: 2 }` (name is ignored; plain JS object)

### 3.4 Record Field Access

```almide
let p = { x: 1, y: 2 }
let x_val = p.x        -- 1
let y_val = p.y        -- 2
```

Field access uses `.field` syntax, represented as `Expr::Member { object, field }`.

### 3.5 Spread Record (Functional Update)

```almide
let p1 = { x: 1, y: 2 }
let p2 = { ...p1, x: 10 }    -- { x: 10, y: 2 }
```

The spread operator `...` copies all fields from a base record and overrides specified fields. Represented as `Expr::SpreadRecord { base, fields }`.

---

## 4. Record Destructuring

### 4.1 Let Destructuring

```almide
let { name, value } = some_record
-- name and value bound to the corresponding fields
```

Record destructuring in `let` uses `Stmt::LetDestructure` with a `Pattern::RecordPattern`.

### 4.2 Match Patterns

```almide
type Shape =
  | Circle(Float)
  | Rect{ width: Float, height: Float }

fn area(s: Shape) -> Float =
  match s {
    Circle(r) => 3.14159 * r * r
    Rect{ width, height } => width * height
  }
```

Record patterns work in variant match arms with record-style fields.

---

## 5. AST Representation

| Construct | AST Node |
|-----------|----------|
| Tuple literal | `Expr::Tuple { elements }` |
| Tuple index | `Expr::TupleIndex { object, index }` |
| Record literal | `Expr::Record { name, fields }` |
| Spread record | `Expr::SpreadRecord { base, fields }` |
| Field access | `Expr::Member { object, field }` |
| Tuple pattern | `Pattern::Tuple { elements }` |
| Record pattern | `Pattern::RecordPattern { name, fields }` |
| Destructuring let | `Stmt::LetDestructure { pattern, value }` |
| Lambda tuple param | `LambdaParam { tuple_names: Some(names) }` |

---

## 6. Type Declarations

| Form | Syntax | Example |
|------|--------|---------|
| Record type | `type Name = { field: Type }` | `type User = { id: Int, name: String }` |
| Tuple type (inline) | `(Type, Type)` | `(Int, String)` |
| Generic record | `type Name[T] = { field: T }` | `type Stack[T] = { items: List[T], size: Int }` |

Note: There is no standalone `type Name = (Int, String)` tuple type declaration. Tuple types are used inline in function signatures and other type positions. For named tuple-like types, use a record or a newtype.
