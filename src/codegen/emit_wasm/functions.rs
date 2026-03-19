//! IrFunction → compiled WASM function.

use std::collections::HashMap;
use wasm_encoder::Function;

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

    // Pre-scan body for all variable bindings to declare as WASM locals
    let body_locals = collect_locals(&func.body);
    let mut local_decls = Vec::new();
    for (var_id, val_type) in &body_locals {
        let idx = param_count + local_decls.len() as u32;
        var_map.insert(var_id.0, idx);
        local_decls.push((1u32, *val_type));
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
    };

    compiler.emit_expr(&func.body);

    // If the function returns a value, it's on the stack.
    // If it returns Unit, nothing is on the stack. Either way, End terminates.
    compiler.func.instruction(&wasm_encoder::Instruction::End);

    CompiledFunc {
        type_idx,
        func: compiler.func,
    }
}
