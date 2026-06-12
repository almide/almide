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

use std::collections::HashMap;
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
        // S2 flip (v0.14.7-phase3.2): audit is a hard contract. Debug
        // builds panic on violation so CI and local dev never ship a
        // program with unresolved `IrExpr.ty`; release builds print the
        // diagnostic and keep compiling. Downstream passes (closure
        // conversion, WASM emit, stdlib dispatch) rely on non-Unknown
        // `expr.ty` unconditionally and no longer carry defensive
        // fallbacks. Residual WASM-target lifted-lambda TypeVars produced
        // by ClosureConversion are tracked separately in S3
        // (pass_resolve_calls Phase 1b-c) — see codegen-ideal-form.md
        // §Phase 3 Arc.
        vec![Postcondition::Custom(audit_remaining_unresolved)]
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        // Build type alias map: alias_name → underlying type.
        // mod type SafeHtml = String → aliases["SafeHtml"] = String
        // Erase aliases throughout the IR so downstream codegen never sees them.
        let mut aliases: HashMap<String, Ty> = HashMap::new();
        for td in program.type_decls.iter().chain(program.modules.iter().flat_map(|m| m.type_decls.iter())) {
            if let almide_ir::IrTypeDeclKind::Alias { target } = &td.kind {
                aliases.insert(td.name.to_string(), target.clone());
            }
        }
        // Erase aliases only for WASM target — Rust codegen handles newtypes natively.
        if !aliases.is_empty() && _target == Target::Wasm {
            erase_type_aliases(&mut program, &aliases);
        }

        let symbols = build_symbol_table(&program);

        // Take var_table out of program so we can mutate it while also
        // mutating program.functions. Back-propagation (below) updates
        // VarTable entries for lambda accumulator params and match-pattern
        // bindings; downstream passes expect the updates to persist.
        let mut prog_vt = std::mem::take(&mut program.var_table);

        // Phase 1: Resolve top_lets first so their types are available
        // when functions reference cross-module let values.
        for tl in &mut program.top_lets {
            concretize_expr(&mut tl.value, &mut prog_vt, &symbols, &Ty::Unknown);
            if !tl.value.ty.has_unresolved_deep() {
                if tl.ty.has_unresolved_deep() {
                    tl.ty = tl.value.ty.clone();
                }
                if (tl.var.0 as usize) < prog_vt.len()
                    && prog_vt.get(tl.var).ty.has_unresolved_deep()
                {
                    prog_vt.entries[tl.var.0 as usize].ty = tl.value.ty.clone();
                }
            }
        }
        for module in &mut program.modules {
            for tl in &mut module.top_lets {
                concretize_expr(&mut tl.value, &mut prog_vt, &symbols, &Ty::Unknown);
                if !tl.value.ty.has_unresolved_deep() {
                    if tl.ty.has_unresolved_deep() {
                        tl.ty = tl.value.ty.clone();
                    }
                    if (tl.var.0 as usize) < prog_vt.len()
                        && prog_vt.get(tl.var).ty.has_unresolved_deep()
                    {
                        prog_vt.entries[tl.var.0 as usize].ty = tl.value.ty.clone();
                    }
                }
            }
        }

        // Phase 1b: Propagate top_let types by name into VarTable entries
        // that are cross-module synthetic references (different VarId, same name).
        let mut top_let_types: std::collections::HashMap<String, Ty> = std::collections::HashMap::new();
        // The use-site synthetic Var carries the SCREAMING_CASE const
        // spelling (lower/expressions.rs `field.to_uppercase()`) while the
        // definition keeps the source name — bridge BOTH spellings, or a
        // lowercase module top-let never propagates (#502 fix C).
        let mut insert_both = |name: String, ty: &Ty, map: &mut std::collections::HashMap<String, Ty>| {
            let upper = name.to_uppercase();
            if upper != name { map.entry(upper).or_insert_with(|| ty.clone()); }
            map.insert(name, ty.clone());
        };
        for tl in &program.top_lets {
            if !tl.ty.has_unresolved_deep() {
                let name = prog_vt.get(tl.var).name.to_string();
                insert_both(name, &tl.ty, &mut top_let_types);
            }
        }
        for module in &program.modules {
            for tl in &module.top_lets {
                if !tl.ty.has_unresolved_deep() {
                    let name = prog_vt.get(tl.var).name.to_string();
                    insert_both(name, &tl.ty, &mut top_let_types);
                }
            }
        }
        if !top_let_types.is_empty() {
            for entry in &mut prog_vt.entries {
                if entry.ty.has_unresolved_deep() {
                    if let Some(ty) = top_let_types.get(entry.name.as_str()) {
                        entry.ty = ty.clone();
                    }
                }
            }
        }

        // Phase 2: Now resolve functions (which may reference cross-module lets
        // whose VarTable types are now populated).
        for func in &mut program.functions {
            let ret = func.ret_ty.clone();
            concretize_expr(&mut func.body, &mut prog_vt, &symbols, &ret);
        }
        for module in &mut program.modules {
            for func in &mut module.functions {
                let ret = func.ret_ty.clone();
                concretize_expr(&mut func.body, &mut prog_vt, &symbols, &ret);
            }
        }

        program.var_table = prog_vt;
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
        // Try exact name first, then scan all records for matching field
        let fs = self.record_fields.get(record).or_else(|| {
            // Cross-module type alias mismatch: Named("R") vs registered "Tween".
            // Fallback: find any record that has the requested field.
            self.record_fields.values().find(|fields| {
                fields.iter().any(|(n, _)| n.as_str() == field)
            })
        })?;
        fs.iter().find(|(n, _)| n.as_str() == field).map(|(_, t)| t)
    }
}

