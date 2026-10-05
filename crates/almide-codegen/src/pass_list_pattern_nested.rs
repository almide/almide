//! ListPatternLowering, the nested half (#3413): a list pattern BELOW a
//! constructor / `some` / `ok` / `err` / record / tuple position —
//! `Op(_, [_, b])`, `Wrap([A(x), B(y)])`, `some([x, ..])`.
//!
//! The length-test chain in `pass_list_pattern` only desugars a list pattern
//! at the top of an arm (or a tuple element of one). A nested one used to
//! survive the pass and trip its `NoPatternKind("List")` postcondition.
//!
//! Each such arm is rewritten IN PLACE, inside the same match, so no other arm
//! is copied:
//!
//! ```text
//!   P[L1..Ln] if g => body
//! ```
//!
//! becomes, with each maximal list sub-pattern `Li` replaced by a fresh binder
//! `vi` typed by its position,
//!
//! ```text
//!   P[v1..vn] if (match (v1..vn) { (L1..Ln) if g => true, _ => false })
//!            => match (v1..vn) { (L1..Ln) => body, _ => <unreachable> }
//! ```
//!
//! The two inner matches hold only top-level list patterns, so the length-test
//! chain lowers them. The user guard runs inside the shape test, after the
//! list's binders are bound — never before them. A guard's copy of the list
//! pattern keeps only the binders the guard reads, so no element is cloned
//! for nothing.

use std::collections::{HashMap, HashSet};
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId, VariantPayload};
use almide_base::intern::Sym;

/// One case of a declared variant: its enum, the enum's generic parameter
/// names, and its payload.
#[derive(Clone)]
struct CaseDecl {
    enum_name: String,
    generics: Vec<Sym>,
    payload: CasePayload,
}

#[derive(Clone)]
enum CasePayload {
    Tuple(Vec<Ty>),
    Record(Vec<(String, Ty)>),
    Unit,
}

/// Every declared variant case by its bare name, and every enum's case list
/// by its bare name: what a nested pattern position needs to learn its type
/// and what the exhaustiveness backstop needs to count cases.
#[derive(Default)]
pub(crate) struct CaseTable {
    by_ctor: HashMap<String, Vec<CaseDecl>>,
    enum_cases: HashMap<String, Vec<String>>,
}

