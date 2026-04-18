//! Type Concretization pass: sync every IrExpr.ty with its authoritative
//! source (VarTable for Var, parent context for TupleIndex/Member/BinOp, etc.)
//! so that downstream emit code can trust `expr.ty` and never needs to
//! re-derive types.
//!
//! This is Phase 1 of roadmap item #4 (codegen-ideal-form). Before this pass,
//! type resolution is scattered across 5+ locations:
//! - `LambdaTypeResolve` (top-down lambda param resolution)
//! - `emit_wasm/closures::resolve_expr_ty` (emit-time fallback)
//! - `emit_wasm/collections::emit_tuple_index` (VarTable priority)
//! - `emit_wasm/calls_list_helpers::resolve_list_elem` (list elem type)
//! - `has_deep_unresolved` checks duplicated in multiple files
//!
//! After this pass: every reachable IrExpr.ty is concrete (no Unknown / no
//! TypeVar / no nested TypeVar in Tuple/Applied/Fn). The emit layer can
//! read `expr.ty` directly.
//!
//! ## Approach
//!
//! Bottom-up (post-order). Resolve children first, then resolve self from
//! children's (now concrete) types. Uses structural reasoning:
//! - `Var { id }`          → `VarTable.get(id).ty`
//! - `TupleIndex { .. }`   → `object.ty` must be `Tuple`, pick element
//! - `BinOp { op, .. }`    → `op.result_ty()` or operand type
//! - `Member { .. }`       → object's record field type
//! - `Block { tail, .. }`  → tail's type
//! - `If { then, .. }`     → then branch type
//! - `Match { arms, .. }`  → first arm's body type
//! - Lambda / Call         → rely on existing annotations
//!
//! ## Not goals
//!
//! - Type inference (frontend's job)
//! - Monomorphization (optimize's job)
//! - Postcondition enforcement — if there's a node we can't concretize,
//!   we leave it alone. The original `.ty` remains (may still be Unknown).
//!   Emit can still fall back, but for all common patterns this pass makes
//!   emit's job trivial.

use almide_ir::*;
use almide_ir::visit::{walk_expr, walk_stmt};
use almide_ir::visit_mut::{walk_expr_mut, walk_stmt_mut};
use almide_lang::types::Ty;
use super::pass::{NanoPass, PassResult, Postcondition, Target};

#[derive(Debug)]
pub struct ConcretizeTypesPass;

impl NanoPass for ConcretizeTypesPass {
    fn name(&self) -> &str { "ConcretizeTypes" }

    fn targets(&self) -> Option<Vec<Target>> {
        // Useful for all targets. WASM benefits most because its emit
        // layer has extensive runtime type lookups, but Rust also wins.
        None
    }

    fn depends_on(&self) -> Vec<&'static str> {
        vec!["LambdaTypeResolve"]
    }

    fn postconditions(&self) -> Vec<Postcondition> {
        // S2 (v0.14.7-phase3.1): audit runs on every build; violations are
        // printed by the harness as `[POSTCONDITION VIOLATION] ...` and
        // escalate to a panic under `ALMIDE_CHECK_IR=1`. spec/ runs clean
        // on Rust at default + ALMIDE_CHECK_IR=1. WASM target on
        // ALMIDE_CHECK_IR=1 still trips on lifted-lambda TypeVar residue
        // produced by ClosureConversion (the second `ConcretizeTypes`
        // pass cannot fully recover the lambda param type from VarTable
        // when the source generic was already specialized away). Closing
        // that gap is part of S3 (pass_resolve_calls Phase 1b-c) — see
        // codegen-ideal-form.md §Phase 3 Arc.
        vec![Postcondition::Custom(audit_remaining_unresolved)]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Build a symbol table of (module, func) -> ret_ty for all user
        // module functions + top-level functions. This lets us resolve Call
        // return types without deferring to emit-time guessing.
        let symbols = build_symbol_table(&program);

        // Take var_table out of program so we can mutate it while also
        // mutating program.functions. Back-propagation (below) updates
        // VarTable entries for lambda accumulator params and match-pattern
        // bindings; downstream passes expect the updates to persist.
        let mut prog_vt = std::mem::take(&mut program.var_table);
        for func in &mut program.functions {
            let ret = func.ret_ty.clone();
            concretize_expr(&mut func.body, &mut prog_vt, &symbols, &ret);
        }
        for tl in &mut program.top_lets {
            concretize_expr(&mut tl.value, &mut prog_vt, &symbols, &Ty::Unknown);
        }
        program.var_table = prog_vt;

        for module in &mut program.modules {
            let mut mod_vt = std::mem::take(&mut module.var_table);
            for func in &mut module.functions {
                let ret = func.ret_ty.clone();
                concretize_expr(&mut func.body, &mut mod_vt, &symbols, &ret);
            }
            for tl in &mut module.top_lets {
                concretize_expr(&mut tl.value, &mut mod_vt, &symbols, &Ty::Unknown);
            }
            module.var_table = mod_vt;
        }
        PassResult { program, changed: true }
    }
}

// ── Symbol table ────────────────────────────────────────────────────

struct SymbolTable {
    /// (module_name, func_name) → return type
    /// "" as module means top-level (for CallTarget::Named).
    sigs: std::collections::HashMap<(String, String), Ty>,
    /// Record and record-payload variant case field types.
    /// Keyed by the record / case name (matches `IrExprKind::Record.name`).
    /// Used to push an expected element / payload type down into empty
    /// list / map literals whose own inference left them `Unknown`.
    record_fields: std::collections::HashMap<String, Vec<(almide_base::intern::Sym, Ty)>>,
}

