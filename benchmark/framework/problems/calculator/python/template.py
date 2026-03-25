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
    # TODO: implement
    raise NotImplementedError
