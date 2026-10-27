//! A comparison relates two values of ONE type (security table row 11b, #4018).
//!
//! `==` / `!=` / `<` / `<=` / `>` / `>=` are structural on a single type:
//! there is no coercion that makes `1 == "1"`, `1 == 1.0` or `[1] == 1` mean
//! anything. The solver's `unify_infer` cannot be the judge here — it is
//! permissive by design (an inference variable binds and never fails, a rigid
//! type variable is `compatible` with anything, a numeric pair unifies in
//! either direction), so a mismatch it met stayed silent and reached rustc
//! (E0308 / E0369) or the wasm wall instead of the checker.
//!
//! So the operand pair is judged after the solve, on the RESOLVED types, by
//! [`operands_agree`]: two types agree when they are the same type, read off
//! the types alone — aliases resolve through the environment's declarations,
//! a rigid generic agrees only with itself (so the rule survives
//! instantiation: substituting one binding for `T` on both sides keeps them
//! equal, and a call that would bind `T` to two types is the call's own
//! mismatch), and two numeric widths agree only where the lowering retypes
//! a LITERAL to the other side's width (`x8 == 1`, `[x8] == [1, 2]`). That
//! last rule follows `coerce_literal_to_sized`, which is the one place a
//! width changes: it descends a block tail, an `if` / `match` arm, a list or
//! tuple element, a record field and an Option / Result payload, so
//! [`LitShape`] records exactly those positions of each operand.
//!
//! A pair with an unresolved slot (`Unknown`, an inference variable, `Never`)
//! is left to E025 and to the error that made it unknown.
use super::types::is_inference_var;
use super::{err, is_literal_numeric_ast, resolve_ty, Checker};
use crate::ast::{Expr, ExprKind, Span};
use crate::types::{Ty, TypeEnv};
use almide_base::intern::Sym;
use almide_lang::types::constructor::TypeConstructorId;

/// Where an operand's literals sit, in the positions the lowering's literal
/// retyping (`coerce_literal_to_sized`) descends into.
#[derive(Debug, Clone)]
pub(crate) enum LitShape {
    /// A numeric literal expression (`1`, `-2`, `(1 + 2)`).
    Num,
    /// A list literal's elements.
    Elems(Vec<LitShape>),
    /// A tuple literal's positions.
    Tuple(Vec<LitShape>),
    /// A record literal's fields, by name.
    Fields(Vec<(Sym, LitShape)>),
    Some(Box<LitShape>),
    Ok(Box<LitShape>),
    Err(Box<LitShape>),
    /// The arms of an `if` / `match`: each arm is a value of the whole.
    Arms(Vec<LitShape>),
    /// Anything else: a value whose width is its type's.
    Opaque,
}

impl LitShape {
    pub(crate) fn of(e: &Expr) -> LitShape {
        if is_literal_numeric_ast(e) {
            return LitShape::Num;
        }
        match &e.kind {
            ExprKind::Paren { expr } => LitShape::of(expr),
            ExprKind::Block { expr: Some(tail), .. } => LitShape::of(tail),
            ExprKind::List { elements } => LitShape::Elems(elements.iter().map(LitShape::of).collect()),
            ExprKind::Tuple { elements } => LitShape::Tuple(elements.iter().map(LitShape::of).collect()),
            ExprKind::Record { fields, .. } => {
                LitShape::Fields(fields.iter().map(|f| (f.name, LitShape::of(&f.value))).collect())
            }
            ExprKind::Some { expr } => LitShape::Some(Box::new(LitShape::of(expr))),
            ExprKind::Ok { expr } => LitShape::Ok(Box::new(LitShape::of(expr))),
            ExprKind::Err { expr } => LitShape::Err(Box::new(LitShape::of(expr))),
            ExprKind::If { then, else_, .. } => LitShape::Arms(vec![LitShape::of(then), LitShape::of(else_)]),
            ExprKind::Match { arms, .. } => LitShape::Arms(arms.iter().map(|a| LitShape::of(&a.body)).collect()),
            _ => LitShape::Opaque,
        }
    }
}

