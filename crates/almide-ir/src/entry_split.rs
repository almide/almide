//! Callback splitting at a self-hosted leg's boundary (R-map-09, dialect epoch 15).
//!
//! The surface hands a callback one TUPLE where an element is a pair:
//! `map.filter(m, f: ((K, V)) -> Bool)`, `map.fold(m, init, f: (A, (K, V)) -> A)`.
//! The frontend lowers the usual `((k, v)) => body` as ONE runtime parameter
//! destructured on entry. The two legs that link a self-hosted implementation
//! (the v1 native render in almide-mir, the structural wasm leg) implement some
//! of these surfaces with the tuple's elements as SEPARATE parameters: the
//! `map_core` / `map_skv` / `map_str` bodies call `f(k, v)` and
//! `f(acc, k, v)`, and the wasm emitter's inline map arms load the key and the
//! value of an entry into two locals.
//!
//! This pass adapts a call to that protocol before either leg lowers it. Which
//! calls, and how, is read from the declarations, never from a list of names:
//! a call is adapted when the self-hosted body registered for its surface
//! (`self_host_registry`) declares a callback parameter with MORE parameters
//! than the argument's function type, and spreading the argument's tuple
//! parameters reaches exactly that count. Every other call is left alone, so a
//! body that takes the tuple as declared keeps it.
//!
//! The adapted callback:
//! - `((k, v)) => body`, the destructure-on-entry form: its parameters become
//!   `k, v` and the destructure is dropped — the IR the legs lowered before
//!   the surface changed, so no pair is built per entry;
//! - any other lambda `(e) => body`: `(k, v) => { let e = (k, v); body }`;
//! - a function value `f`: `(k, v) => f((k, v))`.
//!
//! The interpreter and the codegen-v3 native leg read the tuple form as the
//! surface declares it and do not run this pass.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::visit::{walk_expr, IrVisitor};
use crate::visit_mut::{walk_expr_mut, IrMutVisitor};
use crate::{CallTarget, IrExpr, IrExprKind, IrPattern, IrProgram, IrStmt, IrStmtKind, Mutability, VarId, VarTable};
use almide_lang::ast::{Decl, TypeExpr};
use almide_lang::types::Ty;

/// Adapt every call whose self-hosted body takes a callback's tuple
/// parameters spread (see the module doc). Returns the callbacks rewritten.
pub fn split_entry_callbacks(program: &mut IrProgram) -> usize {
    let IrProgram { functions, modules, top_lets, var_table, .. } = program;
    let mut n = 0;
    {
        let mut s = Split { var_table, rewritten: 0 };
        for f in functions.iter_mut() {
            s.visit_expr_mut(&mut f.body);
        }
        for tl in top_lets.iter_mut() {
            s.visit_expr_mut(&mut tl.value);
        }
        n += s.rewritten;
    }
    for m in modules.iter_mut() {
        let mut s = Split { var_table: &mut m.var_table, rewritten: 0 };
        for f in m.functions.iter_mut() {
            s.visit_expr_mut(&mut f.body);
        }
        for tl in m.top_lets.iter_mut() {
            s.visit_expr_mut(&mut tl.value);
        }
        n += s.rewritten;
    }
    n
}

/// `module.fn` → (embedded source, implementing fn) for every registered
/// self-hosted body.
fn registry() -> &'static HashMap<&'static str, (&'static str, &'static str)> {
    static CELL: OnceLock<HashMap<&'static str, (&'static str, &'static str)>> = OnceLock::new();
    CELL.get_or_init(|| {
        let mut out = HashMap::new();
        for (src, rows) in almide_lang::self_host_registry::self_host_runtime() {
            for (impl_fn, surface) in *rows {
                out.insert(*surface, (*src, *impl_fn));
            }
        }
        out
    })
}

/// The parameter count of the callback the self-hosted body for `surface`
/// declares at parameter `i`, or `None` when that parameter is not a function.
fn impl_callback_arity(surface: &str, i: usize) -> Option<usize> {
    let (src, impl_fn) = *registry().get(surface)?;
    let prog = almide_lang::parse_cached(src)?;
    prog.decls.iter().find_map(|d| match d {
        Decl::Fn { name, params, .. } if name.as_str() == impl_fn => match &params.get(i)?.ty {
            TypeExpr::Fn { params, .. } => Some(params.len()),
            _ => None,
        },
        _ => None,
    })
}

