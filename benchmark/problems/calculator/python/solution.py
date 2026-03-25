from __future__ import annotations
from dataclasses import dataclass
from enum import Enum, auto


class Op(Enum):
    ADD = auto()
    SUB = auto()
    MUL = auto()
    DIV = auto()


@dataclass
class Lit:
    value: float


@dataclass
class BinOp:
    op: Op
    left: "Expr"
    right: "Expr"


@dataclass
class Neg:
    inner: "Expr"


Expr = Lit | BinOp | Neg


def eval_expr(e: Expr) -> float:
    """Evaluate the expression. Raises ValueError on division by zero."""
    match e:
        case Lit(value=n):
            return n
        case Neg(inner=inner):
            return -eval_expr(inner)
        case BinOp(op=op, left=left, right=right):
            l = eval_expr(left)
            r = eval_expr(right)
            match op:
                case Op.ADD:
                    return l + r
                case Op.SUB:
                    return l - r
                case Op.MUL:
                    return l * r
                case Op.DIV:
                    if r == 0.0:
                        raise ValueError("division by zero")
                    return l / r
    raise TypeError(f"unknown expression: {e}")
