//! Lambda/closure pre-scan and compilation for WASM codegen.
//!
//! Scans all function bodies for Lambda and FnRef nodes, registers them
//! in the emitter, and compiles their bodies and FnRef wrappers.

use std::collections::{HashMap, HashSet};
use wasm_encoder::ValType;

use almide_ir::{IrExpr, IrExprKind, IrProgram, IrStmtKind, VarId};
use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use almide_lang::types::Ty;

use super::{CompiledFunc, FuncCompiler, LambdaInfo, WasmEmitter};
use super::values;
use super::statements;

/// Walk all function bodies to find Lambda and FnRef nodes.
/// Register lambda functions and FnRef wrappers in the emitter.
pub(super) fn pre_scan_closures(program: &IrProgram, emitter: &mut WasmEmitter) {
    // Collect all lambdas (in tree-walk order)
    let mut lambda_exprs: Vec<(Vec<(VarId, almide_lang::types::Ty)>, IrExpr, Vec<u32>, Option<u32>)> = Vec::new();
    let mut fn_ref_set: HashSet<String> = HashSet::new();

    let mut mutable_vars: HashSet<u32> = HashSet::new();

    for func in &program.functions {
        let scope_vars: HashSet<u32> = func.params.iter().map(|p| p.var.0).collect();
        scan_closures(&func.body, scope_vars, &mut mutable_vars, &mut lambda_exprs, &mut fn_ref_set);
    }
    // BFS: scan lambda bodies for nested lambdas (repeat until no new lambdas found)
    let mut scan_start = 0;
    loop {
        let current_len = lambda_exprs.len();
        if scan_start >= current_len { break; }
        for i in scan_start..current_len {
            let body = lambda_exprs[i].1.clone();
            let params = &lambda_exprs[i].0;
            let captures = &lambda_exprs[i].2;
            let mut inner_scope: HashSet<u32> = params.iter().map(|(vid, _)| vid.0).collect();
            for &vid in captures { inner_scope.insert(vid); }
            scan_closures(&body, inner_scope, &mut mutable_vars, &mut lambda_exprs, &mut fn_ref_set);
        }
        scan_start = current_len;
    }

    // Build ordered fn_ref list (sorted for determinism)
    let mut fn_ref_names: Vec<String> = fn_ref_set.into_iter().collect();
    fn_ref_names.sort();

    // Register each lambda as a function
    for (params, _body, captures, lid) in &lambda_exprs {
        // Closure calling convention: (env: i32, declared_params...) -> ret
        let mut wasm_params = vec![ValType::I32]; // env_ptr
        for (vid, ty) in params {
            let resolved_ty = resolve_lambda_param_ty(ty, &_body.ty, &program.var_table, *vid);
            if let Some(vt) = values::ty_to_valtype(&resolved_ty) {
                wasm_params.push(vt);
            }
        }
        // Resolve body return type: if Unknown, infer from expression tree + VarTable
        let body_ret_ty = if matches!(&_body.ty, almide_lang::types::Ty::Unknown | almide_lang::types::Ty::TypeVar(_)) {
            resolve_expr_ty(_body, &program.var_table, &emitter.record_fields)
        } else {
            _body.ty.clone()
        };
        let ret_types = values::ret_type(&body_ret_ty);
        let closure_type_idx = emitter.register_type(wasm_params, ret_types);

        let name = format!("__lambda_{}", emitter.lambdas.len());
        let func_idx = emitter.register_func(&name, closure_type_idx);
        let table_idx = emitter.func_table.len() as u32;
        emitter.func_table.push(func_idx);
        emitter.func_to_table_idx.insert(func_idx, table_idx);

        let capture_vars: Vec<(VarId, almide_lang::types::Ty)> = captures.iter()
            .map(|&vid| {
                let info = &program.var_table.get(VarId(vid));
                // Track mutable variables that are captured by closures
                if mutable_vars.contains(&vid) {
                    emitter.mutable_captures.insert(vid);
                }
                (VarId(vid), info.ty.clone())
            })
            .collect();

        let param_ids: Vec<u32> = params.iter().map(|(vid, _)| vid.0).collect();
        emitter.lambdas.push(LambdaInfo {
            table_idx,
            closure_type_idx,
            captures: capture_vars,
            param_ids,
            lambda_id: *lid,
        });
    }

    // Register FnRef wrappers
    for fn_name in &fn_ref_names {
        if emitter.fn_ref_wrappers.contains_key(fn_name.as_str()) { continue; }
        if let Some(&orig_func_idx) = emitter.func_map.get(fn_name.as_str()) {
            if let Some(&orig_type_idx) = emitter.func_type_indices.get(&orig_func_idx) {
                // Get original params/results
                let (orig_params, orig_results) = emitter.types[orig_type_idx as usize].clone();
                // Wrapper type: (env: i32, original_params...) -> original_results
                let mut wrapper_params = vec![ValType::I32];
                wrapper_params.extend_from_slice(&orig_params);
                let wrapper_type_idx = emitter.register_type(wrapper_params, orig_results);

                let wrapper_name = format!("__wrap_{}", fn_name);
                let wrapper_func_idx = emitter.register_func(&wrapper_name, wrapper_type_idx);
                let table_idx = emitter.func_table.len() as u32;
                emitter.func_table.push(wrapper_func_idx);
                emitter.func_to_table_idx.insert(wrapper_func_idx, table_idx);

                emitter.fn_ref_wrappers.insert(fn_name.clone(), table_idx);
            }
        }
    }
}