impl SymbolTable {
    fn lookup_module(&self, module: &str, func: &str) -> Option<&Ty> {
        self.sigs.get(&(module.to_string(), func.to_string()))
    }
    fn lookup_named(&self, func: &str) -> Option<&Ty> {
        self.sigs.get(&(String::new(), func.to_string()))
    }
    fn lookup_field(&self, record: &str, field: &str) -> Option<&Ty> {
        let fs = self.record_fields.get(record)?;
        fs.iter().find(|(n, _)| n.as_str() == field).map(|(_, t)| t)
    }
}

fn build_symbol_table(program: &IrProgram) -> SymbolTable {
    let mut sigs = std::collections::HashMap::new();
    // Top-level functions (Named call targets)
    for func in &program.functions {
        if !func.ret_ty.has_unresolved_deep() {
            sigs.insert((String::new(), func.name.to_string()), func.ret_ty.clone());
        }
    }
    // Module functions (Module call targets)
    for module in &program.modules {
        let mname = module.name.to_string();
        for func in &module.functions {
            if func.is_test { continue; }
            if !func.ret_ty.has_unresolved_deep() {
                sigs.insert((mname.clone(), func.name.to_string()), func.ret_ty.clone());
            }
        }
    }
    let mut record_fields = std::collections::HashMap::new();
    for decl in &program.type_decls {
        match &decl.kind {
            almide_ir::IrTypeDeclKind::Record { fields } => {
                let fs: Vec<_> = fields.iter()
                    .map(|f| (f.name, f.ty.clone()))
                    .collect();
                record_fields.insert(decl.name.to_string(), fs);
            }
            almide_ir::IrTypeDeclKind::Variant { cases, .. } => {
                for case in cases {
                    if let almide_ir::IrVariantKind::Record { fields } = &case.kind {
                        let v: Vec<_> = fields.iter()
                            .map(|f| (f.name, f.ty.clone()))
                            .collect();
                        record_fields.insert(case.name.to_string(), v);
                    }
                }
            }
            _ => {}
        }
    }
    SymbolTable { sigs, record_fields }
}

/// For `ResultErr(payload)` with `ty = Result[Unknown, E]` inside an
/// effect fn whose ret_ty was lifted to `Result[T, String]`, fill the
/// Unknown Ok slot with `T`. The err-channel type stays whatever the
/// inner expression produced so `err("msg")` / `err(custom_err)` both
/// work. Returns `None` when the enclosing fn isn't a lifted Result
/// or the inner doesn't have an Err ty yet.
fn infer_err_ty_from_enclosing(enclosing_ret: &Ty, inner_ty: &Ty) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId;
    let ok_ty = match enclosing_ret {
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2
            && !args[0].has_unresolved_deep() =>
        {
            args[0].clone()
        }
        _ => return None,
    };
    // Pick the Err type: prefer the inner's concrete ty, fall back to
    // the enclosing fn's Err type.
    let err_ty = if !inner_ty.has_unresolved_deep() {
        inner_ty.clone()
    } else if let Ty::Applied(TypeConstructorId::Result, args) = enclosing_ret {
        args[1].clone()
    } else {
        return None;
    };
    Some(Ty::Applied(TypeConstructorId::Result, vec![ok_ty, err_ty]))
}

/// Push an expected type into an expression whose own inference left it
/// `Unknown`. Narrow by design: the target is `Applied(List, [Unknown])`
/// (the empty-list literal case) and the expected type fills the element
/// slot. Other shapes could be added as specific gaps surface, but kept
/// out for now so the audit keeps teeth around shapes we don't fully
/// understand.
fn propagate_expected_ty(expr: &mut IrExpr, expected: &Ty) {
    use almide_lang::types::constructor::TypeConstructorId;
    match (&expr.ty, expected) {
        (Ty::Applied(TypeConstructorId::List, args),
         Ty::Applied(TypeConstructorId::List, exp_args))
            if args.len() == 1 && exp_args.len() == 1
                && args[0].has_unresolved_deep()
                && !exp_args[0].has_unresolved_deep() =>
        {
            expr.ty = expected.clone();
            // Tighten the List literal's declared element type too so
            // downstream consumers (e.g. `emit_wasm::values::byte_size`)
            // see the resolved shape.
            if let IrExprKind::List { elements } = &mut expr.kind {
                if elements.is_empty() {
                    // nothing to rewrite inside — ty was the only carrier
                }
            }
        }
        _ => {}
    }
}

// ── Generic back-propagation helpers ────────────────────────────────

