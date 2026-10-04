//! Instance-parallel `fan` chunks (#3003 stage 1, ADR-0011 §D2a): the IR
//! half. A `list.map(xs, (x) => body)` that is an arm of a `fan { … }`
//! block — the shape native runs on threads (#2044) — is rewritten, when
//! its chunk is PURE and SCALAR-IN / SCALAR-OUT, into
//!
//! ```text
//! { let __fan_xs = xs
//!   fan.__par_K(__fan_xs, list.map(__fan_xs, (x2) => __fan_site_K(x2, c…)), c…) }
//! ```
//!
//! and `fn __fan_site_K(x, c…) -> R = body` (the lambda body, its captures
//! turned into fresh params) joins the same container. The emitter
//! (`fan_par_lower.rs`) exports every `__fan_site_K` and lowers `__par_K` as
//! one host op: a host that can run the chunks on separate instances
//! (the embedded host) fills the results; any other answers "not served"
//! and the second argument — the plain sequential `list.map` — runs, so
//! every other host and artifact form behaves exactly as before.
//!
//! Qualification (all must hold, else the arm is left as it was):
//! - one lambda param of scalar type (Int / Float / Bool);
//! - the element result is a scalar or a tuple of scalars;
//! - every capture is an immutable local of scalar type — never a
//!   top-level let (a child instance never runs `main`, so its globals are
//!   unset), never a `var`;
//! - the body and every user fn it reaches are free of effects: no effect
//!   fn, no builtin outside the scalar/collection stdlib (no `println`, no
//!   `fs`, no `random`), no fn VALUE call, no top-level let read or write,
//!   no nested `fan`, no host splice. An abort (an out-of-bounds index) is
//!   allowed: the host sees the child trap, answers "not served", and the
//!   sequential path reproduces the abort exactly.
//! - the program brackets no budget / timeout region (the deterministic
//!   meter would see the child's work as uncharged).

use std::collections::HashSet;

/// The emitter half (`fan.__par_K` → the host op and its fallback).
#[path = "fan_par_lower.rs"]
pub(crate) mod lower;

use almide_base::intern::sym;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::*;
use almide_types::types::constructor::TypeConstructorId as TC;
use almide_types::types::Ty;

/// The per-chunk export a site's chunk becomes.
pub(crate) const SITE_PREFIX: &str = "__fan_site_";
/// The `fan` module func the rewritten site calls.
pub(crate) const PAR_PREFIX: &str = "__par_";

/// Stdlib modules whose functions are pure over their arguments.
const PURE_MODULES: &[&str] = &["list", "int", "float", "math", "string", "option", "result", "bool", "map", "set"];

fn scalar(t: &Ty) -> bool {
    matches!(t, Ty::Int | Ty::Float | Ty::Bool)
}

fn result_ok(t: &Ty) -> bool {
    scalar(t) || matches!(t, Ty::Tuple(ts) if !ts.is_empty() && ts.iter().all(scalar))
}

/// Rewrite every qualifying site; `None` when there is none (the common
/// program is not cloned).
pub(crate) fn route(ir: &IrProgram) -> Option<IrProgram> {
    if almide_base::env::flag("ALMIDE_FAN_PAR_OFF") || !has_fan(ir) || crate::fuel::program_has_regions(ir) {
        return None;
    }
    let mut out = ir.clone();
    let mut next = 0usize;
    {
        let tl: HashSet<VarId> = out.top_lets.iter().map(|t| t.var).collect();
        let mut fns = std::mem::take(&mut out.functions);
        let mut sites = Vec::new();
        for f in fns.iter_mut().filter(|f| !f.is_test) {
            let mut rw = Rewriter { ir, space: None, vt: &mut out.var_table, tl: &tl, next: &mut next, sites: &mut sites };
            rw.visit_expr_mut(&mut f.body);
        }
        fns.extend(sites);
        out.functions = fns;
    }
    for mi in 0..out.modules.len() {
        let m = &mut out.modules[mi];
        let tl: HashSet<VarId> = m.top_lets.iter().map(|t| t.var).collect();
        let mut fns = std::mem::take(&mut m.functions);
        let mut sites = Vec::new();
        for f in fns.iter_mut().filter(|f| !f.is_test) {
            let mut rw = Rewriter { ir, space: Some(mi), vt: &mut m.var_table, tl: &tl, next: &mut next, sites: &mut sites };
            rw.visit_expr_mut(&mut f.body);
        }
        fns.extend(sites);
        m.functions = fns;
    }
    (next > 0).then_some(out)
}

/// The `(export name, wasm index)` of every reached chunk fn: the host
/// instantiates the module again and calls the chunk by this name. The
/// name is the fn's own (`K` is unique across the program's modules).
pub(crate) fn site_exports(
    program_fns: &[(&IrFunction, Option<String>, u32)],
    visited: &HashSet<usize>,
    table: &crate::FnTable,
) -> Vec<(String, u32)> {
    program_fns
        .iter()
        .enumerate()
        .filter(|(i, (f, _, _))| f.name.as_str().starts_with(SITE_PREFIX) && visited.contains(i))
        .map(|(i, (f, _, _))| (f.name.as_str().to_string(), table.infos[i].wasm_index))
        .collect()
}