fn bare(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

impl CaseTable {
    pub(crate) fn from_program(program: &IrProgram) -> Self {
        let mut t = CaseTable::default();
        let decls = program.type_decls.iter().chain(program.modules.iter().flat_map(|m| m.type_decls.iter()));
        for td in decls {
            let IrTypeDeclKind::Variant { cases, .. } = &td.kind else { continue };
            let enum_name = bare(td.name.as_str()).to_string();
            let generics: Vec<Sym> = td.generics.iter().flatten().map(|g| g.name).collect();
            t.enum_cases.insert(enum_name.clone(), cases.iter().map(|c| c.name.as_str().to_string()).collect());
            for c in cases {
                let payload = match &c.kind {
                    IrVariantKind::Unit => CasePayload::Unit,
                    IrVariantKind::Tuple { fields } => CasePayload::Tuple(fields.clone()),
                    IrVariantKind::Record { fields } => CasePayload::Record(
                        fields.iter().map(|f| (f.name.as_str().to_string(), f.ty.clone())).collect()),
                };
                t.by_ctor.entry(c.name.as_str().to_string()).or_default()
                    .push(CaseDecl { enum_name: enum_name.clone(), generics: generics.clone(), payload });
            }
        }
        t
    }

    /// The case `ctor` of a value typed `ty`, with the enum's type arguments.
    fn case(&self, ty: &Ty, ctor: &str) -> Option<(&CaseDecl, Vec<Ty>)> {
        let found = self.by_ctor.get(bare(ctor))?;
        match ty {
            Ty::Named(n, args) => found.iter()
                .find(|c| c.enum_name == bare(n.as_str()))
                .or_else(|| (found.len() == 1).then(|| &found[0]))
                .map(|c| (c, args.clone())),
            _ => (found.len() == 1).then(|| (&found[0], Vec::new())),
        }
    }

    /// The declared type of payload position `field` (a tuple index or a
    /// record field name) of case `ctor`, under a value typed `ty`.
    fn field_ty(&self, ty: &Ty, ctor: &str, field: &str) -> Option<Ty> {
        if let Ty::Variant { cases, .. } = ty {
            let case = cases.iter().find(|c| c.name.as_str() == bare(ctor))?;
            return match &case.payload {
                VariantPayload::Tuple(fs) => field.parse::<usize>().ok().and_then(|i| fs.get(i).cloned()),
                VariantPayload::Record(fs) => fs.iter().find(|(n, _)| n.as_str() == field).map(|(_, t)| t.clone()),
                VariantPayload::Unit => None,
            };
        }
        let (case, args) = self.case(ty, ctor)?;
        let declared = match &case.payload {
            CasePayload::Tuple(fs) => field.parse::<usize>().ok().and_then(|i| fs.get(i).cloned()),
            CasePayload::Record(fs) => fs.iter().find(|(n, _)| n == field).map(|(_, t)| t.clone()),
            CasePayload::Unit => None,
        }?;
        let bindings: HashMap<Sym, Ty> = case.generics.iter().copied().zip(args).collect();
        Some(almide_lang::types::substitute(&declared, &bindings))
    }

    /// Is `ctor` its enum's only case, so a pattern on it cannot fail?
    fn is_sole_case(&self, ctor: &str) -> bool {
        self.by_ctor.get(bare(ctor)).is_some_and(|found| found.len() == 1
            && self.enum_cases.get(&found[0].enum_name).is_some_and(|cs| cs.len() == 1))
    }
}

/// The pass's working state: the var table it allocates the fresh list
/// binders in, and the declared cases it types them by.
pub(crate) struct Cx<'a> {
    pub(crate) vars: &'a mut VarTable,
    pub(crate) cases: &'a CaseTable,
}

/// Is `p` a list pattern the length-test chain lowers itself: a list (or an
/// as-pattern over one) at the top of the arm, or a tuple with a list
/// directly among its elements? Anything else holding a list — a list below
/// a constructor, `some`, a record field, or only below a tuple's non-list
/// elements — is rewritten in place. (The chain hands a tuple's non-list
/// elements to a residual match on the same elements, so a tuple with no
/// list of its own would come back to it unchanged, forever.)
pub(crate) fn is_list_at_top(p: &IrPattern) -> bool {
    match p {
        IrPattern::List { .. } => true,
        IrPattern::As { inner, .. } => matches!(inner.as_ref(), IrPattern::List { .. }),
        IrPattern::Tuple { elements } => elements.iter().any(|e| matches!(e, IrPattern::List { .. })),
        _ => false,
    }
}

/// The type of the `i`-th sub-position of an `Option` / `Result` / tuple.
fn applied_arg(ty: &Ty, i: usize) -> Ty {
    match ty {
        Ty::Applied(_, args) => args.get(i).cloned().unwrap_or(Ty::Unknown),
        Ty::Tuple(elems) => elems.get(i).cloned().unwrap_or(Ty::Unknown),
        _ => Ty::Unknown,
    }
}

/// A list pattern's type read off its own binders, for a position whose
/// declared type is not known: an element binder `x: T` makes it `List[T]`, a
/// named tail is typed as the list itself.
fn list_ty_from_binders(elements: &[IrPattern], rest: Option<&IrPattern>) -> Ty {
    if let Some(IrPattern::Bind { ty, .. }) = rest {
        return ty.clone();
    }
    elements.iter().find_map(|e| match e {
        IrPattern::Bind { ty, .. } | IrPattern::As { ty, .. } => Some(ty.clone()),
        _ => None,
    }).map_or(Ty::Unknown, |t| Ty::Applied(TypeConstructorId::List, vec![t]))
}