/// Shape-aware merge: returns the most concrete type when `a` and `b`
/// share a shape but differ in `Unknown` slots. Returns `None` when the
/// shapes disagree (can't safely merge). Leaves TypeVar alone since the
/// pre-pass is already expected to have substituted generics.
fn merge_more_concrete(a: &Ty, b: &Ty) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId;
    let _ = TypeConstructorId::List; // silence unused warning in some builds
    match (a, b) {
        (Ty::Unknown, other) | (other, Ty::Unknown) => Some(other.clone()),
        (Ty::Applied(c1, a1), Ty::Applied(c2, a2)) if c1 == c2 && a1.len() == a2.len() => {
            let merged: Option<Vec<Ty>> = a1.iter().zip(a2.iter())
                .map(|(x, y)| merge_more_concrete(x, y))
                .collect();
            merged.map(|m| Ty::Applied(c1.clone(), m))
        }
        (Ty::Tuple(e1), Ty::Tuple(e2)) if e1.len() == e2.len() => {
            let merged: Option<Vec<Ty>> = e1.iter().zip(e2.iter())
                .map(|(x, y)| merge_more_concrete(x, y))
                .collect();
            merged.map(Ty::Tuple)
        }
        (Ty::Fn { params: p1, ret: r1 }, Ty::Fn { params: p2, ret: r2 })
            if p1.len() == p2.len() =>
        {
            let merged_params: Option<Vec<Ty>> = p1.iter().zip(p2.iter())
                .map(|(x, y)| merge_more_concrete(x, y))
                .collect();
            let merged_ret = merge_more_concrete(r1, r2);
            match (merged_params, merged_ret) {
                (Some(ps), Some(r)) => Some(Ty::Fn { params: ps, ret: Box::new(r) }),
                _ => None,
            }
        }
        (x, y) if x == y => Some(x.clone()),
        _ => None,
    }
}

/// Push `expected` down into `expr`, recursing into wrappers (OptionSome,
/// Result*, List, Tuple). Updates expr.ty and any matching sub-expressions
/// whose own types have unresolved slots compatible with `expected`.
fn propagate_ty_down(expr: &mut IrExpr, expected: &Ty) {
    use almide_lang::types::constructor::TypeConstructorId as TCI;
    if let Some(merged) = merge_more_concrete(&expr.ty, expected) {
        expr.ty = merged;
    }
    match (&mut expr.kind, expected) {
        (IrExprKind::OptionSome { expr: inner }, Ty::Applied(TCI::Option, args))
            if args.len() == 1 =>
        {
            propagate_ty_down(inner, &args[0]);
        }
        (IrExprKind::ResultOk { expr: inner }, Ty::Applied(TCI::Result, args))
            if !args.is_empty() =>
        {
            propagate_ty_down(inner, &args[0]);
        }
        (IrExprKind::ResultErr { expr: inner }, Ty::Applied(TCI::Result, args))
            if args.len() >= 2 =>
        {
            propagate_ty_down(inner, &args[1]);
        }
        (IrExprKind::List { elements }, Ty::Applied(TCI::List, args))
            if args.len() == 1 =>
        {
            for e in elements.iter_mut() { propagate_ty_down(e, &args[0]); }
        }
        (IrExprKind::Tuple { elements }, Ty::Tuple(ts)) if elements.len() == ts.len() => {
            for (e, t) in elements.iter_mut().zip(ts.iter()) {
                propagate_ty_down(e, t);
            }
        }
        (IrExprKind::If { then, else_, .. }, _) => {
            propagate_ty_down(then, expected);
            propagate_ty_down(else_, expected);
        }
        (IrExprKind::Block { expr: Some(tail), .. }, _) => {
            propagate_ty_down(tail, expected);
        }
        (IrExprKind::Match { arms, .. }, _) => {
            for arm in arms.iter_mut() {
                propagate_ty_down(&mut arm.body, expected);
            }
        }
        _ => {}
    }
}

/// Propagate a subject type into a match pattern, updating `Bind` pattern
/// ty + the matching VarTable entry. Supports Some/Ok/Err/Tuple destructuring
/// (the shapes that actually surface in spec/).
fn propagate_pattern_ty(pat: &mut IrPattern, subj_ty: &Ty, vt: &mut VarTable) {
    use almide_lang::types::constructor::TypeConstructorId as TCI;
    match (pat, subj_ty) {
        (IrPattern::Bind { var, ty }, t) => {
            if ty.has_unresolved_deep() && !t.has_unresolved_deep() {
                *ty = t.clone();
                if (var.0 as usize) < vt.len() {
                    vt.entries[var.0 as usize].ty = t.clone();
                }
            }
        }
        (IrPattern::Some { inner }, Ty::Applied(TCI::Option, args)) if args.len() == 1 => {
            propagate_pattern_ty(inner, &args[0], vt);
        }
        (IrPattern::Ok { inner }, Ty::Applied(TCI::Result, args)) if !args.is_empty() => {
            propagate_pattern_ty(inner, &args[0], vt);
        }
        (IrPattern::Err { inner }, Ty::Applied(TCI::Result, args)) if args.len() >= 2 => {
            propagate_pattern_ty(inner, &args[1], vt);
        }
        (IrPattern::Tuple { elements }, Ty::Tuple(ts)) if elements.len() == ts.len() => {
            for (e, t) in elements.iter_mut().zip(ts.iter()) {
                propagate_pattern_ty(e, t, vt);
            }
        }
        _ => {}
    }
}

fn is_fold_like_call(expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func }, .. } => {
            module.as_str() == "list" && matches!(func.as_str(), "fold" | "scan")
        }
        _ => false,
    }
}