/// Compile lambda bodies and FnRef wrappers.
pub(super) fn compile_lambda_bodies(program: &IrProgram, emitter: &mut WasmEmitter) {
    // Re-scan to get lambda bodies (in same order as pre-scan)
    let mut lambda_exprs: Vec<(Vec<(VarId, almide_lang::types::Ty)>, IrExpr, Vec<u32>, Option<u32>)> = Vec::new();
    let mut fn_ref_set: HashSet<String> = HashSet::new();
    let mut mutable_vars: HashSet<u32> = HashSet::new();

    for func in &program.functions {
        let scope_vars: HashSet<u32> = func.params.iter().map(|p| p.var.0).collect();
        scan_closures(&func.body, scope_vars, &mut mutable_vars, &mut lambda_exprs, &mut fn_ref_set);
    }
    // BFS: scan lambda bodies for nested lambdas
    let mut scan_start = 0;
    loop {
        let current_len = lambda_exprs.len();
        if scan_start >= current_len { break; }
        for i in scan_start..current_len {
            let body = lambda_exprs[i].1.clone();
            let params = &lambda_exprs[i].0;
            let captures = &lambda_exprs[i].2;
            let mut inner_scope: HashSet<u32> = params.iter().map(|(vid, _)| vid.0).collect();
            for &vid in captures { inner_scope.insert(vid); }
            scan_closures(&body, inner_scope, &mut mutable_vars, &mut lambda_exprs, &mut fn_ref_set);
        }
        scan_start = current_len;
    }
    let mut fn_ref_names: Vec<String> = fn_ref_set.into_iter().collect();
    fn_ref_names.sort();

    // Compile each lambda
    for (i, (params, body, captures, _lid)) in lambda_exprs.iter().enumerate() {
        let info = &emitter.lambdas[i];
        let type_idx = info.closure_type_idx;

        // Build var_map: env_ptr is local 0, params start at 1
        let mut var_map: HashMap<u32, u32> = HashMap::new();
        let mut local_idx = 1u32; // 0 = env_ptr
        for (vid, _) in params {
            var_map.insert(vid.0, local_idx);
            local_idx += 1;
        }

        // Captured vars are loaded from env in the body emission
        // Map them to locals allocated after params
        let capture_list: Vec<(VarId, almide_lang::types::Ty)> = captures.iter()
            .map(|&vid| {
                let vi = program.var_table.get(VarId(vid));
                (VarId(vid), vi.ty.clone())
            })
            .collect();

        // Pre-scan body for additional locals
        let scan = statements::collect_locals(body, &program.var_table, &emitter.record_fields);
        let mut local_decls = Vec::new();

        // Captured var locals
        for (vid, ty) in &capture_list {
            let is_cell = emitter.mutable_captures.contains(&vid.0);
            if is_cell {
                // Mutable capture: local holds cell ptr (i32)
                var_map.insert(vid.0, local_idx);
                local_decls.push((1u32, ValType::I32));
                local_idx += 1;
            } else if let Some(vt) = values::ty_to_valtype(ty) {
                var_map.insert(vid.0, local_idx);
                local_decls.push((1u32, vt));
                local_idx += 1;
            }
        }

        // Body bind locals
        for (vid, vt) in &scan.binds {
            var_map.insert(vid.0, local_idx);
            local_decls.push((1u32, *vt));
            local_idx += 1;
        }

        // ScratchAllocator locals
        let scratch_i32_cap = 32usize;
        let scratch_i64_cap = 16usize;
        let scratch_f64_cap = 4usize;
        let scratch_i32_base = local_idx;
        for _ in 0..scratch_i32_cap { local_decls.push((1, ValType::I32)); local_idx += 1; }
        let scratch_i64_base = local_idx;
        for _ in 0..scratch_i64_cap { local_decls.push((1, ValType::I64)); local_idx += 1; }
        let scratch_f64_base = local_idx;
        for _ in 0..scratch_f64_cap { local_decls.push((1, ValType::F64)); local_idx += 1; }

        let mut wasm_func = wasm_encoder::Function::new(local_decls);

        // Load captured vars from env
        for (ci, (vid, ty)) in capture_list.iter().enumerate() {
            let is_cell = emitter.mutable_captures.contains(&vid.0);
            if is_cell {
                // Mutable capture: env stores cell ptr (i32). Load as i32.
                let cap_local = var_map[&vid.0];
                let offset = ci as u32 * 8;
                wasm_func.instruction(&wasm_encoder::Instruction::LocalGet(0));
                wasm_func.instruction(&wasm_encoder::Instruction::I32Load(
                    wasm_encoder::MemArg { offset: offset as u64, align: 2, memory_index: 0 }
                ));
                wasm_func.instruction(&wasm_encoder::Instruction::LocalSet(cap_local));
            } else if let Some(vt) = values::ty_to_valtype(ty) {
                let cap_local = var_map[&vid.0];
                let offset = ci as u32 * 8;
                wasm_func.instruction(&wasm_encoder::Instruction::LocalGet(0));
                match vt {
                    ValType::I64 => {
                        wasm_func.instruction(&wasm_encoder::Instruction::I64Load(
                            wasm_encoder::MemArg { offset: offset as u64, align: 3, memory_index: 0 }
                        ));
                    }
                    ValType::F64 => {
                        wasm_func.instruction(&wasm_encoder::Instruction::F64Load(
                            wasm_encoder::MemArg { offset: offset as u64, align: 3, memory_index: 0 }
                        ));
                    }
                    _ => {
                        wasm_func.instruction(&wasm_encoder::Instruction::I32Load(
                            wasm_encoder::MemArg { offset: offset as u64, align: 2, memory_index: 0 }
                        ));
                    }
                }
                wasm_func.instruction(&wasm_encoder::Instruction::LocalSet(cap_local));
            }
        }

        // Compile body
        let compiled_func = {
            let mut scratch_alloc = super::scratch::ScratchAllocator::new();
            scratch_alloc.set_bases_with_capacity(scratch_i32_base, scratch_i32_cap, scratch_i64_base, scratch_i64_cap, scratch_f64_base, scratch_f64_cap);
            let mut compiler = FuncCompiler {
                emitter: &mut *emitter,
                func: wasm_func,
                var_map,
                depth: 0,
                loop_stack: Vec::new(),
                scratch: scratch_alloc,
                var_table: &program.var_table,
                stub_ret_ty: Ty::Unit,
            };
            compiler.emit_expr(body);
            compiler.func.instruction(&wasm_encoder::Instruction::End);
            compiler.func
        };

        emitter.add_compiled(CompiledFunc { type_idx, func: compiled_func });
    }

    // Compile FnRef wrappers
    fn_ref_names.sort(); // deterministic order
    for fn_name in &fn_ref_names {
        if let Some(&orig_func_idx) = emitter.func_map.get(fn_name.as_str()) {
            if let Some(&orig_type_idx) = emitter.func_type_indices.get(&orig_func_idx) {
                let (orig_params, orig_results) = emitter.types[orig_type_idx as usize].clone();
                // Wrapper: (env: i32, params...) -> results  { call original(params...) }
                let mut wrapper_params = vec![ValType::I32];
                wrapper_params.extend_from_slice(&orig_params);
                let wrapper_type_idx = emitter.register_type(wrapper_params, orig_results);

                let mut f = wasm_encoder::Function::new([]);
                // Skip env (local 0), pass remaining params to original
                for i in 0..orig_params.len() {
                    f.instruction(&wasm_encoder::Instruction::LocalGet((i + 1) as u32));
                }
                f.instruction(&wasm_encoder::Instruction::Call(orig_func_idx));
                f.instruction(&wasm_encoder::Instruction::End);

                emitter.add_compiled(CompiledFunc { type_idx: wrapper_type_idx, func: f });
            }
        }
    }
}