/// Erase type aliases throughout the IR. Replaces:
/// - `Ty::Named("Alias", _)` → underlying type
/// - `Call(Named("Alias"), [arg])` → `arg` (identity constructor)
/// - `Constructor { name: "Alias", args: [Bind(v)] }` → `Bind(v)` (identity unwrap)
/// This is the Rust `#[repr(transparent)]` / Haskell `newtype` approach.
fn erase_type_aliases(program: &mut IrProgram, aliases: &HashMap<String, Ty>) {
    use almide_ir::{IrExprKind, IrStmtKind, IrPattern};
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};

    fn erase_ty(ty: &mut Ty, aliases: &HashMap<String, Ty>) {
        if let Ty::Named(name, _) = ty {
            if let Some(target) = aliases.get(name.as_str()) {
                *ty = target.clone();
            }
        }
    }

    fn erase_expr(expr: &mut almide_ir::IrExpr, aliases: &HashMap<String, Ty>) {
        erase_ty(&mut expr.ty, aliases);
        match &mut expr.kind {
            // Constructor call: Alias(arg) → arg
            IrExprKind::Call { target: almide_ir::CallTarget::Named { name }, args, .. } => {
                if aliases.contains_key(name.as_str()) && args.len() == 1 {
                    let arg = args.remove(0);
                    *expr = arg;
                    erase_expr(expr, aliases);
                    return;
                }
                for a in args.iter_mut() { erase_expr(a, aliases); }
            }
            IrExprKind::Block { stmts, expr: tail } => {
                for s in stmts.iter_mut() { erase_stmt(s, aliases); }
                if let Some(t) = tail { erase_expr(t, aliases); }
            }
            IrExprKind::If { cond, then, else_ } => {
                erase_expr(cond, aliases);
                erase_expr(then, aliases);
                erase_expr(else_, aliases);
            }
            IrExprKind::Match { subject, arms } => {
                erase_expr(subject, aliases);
                for arm in arms.iter_mut() {
                    erase_pattern(&mut arm.pattern, aliases);
                    if let Some(g) = &mut arm.guard { erase_expr(g, aliases); }
                    erase_expr(&mut arm.body, aliases);
                }
            }
            IrExprKind::Call { args, .. } | IrExprKind::TailCall { args, .. } => {
                for a in args.iter_mut() { erase_expr(a, aliases); }
            }
            IrExprKind::RuntimeCall { args, .. } => {
                for a in args.iter_mut() { erase_expr(a, aliases); }
            }
            IrExprKind::Lambda { body, params, .. } => {
                for (_, t) in params.iter_mut() { erase_ty(t, aliases); }
                erase_expr(body, aliases);
            }
            IrExprKind::BinOp { left, right, .. } => {
                erase_expr(left, aliases);
                erase_expr(right, aliases);
            }
            IrExprKind::UnOp { operand, .. } => erase_expr(operand, aliases),
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
                for e in elements.iter_mut() { erase_expr(e, aliases); }
            }
            IrExprKind::Record { fields, .. } => {
                for (_, e) in fields.iter_mut() { erase_expr(e, aliases); }
            }
            IrExprKind::Member { object, .. } => erase_expr(object, aliases),
            IrExprKind::IndexAccess { object, index } => {
                erase_expr(object, aliases);
                erase_expr(index, aliases);
            }
            IrExprKind::ForIn { iterable, body, .. } => {
                erase_expr(iterable, aliases);
                for s in body.iter_mut() { erase_stmt(s, aliases); }
            }
            IrExprKind::While { cond, body } => {
                erase_expr(cond, aliases);
                for s in body.iter_mut() { erase_stmt(s, aliases); }
            }
            IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e }
            | IrExprKind::OptionSome { expr: e } | IrExprKind::Try { expr: e }
            | IrExprKind::Unwrap { expr: e } | IrExprKind::Clone { expr: e } => erase_expr(e, aliases),
            // Explicit-preserve (total-by-construction). The head of this
            // fn already ran `erase_ty(&mut expr.ty, ..)` for this node;
            // these variants intentionally do not descend further (exactly
            // the old `_ => {}` behaviour). Listing every remaining kind
            // makes a future IrExprKind variant a compile error here so a
            // new node carrying an aliasable subtree cannot be silently
            // skipped.
            IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
            | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. }
            | IrExprKind::Unit | IrExprKind::Var { .. } | IrExprKind::FnRef { .. }
            | IrExprKind::Fan { .. } | IrExprKind::Break | IrExprKind::Continue
            | IrExprKind::MapLiteral { .. } | IrExprKind::EmptyMap
            | IrExprKind::SpreadRecord { .. } | IrExprKind::Range { .. }
            | IrExprKind::TupleIndex { .. } | IrExprKind::MapAccess { .. }
            | IrExprKind::StringInterp { .. } | IrExprKind::OptionNone
            | IrExprKind::UnwrapOr { .. } | IrExprKind::ToOption { .. }
            | IrExprKind::OptionalChain { .. } | IrExprKind::Await { .. }
            | IrExprKind::Deref { .. } | IrExprKind::Borrow { .. }
            | IrExprKind::BoxNew { .. } | IrExprKind::RcWrap { .. }
            | IrExprKind::RustMacro { .. } | IrExprKind::ToVec { .. }
            | IrExprKind::RenderedCall { .. } | IrExprKind::InlineRust { .. }
            | IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }
            | IrExprKind::IterChain { .. } | IrExprKind::Hole
            | IrExprKind::Todo { .. } => {}
        }
    }

    fn erase_stmt(stmt: &mut almide_ir::IrStmt, aliases: &HashMap<String, Ty>) {
        match &mut stmt.kind {
            IrStmtKind::Bind { value, ty, .. } => {
                erase_ty(ty, aliases);
                erase_expr(value, aliases);
            }
            IrStmtKind::Assign { value, .. } => erase_expr(value, aliases),
            IrStmtKind::Expr { expr } => erase_expr(expr, aliases),
            IrStmtKind::BindDestructure { value, pattern, .. } => {
                erase_expr(value, aliases);
                erase_pattern(pattern, aliases);
            }
            // Explicit-preserve (total-by-construction). These statement
            // kinds carry no `Ty::Named` slot or aliasable subtree that
            // alias-erasure needs to touch (the assignable expressions
            // inside IndexAssign/MapInsert/FieldAssign/Guard contain only
            // values whose own types are erased when their enclosing
            // expression is visited). Zero behaviour change vs the old
            // `_ => {}`; listing every kind makes a new IrStmtKind a
            // compile error here.
            IrStmtKind::IndexAssign { .. } | IrStmtKind::MapInsert { .. }
            | IrStmtKind::FieldAssign { .. } | IrStmtKind::Guard { .. }
            | IrStmtKind::Comment { .. } | IrStmtKind::RcInc { .. }
            | IrStmtKind::RcDec { .. } | IrStmtKind::ListSwap { .. }
            | IrStmtKind::ListReverse { .. } | IrStmtKind::ListRotateLeft { .. }
            | IrStmtKind::ListCopySlice { .. } => {}
        }
    }

    fn erase_pattern(pat: &mut IrPattern, aliases: &HashMap<String, Ty>) {
        match pat {
            // Constructor("Alias", [Bind(v)]) → Bind(v)
            IrPattern::Constructor { name, args } => {
                if aliases.contains_key(name.as_str()) && args.len() == 1 {
                    *pat = args.remove(0);
                    return;
                }
            }
            IrPattern::Tuple { elements } => {
                for e in elements.iter_mut() { erase_pattern(e, aliases); }
            }
            IrPattern::Some { inner } => erase_pattern(inner, aliases),
            IrPattern::Ok { inner } | IrPattern::Err { inner } => erase_pattern(inner, aliases),
            _ => {}
        }
    }

    for func in &mut program.functions {
        erase_ty(&mut func.ret_ty, aliases);
        for p in &mut func.params { erase_ty(&mut p.ty, aliases); }
        erase_expr(&mut func.body, aliases);
    }
    for tl in &mut program.top_lets { erase_expr(&mut tl.value, aliases); }
    for module in &mut program.modules {
        for func in &mut module.functions {
            erase_ty(&mut func.ret_ty, aliases);
            for p in &mut func.params { erase_ty(&mut p.ty, aliases); }
            erase_expr(&mut func.body, aliases);
        }
    }
    // Also erase in VarTable
    for entry in &mut program.var_table.entries {
        erase_ty(&mut entry.ty, aliases);
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
    let mut register_type_decl = |decl: &almide_ir::IrTypeDecl| {
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
    };
    for decl in &program.type_decls {
        register_type_decl(decl);
    }
    for module in &program.modules {
        for decl in &module.type_decls {
            register_type_decl(decl);
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
    // Case 1: enclosing fn already returns Result[T, E] (post-ResultPropagation lift)
    if let Ty::Applied(TypeConstructorId::Result, args) = enclosing_ret {
        if args.len() == 2 && !args[0].has_unresolved_deep() {
            let ok_ty = args[0].clone();
            let err_ty = if !inner_ty.has_unresolved_deep() {
                inner_ty.clone()
            } else {
                args[1].clone()
            };
            return Some(Ty::Applied(TypeConstructorId::Result, vec![ok_ty, err_ty]));
        }
    }
    // Case 2: enclosing fn returns T (pre-lift, e.g. effect fn safe_div -> Int).
    // The Ok slot of err() should be T, and Err slot is String (effect fn convention).
    if !enclosing_ret.has_unresolved_deep() && *enclosing_ret != Ty::Unit {
        let ok_ty = enclosing_ret.clone();
        let err_ty = if !inner_ty.has_unresolved_deep() {
            inner_ty.clone()
        } else {
            Ty::String
        };
        return Some(Ty::Applied(TypeConstructorId::Result, vec![ok_ty, err_ty]));
    }
    None
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
        // Explicit-preserve (total-by-construction). The guarded arms above
        // handle the wrapper/branch shapes whose `expr.kind` and `expected`
        // line up; every other (kind, expected) pairing — including a
        // wrapper kind whose `expected` shape did NOT match its guard —
        // falls here and is a no-op, exactly as the old `_ => {}`. The merge
        // of `expr.ty` with `expected` already happened above, so there is
        // nothing further to push down. Wildcarding only the `expected`
        // slot keeps the fall-through identical while making the first
        // tuple element exhaustive: a new IrExprKind variant is a compile
        // error here. `If`/`Match` are NOT re-listed: their arms above are
        // unguarded, so they already cover every `expected` and re-listing
        // them would be an unreachable pattern.
        (IrExprKind::OptionSome { .. }, _)
        | (IrExprKind::ResultOk { .. }, _)
        | (IrExprKind::ResultErr { .. }, _)
        | (IrExprKind::List { .. }, _)
        | (IrExprKind::Tuple { .. }, _)
        | (IrExprKind::Block { .. }, _)
        | (IrExprKind::LitInt { .. }, _) | (IrExprKind::LitFloat { .. }, _)
        | (IrExprKind::LitStr { .. }, _) | (IrExprKind::LitBool { .. }, _)
        | (IrExprKind::Unit, _) | (IrExprKind::Var { .. }, _)
        | (IrExprKind::FnRef { .. }, _) | (IrExprKind::BinOp { .. }, _)
        | (IrExprKind::UnOp { .. }, _) | (IrExprKind::Fan { .. }, _)
        | (IrExprKind::ForIn { .. }, _) | (IrExprKind::While { .. }, _)
        | (IrExprKind::Break, _) | (IrExprKind::Continue, _)
        | (IrExprKind::Call { .. }, _) | (IrExprKind::TailCall { .. }, _)
        | (IrExprKind::RuntimeCall { .. }, _)
        | (IrExprKind::MapLiteral { .. }, _) | (IrExprKind::EmptyMap, _)
        | (IrExprKind::Record { .. }, _) | (IrExprKind::SpreadRecord { .. }, _)
        | (IrExprKind::Range { .. }, _) | (IrExprKind::Member { .. }, _)
        | (IrExprKind::TupleIndex { .. }, _) | (IrExprKind::IndexAccess { .. }, _)
        | (IrExprKind::MapAccess { .. }, _) | (IrExprKind::Lambda { .. }, _)
        | (IrExprKind::StringInterp { .. }, _) | (IrExprKind::OptionNone, _)
        | (IrExprKind::Try { .. }, _) | (IrExprKind::Unwrap { .. }, _)
        | (IrExprKind::UnwrapOr { .. }, _) | (IrExprKind::ToOption { .. }, _)
        | (IrExprKind::OptionalChain { .. }, _) | (IrExprKind::Await { .. }, _)
        | (IrExprKind::Clone { .. }, _) | (IrExprKind::Deref { .. }, _)
        | (IrExprKind::Borrow { .. }, _) | (IrExprKind::BoxNew { .. }, _)
        | (IrExprKind::RcWrap { .. }, _) | (IrExprKind::RustMacro { .. }, _)
        | (IrExprKind::ToVec { .. }, _) | (IrExprKind::RenderedCall { .. }, _)
        | (IrExprKind::InlineRust { .. }, _) | (IrExprKind::ClosureCreate { .. }, _)
        | (IrExprKind::EnvLoad { .. }, _) | (IrExprKind::IterChain { .. }, _)
        | (IrExprKind::Hole, _) | (IrExprKind::Todo { .. }, _) => {}
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
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
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

impl<'a> Concretizer<'a> {
    /// Resolve the empty-list argument element type of `map.from_list(arg)` /
    /// `set.from_list(arg)` from the expected Map/Set type `ret`, pinning the
    /// arg expression, any Borrow/Clone/Deref wrappers, and the ANF-temp's
    /// VarTable entry — the element only flows through the generic return, so
    /// the checker can leave it `List[(?K,?V)]` past the WASM gate (#625).
    fn pin_from_list_arg_elem(&mut self, ret: &Ty, value: &mut IrExpr) {
        use almide_lang::types::constructor::TypeConstructorId as TCI;
        if ret.has_unresolved_deep() { return; }
        // `map.from_list` canonicalizes to `map.from_entries`, and by the WASM
        // emit passes it is a `RuntimeCall` (`almide_rt_map_from_entries`), not a
        // `Module` call — match both forms for each module (set keeps `from_list`).
        let from_list_kind = |module: &str, func: &str| -> (bool, bool) {
            let fl = func == "from_list" || func == "from_entries";
            (module == "map" && fl, module == "set" && fl)
        };
        let (is_map, is_set) = match &value.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } =>
                from_list_kind(module.as_str(), func.as_str()),
            IrExprKind::RuntimeCall { symbol, .. } => {
                let s = symbol.as_str();
                (s.contains("map_from_entries") || s.contains("map_from_list"),
                 s.contains("set_from_entries") || s.contains("set_from_list"))
            }
            _ => return,
        };
        let elem = if is_map {
            match ret { Ty::Applied(TCI::Map, kv) if kv.len() == 2 => Ty::Tuple(vec![kv[0].clone(), kv[1].clone()]), _ => return }
        } else if is_set {
            match ret { Ty::Applied(TCI::Set, e) if e.len() == 1 => e[0].clone(), _ => return }
        } else { return };
        let list_ty = Ty::Applied(TCI::List, vec![elem]);
        let args = match &mut value.kind {
            IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. } => args,
            _ => return,
        };
        {
            if args.len() != 1 || !args[0].ty.has_unresolved_deep() { return; }
            propagate_expected_ty(&mut args[0], &list_ty);
            // Walk through wrappers to the Var, pinning each ty and the VarTable.
            let mut node = &mut args[0];
            loop {
                if node.ty.has_unresolved_deep() { node.ty = list_ty.clone(); }
                match &mut node.kind {
                    IrExprKind::Borrow { expr, .. } | IrExprKind::Clone { expr } | IrExprKind::Deref { expr } => node = expr,
                    IrExprKind::Var { id } => {
                        let i = id.0 as usize;
                        if i < self.vt.entries.len() && self.vt.entries[i].ty.has_unresolved_deep() {
                            self.vt.entries[i].ty = list_ty.clone();
                        }
                        break;
                    }
                    _ => break,
                }
            }
        }
    }
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

        // Resolve Unknown lambda params from body usage (e.g. `(a,b) => a + b` → Int)
        if let IrExprKind::Lambda { params, body, .. } = &mut expr.kind {
            let mut patched = false;
            for (var_id, var_ty) in params.iter_mut() {
                if matches!(var_ty, Ty::Unknown) {
                    if let Some(inferred) = infer_var_type_from_body(body, *var_id) {
                        *var_ty = inferred.clone();
                        self.vt.entries[var_id.0 as usize].ty = inferred;
                        patched = true;
                    }
                }
            }
            // Re-visit body to propagate patched param types into Var nodes
            if patched { walk_expr_mut(self, body); }
        }

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
        // #625: `map.from_list([])` / `set.from_list([])` — the empty-list
        // argument's element type is determined ONLY by the call's return
        // (`Map[K,V]` ← `List[(K,V)]`, `Set[E]` ← `List[E]`). The checker can
        // leave that arg `List[(?K,?V)]` (the K,V flow only through the generic
        // signature's return, not through any literal element), which would slip
        // past this gate on native but be refused by the WASM concretization
        // gate. Derive the arg element from the resolved return type here.
        // #625: `map.from_list([])` / `set.from_list([])` where the call is NOT
        // the direct value of an annotated binding — derive the empty arg's
        // element from the call's own (resolved) return type. The annotated-bind
        // case is handled more reliably in `visit_stmt_mut`.
        {
            let ret_ty = expr.ty.clone();
            self.pin_from_list_arg_elem(&ret_ty, expr);
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
                expr.ty = ty.clone();
                // Propagate: if this was IndexAccess and it resolved, the
                // parent Member can now resolve too. But we're bottom-up,
                // so Member visits AFTER this. Make sure we updated expr.ty.
            }
        }
        // Second chance for Member: debug why it fails
        if (expr.ty).has_unresolved_deep() {
            if let IrExprKind::Member { object, field } = &expr.kind {
                let obj_ty = effective_ty(object, self.vt);
                let resolved = match &obj_ty {
                    Ty::Record { fields } | Ty::OpenRecord { fields } => {
                        fields.iter().find(|(n, _)| n == field.as_str()).map(|(_, t)| t.clone())
                            .filter(|t| !t.has_unresolved_deep())
                    }
                    Ty::Named(name, _) => {
                        let r = self.symbols.lookup_field(name.as_str(), field.as_str());
                        r.filter(|t| !t.has_unresolved_deep()).cloned()
                    }
                    _ => {
                        None
                    }
                };
                if let Some(ty) = resolved {
                    expr.ty = ty;
                }
            }
        }
    }

    fn visit_stmt_mut(&mut self, stmt: &mut IrStmt) {
        walk_stmt_mut(self, stmt);
        // Sync Bind { ty } *and* the VarTable entry for the bound var with
        // value.ty when we now know the value type. Without the VarTable
        // sync, later Var references to the same binding (and the
        // post-pass audit reading VarTable directly) keep seeing Unknown.
        if let IrStmtKind::Bind { var, ty, value, .. } = &mut stmt.kind {
            if !(value.ty).has_unresolved_deep() {
                if ty.has_unresolved_deep() {
                    *ty = value.ty.clone();
                }
                if (var.0 as usize) < self.vt.len()
                    && self.vt.get(*var).ty.has_unresolved_deep()
                {
                    self.vt.entries[var.0 as usize].ty = value.ty.clone();
                }
            }
            // #625: `let m: Map[K,V] = map.from_list(arg)` / `set.from_list`.
            // The arg's element type flows ONLY through the generic call's
            // return, so the checker can leave it `List[(?K,?V)]`. The BIND's
            // declared type is the reliable source (the call's own ty may not
            // be resolved on every pass), so derive the arg element from it and
            // pin the arg (and its ANF-temp VarTable entry) before the gate.
            self.pin_from_list_arg_elem(ty, value);
        }
        // Destructuring let: `let (k, v) = pair`, `let some(x) = opt`, … The
        // checker can leave the bound pattern vars `Unknown` when the subject's
        // type resolved only after binding (e.g. `pair` is `list.zip(..)[i]`,
        // whose tuple element type ConcretizeTypes pins during THIS bottom-up
        // walk). Once `value.ty` is concrete, push it into the pattern bindings
        // and their VarTable entries — the same propagation `match` already gets
        // (via `propagate_pattern_ty` in the Match arm), now extended to the
        // statement form so later `Var` refs and the hard gate see concrete types
        // instead of a leftover `Unknown` (the `let (k, v) = pair` → `v: Unknown`
        // class).
        if let IrStmtKind::BindDestructure { pattern, value } = &mut stmt.kind {
            if !(value.ty).has_unresolved_deep() {
                propagate_pattern_ty(pattern, &value.ty, self.vt);
            }
        }
    }
}