/// For `list.fold(xs, init, f)` where `f: (acc, t) -> acc`: the accumulator
/// type `A` has two sources — `init.ty` and `f.body.ty` — which must agree.
/// Pick the most concrete form available, then push it back into the init
/// sub-expression, the lambda's acc parameter (IR annotation + VarTable),
/// the Ty::Fn wrapper, and the Call's own ty. Returns true when changes
/// were made.
fn back_propagate_fold_acc(expr: &mut IrExpr, vt: &mut VarTable) -> bool {
    let args = match &mut expr.kind {
        IrExprKind::Call { args, .. } => args,
        _ => return false,
    };
    if args.len() < 3 { return false; }

    let body_ty = match &args[2].kind {
        IrExprKind::Lambda { body, .. } => body.ty.clone(),
        _ => return false,
    };
    let init_ty = args[1].ty.clone();

    // Accumulator type: merge init and body, picking the most concrete
    // shape when both are known and compatible. Fall back to whichever is
    // concrete when only one side has a type.
    let acc_ty = if !init_ty.has_unresolved_deep() && !body_ty.has_unresolved_deep() {
        merge_more_concrete(&init_ty, &body_ty)
    } else if !init_ty.has_unresolved_deep() {
        Some(init_ty.clone())
    } else if !body_ty.has_unresolved_deep() {
        Some(body_ty.clone())
    } else {
        None
    };
    let Some(acc_ty) = acc_ty else { return false; };

    let mut changed = false;

    // Push acc_ty into the init sub-expression when init has weaker shape
    if init_ty != acc_ty {
        propagate_ty_down(&mut args[1], &acc_ty);
        changed = true;
    }

    // Update lambda's acc param (IR + VarTable) and the Ty::Fn wrapper
    if let IrExprKind::Lambda { params, .. } = &mut args[2].kind {
        if let Some((vid, pty)) = params.get_mut(0) {
            if pty.has_unresolved_deep() {
                *pty = acc_ty.clone();
                if (vid.0 as usize) < vt.len() {
                    vt.entries[vid.0 as usize].ty = acc_ty.clone();
                }
                changed = true;
            }
        }
    }
    if let Ty::Fn { params: ps, ret } = &mut args[2].ty {
        if let Some(p0) = ps.get_mut(0) {
            if p0.has_unresolved_deep() {
                *p0 = acc_ty.clone();
                changed = true;
            }
        }
        if ret.has_unresolved_deep() {
            **ret = acc_ty.clone();
            changed = true;
        }
    }

    // Update Call's own ty if it's still unresolved
    if expr.ty.has_unresolved_deep() {
        expr.ty = acc_ty;
        changed = true;
    }
    changed
}

// ── Core walker ────────────────────────────────────────────────────

fn concretize_expr(expr: &mut IrExpr, vt: &mut VarTable, symbols: &SymbolTable, enclosing_ret: &Ty) {
    let mut c = Concretizer { vt, symbols, enclosing_ret };
    c.visit_expr_mut(expr);
}

struct Concretizer<'a> {
    vt: &'a mut VarTable,
    symbols: &'a SymbolTable,
    /// Return type of the enclosing IrFunction (post-`ResultPropagation`
    /// lift when applicable). Used to fill in the `Ok` slot of a
    /// `ResultErr` whose payload was written without the Ok type the
    /// checker could infer (`guard x else err(...)!` style).
    enclosing_ret: &'a Ty,
}

