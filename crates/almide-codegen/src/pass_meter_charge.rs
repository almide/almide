//! MeterChargePass (#3673) — the standard native codegen meters the program's
//! `fan.bounded` / `fan.race` / `fan.timeout` regions itself.
//!
//! #3669 hands a region to the v1 render, whose subset refuses a region that
//! calls a stdlib fn it has no native body for (`string.split`) or whose
//! signature it spells differently (a record, `List[String]`, a closure). This
//! pass places the deterministic charges (ALS-DT2) where the wasm leg places
//! them, on the IR, so a region of any shape this codegen builds is metered:
//!
//!   * an ENTRY charge on every user fn the shared plan does not exempt
//!     (`almide_ir::meter_plan` — the one plan the wasm emitter reads);
//!   * a LOOP-HEAD charge on every evaluation of a `while` condition and every
//!     `for` head, the exhausting one included (a `for` head is charged by
//!     the iterator adapter the walker wraps the iterable in);
//!   * a HOP charge on every call through a closure value — at the lambda's
//!     entry. A lambda written directly as the callback of a stdlib
//!     higher-order call (`almide_ir::meter_plan::INLINED_CALLBACK_MODULES`)
//!     is the call's loop body on the wasm leg, not a closure: no hop;
//!   * the size-proportional charge of a string concat (`1 + len / 16`).
//!
//! A stdlib body charges nothing, on either leg (the wasm leg's pool bodies
//! never charge). An effect fn's own body runs outside every region (a region
//! body is pure), so only the closures it builds are charged.
//!
//! At each charge site the CUT (fuel below zero, or a passed deadline)
//! returns the zero value of the enclosing fn's — or closure's — return type,
//! as the wasm leg's cut does. A return type with no spellable zero (a fn
//! value) keeps the charge and drops the cut: the next charge site in a caller
//! cuts instead. The runtime the charges call is
//! `almide_mir::pipeline::native_meter_runtime`, which the CLI links.

use almide_base::intern::sym;
use almide_ir::meter_plan::{meter_plan, program_has_regions, MeterPlan, INLINED_CALLBACK_MODULES};
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::*;
use almide_lang::types::constructor::TypeConstructorId as C;
use almide_lang::types::Ty;
use std::collections::HashSet;
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct MeterChargePass;

impl NanoPass for MeterChargePass {
    fn name(&self) -> &str { "MeterCharge" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }

    /// The charges sit on the IR the wasm leg meters: stdlib calls still
    /// spelled as module calls (the inline-callback test reads them), loops
    /// and calls neither hoisted, fused, tail-converted nor cloned into
    /// region twins — every pass that would move one runs after this.
    fn run_before(&self) -> Vec<&'static str> {
        vec![
            "PatternLiteralGuard", "ResolveCalls", "RegionWindow", "BoxDeref", "LICM",
            "EggSaturation", "MatrixShapeSpec", "ConstFold", "IntrinsicLowering", "BorrowInsertion",
        ]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        if !program.codegen_annotations.meter_in_codegen || !program_has_regions(&program) {
            return PassResult { program, changed: false };
        }
        let registry = registry_impl_names();
        let plan = meter_plan(&program, &registry);
        let decls = program.type_decls.clone();
        let mut loops = HashSet::new();
        let IrProgram { functions, modules, .. } = &mut program;
        for f in functions.iter_mut().filter(|f| !f.is_test) {
            meter_fn(f, &plan, &decls, &mut loops);
        }
        for f in modules.iter_mut().flat_map(|m| m.functions.iter_mut()) {
            if plan.user.contains(f.name.as_str()) {
                meter_fn(f, &plan, &decls, &mut loops);
            }
        }
        program.codegen_annotations.metered_loop_vars = loops;
        PassResult { program, changed: true }
    }
}

/// The self-host registry's implementation symbols — the pool bodies the
/// plan never charges.
fn registry_impl_names() -> HashSet<&'static str> {
    almide_lang::self_host_registry::self_host_runtime()
        .iter()
        .flat_map(|(_, maps)| maps.iter().map(|(impl_fn, _)| *impl_fn))
        .collect()
}

/// Charge one fn: its body's sites, then its entry (unless exempt).
fn meter_fn(f: &mut IrFunction, plan: &MeterPlan, decls: &[IrTypeDecl], loops: &mut HashSet<VarId>) {
    if !f.extern_attrs.is_empty() || matches!(f.body.kind, IrExprKind::Hole) {
        return;
    }
    let active = !f.is_effect;
    let zero = if active { zero_of(&f.ret_ty, decls, 0) } else { None };
    let mut m = Metering { decls, cut: Cut { zero, inline: false }, active, loops };
    m.visit_expr_mut(&mut f.body);
    let name = f.name.as_str();
    if active && plan.user.contains(name) && !plan.exempt.contains(name) {
        prepend(&mut f.body, m.cut.stmt("__almd_meter_tick(1)"));
    }
}