// ── Resolution logic ───────────────────────────────────────────────

/// Infer a lambda param's type by scanning how it's used in the body.
/// e.g., `(a, b) => a + b` where body is BinOp::AddInt → a: Int, b: Int
fn binop_operand_type(op: &BinOp, left: &IrExpr, right: &IrExpr, var: VarId) -> Option<Ty> {
    let fixed_ty = match op {
        BinOp::AddInt | BinOp::SubInt | BinOp::MulInt | BinOp::DivInt | BinOp::ModInt | BinOp::PowInt => Some(Ty::Int),
        BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat | BinOp::DivFloat => Some(Ty::Float),
        BinOp::ConcatStr => Some(Ty::String),
        BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte =>
            infer_from_other_side(left, right, var),
        _ => None,
    };
    if let Some(ref ty) = fixed_ty {
        if matches!(&left.kind, IrExprKind::Var { id } if *id == var) { return Some(ty.clone()); }
        if matches!(&right.kind, IrExprKind::Var { id } if *id == var) { return Some(ty.clone()); }
    }
    None
}

fn infer_from_other_side(left: &IrExpr, right: &IrExpr, var: VarId) -> Option<Ty> {
    if matches!(&left.kind, IrExprKind::Var { id } if *id == var) {
        if !right.ty.has_unresolved_deep() { Some(right.ty.clone()) } else { None }
    } else if matches!(&right.kind, IrExprKind::Var { id } if *id == var) {
        if !left.ty.has_unresolved_deep() { Some(left.ty.clone()) } else { None }
    } else { None }
}

