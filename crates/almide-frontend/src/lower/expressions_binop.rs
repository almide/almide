// Continuation of expressions.rs — the operand-typed `BinOp` table an
// operator lowers to. Split out of expressions_access.rs to keep it under the
// 800-line codopsy max-lines threshold; same file scope via `include!`
// (inherits expressions.rs's `use` imports).

/// The `BinOp` an operator and its operand types lower to.
///
/// Falls back to `AddInt` for a pairing no arm claims. That is not a guess: an
/// unclaimed pairing means the checker already rejected the expression (or
/// resolved an operand to `Unknown` after an earlier error), so the lowered
/// operator is never executed — it only has to exist for lowering to finish and
/// let the real diagnostic be the one the user sees.
pub(super) fn binop_for(op: &str, left_ty: &Ty, right_ty: &Ty) -> BinOp {
    binop_arith(op, left_ty, right_ty)
        .or_else(|| binop_other(op, left_ty, right_ty))
        .unwrap_or(BinOp::AddInt)
}

/// Addition, subtraction and multiplication.
///
/// `+` is overloaded: on strings and lists it concatenates, so the operand types
/// decide the operation and not just its width.
///
/// One group of the operand-typed operator table, arms verbatim and in source
/// order. `None` means "not my group"; `binop_for` tries the groups in that
/// order, so the operator an expression lowers to is unchanged.
fn binop_arith(op: &str, left_ty: &Ty, right_ty: &Ty) -> Option<BinOp> {
    Some(match (op, left_ty, right_ty) {
        ("+", Ty::String, _) | ("+", _, Ty::String) => BinOp::ConcatStr,
        ("+", Ty::Applied(TypeConstructorId::List, _), _) | ("+", _, Ty::Applied(TypeConstructorId::List, _)) => BinOp::ConcatList,
        // Matrix operators
        ("+", Ty::Matrix, Ty::Matrix) => BinOp::AddMatrix,
        ("-", Ty::Matrix, Ty::Matrix) => BinOp::SubMatrix,
        ("*", Ty::Matrix, Ty::Matrix) => BinOp::MulMatrix,
        ("*", Ty::Matrix, Ty::Float) | ("*", Ty::Float, Ty::Matrix) => BinOp::ScaleMatrix,
        ("*", Ty::Matrix, Ty::Int) | ("*", Ty::Int, Ty::Matrix) => BinOp::ScaleMatrix,
        // Float dispatch covers canonical `Float` plus the sized
        // `Float32` and `Float64` (#4055: `Float64` fell to the Int
        // arm). Any other numeric type (Int / Int8 ... / UInt64)
        // takes the Int path. The *width* of the
        // arithmetic op (i32_add vs i64_add vs f32_add vs
        // f64_add) is resolved at WASM emit time from the
        // operand's valtype; Rust codegen emits plain `a + b`
        // and lets rustc pick.
        ("+", Ty::Float, _) | ("+", _, Ty::Float)
        | ("+", Ty::Float32, _) | ("+", _, Ty::Float32)
        | ("+", Ty::Float64, _) | ("+", _, Ty::Float64) => BinOp::AddFloat,
        ("+", _, _) => BinOp::AddInt,
        ("-", Ty::Float, _) | ("-", _, Ty::Float)
        | ("-", Ty::Float32, _) | ("-", _, Ty::Float32)
        | ("-", Ty::Float64, _) | ("-", _, Ty::Float64) => BinOp::SubFloat,
        ("-", _, _) => BinOp::SubInt,
        ("*", Ty::Float, _) | ("*", _, Ty::Float)
        | ("*", Ty::Float32, _) | ("*", _, Ty::Float32)
        | ("*", Ty::Float64, _) | ("*", _, Ty::Float64) => BinOp::MulFloat,
        _ => return None,
    })
}

/// Division, remainder, exponentiation, comparison and the boolean connectives.
///
/// One group of the operand-typed operator table, arms verbatim and in source
/// order. `None` means "not my group"; `binop_for` tries the groups in that
/// order, so the operator an expression lowers to is unchanged.
fn binop_other(op: &str, left_ty: &Ty, right_ty: &Ty) -> Option<BinOp> {
    Some(match (op, left_ty, right_ty) {
        ("*", _, _) => BinOp::MulInt,
        ("/", Ty::Float, _) | ("/", _, Ty::Float)
        | ("/", Ty::Float32, _) | ("/", _, Ty::Float32)
        | ("/", Ty::Float64, _) | ("/", _, Ty::Float64) => BinOp::DivFloat,
        ("/", _, _) => BinOp::DivInt,
        ("%", Ty::Float, _) | ("%", _, Ty::Float)
        | ("%", Ty::Float32, _) | ("%", _, Ty::Float32)
        | ("%", Ty::Float64, _) | ("%", _, Ty::Float64) => BinOp::ModFloat,
        ("%", _, _) => BinOp::ModInt,
        ("^", Ty::Float, _) | ("^", _, Ty::Float)
        | ("^", Ty::Float32, _) | ("^", _, Ty::Float32)
        | ("^", Ty::Float64, _) | ("^", _, Ty::Float64) => BinOp::PowFloat,
        ("^", _, _) => BinOp::PowInt,
        ("==", _, _) => BinOp::Eq, ("!=", _, _) => BinOp::Neq,
        ("<", _, _) => BinOp::Lt, (">", _, _) => BinOp::Gt,
        ("<=", _, _) => BinOp::Lte, (">=", _, _) => BinOp::Gte,
        ("and", _, _) => BinOp::And, ("or", _, _) => BinOp::Or,
        _ => return None,
    })
}
