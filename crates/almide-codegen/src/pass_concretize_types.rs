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
fn erase_wasm_type_aliases(program: &mut IrProgram, _target: Target) {
    let mut aliases: HashMap<String, Ty> = HashMap::new();
    for td in program.type_decls.iter().chain(program.modules.iter().flat_map(|m| m.type_decls.iter())) {
        if let almide_ir::IrTypeDeclKind::Alias { target: alias_target } = &td.kind {
            aliases.insert(td.name.to_string(), alias_target.clone());
        }
    }
    // (The Target::Wasm alias-check branch left with the retired pipeline, #930.)
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
/// Insert `name` → `ty` under both its source spelling and its
/// SCREAMING_CASE const spelling. The use-site synthetic Var carries the
/// SCREAMING_CASE spelling (`lower/expressions.rs` `field.to_uppercase()`)
/// while the definition keeps the source name — bridge BOTH spellings, or
/// a lowercase module top-let never propagates (#502 fix C). Extracted
/// from `propagate_top_let_types_by_name` (cog>25 decomposition): was a
/// local closure, promoted to a named function.
fn insert_top_let_ty_both_spellings(name: String, ty: &Ty, map: &mut std::collections::HashMap<String, Ty>) {
    let upper = name.to_uppercase();
    if upper != name { map.entry(upper).or_insert_with(|| ty.clone()); }
    map.insert(name, ty.clone());
}

/// Collect every resolved top-let's type (top-level and per-module), keyed
/// by name (both spellings — see [`insert_top_let_ty_both_spellings`]).
/// Extracted from `propagate_top_let_types_by_name` (cog>25 decomposition).
fn collect_top_let_types(program: &IrProgram, prog_vt: &VarTable) -> std::collections::HashMap<String, Ty> {
    let mut top_let_types: std::collections::HashMap<String, Ty> = std::collections::HashMap::new();
    for tl in &program.top_lets {
        if !tl.ty.has_unresolved_deep() {
            let name = prog_vt.get(tl.var).name.to_string();
            insert_top_let_ty_both_spellings(name, &tl.ty, &mut top_let_types);
        }
    }
    for module in &program.modules {
        for tl in &module.top_lets {
            if !tl.ty.has_unresolved_deep() {
                let name = prog_vt.get(tl.var).name.to_string();
                insert_top_let_ty_both_spellings(name, &tl.ty, &mut top_let_types);
            }
        }
    }
    top_let_types
}

fn propagate_top_let_types_by_name(program: &IrProgram, prog_vt: &mut VarTable) {
    let top_let_types = collect_top_let_types(program, prog_vt);
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

include!("pass_concretize_types_signatures.rs");
include!("pass_concretize_types_walker.rs");
include!("pass_concretize_types_call_ret.rs");
include!("pass_concretize_types_unresolved.rs");
