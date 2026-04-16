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

        for func in &mut program.functions {
            let vt = program.var_table.clone();
            concretize_expr(&mut func.body, &vt, &symbols);
        }
        for tl in &mut program.top_lets {
            let vt = program.var_table.clone();
            concretize_expr(&mut tl.value, &vt, &symbols);
        }
        for module in &mut program.modules {
            let vt = module.var_table.clone();
            for func in &mut module.functions {
                concretize_expr(&mut func.body, &vt, &symbols);
            }
            for tl in &mut module.top_lets {
                concretize_expr(&mut tl.value, &vt, &symbols);
            }
        }
        PassResult { program, changed: true }
    }
}

// ── Symbol table ────────────────────────────────────────────────────

struct SymbolTable {
    /// (module_name, func_name) → return type
    /// "" as module means top-level (for CallTarget::Named).
    sigs: std::collections::HashMap<(String, String), Ty>,
}

impl SymbolTable {
    fn lookup_module(&self, module: &str, func: &str) -> Option<&Ty> {
        self.sigs.get(&(module.to_string(), func.to_string()))
    }
    fn lookup_named(&self, func: &str) -> Option<&Ty> {
        self.sigs.get(&(String::new(), func.to_string()))
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
    SymbolTable { sigs }
}

// ── Core walker ────────────────────────────────────────────────────

fn concretize_expr(expr: &mut IrExpr, vt: &VarTable, symbols: &SymbolTable) {
    let mut c = Concretizer { vt, symbols };
    c.visit_expr_mut(expr);
}

struct Concretizer<'a> {
    vt: &'a VarTable,
    symbols: &'a SymbolTable,
}

impl<'a> IrMutVisitor for Concretizer<'a> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
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

    // 2. Generated stdlib signature lookup with TypeVar substitution.
    //    This covers every stdlib function declared in stdlib/defs/*.toml
    //    (int/float/string/list/map/set/option/result/bytes/value/... etc.)
    //    and substitutes type variables (K, V, T, A, B, ...) from the
    //    declared parameter types against the actual argument types.
    if let Some(sig) = crate::generated::stdlib_ret_ty::lookup_stdlib_ret(module, func) {
        let substituted = substitute_type_vars(&sig.ret_ty, sig.param_tys, args);
        if !substituted.has_unresolved_deep() {
            return Some(substituted);
        }
        // Fall through to list-specific helpers below if sig had TypeVars we
        // couldn't substitute (e.g. lambda return types for list.map).
    }

    // 3. Stdlib polymorphic list operations with lambda return types.
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

/// Match a declared parameter type template (e.g. "Map[K, V]") against an
/// actual argument type (e.g. `Applied(Map, [String, Int])`) and record
/// the type-variable bindings (`K -> String, V -> Int`) into `bindings`.
fn bind_type_vars(template: &str, actual: &Ty, bindings: &mut std::collections::HashMap<String, Ty>) {
    use almide_lang::types::constructor::TypeConstructorId as TCI;
    let t = template.trim();

    // Type variable like K, V, T, A, B — single uppercase letter or short
    fn is_type_var(s: &str) -> bool {
        !s.is_empty()
            && s.chars().next().unwrap().is_uppercase()
            && s.len() <= 2
            && s.chars().all(|c| c.is_uppercase() || c.is_ascii_digit())
    }

    if is_type_var(t) {
        bindings.entry(t.to_string()).or_insert_with(|| actual.clone());
        return;
    }

    // Parse `Head[arg1, arg2, ...]` against Applied(Head, [arg1, arg2, ...])
    if let Some(idx) = t.find('[') {
        if t.ends_with(']') {
            let head = &t[..idx];
            let inner = &t[idx+1..t.len()-1];
            let parts = split_top_level_commas(inner);
            let head_tci = match head {
                "List"   => Some(TCI::List),
                "Option" => Some(TCI::Option),
                "Result" => Some(TCI::Result),
                "Map"    => Some(TCI::Map),
                "Set"    => Some(TCI::Set),
                _ => None,
            };
            if let Some(tci) = head_tci {
                if let Ty::Applied(act_tci, act_args) = actual {
                    if std::mem::discriminant(act_tci) == std::mem::discriminant(&tci) {
                        for (sub_template, sub_actual) in parts.iter().zip(act_args.iter()) {
                            bind_type_vars(sub_template, sub_actual, bindings);
                        }
                    }
                }
            }
        }
    }

    // Tuple: `(T1, T2, ...)`
    if t.starts_with('(') && t.ends_with(')') {
        let inner = &t[1..t.len()-1];
        let parts = split_top_level_commas(inner);
        if let Ty::Tuple(act_elems) = actual {
            for (sub_template, sub_actual) in parts.iter().zip(act_elems.iter()) {
                bind_type_vars(sub_template, sub_actual, bindings);
            }
        }
    }
}

/// Split a comma list respecting nested brackets.
fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '[' | '(' | '<' => { depth += 1; cur.push(ch); }
            ']' | ')' | '>' => { depth -= 1; cur.push(ch); }
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            c => cur.push(c),
        }
    }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
    out
}

/// Apply the bindings from `bind_type_vars` to a template `Ty` (as stored
/// in the generated signature table). Returns the substituted type.
fn apply_bindings(ty: &Ty, bindings: &std::collections::HashMap<String, Ty>) -> Ty {
    match ty {
        Ty::TypeVar(name) => {
            if let Some(bound) = bindings.get(name.as_str()) {
                bound.clone()
            } else {
                ty.clone()
            }
        }
        Ty::Applied(tci, args) => {
            let new_args = args.iter().map(|a| apply_bindings(a, bindings)).collect();
            Ty::Applied(tci.clone(), new_args)
        }
        Ty::Tuple(elems) => {
            Ty::Tuple(elems.iter().map(|e| apply_bindings(e, bindings)).collect())
        }
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|p| apply_bindings(p, bindings)).collect(),
            ret: Box::new(apply_bindings(ret, bindings)),
        },
        _ => ty.clone(),
    }
}

/// Substitute TypeVars in `ret_template` using actual arg types matched
/// against `param_templates`. Covers the common generic case:
///   `map.get: (Map[K, V], K) -> Option[V]` with arg types
///   `(Map[String, Int], String)` yields `Option[Int]`.
fn substitute_type_vars(ret_template: &Ty, param_templates: &[&str], args: &[IrExpr]) -> Ty {
    let mut bindings = std::collections::HashMap::new();
    for (template, arg) in param_templates.iter().zip(args.iter()) {
        bind_type_vars(template, &arg.ty, &mut bindings);
    }
    apply_bindings(ret_template, &bindings)
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
                );
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
