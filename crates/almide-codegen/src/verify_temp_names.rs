//! SynthesizedNameCapture — a compiler temp read under another temp's name.
//!
//! The Rust walker renders a variable by its NAME (`RenderContext::var_name`),
//! not by its `VarId`. Two distinct compiler-synthesized temps that share a
//! fixed name are therefore one Rust identifier: when both are bound in one
//! scope and the first is read after the second is bound, Rust's shadowing
//! hands the read the SECOND value. The program compiles and computes the
//! wrong result — #3049 (`let __hoist = arg(s, 1); let __hoist = arg(s, 0);
//! assign(s, __hoist, __hoist)`) and the `__opnd` operand hoist
//! (`a()! - b()!` evaluated `b - b`), both silent native miscompiles while
//! wasm, which keys locals by `VarId`, answered correctly.
//!
//! This check re-resolves every read of a synthesized (`VarInfo::synthetic`) var the
//! way rustc will — innermost enclosing binding of that NAME — and refuses the
//! program when the binding found is a different `VarId`. It runs on the
//! final IR of the Rust target, in debug and release, so the bug class aborts
//! with a `[COMPILER BUG]` report instead of emitting a wrong program. The
//! producer-side rule it enforces: a pass that can bind the same temp kind
//! twice in one scope allocates through `VarTable::alloc_fresh`.

use std::collections::HashMap;
use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use almide_base::intern::Sym;

/// A read of synthesized var `read` that Rust would resolve to `shadow`.
#[derive(Debug, PartialEq, Eq)]
pub struct CapturedTemp {
    pub name: String,
    pub func: String,
    pub read: VarId,
    pub shadow: VarId,
}

impl std::fmt::Display for CapturedTemp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "fn `{}`: a read of `{}` (var {}) resolves by name to the later binding (var {})",
            self.func, self.name, self.read.0, self.shadow.0)
    }
}

/// A compiler temp by construction (`VarInfo::synthetic`, #3333) — not by
/// spelling: a user `let __x` renders in the user space and cannot meet one.
fn is_synthesized(info: &VarInfo) -> bool {
    info.synthetic
}

struct Resolver<'a> {
    vt: &'a VarTable,
    scopes: Vec<HashMap<Sym, VarId>>,
    func: String,
    out: Vec<CapturedTemp>,
}

impl Resolver<'_> {
    fn bind(&mut self, id: VarId) {
        let Some(info) = self.vt.entries.get(id.0 as usize) else { return };
        if !is_synthesized(info) { return; }
        if let Some(top) = self.scopes.last_mut() {
            top.insert(info.name, id);
        }
    }

    fn bind_pattern(&mut self, pat: &IrPattern) {
        match pat {
            IrPattern::Bind { var, .. } => self.bind(*var),
            IrPattern::As { var, inner, .. } => { self.bind(*var); self.bind_pattern(inner); }
            IrPattern::Constructor { args, .. } => args.iter().for_each(|a| self.bind_pattern(a)),
            IrPattern::RecordPattern { fields, .. } => {
                fields.iter().filter_map(|f| f.pattern.as_ref()).for_each(|p| self.bind_pattern(p));
            }
            IrPattern::Tuple { elements } => elements.iter().for_each(|e| self.bind_pattern(e)),
            IrPattern::List { elements, rest } => {
                elements.iter().for_each(|e| self.bind_pattern(e));
                if let Some(r) = rest { self.bind_pattern(r); }
            }
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => self.bind_pattern(inner),
            IrPattern::Wildcard | IrPattern::None | IrPattern::Literal { .. } => {}
        }
    }

    fn read(&mut self, id: VarId) {
        let Some(info) = self.vt.entries.get(id.0 as usize) else { return };
        if !is_synthesized(info) { return; }
        let found = self.scopes.iter().rev().find_map(|s| s.get(&info.name).copied());
        if let Some(shadow) = found && shadow != id {
            self.out.push(CapturedTemp { name: info.name.to_string(), func: self.func.clone(), read: id, shadow });
        }
    }

    /// A statement list in its own scope: each statement's expressions are
    /// resolved before its own bindings enter scope (`let x = x + 1`).
    fn stmts_scoped(&mut self, stmts: &[IrStmt], tail: Option<&IrExpr>) {
        self.scopes.push(HashMap::new());
        for s in stmts {
            walk_stmt(self, s);
            match &s.kind {
                IrStmtKind::Bind { var, .. } => self.bind(*var),
                IrStmtKind::BindDestructure { pattern, .. } => self.bind_pattern(pattern),
                _ => {}
            }
        }
        if let Some(t) = tail { self.visit_expr(t); }
        self.scopes.pop();
    }
}