pub fn infer_var_type_from_body(body: &IrExpr, var: VarId) -> Option<Ty> {
    match &body.kind {
        IrExprKind::BinOp { op, left, right } =>
            binop_operand_type(op, left, right, var)
                .or_else(|| infer_var_type_from_body(left, var))
                .or_else(|| infer_var_type_from_body(right, var)),
        IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. } =>
            args.iter().find_map(|a| infer_var_type_from_body(a, var)),
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().find_map(|s| match &s.kind {
                IrStmtKind::Bind { value, .. } | IrStmtKind::Expr { expr: value } =>
                    infer_var_type_from_body(value, var),
                _ => None,
            }).or_else(|| expr.as_ref().and_then(|e| infer_var_type_from_body(e, var)))
        }
        IrExprKind::If { cond, then, else_ } =>
            infer_var_type_from_body(cond, var)
                .or_else(|| infer_var_type_from_body(then, var))
                .or_else(|| infer_var_type_from_body(else_, var)),
        IrExprKind::Match { subject, arms } =>
            infer_var_type_from_body(subject, var)
                .or_else(|| arms.iter().find_map(|a| infer_var_type_from_body(&a.body, var))),
        // Look through Result/Option constructors so a wrapped body like
        // `ok(x * 10)` or `some(x * 10)` still exposes `x`'s use site. Without
        // this, a callback whose param type the checker failed to pin would have
        // no body-derived fallback either.
        IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } =>
            infer_var_type_from_body(expr, var),
        _ => None,
    }
}

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
                Ty::Named(name, _) => {
                    symbols.lookup_field(name.as_str(), field.as_str())
                        .filter(|t| !t.has_unresolved_deep())
                        .cloned()
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
            // For List[T], result is T. Use effective_ty to resolve through VarTable.
            let obj_ty = effective_ty(object, vt);
            if obj_ty.has_unresolved_deep() {
            }
            if let Ty::Applied(_, args) = &obj_ty {
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
        // StringInterp always produces String
        IrExprKind::StringInterp { .. } => Some(Ty::String),
        // Clone preserves the inner type
        IrExprKind::Clone { expr } => {
            if !expr.ty.has_unresolved_deep() { Some(expr.ty.clone()) } else { None }
        }
        // Layout-transparent codegen wrappers: the node's value type is the
        // inner expression's type. `*box` (Deref), `Box::new(x)` (BoxNew),
        // `(x).to_vec()` (ToVec), `&x` / `&*x` (Borrow), and `await x` all
        // carry the same Almide-level `Ty` as their operand — the wrapper is a
        // representation detail the emit layer applies, not a type change. After
        // the bottom-up walk the operand is concrete, so we can pull its type up.
        // Each makes one more shape resolvable instead of bottoming out at the
        // `_ => None` arm and surfacing as an audit violation.
        IrExprKind::Deref { expr }
        | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr }
        | IrExprKind::Borrow { expr, .. }
        | IrExprKind::Await { expr } => {
            if !expr.ty.has_unresolved_deep() { Some(expr.ty.clone()) } else { None }
        }
        // Range produces List[Int]
        IrExprKind::Range { .. } => Some(Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::List, vec![Ty::Int],
        )),
        // MapAccess: Map[K,V] → Option[V]
        IrExprKind::MapAccess { object, .. } => {
            let obj_ty = effective_ty(object, vt);
            if let Ty::Applied(_, args) = &obj_ty {
                args.get(1).cloned()
                    .filter(|t| !t.has_unresolved_deep())
                    .map(|v| Ty::Applied(
                        almide_lang::types::constructor::TypeConstructorId::Option, vec![v],
                    ))
            } else { None }
        }
        // ResultOk wraps in Result[T, E]
        IrExprKind::ResultOk { expr } => {
            if !expr.ty.has_unresolved_deep() {
                Some(Ty::Applied(
                    almide_lang::types::constructor::TypeConstructorId::Result,
                    vec![expr.ty.clone(), Ty::String],
                ))
            } else { None }
        }
        IrExprKind::Call { target, args, .. } => resolve_call_ret_ty(target, args, vt, symbols),
        IrExprKind::RuntimeCall { symbol, args } => {
            // Post-IntrinsicLowering, the `Call { target: Module }` node
            // has been rewritten to RuntimeCall. Rebuild a synthetic
            // `Named { symbol }` target so the existing stdlib
            // polymorphic logic (list.map / list.zip / ...) keeps
            // firing for post-lowering shape.
            let target = CallTarget::Named { name: *symbol };
            resolve_call_ret_ty(&target, args, vt, symbols)
        }
        // A spread copies its base's record type — the checker's own rule
        // (infer's SpreadRecord = base passthrough). Without this arm a
        // cross-module spread base whose type lands late (module top-lets
        // are checked AFTER main) bottomed out at `_ => None` and was
        // refused by the AllTypesConcrete gate (#502).
        IrExprKind::SpreadRecord { base, .. } => {
            let base_ty = effective_ty(base, vt);
            if !base_ty.has_unresolved_deep() { Some(base_ty) } else { None }
        }
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
        CallTarget::Module { module, func, .. } => {
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
        // Calling a closure VALUE (`f(x)` where `f` is a Fn-typed var/expr — e.g.
        // a HOF lambda parameter): the call's type is the callee's RETURN type, not
        // its whole Fn type. Without this the node keeps the `fn(..) -> T` type and
        // a later `acc + f(x)` trips the IR verifier (AddInt on a function value).
        CallTarget::Computed { callee } => {
            if let Ty::Fn { ret, .. } = &callee.ty {
                if !ret.has_unresolved_deep() {
                    return Some((**ret).clone());
                }
            }
        }
        _ => {}
    }

    // Decode (module, func) from every stdlib call-target shape:
    //   - `Module { list, map }`                 — pre-lowering
    //   - `Named { "almide_rt_list_map" }`       — post-ResolveCalls or
    //                                              frontend mangling
    let (module_owned, func_owned): (String, String) = match target {
        CallTarget::Module { module, func, .. } => (module.as_str().to_string(), func.as_str().to_string()),
        CallTarget::Named { name } => {
            let s = name.as_str();
            if let Some(rest) = s.strip_prefix("almide_rt_") {
                if let Some(under) = rest.find('_') {
                    (rest[..under].to_string(), rest[under+1..].to_string())
                } else { return None }
            } else { return None }
        }
        _ => return None,
    };
    let module = module_owned.as_str();
    let func = func_owned.as_str();

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


