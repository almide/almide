from __future__ import annotations
# ========== V1 SOLUTION (working code — all tests pass) ==========

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


def eval_expr(e: Expr) -> tuple:
    if isinstance(e, Lit):
        return (True, e.value)
    elif isinstance(e, Add):
        ok_l, val_l = eval_expr(e.left)
        if not ok_l: return (False, val_l)
        ok_r, val_r = eval_expr(e.right)
        if not ok_r: return (False, val_r)
        return (True, val_l + val_r)
    elif isinstance(e, Mul):
        ok_l, val_l = eval_expr(e.left)
        if not ok_l: return (False, val_l)
        ok_r, val_r = eval_expr(e.right)
        if not ok_r: return (False, val_r)
        return (True, val_l * val_r)
    elif isinstance(e, Sub):
        ok_l, val_l = eval_expr(e.left)
        if not ok_l: return (False, val_l)
        ok_r, val_r = eval_expr(e.right)
        if not ok_r: return (False, val_r)
        return (True, val_l - val_r)
    elif isinstance(e, Div):
        ok_l, val_l = eval_expr(e.left)
        if not ok_l: return (False, val_l)
        ok_r, val_r = eval_expr(e.right)
        if not ok_r: return (False, val_r)
        if val_r == 0: return (False, "division by zero")
        return (True, val_l // val_r)


def to_string(e: Expr) -> str:
    if isinstance(e, Lit): return str(e.value)
    elif isinstance(e, Add): return f"({to_string(e.left)} + {to_string(e.right)})"
    elif isinstance(e, Mul): return f"({to_string(e.left)} * {to_string(e.right)})"
    elif isinstance(e, Sub): return f"({to_string(e.left)} - {to_string(e.right)})"
    elif isinstance(e, Div): return f"({to_string(e.left)} / {to_string(e.right)})"


# Tests
assert eval_expr(Lit(5)) == (True, 5), "literal"
assert eval_expr(Add(Lit(2), Lit(3))) == (True, 5), "add"
assert eval_expr(Mul(Lit(4), Lit(5))) == (True, 20), "mul"
assert eval_expr(Sub(Lit(10), Lit(3))) == (True, 7), "sub"
assert eval_expr(Add(Lit(1), Mul(Lit(2), Lit(3)))) == (True, 7), "nested add mul"
assert eval_expr(Mul(Add(Lit(1), Lit(2)), Sub(Lit(10), Lit(4)))) == (True, 18), "deep nesting"
assert to_string(Lit(42)) == "42", "to_string literal"
assert to_string(Add(Lit(1), Lit(2))) == "(1 + 2)", "to_string add"
assert to_string(Mul(Add(Lit(1), Lit(2)), Lit(3))) == "((1 + 2) * 3)", "to_string nested"
assert eval_expr(Add(Mul(Lit(2), Lit(3)), Sub(Lit(10), Lit(1)))) == (True, 15), "eval matches"
assert eval_expr(Sub(Mul(Add(Lit(1), Lit(2)), Lit(5)), Add(Lit(3), Lit(4)))) == (True, 8), "deeply nested"
assert eval_expr(Sub(Lit(0), Lit(5))) == (True, -5), "single negative via sub"

# ========== V2 TESTS (must also pass after modification) ==========

assert eval_expr(Div(Lit(10), Lit(3))) == (True, 3), "eval div"
assert eval_expr(Div(Lit(5), Lit(0))) == (False, "division by zero"), "eval div by zero"
assert to_string(Div(Lit(10), Lit(3))) == "(10 / 3)", "to_string div"
assert eval_expr(Add(Lit(1), Div(Lit(10), Lit(2)))) == (True, 6), "nested div"
assert eval_expr(Add(Lit(1), Div(Lit(5), Lit(0)))) == (False, "division by zero"), "div by zero propagates"
