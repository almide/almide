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

/// #644: true iff a function (top-level when `module` is None, else a module fn)
/// is reachable from the entry surface. Mirrors the gate in `emit_wasm::emit`,
/// using the SAME `registered_keys` spellings so the closure scanners and the
/// body-compile loops agree on exactly which functions are live.
fn fn_reachable(reachable: &HashSet<String>, module: Option<&str>, fname: &str) -> bool {
    super::reachability::registered_keys(module, fname)
        .iter()
        .any(|k| reachable.contains(k))
}

/// Walk all function bodies to find Lambda and FnRef nodes.
/// Register lambda functions and FnRef wrappers in the emitter.
pub(super) fn pre_scan_closures(
    program: &IrProgram,
    emitter: &mut WasmEmitter,
    reachable_fns: &HashSet<String>,
) {
    // Collect all lambdas (in tree-walk order)
    let mut lambda_exprs: Vec<(Vec<(VarId, almide_lang::types::Ty)>, IrExpr, Vec<u32>, Option<u32>)> = Vec::new();
    let mut fn_ref_set: HashSet<String> = HashSet::new();

    let mut mutable_vars: HashSet<u32> = HashSet::new();
    // Seed with captured-and-mutated vars detected before closure conversion
    // (ClosureConversionPass → `shared_mut_vars`). Their references are now
    // `EnvLoad`s, so scanning the closure body can't recover the original VarId,
    // but those captures must still become shared heap cells. Covers a non-Copy
    // var mutated only via a method (`list.push`), recorded `Mutability::Let`
    // since it is never reassigned. (Closure v2 P6.)
    for v in &program.codegen_annotations.shared_mut_vars {
        mutable_vars.insert(v.0);
    }

    for func in &program.functions {
        // #644: a dead function's lambdas are dead too — skip them so their
        // bodies (which may hit a native-only intrinsic) are never compiled.
        // `compile_lambda_bodies` applies the identical filter → index alignment.
        if !fn_reachable(reachable_fns, None, func.name.as_str()) { continue; }
        let scope_vars: HashSet<u32> = func.params.iter().map(|p| p.var.0).collect();
        scan_closures(&func.body, scope_vars, &mut mutable_vars, &mut lambda_exprs, &mut fn_ref_set);
    }
    // Module functions also carry raw Lambdas (e.g. a submodule combinator
    // factory returning a non-capturing lambda). They are NOT flattened into
    // program.functions on the WASM path, so without this scan they get no
    // LambdaInfo / table slot, and a call site resolves to the wrong function
    // (or none) — the cross-module closure bug. Order: functions then modules,
    // IDENTICAL to compile_lambda_bodies so the positional index aligns.
    // (Closure v2, P0.)
    for module in &program.modules {
        let mod_name = module.name.to_string();
        for func in &module.functions {
            if !fn_reachable(reachable_fns, Some(&mod_name), func.name.as_str()) { continue; }
            let scope_vars: HashSet<u32> = func.params.iter().map(|p| p.var.0).collect();
            scan_closures(&func.body, scope_vars, &mut mutable_vars, &mut lambda_exprs, &mut fn_ref_set);
        }
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
            let resolved_ty = resolve_lambda_param_ty(ty, _body, &program.var_table, *vid);
            if let Some(vt) = values::ty_to_valtype(&resolved_ty) {
                wasm_params.push(vt);
            }
        }
        // Body return type: trust `.ty` set by ConcretizeTypes.
        let body_ret_ty = _body.ty.clone();
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

    // Register lifted closure functions (from ClosureConversion pass) in the
    // function table. After ClosureConversion, Lambda nodes become ClosureCreate
    // nodes referencing lifted __closure_N functions. These functions must be in
    // the table so call_indirect can dispatch them.
    let mut closure_create_set: HashSet<String> = HashSet::new();
    for func in &program.functions {
        if !fn_reachable(reachable_fns, None, func.name.as_str()) { continue; }
        collect_closure_creates(&func.body, &mut closure_create_set);
    }
    for module in &program.modules {
        let mod_name = module.name.to_string();
        for func in &module.functions {
            if !fn_reachable(reachable_fns, Some(&mod_name), func.name.as_str()) { continue; }
            collect_closure_creates(&func.body, &mut closure_create_set);
        }
    }
    // Sort before assigning function-table slots: HashSet iteration order is
    // host-dependent (hash seed + usize-width bucket layout), which made the
    // compiler emit different `call_indirect` table indices when run on a
    // 32-bit host (wasm32, the browser playground) vs a 64-bit host — a valid
    // but divergent module that traps with `unreachable`. Sorting makes the
    // table layout a pure function of the program (mirrors fn_ref_names.sort()).
    let mut closure_create_names: Vec<String> = closure_create_set.into_iter().collect();
    closure_create_names.sort();
    for name in &closure_create_names {
        if let Some(&func_idx) = emitter.func_map.get(name.as_str()) {
            if !emitter.func_to_table_idx.contains_key(&func_idx) {
                let table_idx = emitter.func_table.len() as u32;
                emitter.func_table.push(func_idx);
                emitter.func_to_table_idx.insert(func_idx, table_idx);
            }
        }
    }
}