// ── Audit / hard gate: residual unresolved (or value-Never) types ───
//
// Two consumers share one collector ([`collect_unresolved_sites`]):
//
//   1. The `ConcretizeTypes` postcondition ([`audit_remaining_unresolved`]),
//      verified mid-pipeline in debug / `ALMIDE_VERIFY_IR` builds.
//   2. The HARD codegen-entry gate ([`assert_types_concretized`]), run
//      unconditionally on EVERY build (debug AND release, Rust AND WASM)
//      right before emit. A surviving `Ty::Unknown` (or a value-position
//      `Ty::Never`) here is the root of the `Unknown→i32` WASM fallback that
//      silently miscompiled `fan.map` and friends: this gate turns that whole
//      class from a runtime trap into a clean compile-time error.
//
// Both read the same skip predicate so "what is a legitimate residual" is
// defined in exactly one place.

/// One residual-unresolved expression, with enough context for a diagnostic
/// that names the function and source span. `span` is `None` when the IR node
/// lost its provenance (synthetic nodes inserted by passes).
#[derive(Debug, Clone)]
pub struct UnresolvedSite {
    /// Enclosing function, e.g. `fn main` or `list::map`.
    pub location: String,
    /// IR node kind name (`Var`, `Member`, `Call`, …).
    pub kind: &'static str,
    /// `{:?}` of the offending `Ty` (e.g. `Unknown`, `Tuple([Unknown, Int])`).
    pub ty: String,
    /// Source span of the node, if it carries one.
    pub span: Option<almide_base::span::Span>,
    /// Extra context (var name + stored ty, member field, …).
    pub detail: String,
    /// True when the violation is a value-position `Ty::Never` rather than an
    /// `Unknown`/`TypeVar`. Distinguished so the diagnostic can say which.
    pub value_never: bool,
}

