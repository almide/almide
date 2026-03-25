# Calculator

**Level**: 2 (Medium)

## Description

Implement an expression evaluator for a simple calculator using algebraic data types.

The calculator supports:
- Literal floating-point values
- Binary operations: addition, subtraction, multiplication, division
- Unary negation
- Nested expressions

Division by zero should return an error.

## Types

```
Op = Add | Sub | Mul | Div

Expr =
  | Lit(Float)
  | BinOp { op: Op, left: Expr, right: Expr }
  | Neg(Expr)
```

## Function Signature

```
eval(expr: Expr) -> Result[Float, String]
```

- On success: `ok(value)`
- On division by zero: `err("division by zero")`

## Test Cases

| Expression | Expected |
|-----------|----------|
| `Lit(42.0)` | `ok(42.0)` |
| `2.0 + 3.0` | `ok(5.0)` |
| `10.0 - 4.0` | `ok(6.0)` |
| `3.0 * 7.0` | `ok(21.0)` |
| `10.0 / 4.0` | `ok(2.5)` |
| `1.0 / 0.0` | `err("division by zero")` |
| `Neg(5.0)` | `ok(-5.0)` |
| `1.0 + (2.0 * 3.0)` | `ok(7.0)` |
| `(10.0 + 5.0) - Neg(3.0)` | `ok(18.0)` |
| `Neg(Neg(2.0) * (3.0 + 4.0))` | `ok(14.0)` |
