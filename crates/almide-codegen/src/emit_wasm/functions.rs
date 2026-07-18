//! IrFunction → compiled WASM function.

use std::collections::HashMap;
use wasm_encoder::{Function, ValType};

use almide_ir::{IrFunction, IrExpr, IrExprKind, IrStmt, IrStmtKind, VarTable};

use super::{CompiledFunc, FuncCompiler, WasmEmitter};
use super::statements::collect_locals;

/// Check if a function body uses stdlib calls, closures, or complex operations
/// that require the full scratch local pool.
fn body_needs_scratch(body: &IrExpr) -> bool {
    // Conservative: only return false for leaf expressions that definitely
    // don't use scratch locals. Everything else gets full scratch capacity.
    match &body.kind {
        // Leaves: no scratch needed
        IrExprKind::Var { .. } | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. }
        | IrExprKind::LitBool { .. } | IrExprKind::LitStr { .. } | IrExprKind::Unit => false,
        // Simple operations: recurse into children
        IrExprKind::BinOp { left, right, .. } => body_needs_scratch(left) || body_needs_scratch(right),
        IrExprKind::UnOp { operand, .. } => body_needs_scratch(operand),
        IrExprKind::If { cond, then, else_ } => {
            body_needs_scratch(cond) || body_needs_scratch(then) || body_needs_scratch(else_)
        }
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().any(|s| stmt_needs_scratch(s)) || expr.as_ref().is_some_and(|e| body_needs_scratch(e))
        }
        IrExprKind::While { cond, body: stmts } => {
            body_needs_scratch(cond) || stmts.iter().any(|s| stmt_needs_scratch(s))
        }
        // Direct function calls: only need scratch if args do
        IrExprKind::Call { target: almide_ir::CallTarget::Named { .. }, args, .. }
        | IrExprKind::TailCall { target: almide_ir::CallTarget::Named { .. }, args } => {
            args.iter().any(|a| body_needs_scratch(a))
        }
        // Everything else: conservatively assume scratch needed
        _ => true,
    }
}

fn stmt_needs_scratch(stmt: &IrStmt) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => body_needs_scratch(value),
        IrStmtKind::Assign { value, .. } => body_needs_scratch(value),
        IrStmtKind::Expr { expr } => body_needs_scratch(expr),
        _ => false,
    }
}

/// Compile an IR function into a WASM function body.
pub fn compile_function(
    emitter: &mut WasmEmitter,
    func: &IrFunction,
    _var_table: &VarTable,
    type_idx: u32,
) -> CompiledFunc {
    compile_function_inner(emitter, func, _var_table, type_idx, None, None)
}

/// Compile a module function with module-name context for intra-module call resolution.
pub fn compile_module_function(
    emitter: &mut WasmEmitter,
    func: &IrFunction,
    _var_table: &VarTable,
    type_idx: u32,
    module_name: &str,
) -> CompiledFunc {
    compile_function_inner(emitter, func, _var_table, type_idx, None, Some(module_name.to_string()))
}

pub fn compile_function_with_init(
    emitter: &mut WasmEmitter,
    func: &IrFunction,
    _var_table: &VarTable,
    type_idx: u32,
    init_globals_idx: Option<u32>,
) -> CompiledFunc {
    compile_function_inner(emitter, func, _var_table, type_idx, init_globals_idx, None)
}