impl IrVisitor for Resolver<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Var { id } => self.read(*id),
            IrExprKind::Block { stmts, expr } => self.stmts_scoped(stmts, expr.as_deref()),
            IrExprKind::Lambda { params, body, .. } => {
                self.scopes.push(HashMap::new());
                params.iter().for_each(|(v, _)| self.bind(*v));
                self.visit_expr(body);
                self.scopes.pop();
            }
            IrExprKind::Match { subject, arms } => {
                self.visit_expr(subject);
                for arm in arms {
                    self.scopes.push(HashMap::new());
                    self.bind_pattern(&arm.pattern);
                    if let Some(g) = &arm.guard { self.visit_expr(g); }
                    self.visit_expr(&arm.body);
                    self.scopes.pop();
                }
            }
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.visit_expr(iterable);
                self.scopes.push(HashMap::new());
                self.bind(*var);
                var_tuple.iter().flatten().for_each(|v| self.bind(*v));
                self.stmts_scoped(body, None);
                self.scopes.pop();
            }
            IrExprKind::While { cond, body } => {
                self.visit_expr(cond);
                self.stmts_scoped(body, None);
            }
            _ => walk_expr(self, e),
        }
    }
}

fn check_fn(vt: &VarTable, f: &IrFunction, out: &mut Vec<CapturedTemp>) {
    let mut r = Resolver { vt, scopes: vec![HashMap::new()], func: f.name.to_string(), out: Vec::new() };
    f.params.iter().for_each(|p| r.bind(p.var));
    r.visit_expr(&f.body);
    out.append(&mut r.out);
}

/// Every read of a synthesized temp that Rust's by-name scoping would bind
/// to a different temp. Empty = the emitted names resolve as the IR does.
pub fn collect_captured_temps(program: &IrProgram) -> Vec<CapturedTemp> {
    let mut out = Vec::new();
    for f in &program.functions { check_fn(&program.var_table, f, &mut out); }
    for m in &program.modules {
        for f in &m.functions { check_fn(&program.var_table, f, &mut out); }
    }
    out
}