fn has_fan(ir: &IrProgram) -> bool {
    struct Scan(bool);
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::Fan { .. }) {
                self.0 = true;
            } else if !self.0 {
                walk_expr(self, e);
            }
        }
    }
    let mut s = Scan(false);
    let bodies = ir.functions.iter().chain(ir.modules.iter().flat_map(|m| m.functions.iter()));
    for f in bodies {
        s.visit_expr(&f.body);
    }
    s.0
}

struct Rewriter<'a> {
    /// The UNREWRITTEN program: callee bodies are judged from here.
    ir: &'a IrProgram,
    /// `None` = the entry program, `Some(i)` = `ir.modules[i]`.
    space: Option<usize>,
    vt: &'a mut VarTable,
    tl: &'a HashSet<VarId>,
    next: &'a mut usize,
    sites: &'a mut Vec<IrFunction>,
}

impl IrMutVisitor for Rewriter<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if let IrExprKind::Fan { exprs } = &mut e.kind {
            for arm in exprs.iter_mut() {
                if let Some(new) = self.try_site(arm) {
                    *arm = new;
                }
            }
        }
        walk_expr_mut(self, e);
    }
}

impl Rewriter<'_> {
    fn try_site(&mut self, arm: &IrExpr) -> Option<IrExpr> {
        let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &arm.kind else {
            return None;
        };
        if module.as_str() != "list" || func.as_str() != "map" || args.len() != 2 {
            return None;
        }
        let Ty::Applied(TC::List, rargs) = &arm.ty else { return None };
        let ret = rargs.first()?;
        let IrExprKind::Lambda { params, body, .. } = &args[1].kind else { return None };
        let [(p, pty)] = params.as_slice() else { return None };
        if !scalar(pty) || !result_ok(ret) || body.ty != *ret {
            return None;
        }
        let bound: HashSet<VarId> = [*p].into_iter().collect();
        let caps = almide_ir::free_vars::free_vars(body, &bound);
        for c in &caps {
            let info = self.vt.get(*c);
            if self.tl.contains(c) || info.module_origin.is_some() || info.mutability != Mutability::Let || !scalar(&info.ty) {
                return None;
            }
        }
        let mut purity = Purity { ir: self.ir, visited: HashSet::new(), ok: true, space: self.space };
        if !purity.body_is_pure(body, self.space) {
            return None;
        }
        let k = *self.next;
        *self.next += 1;
        let site_name = format!("{SITE_PREFIX}{k}");
        let span = arm.span;
        let mk = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span, def_id: None };

        // The site fn: the lambda param, then one fresh param per capture.
        let mut site_body = (**body).clone();
        let mut site_params = vec![param(*p, pty.clone(), self.vt.get(*p).name)];
        for c in &caps {
            let info = self.vt.get(*c).clone();
            let pv = self.vt.alloc(info.name, info.ty.clone(), Mutability::Let, span);
            site_body = almide_ir::substitute::substitute_var_in_expr(&site_body, *c, &mk(IrExprKind::Var { id: pv }, info.ty.clone()));
            site_params.push(param(pv, info.ty, info.name));
        }
        self.sites.push(IrFunction {
            name: sym(&site_name),
            params: site_params,
            ret_ty: ret.clone(),
            body: site_body,
            is_effect: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Private,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: Vec::new(),
            module_origin: None,
        });

        // The rewritten site.
        let xs = &args[0];
        let xs_var = self.vt.alloc_fresh("__fan_xs", xs.ty.clone(), Mutability::Let, span);
        let x2 = self.vt.alloc_fresh("__fan_x", pty.clone(), Mutability::Let, span);
        let cap_args: Vec<IrExpr> = caps.iter().map(|c| mk(IrExprKind::Var { id: *c }, self.vt.get(*c).ty.clone())).collect();
        let mut site_args = vec![mk(IrExprKind::Var { id: x2 }, pty.clone())];
        site_args.extend(cap_args.iter().cloned());
        let site_call = mk(
            IrExprKind::Call { target: CallTarget::Named { name: sym(&site_name) }, args: site_args, type_args: vec![] },
            ret.clone(),
        );
        let lambda_ty = Ty::Fn { params: vec![pty.clone()], ret: Box::new(ret.clone()), is_effect: false };
        let fallback = mk(
            IrExprKind::Call {
                target: CallTarget::Module { module: sym("list"), func: sym("map"), def_id: None },
                args: vec![
                    mk(IrExprKind::Var { id: xs_var }, xs.ty.clone()),
                    mk(IrExprKind::Lambda { params: vec![(x2, pty.clone())], body: Box::new(site_call), lambda_id: None }, lambda_ty),
                ],
                type_args: vec![],
            },
            arm.ty.clone(),
        );
        let mut par_args = vec![mk(IrExprKind::Var { id: xs_var }, xs.ty.clone()), fallback];
        par_args.extend(cap_args);
        let par = mk(
            IrExprKind::Call {
                target: CallTarget::Module { module: sym("fan"), func: sym(&format!("{PAR_PREFIX}{k}")), def_id: None },
                args: par_args,
                type_args: vec![],
            },
            arm.ty.clone(),
        );
        Some(mk(
            IrExprKind::Block {
                stmts: vec![IrStmt {
                    kind: IrStmtKind::Bind { var: xs_var, mutability: Mutability::Let, ty: xs.ty.clone(), value: xs.clone() },
                    span,
                }],
                expr: Some(Box::new(par)),
            },
            arm.ty.clone(),
        ))
    }
}

