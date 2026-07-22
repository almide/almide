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
        erase_wasm_type_aliases(&mut program, _target);

        let symbols = build_symbol_table(&program);

        // Take var_table out of program so we can mutate it while also
        // mutating program.functions. Back-propagation (below) updates
        // VarTable entries for lambda accumulator params and match-pattern
        // bindings; downstream passes expect the updates to persist.
        let mut prog_vt = std::mem::take(&mut program.var_table);

        concretize_top_lets(&mut program, &mut prog_vt, &symbols);
        propagate_top_let_types_by_name(&program, &mut prog_vt);
        concretize_fn_bodies(&mut program, &mut prog_vt, &symbols);

        program.var_table = prog_vt;
        PassResult { program, changed: true }
    }
}

// ── `run` phase extraction (cog>100 decomposition, pattern 2) ──
//
// Each of these is a 1:1 text-move of one of `run`'s sequential,
// independent phases. None reads a value a LATER phase produces.

/// Build type alias map: alias_name → underlying type.
/// `mod type SafeHtml = String` → `aliases["SafeHtml"] = String`.
/// Erase aliases throughout the IR (WASM target only — Rust codegen
/// handles newtypes natively) so downstream codegen never sees them.
fn erase_wasm_type_aliases(program: &mut IrProgram, target: Target) {
    let mut aliases: HashMap<String, Ty> = HashMap::new();
    for td in program.type_decls.iter().chain(program.modules.iter().flat_map(|m| m.type_decls.iter())) {
        if let almide_ir::IrTypeDeclKind::Alias { target: alias_target } = &td.kind {
            aliases.insert(td.name.to_string(), alias_target.clone());
        }
    }
    if !aliases.is_empty() && target == Target::Wasm {
        erase_type_aliases(program, &aliases);
    }
}