/// One comparison awaiting the post-solve operand-agreement check.
#[derive(Debug, Clone)]
pub(crate) struct EqPairSite {
    pub op: Sym,
    pub left: (Ty, LitShape),
    pub right: (Ty, LitShape),
    pub span: Option<Span>,
}

const OPAQUE: LitShape = LitShape::Opaque;

fn is_numeric(t: &Ty) -> bool {
    matches!(
        t,
        Ty::Int | Ty::Float | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 | Ty::Float32 | Ty::Float64
    )
}

/// Two numeric types: the same width, the 64-bit spellings of the canonical
/// types, or a canonical LITERAL the lowering retypes to the sized side.
fn numeric_agree(a: &Ty, b: &Ty, sa: &LitShape, sb: &LitShape) -> bool {
    if matches!((a, b), (Ty::Int, Ty::Int64) | (Ty::Int64, Ty::Int) | (Ty::Float, Ty::Float64) | (Ty::Float64, Ty::Float)) {
        return true;
    }
    // `compatible(expected, actual)` admits exactly the literal coercion
    // INTO the expected sized width; the actual side must then be a literal.
    (a.compatible(b) && matches!(sb, LitShape::Num)) || (b.compatible(a) && matches!(sa, LitShape::Num))
}

fn elems(s: &LitShape) -> Vec<&LitShape> {
    match s {
        LitShape::Elems(v) if !v.is_empty() => v.iter().collect(),
        _ => vec![&OPAQUE],
    }
}

fn position(s: &LitShape, i: usize) -> &LitShape {
    match s {
        LitShape::Tuple(v) => v.get(i).unwrap_or(&OPAQUE),
        _ => &OPAQUE,
    }
}

fn field<'a>(s: &'a LitShape, name: Sym) -> &'a LitShape {
    match s {
        LitShape::Fields(v) => v.iter().find(|(n, _)| *n == name).map_or(&OPAQUE, |(_, s)| s),
        _ => &OPAQUE,
    }
}

/// The payload shape of argument `i` of an Option / Result literal.
fn payload(s: &LitShape, id: &TypeConstructorId, i: usize) -> &LitShape {
    match (s, id, i) {
        (LitShape::Some(p), TypeConstructorId::Option, 0)
        | (LitShape::Ok(p), TypeConstructorId::Result, 0)
        | (LitShape::Err(p), TypeConstructorId::Result, 1) => p,
        _ => &OPAQUE,
    }
}