impl<'a> IrMutVisitor for Concretizer<'a> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        // Custom Match handling: propagate subject ty into pattern bindings
        // (updating both the pattern's declared ty and the VarTable entry)
        // BEFORE visiting arm bodies, so Var references to pattern-bound
        // names pick up the refreshed ty during the bottom-up walk.
        if let IrExprKind::Match { subject, arms } = &mut expr.kind {
            self.visit_expr_mut(subject);
            let sty = subject.ty.clone();
            if !sty.has_unresolved_deep() {
                for arm in arms.iter_mut() {
                    propagate_pattern_ty(&mut arm.pattern, &sty, self.vt);
                }
            }
            for arm in arms.iter_mut() {
                if let Some(g) = &mut arm.guard { self.visit_expr_mut(g); }
                self.visit_expr_mut(&mut arm.body);
            }
            // After arms are fully resolved, push any concrete arm body ty
            // into sibling arms whose body is an unresolved shape wrapper
            // (e.g. `none => none` has body ty Option[Unknown] but the
            // sibling `some(...)` arm resolves to Option[List[String]]).
            let concrete_arm_ty = arms.iter().find_map(|arm| {
                if !arm.body.ty.has_unresolved_deep() { Some(arm.body.ty.clone()) } else { None }
            });
            if let Some(cty) = concrete_arm_ty {
                for arm in arms.iter_mut() {
                    if arm.body.ty.has_unresolved_deep() {
                        propagate_ty_down(&mut arm.body, &cty);
                    }
                }
            }
            // Resolve the Match node itself
            if expr.ty.has_unresolved_deep() {
                if let Some(ty) = resolve_node_ty(expr, self.vt, self.symbols) {
                    expr.ty = ty;
                }
            }
            return;
        }

        // Recurse into children FIRST (bottom-up) so nested types are
        // concrete before we use them here.
        walk_expr_mut(self, expr);

        // Rewrite BinOp when operand types disagree with the op kind.
        // Type checker may have picked `AddInt` for polymorphic code that
        // later specialized to Float (e.g. via list element type). Without
        // this fix emit generates i64.add on f64 operands.
        if let IrExprKind::BinOp { op, left, right } = &mut expr.kind {
            if let Some(new_op) = reconcile_binop(*op, &left.ty, &right.ty) {
                *op = new_op;
            }
        }

        // Effect-fn `guard` / `?` paths can leave a `ResultErr` with
        // `Ok = Unknown` when the error value is the only thing the
        // checker can pin down (`guard x else err("msg")!`). The Ok
        // slot is the enclosing fn's return Ok type — after
        // ResultPropagation has lifted it to `Result[T, String]`, we
        // know `T` precisely.
        if let IrExprKind::ResultErr { expr: inner } = &mut expr.kind {
            if expr.ty.has_unresolved_deep() {
                if let Some(fixed) = infer_err_ty_from_enclosing(self.enclosing_ret, &inner.ty) {
                    expr.ty = fixed;
                }
            }
        }
        // Record literal construction: push the declared field types from
        // the registered type down into field value expressions whose own
        // inference left them unresolved (typically `Applied(List,
        // [Unknown])` for a field defaulted to `[]`). The checker sees
        // `items: []` and can only type it `List[Unknown]`; we know from
        // the record decl that `items: List[Int]`, so substitute.
        if let IrExprKind::Record { name: Some(name), fields } = &mut expr.kind {
            let rname = name.to_string();
            for (fname, fvalue) in fields.iter_mut() {
                if fvalue.ty.has_unresolved_deep() {
                    if let Some(expected) = self.symbols.lookup_field(&rname, fname.as_str()) {
                        if !expected.has_unresolved_deep() {
                            propagate_expected_ty(fvalue, expected);
                        }
                    }
                }
            }
        }

        // Generic-accumulator back-propagation for `list.fold` / `list.scan`:
        // both `init` arg and lambda `body.ty` represent the accumulator A.
        // After the bottom-up walk, body.ty may be strictly more concrete
        // than init.ty (because init started from a literal like `some([])`
        // whose empty list has element type Unknown). Merge, push the
        // merged shape back into init's sub-expressions, and update the
        // lambda's acc param + VarTable so arm Var refs refresh on the
        // re-visit below.
        if is_fold_like_call(expr) {
            if back_propagate_fold_acc(expr, self.vt) {
                // Re-visit the lambda body so pattern bindings and Var
                // references pick up the refreshed acc type.
                if let IrExprKind::Call { args, .. } = &mut expr.kind {
                    if let Some(lambda) = args.get_mut(2) {
                        if let IrExprKind::Lambda { body, .. } = &mut lambda.kind {
                            self.visit_expr_mut(body);
                        }
                    }
                }
            }
        }

        // Now resolve this node's type from child types + VarTable + symbols.
        if (expr.ty).has_unresolved_deep() {
            if let Some(ty) = resolve_node_ty(expr, self.vt, self.symbols) {
                expr.ty = ty;
            }
        }
    }

    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        walk_stmt_mut(self, stmt);
        // Sync Bind { ty } with value.ty when we now know the value type
        if let IrStmtKind::Bind { ty, value, .. } = &mut stmt.kind {
            if ty.has_unresolved_deep() && !(value.ty).has_unresolved_deep() {
                *ty = value.ty.clone();
            }
        }
    }
}

// ── Resolution logic ───────────────────────────────────────────────