// ── ClosureScanner: IrVisitor-based Lambda/FnRef collector ──────────
//
// Uses the shared `walk_expr`/`walk_stmt` from ir::visit for exhaustive
// traversal. Only overrides visit_expr/visit_stmt where custom logic is needed:
// - Lambda: collect captures, do NOT recurse (BFS second pass handles nesting)
// - FnRef: collect name
// - ForIn: insert loop vars into scope before visiting body
// - Bind/BindDestructure: insert bound vars into scope after visiting value

struct ClosureScanner<'a> {
    scope_vars: HashSet<u32>,
    mutable_vars: &'a mut HashSet<u32>,
    lambdas: &'a mut Vec<(Vec<(VarId, Ty)>, IrExpr, Vec<u32>, Option<u32>)>,
    fn_refs: &'a mut HashSet<String>,
}

impl IrVisitor for ClosureScanner<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Lambda { params, body, lambda_id } => {
                let param_ids: HashSet<u32> = params.iter().map(|(vid, _)| vid.0).collect();
                let mut body_vars = HashSet::new();
                collect_var_refs(body, &mut body_vars);
                let mut captures: Vec<u32> = body_vars.difference(&param_ids)
                    .copied()
                    .filter(|vid| self.scope_vars.contains(vid))
                    .collect();
                captures.sort();
                let param_list: Vec<(VarId, Ty)> = params.iter()
                    .map(|(vid, ty)| (*vid, ty.clone()))
                    .collect();
                self.lambdas.push((param_list, *body.clone(), captures, *lambda_id));
                // Do NOT recurse — nested lambdas scanned in BFS second pass
            }
            IrExprKind::FnRef { name } => {
                self.fn_refs.insert(name.to_string());
            }
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.visit_expr(iterable);
                self.scope_vars.insert(var.0);
                if let Some(vt) = var_tuple { for v in vt { self.scope_vars.insert(v.0); } }
                for stmt in body { self.visit_stmt(stmt); }
            }
            _ => walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
        match &stmt.kind {
            IrStmtKind::Bind { var, mutability, value, .. } => {
                self.visit_expr(value);
                self.scope_vars.insert(var.0);
                if *mutability == almide_ir::Mutability::Var {
                    self.mutable_vars.insert(var.0);
                }
            }
            IrStmtKind::BindDestructure { pattern, value, .. } => {
                self.visit_expr(value);
                collect_pattern_var_ids(pattern, &mut self.scope_vars);
            }
            _ => walk_stmt(self, stmt),
        }
    }
}