/// Lists pulled out of one pattern: the fresh binders' reads and the list
/// patterns they must match.
#[derive(Default)]
struct Pulled {
    reads: Vec<IrExpr>,
    lists: Vec<IrPattern>,
}

/// `pat` (matched against a value typed `ty`) with every maximal list
/// sub-pattern replaced by a fresh binder, queued in `out`.
fn pull_lists(pat: &IrPattern, ty: &Ty, cx: &mut Cx, out: &mut Pulled) -> IrPattern {
    if !super::pass_list_pattern::pattern_contains_list(pat) {
        return pat.clone();
    }
    match pat {
        IrPattern::List { elements, rest } => {
            let list_ty = match ty {
                Ty::Applied(TypeConstructorId::List, _) => ty.clone(),
                _ => list_ty_from_binders(elements, rest.as_deref()),
            };
            let var = cx.vars.alloc_fresh("__lp", list_ty.clone(), Mutability::Let, None);
            out.reads.push(IrExpr { kind: IrExprKind::Var { id: var }, ty: list_ty.clone(), span: None, def_id: None });
            out.lists.push(pat.clone());
            IrPattern::Bind { var, ty: list_ty }
        }
        IrPattern::As { var, ty: as_ty, inner } => IrPattern::As {
            var: *var, ty: as_ty.clone(), inner: Box::new(pull_lists(inner, ty, cx, out)),
        },
        IrPattern::Some { inner } => IrPattern::Some { inner: Box::new(pull_lists(inner, &applied_arg(ty, 0), cx, out)) },
        IrPattern::Ok { inner } => IrPattern::Ok { inner: Box::new(pull_lists(inner, &applied_arg(ty, 0), cx, out)) },
        IrPattern::Err { inner } => IrPattern::Err { inner: Box::new(pull_lists(inner, &applied_arg(ty, 1), cx, out)) },
        IrPattern::Tuple { elements } => IrPattern::Tuple {
            elements: elements.iter().enumerate().map(|(i, e)| pull_lists(e, &applied_arg(ty, i), cx, out)).collect(),
        },
        IrPattern::Constructor { name, args } => IrPattern::Constructor {
            name: name.clone(),
            args: args.iter().enumerate().map(|(i, a)| {
                let fty = cx.cases.field_ty(ty, name, &i.to_string()).unwrap_or(Ty::Unknown);
                pull_lists(a, &fty, cx, out)
            }).collect(),
        },
        IrPattern::RecordPattern { name, fields, rest } => IrPattern::RecordPattern {
            name: name.clone(),
            rest: *rest,
            fields: fields.iter().map(|f| IrFieldPattern {
                name: f.name.clone(),
                pattern: f.pattern.as_ref().map(|p| {
                    let fty = cx.cases.field_ty(ty, name, &f.name).unwrap_or(Ty::Unknown);
                    pull_lists(p, &fty, cx, out)
                }),
            }).collect(),
        },
        other => other.clone(),
    }
}

/// `pat` with every binder `keep` does not name erased: the guard's copy of a
/// list pattern binds only what the guard reads.
fn keep_binders(pat: &IrPattern, keep: &HashSet<VarId>) -> IrPattern {
    let k = |p: &IrPattern| keep_binders(p, keep);
    match pat {
        IrPattern::Bind { var, .. } if !keep.contains(var) => IrPattern::Wildcard,
        IrPattern::As { var, inner, .. } if !keep.contains(var) => k(inner),
        IrPattern::As { var, ty, inner } => IrPattern::As { var: *var, ty: ty.clone(), inner: Box::new(k(inner)) },
        IrPattern::Some { inner } => IrPattern::Some { inner: Box::new(k(inner)) },
        IrPattern::Ok { inner } => IrPattern::Ok { inner: Box::new(k(inner)) },
        IrPattern::Err { inner } => IrPattern::Err { inner: Box::new(k(inner)) },
        IrPattern::Tuple { elements } => IrPattern::Tuple { elements: elements.iter().map(k).collect() },
        IrPattern::Constructor { name, args } => IrPattern::Constructor { name: name.clone(), args: args.iter().map(k).collect() },
        IrPattern::List { elements, rest } => IrPattern::List {
            elements: elements.iter().map(k).collect(),
            rest: rest.as_ref().map(|r| Box::new(k(r))),
        },
        IrPattern::RecordPattern { name, fields, rest } => IrPattern::RecordPattern {
            name: name.clone(),
            rest: *rest,
            fields: fields.iter().map(|f| IrFieldPattern {
                name: f.name.clone(),
                pattern: f.pattern.as_ref().map(k),
            }).collect(),
        },
        other => other.clone(),
    }
}