fn compile_function_inner(
    emitter: &mut WasmEmitter,
    func: &IrFunction,
    _var_table: &VarTable,
    type_idx: u32,
    init_globals_idx: Option<u32>,
    module_name: Option<String>,
) -> CompiledFunc {
    let param_count = func.params.len() as u32;

    // Map parameters to WASM local indices 0..N-1
    let mut var_map: HashMap<u32, u32> = HashMap::new();
    for (i, param) in func.params.iter().enumerate() {
        var_map.insert(param.var.0, i as u32);
    }

    // Pre-scan body for variable bindings and match scratch requirements
    let scan = collect_locals(&func.body, _var_table, &emitter.record_fields, &emitter.variant_info);
    let mut local_decls = Vec::new();

    // Bind locals
    for (var_id, val_type) in &scan.binds {
        let idx = param_count + local_decls.len() as u32;
        var_map.insert(var_id.0, idx);
        // Mutable captures use i32 cell ptr instead of original type
        if emitter.mutable_captures.contains(&var_id.0) {
            local_decls.push((1u32, ValType::I32));
        } else {
            local_decls.push((1u32, *val_type));
        }
    }

    // ScratchAllocator locals — sized per function complexity.
    // Simple functions (pure recursion, no stdlib calls) get minimal scratch.
    // Complex functions (stdlib pipelines) get full capacity.
    let needs_full_scratch = body_needs_scratch(&func.body);
    // Generous fixed caps: scratch locals live at `base..base+cap`, so a deep
    // stdlib pipeline that needs more simultaneous temps than the cap overflows
    // (#417) and falls back to the native build. These margins cover realistic
    // functions; the exact fix is a two-pass emit that sizes caps to the measured
    // high-water mark. Unused scratch locals are zero-cost declarations.
    let (scratch_i32_cap, scratch_i64_cap, scratch_f64_cap, scratch_v128_cap) = if needs_full_scratch {
        (64usize, 48usize, 48usize, 8usize)
    } else {
        // Minimal: enough for basic match/if temporaries. 8/4/4 (was 4/2/2):
        // a "simple" body can still nest enough match/if temporaries to hold
        // 3+ simultaneous i64 slots (#787, ceangal's scroll ticker — need 3,
        // had 2 → ScratchAllocator overflow panic). Unused slots are zero-cost
        // declarations, so the wider margin is free; the exact fix remains the
        // two-pass hwm-sized emit noted above.
        (8usize, 4usize, 4usize, 0usize)
    };
    let scratch_i32_base = param_count + local_decls.len() as u32;
    for _ in 0..scratch_i32_cap { local_decls.push((1, ValType::I32)); }
    let scratch_i64_base = param_count + local_decls.len() as u32;
    for _ in 0..scratch_i64_cap { local_decls.push((1, ValType::I64)); }
    let scratch_f64_base = param_count + local_decls.len() as u32;
    for _ in 0..scratch_f64_cap { local_decls.push((1, ValType::F64)); }
    let scratch_v128_base = param_count + local_decls.len() as u32;
    for _ in 0..scratch_v128_cap { local_decls.push((1, ValType::V128)); }

    let wasm_func = super::TrackedFunction::new(local_decls);

    let mut scratch_alloc = super::scratch::ScratchAllocator::new();
    scratch_alloc.set_bases_with_capacity(scratch_i32_base, scratch_i32_cap, scratch_i64_base, scratch_i64_cap, scratch_f64_base, scratch_f64_cap);
    scratch_alloc.set_v128_base_with_capacity(scratch_v128_base, scratch_v128_cap);
    let mut compiler = FuncCompiler {
        emitter,
        func: wasm_func,
        var_map,
        depth: 0,
        loop_stack: Vec::new(),
        scratch: scratch_alloc,
        var_table: _var_table,
        stub_ret_ty: almide_lang::types::Ty::Unit,
        current_module_name: module_name,
        live_heap: Vec::new(),
    };

    if let Some(init_idx) = init_globals_idx {
        wasm!(compiler.func, { call(init_idx); });
    }
    // Initialize preopened directory table for fs path resolution (only if program uses fs)
    if func.name == "main" && !func.is_test && compiler.emitter.needs_fs {
        wasm!(compiler.func, { call(compiler.emitter.rt.init_preopen_dirs); });
    }

    compiler.emit_expr(&func.body);

    // A LIFTED closure (`__closure_N`) whose whole body RETURNS a bare CAPTURED
    // heap var (`(v) => s1` — the fuzz B-198 or_else recovery shape; after
    // closure conversion the body is `EnvLoad` or `{ let x = EnvLoad; x }`): the
    // tail left the BORROWED env handle, but every consumer treats a closure
    // result as OWNED — the un-inc'd alias double-freed at scope end with the
    // still-owning binding (__rc_dec trap after correct output). Hand out a
    // co-owned +1 (rc_inc is (ptr) -> ptr, the #666/#668 share discipline).
    // EnvLoad exists only inside lifted closures, so the check cannot fire
    // elsewhere; a param or computed tail keeps its existing balance.
    fn tail_is_env_alias(body: &almide_ir::IrExpr) -> bool {
        use almide_ir::{IrExpr, IrExprKind, IrStmtKind};
        use std::collections::HashMap;
        // Is `e` an ALIAS of a captured env slot? Chases Var → bind chains and
        // nested Blocks (the conversion emits `{ let a = { let b = EnvLoad; b }; a }`).
        fn expr_alias<'a>(e: &'a IrExpr, binds: &HashMap<u32, &'a IrExpr>) -> bool {
            match &e.kind {
                IrExprKind::EnvLoad { .. } => true,
                IrExprKind::Var { id } => {
                    binds.get(&id.0).is_some_and(|v| expr_alias(v, binds))
                }
                IrExprKind::Block { stmts, expr: Some(tail) } => {
                    let mut scoped = binds.clone();
                    for s in stmts {
                        if let IrStmtKind::Bind { var, value, .. } = &s.kind {
                            scoped.insert(var.0, value);
                        }
                    }
                    expr_alias(tail, &scoped)
                }
                _ => false,
            }
        }
        expr_alias(body, &HashMap::new())
    }
    if tail_is_env_alias(&func.body) && FuncCompiler::is_heap_type(&func.body.ty) {
        let rc_inc = compiler.emitter.rt.rc_inc;
        wasm!(compiler.func, { call(rc_inc); });
    }

    // Perceus function-exit rc_dec is now handled by PerceusPass (IR-level RcDec nodes)

    // If function returns a value but body produces Unit (e.g., while loop with guard returns),
    // insert Unreachable to satisfy the validator (the code is unreachable in practice).
    let body_produces = super::values::ty_to_valtype(&func.body.ty);
    let func_expects = super::values::ty_to_valtype(&func.ret_ty);
    if func_expects.is_none() && body_produces.is_some() {
        // Void function but body pushes a value (Perceus tail expression
        // in void blocks). Drop to maintain WASM stack balance.
        wasm!(compiler.func, { drop; });
    } else if func_expects.is_some() && body_produces.is_none() {
        wasm!(compiler.func, { unreachable; });
    }

    wasm!(compiler.func, { end; });

    CompiledFunc::tracked(type_idx, compiler.func)
}
