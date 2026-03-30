from __future__ import annotations
from dataclasses import dataclass


@dataclass
class Lit:
    value: int

@dataclass
class Add:
    left: 'Expr'
    right: 'Expr'

@dataclass
class Mul:
    left: 'Expr'
    right: 'Expr'

@dataclass
class Sub:
    left: 'Expr'
    right: 'Expr'

@dataclass
class Div:
    left: 'Expr'
    right: 'Expr'

# Expr is one of: Lit, Add, Mul, Sub, Div

Expr = Lit | Add | Mul | Sub | Div


def eval_expr(e: Expr) -> int:
    if isinstance(e, Lit): return e.value
    elif isinstance(e, Add): return eval_expr(e.left) + eval_expr(e.right)
    elif isinstance(e, Mul): return eval_expr(e.left) * eval_expr(e.right)
    elif isinstance(e, Sub): return eval_expr(e.left) - eval_expr(e.right)
    elif isinstance(e, Div): return eval_expr(e.left) // eval_expr(e.right)


def to_string(e: Expr) -> str:
    if isinstance(e, Lit): return str(e.value)
    elif isinstance(e, Add): return f"({to_string(e.left)} + {to_string(e.right)})"
    elif isinstance(e, Mul): return f"({to_string(e.left)} * {to_string(e.right)})"
    elif isinstance(e, Sub): return f"({to_string(e.left)} - {to_string(e.right)})"
    elif isinstance(e, Div): return f"({to_string(e.left)} / {to_string(e.right)})"


# Tests
assert eval_expr(Lit(5)) == 5, "literal"
assert eval_expr(Add(Lit(2), Lit(3))) == 5, "add"
assert eval_expr(Mul(Lit(4), Lit(5))) == 20, "mul"
assert eval_expr(Sub(Lit(10), Lit(3))) == 7, "sub"
assert eval_expr(Add(Lit(1), Mul(Lit(2), Lit(3)))) == 7, "nested add mul"
assert eval_expr(Mul(Add(Lit(1), Lit(2)), Sub(Lit(10), Lit(4)))) == 18, "deep nesting"
assert to_string(Lit(42)) == "42", "to_string literal"
assert to_string(Add(Lit(1), Lit(2))) == "(1 + 2)", "to_string add"
assert to_string(Mul(Add(Lit(1), Lit(2)), Lit(3))) == "((1 + 2) * 3)", "to_string nested"
assert eval_expr(Add(Mul(Lit(2), Lit(3)), Sub(Lit(10), Lit(1)))) == 15, "eval matches"
assert eval_expr(Sub(Mul(Add(Lit(1), Lit(2)), Lit(5)), Add(Lit(3), Lit(4)))) == 8, "deeply nested"
assert eval_expr(Sub(Lit(0), Lit(5))) == -5, "single negative via sub"

# V2 Tests
assert eval_expr(Div(Lit(10), Lit(3))) == 3, "eval div"
assert eval_expr(Div(Lit(10), Lit(2))) == 5, "eval div exact"
assert to_string(Div(Lit(10), Lit(3))) == "(10 / 3)", "to_string div"
assert eval_expr(Add(Lit(1), Div(Lit(10), Lit(3)))) == 4, "nested with div"
assert eval_expr(Mul(Div(Lit(20), Lit(4)), Sub(Lit(10), Lit(7)))) == 15, "div in complex expr"
