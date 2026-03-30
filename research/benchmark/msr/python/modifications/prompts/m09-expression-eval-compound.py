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

# Expr is one of: Lit, Add, Mul, Sub


def eval_expr(e: Expr) -> int:
    if isinstance(e, Lit): return e.value
    elif isinstance(e, Add): return eval_expr(e.left) + eval_expr(e.right)
    elif isinstance(e, Mul): return eval_expr(e.left) * eval_expr(e.right)
    elif isinstance(e, Sub): return eval_expr(e.left) - eval_expr(e.right)


def to_string(e: Expr) -> str:
    if isinstance(e, Lit): return str(e.value)
    elif isinstance(e, Add): return f"({to_string(e.left)} + {to_string(e.right)})"
    elif isinstance(e, Mul): return f"({to_string(e.left)} * {to_string(e.right)})"
    elif isinstance(e, Sub): return f"({to_string(e.left)} - {to_string(e.right)})"


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

# ========== MODIFICATION INSTRUCTION ==========
# Make TWO changes to the Expr types simultaneously:
#
# 1. Add a `Neg` dataclass for unary negation.
#    `eval_expr(Neg(e))` returns `-eval_expr(e)` (or `0 - eval_expr(e)`).
#    `to_string(Neg(e))` returns `f"(-{to_string(e)})"`.
#
# 2. Add a `Pow` dataclass for exponentiation.
#    `eval_expr(Pow(base, exp))` computes base raised to the power of exp.
#    Assume exp is always >= 0.
#    `to_string(Pow(a, b))` returns `f"({to_string(a)} ^ {to_string(b)})"`.
#
# Update the `Expr` type alias, `eval_expr`, and `to_string` to handle both.
# All existing tests must still pass unchanged.

# ========== V2 TESTS (must also pass after modification) ==========

assert eval_expr(Neg(Lit(5))) == -5, "neg literal"
assert eval_expr(Neg(Neg(Lit(3)))) == 3, "neg of neg"
assert to_string(Neg(Lit(5))) == "(-5)", "to_string neg"
assert eval_expr(Pow(Lit(2), Lit(3))) == 8, "pow basic"
assert eval_expr(Pow(Lit(5), Lit(0))) == 1, "pow zero"
assert eval_expr(Pow(Lit(7), Lit(1))) == 7, "pow one"
assert to_string(Pow(Lit(2), Lit(3))) == "(2 ^ 3)", "to_string pow"
assert eval_expr(Neg(Pow(Lit(2), Lit(3)))) == -8, "neg with pow"
assert eval_expr(Add(Pow(Lit(2), Lit(3)), Lit(2))) == 10, "pow in expression"
