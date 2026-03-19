//! IrFunction → compiled WASM function.

use std::collections::HashMap;
use wasm_encoder::{Function, ValType};

use crate::ir::{IrFunction, VarTable};

use super::{CompiledFunc, FuncCompiler, WasmEmitter};
use super::statements::collect_locals;

/// Compile an IR function into a WASM function body.
pub fn compile_function(
    emitter: &mut WasmEmitter,
    func: &IrFunction,
    _var_table: &VarTable,
    type_idx: u32,
) -> CompiledFunc {
    let param_count = func.params.len() as u32;

    // Map parameters to WASM local indices 0..N-1
    let mut var_map: HashMap<u32, u32> = HashMap::new();
    for (i, param) in func.params.iter().enumerate() {
        var_map.insert(param.var.0, i as u32);
    }

    // Pre-scan body for variable bindings and match scratch requirements
    let scan = collect_locals(&func.body);
    let mut local_decls = Vec::new();

    // Bind locals
    for (var_id, val_type) in &scan.binds {
        let idx = param_count + local_decls.len() as u32;
        var_map.insert(var_id.0, idx);
        local_decls.push((1u32, *val_type));
    }

    // Match scratch locals: one i64 + one i32 per nesting depth level
    let match_i64_base = param_count + local_decls.len() as u32;
    for _ in 0..scan.scratch_depth {
        local_decls.push((1, ValType::I64));
    }
    let match_i32_base = param_count + local_decls.len() as u32;
    for _ in 0..scan.scratch_depth {
        local_decls.push((1, ValType::I32));
    }

    // Create WASM function with declared locals
    let wasm_func = Function::new(local_decls);

    // Compile the body
    let mut compiler = FuncCompiler {
        emitter,
        func: wasm_func,
        var_map,
        depth: 0,
        loop_stack: Vec::new(),
        match_i64_base,
        match_i32_base,
        match_depth: 0,
    };

    compiler.emit_expr(&func.body);
    compiler.func.instruction(&wasm_encoder::Instruction::End);

    CompiledFunc {
        type_idx,
        func: compiler.func,
    }
}