fn param(var: VarId, ty: Ty, name: almide_base::intern::Sym) -> IrParam {
    IrParam { var, ty, name, borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }
}

/// The effect-freedom judge over a chunk body and every user fn it reaches.
struct Purity<'a> {
    ir: &'a IrProgram,
    visited: HashSet<(Option<usize>, almide_base::intern::Sym)>,
    ok: bool,
    /// The space of the body being walked (the callee's, inside a callee).
    space: Option<usize>,
}

impl Purity<'_> {
    fn body_is_pure(&mut self, body: &IrExpr, space: Option<usize>) -> bool {
        let saved = self.space;
        self.space = space;
        self.visit_expr(body);
        self.space = saved;
        self.ok
    }

    fn top_lets(&self, space: Option<usize>) -> &[IrTopLet] {
        match space {
            None => &self.ir.top_lets,
            Some(i) => &self.ir.modules[i].top_lets,
        }
    }

    fn var_table(&self, space: Option<usize>) -> &VarTable {
        match space {
            None => &self.ir.var_table,
            Some(i) => &self.ir.modules[i].var_table,
        }
    }

    fn is_global(&self, id: VarId) -> bool {
        self.top_lets(self.space).iter().any(|t| t.var == id)
            || self.var_table(self.space).entries.get(id.0 as usize).is_some_and(|v| v.module_origin.is_some())
    }

    fn is_ctor(&self, name: &str) -> bool {
        let decls = self.ir.type_decls.iter().chain(self.ir.modules.iter().flat_map(|m| m.type_decls.iter()));
        decls.into_iter().any(|d| matches!(&d.kind, IrTypeDeclKind::Variant { cases, .. } if cases.iter().any(|c| c.name.as_str() == name)))
    }

    /// Judge a call to a user fn `name` in `space`; false when it is not one.
    fn user_fn(&mut self, space: Option<usize>, name: almide_base::intern::Sym) -> bool {
        let fns = match space {
            None => &self.ir.functions,
            Some(i) => &self.ir.modules[i].functions,
        };
        let Some(f) = fns.iter().find(|f| f.name == name) else { return false };
        if f.is_effect || !f.extern_attrs.is_empty() {
            self.ok = false;
            return true;
        }
        if self.visited.insert((space, name)) {
            let body = f.body.clone();
            self.body_is_pure(&body, space);
        }
        true
    }

    fn judge_target(&mut self, target: &CallTarget) {
        match target {
            CallTarget::Named { name } => {
                if !self.user_fn(self.space, *name) && !self.is_ctor(name.as_str()) {
                    self.ok = false;
                }
            }
            CallTarget::Module { module, func, .. } => {
                if let Some(mi) = self.ir.modules.iter().position(|m| m.name == *module)
                    && !PURE_MODULES.contains(&module.as_str())
                {
                    if !self.user_fn(Some(mi), *func) {
                        self.ok = false;
                    }
                } else if !PURE_MODULES.contains(&module.as_str()) {
                    self.ok = false;
                }
            }
            CallTarget::Method { .. } | CallTarget::Computed { .. } => self.ok = false,
        }
    }
}

impl IrVisitor for Purity<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        if !self.ok {
            return;
        }
        match &e.kind {
            IrExprKind::Var { id } if self.is_global(*id) => self.ok = false,
            IrExprKind::Call { target, .. } | IrExprKind::TailCall { target, .. } => self.judge_target(target),
            IrExprKind::Fan { .. }
            | IrExprKind::FnRef { .. }
            | IrExprKind::RuntimeCall { .. }
            | IrExprKind::RustMacro { .. }
            | IrExprKind::RenderedCall { .. }
            | IrExprKind::InlineRust { .. }
            | IrExprKind::ClosureCreate { .. }
            | IrExprKind::EnvLoad { .. }
            | IrExprKind::Hole
            | IrExprKind::Todo { .. } => self.ok = false,
            _ => {}
        }
        if self.ok {
            walk_expr(self, e);
        }
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        if !self.ok {
            return;
        }
        let written = match &s.kind {
            IrStmtKind::Assign { var, .. } => Some(*var),
            IrStmtKind::IndexAssign { target, .. }
            | IrStmtKind::MapInsert { target, .. }
            | IrStmtKind::FieldAssign { target, .. } => Some(*target),
            _ => None,
        };
        if written.is_some_and(|v| self.is_global(v)) {
            self.ok = false;
            return;
        }
        walk_stmt(self, s);
    }
}