/// Refuse to emit a program whose synthesized temps capture each other (#3049).
pub fn assert_no_captured_temps(program: &IrProgram) {
    let found = collect_captured_temps(program);
    if found.is_empty() { return; }
    let listed: Vec<String> = found.iter().take(8).map(|c| format!("  - {c}")).collect();
    eprintln!(
        "[COMPILER BUG] {} compiler temp read(s) would bind to another temp of the same name in the \
         emitted Rust (#3049):\n{}\nA pass bound two temps with one fixed name in one scope; it must \
         allocate them through `VarTable::alloc_fresh`. Please report this with the program.",
        found.len(), listed.join("\n"),
    );
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_base::intern::sym;
    use almide_lang::types::Ty;

    fn var(id: VarId) -> IrExpr {
        IrExpr { kind: IrExprKind::Var { id }, ty: Ty::Int, span: None, def_id: None }
    }
    fn lit(n: i64) -> IrExpr {
        IrExpr { kind: IrExprKind::LitInt { value: n }, ty: Ty::Int, span: None, def_id: None }
    }
    fn bind(id: VarId, value: IrExpr) -> IrStmt {
        IrStmt { kind: IrStmtKind::Bind { var: id, mutability: Mutability::Let, ty: Ty::Int, value }, span: None }
    }
    fn block(stmts: Vec<IrStmt>, tail: IrExpr) -> IrExpr {
        IrExpr { kind: IrExprKind::Block { stmts, expr: Some(Box::new(tail)) }, ty: Ty::Int, span: None, def_id: None }
    }
    fn pair(a: IrExpr, b: IrExpr) -> IrExpr {
        IrExpr { kind: IrExprKind::Tuple { elements: vec![a, b] }, ty: Ty::Int, span: None, def_id: None }
    }
    fn program(vt: VarTable, body: IrExpr) -> IrProgram {
        let mut p = IrProgram::default();
        p.var_table = vt;
        p.functions.push(IrFunction {
            name: sym("f"), params: vec![], ret_ty: Ty::Int, body, is_effect: false, is_test: false,
            generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![],
            visibility: IrVisibility::Public, doc: None, blank_lines_before: 0, def_id: None,
            mutated_params: vec![], // fresh-fn: test fixture, zero params
            module_origin: None,
        });
        p
    }

    /// The #3049 shape: two temps named `__hoist` bound in one block, both
    /// read after the second — the first read is captured.
    #[test]
    fn two_fixed_name_temps_read_after_the_second_bind_are_flagged() {
        let mut vt = VarTable::new();
        let a = vt.alloc(sym("__hoist"), Ty::Int, Mutability::Let, None);
        let b = vt.alloc(sym("__hoist"), Ty::Int, Mutability::Let, None);
        let body = block(vec![bind(a, lit(1)), bind(b, lit(0))], pair(var(a), var(b)));
        let found = collect_captured_temps(&program(vt, body));
        assert_eq!(found, vec![CapturedTemp { name: "__hoist".into(), func: "f".into(), read: a, shadow: b }]);
    }

    #[test]
    fn fresh_names_resolve_to_their_own_binding() {
        let mut vt = VarTable::new();
        let a = vt.alloc_fresh("__hoist", Ty::Int, Mutability::Let, None);
        let b = vt.alloc_fresh("__hoist", Ty::Int, Mutability::Let, None);
        assert_ne!(vt.get(a).name, vt.get(b).name);
        let body = block(vec![bind(a, lit(1)), bind(b, lit(0))], pair(var(a), var(b)));
        assert!(collect_captured_temps(&program(vt, body)).is_empty());
    }

    /// A read BEFORE the shadowing bind, and a same-named temp in a NESTED
    /// block, are both sound Rust — neither may be flagged.
    #[test]
    fn sequential_reuse_and_nested_scopes_are_not_flagged() {
        let mut vt = VarTable::new();
        let a = vt.alloc(sym("__opnd"), Ty::Int, Mutability::Let, None);
        let b = vt.alloc(sym("__opnd"), Ty::Int, Mutability::Let, None);
        let c = vt.alloc(sym("__opnd"), Ty::Int, Mutability::Let, None);
        let x = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
        let inner = block(vec![bind(c, lit(2))], var(c));
        let body = block(
            vec![bind(a, lit(1)), bind(x, var(a)), bind(b, inner)],
            pair(var(b), var(x)),
        );
        assert!(collect_captured_temps(&program(vt, body)).is_empty());
    }

    /// A user-named var is never judged: Almide's own shadowing rules own it.
    #[test]
    fn user_names_are_out_of_scope() {
        let mut vt = VarTable::new();
        let a = vt.alloc_source(sym("x"), Ty::Int, Mutability::Let, None);
        let b = vt.alloc_source(sym("x"), Ty::Int, Mutability::Let, None);
        let body = block(vec![bind(a, lit(1)), bind(b, lit(0))], pair(var(a), var(b)));
        assert!(collect_captured_temps(&program(vt, body)).is_empty());
    }

    /// A user binding that SPELLS a temp's name renders in the user space,
    /// so a temp bound over it cannot capture its read (#3333).
    #[test]
    fn a_user_binding_spelling_a_temp_name_is_not_a_temp() {
        let mut vt = VarTable::new();
        let user = vt.alloc_source(sym("__lit_guard_0"), Ty::Int, Mutability::Let, None);
        let temp = vt.alloc(sym("__lit_guard_0"), Ty::Int, Mutability::Let, None);
        let body = block(vec![bind(user, lit(1)), bind(temp, lit(0))], pair(var(user), var(temp)));
        assert!(collect_captured_temps(&program(vt, body)).is_empty());
    }
}