fn resolve_node_ty(expr: &IrExpr, vt: &VarTable, symbols: &SymbolTable) -> Option<Ty> {
    match &expr.kind {
        IrExprKind::Var { id } => {
            let vt_ty = &vt.get(*id).ty;
            if !vt_ty.has_unresolved_deep() { Some(vt_ty.clone()) } else { None }
        }
        IrExprKind::EnvLoad { env_var, .. } => {
            let vt_ty = &vt.get(*env_var).ty;
            if !vt_ty.has_unresolved_deep() { Some(vt_ty.clone()) } else { None }
        }
        IrExprKind::TupleIndex { object, index } => {
            let obj_ty = effective_ty(object, vt);
            if let Ty::Tuple(elems) = &obj_ty {
                elems.get(*index).cloned().filter(|t| !t.has_unresolved_deep())
            } else { None }
        }
        IrExprKind::Member { object, field } => {
            let obj_ty = effective_ty(object, vt);
            match &obj_ty {
                Ty::Record { fields } | Ty::OpenRecord { fields } => {
                    fields.iter()
                        .find(|(n, _)| n == field.as_str())
                        .map(|(_, t)| t.clone())
                        .filter(|t| !t.has_unresolved_deep())
                }
                _ => None,
            }
        }
        IrExprKind::BinOp { op, left, right } => {
            op.result_ty().or_else(|| {
                if !(left.ty).has_unresolved_deep() { Some(left.ty.clone()) }
                else if !(right.ty).has_unresolved_deep() { Some(right.ty.clone()) }
                else { None }
            })
        }
        IrExprKind::UnOp { operand, .. } => {
            // Most UnOps (Neg, Not, Minus) preserve operand type
            if !(operand.ty).has_unresolved_deep() { Some(operand.ty.clone()) } else { None }
        }
        IrExprKind::Block { expr: Some(tail), .. } => {
            if !(tail.ty).has_unresolved_deep() { Some(tail.ty.clone()) } else { None }
        }
        IrExprKind::If { then, else_, .. } => {
            if !(then.ty).has_unresolved_deep() { Some(then.ty.clone()) }
            else if !(else_.ty).has_unresolved_deep() { Some(else_.ty.clone()) }
            else { None }
        }
        IrExprKind::Match { arms, .. } => {
            arms.iter()
                .find_map(|arm| if !(arm.body.ty).has_unresolved_deep() { Some(arm.body.ty.clone()) } else { None })
        }
        IrExprKind::Lambda { params, body, .. } => {
            // Build Ty::Fn from resolved params + body.ty (if concrete)
            let fparams: Vec<Ty> = params.iter().map(|(_, t)| t.clone()).collect();
            if fparams.iter().any(Ty::has_unresolved_deep) || (body.ty).has_unresolved_deep() {
                return None;
            }
            Some(Ty::Fn {
                params: fparams,
                ret: Box::new(body.ty.clone()),
            })
        }
        IrExprKind::IndexAccess { object, .. } => {
            // For List[T], result is T
            if let Ty::Applied(_, args) = &object.ty {
                args.first().cloned().filter(|t| !t.has_unresolved_deep())
            } else { None }
        }
        IrExprKind::List { elements } => {
            // List[T] where T = first element's type
            elements.first()
                .and_then(|e| if !(e.ty).has_unresolved_deep() { Some(e.ty.clone()) } else { None })
                .map(|t| Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, vec![t]))
        }
        IrExprKind::Tuple { elements } => {
            let elem_tys: Vec<Ty> = elements.iter().map(|e| e.ty.clone()).collect();
            if elem_tys.iter().any(Ty::has_unresolved_deep) { None }
            else { Some(Ty::Tuple(elem_tys)) }
        }
        IrExprKind::OptionSome { expr } => {
            // `some(x)` has type `Option[x.ty]`; recover when the type
            // checker left an `Option[Unknown]` placeholder (typical for
            // payloads built from pattern-bound names).
            if expr.ty.has_unresolved_deep() { None }
            else {
                Some(Ty::Applied(
                    almide_lang::types::constructor::TypeConstructorId::Option,
                    vec![expr.ty.clone()],
                ))
            }
        }
        IrExprKind::LitInt { .. } => Some(Ty::Int),
        IrExprKind::LitFloat { .. } => Some(Ty::Float),
        IrExprKind::LitBool { .. } => Some(Ty::Bool),
        IrExprKind::LitStr { .. } => Some(Ty::String),
        IrExprKind::Unit => Some(Ty::Unit),
        IrExprKind::Call { target, args, .. } => resolve_call_ret_ty(target, args, vt, symbols),
        _ => None,
    }
}

/// Resolve a Call's return type. Order:
/// 1. User-defined functions (top-level or module) — read from SymbolTable
/// 2. Generated stdlib signatures (from TOML) with TypeVar substitution
/// 3. Stdlib `list.*` polymorphic ops — compute from lambda return types
///
/// Returning `None` is fine; the emit layer still has its fallbacks.
fn resolve_call_ret_ty(
    target: &CallTarget,
    args: &[IrExpr],
    _vt: &VarTable,
    symbols: &SymbolTable,
) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId as TCI;

    // 1. User-defined function lookup
    match target {
        CallTarget::Module { module, func } => {
            if let Some(ret) = symbols.lookup_module(module.as_str(), func.as_str()) {
                if !ret.has_unresolved_deep() {
                    return Some(ret.clone());
                }
            }
        }
        CallTarget::Named { name } => {
            if let Some(ret) = symbols.lookup_named(name.as_str()) {
                if !ret.has_unresolved_deep() {
                    return Some(ret.clone());
                }
            }
        }
        _ => {}
    }

    let (module, func) = match target {
        CallTarget::Module { module, func } => (module.as_str(), func.as_str()),
        _ => return None,
    };

    // 2. Stdlib polymorphic list operations with lambda return types.
    //    These need the lambda argument's Fn::ret, which isn't expressible
    //    in the TOML template.
    if module != "list" { return None; }

    // Helper: get the element type of List[T] argument at given index.
    let list_elem = |idx: usize| -> Option<Ty> {
        let arg = args.get(idx)?;
        if let Ty::Applied(_, a) = &arg.ty {
            a.first().cloned().filter(|t| !t.has_unresolved_deep())
        } else { None }
    };
    // Helper: get a lambda argument's return type (if it's a concrete Fn).
    let lambda_ret = |idx: usize| -> Option<Ty> {
        let arg = args.get(idx)?;
        if let Ty::Fn { ret, .. } = &arg.ty {
            if !ret.has_unresolved_deep() { Some((**ret).clone()) } else { None }
        } else { None }
    };
    // Helper: wrap in List
    let list_of = |t: Ty| Ty::Applied(TCI::List, vec![t]);

    match func {
        "map" | "filter_map" => {
            // map(list, f) -> List[ret of f]
            lambda_ret(1).map(list_of)
        }
        "filter" | "take_while" | "drop_while" | "unique_by" | "dedup_by" => {
            // filter(list, pred) -> List[elem]
            list_elem(0).map(list_of)
        }
        "flat_map" => {
            // flat_map(list, f) -> List[inner_elem of f's return]
            if let Some(inner) = lambda_ret(1) {
                if let Ty::Applied(_, a) = &inner {
                    a.first().cloned().filter(|t| !t.has_unresolved_deep()).map(list_of)
                } else { None }
            } else { None }
        }
        "zip" => {
            // zip(xs, ys) -> List[(A, B)]
            let a = list_elem(0)?;
            let b = list_elem(1)?;
            Some(list_of(Ty::Tuple(vec![a, b])))
        }
        "fold" => {
            // fold(list, init, f) -> type of init
            let init = args.get(1)?;
            if !init.ty.has_unresolved_deep() { Some(init.ty.clone()) } else { None }
        }
        "reduce" | "min_by" | "max_by" => {
            // Option[elem]
            let elem = list_elem(0)?;
            Some(Ty::Applied(TCI::Option, vec![elem]))
        }
        "any" | "all" => Some(Ty::Bool),
        "count" => Some(Ty::Int),
        "len" => Some(Ty::Int),
        "first" | "last" | "find" => {
            // Option[elem]
            let elem = list_elem(0)?;
            Some(Ty::Applied(TCI::Option, vec![elem]))
        }
        "reverse" | "sort" | "sort_by" | "dedup" => list_elem(0).map(list_of),
        "concat" | "append" | "prepend" => list_elem(0).map(list_of),
        "slice" | "take" | "drop" | "chunks" => list_elem(0).map(list_of),
        "flatten" => {
            // flatten(List[List[T]]) -> List[T]
            list_elem(0).and_then(|inner| {
                if let Ty::Applied(_, a) = &inner {
                    a.first().cloned().filter(|t| !t.has_unresolved_deep()).map(list_of)
                } else { None }
            })
        }
        "partition" => {
            // (List[elem], List[elem])
            let elem = list_elem(0)?;
            let l = list_of(elem);
            Some(Ty::Tuple(vec![l.clone(), l]))
        }
        "enumerate" => {
            // List[(Int, elem)]
            let elem = list_elem(0)?;
            Some(list_of(Ty::Tuple(vec![Ty::Int, elem])))
        }
        _ => None,
    }
}