struct Split<'a> {
    var_table: &'a mut VarTable,
    rewritten: usize,
}

impl IrMutVisitor for Split<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        walk_expr_mut(self, expr);
        let (target, args) = match &mut expr.kind {
            IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => (target, args),
            _ => return,
        };
        let CallTarget::Module { module, func, .. } = target else { return };
        let surface = format!("{}.{}", module.as_str(), crate::mono_base_or_self(func.as_str()));
        if !registry().contains_key(surface.as_str()) {
            return;
        }
        for (i, arg) in args.iter_mut().enumerate() {
            let Ty::Fn { params, .. } = &arg.ty else { continue };
            let Some(want) = impl_callback_arity(&surface, i) else { continue };
            let Some(mask) = spread_mask(params, want) else { continue };
            if self.split(arg, &mask) {
                self.rewritten += 1;
            }
        }
    }
}

/// How many separate parameters a parameter of this type spreads into.
fn tuple_width(t: &Ty) -> usize {
    match t {
        Ty::Tuple(es) if es.len() >= 2 => es.len(),
        _ => 1,
    }
}

/// Which of a callback's parameters to spread so it takes `want` separate
/// parameters, or `None` when no spread reaches that count. Every tuple
/// parameter spreads first; when that overshoots — an accumulator that is
/// itself a tuple, `map.fold(m, (0, 0), (acc, (k, v)) => …)` — only the
/// trailing parameter (the entry) spreads.
pub fn spread_mask(params: &[Ty], want: usize) -> Option<Vec<bool>> {
    let all: Vec<bool> = params.iter().map(|t| tuple_width(t) > 1).collect();
    let count = |mask: &[bool]| -> usize {
        params.iter().zip(mask).map(|(t, &m)| if m { tuple_width(t) } else { 1 }).sum()
    };
    let last: Vec<bool> = (0..params.len()).map(|i| i + 1 == params.len() && all[i]).collect();
    [all, last].into_iter().find(|m| m.iter().any(|&b| b) && count(m) == want)
}

