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
    CodeSection, ExportSection, Function, FunctionSection, GlobalSection, GlobalType,
    MemorySection, MemoryType, Module, TypeSection, ValType,
};

use super::ir::{Op, WasmFunc, WasmTy, FuncIdx, verify_func_stack};
use super::emit::emit_ops;
use super::layout::LayoutRegistry;
use super::data::DataInterner;
use super::runtime::{self, RuntimeFns, HEAP_GLOBAL};

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
    // Runtime functions occupy the first `COUNT` indices; user functions follow.
    let rt = RuntimeFns::fixed();
    let base = runtime::COUNT;

    // ── Phase A: name → index for runtime fns and every user function ──
    let mut name_idx: HashMap<String, FuncIdx> = HashMap::new();
    for (name, idx) in rt.name_table() {
        name_idx.insert(name.to_string(), idx);
    }
    for (i, f) in ir_funcs.iter().enumerate() {
        name_idx.insert(f.name.as_str().to_string(), base + i as FuncIdx);
    }
    let lookup = |name: &str| name_idx.get(name).copied();

    // ── Phase B: lower user functions, interning string literals as we go ──
    let mut interner = DataInterner::new(DATA_BASE);
    let mut user_funcs: Vec<WasmFunc> = Vec::with_capacity(ir_funcs.len());
    for f in ir_funcs {
        let mut wf = super::lower::lower_function(f, var_table, reg, &lookup, &mut interner);
        // Resolve Alloc / RcInc / RcDec / StringConcat into Calls to the runtime.
        // Stack-effect preserving, so verification below still holds.
        runtime::resolve_abstract_ops(&mut wf.body, &rt);
        user_funcs.push(wf);
    }

    // ── Phase C: heap starts after the (now-complete) data segment ──
    let heap_start = align8(interner.end());

    // Build runtime functions with the resolved heap_start (RC guard baked in),
    // then place them before the user functions.
    let mut funcs: Vec<WasmFunc> = runtime::runtime_funcs(reg, heap_start as i32);
    funcs.append(&mut user_funcs);

    // ── Phase D: verify every function and reject unresolved abstract ops ──
    for wf in &funcs {
        verify_func_stack(wf).map_err(|detail| BuildError::StackVerify {
            func: wf.name.clone(),
            detail,
        })?;
        if let Some(op) = first_abstract_op(&wf.body) {
            return Err(BuildError::UnresolvedAbstract { func: wf.name.clone(), op });
        }
    }

    // ── Phase E: assemble sections ──
    Ok(assemble(&funcs, &name_idx, reg, &interner, heap_start))
}

/// First offset of the string-literal data segment. Above the null page so
/// pointer 0 stays invalid; 8-aligned for clean string headers.
const DATA_BASE: u32 = 16;

