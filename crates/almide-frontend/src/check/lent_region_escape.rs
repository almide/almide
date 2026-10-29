//! E119: a Region lent to an `@export(c)` fn escapes its call (ADR-0031
//! amendment A2).
//!
//! An `@export(c)` fn's Region parameter views its C caller's buffer, which
//! the caller may free as soon as the call returns. So a copy of the Region
//! must not outlive the call. This pass refuses, in the fn's own body, every
//! way a copy can leave it:
//!
//! - stored in a top-level `var` (`kept = buf`, or a field or element of one);
//! - returned (the fn's return type holds a Region);
//! - captured by a closure (a lambda that names the Region);
//! - put into a container (a list, record, tuple, option, …) that then does
//!   one of the above.
//!
//! The lent names are the Region parameters, then every local `let` / `var`
//! whose value holds a Region and names a lent name (`let v = buf`,
//! `let s = ffi.slice(buf, 0, 4)!`, `let xs = [buf]`), to a fixpoint. The rule
//! reads the checker's types, so `let n = ffi.len(buf)` (an `Int`) is not lent.
//! A call that hands the Region to another fn is not followed: the wrapper's
//! run-time check (`almide_rt_ffi_lent_end`) stays as the backstop for that.
use super::{err, Checker};
use crate::ast::{Decl, Expr, ExprKind, Span, Stmt};
use almide_base::intern::Sym;
use almide_lang::types::extern_abi::is_region;
use std::collections::HashSet;

impl Checker {
    pub(super) fn check_lent_region_escapes(&mut self, program: &crate::ast::Program) {
        let globals: HashSet<Sym> = program
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::TopLet { name, mutable: true, .. } => Some(*name),
                _ => None,
            })
            .collect();
        for decl in &program.decls {
            let Decl::Fn { name, params, return_type, export_attrs, body: Some(body), span, .. } = decl else { continue };
            if !export_attrs.iter().any(|a| a.target.as_str() == "c") {
                continue;
            }
            let lent: Vec<Sym> = params.iter().filter(|p| is_region(&self.resolve_type_expr(&p.ty))).map(|p| p.name).collect();
            if lent.is_empty() {
                continue;
            }
            let mut found: Vec<(String, Option<Span>)> = Vec::new();
            let ret = self.resolve_type_expr(return_type);
            if is_region(&ret) || ret.any_child_recursive(&|c| is_region(c)) {
                found.push(("it is returned (the return type holds a Region)".to_string(), *span));
            }
            let tainted = self.lent_names(body, lent);
            self.find_escapes(body, &tainted, &globals, &mut found);
            for (how, at) in found {
                let mut d = err(
                    format!("a Region lent to @export(c) fn '{name}' escapes its call: {how}"),
                    "a lent Region views the C caller's buffer only for the call: copy what you need out with `ffi.read` / `ffi.to_bytes` and keep the Bytes (ADR-0031 amendment A2)".to_string(),
                    format!("fn {name}"),
                )
                .with_code("E119");
                if let Some(sp) = at {
                    d.file = self.source_file.clone();
                    d.line = Some(sp.line);
                    d.col = Some(sp.col);
                }
                self.emit(d);
            }
        }
    }

    /// Does `e`'s type hold a Region anywhere?
    fn holds_region(&self, e: &Expr) -> bool {
        self.type_map.get(&e.id).is_some_and(|t| {
            let t = super::resolve_ty(t, &self.uf);
            is_region(&t) || t.any_child_recursive(&|c| is_region(c))
        })
    }

    /// The lent names: the Region parameters, then every local binding whose
    /// value holds a Region and names a lent name, to a fixpoint.
    fn lent_names(&self, body: &Expr, params: Vec<Sym>) -> HashSet<Sym> {
        let mut lent: HashSet<Sym> = params.into_iter().collect();
        loop {
            let before = lent.len();
            crate::ast::visit_expr(body, &mut |e| {
                if let ExprKind::Block { stmts, .. } = &e.kind {
                    for s in stmts {
                        if let Stmt::Let { name, value, .. } | Stmt::Var { name, value, .. } | Stmt::Assign { name, value, .. } = s
                            && self.holds_region(value)
                            && mentions(value, &lent)
                        {
                            lent.insert(*name);
                        }
                    }
                }
            });
            if lent.len() == before {
                return lent;
            }
        }
    }

    fn find_escapes(&self, body: &Expr, lent: &HashSet<Sym>, globals: &HashSet<Sym>, found: &mut Vec<(String, Option<Span>)>) {
        // A local `var` of a global's name shadows it in this body.
        let mut locals: HashSet<Sym> = HashSet::new();
        crate::ast::visit_expr(body, &mut |e| {
            if let ExprKind::Block { stmts, .. } = &e.kind {
                for s in stmts {
                    if let Stmt::Var { name, .. } | Stmt::Let { name, .. } = s {
                        locals.insert(*name);
                    }
                }
            }
        });
        let global = |n: &Sym| globals.contains(n) && !locals.contains(n);
        crate::ast::visit_expr(body, &mut |e| match &e.kind {
            ExprKind::Block { stmts, .. } => {
                for s in stmts {
                    let (target, value, span) = match s {
                        Stmt::Assign { name, value, span } => (*name, value, *span),
                        Stmt::IndexAssign { target, value, span, .. } | Stmt::FieldAssign { target, value, span, .. } => (*target, value, *span),
                        _ => continue,
                    };
                    if global(&target) && mentions(value, lent) && self.holds_region(value) {
                        found.push((format!("it is stored in the top-level var `{target}`"), span.or(e.span)));
                    }
                }
            }
            ExprKind::Lambda { body: inner, .. } if mentions(inner, lent) => {
                found.push(("it is captured by a closure".to_string(), e.span));
            }
            _ => {}
        });
    }
}

/// Does `e` name any of `names`?
fn mentions(e: &Expr, names: &HashSet<Sym>) -> bool {
    let mut hit = false;
    crate::ast::visit_expr(e, &mut |x| {
        if let ExprKind::Ident { name } = &x.kind
            && names.contains(name)
        {
            hit = true;
        }
    });
    hit
}