fn scan_closures(
    expr: &IrExpr,
    scope_vars: HashSet<u32>,
    mutable_vars: &mut HashSet<u32>,
    lambdas: &mut Vec<(Vec<(VarId, Ty)>, IrExpr, Vec<u32>, Option<u32>)>,
    fn_refs: &mut HashSet<String>,
) {
    let mut scanner = ClosureScanner { scope_vars, mutable_vars, lambdas, fn_refs };
    scanner.visit_expr(expr);
}

/// Collect all VarIds bound by an IrPattern into a set.
fn collect_pattern_var_ids(pattern: &almide_ir::IrPattern, out: &mut HashSet<u32>) {
    use almide_ir::IrPattern;
    match pattern {
        IrPattern::Bind { var, .. } => { out.insert(var.0); }
        IrPattern::Constructor { args, .. } => { for a in args { collect_pattern_var_ids(a, out); } }
        IrPattern::Tuple { elements } => { for e in elements { collect_pattern_var_ids(e, out); } }
        IrPattern::Some { inner, .. } | IrPattern::Ok { inner, .. } | IrPattern::Err { inner, .. } => {
            collect_pattern_var_ids(inner, out);
        }
        IrPattern::RecordPattern { fields, .. } => {
            for f in fields { if let Some(p) = &f.pattern { collect_pattern_var_ids(p, out); } }
        }
        _ => {}
    }
}