/// A node whose `ty` is unresolved but which legitimately has no concrete
/// runtime type to fill in — these are NOT violations. The list is small and
/// every entry is justified; it is the single source of truth shared by the
/// soft audit and the hard gate.
fn is_legit_unresolved(expr: &IrExpr) -> bool {
    // Nodes that have no runtime representation at all.
    matches!(&expr.kind,
        IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::Hole | IrExprKind::Todo { .. }
        | IrExprKind::OptionNone
        | IrExprKind::EmptyMap
    )
    // Empty list literal `[]` whose element type could not be pinned down by
    // either upstream inference or `propagate_expected_ty`. The stored element
    // count is zero, so every emit path — `Vec::<T>::new()` on Rust, the 4-byte
    // `[len=0]` header on WASM — produces the same bytes regardless of `T`.
    // Treating it as a violation would force the gate to stay soft just to
    // cover `for _ in []` / `fan.map([], f)` style uses that have no bearing on
    // runtime behavior.
    || matches!(&expr.kind,
        IrExprKind::List { elements } if elements.is_empty())
    // `ResultErr(...)` or `Unwrap { ResultErr(...) }` in guard-else: the Ok
    // slot may remain Unknown because the checker can't determine it from
    // `err()` alone. The ok-path is unreachable at runtime so the Unknown is
    // harmless.
    || matches!(&expr.kind, IrExprKind::ResultErr { .. })
    || matches!(&expr.kind,
        IrExprKind::Unwrap { expr: inner }
            if matches!(inner.kind, IrExprKind::ResultErr { .. }))
    // `Block` whose sole tail is the same skipped `Unwrap` pattern — the block
    // is just the desugared `else { err(...)! }` wrapper that lowering emits for
    // `guard` statements. `Block.ty` mirrors `tail.ty`, so marking only the
    // Unwrap would leave the outer Block as a spurious violation.
    || matches!(&expr.kind,
        IrExprKind::Block { stmts, expr: Some(tail) }
            if stmts.is_empty()
                && matches!(&tail.kind,
                    IrExprKind::Unwrap { expr: inner }
                        if matches!(inner.kind, IrExprKind::ResultErr { .. })))
    // OpenRecord-typed expressions: an open-record bound
    // (`fn f(x: { name: String, .. })`) is a structural constraint, not an
    // inference failure. The Var node for such a param trivially carries its
    // declared OpenRecord ty through monomorphization's `__Unknown` fallback
    // path. Emit handles OpenRecord via its structural dispatch — no Unknown
    // slot to fill.
    || matches!(&expr.ty, Ty::OpenRecord { .. })
    // The node's type is unresolved ONLY inside empty-container payload slots
    // (`Option[Unknown]`, `List[Unknown]`, `Set[Unknown]`, `Map[_, Unknown]`,
    // possibly nested in a `Record`/`Tuple`). This generalizes the two leaf
    // entries above (bare `OptionNone`, empty `[]`) one level up: an unannotated
    // `let leaf = { value: 1, left: none, right: none }` gives the *record* —
    // and any `Var`/`Member` reading it — a type whose only Unknowns sit in the
    // `Option` payloads of fields that are only ever `none`. A `some(x)` /
    // non-empty literal would have pinned the payload during inference, so an
    // Unknown payload that survived here is NEVER materialized; the container is
    // empty/None at runtime and its payload type is unobservable on both targets
    // (the very property that makes the bare-`OptionNone`/empty-`[]` entries
    // sound — emit already handles those exact slots). A bare `Unknown`, or one
    // inside a Tuple/Result-Ok/Fn position (which DOES carry a value), is not
    // covered and still fails the gate.
    || unresolved_only_in_empty_payloads(&expr.ty)
}

/// True when every `Unknown`/`TypeVar` in `ty` sits in an *empty-container
/// payload* position — the element slot of `Option`/`List`/`Set`, or the value
/// slot of `Map` — possibly nested through `Record`/`Tuple` fields. Such a slot
/// holds no bytes unless the container is populated, and a populated container
/// would have pinned the payload during inference; so an Unknown that reaches
/// here marks an empty/None container whose payload type is unobservable.
///
/// Returns `false` for a fully-concrete `ty` (so it never masks a real value),
/// for a bare `Unknown`/`TypeVar`, and for an Unknown in any value-bearing
/// position (`Tuple` element, `Result` Ok, `Map` KEY, `Fn` param/ret) — those
/// stay hard violations.
fn unresolved_only_in_empty_payloads(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId as TCI;
    // Nothing unresolved ⇒ not "unresolved only in payloads" (the caller already
    // gates on `has_unresolved_deep`, but be explicit so the helper is total).
    if !ty.has_unresolved_deep() { return false; }
    // A bare residual `Unknown`/`TypeVar` is legit ONLY in an `Option` element
    // slot. That is the one undecidable-empty-payload class the frontend E018
    // check does NOT own: an unannotated `none` that is only ever `none`
    // (a recursive record field — `let leaf = { value: 1, left: none }`), whose
    // `Option` payload is never materialized. Every OTHER undecidable empty
    // collection — an empty `[]` / `[:]` / `set.new()` / `map.new()` /
    // `list.with_capacity` whose element the program never pins — is now a
    // user-facing compile error raised in the frontend BEFORE codegen (E018),
    // so a bare-`Unknown` `List`/`Set` element or `Map` value can no longer
    // reach this gate from user code. We therefore no longer whitelist it: the
    // gate is back to "an Unknown here is a COMPILER bug". The collection slots
    // still RECURSE (so a `List[Option[Unknown]]` of only-`none` elements stays
    // legit through the `Option`), but a bare `Unknown` directly in them is a
    // violation again.
    fn ok(ty: &Ty) -> bool {
        // A concrete subtree is always fine.
        if !ty.has_unresolved_deep() { return true; }
        use almide_lang::types::constructor::TypeConstructorId as TCI;
        match ty {
            // Option element: a bare `Unknown`/`TypeVar` here is the never-
            // materialized `none` payload — the one whitelisted leaf.
            Ty::Applied(TCI::Option, args) if args.len() == 1 => {
                matches!(args[0], Ty::Unknown | Ty::TypeVar(_)) || ok(&args[0])
            }
            // List/Set element: only a DEEPER empty-payload shape (e.g. an
            // `Option` of `none`) is legit; a bare `Unknown` here was an
            // undecidable empty collection and is now an E018 the frontend
            // rejects first, so it is a gate violation again.
            Ty::Applied(TCI::List, args)
            | Ty::Applied(TCI::Set, args) if args.len() == 1 => ok(&args[0]),
            // Map[K, V]: the KEY is load-bearing (hashed/compared). The VALUE,
            // like a List element, is legit only via a deeper empty payload —
            // a bare `Unknown` value is an undecidable empty map (E018).
            Ty::Applied(TCI::Map, args) if args.len() == 2 => {
                !args[0].has_unresolved_deep() && ok(&args[1])
            }
            // Records/tuples are transparent: qualify iff EVERY unresolved field
            // is itself an empty-payload slot.
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                fields.iter().all(|(_, t)| ok(t))
            }
            Ty::Tuple(elems) => elems.iter().all(ok),
            // Anything else carrying an Unknown (bare Unknown/TypeVar, Result,
            // Fn, …) is load-bearing — not covered.
            _ => false,
        }
    }
    let _ = TCI::Option; // keep the import used on all cfgs
    ok(ty)
}

/// True when `expr` is a value-position `Ty::Never` violation. `Ty::Never` is
/// legitimate for *divergent* expressions — `break` / `continue` / `todo()` /
/// a hole never yield a value, and a call to a `-> Never` function diverges. It
/// is a BUG only when a node that DOES produce a usable runtime value is typed
/// `Never`: emit would then have to materialize a value of an uninhabited type,
/// the value-Never analogue of the `Unknown→i32` fallback. Mirrors the wasm
/// `ty_to_valtype` convention where `Never` maps to "no value" (`None`).
fn is_value_never(expr: &IrExpr) -> bool {
    if expr.ty != Ty::Never { return false; }
    // Inherently-divergent kinds are *allowed* to be Never.
    !matches!(&expr.kind,
        IrExprKind::Break | IrExprKind::Continue
        | IrExprKind::Hole | IrExprKind::Todo { .. }
        // A call / runtime-call may legitimately be a `-> Never` divergent
        // function (panic, exit). Control-flow joins (If/Match/Block) inherit
        // Never from a diverging branch and are fine. Returning/propagation
        // wrappers likewise carry through a diverging inner.
        | IrExprKind::Call { .. } | IrExprKind::TailCall { .. }
        | IrExprKind::RuntimeCall { .. } | IrExprKind::RustMacro { .. }
        | IrExprKind::If { .. } | IrExprKind::Match { .. }
        | IrExprKind::Block { .. }
        | IrExprKind::Try { .. } | IrExprKind::Unwrap { .. }
    )
}