/// Do two resolved operand types name ONE type? See the module doc.
pub(crate) fn operands_agree(env: &TypeEnv, a: &Ty, b: &Ty, sa: &LitShape, sb: &LitShape, depth: u32) -> bool {
    if depth > 64 || a == b {
        return true;
    }
    if let LitShape::Arms(arms) = sa {
        return arms.iter().all(|s| operands_agree(env, a, b, s, sb, depth + 1));
    }
    if let LitShape::Arms(arms) = sb {
        return arms.iter().all(|s| operands_agree(env, a, b, sa, s, depth + 1));
    }
    if is_numeric(a) && is_numeric(b) {
        return numeric_agree(a, b, sa, sb);
    }
    let either_compatible = || a.compatible(b) || b.compatible(a);
    match (a, b) {
        // A rigid generic is one unknown type: it agrees only with itself.
        (Ty::TypeVar(_), _) | (_, Ty::TypeVar(_)) => false,
        (Ty::Named(na, aa), Ty::Named(nb, ab)) if na == nb => {
            aa.len() != ab.len()
                || aa.iter().zip(ab).all(|(x, y)| operands_agree(env, x, y, &OPAQUE, &OPAQUE, depth + 1))
        }
        (Ty::Named(na, _), Ty::Named(nb, _))
            if almide_lang::stdlib_info::stdlib_type_vs_user_shadow(na.as_str(), nb.as_str()) => false,
        (Ty::Named(..), _) | (_, Ty::Named(..)) => {
            let (ra, rb) = (env.resolve_named(a), env.resolve_named(b));
            if ra != *a || rb != *b {
                operands_agree(env, &ra, &rb, sa, sb, depth + 1)
            } else {
                either_compatible()
            }
        }
        (Ty::Applied(i1, a1), Ty::Applied(i2, a2)) if i1 == i2 && a1.len() == a2.len() => {
            a1.iter().zip(a2).enumerate().all(|(i, (x, y))| {
                if *i1 == TypeConstructorId::List {
                    elems(sa).iter().all(|l| elems(sb).iter().all(|r| operands_agree(env, x, y, l, r, depth + 1)))
                } else {
                    operands_agree(env, x, y, payload(sa, i1, i), payload(sb, i1, i), depth + 1)
                }
            })
        }
        (Ty::Tuple(x), Ty::Tuple(y)) => {
            x.len() == y.len()
                && x.iter().zip(y).enumerate().all(|(i, (p, q))| {
                    operands_agree(env, p, q, position(sa, i), position(sb, i), depth + 1)
                })
        }
        (Ty::Record { fields: fa }, Ty::Record { fields: fb }) => {
            fa.len() == fb.len()
                && fa.iter().all(|(n, t)| {
                    fb.iter().any(|(n2, t2)| n == n2 && operands_agree(env, t, t2, field(sa, *n), field(sb, *n), depth + 1))
                })
        }
        // Function types have no equality at all: E016 reports them.
        (Ty::Fn { .. }, Ty::Fn { .. }) => true,
        // Shapes the type system relates by declaration rather than by
        // structure: the Matrix dtype bridge, union membership, open rows,
        // a variant against its own name, const parameters.
        (Ty::Matrix, Ty::Applied(TypeConstructorId::Matrix, _))
        | (Ty::Applied(TypeConstructorId::Matrix, _), Ty::Matrix)
        | (Ty::Union(_), _) | (_, Ty::Union(_))
        | (Ty::OpenRecord { .. }, _) | (_, Ty::OpenRecord { .. })
        | (Ty::Variant { .. }, _) | (_, Ty::Variant { .. })
        | (Ty::ConstParam { .. } | Ty::ConstValue { .. }, _)
        | (_, Ty::ConstParam { .. } | Ty::ConstValue { .. }) => either_compatible(),
        _ => false,
    }
}

fn unresolved(t: &Ty) -> bool {
    let hit = |t: &Ty| matches!(t, Ty::Unknown | Ty::Never) || is_inference_var(t).is_some();
    hit(t) || t.any_child_recursive(&hit)
}

impl Checker {
    /// Post-solve (#4018, row 11b): every comparison's two operands name
    /// one type. E037, at the operator.
    pub(super) fn validate_eq_operand_pairs(&mut self) {
        let sites = std::mem::take(&mut self.deferred_eq_pair_checks);
        for site in sites {
            let lc = resolve_ty(&site.left.0, &self.uf);
            let rc = resolve_ty(&site.right.0, &self.uf);
            if unresolved(&lc) || unresolved(&rc) {
                continue;
            }
            if operands_agree(&self.env, &lc, &rc, &site.left.1, &site.right.1, 0) {
                continue;
            }
            let mut diag = err(
                format!("operator '{}' compares {} with {}", site.op, lc.display(), rc.display()),
                "A comparison relates two values of the same type, and nothing converts one side \
                 implicitly — convert one side explicitly (`int.to_string(n)`, `int.parse(s)`, \
                 `float.from(n)`, `<type>.from(x)` for a sized width), or compare the parts \
                 you mean",
                format!("operator {}", site.op),
            )
            .with_code("E037");
            if let Some(s) = site.span {
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            self.diagnostics.push(diag);
        }
    }
}