/// Round up to the next multiple of 8.
fn align8(n: u32) -> u32 {
    (n + 7) & !7
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
            Op::If { then, else_, .. } | Op::IfVoid { then, else_ } => {
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
    interner: &DataInterner,
    heap_start: u32,
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

    // ── Global section ──
    // Global 0: bump-allocator heap pointer, starting just past the data segment.
    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &wasm_encoder::ConstExpr::i32_const(heap_start as i32),
    );
    debug_assert_eq!(HEAP_GLOBAL, 0, "runtime expects the heap pointer at global 0");
    module.section(&globals);

    // ── Export section ──
    let mut exports = ExportSection::new();
    exports.export("memory", wasm_encoder::ExportKind::Memory, 0);
    if let Some(&main_idx) = name_idx.get("main") {
        exports.export("_start", wasm_encoder::ExportKind::Func, main_idx);
    }
    // Export every user function under its own name so callers (and the
    // wasmtime `--invoke` test harness) can reach them directly.
    let mut by_name: Vec<(&String, FuncIdx)> = name_idx.iter().map(|(n, &i)| (n, i)).collect();
    by_name.sort_by_key(|&(_, idx)| idx); // deterministic export order
    for (name, idx) in by_name {
        exports.export(name, wasm_encoder::ExportKind::Func, idx);
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

    // ── Data section (interned string literals at DATA_BASE) ──
    if !interner.bytes().is_empty() {
        let mut data = wasm_encoder::DataSection::new();
        data.active(
            0,
            &wasm_encoder::ConstExpr::i32_const(interner.base() as i32),
            interner.bytes().iter().copied(),
        );
        module.section(&data);
    }

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

    // ── wasmtime execution harness ──
    //
    // Builds a module, writes it to a temp file, and invokes a function via the
    // wasmtime CLI. Returns the printed return value, or None if wasmtime is
    // unavailable (so CI hosts without it skip rather than fail).

    use std::sync::atomic::{AtomicU32, Ordering};
    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    fn wasmtime_available() -> bool {
        std::process::Command::new("wasmtime")
            .arg("--version")
            .output()
            .map_or(false, |o| o.status.success())
    }

    /// Build, invoke `func`, and return its printed result. `None` ⇒ skip.
    fn run(funcs: &[IrFunction], func: &str) -> Option<String> {
        run_vt(funcs, &VarTable::new(), func)
    }

    /// Like `run`, but with an explicit VarTable (for functions with bindings).
    fn run_vt(funcs: &[IrFunction], vt: &VarTable, func: &str) -> Option<String> {
        if !wasmtime_available() {
            eprintln!("[skip] wasmtime not found — skipping execution test");
            return None;
        }
        let reg = LayoutRegistry::new();
        let bytes = build_module(funcs, vt, &reg).expect("build should succeed");
        assert!(wasmparser::validate(&bytes).is_ok(), "module must validate before exec");

        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("almide_engine_{}_{}.wasm", std::process::id(), seq));
        std::fs::write(&path, &bytes).expect("write temp wasm");

        let out = std::process::Command::new("wasmtime")
            .arg("--invoke").arg(func)
            .arg(&path)
            .output()
            .expect("spawn wasmtime");
        let _ = std::fs::remove_file(&path);

        assert!(
            out.status.success(),
            "wasmtime failed for `{}`: {}",
            func,
            String::from_utf8_lossy(&out.stderr),
        );
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// `main() -> 42` actually returns 42 when executed.
    #[test]
    fn exec_const() {
        let main = mk_func("main", Ty::Int, lit_int(42));
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "42");
        }
    }

    /// `main() -> 1 + 2` executes to 3.
    #[test]
    fn exec_arithmetic() {
        let body = IrExpr {
            kind: IrExprKind::BinOp {
                op: almide_ir::BinOp::AddInt,
                left: Box::new(lit_int(1)),
                right: Box::new(lit_int(2)),
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, body);
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "3");
        }
    }

    fn binop(op: almide_ir::BinOp, l: IrExpr, r: IrExpr, ty: Ty) -> IrExpr {
        IrExpr {
            kind: IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) },
            ty, span: None, def_id: None,
        }
    }

    fn iff(cond: IrExpr, then: IrExpr, else_: IrExpr, ty: Ty) -> IrExpr {
        IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(cond), then: Box::new(then), else_: Box::new(else_),
            },
            ty, span: None, def_id: None,
        }
    }

    /// Integer equality dispatches to i64.eq: `if 2 == 2 then 7 else 8` → 7.
    #[test]
    fn exec_int_eq() {
        let cond = binop(almide_ir::BinOp::Eq, lit_int(2), lit_int(2), Ty::Bool);
        let main = mk_func("main", Ty::Int, iff(cond, lit_int(7), lit_int(8), Ty::Int));
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "7");
        }
    }

    /// Integer comparison, false branch: `if 5 < 3 then 1 else 0` → 0.
    #[test]
    fn exec_int_lt_false() {
        let cond = binop(almide_ir::BinOp::Lt, lit_int(5), lit_int(3), Ty::Bool);
        let main = mk_func("main", Ty::Int, iff(cond, lit_int(1), lit_int(0), Ty::Int));
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "0");
        }
    }

    /// Float arithmetic + comparison: `if 1.5 + 2.0 > 3.0 then ... ` exercises f64.
    #[test]
    fn exec_float_arith() {
        fn litf(v: f64) -> IrExpr {
            IrExpr { kind: IrExprKind::LitFloat { value: v }, ty: Ty::Float, span: None, def_id: None }
        }
        // 1.5 + 2.0 = 3.5
        let sum = binop(almide_ir::BinOp::AddFloat, litf(1.5), litf(2.0), Ty::Float);
        let main = mk_func("main", Ty::Float, sum);
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "3.5");
        }
    }

    /// Let binding then use: `{ let x = 5; x + 10 }` → 15.
    #[test]
    fn exec_let_binding() {
        use almide_ir::{IrStmt, IrStmtKind, Mutability};
        let mut vt = VarTable::new();
        let x = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
        let var_x = IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None };
        let body = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![IrStmt {
                    kind: IrStmtKind::Bind {
                        var: x, ty: Ty::Int, mutability: Mutability::Let, value: lit_int(5),
                    },
                    span: None,
                }],
                expr: Some(Box::new(binop(almide_ir::BinOp::AddInt, var_x, lit_int(10), Ty::Int))),
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, body);
        if let Some(r) = run_vt(&[main], &vt, "main") {
            assert_eq!(r, "15");
        }
    }

    /// Integer negation lowers to `0 - x`: `-(5)` → -5.
    #[test]
    fn exec_neg_int() {
        let neg = IrExpr {
            kind: IrExprKind::UnOp {
                op: almide_ir::UnOp::NegInt,
                operand: Box::new(lit_int(5)),
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, neg);
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "-5");
        }
    }

    /// Match on a literal: `match 2 { 1 => 10, 2 => 20, _ => 0 }` → 20.
    #[test]
    fn exec_match_literal() {
        use almide_ir::{IrMatchArm, IrPattern};
        fn lit_arm(v: i64, body: i64) -> IrMatchArm {
            IrMatchArm {
                pattern: IrPattern::Literal { expr: lit_int(v) },
                guard: None,
                body: lit_int(body),
            }
        }
        let arms = vec![
            lit_arm(1, 10),
            lit_arm(2, 20),
            IrMatchArm { pattern: IrPattern::Wildcard, guard: None, body: lit_int(0) },
        ];
        let m = IrExpr {
            kind: IrExprKind::Match { subject: Box::new(lit_int(2)), arms },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, m);
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "20");
        }
    }

    /// Function with parameters called with arguments: `add(3, 4)` → 7.
    #[test]
    fn exec_call_with_params() {
        use almide_ir::{IrParam, ParamBorrow, CallTarget};
        let mut vt = VarTable::new();
        let a = vt.alloc(sym("a"), Ty::Int, almide_ir::Mutability::Let, None);
        let b = vt.alloc(sym("b"), Ty::Int, almide_ir::Mutability::Let, None);
        let mk_param = |var, name| IrParam {
            var, ty: Ty::Int, name, borrow: ParamBorrow::Own,
            open_record: None, default: None, attrs: vec![],
        };
        let var = |id| IrExpr { kind: IrExprKind::Var { id }, ty: Ty::Int, span: None, def_id: None };
        let add_body = binop(almide_ir::BinOp::AddInt, var(a), var(b), Ty::Int);
        let mut add = mk_func("add", Ty::Int, add_body);
        add.params = vec![mk_param(a, sym("a")), mk_param(b, sym("b"))];

        let call = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: sym("add") },
                args: vec![lit_int(3), lit_int(4)],
                type_args: vec![],
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, call);
        if let Some(r) = run_vt(&[add, main], &vt, "main") {
            assert_eq!(r, "7");
        }
    }

    /// Recursion integration test: `fib(10)` → 55.
    /// Exercises params, self-call, if-with-i64-result, comparison, +/-.
    #[test]
    fn exec_recursive_fib() {
        use almide_ir::{IrParam, ParamBorrow, CallTarget, BinOp};
        let mut vt = VarTable::new();
        let n = vt.alloc(sym("n"), Ty::Int, almide_ir::Mutability::Let, None);
        let var_n = || IrExpr { kind: IrExprKind::Var { id: n }, ty: Ty::Int, span: None, def_id: None };
        let call_fib = |arg: IrExpr| IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: sym("fib") },
                args: vec![arg], type_args: vec![],
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        // if n < 2 then n else fib(n-1) + fib(n-2)
        let cond = binop(BinOp::Lt, var_n(), lit_int(2), Ty::Bool);
        let rec = binop(
            BinOp::AddInt,
            call_fib(binop(BinOp::SubInt, var_n(), lit_int(1), Ty::Int)),
            call_fib(binop(BinOp::SubInt, var_n(), lit_int(2), Ty::Int)),
            Ty::Int,
        );
        let body = iff(cond, var_n(), rec, Ty::Int);
        let mut fib = mk_func("fib", Ty::Int, body);
        fib.params = vec![IrParam {
            var: n, ty: Ty::Int, name: sym("n"), borrow: ParamBorrow::Own,
            open_record: None, default: None, attrs: vec![],
        }];

        let main = mk_func("main", Ty::Int, call_fib(lit_int(10)));
        if let Some(r) = run_vt(&[fib, main], &vt, "main") {
            assert_eq!(r, "55");
        }
    }

    /// While loop with mutable accumulators:
    /// `var i=0; var sum=0; while i<5 { sum=sum+i; i=i+1 }; sum` → 10.
    #[test]
    fn exec_while_loop() {
        use almide_ir::{IrStmt, IrStmtKind, Mutability, BinOp};
        let mut vt = VarTable::new();
        let i = vt.alloc(sym("i"), Ty::Int, Mutability::Var, None);
        let sum = vt.alloc(sym("sum"), Ty::Int, Mutability::Var, None);
        let var = |id| IrExpr { kind: IrExprKind::Var { id }, ty: Ty::Int, span: None, def_id: None };

        let while_body = vec![
            IrStmt {
                kind: IrStmtKind::Assign {
                    var: sum,
                    value: binop(BinOp::AddInt, var(sum), var(i), Ty::Int),
                },
                span: None,
            },
            IrStmt {
                kind: IrStmtKind::Assign {
                    var: i,
                    value: binop(BinOp::AddInt, var(i), lit_int(1), Ty::Int),
                },
                span: None,
            },
        ];
        let while_expr = IrExpr {
            kind: IrExprKind::While {
                cond: Box::new(binop(BinOp::Lt, var(i), lit_int(5), Ty::Bool)),
                body: while_body,
            },
            ty: Ty::Unit, span: None, def_id: None,
        };
        let block = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![
                    IrStmt { kind: IrStmtKind::Bind { var: i, ty: Ty::Int, mutability: Mutability::Var, value: lit_int(0) }, span: None },
                    IrStmt { kind: IrStmtKind::Bind { var: sum, ty: Ty::Int, mutability: Mutability::Var, value: lit_int(0) }, span: None },
                    IrStmt { kind: IrStmtKind::Expr { expr: while_expr }, span: None },
                ],
                expr: Some(Box::new(var(sum))),
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, block);
        if let Some(r) = run_vt(&[main], &vt, "main") {
            assert_eq!(r, "10");
        }
    }

    /// End-to-end allocation: `[10, 20][1]` must return 20 at runtime,
    /// exercising __alloc + element store + element load through the runtime.
    #[test]
    fn exec_list_index() {
        let list = IrExpr {
            kind: IrExprKind::List { elements: vec![lit_int(10), lit_int(20)] },
            ty: Ty::list(Ty::Int), span: None, def_id: None,
        };
        let index = IrExpr {
            kind: IrExprKind::IndexAccess {
                object: Box::new(list),
                index: Box::new(lit_int(1)),
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, index);
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "20", "list[1] should be 20 (alloc/store/load round-trip)");
        }
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

    /// A list literal now lowers through the runtime allocator and validates:
    /// `Op::Alloc` is resolved to a `Call` to `__alloc`.
    #[test]
    fn build_list_via_runtime_alloc() {
        let vt = VarTable::new();
        let reg = LayoutRegistry::new();
        let list = IrExpr {
            kind: IrExprKind::List { elements: vec![lit_int(1), lit_int(2)] },
            ty: Ty::list(Ty::Int),
            span: None,
            def_id: None,
        };
        let main = mk_func("main", Ty::list(Ty::Int), list);
        let bytes = build_module(&[main], &vt, &reg).expect("list build should succeed");
        assert!(wasmparser::validate(&bytes).is_ok(), "module must validate");
    }

    /// Interpolating a Float still needs to_string (no runtime yet), so it is
    /// rejected cleanly as an unresolved StringInterp.
    #[test]
    fn string_interp_with_float_rejected() {
        use almide_ir::IrStringPart;
        let vt = VarTable::new();
        let reg = LayoutRegistry::new();
        let litf = IrExpr { kind: IrExprKind::LitFloat { value: 1.5 }, ty: Ty::Float, span: None, def_id: None };
        let interp = IrExpr {
            kind: IrExprKind::StringInterp {
                parts: vec![
                    IrStringPart::Lit { value: "x=".into() },
                    IrStringPart::Expr { expr: litf },
                ],
            },
            ty: Ty::String, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::String, interp);
        let err = build_module(&[main], &vt, &reg).unwrap_err();
        assert!(matches!(err, BuildError::UnresolvedAbstract { op: "StringInterp", .. }), "got {:?}", err);
    }

    fn lit_str(s: &str) -> IrExpr {
        IrExpr { kind: IrExprKind::LitStr { value: s.to_string() }, ty: Ty::String, span: None, def_id: None }
    }

    /// Call a runtime helper that returns an i64, with the given string args.
    fn rt_call(symbol: &str, args: Vec<IrExpr>) -> IrExpr {
        IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args },
            ty: Ty::Int, span: None, def_id: None,
        }
    }

    /// A string literal lands in the data segment with the right byte length:
    /// `__strlen("hello")` → 5.
    #[test]
    fn exec_string_literal_len() {
        let main = mk_func("main", Ty::Int, rt_call("__strlen", vec![lit_str("hello")]));
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "5");
        }
    }

    /// Concatenation length: `__strlen("foo" + "bar")` → 6.
    #[test]
    fn exec_string_concat_len() {
        let concat = binop(almide_ir::BinOp::ConcatStr, lit_str("foo"), lit_str("bar"), Ty::String);
        let main = mk_func("main", Ty::Int, rt_call("__strlen", vec![concat]));
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "6");
        }
    }

    /// Concatenation content: the byte at index 3 of `"foo" + "bar"` is 'b' (98).
    #[test]
    fn exec_string_concat_content() {
        let concat = binop(almide_ir::BinOp::ConcatStr, lit_str("foo"), lit_str("bar"), Ty::String);
        let main = mk_func("main", Ty::Int, rt_call("__byte_at", vec![concat, lit_int(3)]));
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "98", "byte 3 of 'foobar' is 'b'");
        }
    }

    /// String interpolation of String-typed parts works: `"${a}${b}"` for
    /// a="foo", b="bar" has length 6.
    #[test]
    fn exec_string_interp_strings() {
        use almide_ir::IrStringPart;
        let interp = IrExpr {
            kind: IrExprKind::StringInterp {
                parts: vec![
                    IrStringPart::Expr { expr: lit_str("foo") },
                    IrStringPart::Expr { expr: lit_str("bar") },
                ],
            },
            ty: Ty::String, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, rt_call("__strlen", vec![interp]));
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "6");
        }
    }

    /// `Some(42)` then unwrap → 42 (tagged-union alloc + payload load).
    #[test]
    fn exec_option_unwrap() {
        let some = IrExpr {
            kind: IrExprKind::OptionSome { expr: Box::new(lit_int(42)) },
            ty: Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, vec![Ty::Int]),
            span: None, def_id: None,
        };
        let unwrap = IrExpr {
            kind: IrExprKind::Unwrap { expr: Box::new(some) },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, unwrap);
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "42");
        }
    }

    /// `Ok(7)` then unwrap → 7.
    #[test]
    fn exec_result_unwrap() {
        let ok = IrExpr {
            kind: IrExprKind::ResultOk { expr: Box::new(lit_int(7)) },
            ty: Ty::Applied(
                almide_lang::types::constructor::TypeConstructorId::Result,
                vec![Ty::Int, Ty::String],
            ),
            span: None, def_id: None,
        };
        let unwrap = IrExpr {
            kind: IrExprKind::Unwrap { expr: Box::new(ok) },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, unwrap);
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "7");
        }
    }

    /// `(10, 20).1` → 20 (tuple element offset must account for i64 stride).
    #[test]
    fn exec_tuple_index() {
        let tup = IrExpr {
            kind: IrExprKind::Tuple { elements: vec![lit_int(10), lit_int(20)] },
            ty: Ty::Tuple(vec![Ty::Int, Ty::Int]), span: None, def_id: None,
        };
        let idx = IrExpr {
            kind: IrExprKind::TupleIndex { object: Box::new(tup), index: 1 },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, idx);
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "20", "tuple.1 of (10,20)");
        }
    }

    /// String equality: `"foo" == "foo"` → 1, `"foo" == "bar"` → 0,
    /// `"foo" != "bar"` → 1.
    #[test]
    fn exec_string_eq() {
        let eq = |a: &str, b: &str, op| binop(op, lit_str(a), lit_str(b), Ty::Bool);
        // "foo" == "foo" → if-then 1 else 0
        let same = mk_func("main", Ty::Int,
            iff(eq("foo", "foo", almide_ir::BinOp::Eq), lit_int(1), lit_int(0), Ty::Int));
        if let Some(r) = run(&[same], "main") { assert_eq!(r, "1", "foo==foo"); }

        let diff = mk_func("main", Ty::Int,
            iff(eq("foo", "bar", almide_ir::BinOp::Eq), lit_int(1), lit_int(0), Ty::Int));
        if let Some(r) = run(&[diff], "main") { assert_eq!(r, "0", "foo==bar"); }

        let ne = mk_func("main", Ty::Int,
            iff(eq("foo", "bar", almide_ir::BinOp::Neq), lit_int(1), lit_int(0), Ty::Int));
        if let Some(r) = run(&[ne], "main") { assert_eq!(r, "1", "foo!=bar"); }

        // Different lengths must be unequal: "ab" == "abc" → 0
        let lens = mk_func("main", Ty::Int,
            iff(eq("ab", "abc", almide_ir::BinOp::Eq), lit_int(1), lit_int(0), Ty::Int));
        if let Some(r) = run(&[lens], "main") { assert_eq!(r, "0", "ab==abc"); }
    }

    /// Match on string literals dispatches via __string_eq:
    /// `match "b" { "a" => 1, "b" => 2, _ => 0 }` → 2.
    #[test]
    fn exec_match_string() {
        use almide_ir::{IrMatchArm, IrPattern};
        fn str_arm(pat: &str, body: i64) -> IrMatchArm {
            IrMatchArm {
                pattern: IrPattern::Literal { expr: lit_str(pat) },
                guard: None,
                body: lit_int(body),
            }
        }
        let arms = vec![
            str_arm("a", 1),
            str_arm("b", 2),
            IrMatchArm { pattern: IrPattern::Wildcard, guard: None, body: lit_int(0) },
        ];
        let m = IrExpr {
            kind: IrExprKind::Match { subject: Box::new(lit_str("b")), arms },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, m);
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "2", "match \"b\"");
        }
    }

    /// Record field access reads the correct (type-derived) offset:
    /// `{a: 1, b: 2}.b` → 2. With i64 fields this requires an 8-byte stride,
    /// so a non-first field exercises the offset computation.
    #[test]
    fn exec_record_member() {
        let rec_ty = Ty::Record { fields: vec![(sym("a"), Ty::Int), (sym("b"), Ty::Int)] };
        let record = IrExpr {
            kind: IrExprKind::Record {
                name: None,
                fields: vec![(sym("a"), lit_int(1)), (sym("b"), lit_int(2))],
            },
            ty: rec_ty.clone(), span: None, def_id: None,
        };
        let member = IrExpr {
            kind: IrExprKind::Member { object: Box::new(record), field: sym("b") },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, member);
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "2", "record.b of {{a:1, b:2}}");
        }
    }

    /// Build `"${n}"` for an integer literal `n`.
    fn interp_int(n: i64) -> IrExpr {
        use almide_ir::IrStringPart;
        IrExpr {
            kind: IrExprKind::StringInterp {
                parts: vec![IrStringPart::Expr { expr: lit_int(n) }],
            },
            ty: Ty::String, span: None, def_id: None,
        }
    }

    /// `__int_to_string(42)` via `"${42}"` → length 2, bytes '4','2'.
    #[test]
    fn exec_int_to_string_positive() {
        let len = mk_func("main", Ty::Int, rt_call("__strlen", vec![interp_int(42)]));
        if let Some(r) = run(&[len], "main") { assert_eq!(r, "2"); }
        let b0 = mk_func("main", Ty::Int, rt_call("__byte_at", vec![interp_int(42), lit_int(0)]));
        if let Some(r) = run(&[b0], "main") { assert_eq!(r, "52", "'4'"); }
        let b1 = mk_func("main", Ty::Int, rt_call("__byte_at", vec![interp_int(42), lit_int(1)]));
        if let Some(r) = run(&[b1], "main") { assert_eq!(r, "50", "'2'"); }
    }

    /// `"${0}"` → length 1, byte '0'.
    #[test]
    fn exec_int_to_string_zero() {
        let len = mk_func("main", Ty::Int, rt_call("__strlen", vec![interp_int(0)]));
        if let Some(r) = run(&[len], "main") { assert_eq!(r, "1"); }
        let b0 = mk_func("main", Ty::Int, rt_call("__byte_at", vec![interp_int(0), lit_int(0)]));
        if let Some(r) = run(&[b0], "main") { assert_eq!(r, "48", "'0'"); }
    }

    /// `"${-5}"` → length 2, leading '-' then '5'.
    #[test]
    fn exec_int_to_string_negative() {
        let len = mk_func("main", Ty::Int, rt_call("__strlen", vec![interp_int(-5)]));
        if let Some(r) = run(&[len], "main") { assert_eq!(r, "2"); }
        let b0 = mk_func("main", Ty::Int, rt_call("__byte_at", vec![interp_int(-5), lit_int(0)]));
        if let Some(r) = run(&[b0], "main") { assert_eq!(r, "45", "'-'"); }
        let b1 = mk_func("main", Ty::Int, rt_call("__byte_at", vec![interp_int(-5), lit_int(1)]));
        if let Some(r) = run(&[b1], "main") { assert_eq!(r, "53", "'5'"); }
    }

    /// Mixed literal + int interpolation: `"x=${42}"` → "x=42", length 4.
    #[test]
    fn exec_interp_mixed() {
        use almide_ir::IrStringPart;
        let interp = IrExpr {
            kind: IrExprKind::StringInterp {
                parts: vec![
                    IrStringPart::Lit { value: "x=".into() },
                    IrStringPart::Expr { expr: lit_int(42) },
                ],
            },
            ty: Ty::String, span: None, def_id: None,
        };
        let len = mk_func("main", Ty::Int, rt_call("__strlen", vec![interp.clone()]));
        if let Some(r) = run(&[len], "main") { assert_eq!(r, "4"); }
        // byte 2 is '4'
        let b2 = mk_func("main", Ty::Int, rt_call("__byte_at", vec![interp, lit_int(2)]));
        if let Some(r) = run(&[b2], "main") { assert_eq!(r, "52", "'4'"); }
    }
}