/// Walk every reachable expression and collect residual unresolved-type (or
/// value-`Never`) sites that are NOT covered by [`is_legit_unresolved`]. This
/// is the shared engine behind the soft audit and the hard gate.
pub fn collect_unresolved_sites(program: &IrProgram) -> Vec<UnresolvedSite> {
    struct Auditor<'a> {
        location: String,
        sites: Vec<UnresolvedSite>,
        var_table: &'a VarTable,
    }
    impl<'a> Auditor<'a> {
        fn detail_of(&self, expr: &IrExpr) -> String {
            match &expr.kind {
                IrExprKind::Var { id } => {
                    if (id.0 as usize) < self.var_table.entries.len() {
                        let info = &self.var_table.entries[id.0 as usize];
                        format!("var_id={} name={} stored_ty={:?}", id.0, info.name.as_str(), info.ty)
                    } else {
                        format!("var_id={}", id.0)
                    }
                }
                IrExprKind::Member { field, .. } => format!("member={}", field.as_str()),
                IrExprKind::Call { .. } => "(call)".to_string(),
                _ => String::new(),
            }
        }
        fn record(&mut self, expr: &IrExpr, value_never: bool) {
            let detail = self.detail_of(expr);
            self.sites.push(UnresolvedSite {
                location: self.location.clone(),
                kind: kind_name(&expr.kind),
                ty: format!("{:?}", expr.ty),
                span: expr.span,
                detail,
                value_never,
            });
        }
    }
    impl<'a> IrVisitor for Auditor<'a> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if (expr.ty).has_unresolved_deep() {
                if !is_legit_unresolved(expr) {
                    self.record(expr, false);
                }
            } else if is_value_never(expr) {
                self.record(expr, true);
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
            IrExprKind::ResultOk { .. } => "ResultOk",
            IrExprKind::ResultErr { .. } => "ResultErr",
            IrExprKind::Try { .. } => "Try",
            IrExprKind::Unwrap { .. } => "Unwrap",
            IrExprKind::UnwrapOr { .. } => "UnwrapOr",
            IrExprKind::ToOption { .. } => "ToOption",
            IrExprKind::OptionalChain { .. } => "OptionalChain",
            IrExprKind::OptionSome { .. } => "OptionSome",
            IrExprKind::OptionNone => "OptionNone",
            IrExprKind::Break => "Break",
            IrExprKind::Continue => "Continue",
            IrExprKind::StringInterp { .. } => "StringInterp",
            IrExprKind::RenderedCall { .. } => "RenderedCall",
            IrExprKind::RuntimeCall { .. } => "RuntimeCall",
            IrExprKind::InlineRust { .. } => "InlineRust",
            IrExprKind::RustMacro { .. } => "RustMacro",
            IrExprKind::Clone { .. } => "Clone",
            IrExprKind::Deref { .. } => "Deref",
            IrExprKind::Borrow { .. } => "Borrow",
            IrExprKind::BoxNew { .. } => "BoxNew",
            IrExprKind::RcWrap { .. } => "RcWrap",
            IrExprKind::ToVec { .. } => "ToVec",
            IrExprKind::Await { .. } => "Await",
            IrExprKind::Todo { .. } => "Todo",
            IrExprKind::Hole => "Hole",
            IrExprKind::IterChain { .. } => "IterChain",
            IrExprKind::EmptyMap => "EmptyMap",
            _ => "(unknown-variant)",
        }
    }
    let mut a = Auditor { location: String::new(), sites: Vec::new(), var_table: &program.var_table };
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
    a.sites
}

/// Render one site as a one-line span-tagged description.
fn render_site(s: &UnresolvedSite) -> String {
    let loc = match s.span {
        Some(sp) => format!("{}:{}:{}", s.location, sp.line, sp.col),
        None => format!("{} <no span>", s.location),
    };
    let detail = if s.detail.is_empty() { String::new() } else { format!(" {}", s.detail) };
    let what = if s.value_never { "value-Never" } else { "unresolved" };
    format!("[{}] {} {} ty={}{}", loc, what, s.kind, s.ty, detail)
}

/// Postcondition audit (soft): used by the mid-pipeline `ConcretizeTypes`
/// postcondition. Returns a single summary violation string when any residual
/// site survives, formatted like the historical message so existing log
/// scrapers (`grep POSTCONDITION VIOLATION`) keep working.
fn audit_remaining_unresolved(program: &IrProgram) -> Vec<String> {
    let sites = collect_unresolved_sites(program);
    if sites.is_empty() { return Vec::new(); }
    let samples: Vec<String> = sites.iter().take(5).map(render_site).collect();
    vec![format!("[ConcretizeTypes] {} expression(s) remain with unresolved types. Samples: {}",
        sites.len(), samples.join(" | "))]
}

/// HARD codegen-entry gate. Runs on EVERY build (debug AND release, Rust AND
/// WASM) right before emit. If any reachable expression still carries a
/// `Ty::Unknown`/`Ty::TypeVar` (or a value-position `Ty::Never`) that the
/// concretization machinery could not resolve, this is a COMPILER bug — emit
/// would otherwise fall back to `i32` on WASM (the `fan.map` silent-miscompile
/// class) or to an arbitrary type on Rust. We refuse to emit and abort with a
/// clean, structured diagnostic that names the function + span. This is a
/// compiler-bug detector, so the message targets compiler developers ("please
/// report"); it is a controlled error, NOT an ICE (no panic, no backtrace).
///
/// The detection ([`collect_unresolved_sites`]) is a pure function, unit-tested
/// directly; this wrapper only adds the formatting + abort so the test process
/// is never killed.
pub fn assert_types_concretized(program: &IrProgram) {
    let sites = collect_unresolved_sites(program);
    if sites.is_empty() { return; }

    let mut msg = String::new();
    msg.push_str("error: [COMPILER BUG] internal type resolution failed before codegen\n");
    msg.push_str(&format!(
        "  {} expression(s) still carry an unresolved (Unknown/TypeVar) or value-Never type\n",
        sites.len()
    ));
    msg.push_str("  after the ConcretizeTypes pass. Emitting these would silently fall back to a\n");
    msg.push_str("  wrong runtime representation (e.g. the WASM Unknown→i32 fallback), so the build\n");
    msg.push_str("  is refused instead. This is a compiler bug, not an error in your program.\n");
    // Cap the listed sites so a pathological program can't flood the terminal;
    // the count above always reflects the true total.
    const MAX_LISTED: usize = 20;
    for s in sites.iter().take(MAX_LISTED) {
        msg.push_str(&format!("    {}\n", render_site(s)));
    }
    if sites.len() > MAX_LISTED {
        msg.push_str(&format!("    ... and {} more\n", sites.len() - MAX_LISTED));
    }
    msg.push_str("  hint: please report this at https://github.com/almide/almide/issues\n");
    msg.push_str("        with the source above — include the function name(s) and span(s) shown.\n");

    eprint!("{}", msg);
    // Controlled abort: print the diagnostic, then terminate the build with a
    // non-zero status. This mirrors the established codegen failure convention
    // (`main.rs` build paths, the generated div-by-zero runtime) — a clean
    // process exit, NOT a `panic!` that would dump a Rust backtrace (an ICE) or
    // be swallowed as a "skip" by the spec harness's `catch_unwind`. The
    // collector is unit-tested separately so this branch never runs under
    // `cargo test` for a well-formed program.
    std::process::exit(1);
}

#[cfg(test)]
mod hard_gate_tests {
    use super::*;
    use almide_ir::{IrFunction, IrStmt, IrStmtKind, IrVisibility, Mutability, VarId};

