import pytest
from solution import eval_expr, Lit, BinOp, Neg, Op


def test_literal():
    assert eval_expr(Lit(42.0)) == 42.0


def test_addition():
    assert eval_expr(BinOp(Op.ADD, Lit(2.0), Lit(3.0))) == 5.0


def test_subtraction():
    assert eval_expr(BinOp(Op.SUB, Lit(10.0), Lit(4.0))) == 6.0


def test_multiplication():
    assert eval_expr(BinOp(Op.MUL, Lit(3.0), Lit(7.0))) == 21.0


def test_division():
    assert eval_expr(BinOp(Op.DIV, Lit(10.0), Lit(4.0))) == 2.5


def test_division_by_zero():
    with pytest.raises(ValueError, match="division by zero"):
        eval_expr(BinOp(Op.DIV, Lit(1.0), Lit(0.0)))


def test_negation():
    assert eval_expr(Neg(Lit(5.0))) == -5.0


def test_nested():
    e = BinOp(Op.ADD, Lit(1.0), BinOp(Op.MUL, Lit(2.0), Lit(3.0)))
    assert eval_expr(e) == 7.0


def test_complex():
    e = BinOp(Op.SUB, BinOp(Op.ADD, Lit(10.0), Lit(5.0)), Neg(Lit(3.0)))
    assert eval_expr(e) == 18.0


def test_deeply_nested():
    e = Neg(BinOp(Op.MUL, Neg(Lit(2.0)), BinOp(Op.ADD, Lit(3.0), Lit(4.0))))
    assert eval_expr(e) == 14.0