/// What a cut does where it fires. `zero` is the return value of the
/// current fn or closure (`None`: charge without a cut). `inline`: the site
/// is in a callback the wasm leg runs as its stdlib call's loop body, so a
/// cut there leaves the ENCLOSING fn — the closure returns, flags the unwind,
/// and the call site returns in turn.
#[derive(Clone)]
struct Cut {
    zero: Option<IrExpr>,
    inline: bool,
}

impl Cut {
    /// `(if <check> { return <zero>; })`, or the bare check without a zero.
    fn guarded(&self, check: &str) -> IrExpr {
        let leave = if self.inline { "__almd_meter_unwind(); return {z};" } else { "return {z};" };
        let (template, args) = match &self.zero {
            Some(z) => (format!("(if {check} {{ {leave} }})"), vec![(sym("z"), z.clone())]),
            None => (format!("({check})"), Vec::new()),
        };
        IrExpr { kind: IrExprKind::InlineRust { template, args }, ty: Ty::Unit, span: None, def_id: None }
    }

    fn stmt(&self, check: &str) -> IrStmt {
        expr_stmt(self.guarded(check))
    }

    /// `{ let __almd_s = <concat>; <cut on its byte length>; __almd_s }`.
    fn concat(&self, concat: IrExpr) -> IrExpr {
        let (ty, span) = (concat.ty.clone(), concat.span);
        let site = self.guarded("__almd_meter_tick_len(__almd_s.len())");
        let template = "{ let __almd_s = {e}; {site}; __almd_s }".to_string();
        let args = vec![(sym("e"), concat), (sym("site"), site)];
        IrExpr { kind: IrExprKind::InlineRust { template, args }, ty, span, def_id: None }
    }

    /// A stdlib call whose inline callback may have cut: leave as the
    /// callback's cut would have on the wasm leg. In an enclosing fn or
    /// closure the unwind ends here; in an inline callback it carries on.
    fn after_inline_call(&self, call: IrExpr) -> IrExpr {
        let (ty, span) = (call.ty.clone(), call.span);
        let check = if self.inline { "__almd_meter_unwinding()" } else { "__almd_meter_unwind_take()" };
        let site = self.guarded(check);
        let template = "{ let __almd_r = {c}; {site}; __almd_r }".to_string();
        let args = vec![(sym("c"), call), (sym("site"), site)];
        IrExpr { kind: IrExprKind::InlineRust { template, args }, ty, span, def_id: None }
    }
}

/// The charge-site rewriter. `active` is false in an effect fn's own body,
/// which no region runs.
struct Metering<'a> {
    decls: &'a [IrTypeDecl],
    cut: Cut,
    active: bool,
    loops: &'a mut HashSet<VarId>,
}

impl IrMutVisitor for Metering<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if matches!(e.kind, IrExprKind::Lambda { .. }) {
            return self.visit_lambda(e, false);
        }
        if let Some(args) = inline_callback_args(e) {
            let mut inline = false;
            for a in args.iter_mut() {
                inline |= matches!(a.kind, IrExprKind::Lambda { .. });
                match a.kind {
                    IrExprKind::Lambda { .. } => self.visit_lambda(a, true),
                    _ => self.visit_expr_mut(a),
                }
            }
            if inline && self.active {
                let call = std::mem::take(e);
                *e = self.cut.after_inline_call(call);
            }
            return;
        }
        walk_expr_mut(self, e);
        if self.active {
            self.charge_site(e);
        }
    }
}

/// The args of a stdlib higher-order call whose lambda callbacks the wasm
/// leg runs inline.
fn inline_callback_args(e: &mut IrExpr) -> Option<&mut Vec<IrExpr>> {
    match &mut e.kind {
        IrExprKind::Call { target: CallTarget::Module { module, .. }, args, .. }
            if INLINED_CALLBACK_MODULES.contains(&module.as_str()) => Some(args),
        _ => None,
    }
}