/// Resolve a lambda parameter type when it's TypeVar or Unknown.
fn resolve_lambda_param_ty(param_ty: &almide_lang::types::Ty, _body_ty: &almide_lang::types::Ty, var_table: &almide_ir::VarTable, vid: VarId) -> almide_lang::types::Ty {
    match param_ty {
        almide_lang::types::Ty::TypeVar(_) | almide_lang::types::Ty::Unknown | almide_lang::types::Ty::OpenRecord { .. } => {
            // Try VarTable for the resolved type
            if (vid.0 as usize) < var_table.len() {
                let info = var_table.get(vid);
                if !matches!(&info.ty, almide_lang::types::Ty::TypeVar(_) | almide_lang::types::Ty::Unknown | almide_lang::types::Ty::OpenRecord { .. }) {
                    return info.ty.clone();
                }
            }
            // Fallback: default to Int. This matches the most common case (numeric).
            // For non-numeric types (String, List, etc.), the caller must resolve
            // the type from call context (e.g., list element type, fn signature).
            almide_lang::types::Ty::Int
        }
        _ => param_ty.clone(),
    }
}

/// Resolve the effective type of an expression tree, using VarTable for Var references
/// and record_fields from the emitter for Member accesses.
/// This is needed because lambda body expressions may have Unknown type from the type
/// checker when the lambda is inside a generic function.
pub(super) fn resolve_expr_ty(expr: &IrExpr, var_table: &almide_ir::VarTable, record_fields: &HashMap<String, Vec<(String, almide_lang::types::Ty)>>) -> almide_lang::types::Ty {
    use almide_lang::types::Ty;
    // If the expression already has a concrete type, use it
    if !matches!(&expr.ty, Ty::Unknown | Ty::TypeVar(_)) {
        return expr.ty.clone();
    }
    match &expr.kind {
        IrExprKind::Var { id } => {
            if (id.0 as usize) < var_table.len() {
                let info = var_table.get(*id);
                if !matches!(&info.ty, Ty::Unknown | Ty::TypeVar(_)) {
                    return info.ty.clone();
                }
            }
            expr.ty.clone()
        }
        IrExprKind::Member { object, field } => {
            let obj_ty = resolve_expr_ty(object, var_table, record_fields);
            match &obj_ty {
                Ty::Record { fields } | Ty::OpenRecord { fields } => {
                    if let Some((_, fty)) = fields.iter().find(|(n, _)| n == field) {
                        return fty.clone();
                    }
                }
                Ty::Named(name, _) => {
                    if let Some(fields) = record_fields.get(name.as_str()) {
                        if let Some((_, fty)) = fields.iter().find(|(n, _)| n == field) {
                            return fty.clone();
                        }
                    }
                }
                _ => {
                    // Unknown object type: search record_fields for a type that has this field
                    for (_name, fields) in record_fields {
                        if let Some((_, fty)) = fields.iter().find(|(n, _)| n == field) {
                            return fty.clone();
                        }
                    }
                }
            }
            expr.ty.clone()
        }
        IrExprKind::TupleIndex { object, index } => {
            let obj_ty = resolve_expr_ty(object, var_table, record_fields);
            if let Ty::Tuple(elems) = &obj_ty {
                if let Some(t) = elems.get(*index as usize) {
                    return t.clone();
                }
            }
            expr.ty.clone()
        }
        IrExprKind::If { then, .. } => resolve_expr_ty(then, var_table, record_fields),
        IrExprKind::Match { arms, .. } => {
            // Resolve from the first arm's body
            if let Some(arm) = arms.first() {
                let resolved = resolve_expr_ty(&arm.body, var_table, record_fields);
                if !matches!(&resolved, Ty::Unknown | Ty::TypeVar(_)) {
                    return resolved;
                }
            }
            expr.ty.clone()
        }
        IrExprKind::Block { expr: Some(e), .. } => {
            resolve_expr_ty(e, var_table, record_fields)
        }
        _ => expr.ty.clone(),
    }
}

// ── VarRefCollector: IrVisitor-based variable reference collector ────
//
// Collects all VarIds referenced in an expression tree (including inside
// nested lambdas). Uses walk_expr for exhaustive traversal.

struct VarRefCollector<'a> {
    vars: &'a mut HashSet<u32>,
}

impl IrVisitor for VarRefCollector<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExprKind::Var { id } = &expr.kind {
            self.vars.insert(id.0);
        }
        walk_expr(self, expr);
    }
}

pub(super) fn collect_var_refs(expr: &IrExpr, vars: &mut HashSet<u32>) {
    VarRefCollector { vars }.visit_expr(expr);
}
