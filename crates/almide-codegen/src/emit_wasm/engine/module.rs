//! Module assembly: IrFunction[] → WasmIR → verified → wasm_encoder::Module → bytes.
//!
//! This is the Phase 1 capstone of the WASM engine redesign. It ties the
//! lowering (lower.rs), stack verification (ir.rs), and emission (emit.rs)
//! into a complete, self-contained pipeline that produces a valid WASM
//! binary — without touching the legacy hand-written emitter.
//!
//! Pipeline:
//! ```text
//!   IrFunction[]
//!     → assign func indices (by name)
//!     → lower each function     (lower::lower_function)
//!     → verify stack balance    (ir::verify_func_stack)
//!     → check no abstract ops   (this module)
//!     → assemble sections       (wasm-encoder)
//!     → finish() → Vec<u8>
//! ```
//!
//! Abstract ops (Alloc, RcInc, StringConcat, …) must be resolved to concrete
//! Calls before this stage. Until the engine grows its own runtime (Phase 2),
//! any function still containing them is rejected with a clear error rather
//! than panicking inside emit.rs.

use std::collections::HashMap;

use almide_ir::{IrFunction, VarTable};
use wasm_encoder::{
    CodeSection, ExportSection, Function, FunctionSection, MemorySection, MemoryType,
    Module, TypeSection,
};

use super::ir::{Op, WasmFunc, WasmTy, FuncIdx, verify_func_stack};
use super::emit::emit_ops;
use super::layout::LayoutRegistry;

/// Errors produced while building a module from IR.
#[derive(Debug)]
pub enum BuildError {
    /// A lowered function failed stack-effect verification.
    StackVerify { func: String, detail: String },
    /// A function still contains an abstract op that has no concrete lowering yet.
    UnresolvedAbstract { func: String, op: &'static str },
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::StackVerify { func, detail } => {
                write!(f, "stack verification failed in `{}`: {}", func, detail)
            }
            BuildError::UnresolvedAbstract { func, op } => write!(
                f,
                "`{}` contains unresolved abstract op `{}` — needs runtime lowering (engine Phase 2)",
                func, op,
            ),
        }
    }
}

impl std::error::Error for BuildError {}

/// Build a complete WASM module from a set of IR functions.
///
/// All functions share a single `VarTable` (the program-global VarId space)
/// and `LayoutRegistry`. Returns the encoded WASM binary on success.
pub fn build_module(
    ir_funcs: &[IrFunction],
    var_table: &VarTable,
    reg: &LayoutRegistry,
) -> Result<Vec<u8>, BuildError> {
    // ── Phase A: assign a function index to every function, by name ──
    let mut name_idx: HashMap<String, FuncIdx> = HashMap::new();
    for (i, f) in ir_funcs.iter().enumerate() {
        name_idx.insert(f.name.as_str().to_string(), i as FuncIdx);
    }
    let lookup = |name: &str| name_idx.get(name).copied();

    // ── Phase B: lower + verify each function ──
    let mut funcs: Vec<WasmFunc> = Vec::with_capacity(ir_funcs.len());
    for f in ir_funcs {
        let wf = super::lower::lower_function(f, var_table, reg, &lookup);
        verify_func_stack(&wf).map_err(|detail| BuildError::StackVerify {
            func: wf.name.clone(),
            detail,
        })?;
        if let Some(op) = first_abstract_op(&wf.body) {
            return Err(BuildError::UnresolvedAbstract {
                func: wf.name.clone(),
                op,
            });
        }
        funcs.push(wf);
    }

    // ── Phase C: assemble sections ──
    Ok(assemble(&funcs, &name_idx, reg))
}

/// Walk an op tree and return the name of the first abstract op found, if any.
///
/// Abstract ops (`Alloc`, `RcInc`, `StringConcat`, …) panic in `emit_op`
/// because they require runtime support. We surface them as a typed error
/// here so the build fails cleanly with a diagnostic instead of crashing.
fn first_abstract_op(ops: &[Op]) -> Option<&'static str> {
    for op in ops {
        match op {
            Op::Alloc => return Some("Alloc"),
            Op::AllocCollection { .. } => return Some("AllocCollection"),
            Op::RcInc => return Some("RcInc"),
            Op::RcDec { .. } => return Some("RcDec"),
            Op::CowCheck { clone_body, .. } => {
                return first_abstract_op(clone_body).or(Some("CowCheck"));
            }
            Op::StringConcat => return Some("StringConcat"),
            Op::StringInterp { .. } => return Some("StringInterp"),

            // Recurse into compound control flow.
            Op::Block(body) | Op::Loop(body) | Op::Seq(body) => {
                if let Some(n) = first_abstract_op(body) {
                    return Some(n);
                }
            }
            Op::If { then, else_ } | Op::IfVoid { then, else_ } => {
                if let Some(n) = first_abstract_op(then).or_else(|| first_abstract_op(else_)) {
                    return Some(n);
                }
            }
            Op::ListForEach { body, .. } | Op::MapForEach { body, .. } => {
                if let Some(n) = first_abstract_op(body) {
                    return Some(n);
                }
            }
            _ => {}
        }
    }
    None
}