/// Compile lambda bodies and FnRef wrappers.
pub(super) fn compile_lambda_bodies(
    program: &IrProgram,
    emitter: &mut WasmEmitter,
    reachable_fns: &HashSet<String>,
) {
    // Re-scan to get lambda bodies (in same order as pre-scan)
    let mut lambda_exprs: Vec<(Vec<(VarId, almide_lang::types::Ty)>, IrExpr, Vec<u32>, Option<u32>)> = Vec::new();
    let mut fn_ref_set: HashSet<String> = HashSet::new();
    let mut mutable_vars: HashSet<u32> = HashSet::new();

    for func in &program.functions {
        // #644: identical reachable-fn filter to pre_scan_closures — dead
        // functions contribute no lambdas, so emitter.lambdas[i] stays aligned.
        if !fn_reachable(reachable_fns, None, func.name.as_str()) { continue; }
        let scope_vars: HashSet<u32> = func.params.iter().map(|p| p.var.0).collect();
        scan_closures(&func.body, scope_vars, &mut mutable_vars, &mut lambda_exprs, &mut fn_ref_set);
    }
    // Module functions, IDENTICAL order to pre_scan_closures (functions then
    // modules) so emitter.lambdas[i] lines up with the pre-scan registration.
    // (Closure v2, P0.)
    for module in &program.modules {
        let mod_name = module.name.to_string();
        for func in &module.functions {
            if !fn_reachable(reachable_fns, Some(&mod_name), func.name.as_str()) { continue; }
            let scope_vars: HashSet<u32> = func.params.iter().map(|p| p.var.0).collect();
            scan_closures(&func.body, scope_vars, &mut mutable_vars, &mut lambda_exprs, &mut fn_ref_set);
        }
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
        let scan = statements::collect_locals(body, &program.var_table, &emitter.record_fields, &emitter.variant_info);
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

        // ScratchAllocator locals — generous fixed caps, see functions.rs note (#417).
        let scratch_i32_cap = 64usize;
        let scratch_i64_cap = 48usize;
        let scratch_f64_cap = 48usize;
        let scratch_i32_base = local_idx;
        for _ in 0..scratch_i32_cap { local_decls.push((1, ValType::I32)); local_idx += 1; }
        let scratch_i64_base = local_idx;
        for _ in 0..scratch_i64_cap { local_decls.push((1, ValType::I64)); local_idx += 1; }
        let scratch_f64_base = local_idx;
        for _ in 0..scratch_f64_cap { local_decls.push((1, ValType::F64)); local_idx += 1; }
        let scratch_v128_cap = 8usize;
        let scratch_v128_base = local_idx;
        for _ in 0..scratch_v128_cap { local_decls.push((1, ValType::V128)); local_idx += 1; }

        let mut wasm_func = super::TrackedFunction::new(local_decls);

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
            scratch_alloc.set_v128_base_with_capacity(scratch_v128_base, scratch_v128_cap);
            let mut compiler = FuncCompiler {
                emitter: &mut *emitter,
                func: wasm_func,
                var_map,
                depth: 0,
                loop_stack: Vec::new(),
                scratch: scratch_alloc,
                var_table: &program.var_table,
                stub_ret_ty: Ty::Unit,
                current_module_name: None,
                live_heap: Vec::new(),
            };
            compiler.emit_expr(body);
            compiler.func.instruction(&wasm_encoder::Instruction::End);
            compiler.func
        };

        emitter.add_compiled(CompiledFunc::tracked(type_idx, compiled_func));
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

                let mut f = super::TrackedFunction::new([]);
                // Skip env (local 0), pass remaining params to original
                for i in 0..orig_params.len() {
                    f.instruction(&wasm_encoder::Instruction::LocalGet((i + 1) as u32));
                }
                f.instruction(&wasm_encoder::Instruction::Call(orig_func_idx));
                f.instruction(&wasm_encoder::Instruction::End);

                emitter.add_compiled(CompiledFunc::tracked(wrapper_type_idx, f));
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
                // Captures via the single shared analysis (almide_ir::free_vars),
                // intersected with the enclosing scope as a safety net. This is
                // provably identical to the prior `(flat-refs − params) ∩ scope`:
                // free_vars already excludes the lambda's own params and any
                // body-local bindings (it is scope-tracking), and `∩ scope_vars`
                // is what removed those body-locals before. free_vars returns a
                // VarId-sorted Vec, so the env layout stays deterministic.
                // (Closure v2, P1 — one capture analysis.)
                let param_set: HashSet<VarId> = params.iter().map(|(vid, _)| *vid).collect();
                let captures: Vec<u32> = almide_ir::free_vars::free_vars(body, &param_set)
                    .into_iter()
                    .map(|vid| vid.0)
                    .filter(|vid| self.scope_vars.contains(vid))
                    .collect();
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
fn resolve_lambda_param_ty(
    param_ty: &almide_lang::types::Ty,
    body: &almide_ir::IrExpr,
    var_table: &almide_ir::VarTable,
    vid: VarId,
) -> almide_lang::types::Ty {
    // 1. VarTable
    if (vid.0 as usize) < var_table.len() {
        let info = var_table.get(vid);
        if !info.ty.is_unresolved() {
            return info.ty.clone();
        }
    }
    // 2. IR annotation
    if !param_ty.is_unresolved() {
        return param_ty.clone();
    }
    // 3. Infer from body usage (e.g. string interp → String)
    if let Some(inferred) = crate::pass_concretize_types::infer_var_type_from_body(body, vid) {
        return inferred;
    }
    // 4. Body return type as fallback (fold accumulator pattern)
    if !body.ty.is_unresolved() && !matches!(body.ty, almide_lang::types::Ty::Unit | almide_lang::types::Ty::Bool) {
        return body.ty.clone();
    }
    almide_lang::types::Ty::Int
}

/// Collect all ClosureCreate func_names referenced in an expression tree.
fn collect_closure_creates(expr: &IrExpr, names: &mut HashSet<String>) {
    struct Collector<'a> { names: &'a mut HashSet<String> }
    impl IrVisitor for Collector<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::ClosureCreate { func_name, .. } = &expr.kind {
                self.names.insert(func_name.to_string());
            }
            walk_expr(self, expr);
        }
    }
    Collector { names }.visit_expr(expr);
}