/// Concretize a single top-let's value, then push its type into the
/// declared `ty` and VarTable entry if both were still unresolved.
/// Extracted from `concretize_top_lets` (cog>30 decomposition, second
/// round): the top-level and module loops called the exact same
/// per-top-let body.
fn concretize_top_let(tl: &mut IrTopLet, prog_vt: &mut VarTable, symbols: &SymbolTable) {
    concretize_expr(&mut tl.value, prog_vt, symbols, &Ty::Unknown);
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

/// Phase 1: Resolve top_lets first so their types are available
/// when functions reference cross-module let values.
fn concretize_top_lets(program: &mut IrProgram, prog_vt: &mut VarTable, symbols: &SymbolTable) {
    for tl in &mut program.top_lets {
        concretize_top_let(tl, prog_vt, symbols);
    }
    for module in &mut program.modules {
        for tl in &mut module.top_lets {
            concretize_top_let(tl, prog_vt, symbols);
        }
    }
}

/// Phase 1b: Propagate top_let types by name into VarTable entries
/// that are cross-module synthetic references (different VarId, same name).
fn propagate_top_let_types_by_name(program: &IrProgram, prog_vt: &mut VarTable) {
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
}

/// Phase 2: Now resolve functions (which may reference cross-module lets
/// whose VarTable types are now populated).
fn concretize_fn_bodies(program: &mut IrProgram, prog_vt: &mut VarTable, symbols: &SymbolTable) {
    for func in &mut program.functions {
        let ret = func.ret_ty.clone();
        concretize_expr(&mut func.body, prog_vt, symbols, &ret);
    }
    for module in &mut program.modules {
        for func in &mut module.functions {
            let ret = func.ret_ty.clone();
            concretize_expr(&mut func.body, prog_vt, symbols, &ret);
        }
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
    for func in &mut program.functions { erase_fn_types(func, aliases); }
    for tl in &mut program.top_lets { erase_expr(&mut tl.value, aliases); }
    for module in &mut program.modules {
        for func in &mut module.functions { erase_fn_types(func, aliases); }
    }
    // Also erase in VarTable
    for entry in &mut program.var_table.entries {
        erase_ty(&mut entry.ty, aliases);
    }
}

// ── `erase_type_aliases` helpers (Ty::Named alias substitution) ──────────

/// Erase aliases in a function's return type, param types, and body — the
/// identical per-function step `erase_type_aliases` runs both for top-level
/// and module functions.
fn erase_fn_types(func: &mut IrFunction, aliases: &HashMap<String, Ty>) {
    erase_ty(&mut func.ret_ty, aliases);
    for p in &mut func.params { erase_ty(&mut p.ty, aliases); }
    erase_expr(&mut func.body, aliases);
}

fn erase_ty(ty: &mut Ty, aliases: &HashMap<String, Ty>) {
    if let Ty::Named(name, _) = ty {
        if let Some(target) = aliases.get(name.as_str()) {
            *ty = target.clone();
        }
    }
}

// `Call { target: Named { name }, args, .. }` arm of `erase_expr`: a
// constructor call for an alias, `Alias(arg)`, unwraps to `arg`
// (recursively erased); otherwise erase each arg in place.
fn erase_expr_alias_call(expr: &mut IrExpr, aliases: &HashMap<String, Ty>) {
    let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &mut expr.kind else { unreachable!() };
    if aliases.contains_key(name.as_str()) && args.len() == 1 {
        let arg = args.remove(0);
        *expr = arg;
        erase_expr(expr, aliases);
        return;
    }
    for a in args.iter_mut() { erase_expr(a, aliases); }
}

// `Block { stmts, expr: tail }` arm of `erase_expr`.
fn erase_expr_block(stmts: &mut [IrStmt], tail: &mut Option<Box<IrExpr>>, aliases: &HashMap<String, Ty>) {
    for s in stmts.iter_mut() { erase_stmt(s, aliases); }
    if let Some(t) = tail { erase_expr(t, aliases); }
}

// `If { cond, then, else_ }` arm of `erase_expr`.
fn erase_expr_if(cond: &mut IrExpr, then: &mut IrExpr, else_: &mut IrExpr, aliases: &HashMap<String, Ty>) {
    erase_expr(cond, aliases);
    erase_expr(then, aliases);
    erase_expr(else_, aliases);
}

// `Match { subject, arms }` arm of `erase_expr`.
fn erase_expr_match(subject: &mut IrExpr, arms: &mut [IrMatchArm], aliases: &HashMap<String, Ty>) {
    erase_expr(subject, aliases);
    for arm in arms.iter_mut() {
        erase_pattern(&mut arm.pattern, aliases);
        if let Some(g) = &mut arm.guard { erase_expr(g, aliases); }
        erase_expr(&mut arm.body, aliases);
    }
}

// `ForIn { iterable, body, .. }` arm of `erase_expr`.
fn erase_expr_for_in(iterable: &mut IrExpr, body: &mut [IrStmt], aliases: &HashMap<String, Ty>) {
    erase_expr(iterable, aliases);
    for s in body.iter_mut() { erase_stmt(s, aliases); }
}

// `While { cond, body }` arm of `erase_expr`.
fn erase_expr_while(cond: &mut IrExpr, body: &mut [IrStmt], aliases: &HashMap<String, Ty>) {
    erase_expr(cond, aliases);
    for s in body.iter_mut() { erase_stmt(s, aliases); }
}

// `Call { args, .. } | TailCall { args, .. }`, `RuntimeCall { args, .. }`,
// and `List { elements } | Tuple { elements }` arms of `erase_expr` — all
// erase a flat `Vec<IrExpr>` in place.
fn erase_expr_args(args: &mut [IrExpr], aliases: &HashMap<String, Ty>) {
    for a in args.iter_mut() { erase_expr(a, aliases); }
}

// `Lambda { body, params, .. }` arm of `erase_expr`.
fn erase_expr_lambda(body: &mut IrExpr, params: &mut [(VarId, Ty)], aliases: &HashMap<String, Ty>) {
    for (_, t) in params.iter_mut() { erase_ty(t, aliases); }
    erase_expr(body, aliases);
}

// `Record { fields, .. }` arm of `erase_expr`.
fn erase_expr_field_values(fields: &mut [(almide_base::intern::Sym, IrExpr)], aliases: &HashMap<String, Ty>) {
    for (_, e) in fields.iter_mut() { erase_expr(e, aliases); }
}

fn erase_expr(expr: &mut IrExpr, aliases: &HashMap<String, Ty>) {
    erase_ty(&mut expr.ty, aliases);
    match &mut expr.kind {
        // Constructor call: Alias(arg) → arg
        IrExprKind::Call { target: CallTarget::Named { .. }, .. } => erase_expr_alias_call(expr, aliases),
        IrExprKind::Block { stmts, expr: tail } => erase_expr_block(stmts, tail, aliases),
        IrExprKind::If { cond, then, else_ } => erase_expr_if(cond, then, else_, aliases),
        IrExprKind::Match { subject, arms } => erase_expr_match(subject, arms, aliases),
        IrExprKind::Call { args, .. } | IrExprKind::TailCall { args, .. } => erase_expr_args(args, aliases),
        IrExprKind::RuntimeCall { args, .. } => erase_expr_args(args, aliases),
        IrExprKind::Lambda { body, params, .. } => erase_expr_lambda(body, params, aliases),
        IrExprKind::BinOp { left, right, .. } => {
            erase_expr(left, aliases);
            erase_expr(right, aliases);
        }
        IrExprKind::UnOp { operand, .. } => erase_expr(operand, aliases),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => erase_expr_args(elements, aliases),
        IrExprKind::Record { fields, .. } => erase_expr_field_values(fields, aliases),
        IrExprKind::Member { object, .. } => erase_expr(object, aliases),
        IrExprKind::IndexAccess { object, index } => {
            erase_expr(object, aliases);
            erase_expr(index, aliases);
        }
        IrExprKind::ForIn { iterable, body, .. } => erase_expr_for_in(iterable, body, aliases),
        IrExprKind::While { cond, body } => erase_expr_while(cond, body, aliases),
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

fn erase_stmt(stmt: &mut IrStmt, aliases: &HashMap<String, Ty>) {
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

include!("pass_concretize_types_p2.rs");
include!("pass_concretize_types_p3.rs");
include!("pass_concretize_types_p4.rs");