/// Get the effective type of an expression, preferring VarTable for Var/EnvLoad
/// over the potentially-stale expr.ty.
fn effective_ty(expr: &IrExpr, vt: &VarTable) -> Ty {
    match &expr.kind {
        IrExprKind::Var { id } => {
            let vt_ty = &vt.get(*id).ty;
            if !vt_ty.has_unresolved_deep() { vt_ty.clone() }
            else { expr.ty.clone() }
        }
        IrExprKind::EnvLoad { env_var, .. } => {
            let vt_ty = &vt.get(*env_var).ty;
            if !vt_ty.has_unresolved_deep() { vt_ty.clone() }
            else { expr.ty.clone() }
        }
        _ => expr.ty.clone(),
    }
}

// ── Canonical "is this type unresolved?" check ──────────────────────
//
// Replaces the three-way confusion between:
//   - `Ty::is_unresolved()`            — Unknown | TypeVar
//   - `Ty::is_unresolved_structural()` — Unknown | TypeVar | OpenRecord
//   - `has_deep_unresolved()`          — recursive into Tuple/Applied/Fn
// This pass uses the recursive form because `Tuple([Unknown, Float])`
// must count as unresolved even though `Tuple` itself isn't.

/// Reconcile a BinOp's variant with its operand types.
/// Returns Some(new_op) when we should rewrite. Only fixes Int↔Float
/// confusion; leaves other ops alone.
fn reconcile_binop(op: BinOp, lt: &Ty, rt: &Ty) -> Option<BinOp> {
    let operand_is_float = matches!(lt, Ty::Float) || matches!(rt, Ty::Float);
    let operand_is_int = matches!(lt, Ty::Int) && matches!(rt, Ty::Int);

    match op {
        BinOp::AddInt if operand_is_float => Some(BinOp::AddFloat),
        BinOp::SubInt if operand_is_float => Some(BinOp::SubFloat),
        BinOp::MulInt if operand_is_float => Some(BinOp::MulFloat),
        BinOp::DivInt if operand_is_float => Some(BinOp::DivFloat),
        BinOp::ModInt if operand_is_float => Some(BinOp::ModFloat),
        BinOp::PowInt if operand_is_float => Some(BinOp::PowFloat),

        BinOp::AddFloat if operand_is_int => Some(BinOp::AddInt),
        BinOp::SubFloat if operand_is_int => Some(BinOp::SubInt),
        BinOp::MulFloat if operand_is_int => Some(BinOp::MulInt),
        BinOp::DivFloat if operand_is_int => Some(BinOp::DivInt),
        BinOp::ModFloat if operand_is_int => Some(BinOp::ModInt),
        BinOp::PowFloat if operand_is_int => Some(BinOp::PowInt),

        _ => None,
    }
}


// ── Audit: count remaining unresolved types (diagnostic) ────────────