fn bool_lit(value: bool) -> IrExpr {
    IrExpr { kind: IrExprKind::LitBool { value }, ty: Ty::Bool, span: None, def_id: None }
}

fn two_arm_match(subject: IrExpr, first: IrMatchArm, rest: IrExpr, ty: &Ty) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Match {
            subject: Box::new(subject),
            arms: vec![first, IrMatchArm { pattern: IrPattern::Wildcard, guard: None, body: rest }],
        },
        ty: ty.clone(),
        span: None, def_id: None,
    }
}

/// `if c then true else false` is `c`, also as a block's tail: the shape
/// test of a guard-free list arm lowers to exactly that.
fn simplify_bool(e: IrExpr) -> IrExpr {
    let lit = |e: &IrExpr, v: bool| matches!(e.kind, IrExprKind::LitBool { value } if value == v);
    match e.kind {
        IrExprKind::If { cond, then, else_ } if lit(&then, true) && lit(&else_, false) => *cond,
        IrExprKind::Block { stmts, expr: Some(tail) } => IrExpr {
            kind: IrExprKind::Block { stmts, expr: Some(Box::new(simplify_bool(*tail))) },
            ..e
        },
        kind => IrExpr { kind, ..e },
    }
}

/// Rewrite one arm whose pattern holds a list pattern (module doc).
fn rewrite_arm(arm: IrMatchArm, subject_ty: &Ty, result_ty: &Ty, cx: &mut Cx) -> IrMatchArm {
    let mut pulled = Pulled::default();
    let outer = pull_lists(&arm.pattern, subject_ty, cx, &mut pulled);
    let (test_subject, test_pat) = if pulled.reads.len() == 1 {
        (pulled.reads.remove(0), pulled.lists.remove(0))
    } else {
        let tys = pulled.reads.iter().map(|e| e.ty.clone()).collect();
        (
            IrExpr { kind: IrExprKind::Tuple { elements: pulled.reads }, ty: Ty::Tuple(tys), span: None, def_id: None },
            IrPattern::Tuple { elements: pulled.lists },
        )
    };
    let read: HashSet<VarId> = arm.guard.as_ref()
        .map(|g| almide_ir::free_vars::free_vars(g, &HashSet::new()).into_iter().collect())
        .unwrap_or_default();
    let shape = IrMatchArm { pattern: keep_binders(&test_pat, &read), guard: arm.guard, body: bool_lit(true) };
    let guard = two_arm_match(test_subject.clone(), shape, bool_lit(false), &Ty::Bool);
    let taken = IrMatchArm { pattern: test_pat, guard: None, body: arm.body };
    let body = two_arm_match(test_subject, taken, super::pass_list_pattern::fell_through_every_arm(result_ty), result_ty);
    IrMatchArm {
        pattern: outer,
        guard: Some(simplify_bool(super::pass_list_pattern::rewrite_expr(guard, cx).0)),
        body: super::pass_list_pattern::rewrite_expr(body, cx).0,
    }
}