/// Assemble verified WasmFuncs into a final module binary.
fn assemble(
    funcs: &[WasmFunc],
    name_idx: &HashMap<String, FuncIdx>,
    reg: &LayoutRegistry,
) -> Vec<u8> {
    let mut module = Module::new();

    // ── Type section (deduplicated signatures) ──
    let mut types = TypeSection::new();
    let mut sig_map: HashMap<(Vec<WasmTy>, Vec<WasmTy>), u32> = HashMap::new();
    let mut func_sig: Vec<u32> = Vec::with_capacity(funcs.len());
    for f in funcs {
        let key = (f.params.clone(), f.results.clone());
        let idx = *sig_map.entry(key).or_insert_with(|| {
            let i = types.len();
            types.ty().function(
                f.params.iter().map(|t| t.to_valtype()),
                f.results.iter().map(|t| t.to_valtype()),
            );
            i
        });
        func_sig.push(idx);
    }
    module.section(&types);

    // ── Function section (type index per function) ──
    let mut functions = FunctionSection::new();
    for &sig in &func_sig {
        functions.function(sig);
    }
    module.section(&functions);

    // ── Memory section (single linear memory) ──
    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: 2,
        maximum: Some(65536),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memory);

    // ── Export section ──
    let mut exports = ExportSection::new();
    exports.export("memory", wasm_encoder::ExportKind::Memory, 0);
    if let Some(&main_idx) = name_idx.get("main") {
        exports.export("_start", wasm_encoder::ExportKind::Func, main_idx);
    }
    module.section(&exports);

    // ── Code section ──
    let mut codes = CodeSection::new();
    for f in funcs {
        // Run-length encode locals.
        let locals = rle_locals(&f.locals);
        let mut wf = Function::new(locals);
        emit_ops(&f.body, &mut wf, reg);
        wf.instruction(&wasm_encoder::Instruction::End);
        codes.function(&wf);
    }
    module.section(&codes);

    module.finish()
}

/// Run-length encode a flat list of local types into wasm-encoder's
/// `(count, ValType)` form.
fn rle_locals(locals: &[WasmTy]) -> Vec<(u32, wasm_encoder::ValType)> {
    let mut out: Vec<(u32, wasm_encoder::ValType)> = Vec::new();
    for &ty in locals {
        let vt = ty.to_valtype();
        match out.last_mut() {
            Some((count, last)) if *last == vt => *count += 1,
            _ => out.push((1, vt)),
        }
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use almide_base::intern::sym;
    use almide_ir::{IrExpr, IrExprKind, IrVisibility};
    use almide_lang::types::Ty;

    fn mk_func(name: &str, ret_ty: Ty, body: IrExpr) -> IrFunction {
        IrFunction {
            name: sym(name),
            params: vec![],
            ret_ty,
            body,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Private,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![],
            module_origin: None,
        }
    }

    fn lit_int(v: i64) -> IrExpr {
        IrExpr { kind: IrExprKind::LitInt { value: v }, ty: Ty::Int, span: None, def_id: None }
    }

    /// A function returning a constant produces a valid, parseable WASM module.
    #[test]
    fn build_const_function() {
        let vt = VarTable::new();
        let reg = LayoutRegistry::new();
        let main = mk_func("main", Ty::Int, lit_int(42));
        let bytes = build_module(&[main], &vt, &reg).expect("build should succeed");
        // Valid WASM magic header.
        assert_eq!(&bytes[0..4], b"\0asm");
        // Parses cleanly through wasmparser via the validate path.
        assert!(wasmparser::validate(&bytes).is_ok(), "module must validate");
    }

    /// Arithmetic across two functions: caller calls callee.
    #[test]
    fn build_arithmetic_and_call() {
        let vt = VarTable::new();
        let reg = LayoutRegistry::new();
        // callee() -> Int = 1 + 2
        let add = IrExpr {
            kind: IrExprKind::BinOp {
                op: almide_ir::BinOp::AddInt,
                left: Box::new(lit_int(1)),
                right: Box::new(lit_int(2)),
            },
            ty: Ty::Int,
            span: None,
            def_id: None,
        };
        let callee = mk_func("callee", Ty::Int, add);
        // main() -> Int = callee()
        let call = IrExpr {
            kind: IrExprKind::Call {
                target: almide_ir::CallTarget::Named { name: sym("callee") },
                args: vec![],
                type_args: vec![],
            },
            ty: Ty::Int,
            span: None,
            def_id: None,
        };
        let main = mk_func("main", Ty::Int, call);
        let bytes = build_module(&[callee, main], &vt, &reg).expect("build should succeed");
        assert!(wasmparser::validate(&bytes).is_ok(), "module must validate");
    }

    /// A function that allocates (list literal) is rejected cleanly, not panicked.
    #[test]
    fn abstract_op_rejected() {
        let vt = VarTable::new();
        let reg = LayoutRegistry::new();
        let list = IrExpr {
            kind: IrExprKind::List { elements: vec![lit_int(1), lit_int(2)] },
            ty: Ty::list(Ty::Int),
            span: None,
            def_id: None,
        };
        let main = mk_func("main", Ty::list(Ty::Int), list);
        let err = build_module(&[main], &vt, &reg).unwrap_err();
        assert!(matches!(err, BuildError::UnresolvedAbstract { .. }), "got {:?}", err);
    }
}