impl Metering<'_> {
    /// A lambda: its body is metered against ITS return type (a cut returns
    /// from the closure). A closure value is charged a hop at entry; an
    /// inline callback is not, and returns at entry while an unwind is on.
    fn visit_lambda(&mut self, e: &mut IrExpr, inline: bool) {
        let Ty::Fn { ret, is_effect, .. } = &e.ty else {
            return walk_expr_mut(self, e);
        };
        let active = !*is_effect;
        let zero = if active { zero_of(ret, self.decls, 0) } else { None };
        let saved = (std::mem::replace(&mut self.cut, Cut { zero, inline }), std::mem::replace(&mut self.active, active));
        walk_expr_mut(self, e);
        let entry = if inline { "__almd_meter_unwinding()" } else { "__almd_meter_tick(1)" };
        if let (true, IrExprKind::Lambda { body, .. }) = (self.active, &mut e.kind) {
            prepend(&mut **body, self.cut.stmt(entry));
        }
        (self.cut, self.active) = saved;
    }

    /// The charge a node carries, placed after its children were metered.
    fn charge_site(&mut self, e: &mut IrExpr) {
        match &mut e.kind {
            IrExprKind::While { cond, .. } => prepend(&mut **cond, self.cut.stmt("__almd_meter_tick(1)")),
            IrExprKind::ForIn { var, body, .. } => {
                self.loops.insert(*var);
                body.insert(0, self.cut.stmt("__almd_meter_pending()"));
                let for_in = std::mem::take(e);
                let span = for_in.span;
                let stmts = vec![expr_stmt(for_in), self.cut.stmt("__almd_meter_pending()")];
                *e = IrExpr { kind: IrExprKind::Block { stmts, expr: None }, ty: Ty::Unit, span, def_id: None };
            }
            IrExprKind::BinOp { op: BinOp::ConcatStr, .. } => {
                let concat = std::mem::take(e);
                *e = self.cut.concat(concat);
            }
            _ => {}
        }
    }
}

fn expr_stmt(expr: IrExpr) -> IrStmt {
    let span = expr.span;
    IrStmt { kind: IrStmtKind::Expr { expr }, span }
}

/// Put `stmt` before everything `e` evaluates.
fn prepend(e: &mut IrExpr, stmt: IrStmt) {
    let inner = std::mem::take(e);
    let (ty, span) = (inner.ty.clone(), inner.span);
    *e = IrExpr { kind: IrExprKind::Block { stmts: vec![stmt], expr: Some(Box::new(inner)) }, ty, span, def_id: None };
}

/// The zero value of `ty`: what the cut returns. A record is its fields'
/// zeros, a variant its first constructor whose payload has one; `None` when
/// the type has no spellable zero (a fn value, an unresolved type).
fn zero_of(ty: &Ty, decls: &[IrTypeDecl], depth: u32) -> Option<IrExpr> {
    if depth > 8 {
        return None;
    }
    let kind = match ty {
        Ty::Int => IrExprKind::LitInt { value: 0 },
        Ty::Float => IrExprKind::LitFloat { value: 0.0 },
        Ty::Bool => IrExprKind::LitBool { value: false },
        Ty::String => IrExprKind::LitStr { value: String::new() },
        Ty::Unit => IrExprKind::Unit,
        Ty::Applied(C::List, _) => IrExprKind::List { elements: Vec::new() },
        Ty::Applied(C::Option, _) => IrExprKind::OptionNone,
        Ty::Applied(C::Map, _) => IrExprKind::EmptyMap,
        Ty::Applied(C::Result, a) => IrExprKind::ResultOk { expr: Box::new(zero_of(a.first()?, decls, depth + 1)?) },
        Ty::Tuple(ts) => IrExprKind::Tuple { elements: zeros_of(ts, decls, depth)? },
        Ty::Named(name, args) if args.is_empty() => return named_zero(ty, *name, decls, depth),
        _ => return None,
    };
    Some(IrExpr { kind, ty: ty.clone(), span: None, def_id: None })
}

fn zeros_of(ts: &[Ty], decls: &[IrTypeDecl], depth: u32) -> Option<Vec<IrExpr>> {
    ts.iter().map(|t| zero_of(t, decls, depth + 1)).collect()
}

/// The zero of a declared (non-generic) record, variant or alias.
fn named_zero(ty: &Ty, name: almide_base::intern::Sym, decls: &[IrTypeDecl], depth: u32) -> Option<IrExpr> {
    let decl = decls.iter().find(|d| d.name == name && d.generics.as_ref().is_none_or(|g| g.is_empty()))?;
    let kind = match &decl.kind {
        IrTypeDeclKind::Alias { target } => return zero_of(target, decls, depth + 1),
        IrTypeDeclKind::Record { fields } => {
            let fields = fields.iter().map(|f| Some((f.name, zero_of(&f.ty, decls, depth + 1)?))).collect::<Option<Vec<_>>>()?;
            IrExprKind::Record { name: None, fields }
        }
        IrTypeDeclKind::Variant { cases, .. } => {
            let (ctor, args) = cases.iter().find_map(|c| match &c.kind {
                IrVariantKind::Unit => Some((c.name, Vec::new())),
                IrVariantKind::Tuple { fields } => Some((c.name, zeros_of(fields, decls, depth)?)),
                IrVariantKind::Record { .. } => None,
            })?;
            IrExprKind::Call { target: CallTarget::Named { name: ctor }, args, type_args: Vec::new() }
        }
    };
    Some(IrExpr { kind, ty: ty.clone(), span: None, def_id: None })
}