impl Split<'_> {
    fn fresh(&mut self, ty: &Ty) -> VarId {
        self.var_table.alloc_fresh("__entry", ty.clone(), Mutability::Let, None)
    }

    /// Rewrite one callback argument to take its tuple parameters spread.
    fn split(&mut self, arg: &mut IrExpr, mask: &[bool]) -> bool {
        let Ty::Fn { params: ptys, ret, is_effect } = arg.ty.clone() else { return false };
        let spread_tys: Vec<Ty> = ptys
            .iter()
            .zip(mask)
            .flat_map(|(t, &spread)| match t {
                Ty::Tuple(es) if spread => es.to_vec(),
                other => vec![other.clone()],
            })
            .collect();
        let new_ty = Ty::Fn { params: spread_tys, ret: ret.clone(), is_effect };
        let span = arg.span;
        if let IrExprKind::Lambda { params, body, .. } = &mut arg.kind {
            let mut new_params: Vec<(VarId, Ty)> = Vec::new();
            let mut prologue: Vec<IrStmt> = Vec::new();
            for ((var, pty), &spread) in std::mem::take(params).into_iter().zip(mask) {
                let Ty::Tuple(es) = &pty else {
                    new_params.push((var, pty));
                    continue;
                };
                if !spread {
                    new_params.push((var, pty));
                    continue;
                }
                if es.len() < 2 {
                    new_params.push((var, pty));
                    continue;
                }
                match take_entry_destructure(body, var, es.len()) {
                    Some(binds) => {
                        for (b, ety) in binds.into_iter().zip(es.iter()) {
                            let v = match b {
                                Some(v) => v,
                                None => self.fresh(ety),
                            };
                            new_params.push((v, ety.clone()));
                        }
                    }
                    None => {
                        // `(e) => body`: the elements arrive separately and `e`
                        // is rebuilt from them.
                        let parts: Vec<VarId> = es.iter().map(|t| self.fresh(t)).collect();
                        let tuple = IrExpr {
                            kind: IrExprKind::Tuple {
                                elements: parts
                                    .iter()
                                    .zip(es.iter())
                                    .map(|(v, t)| IrExpr { kind: IrExprKind::Var { id: *v }, ty: t.clone(), span, def_id: None })
                                    .collect(),
                            },
                            ty: pty.clone(),
                            span,
                            def_id: None,
                        };
                        prologue.push(IrStmt {
                            kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty: pty.clone(), value: tuple },
                            span,
                        });
                        new_params.extend(parts.into_iter().zip(es.iter().cloned()));
                    }
                }
            }
            *params = new_params;
            if !prologue.is_empty() {
                let inner = std::mem::take(&mut **body);
                let ty = inner.ty.clone();
                **body = IrExpr {
                    kind: IrExprKind::Block { stmts: prologue, expr: Some(Box::new(inner)) },
                    ty,
                    span,
                    def_id: None,
                };
            }
            arg.ty = new_ty;
            return true;
        }
        // A function value: `(k, v) => f((k, v))`.
        let callee = std::mem::take(arg);
        let mut new_params: Vec<(VarId, Ty)> = Vec::new();
        let mut call_args: Vec<IrExpr> = Vec::new();
        for (pty, &spread) in ptys.iter().zip(mask) {
            match pty {
                Ty::Tuple(es) if spread => {
                    let parts: Vec<VarId> = es.iter().map(|t| self.fresh(t)).collect();
                    call_args.push(IrExpr {
                        kind: IrExprKind::Tuple {
                            elements: parts
                                .iter()
                                .zip(es.iter())
                                .map(|(v, t)| IrExpr { kind: IrExprKind::Var { id: *v }, ty: t.clone(), span, def_id: None })
                                .collect(),
                        },
                        ty: pty.clone(),
                        span,
                        def_id: None,
                    });
                    new_params.extend(parts.into_iter().zip(es.iter().cloned()));
                }
                other => {
                    let v = self.fresh(other);
                    call_args.push(IrExpr { kind: IrExprKind::Var { id: v }, ty: other.clone(), span, def_id: None });
                    new_params.push((v, other.clone()));
                }
            }
        }
        let target = match &callee.kind {
            IrExprKind::FnRef { name } => CallTarget::Named { name: *name },
            _ => CallTarget::Computed { callee: Box::new(callee) },
        };
        let call = IrExpr {
            kind: IrExprKind::Call { target, args: call_args, type_args: vec![] },
            ty: (*ret).clone(),
            span,
            def_id: None,
        };
        *arg = IrExpr {
            kind: IrExprKind::Lambda { params: new_params, body: Box::new(call), lambda_id: None },
            ty: new_ty,
            span,
            def_id: None,
        };
        true
    }
}

/// When `body` opens with `let (a, b, …) = <pair>` — the frontend's lowering of
/// a tuple-pattern parameter — and reads `pair` nowhere else, remove that
/// statement and answer what each element binds (`None` for a wildcard).
fn take_entry_destructure(body: &mut IrExpr, pair: VarId, width: usize) -> Option<Vec<Option<VarId>>> {
    let IrExprKind::Block { stmts, expr } = &mut body.kind else { return None };
    let first = stmts.first()?;
    let IrStmtKind::BindDestructure { pattern: IrPattern::Tuple { elements }, value } = &first.kind else {
        return None;
    };
    if elements.len() != width || !matches!(value.kind, IrExprKind::Var { id } if id == pair) {
        return None;
    }
    let mut binds = Vec::with_capacity(width);
    for el in elements {
        match el {
            IrPattern::Bind { var, .. } => binds.push(Some(*var)),
            IrPattern::Wildcard => binds.push(None),
            _ => return None,
        }
    }
    let mut reads = Reads { var: pair, n: 0 };
    for s in &stmts[1..] {
        reads.visit_stmt(s);
    }
    if let Some(e) = expr {
        reads.visit_expr(e);
    }
    if reads.n != 0 {
        return None;
    }
    stmts.remove(0);
    Some(binds)
}

struct Reads {
    var: VarId,
    n: usize,
}

impl IrVisitor for Reads {
    fn visit_expr(&mut self, e: &IrExpr) {
        if matches!(e.kind, IrExprKind::Var { id } if id == self.var) {
            self.n += 1;
        }
        walk_expr(self, e);
    }
}