/// Can `p` never fail against a value of its type?
fn irrefutable(p: &IrPattern, cases: &CaseTable) -> bool {
    match p {
        IrPattern::Wildcard | IrPattern::Bind { .. } => true,
        IrPattern::As { inner, .. } => irrefutable(inner, cases),
        IrPattern::Tuple { elements } => elements.iter().all(|e| irrefutable(e, cases)),
        IrPattern::Constructor { name, args } => cases.is_sole_case(name) && args.iter().all(|a| irrefutable(a, cases)),
        IrPattern::RecordPattern { name, fields, .. } => cases.is_sole_case(name)
            && fields.iter().all(|f| f.pattern.as_ref().is_none_or(|p| irrefutable(p, cases))),
        _ => false,
    }
}

/// Do the UNGUARDED arms alone cover every value, as rustc counts it? The
/// rewritten arms are guarded, so a match the checker proved total through
/// one of them would otherwise die as rustc E0004. A sufficient test: an
/// irrefutable row, or one irrefutable row per case of the subject's enum /
/// `Option` / `Result` / `Bool`.
fn unguarded_cover(arms: &[IrMatchArm], cases: &CaseTable) -> bool {
    let rows: Vec<&IrPattern> = arms.iter().filter(|a| a.guard.is_none()).map(|a| &a.pattern).collect();
    if rows.iter().any(|p| irrefutable(p, cases)) {
        return true;
    }
    let mut seen: HashSet<String> = HashSet::new();
    for p in &rows {
        let tag = match p {
            IrPattern::Constructor { name, args } if args.iter().all(|a| irrefutable(a, cases)) => bare(name).to_string(),
            IrPattern::RecordPattern { name, fields, .. }
                if fields.iter().all(|f| f.pattern.as_ref().is_none_or(|p| irrefutable(p, cases))) => bare(name).to_string(),
            IrPattern::Some { inner } if irrefutable(inner, cases) => "some".into(),
            IrPattern::None => "none".into(),
            IrPattern::Ok { inner } if irrefutable(inner, cases) => "ok".into(),
            IrPattern::Err { inner } if irrefutable(inner, cases) => "err".into(),
            IrPattern::Literal { expr } => match expr.kind {
                IrExprKind::LitBool { value } => value.to_string(),
                _ => continue,
            },
            _ => continue,
        };
        seen.insert(tag);
    }
    let all = |tags: &[&str]| tags.iter().all(|t| seen.contains(*t));
    if all(&["some", "none"]) || all(&["ok", "err"]) || all(&["true", "false"]) {
        return true;
    }
    seen.iter().next().and_then(|t| cases.by_ctor.get(t)).is_some_and(|found| found.iter().any(|c| {
        cases.enum_cases.get(&c.enum_name).is_some_and(|all_cases| all_cases.iter().all(|n| seen.contains(n)))
    }))
}

/// Lower a match whose list patterns may sit at any depth, keeping it one
/// match: every arm holding a list is rewritten in place, and a dead
/// catch-all is appended when the rewrite left rustc unable to see the match
/// is total.
pub(crate) fn lower_in_place(subject: IrExpr, arms: Vec<IrMatchArm>, result_ty: &Ty, cx: &mut Cx) -> IrExprKind {
    let subject_ty = subject.ty.clone();
    let rewrote = arms.iter().any(|a| super::pass_list_pattern::pattern_contains_list(&a.pattern));
    let mut arms: Vec<IrMatchArm> = arms.into_iter().map(|arm| {
        if super::pass_list_pattern::pattern_contains_list(&arm.pattern) {
            rewrite_arm(arm, &subject_ty, result_ty, cx)
        } else {
            arm
        }
    }).collect();
    if rewrote && !unguarded_cover(&arms, cx.cases) {
        arms.push(IrMatchArm {
            pattern: IrPattern::Wildcard,
            guard: None,
            body: super::pass_list_pattern::fell_through_every_arm(result_ty),
        });
    }
    IrExprKind::Match { subject: Box::new(subject), arms }
}