/// Postcondition audit: report any reachable IrExpr with an unresolved
/// type after this pass. Emitted only under ALMIDE_CHECK_IR=1. Treated
/// as informational — some cases (Break/Continue, error recovery paths)
/// legitimately have unresolved types.
fn audit_remaining_unresolved(program: &IrProgram) -> Vec<String> {
    struct Auditor {
        location: String,
        remaining: usize,
        samples: Vec<String>,
    }
    impl IrVisitor for Auditor {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if (expr.ty).has_unresolved_deep() {
                // Skip nodes that legitimately have no runtime representation
                let skip = matches!(&expr.kind,
                    IrExprKind::Break | IrExprKind::Continue
                    | IrExprKind::Hole | IrExprKind::Todo { .. }
                    | IrExprKind::OptionNone
                    | IrExprKind::EmptyMap
                )
                // Empty list literal `[]` whose element type could not be
                // pinned down by either upstream inference or
                // `propagate_expected_ty`. The stored element count is
                // zero, so every emit path — `Vec::<T>::new()` on Rust,
                // the 4-byte `[len=0]` header on WASM — produces the same
                // bytes regardless of `T`. Treating it as a violation
                // would force the audit to stay soft just to cover
                // `for _ in []` / `fan.map([], f)` style uses that have
                // no bearing on runtime behavior.
                || matches!(&expr.kind,
                    IrExprKind::List { elements } if elements.is_empty())
                // `Unwrap { ResultErr(...) }` in the `guard x else err(_)!`
                // idiom: the inner expression is a compile-time-known `err`
                // literal, so the "ok path" value the Unwrap would produce
                // is unreachable. The checker leaves the Unwrap's ty
                // `Unknown` for the same reason (`err()` doesn't fix the
                // Ok type). Leaving the Unwrap's ty concrete would
                // desynchronise the WASM emit — the "ok path" branch in
                // `emit_wasm/expressions.rs::Unwrap` would try to
                // `emit_load_at` an i64 value into a function whose sig
                // returns i32 (the Result pointer), tripping the
                // validator on otherwise-dead code. Accept the Unknown
                // here as "harmless: ok branch unreachable".
                || matches!(&expr.kind,
                    IrExprKind::Unwrap { expr: inner }
                        if matches!(inner.kind, IrExprKind::ResultErr { .. }))
                // `Block` whose sole tail is the same skipped `Unwrap`
                // pattern — the block is just the desugared `else
                // { err(...)! }` wrapper that lowering emits for
                // `guard` statements. `Block.ty` mirrors `tail.ty`, so
                // marking only the Unwrap would leave the outer Block
                // as a spurious violation.
                || matches!(&expr.kind,
                    IrExprKind::Block { stmts, expr: Some(tail) }
                        if stmts.is_empty()
                            && matches!(&tail.kind,
                                IrExprKind::Unwrap { expr: inner }
                                    if matches!(inner.kind, IrExprKind::ResultErr { .. })))
                // OpenRecord-typed expressions: an open-record bound
                // (`fn f(x: { name: String, .. })`) is a structural
                // constraint, not an inference failure. The Var node
                // for such a param trivially carries its declared
                // OpenRecord ty through monomorphization's `__Unknown`
                // fallback path. Emit handles OpenRecord via its
                // structural dispatch — no Unknown slot to fill.
                || matches!(&expr.ty, Ty::OpenRecord { .. });
                if !skip {
                    self.remaining += 1;
                    if self.samples.len() < 5 {
                        self.samples.push(format!("[{}] {:?} ty={:?}",
                            self.location,
                            kind_name(&expr.kind), expr.ty));
                    }
                }
            }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) {
            walk_stmt(self, stmt);
        }
    }
    fn kind_name(k: &IrExprKind) -> &'static str {
        match k {
            IrExprKind::LitInt { .. } => "LitInt",
            IrExprKind::LitFloat { .. } => "LitFloat",
            IrExprKind::LitStr { .. } => "LitStr",
            IrExprKind::LitBool { .. } => "LitBool",
            IrExprKind::Unit => "Unit",
            IrExprKind::Var { .. } => "Var",
            IrExprKind::FnRef { .. } => "FnRef",
            IrExprKind::BinOp { .. } => "BinOp",
            IrExprKind::UnOp { .. } => "UnOp",
            IrExprKind::If { .. } => "If",
            IrExprKind::Match { .. } => "Match",
            IrExprKind::Block { .. } => "Block",
            IrExprKind::Fan { .. } => "Fan",
            IrExprKind::ForIn { .. } => "ForIn",
            IrExprKind::While { .. } => "While",
            IrExprKind::Call { .. } => "Call",
            IrExprKind::TailCall { .. } => "TailCall",
            IrExprKind::List { .. } => "List",
            IrExprKind::MapLiteral { .. } => "MapLiteral",
            IrExprKind::Record { .. } => "Record",
            IrExprKind::SpreadRecord { .. } => "SpreadRecord",
            IrExprKind::Tuple { .. } => "Tuple",
            IrExprKind::Range { .. } => "Range",
            IrExprKind::Member { .. } => "Member",
            IrExprKind::TupleIndex { .. } => "TupleIndex",
            IrExprKind::IndexAccess { .. } => "IndexAccess",
            IrExprKind::MapAccess { .. } => "MapAccess",
            IrExprKind::Lambda { .. } => "Lambda",
            IrExprKind::ClosureCreate { .. } => "ClosureCreate",
            IrExprKind::EnvLoad { .. } => "EnvLoad",
            _ => "(other)",
        }
    }
    let mut a = Auditor { location: String::new(), remaining: 0, samples: Vec::new() };
    for f in &program.functions {
        a.location = format!("fn {}", f.name);
        a.visit_expr(&f.body);
    }
    for m in &program.modules {
        let mname = m.name.to_string();
        for f in &m.functions {
            a.location = format!("{}::{}", mname, f.name);
            a.visit_expr(&f.body);
        }
    }
    if a.remaining > 0 {
        vec![format!("[ConcretizeTypes] {} expressions remain with unresolved types. Samples: {}",
            a.remaining, a.samples.join(" | "))]
    } else {
        Vec::new()
    }
}