    fn expr(kind: IrExprKind, ty: Ty) -> IrExpr {
        IrExpr { kind, ty, span: None, def_id: None }
    }

    fn make_fn(name: &str, body: IrExpr) -> IrFunction {
        IrFunction {
            name: name.into(),
            params: vec![],
            ret_ty: body.ty.clone(),
            body,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![],
            module_origin: None,
        }
    }

    fn program(body: IrExpr, var_table: VarTable) -> IrProgram {
        IrProgram {
            functions: vec![make_fn("main", body)],
            top_lets: vec![],
            type_decls: vec![],
            var_table,
            def_table: Default::default(),
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
            used_stdlib_modules: Default::default(),
        }
    }

    /// A synthetic Unknown-carrying IR (a `Member` access typed Unknown — NOT
    /// in the legitimate-residual skip list) must fail the postcondition / gate.
    #[test]
    fn synthetic_unknown_member_is_a_violation() {
        let mut vt = VarTable::new();
        let rec = vt.alloc("rec".into(), Ty::Unknown, Mutability::Let, None);
        let body = expr(
            IrExprKind::Member {
                object: Box::new(expr(IrExprKind::Var { id: rec }, Ty::Unknown)),
                field: "field".into(),
            },
            Ty::Unknown,
        );
        let prog = program(body, vt);
        let sites = collect_unresolved_sites(&prog);
        // The Member node and the Var node both carry Unknown → at least one
        // violation, none of them whitelisted.
        assert!(!sites.is_empty(), "Unknown Member must be flagged");
        assert!(sites.iter().any(|s| s.kind == "Member"), "Member site expected: {sites:?}");
        assert!(sites.iter().all(|s| !s.value_never), "these are Unknown, not Never");
        // The soft audit must agree with the hard collector.
        assert!(!audit_remaining_unresolved(&prog).is_empty());
    }

    /// A fully-concrete program produces zero sites (the gate is silent).
    #[test]
    fn concrete_program_has_no_sites() {
        let body = expr(IrExprKind::LitInt { value: 7 }, Ty::Int);
        let prog = program(body, VarTable::new());
        assert!(collect_unresolved_sites(&prog).is_empty());
        assert!(audit_remaining_unresolved(&prog).is_empty());
    }

    /// Whitelisted residuals (empty list literal, OptionNone) are NOT flagged,
    /// so the hard gate does not regress programs the soft audit accepted.
    #[test]
    fn whitelisted_residuals_are_not_violations() {
        let empty_list = expr(
            IrExprKind::List { elements: vec![] },
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, vec![Ty::Unknown]),
        );
        let prog = program(empty_list, VarTable::new());
        assert!(collect_unresolved_sites(&prog).is_empty(), "empty `[]` is whitelisted");

        let none = expr(IrExprKind::OptionNone, Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::Option, vec![Ty::Unknown]));
        let prog = program(none, VarTable::new());
        assert!(collect_unresolved_sites(&prog).is_empty(), "OptionNone is whitelisted");
    }

    /// A value-position `Ty::Never` (a `Var` typed Never) is a violation, but a
    /// divergent call typed Never is allowed — distinguishing "uninhabited value
    /// materialized" from "expression diverges".
    #[test]
    fn value_never_var_flagged_but_divergent_call_allowed() {
        let mut vt = VarTable::new();
        let v = vt.alloc("v".into(), Ty::Never, Mutability::Let, None);
        let body = expr(IrExprKind::Var { id: v }, Ty::Never);
        let prog = program(body, vt);
        let sites = collect_unresolved_sites(&prog);
        assert!(sites.iter().any(|s| s.value_never && s.kind == "Var"),
            "value-Never Var must be flagged: {sites:?}");

        // A diverging `todo()`-style hole / break is allowed to be Never.
        let brk = expr(IrExprKind::Break, Ty::Never);
        let prog = program(brk, VarTable::new());
        assert!(collect_unresolved_sites(&prog).is_empty(), "Break:Never is allowed");
    }

    /// An unannotated record whose Unknowns live ONLY in `Option`/`List` payload
    /// slots (an only-ever-`none` field, `let leaf = { value: 1, left: none }`)
    /// is whitelisted; the same record with an Unknown in a load-bearing slot
    /// (a `Result` Ok, a tuple element) is NOT — the gate stays strict there.
    #[test]
    fn unknown_only_in_empty_container_payloads_is_whitelisted() {
        use almide_lang::types::constructor::TypeConstructorId as TCI;
        let opt_unknown = Ty::Applied(TCI::Option, vec![Ty::Unknown]);
        let rec_ty = Ty::Record { fields: vec![
            ("value".into(), Ty::Int),
            ("left".into(), opt_unknown.clone()),
            ("right".into(), opt_unknown.clone()),
        ] };
        // A Var carrying this record type is whitelisted (payload-only Unknown).
        let mut vt = VarTable::new();
        let leaf = vt.alloc("leaf".into(), rec_ty.clone(), Mutability::Let, None);
        let prog = program(expr(IrExprKind::Var { id: leaf }, rec_ty.clone()), vt);
        assert!(collect_unresolved_sites(&prog).is_empty(),
            "record with Unknown only in Option payloads is whitelisted");

        // Direct predicate checks. The ONLY whitelisted bare-Unknown leaf is an
        // `Option` payload (a never-materialized `none`); every other undecidable
        // empty collection is now rejected in the frontend (E018) before it can
        // reach this gate, so the gate no longer whitelists them.
        assert!(unresolved_only_in_empty_payloads(&opt_unknown));
        assert!(unresolved_only_in_empty_payloads(&rec_ty));
        // A bare-Unknown List/Set element or Map value is NO LONGER whitelisted —
        // it would be an undecidable empty collection, which E018 rejects first,
        // so reaching here is a compiler bug.
        assert!(!unresolved_only_in_empty_payloads(&Ty::Applied(TCI::List, vec![Ty::Unknown])),
            "bare List[Unknown] is now an E018 the frontend owns — a gate violation");
        assert!(!unresolved_only_in_empty_payloads(&Ty::Applied(TCI::Set, vec![Ty::Unknown])),
            "bare Set[Unknown] is now an E018 the frontend owns — a gate violation");
        assert!(!unresolved_only_in_empty_payloads(&Ty::Applied(TCI::Map, vec![Ty::String, Ty::Unknown])),
            "bare Map[_, Unknown] is now an E018 the frontend owns — a gate violation");
        // A List/Set of only-`none` elements stays legit THROUGH the Option.
        assert!(unresolved_only_in_empty_payloads(&Ty::Applied(TCI::List, vec![opt_unknown.clone()])),
            "List[Option[Unknown]] (a list of nones) is legit via the Option payload");
        // Load-bearing Unknowns are NOT whitelisted.
        assert!(!unresolved_only_in_empty_payloads(&Ty::Unknown), "bare Unknown is load-bearing");
        assert!(!unresolved_only_in_empty_payloads(&Ty::Tuple(vec![Ty::Int, Ty::Unknown])),
            "tuple element Unknown is load-bearing");
        assert!(!unresolved_only_in_empty_payloads(&Ty::Applied(TCI::Result, vec![Ty::Unknown, Ty::String])),
            "Result Ok Unknown is load-bearing");
        // Map KEY Unknown is load-bearing.
        assert!(!unresolved_only_in_empty_payloads(&Ty::Applied(TCI::Map, vec![Ty::Unknown, Ty::Int])),
            "Map key Unknown is load-bearing");
        // A fully concrete type is never reported as payload-only.
        assert!(!unresolved_only_in_empty_payloads(&Ty::Int));
    }
}
