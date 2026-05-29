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

/// Interns WASM function signatures `(params, results)` into stable type
/// indices, shared between lowering (for `call_indirect`) and assembly (for the
/// type section). Append-only, so indices handed out during lowering stay valid
/// when assembly adds the concrete function signatures.
#[derive(Default)]
pub struct SigTable {
    sigs: Vec<(Vec<WasmTy>, Vec<WasmTy>)>,
}

impl SigTable {
    pub fn new() -> Self {
        SigTable { sigs: Vec::new() }
    }

    /// Intern a signature, returning its type index.
    pub fn intern(&mut self, params: Vec<WasmTy>, results: Vec<WasmTy>) -> u32 {
        if let Some(i) = self.sigs.iter().position(|(p, r)| *p == params && *r == results) {
            return i as u32;
        }
        self.sigs.push((params, results));
        (self.sigs.len() - 1) as u32
    }

    fn all(&self) -> &[(Vec<WasmTy>, Vec<WasmTy>)] {
        &self.sigs
    }
}

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
    // Imports occupy the lowest indices, then runtime functions, then user
    // functions.
    let rt = RuntimeFns::fixed();
    let base = runtime::IMPORT_COUNT + runtime::COUNT;

    // ── Phase A: name → index for runtime fns and every user function ──
    let mut name_idx: HashMap<String, FuncIdx> = HashMap::new();
    for (name, idx) in rt.name_table() {
        name_idx.insert(name.to_string(), idx);
    }
    for (i, f) in ir_funcs.iter().enumerate() {
        name_idx.insert(f.name.as_str().to_string(), base + i as FuncIdx);
    }
    let lookup = |name: &str| name_idx.get(name).copied();

    // ── Phase B: lower user functions, interning string literals and
    //    call_indirect signatures as we go ──
    let mut interner = DataInterner::new(DATA_BASE);
    let mut sigs = SigTable::new();
    let mut user_funcs: Vec<WasmFunc> = Vec::with_capacity(ir_funcs.len());
    for f in ir_funcs {
        let mut wf = super::lower::lower_function(f, var_table, reg, &lookup, &mut interner, &mut sigs);
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

    // ── Phase D: reject unresolved abstract ops, then verify stack balance ──
    // Abstract-op check first: an Unsupported marker yields a clear feature
    // diagnostic, whereas it would otherwise surface as a confusing stack
    // imbalance (it pushes a placeholder value). Verification then guards the
    // fully-supported functions.
    for wf in &funcs {
        if let Some(op) = first_abstract_op(&wf.body) {
            return Err(BuildError::UnresolvedAbstract { func: wf.name.clone(), op });
        }
        verify_func_stack(wf).map_err(|detail| BuildError::StackVerify {
            func: wf.name.clone(),
            detail,
        })?;
    }

    // ── Phase E: assemble sections ──
    Ok(assemble(&funcs, &name_idx, reg, &interner, heap_start, &mut sigs))
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
            Op::Unsupported(what) => return Some(what),

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
    sigs: &mut SigTable,
) -> Vec<u8> {
    let mut module = Module::new();

    // Intern each function's signature into the shared table (call_indirect
    // signatures were already interned during lowering; this only adds the
    // concrete function signatures, never reordering existing entries).
    let func_sig: Vec<u32> = funcs.iter()
        .map(|f| sigs.intern(f.params.clone(), f.results.clone()))
        .collect();
    // fd_write: (i32 fd, i32 iovs, i32 iovs_len, i32 nwritten) -> i32 errno.
    let fd_write_sig = sigs.intern(vec![WasmTy::I32; 4], vec![WasmTy::I32]);

    // ── Type section (every interned signature, in index order) ──
    let mut types = TypeSection::new();
    for (params, results) in sigs.all() {
        types.ty().function(
            params.iter().map(|t| t.to_valtype()),
            results.iter().map(|t| t.to_valtype()),
        );
    }
    module.section(&types);

    // ── Import section: WASI fd_write at function index 0 (imports precede
    //    every defined function in the index space). ──
    let mut imports = wasm_encoder::ImportSection::new();
    imports.import("wasi_snapshot_preview1", "fd_write",
        wasm_encoder::EntityType::Function(fd_write_sig));
    module.section(&imports);

    // ── Function section (type index per defined function) ──
    let mut functions = FunctionSection::new();
    for &sig in &func_sig {
        functions.function(sig);
    }
    module.section(&functions);

    // ── Table section: one funcref slot per function (imports + defined) so
    //    closures and FnRef can call_indirect with table index == func index. ──
    let n = (runtime::IMPORT_COUNT as u64) + funcs.len() as u64;
    let mut tables = wasm_encoder::TableSection::new();
    tables.table(wasm_encoder::TableType {
        element_type: wasm_encoder::RefType::FUNCREF,
        minimum: n,
        maximum: Some(n),
        table64: false,
        shared: false,
    });
    module.section(&tables);

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

    // ── Element section: populate the table so slot i → function i. ──
    let elem_funcs: Vec<u32> = (0..runtime::IMPORT_COUNT + funcs.len() as u32).collect();
    let mut elements = wasm_encoder::ElementSection::new();
    elements.active(
        Some(0),
        &wasm_encoder::ConstExpr::i32_const(0),
        wasm_encoder::Elements::Functions(std::borrow::Cow::Borrowed(&elem_funcs)),
    );
    module.section(&elements);

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
        if let Err(e) = wasmparser::validate(&bytes) {
            panic!("module must validate before exec: {e}");
        }

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

    /// Map[Int,Int]: literal construction, get (hit/miss), len, contains, set
    /// (insert + overwrite). Exercises the Swiss-table runtime end to end.
    #[test]
    fn exec_map_int_int() {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let map_ty = || Ty::Applied(TC::Map, vec![Ty::Int, Ty::Int]);
        let opt_ty = || Ty::Applied(TC::Option, vec![Ty::Int]);
        // {1:10, 2:20, 3:30}
        let lit = || IrExpr {
            kind: IrExprKind::MapLiteral { entries: vec![
                (lit_int(1), lit_int(10)), (lit_int(2), lit_int(20)), (lit_int(3), lit_int(30)),
            ] },
            ty: map_ty(), span: None, def_id: None,
        };
        let get_or = |m: IrExpr, k: i64, d: i64| {
            let g = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_map_get"), args: vec![m, lit_int(k)] }, ty: opt_ty(), span: None, def_id: None };
            IrExpr { kind: IrExprKind::UnwrapOr { expr: Box::new(g), fallback: Box::new(lit_int(d)) }, ty: Ty::Int, span: None, def_id: None }
        };
        let call_i = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };

        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };
        let tb = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Bool, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };

        ti(get_or(lit(), 2, -1), "20", "get hit");
        ti(get_or(lit(), 9, -1), "-1", "get miss");
        ti(call_i("almide_rt_map_len", vec![lit()], Ty::Int), "3", "len");
        tb(call_i("almide_rt_map_contains", vec![lit(), lit_int(3)], Ty::Bool), "1", "contains hit");
        tb(call_i("almide_rt_map_contains", vec![lit(), lit_int(9)], Ty::Bool), "0", "contains miss");

        // set: insert a new key, then look it up; len grows to 4.
        let set = |m: IrExpr, k: i64, v: i64| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_map_set"), args: vec![m, lit_int(k), lit_int(v)] },
            ty: map_ty(), span: None, def_id: None };
        ti(get_or(set(lit(), 4, 40), 4, -1), "40", "set insert");
        ti(call_i("almide_rt_map_len", vec![set(lit(), 4, 40)], Ty::Int), "4", "len after insert");
        // overwrite existing key keeps len at 3 and updates the value.
        ti(get_or(set(lit(), 1, 99), 1, -1), "99", "set overwrite");
        ti(call_i("almide_rt_map_len", vec![set(lit(), 1, 99)], Ty::Int), "3", "len after overwrite");
    }

    /// map.fold: sum keys+values of {1:10,2:20,3:30} → (1+2+3)+(10+20+30)=66.
    #[test]
    fn exec_map_fold() {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let map_ty = Ty::Applied(TC::Map, vec![Ty::Int, Ty::Int]);
        let mut vt = VarTable::new();
        let acc = vt.alloc(sym("acc"), Ty::Int, almide_ir::Mutability::Let, None);
        let k = vt.alloc(sym("k"), Ty::Int, almide_ir::Mutability::Let, None);
        let v = vt.alloc(sym("v"), Ty::Int, almide_ir::Mutability::Let, None);
        let var = |id| IrExpr { kind: IrExprKind::Var { id }, ty: Ty::Int, span: None, def_id: None };
        // acc + k + v
        let body = binop(almide_ir::BinOp::AddInt,
            binop(almide_ir::BinOp::AddInt, var(acc), var(k), Ty::Int), var(v), Ty::Int);
        let lam = IrExpr {
            kind: IrExprKind::Lambda { params: vec![(acc, Ty::Int), (k, Ty::Int), (v, Ty::Int)], body: Box::new(body), lambda_id: None },
            ty: Ty::Fn { params: vec![Ty::Int, Ty::Int, Ty::Int], ret: Box::new(Ty::Int) }, span: None, def_id: None,
        };
        let lit = IrExpr {
            kind: IrExprKind::MapLiteral { entries: vec![(lit_int(1), lit_int(10)), (lit_int(2), lit_int(20)), (lit_int(3), lit_int(30))] },
            ty: map_ty.clone(), span: None, def_id: None,
        };
        let folded = IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_map_fold"), args: vec![lit, lit_int(0), lam] },
            ty: Ty::Int, span: None, def_id: None,
        };
        let m = mk_func("main", Ty::Int, folded);
        if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(r, "66", "fold sum keys+values"); }
    }

    /// map.filter: keep entries with value > 15 from {1:10,2:20,3:30} → {2,3}.
    #[test]
    fn exec_map_filter() {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let map_ty = || Ty::Applied(TC::Map, vec![Ty::Int, Ty::Int]);
        let mut vt = VarTable::new();
        let k = vt.alloc(sym("k"), Ty::Int, almide_ir::Mutability::Let, None);
        let v = vt.alloc(sym("v"), Ty::Int, almide_ir::Mutability::Let, None);
        let lam = IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(k, Ty::Int), (v, Ty::Int)],
                body: Box::new(binop(almide_ir::BinOp::Gt,
                    IrExpr { kind: IrExprKind::Var { id: v }, ty: Ty::Int, span: None, def_id: None }, lit_int(15), Ty::Bool)),
                lambda_id: None,
            },
            ty: Ty::Fn { params: vec![Ty::Int, Ty::Int], ret: Box::new(Ty::Bool) }, span: None, def_id: None,
        };
        let filtered = || IrExpr {
            kind: IrExprKind::RuntimeCall {
                symbol: sym("almide_rt_map_filter"),
                args: vec![
                    IrExpr { kind: IrExprKind::MapLiteral { entries: vec![(lit_int(1), lit_int(10)), (lit_int(2), lit_int(20)), (lit_int(3), lit_int(30))] }, ty: map_ty(), span: None, def_id: None },
                    lam.clone(),
                ],
            },
            ty: map_ty(), span: None, def_id: None,
        };
        let call = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(&r, exp, "{}", msg); } };
        let tb = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Bool, e); if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(&r, exp, "{}", msg); } };
        ti(call("almide_rt_map_len", vec![filtered()], Ty::Int), "2", "filter len");
        tb(call("almide_rt_map_contains", vec![filtered(), lit_int(1)], Ty::Bool), "0", "1 filtered out");
        tb(call("almide_rt_map_contains", vec![filtered(), lit_int(3)], Ty::Bool), "1", "3 kept");
    }

    /// map.map (map_values): `{1:10,2:20}.map(v => v+1)` → {1:11, 2:21}.
    #[test]
    fn exec_map_map_values() {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let map_ty = || Ty::Applied(TC::Map, vec![Ty::Int, Ty::Int]);
        let mut vt = VarTable::new();
        let v = vt.alloc(sym("v"), Ty::Int, almide_ir::Mutability::Let, None);
        let lit = || IrExpr {
            kind: IrExprKind::MapLiteral { entries: vec![(lit_int(1), lit_int(10)), (lit_int(2), lit_int(20))] },
            ty: map_ty(), span: None, def_id: None,
        };
        let lam = IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(v, Ty::Int)],
                body: Box::new(binop(almide_ir::BinOp::AddInt,
                    IrExpr { kind: IrExprKind::Var { id: v }, ty: Ty::Int, span: None, def_id: None }, lit_int(1), Ty::Int)),
                lambda_id: None,
            },
            ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) }, span: None, def_id: None,
        };
        let mapped = || IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_map_map_values"), args: vec![lit(), lam.clone()] },
            ty: map_ty(), span: None, def_id: None,
        };
        let get_or = |m: IrExpr, k: i64| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_map_get_or"), args: vec![m, lit_int(k), lit_int(-1)] },
            ty: Ty::Int, span: None, def_id: None };
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(&r, exp, "{}", msg); } };
        ti(get_or(mapped(), 1), "11", "v[1]+1");
        ti(get_or(mapped(), 2), "21", "v[2]+1");
    }

    /// map.merge: `{1:10,2:20}` merged with `{2:99,3:30}` → b wins on key 2.
    #[test]
    fn exec_map_merge() {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let map_ty = || Ty::Applied(TC::Map, vec![Ty::Int, Ty::Int]);
        let mk = |entries: Vec<(i64, i64)>| IrExpr {
            kind: IrExprKind::MapLiteral { entries: entries.into_iter().map(|(k, v)| (lit_int(k), lit_int(v))).collect() },
            ty: map_ty(), span: None, def_id: None,
        };
        let call = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };
        let merged = || call("almide_rt_map_merge", vec![mk(vec![(1,10),(2,20)]), mk(vec![(2,99),(3,30)])], map_ty());
        let get_or = |m: IrExpr, k: i64| call("almide_rt_map_get_or", vec![m, lit_int(k), lit_int(-1)], Ty::Int);
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };
        ti(call("almide_rt_map_len", vec![merged()], Ty::Int), "3", "merge len (keys 1,2,3)");
        ti(get_or(merged(), 1), "10", "key 1 from a");
        ti(get_or(merged(), 2), "99", "key 2 from b (wins)");
        ti(get_or(merged(), 3), "30", "key 3 from b");
    }

    /// map.keys / map.values / map.remove on Map[Int,Int].
    #[test]
    fn exec_map_keys_values_remove() {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let map_ty = || Ty::Applied(TC::Map, vec![Ty::Int, Ty::Int]);
        let lit = || IrExpr {
            kind: IrExprKind::MapLiteral { entries: vec![
                (lit_int(1), lit_int(10)), (lit_int(2), lit_int(20)), (lit_int(3), lit_int(30)),
            ] },
            ty: map_ty(), span: None, def_id: None,
        };
        let call = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };
        let sum = |list: IrExpr| call("almide_rt_list_sum", vec![list], Ty::Int);
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };
        let tb = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Bool, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };

        // keys sum 1+2+3=6, values sum 10+20+30=60 (order is hash-dependent)
        ti(sum(call("almide_rt_map_keys", vec![lit()], Ty::list(Ty::Int))), "6", "keys sum");
        ti(sum(call("almide_rt_map_values", vec![lit()], Ty::list(Ty::Int))), "60", "values sum");
        // remove 2 → len 2, no longer contains 2, still contains 1
        let removed = || call("almide_rt_map_remove", vec![lit(), lit_int(2)], map_ty());
        ti(call("almide_rt_map_len", vec![removed()], Ty::Int), "2", "len after remove");
        tb(call("almide_rt_map_contains", vec![removed(), lit_int(2)], Ty::Bool), "0", "removed key gone");
        tb(call("almide_rt_map_contains", vec![removed(), lit_int(1)], Ty::Bool), "1", "other key kept");
    }

    /// Map[String, Int]: string-keyed lookup via FNV hash + __string_eq probing.
    /// {"a":1,"b":2,"c":3} → get "b" ==2, miss "z" ==-1, contains "c"==1, len 3.
    #[test]
    fn exec_map_string_int() {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let map_ty = || Ty::Applied(TC::Map, vec![Ty::String, Ty::Int]);
        let opt_ty = || Ty::Applied(TC::Option, vec![Ty::Int]);
        let lit = || IrExpr {
            kind: IrExprKind::MapLiteral { entries: vec![
                (lit_str("a"), lit_int(1)), (lit_str("b"), lit_int(2)), (lit_str("c"), lit_int(3)),
            ] },
            ty: map_ty(), span: None, def_id: None,
        };
        let get_or = |key: &str, d: i64| {
            let g = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_map_get"), args: vec![lit(), lit_str(key)] }, ty: opt_ty(), span: None, def_id: None };
            IrExpr { kind: IrExprKind::UnwrapOr { expr: Box::new(g), fallback: Box::new(lit_int(d)) }, ty: Ty::Int, span: None, def_id: None }
        };
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };
        let tb = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Bool, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };
        ti(get_or("b", -1), "2", "get b");
        ti(get_or("z", -1), "-1", "miss z");
        tb(IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_map_contains"), args: vec![lit(), lit_str("c")] }, ty: Ty::Bool, span: None, def_id: None }, "1", "contains c");
        ti(IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_map_len"), args: vec![lit()] }, ty: Ty::Int, span: None, def_id: None }, "3", "len");
        // distinct keys that may collide under FNV must stay separate
        ti(get_or("a", -1), "1", "get a");
        ti(get_or("c", -1), "3", "get c");
    }

    /// Stdlib intrinsics dispatched via the registry (Tier 1):
    /// string.len("hello")==5, list.len([10,20,30])==3.
    #[test]
    fn exec_intrinsic_len() {
        let s = mk_func("main", Ty::Int, rt_call("almide_rt_string_len", vec![lit_str("hello")]));
        if let Some(r) = run(&[s], "main") { assert_eq!(r, "5"); }

        let list = IrExpr {
            kind: IrExprKind::List { elements: vec![lit_int(10), lit_int(20), lit_int(30)] },
            ty: Ty::list(Ty::Int), span: None, def_id: None,
        };
        let l = mk_func("main", Ty::Int, rt_call("almide_rt_list_len", vec![list]));
        if let Some(r) = run(&[l], "main") { assert_eq!(r, "3"); }
    }

    /// list.map with an inline lambda: `[1,2,3].map(x => x*2)` → [2,4,6];
    /// indexing the result at 1 gives 4.
    #[test]
    fn exec_intrinsic_list_map() {
        let mut vt = VarTable::new();
        let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
        let list = IrExpr {
            kind: IrExprKind::List { elements: vec![lit_int(1), lit_int(2), lit_int(3)] },
            ty: Ty::list(Ty::Int), span: None, def_id: None,
        };
        let lam = IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(x, Ty::Int)],
                body: Box::new(binop(almide_ir::BinOp::MulInt,
                    IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None },
                    lit_int(2), Ty::Int)),
                lambda_id: None,
            },
            ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) }, span: None, def_id: None,
        };
        let mapped = IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_map"), args: vec![list, lam] },
            ty: Ty::list(Ty::Int), span: None, def_id: None,
        };
        let index = IrExpr {
            kind: IrExprKind::IndexAccess { object: Box::new(mapped), index: Box::new(lit_int(1)) },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, index);
        if let Some(r) = run_vt(&[main], &vt, "main") {
            assert_eq!(r, "4", "[1,2,3].map(x=>x*2)[1]");
        }
    }

    /// Build a predicate-HOF call `symbol(list, (x) => x CMP n)` with result ty.
    fn pred_call(symbol: &str, elems: Vec<i64>, op: almide_ir::BinOp, n: i64, ret: Ty) -> (VarTable, IrExpr) {
        let mut vt = VarTable::new();
        let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
        let list = IrExpr {
            kind: IrExprKind::List { elements: elems.into_iter().map(lit_int).collect() },
            ty: Ty::list(Ty::Int), span: None, def_id: None,
        };
        let lam = IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(x, Ty::Int)],
                body: Box::new(binop(op,
                    IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None },
                    lit_int(n), Ty::Bool)),
                lambda_id: None,
            },
            ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Bool) }, span: None, def_id: None,
        };
        let call = IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args: vec![list, lam] },
            ty: ret, span: None, def_id: None,
        };
        (vt, call)
    }

    /// list.any / list.all with predicates.
    #[test]
    fn exec_intrinsic_any_all() {
        use almide_ir::BinOp::Gt;
        let cases: &[(&str, Vec<i64>, i64, &str)] = &[
            ("almide_rt_list_any", vec![1, 2, 3], 2, "1"),  // any > 2 → true
            ("almide_rt_list_any", vec![1, 2], 5, "0"),     // any > 5 → false
            ("almide_rt_list_all", vec![2, 4, 6], 1, "1"),  // all > 1 → true
            ("almide_rt_list_all", vec![2, 4, 6], 3, "0"),  // all > 3 → false
        ];
        for (sym_, elems, n, expect) in cases {
            let (vt, call) = pred_call(sym_, elems.clone(), Gt, *n, Ty::Bool);
            let main = mk_func("main", Ty::Bool, call);
            if let Some(r) = run_vt(&[main], &vt, "main") {
                assert_eq!(&r, expect, "{} {:?} > {}", sym_, elems, n);
            }
        }
    }

    /// list.flat_map: `[1,2] |> flat_map(x => [x, x*10])` → [1,10,2,20].
    #[test]
    fn exec_intrinsic_flat_map() {
        let mk = |i: i64| {
            let mut vt = VarTable::new();
            let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
            let vx = || IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None };
            let sublist = IrExpr {
                kind: IrExprKind::List { elements: vec![vx(), binop(almide_ir::BinOp::MulInt, vx(), lit_int(10), Ty::Int)] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            let lam = IrExpr {
                kind: IrExprKind::Lambda { params: vec![(x, Ty::Int)], body: Box::new(sublist), lambda_id: None },
                ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::list(Ty::Int)) }, span: None, def_id: None,
            };
            let xs = IrExpr {
                kind: IrExprKind::List { elements: vec![lit_int(1), lit_int(2)] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            let fm = IrExpr {
                kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_flat_map"), args: vec![xs, lam] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            (vt, IrExpr { kind: IrExprKind::IndexAccess { object: Box::new(fm), index: Box::new(lit_int(i)) }, ty: Ty::Int, span: None, def_id: None })
        };
        for (i, exp) in [(0, "1"), (1, "10"), (2, "2"), (3, "20")] {
            let (vt, e) = mk(i);
            let m = mk_func("main", Ty::Int, e);
            if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(&r, exp, "flat_map[{}]", i); }
        }
    }

    /// list.reverse: `[1,2,3].reverse()` → [3,2,1]; [0]==3, [2]==1.
    #[test]
    fn exec_intrinsic_reverse() {
        let mk = |i: i64| {
            let list = IrExpr {
                kind: IrExprKind::List { elements: vec![lit_int(1), lit_int(2), lit_int(3)] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            let rev = IrExpr {
                kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_reverse"), args: vec![list] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            IrExpr {
                kind: IrExprKind::IndexAccess { object: Box::new(rev), index: Box::new(lit_int(i)) },
                ty: Ty::Int, span: None, def_id: None,
            }
        };
        let m0 = mk_func("main", Ty::Int, mk(0));
        if let Some(r) = run(&[m0], "main") { assert_eq!(r, "3", "reverse[0]"); }
        let m2 = mk_func("main", Ty::Int, mk(2));
        if let Some(r) = run(&[m2], "main") { assert_eq!(r, "1", "reverse[2]"); }
    }

    /// list.filter_map: `[1,2,3,4].filter_map(x => if x>2 then Some(x*10) else None)`
    /// → [30,40]; length 2, [0]==30.
    #[test]
    fn exec_intrinsic_filter_map() {
        let opt_int = Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::Option, vec![Ty::Int]);
        let mk_fm = || {
            let mut vt = VarTable::new();
            let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
            let vx = || IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None };
            let body = IrExpr {
                kind: IrExprKind::If {
                    cond: Box::new(binop(almide_ir::BinOp::Gt, vx(), lit_int(2), Ty::Bool)),
                    then: Box::new(IrExpr {
                        kind: IrExprKind::OptionSome {
                            expr: Box::new(binop(almide_ir::BinOp::MulInt, vx(), lit_int(10), Ty::Int)),
                        },
                        ty: opt_int.clone(), span: None, def_id: None,
                    }),
                    else_: Box::new(IrExpr { kind: IrExprKind::OptionNone, ty: opt_int.clone(), span: None, def_id: None }),
                },
                ty: opt_int.clone(), span: None, def_id: None,
            };
            let lam = IrExpr {
                kind: IrExprKind::Lambda { params: vec![(x, Ty::Int)], body: Box::new(body), lambda_id: None },
                ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(opt_int.clone()) }, span: None, def_id: None,
            };
            let list = IrExpr {
                kind: IrExprKind::List { elements: vec![lit_int(1), lit_int(2), lit_int(3), lit_int(4)] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            let fm = IrExpr {
                kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_filter_map"), args: vec![list, lam] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            (vt, fm)
        };
        let (vt, fm) = mk_fm();
        let len = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_len"), args: vec![fm] }, ty: Ty::Int, span: None, def_id: None };
        let m = mk_func("main", Ty::Int, len);
        if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(r, "2", "filter_map len"); }
        let (vt2, fm2) = mk_fm();
        let idx0 = IrExpr { kind: IrExprKind::IndexAccess { object: Box::new(fm2), index: Box::new(lit_int(0)) }, ty: Ty::Int, span: None, def_id: None };
        let m2 = mk_func("main", Ty::Int, idx0);
        if let Some(r) = run_vt(&[m2], &vt2, "main") { assert_eq!(r, "30", "filter_map[0]"); }
    }

    /// list.find returns Option: `[1,2,3].find(x=>x>1) ?? -1` → 2 (Some(2));
    /// `[1,2,3].find(x=>x>5) ?? -1` → -1 (None).
    #[test]
    fn exec_intrinsic_find() {
        let opt_int = Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::Option, vec![Ty::Int]);
        let build = |n: i64| {
            let (vt, find) = pred_call("almide_rt_list_find", vec![1, 2, 3], almide_ir::BinOp::Gt, n, opt_int.clone());
            let unwrap_or = IrExpr {
                kind: IrExprKind::UnwrapOr { expr: Box::new(find), fallback: Box::new(lit_int(-1)) },
                ty: Ty::Int, span: None, def_id: None,
            };
            (vt, unwrap_or)
        };
        let (vt, found) = build(1);
        let main = mk_func("main", Ty::Int, found);
        if let Some(r) = run_vt(&[main], &vt, "main") { assert_eq!(r, "2", "find(x>1)"); }
        let (vt2, none) = build(5);
        let main2 = mk_func("main", Ty::Int, none);
        if let Some(r) = run_vt(&[main2], &vt2, "main") { assert_eq!(r, "-1", "find(x>5)"); }
    }

    /// list.count: `[1,2,3,4].count(x => x > 2)` → 2.
    #[test]
    fn exec_intrinsic_count() {
        let (vt, call) = pred_call("almide_rt_list_count", vec![1, 2, 3, 4], almide_ir::BinOp::Gt, 2, Ty::Int);
        let main = mk_func("main", Ty::Int, call);
        if let Some(r) = run_vt(&[main], &vt, "main") { assert_eq!(r, "2"); }
    }

    /// list.filter with an inline lambda: `[1,2,3,4].filter(x => x > 2)` →
    /// [3,4]; length 2 and element 0 is 3.
    #[test]
    fn exec_intrinsic_list_filter() {
        let mk_filtered = || {
            let mut vt = VarTable::new();
            let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
            let list = IrExpr {
                kind: IrExprKind::List { elements: vec![lit_int(1), lit_int(2), lit_int(3), lit_int(4)] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            let lam = IrExpr {
                kind: IrExprKind::Lambda {
                    params: vec![(x, Ty::Int)],
                    body: Box::new(binop(almide_ir::BinOp::Gt,
                        IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None },
                        lit_int(2), Ty::Bool)),
                    lambda_id: None,
                },
                ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Bool) }, span: None, def_id: None,
            };
            let filtered = IrExpr {
                kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_filter"), args: vec![list, lam] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            (vt, filtered)
        };
        // length == 2
        let (vt, f) = mk_filtered();
        let len = IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_len"), args: vec![f] },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, len);
        if let Some(r) = run_vt(&[main], &vt, "main") { assert_eq!(r, "2", "filter len"); }
        // element 0 == 3
        let (vt2, f2) = mk_filtered();
        let idx0 = IrExpr {
            kind: IrExprKind::IndexAccess { object: Box::new(f2), index: Box::new(lit_int(0)) },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main2 = mk_func("main", Ty::Int, idx0);
        if let Some(r) = run_vt(&[main2], &vt2, "main") { assert_eq!(r, "3", "filter[0]"); }
    }

    /// list.fold with an inline lambda: `[1,2,3,4].fold(0, (a,x) => a+x)` → 10.
    #[test]
    fn exec_intrinsic_list_fold() {
        let mut vt = VarTable::new();
        let acc = vt.alloc(sym("acc"), Ty::Int, almide_ir::Mutability::Let, None);
        let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
        let v = |id| IrExpr { kind: IrExprKind::Var { id }, ty: Ty::Int, span: None, def_id: None };
        let list = IrExpr {
            kind: IrExprKind::List { elements: vec![lit_int(1), lit_int(2), lit_int(3), lit_int(4)] },
            ty: Ty::list(Ty::Int), span: None, def_id: None,
        };
        let lam = IrExpr {
            kind: IrExprKind::Lambda {
                params: vec![(acc, Ty::Int), (x, Ty::Int)],
                body: Box::new(binop(almide_ir::BinOp::AddInt, v(acc), v(x), Ty::Int)),
                lambda_id: None,
            },
            ty: Ty::Fn { params: vec![Ty::Int, Ty::Int], ret: Box::new(Ty::Int) }, span: None, def_id: None,
        };
        let folded = IrExpr {
            kind: IrExprKind::RuntimeCall {
                symbol: sym("almide_rt_list_fold"),
                args: vec![list, lit_int(0), lam],
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, folded);
        if let Some(r) = run_vt(&[main], &vt, "main") {
            assert_eq!(r, "10", "[1,2,3,4].fold(0,+)");
        }
    }

    /// Integer min/max/abs intrinsics: abs(-5)==5, min(3,7)==3, max(3,7)==7.
    #[test]
    fn exec_intrinsic_int_minmax_abs() {
        let abs = mk_func("main", Ty::Int, rt_call("almide_rt_int_abs", vec![lit_int(-5)]));
        if let Some(r) = run(&[abs], "main") { assert_eq!(r, "5", "abs(-5)"); }
        let mn = mk_func("main", Ty::Int, rt_call("almide_rt_int_min", vec![lit_int(3), lit_int(7)]));
        if let Some(r) = run(&[mn], "main") { assert_eq!(r, "3", "min(3,7)"); }
        let mx = mk_func("main", Ty::Int, rt_call("almide_rt_int_max", vec![lit_int(3), lit_int(7)]));
        if let Some(r) = run(&[mx], "main") { assert_eq!(r, "7", "max(3,7)"); }
    }

    /// string.get(s, i): Some(char) in range, None out of range.
    /// `"hi".get(1)` → Some("i") (len 1, byte 'i'); `"hi".get(5)` → None.
    #[test]
    fn exec_intrinsic_string_get() {
        let get = |s: &str, i: i64| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_string_char_at"), args: vec![lit_str(s), lit_int(i)] },
            ty: Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, vec![Ty::String]),
            span: None, def_id: None };
        // is_some("hi".get(1)) == 1 ; is_some("hi".get(5)) == 0
        let is_some = |e: IrExpr| IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_option_is_some"), args: vec![e] }, ty: Ty::Bool, span: None, def_id: None };
        let m1 = mk_func("main", Ty::Bool, is_some(get("hi", 1)));
        if let Some(r) = run(&[m1], "main") { assert_eq!(r, "1", "get(1) is some"); }
        let m2 = mk_func("main", Ty::Bool, is_some(get("hi", 5)));
        if let Some(r) = run(&[m2], "main") { assert_eq!(r, "0", "get(5) is none"); }
        // unwrap the Some and check the byte: "hi".get(1) → "i" → byte 0 == 105
        let unwrapped = IrExpr { kind: IrExprKind::Unwrap { expr: Box::new(get("hi", 1)) }, ty: Ty::String, span: None, def_id: None };
        let byte0 = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("__byte_at"), args: vec![unwrapped, lit_int(0)] }, ty: Ty::Int, span: None, def_id: None };
        let m3 = mk_func("main", Ty::Int, byte0);
        if let Some(r) = run(&[m3], "main") { assert_eq!(r, "105", "get(1)=='i'"); }
    }

    /// result.map_err: Ok passes through, Err is mapped.
    #[test]
    fn exec_intrinsic_result_map_err() {
        let res_ii = Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, vec![Ty::Int, Ty::Int]);
        let build = |is_ok: bool| {
            let mut vt = VarTable::new();
            let e = vt.alloc(sym("e"), Ty::Int, almide_ir::Mutability::Let, None);
            let inner = if is_ok {
                IrExpr { kind: IrExprKind::ResultOk { expr: Box::new(lit_int(5)) }, ty: res_ii.clone(), span: None, def_id: None }
            } else {
                IrExpr { kind: IrExprKind::ResultErr { expr: Box::new(lit_int(3)) }, ty: res_ii.clone(), span: None, def_id: None }
            };
            let lam = IrExpr {
                kind: IrExprKind::Lambda {
                    params: vec![(e, Ty::Int)],
                    body: Box::new(binop(almide_ir::BinOp::AddInt,
                        IrExpr { kind: IrExprKind::Var { id: e }, ty: Ty::Int, span: None, def_id: None },
                        lit_int(100), Ty::Int)),
                    lambda_id: None,
                },
                ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) }, span: None, def_id: None,
            };
            let mapped = IrExpr {
                kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_result_map_err"), args: vec![inner, lam] },
                ty: res_ii.clone(), span: None, def_id: None,
            };
            (vt, mapped)
        };
        // Ok(5).map_err(+100) ?? -1 == 5 (Ok preserved)
        let (vt, ok) = build(true);
        let uo = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_result_unwrap_or"), args: vec![ok, lit_int(-1)] }, ty: Ty::Int, span: None, def_id: None };
        let m = mk_func("main", Ty::Int, uo);
        if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(r, "5", "map_err Ok passthrough"); }
        // Err(3).map_err(+100) is still Err
        let (vt2, err) = build(false);
        let ie = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_result_is_err"), args: vec![err] }, ty: Ty::Bool, span: None, def_id: None };
        let m2 = mk_func("main", Ty::Bool, ie);
        if let Some(r) = run_vt(&[m2], &vt2, "main") { assert_eq!(r, "1", "map_err keeps Err"); }
    }

    /// string.slice by code points (UTF-8): slice("hello",1,4)=="ell" (len 3,
    /// [0]=='e'); slice("café",3,4)=="é" (1 code point).
    #[test]
    fn exec_intrinsic_string_slice() {
        let slice = |s: &str, a: i64, b: i64| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_string_slice"), args: vec![lit_str(s), lit_int(a), lit_int(b)] },
            ty: Ty::String, span: None, def_id: None };
        let len_of = |e: IrExpr| IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_string_len"), args: vec![e] }, ty: Ty::Int, span: None, def_id: None };
        let byte_of = |e: IrExpr, i: i64| IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("__byte_at"), args: vec![e, lit_int(i)] }, ty: Ty::Int, span: None, def_id: None };

        let m1 = mk_func("main", Ty::Int, len_of(slice("hello", 1, 4)));
        if let Some(r) = run(&[m1], "main") { assert_eq!(r, "3", "slice hello[1,4] len"); }
        let m2 = mk_func("main", Ty::Int, byte_of(slice("hello", 1, 4), 0));
        if let Some(r) = run(&[m2], "main") { assert_eq!(r, "101", "slice[0]=='e'"); }
        // café code points: c a f é ; slice(3,4) = "é" → 1 code point.
        let m3 = mk_func("main", Ty::Int, len_of(slice("café", 3, 4)));
        if let Some(r) = run(&[m3], "main") { assert_eq!(r, "1", "slice café[3,4] len chars"); }
    }

    /// Set HOFs: fold/any/all/filter/map with inline lambdas.
    #[test]
    fn exec_set_hof() {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let set_i = Ty::Applied(TC::Set, vec![Ty::Int]);
        // from_list([..]) → Set[Int]
        let from_i = |xs: &[i64]| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_set_from_list"),
                args: vec![IrExpr { kind: IrExprKind::List { elements: xs.iter().map(|n| lit_int(*n)).collect() },
                                    ty: Ty::list(Ty::Int), span: None, def_id: None }] },
            ty: set_i.clone(), span: None, def_id: None };
        let var = |id| IrExpr { kind: IrExprKind::Var { id }, ty: Ty::Int, span: None, def_id: None };

        // fold(from_list([1,2,3]), 0, (acc,x) => acc + x) → 6
        {
            let mut vt = VarTable::new();
            let acc = vt.alloc(sym("acc"), Ty::Int, almide_ir::Mutability::Let, None);
            let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
            let lam = IrExpr { kind: IrExprKind::Lambda { params: vec![(acc, Ty::Int), (x, Ty::Int)],
                body: Box::new(binop(almide_ir::BinOp::AddInt, var(acc), var(x), Ty::Int)), lambda_id: None },
                ty: Ty::Fn { params: vec![Ty::Int, Ty::Int], ret: Box::new(Ty::Int) }, span: None, def_id: None };
            let call = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_set_fold"),
                args: vec![from_i(&[1, 2, 3]), lit_int(0), lam] }, ty: Ty::Int, span: None, def_id: None };
            let m = mk_func("main", Ty::Int, call);
            if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(r, "6", "set.fold sum"); }
        }

        // predicate-lambda helper: (x) => x <op> n, applied to set via `symbol`,
        // result wrapped by `wrap` (e.g. len) to an Int we can print.
        let pred = |symbol: &str, xs: &[i64], op: almide_ir::BinOp, n: i64, ret: Ty| {
            let mut vt = VarTable::new();
            let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
            let lam = IrExpr { kind: IrExprKind::Lambda { params: vec![(x, Ty::Int)],
                body: Box::new(binop(op, var(x), lit_int(n), Ty::Bool)), lambda_id: None },
                ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Bool) }, span: None, def_id: None };
            let call = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym(symbol),
                args: vec![from_i(xs), lam] }, ty: ret, span: None, def_id: None };
            (vt, call)
        };
        let run_b = |vt: VarTable, e: IrExpr, exp: &str, msg: &str| {
            let m = mk_func("main", Ty::Bool, e);
            if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(&r, exp, "{}", msg); }
        };

        // any / all (Bool)
        let (vt, e) = pred("almide_rt_set_any", &[1, 2, 3], almide_ir::BinOp::Gt, 2, Ty::Bool);
        run_b(vt, e, "1", "any x>2");
        let (vt, e) = pred("almide_rt_set_any", &[1, 2, 3], almide_ir::BinOp::Gt, 5, Ty::Bool);
        run_b(vt, e, "0", "any x>5");
        let (vt, e) = pred("almide_rt_set_all", &[1, 2, 3], almide_ir::BinOp::Gt, 0, Ty::Bool);
        run_b(vt, e, "1", "all x>0");
        let (vt, e) = pred("almide_rt_set_all", &[1, 2, 3], almide_ir::BinOp::Gt, 1, Ty::Bool);
        run_b(vt, e, "0", "all x>1");

        // filter(set, x=>x>2) → Set; len == 2, and 3 present / 1 absent
        let filt_len = |xs: &[i64], n: i64| {
            let (mut vt, set) = pred("almide_rt_set_filter", xs, almide_ir::BinOp::Gt, n, set_i.clone());
            let _ = &mut vt;
            let len = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_set_len"), args: vec![set] },
                ty: Ty::Int, span: None, def_id: None };
            (vt, len)
        };
        let (vt, e) = filt_len(&[1, 2, 3, 4], 2);
        let m = mk_func("main", Ty::Int, e);
        if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(r, "2", "set.filter len"); }

        // map(set, x=>x*10) → Set; len == 3
        {
            let mut vt = VarTable::new();
            let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
            let lam = IrExpr { kind: IrExprKind::Lambda { params: vec![(x, Ty::Int)],
                body: Box::new(binop(almide_ir::BinOp::MulInt, var(x), lit_int(10), Ty::Int)), lambda_id: None },
                ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) }, span: None, def_id: None };
            let mapped = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_set_map"),
                args: vec![from_i(&[1, 2, 3]), lam] }, ty: set_i.clone(), span: None, def_id: None };
            let len = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_set_len"), args: vec![mapped] },
                ty: Ty::Int, span: None, def_id: None };
            let m = mk_func("main", Ty::Int, len);
            if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(r, "3", "set.map len"); }
        }
    }

    /// Set core ops (Set[A] = Map[A,A]): from_list/len/contains/insert/remove/
    /// is_empty/union/intersection/difference/symmetric_difference/is_subset/
    /// is_disjoint/to_list, for Int and String elements. No lambdas.
    #[test]
    fn exec_set_core() {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let set_i = Ty::Applied(TC::Set, vec![Ty::Int]);
        let set_s = Ty::Applied(TC::Set, vec![Ty::String]);
        let call = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };
        let from_i = |xs: &[i64]| call("almide_rt_set_from_list",
            vec![IrExpr { kind: IrExprKind::List { elements: xs.iter().map(|n| lit_int(*n)).collect() },
                          ty: Ty::list(Ty::Int), span: None, def_id: None }], set_i.clone());
        let from_s = |xs: &[&str]| call("almide_rt_set_from_list",
            vec![IrExpr { kind: IrExprKind::List { elements: xs.iter().map(|s| lit_str(s)).collect() },
                          ty: Ty::list(Ty::String), span: None, def_id: None }], set_s.clone());
        let len = |s: IrExpr| call("almide_rt_set_len", vec![s], Ty::Int);
        let contains = |s: IrExpr, v: i64| call("almide_rt_set_contains", vec![s, lit_int(v)], Ty::Bool);
        let contains_s = |s: IrExpr, v: &str| call("almide_rt_set_contains", vec![s, lit_str(v)], Ty::Bool);
        let bin = |a: IrExpr, b: IrExpr, op: &str, ty: Ty| call(op, vec![a, b], ty);
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };
        let tb = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Bool, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };

        // dedup on construction + membership
        ti(len(from_i(&[1, 2, 3, 2, 1])), "3", "from_list dedup len");
        tb(contains(from_i(&[1, 2, 3]), 2), "1", "contains hit");
        tb(contains(from_i(&[1, 2, 3]), 5), "0", "contains miss");
        // is_empty
        tb(call("almide_rt_set_is_empty", vec![call("almide_rt_set_new", vec![], set_i.clone())], Ty::Bool), "1", "new is_empty");
        tb(call("almide_rt_set_is_empty", vec![from_i(&[1])], Ty::Bool), "0", "non-empty");
        // insert / remove
        ti(len(call("almide_rt_set_insert", vec![from_i(&[1, 2]), lit_int(9)], set_i.clone())), "3", "insert grows");
        ti(len(call("almide_rt_set_insert", vec![from_i(&[1, 2]), lit_int(1)], set_i.clone())), "2", "insert dup no-op");
        let removed = || call("almide_rt_set_remove", vec![from_i(&[1, 2, 3]), lit_int(2)], set_i.clone());
        ti(len(removed()), "2", "remove len");
        tb(contains(removed(), 2), "0", "removed gone");
        // union / intersection / difference / symmetric_difference
        ti(len(bin(from_i(&[1, 2]), from_i(&[2, 3]), "almide_rt_set_union", set_i.clone())), "3", "union len");
        let inter = || bin(from_i(&[1, 2, 3]), from_i(&[2, 3, 4]), "almide_rt_set_intersection", set_i.clone());
        ti(len(inter()), "2", "intersection len");
        tb(contains(inter(), 2), "1", "intersection has 2");
        tb(contains(inter(), 1), "0", "intersection lacks 1");
        let diff = || bin(from_i(&[1, 2, 3]), from_i(&[2, 3, 4]), "almide_rt_set_difference", set_i.clone());
        ti(len(diff()), "1", "difference len");
        tb(contains(diff(), 1), "1", "difference has 1");
        tb(contains(diff(), 2), "0", "difference lacks 2");
        let sym = || bin(from_i(&[1, 2, 3]), from_i(&[2, 3, 4]), "almide_rt_set_symmetric_difference", set_i.clone());
        ti(len(sym()), "2", "sym_diff len");
        tb(contains(sym(), 1), "1", "sym_diff has 1");
        tb(contains(sym(), 4), "1", "sym_diff has 4");
        tb(contains(sym(), 3), "0", "sym_diff lacks 3");
        // is_subset / is_disjoint
        tb(bin(from_i(&[1, 2]), from_i(&[1, 2, 3]), "almide_rt_set_is_subset", Ty::Bool), "1", "subset yes");
        tb(bin(from_i(&[1, 4]), from_i(&[1, 2, 3]), "almide_rt_set_is_subset", Ty::Bool), "0", "subset no");
        tb(bin(from_i(&[1, 2]), from_i(&[3, 4]), "almide_rt_set_is_disjoint", Ty::Bool), "1", "disjoint yes");
        tb(bin(from_i(&[1, 2]), from_i(&[2, 3]), "almide_rt_set_is_disjoint", Ty::Bool), "0", "disjoint no");
        // to_list → List[Int], check its length
        ti(call("almide_rt_string_len", vec![lit_str("x")], Ty::Int), "1", "harness sanity"); // (unrelated guard)
        ti(call("almide_rt_list_len", vec![call("almide_rt_set_to_list", vec![from_i(&[7, 8, 9])], Ty::list(Ty::Int))], Ty::Int), "3", "to_list len");
        // String elements (FNV hash + __string_eq path)
        ti(len(from_s(&["a", "b", "a"])), "2", "string set dedup");
        tb(contains_s(from_s(&["a", "b"]), "a"), "1", "string contains hit");
        tb(contains_s(from_s(&["a", "b"]), "c"), "0", "string contains miss");
    }

    /// string.split / join (+ list.join), including a split→join round-trip.
    #[test]
    fn exec_string_split_join() {
        let call = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };
        let len = |e: IrExpr| call("almide_rt_string_len", vec![e], Ty::Int);
        let byte = |e: IrExpr, i: i64| call("__byte_at", vec![e, lit_int(i)], Ty::Int);
        let str_list = |xs: &[&str]| IrExpr {
            kind: IrExprKind::List { elements: xs.iter().map(|s| lit_str(s)).collect() },
            ty: Ty::list(Ty::String), span: None, def_id: None };
        let join = |list: IrExpr, sep: &str| call("almide_rt_string_join", vec![list, lit_str(sep)], Ty::String);
        let split = |s: &str, sep: &str| call("almide_rt_string_split", vec![lit_str(s), lit_str(sep)], Ty::list(Ty::String));
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };

        // join(["foo","bar","baz"], "-") → "foo-bar-baz" (len 11)
        ti(len(join(str_list(&["foo", "bar", "baz"]), "-")), "11", "join len");
        ti(byte(join(str_list(&["foo", "bar", "baz"]), "-"), 3), "45", "join sep '-'");
        // join edge cases
        ti(len(join(str_list(&[]), "-")), "0", "join empty list");
        ti(len(join(str_list(&["x"]), "-")), "1", "join single (no sep)");
        // list.join routes to the same runtime fn
        ti(len(call("almide_rt_list_join", vec![str_list(&["ab", "cd"]), lit_str(", ")], Ty::String)), "6", "list.join len");
        // split→join round-trip: split("a,bb,ccc", ",") |> join("/") → "a/bb/ccc" (len 8)
        ti(len(join(split("a,bb,ccc", ","), "/")), "8", "round-trip len");
        ti(byte(join(split("a,bb,ccc", ","), "/"), 1), "47", "round-trip sep '/'");
        ti(byte(join(split("a,bb,ccc", ","), "/"), 0), "97", "round-trip [0]=='a'");
        // split with no match → single piece
        ti(len(join(split("hello", ","), "_")), "5", "split no-match → [s]");
        // empty separator → [s]
        ti(len(join(split("abc", ""), "_")), "3", "split empty sep → [s]");
        // trailing separator → trailing empty piece: split("a,", ",") → ["a",""]
        ti(len(join(split("a,", ","), "X")), "2", "trailing sep → a + X + empty");
        ti(byte(join(split("a,", ","), "X"), 1), "88", "trailing sep join [1]=='X'");
    }

    /// string.replace / replace_first.
    #[test]
    fn exec_string_replace() {
        let call = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };
        let rep = |sym_: &str, s: &str, from: &str, to: &str| call(sym_, vec![lit_str(s), lit_str(from), lit_str(to)], Ty::String);
        let len = |e: IrExpr| call("almide_rt_string_len", vec![e], Ty::Int);
        let byte = |e: IrExpr, i: i64| call("__byte_at", vec![e, lit_int(i)], Ty::Int);
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };

        // replace all "a"→"X" in "banana" → "bXnXnX" (len 6)
        let r = || rep("almide_rt_string_replace", "banana", "a", "X");
        ti(len(r()), "6", "replace all len");
        ti(byte(r(), 1), "88", "replace all [1]=='X'");
        ti(byte(r(), 0), "98", "replace all [0]=='b'");
        // grow: replace "a"→"yy" in "aa" → "yyyy" (len 4)
        ti(len(rep("almide_rt_string_replace", "aa", "a", "yy")), "4", "replace grow len");
        // shrink: replace "ab"→"" in "abXab" → "X" (len 1)
        ti(len(rep("almide_rt_string_replace", "abXab", "ab", "")), "1", "replace shrink len");
        ti(byte(rep("almide_rt_string_replace", "abXab", "ab", ""), 0), "88", "replace shrink [0]=='X'");
        // replace_first only: "banana" "a"→"X" → "bXnana" (len 6, [3]=='a')
        let rf = || rep("almide_rt_string_replace_first", "banana", "a", "X");
        ti(len(rf()), "6", "replace_first len");
        ti(byte(rf(), 1), "88", "replace_first [1]=='X'");
        ti(byte(rf(), 3), "97", "replace_first [3]=='a' (unchanged)");
        // no match → unchanged
        ti(len(rep("almide_rt_string_replace", "hello", "z", "Q")), "5", "replace no-match len");
        // multi-char replacement with longer target: "cat"→"dog" in "cat cat"
        let m = || rep("almide_rt_string_replace", "cat cat", "cat", "dog");
        ti(len(m()), "7", "replace word len");
        ti(byte(m(), 0), "100", "replace word [0]=='d'");
        ti(byte(m(), 4), "100", "replace word [4]=='d'");
    }

    /// string.index_of / last_index_of → Option[Int] (code-point index).
    #[test]
    fn exec_string_index_of() {
        let opt_int = Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, vec![Ty::Int]);
        let idx = |symbol: &str, s: &str, sub: &str| {
            let call = IrExpr {
                kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args: vec![lit_str(s), lit_str(sub)] },
                ty: opt_int.clone(), span: None, def_id: None };
            IrExpr { kind: IrExprKind::UnwrapOr { expr: Box::new(call), fallback: Box::new(lit_int(-1)) },
                     ty: Ty::Int, span: None, def_id: None }
        };
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };

        ti(idx("almide_rt_string_index_of", "hello", "ll"), "2", "index_of ll");
        ti(idx("almide_rt_string_index_of", "hello", "h"), "0", "index_of h");
        ti(idx("almide_rt_string_index_of", "hello", "z"), "-1", "index_of miss → None");
        // UTF-8: "héllo" — é is one code point (2 bytes); "llo" starts at cp index 2
        ti(idx("almide_rt_string_index_of", "héllo", "llo"), "2", "index_of utf8 cp index");
        // last_index_of: overlapping "aba" in "ababa" → last at cp 2
        ti(idx("almide_rt_string_last_index_of", "ababa", "aba"), "2", "last_index_of aba");
        ti(idx("almide_rt_string_last_index_of", "hello", "l"), "3", "last_index_of l");
        ti(idx("almide_rt_string_last_index_of", "hello", "z"), "-1", "last_index_of miss → None");
    }

    /// string.trim / trim_start / trim_end.
    #[test]
    fn exec_string_trim() {
        let call = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };
        let len = |e: IrExpr| call("almide_rt_string_len", vec![e], Ty::Int);
        let byte = |e: IrExpr, i: i64| call("__byte_at", vec![e, lit_int(i)], Ty::Int);
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };

        // trim("  hi  ") → "hi" (len 2, bytes 'h'(104) 'i'(105))
        let tr = |sym_: &str, s: &str| call(sym_, vec![lit_str(s)], Ty::String);
        ti(len(tr("almide_rt_string_trim", "  hi  ")), "2", "trim len");
        ti(byte(tr("almide_rt_string_trim", "  hi  "), 0), "104", "trim[0]=='h'");
        ti(byte(tr("almide_rt_string_trim", "\t\nhi\r "), 1), "105", "trim mixed ws [1]=='i'");
        // trim_start leaves trailing ws: "  hi  " → "hi  " len 4
        ti(len(tr("almide_rt_string_trim_start", "  hi  ")), "4", "trim_start len");
        ti(byte(tr("almide_rt_string_trim_start", "  hi  "), 0), "104", "trim_start[0]=='h'");
        // trim_end leaves leading ws: "  hi  " → "  hi" len 4
        ti(len(tr("almide_rt_string_trim_end", "  hi  ")), "4", "trim_end len");
        ti(byte(tr("almide_rt_string_trim_end", "  hi  "), 0), "32", "trim_end[0]==' '");
        // all whitespace → empty
        ti(len(tr("almide_rt_string_trim", "   ")), "0", "trim all ws → empty");
        // no whitespace → unchanged
        ti(len(tr("almide_rt_string_trim", "abc")), "3", "trim no-ws len");
    }

    /// float.to_string — fixed 6-decimal, trailing zeros trimmed.
    #[test]
    fn exec_float_to_string() {
        let lit_float = |v: f64| IrExpr { kind: IrExprKind::LitFloat { value: v }, ty: Ty::Float, span: None, def_id: None };
        let call = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };
        let fts = |v: f64| call("almide_rt_float_to_string", vec![lit_float(v)], Ty::String);
        let byte = |e: IrExpr, i: i64| call("__byte_at", vec![e, lit_int(i)], Ty::Int);
        let len = |e: IrExpr| call("almide_rt_string_len", vec![e], Ty::Int);
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };

        // 1.5 → "1.5" : len 3, bytes '1'(49) '.'(46) '5'(53)
        ti(len(fts(1.5)), "3", "1.5 len");
        ti(byte(fts(1.5), 0), "49", "1.5[0]");
        ti(byte(fts(1.5), 1), "46", "1.5[1]");
        ti(byte(fts(1.5), 2), "53", "1.5[2]");
        // 2.0 → "2.0" : trailing zeros trimmed to one frac digit
        ti(len(fts(2.0)), "3", "2.0 len");
        ti(byte(fts(2.0), 2), "48", "2.0 frac '0'");
        // 3.14 → "3.14"
        ti(len(fts(3.14)), "4", "3.14 len");
        ti(byte(fts(3.14), 3), "52", "3.14[3]=='4'");
        // -0.5 → "-0.5" : sign even when int part is 0
        ti(len(fts(-0.5)), "4", "-0.5 len");
        ti(byte(fts(-0.5), 0), "45", "-0.5[0]=='-'");
        ti(byte(fts(-0.5), 1), "48", "-0.5[1]=='0'");
        ti(byte(fts(-0.5), 3), "53", "-0.5[3]=='5'");
        // 0.0 → "0.0"
        ti(len(fts(0.0)), "3", "0.0 len");
    }

    /// string.to_upper / to_lower / repeat / contains.
    #[test]
    fn exec_string_transforms() {
        let call = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };
        let byte = |e: IrExpr, i: i64| call("__byte_at", vec![e, lit_int(i)], Ty::Int);
        let len = |e: IrExpr| call("almide_rt_string_len", vec![e], Ty::Int);
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };
        let tb = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Bool, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };

        // to_upper("aB3") → "AB3" : byte0 'a'->'A'(65), byte1 'B' stays (66), byte2 '3' stays (51)
        let up = || call("almide_rt_string_to_upper", vec![lit_str("aB3")], Ty::String);
        ti(byte(up(), 0), "65", "to_upper[0]");
        ti(byte(up(), 1), "66", "to_upper[1]");
        ti(byte(up(), 2), "51", "to_upper[2]");
        // to_lower("aB") → byte1 'B'->'b'(98)
        ti(byte(call("almide_rt_string_to_lower", vec![lit_str("aB")], Ty::String), 1), "98", "to_lower[1]");
        // repeat("ab", 3) → "ababab" len 6, byte4 'a'(97)
        let rep = || call("almide_rt_string_repeat", vec![lit_str("ab"), lit_int(3)], Ty::String);
        ti(len(rep()), "6", "repeat len");
        ti(byte(rep(), 4), "97", "repeat[4]=='a'");
        // contains
        tb(call("almide_rt_string_contains", vec![lit_str("hello"), lit_str("ell")], Ty::Bool), "1", "contains ell");
        tb(call("almide_rt_string_contains", vec![lit_str("hello"), lit_str("xyz")], Ty::Bool), "0", "!contains xyz");
        tb(call("almide_rt_string_contains", vec![lit_str("hello"), lit_str("")], Ty::Bool), "1", "contains empty");
    }

    /// string.starts_with / ends_with (byte comparison).
    #[test]
    fn exec_intrinsic_starts_ends_with() {
        let call = |symbol: &str, s: &str, p: &str| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args: vec![lit_str(s), lit_str(p)] },
            ty: Ty::Bool, span: None, def_id: None };
        let t = |e: IrExpr, exp: &str, msg: &str| {
            let m = mk_func("main", Ty::Bool, e);
            if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); }
        };
        t(call("almide_rt_string_starts_with", "hello", "he"), "1", "hello starts he");
        t(call("almide_rt_string_starts_with", "hello", "lo"), "0", "hello !starts lo");
        t(call("almide_rt_string_starts_with", "hi", "hello"), "0", "prefix longer");
        t(call("almide_rt_string_ends_with", "hello", "lo"), "1", "hello ends lo");
        t(call("almide_rt_string_ends_with", "hello", "he"), "0", "hello !ends he");
    }

    /// list.sort (Int, ascending): `[3,1,2].sort()` → [1,2,3]; [0]==1, [2]==3.
    #[test]
    fn exec_intrinsic_sort() {
        let mk = |i: i64| {
            let list = IrExpr {
                kind: IrExprKind::List { elements: vec![lit_int(3), lit_int(1), lit_int(2)] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            let sorted = IrExpr {
                kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_sort"), args: vec![list] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            IrExpr { kind: IrExprKind::IndexAccess { object: Box::new(sorted), index: Box::new(lit_int(i)) }, ty: Ty::Int, span: None, def_id: None }
        };
        let m0 = mk_func("main", Ty::Int, mk(0));
        if let Some(r) = run(&[m0], "main") { assert_eq!(r, "1", "sort[0]"); }
        let m1 = mk_func("main", Ty::Int, mk(1));
        if let Some(r) = run(&[m1], "main") { assert_eq!(r, "2", "sort[1]"); }
        let m2 = mk_func("main", Ty::Int, mk(2));
        if let Some(r) = run(&[m2], "main") { assert_eq!(r, "3", "sort[2]"); }
    }

    /// list.sum and list.contains (Int elements).
    #[test]
    fn exec_intrinsic_sum_contains() {
        let list = || IrExpr {
            kind: IrExprKind::List { elements: vec![lit_int(10), lit_int(20), lit_int(30)] },
            ty: Ty::list(Ty::Int), span: None, def_id: None,
        };
        let sum = mk_func("main", Ty::Int, IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_sum"), args: vec![list()] },
            ty: Ty::Int, span: None, def_id: None });
        if let Some(r) = run(&[sum], "main") { assert_eq!(r, "60", "sum"); }
        let has = mk_func("main", Ty::Bool, IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_contains"), args: vec![list(), lit_int(20)] },
            ty: Ty::Bool, span: None, def_id: None });
        if let Some(r) = run(&[has], "main") { assert_eq!(r, "1", "contains 20"); }
        let hasnt = mk_func("main", Ty::Bool, IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_contains"), args: vec![list(), lit_int(99)] },
            ty: Ty::Bool, span: None, def_id: None });
        if let Some(r) = run(&[hasnt], "main") { assert_eq!(r, "0", "contains 99"); }
    }

    /// result.map: `Ok(5).map(x=>x*2) ?? -1` → 10; `Err(_).map(...) ?? -1` → -1.
    #[test]
    fn exec_intrinsic_result_map() {
        let res_int = Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, vec![Ty::Int, Ty::String]);
        let build = |is_ok: bool| {
            let mut vt = VarTable::new();
            let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
            let inner = if is_ok {
                IrExpr { kind: IrExprKind::ResultOk { expr: Box::new(lit_int(5)) }, ty: res_int.clone(), span: None, def_id: None }
            } else {
                IrExpr { kind: IrExprKind::ResultErr { expr: Box::new(lit_str("boom")) }, ty: res_int.clone(), span: None, def_id: None }
            };
            let lam = IrExpr {
                kind: IrExprKind::Lambda {
                    params: vec![(x, Ty::Int)],
                    body: Box::new(binop(almide_ir::BinOp::MulInt,
                        IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None },
                        lit_int(2), Ty::Int)),
                    lambda_id: None,
                },
                ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) }, span: None, def_id: None,
            };
            let mapped = IrExpr {
                kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_result_map"), args: vec![inner, lam] },
                ty: res_int.clone(), span: None, def_id: None,
            };
            // unwrap_or via the result intrinsic (Ok → payload, Err → fallback)
            let uo = IrExpr {
                kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_result_unwrap_or"), args: vec![mapped, lit_int(-1)] },
                ty: Ty::Int, span: None, def_id: None,
            };
            (vt, uo)
        };
        let (vt, ok) = build(true);
        let m = mk_func("main", Ty::Int, ok);
        if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(r, "10", "Ok(5).map(*2)"); }
        let (vt2, err) = build(false);
        let m2 = mk_func("main", Ty::Int, err);
        if let Some(r) = run_vt(&[m2], &vt2, "main") { assert_eq!(r, "-1", "Err.map"); }
    }

    /// Option/Result tag tests + unwrap_or via intrinsics.
    #[test]
    fn exec_intrinsic_option_result() {
        let opt_int = Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, vec![Ty::Int]);
        let res_int = Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, vec![Ty::Int, Ty::String]);
        let some = || IrExpr { kind: IrExprKind::OptionSome { expr: Box::new(lit_int(7)) }, ty: opt_int.clone(), span: None, def_id: None };
        let none = || IrExpr { kind: IrExprKind::OptionNone, ty: opt_int.clone(), span: None, def_id: None };
        let ok = || IrExpr { kind: IrExprKind::ResultOk { expr: Box::new(lit_int(9)) }, ty: res_int.clone(), span: None, def_id: None };

        let call1 = |sym_: &str, a: IrExpr, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(sym_), args: vec![a] }, ty, span: None, def_id: None };

        // is_some(Some)=1, is_none(Some)=0, is_some(None)=0
        let t = |e: IrExpr, exp: &str| { let m = mk_func("main", Ty::Bool, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp); } };
        t(call1("almide_rt_option_is_some", some(), Ty::Bool), "1");
        t(call1("almide_rt_option_is_none", some(), Ty::Bool), "0");
        t(call1("almide_rt_option_is_some", none(), Ty::Bool), "0");
        t(call1("almide_rt_result_is_ok", ok(), Ty::Bool), "1");
        t(call1("almide_rt_result_is_err", ok(), Ty::Bool), "0");

        // unwrap_or: Some(7) ?? 99 = 7 ; None ?? 99 = 99 ; Ok(9) ?? 99 = 9
        let uo = |sym_: &str, a: IrExpr| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(sym_), args: vec![a, lit_int(99)] },
            ty: Ty::Int, span: None, def_id: None };
        let ti = |e: IrExpr, exp: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp); } };
        ti(uo("almide_rt_option_unwrap_or", some()), "7");
        ti(uo("almide_rt_option_unwrap_or", none()), "99");
        ti(uo("almide_rt_result_unwrap_or", ok()), "9");
    }

    /// option.map: `Some(5).map(x => x*2) ?? -1` → 10; `None.map(...) ?? -1` → -1.
    #[test]
    fn exec_intrinsic_option_map() {
        let opt_int = Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, vec![Ty::Int]);
        let build = |is_some: bool| {
            let mut vt = VarTable::new();
            let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
            let inner = if is_some {
                IrExpr { kind: IrExprKind::OptionSome { expr: Box::new(lit_int(5)) }, ty: opt_int.clone(), span: None, def_id: None }
            } else {
                IrExpr { kind: IrExprKind::OptionNone, ty: opt_int.clone(), span: None, def_id: None }
            };
            let lam = IrExpr {
                kind: IrExprKind::Lambda {
                    params: vec![(x, Ty::Int)],
                    body: Box::new(binop(almide_ir::BinOp::MulInt,
                        IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None },
                        lit_int(2), Ty::Int)),
                    lambda_id: None,
                },
                ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) }, span: None, def_id: None,
            };
            let mapped = IrExpr {
                kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_option_map"), args: vec![inner, lam] },
                ty: opt_int.clone(), span: None, def_id: None,
            };
            let uo = IrExpr {
                kind: IrExprKind::UnwrapOr { expr: Box::new(mapped), fallback: Box::new(lit_int(-1)) },
                ty: Ty::Int, span: None, def_id: None,
            };
            (vt, uo)
        };
        let (vt, s) = build(true);
        let m = mk_func("main", Ty::Int, s);
        if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(r, "10", "Some(5).map(*2)"); }
        let (vt2, n) = build(false);
        let m2 = mk_func("main", Ty::Int, n);
        if let Some(r) = run_vt(&[m2], &vt2, "main") { assert_eq!(r, "-1", "None.map"); }
    }

    /// string.len counts UTF-8 code points, not bytes:
    /// "café" is 5 bytes but 4 chars; "abc" is 3.
    #[test]
    fn exec_intrinsic_string_len_unicode() {
        let cafe = mk_func("main", Ty::Int, rt_call("almide_rt_string_len", vec![lit_str("café")]));
        if let Some(r) = run(&[cafe], "main") { assert_eq!(r, "4", "len(café) chars"); }
        let abc = mk_func("main", Ty::Int, rt_call("almide_rt_string_len", vec![lit_str("abc")]));
        if let Some(r) = run(&[abc], "main") { assert_eq!(r, "3", "len(abc)"); }
    }

    /// list.get_or with in-bounds and out-of-bounds indices:
    /// [10,20,30].get_or(1, 99)==20 ; .get_or(5, 99)==99.
    #[test]
    fn exec_intrinsic_list_get_or() {
        let mk = |idx: i64| {
            let list = IrExpr {
                kind: IrExprKind::List { elements: vec![lit_int(10), lit_int(20), lit_int(30)] },
                ty: Ty::list(Ty::Int), span: None, def_id: None,
            };
            rt_call("almide_rt_list_get_or", vec![list, lit_int(idx), lit_int(99)])
        };
        let in_b = mk_func("main", Ty::Int, mk(1));
        if let Some(r) = run(&[in_b], "main") { assert_eq!(r, "20", "get_or(1)"); }
        let out_b = mk_func("main", Ty::Int, mk(5));
        if let Some(r) = run(&[out_b], "main") { assert_eq!(r, "99", "get_or(5)"); }
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

    /// Closure call through the function table: a lifted lambda
    /// `__lam(env, x) = x + 1` is created as a closure and invoked indirectly
    /// with argument 5 → 6. Exercises ClosureCreate, the closure calling
    /// convention, and call_indirect.
    #[test]
    fn exec_closure_indirect_call() {
        use almide_ir::{IrParam, ParamBorrow, CallTarget};
        let mut vt = VarTable::new();
        let env = vt.alloc(sym("env"), Ty::Unknown, almide_ir::Mutability::Let, None);
        let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);

        // __lam(env, x) -> Int = x + 1
        let lam_body = binop(
            almide_ir::BinOp::AddInt,
            IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None },
            lit_int(1),
            Ty::Int,
        );
        let mut lam = mk_func("__lam", Ty::Int, lam_body);
        lam.params = vec![
            IrParam { var: env, ty: Ty::Unknown, name: sym("env"), borrow: ParamBorrow::Own,
                      open_record: None, default: None, attrs: vec![] },
            IrParam { var: x, ty: Ty::Int, name: sym("x"), borrow: ParamBorrow::Own,
                      open_record: None, default: None, attrs: vec![] },
        ];

        // closure = ClosureCreate(__lam, []) ; main = closure(5)
        let closure = IrExpr {
            kind: IrExprKind::ClosureCreate { func_name: sym("__lam"), captures: vec![] },
            ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) },
            span: None, def_id: None,
        };
        let call = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(closure) },
                args: vec![lit_int(5)],
                type_args: vec![],
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, call);
        if let Some(r) = run_vt(&[lam, main], &vt, "main") {
            assert_eq!(r, "6", "closure(5) where lam(x)=x+1");
        }
    }

    /// Iterate a list summing its elements:
    /// `sum = 0; for x in [10,20,30] { sum = sum + x }; sum` → 60.
    /// Requires the loop to use an 8-byte stride / i64 load for Int elements.
    #[test]
    fn exec_for_in_list() {
        use almide_ir::{IrStmt, IrStmtKind, Mutability};
        let mut vt = VarTable::new();
        let sum = vt.alloc(sym("sum"), Ty::Int, Mutability::Var, None);
        let x = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
        let var = |id| IrExpr { kind: IrExprKind::Var { id }, ty: Ty::Int, span: None, def_id: None };

        let list = IrExpr {
            kind: IrExprKind::List { elements: vec![lit_int(10), lit_int(20), lit_int(30)] },
            ty: Ty::list(Ty::Int), span: None, def_id: None,
        };
        let for_body = vec![IrStmt {
            kind: IrStmtKind::Assign { var: sum, value: binop(almide_ir::BinOp::AddInt, var(sum), var(x), Ty::Int) },
            span: None,
        }];
        let for_expr = IrExpr {
            kind: IrExprKind::ForIn { var: x, var_tuple: None, iterable: Box::new(list), body: for_body },
            ty: Ty::Unit, span: None, def_id: None,
        };
        let block = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![
                    IrStmt { kind: IrStmtKind::Bind { var: sum, ty: Ty::Int, mutability: Mutability::Var, value: lit_int(0) }, span: None },
                    IrStmt { kind: IrStmtKind::Expr { expr: for_expr }, span: None },
                ],
                expr: Some(Box::new(var(sum))),
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, block);
        if let Some(r) = run_vt(&[main], &vt, "main") {
            assert_eq!(r, "60", "sum of [10,20,30]");
        }
    }

    /// Match with payload binding: `match Some(42) { Some(x) => x, None => 0 }`
    /// → 42, and the None subject → 0.
    #[test]
    fn exec_match_some_bind() {
        use almide_ir::{IrMatchArm, IrPattern};
        let opt_ty = Ty::Applied(
            almide_lang::types::constructor::TypeConstructorId::Option, vec![Ty::Int]);
        let build = |subject: IrExpr, vt: &VarTable, x: almide_ir::VarId| {
            let arms = vec![
                IrMatchArm {
                    pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: x, ty: Ty::Int }) },
                    guard: None,
                    body: IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None },
                },
                IrMatchArm { pattern: IrPattern::None, guard: None, body: lit_int(0) },
            ];
            IrExpr {
                kind: IrExprKind::Match { subject: Box::new(subject), arms },
                ty: Ty::Int, span: None, def_id: None,
            }
        };
        // Some(42) → 42
        let mut vt = VarTable::new();
        let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
        let some = IrExpr {
            kind: IrExprKind::OptionSome { expr: Box::new(lit_int(42)) },
            ty: opt_ty.clone(), span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, build(some, &vt, x));
        if let Some(r) = run_vt(&[main], &vt, "main") { assert_eq!(r, "42", "Some(42)"); }

        // None → 0
        let none = IrExpr { kind: IrExprKind::OptionNone, ty: opt_ty, span: None, def_id: None };
        let main2 = mk_func("main", Ty::Int, build(none, &vt, x));
        if let Some(r) = run_vt(&[main2], &vt, "main") { assert_eq!(r, "0", "None"); }
    }

    /// map.entries / from_entries round-trip (Map[Int,Int]).
    #[test]
    fn exec_map_entries() {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let map_ty = Ty::Applied(TC::Map, vec![Ty::Int, Ty::Int]);
        let pair_ty = Ty::Tuple(vec![Ty::Int, Ty::Int]);
        let call = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };
        let pair = |k: i64, v: i64| IrExpr { kind: IrExprKind::Tuple { elements: vec![lit_int(k), lit_int(v)] },
            ty: pair_ty.clone(), span: None, def_id: None };
        let entry_list = |pairs: &[(i64, i64)]| IrExpr {
            kind: IrExprKind::List { elements: pairs.iter().map(|(k, v)| pair(*k, *v)).collect() },
            ty: Ty::list(pair_ty.clone()), span: None, def_id: None };
        let from_entries = |pairs: &[(i64, i64)]| call("almide_rt_map_from_entries", vec![entry_list(pairs)], map_ty.clone());
        let get_or = |m: IrExpr, k: i64| call("almide_rt_map_get_or", vec![m, lit_int(k), lit_int(-1)], Ty::Int);
        let len = |m: IrExpr| call("almide_rt_map_len", vec![m], Ty::Int);
        let entries = |m: IrExpr| call("almide_rt_map_entries", vec![m], Ty::list(pair_ty.clone()));
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };

        // from_entries builds the map
        ti(len(from_entries(&[(1, 10), (2, 20), (3, 30)])), "3", "from_entries len");
        ti(get_or(from_entries(&[(1, 10), (2, 20)]), 2), "20", "from_entries get 2");
        ti(get_or(from_entries(&[(1, 10), (2, 20)]), 9), "-1", "from_entries miss");
        // entries length == map size
        ti(len(from_entries(&[(5, 50), (6, 60)])), "2", "sanity");
        ti(call("almide_rt_list_len", vec![entries(from_entries(&[(1, 10), (2, 20), (3, 30)]))], Ty::Int), "3", "entries len");
        // round-trip: from_entries(entries(m)) preserves bindings
        let rt = from_entries(&[(7, 70), (8, 80)]);
        let round = call("almide_rt_map_from_entries", vec![entries(rt)], map_ty.clone());
        ti(get_or(round, 8), "80", "entries→from_entries round-trip");
    }

    /// Gap-closing intrinsics: float.floor/ceil, int.parse, option.to_list,
    /// option.and_then, string.lines.
    #[test]
    fn exec_gap_batch() {
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let call = |symbol: &str, args: Vec<IrExpr>, ty: Ty| IrExpr {
            kind: IrExprKind::RuntimeCall { symbol: sym(symbol), args }, ty, span: None, def_id: None };
        let lit_float = |v: f64| IrExpr { kind: IrExprKind::LitFloat { value: v }, ty: Ty::Float, span: None, def_id: None };
        let fstr = |e: IrExpr| call("almide_rt_float_to_string", vec![e], Ty::String);
        let byte = |e: IrExpr, i: i64| call("__byte_at", vec![e, lit_int(i)], Ty::Int);
        let ti = |e: IrExpr, exp: &str, msg: &str| { let m = mk_func("main", Ty::Int, e); if let Some(r) = run(&[m], "main") { assert_eq!(&r, exp, "{}", msg); } };

        // float.floor(2.7) → "2.0" ; float.ceil(2.3) → "3.0"
        ti(byte(fstr(call("almide_rt_float_floor", vec![lit_float(2.7)], Ty::Float)), 0), "50", "floor 2.7 → '2'");
        ti(byte(fstr(call("almide_rt_float_ceil", vec![lit_float(2.3)], Ty::Float)), 0), "51", "ceil 2.3 → '3'");
        ti(byte(fstr(call("almide_rt_float_floor", vec![lit_float(-1.5)], Ty::Float)), 0), "45", "floor -1.5 → '-'");

        // int.parse → Result[Int,String]; unwrap_or via ?? -1
        let res_ty = Ty::Applied(TC::Result, vec![Ty::Int, Ty::String]);
        let parse = |s: &str| {
            let p = call("almide_rt_int_parse", vec![lit_str(s)], res_ty.clone());
            IrExpr { kind: IrExprKind::UnwrapOr { expr: Box::new(p), fallback: Box::new(lit_int(-1)) },
                     ty: Ty::Int, span: None, def_id: None }
        };
        ti(parse("42"), "42", "parse 42");
        ti(parse("-7"), "-7", "parse -7");
        ti(parse("abc"), "-1", "parse abc → Err");
        ti(parse(""), "-1", "parse empty → Err");
        ti(parse("1x"), "-1", "parse trailing junk → Err");

        // option.to_list: Some(5) → [5] (len 1), None → [] (len 0)
        let opt_int = Ty::Applied(TC::Option, vec![Ty::Int]);
        let some5 = IrExpr { kind: IrExprKind::OptionSome { expr: Box::new(lit_int(5)) }, ty: opt_int.clone(), span: None, def_id: None };
        let none = || IrExpr { kind: IrExprKind::OptionNone, ty: opt_int.clone(), span: None, def_id: None };
        let to_list = |o: IrExpr| call("almide_rt_option_to_list", vec![o], Ty::list(Ty::Int));
        ti(call("almide_rt_list_len", vec![to_list(some5.clone())], Ty::Int), "1", "to_list Some len");
        ti(call("almide_rt_list_get_or", vec![to_list(some5), lit_int(0), lit_int(-1)], Ty::Int), "5", "to_list Some [0]");
        ti(call("almide_rt_list_len", vec![to_list(none())], Ty::Int), "0", "to_list None len");

        // option.and_then(Some(3), x => Some(x*10)) → Some(30) unwrap → 30
        {
            let mut vt = VarTable::new();
            let x = vt.alloc(sym("x"), Ty::Int, almide_ir::Mutability::Let, None);
            let var_x = || IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None };
            let lam = IrExpr { kind: IrExprKind::Lambda { params: vec![(x, Ty::Int)],
                body: Box::new(IrExpr { kind: IrExprKind::OptionSome {
                    expr: Box::new(binop(almide_ir::BinOp::MulInt, var_x(), lit_int(10), Ty::Int)) },
                    ty: opt_int.clone(), span: None, def_id: None }), lambda_id: None },
                ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(opt_int.clone()) }, span: None, def_id: None };
            let some3 = IrExpr { kind: IrExprKind::OptionSome { expr: Box::new(lit_int(3)) }, ty: opt_int.clone(), span: None, def_id: None };
            let chained = call("almide_rt_option_and_then", vec![some3, lam], opt_int.clone());
            let unwrapped = IrExpr { kind: IrExprKind::UnwrapOr { expr: Box::new(chained), fallback: Box::new(lit_int(-1)) },
                ty: Ty::Int, span: None, def_id: None };
            let m = mk_func("main", Ty::Int, unwrapped);
            if let Some(r) = run_vt(&[m], &vt, "main") { assert_eq!(r, "30", "and_then Some(3)*10"); }
        }

        // string.lines: "a\nb\nc" → 3 ; "a\n" → 1 ; "" → 0 ; "a\n\nb" → 3
        let lines_len = |s: &str| call("almide_rt_list_len", vec![call("almide_rt_string_lines", vec![lit_str(s)], Ty::list(Ty::String))], Ty::Int);
        ti(lines_len("a\nb\nc"), "3", "lines abc");
        ti(lines_len("a\n"), "1", "lines trailing nl");
        ti(lines_len(""), "0", "lines empty");
        ti(lines_len("a\n\nb"), "3", "lines blank middle");
    }

    /// Match destructuring: Tuple / Record / List patterns + nesting in Some.
    #[test]
    fn exec_match_destructure() {
        use almide_ir::{IrMatchArm, IrPattern, IrFieldPattern};
        use almide_lang::types::constructor::TypeConstructorId as TC;
        let bind = |id, ty: Ty| IrPattern::Bind { var: id, ty };
        let var = |id, ty: Ty| IrExpr { kind: IrExprKind::Var { id }, ty, span: None, def_id: None };
        let mk_match = |subject: IrExpr, arms: Vec<IrMatchArm>| IrExpr {
            kind: IrExprKind::Match { subject: Box::new(subject), arms }, ty: Ty::Int, span: None, def_id: None };
        let arm = |pattern, body| IrMatchArm { pattern, guard: None, body };
        let wild = || IrMatchArm { pattern: IrPattern::Wildcard, guard: None, body: lit_int(0) };

        // (a, b) => a + b   over (10, 20) → 30
        {
            let mut vt = VarTable::new();
            let a = vt.alloc(sym("a"), Ty::Int, almide_ir::Mutability::Let, None);
            let b = vt.alloc(sym("b"), Ty::Int, almide_ir::Mutability::Let, None);
            let tup = IrExpr { kind: IrExprKind::Tuple { elements: vec![lit_int(10), lit_int(20)] },
                ty: Ty::Tuple(vec![Ty::Int, Ty::Int]), span: None, def_id: None };
            let body = binop(almide_ir::BinOp::AddInt, var(a, Ty::Int), var(b, Ty::Int), Ty::Int);
            let m = mk_match(tup, vec![arm(IrPattern::Tuple { elements: vec![bind(a, Ty::Int), bind(b, Ty::Int)] }, body)]);
            let f = mk_func("main", Ty::Int, m);
            if let Some(r) = run_vt(&[f], &vt, "main") { assert_eq!(r, "30", "tuple (a,b)=>a+b"); }
        }

        // (1, b) => b ; _ => 0    refutable literal head
        {
            let build = |t0: i64| {
                let mut vt = VarTable::new();
                let b = vt.alloc(sym("b"), Ty::Int, almide_ir::Mutability::Let, None);
                let tup = IrExpr { kind: IrExprKind::Tuple { elements: vec![lit_int(t0), lit_int(5)] },
                    ty: Ty::Tuple(vec![Ty::Int, Ty::Int]), span: None, def_id: None };
                let pat = IrPattern::Tuple { elements: vec![IrPattern::Literal { expr: lit_int(1) }, bind(b, Ty::Int)] };
                (vt, mk_match(tup, vec![arm(pat, var(b, Ty::Int)), wild()]))
            };
            let (vt, m) = build(1);
            let f = mk_func("main", Ty::Int, m);
            if let Some(r) = run_vt(&[f], &vt, "main") { assert_eq!(r, "5", "(1,b)=>b matches"); }
            let (vt, m) = build(2);
            let f = mk_func("main", Ty::Int, m);
            if let Some(r) = run_vt(&[f], &vt, "main") { assert_eq!(r, "0", "(1,b) no match → wildcard"); }
        }

        // {a, b} => a*10 + b    over {a:1, b:2} → 12
        {
            let mut vt = VarTable::new();
            let a = vt.alloc(sym("a"), Ty::Int, almide_ir::Mutability::Let, None);
            let b = vt.alloc(sym("b"), Ty::Int, almide_ir::Mutability::Let, None);
            let rec_ty = Ty::Record { fields: vec![(sym("a"), Ty::Int), (sym("b"), Ty::Int)] };
            let rec = IrExpr { kind: IrExprKind::Record { name: None, fields: vec![(sym("a"), lit_int(1)), (sym("b"), lit_int(2))] },
                ty: rec_ty.clone(), span: None, def_id: None };
            let pat = IrPattern::RecordPattern { name: String::new(), rest: false, fields: vec![
                IrFieldPattern { name: "a".into(), pattern: Some(bind(a, Ty::Int)) },
                IrFieldPattern { name: "b".into(), pattern: Some(bind(b, Ty::Int)) },
            ] };
            let body = binop(almide_ir::BinOp::AddInt,
                binop(almide_ir::BinOp::MulInt, var(a, Ty::Int), lit_int(10), Ty::Int), var(b, Ty::Int), Ty::Int);
            let m = mk_match(rec, vec![arm(pat, body)]);
            let f = mk_func("main", Ty::Int, m);
            if let Some(r) = run_vt(&[f], &vt, "main") { assert_eq!(r, "12", "record a,b => a*10+b"); }
        }

        // [a, b, c] => a + b + c   length-exact; mismatch falls through
        {
            let build = |elems: Vec<i64>| {
                let mut vt = VarTable::new();
                let a = vt.alloc(sym("a"), Ty::Int, almide_ir::Mutability::Let, None);
                let b = vt.alloc(sym("b"), Ty::Int, almide_ir::Mutability::Let, None);
                let c = vt.alloc(sym("c"), Ty::Int, almide_ir::Mutability::Let, None);
                let list = IrExpr { kind: IrExprKind::List { elements: elems.into_iter().map(lit_int).collect() },
                    ty: Ty::list(Ty::Int), span: None, def_id: None };
                let pat = IrPattern::List { elements: vec![bind(a, Ty::Int), bind(b, Ty::Int), bind(c, Ty::Int)] };
                let body = binop(almide_ir::BinOp::AddInt,
                    binop(almide_ir::BinOp::AddInt, var(a, Ty::Int), var(b, Ty::Int), Ty::Int), var(c, Ty::Int), Ty::Int);
                (vt, mk_match(list, vec![arm(pat, body), wild()]))
            };
            let (vt, m) = build(vec![7, 8, 9]);
            let f = mk_func("main", Ty::Int, m);
            if let Some(r) = run_vt(&[f], &vt, "main") { assert_eq!(r, "24", "[a,b,c] sum"); }
            let (vt, m) = build(vec![7, 8]);
            let f = mk_func("main", Ty::Int, m);
            if let Some(r) = run_vt(&[f], &vt, "main") { assert_eq!(r, "0", "wrong length → wildcard"); }
        }

        // Some((a, b)) => a + b ; None => 0   nested tuple in Some
        {
            let opt_ty = Ty::Applied(TC::Option, vec![Ty::Tuple(vec![Ty::Int, Ty::Int])]);
            let build = |subject: IrExpr, vt: &VarTable, a, b| {
                let pat = IrPattern::Some { inner: Box::new(IrPattern::Tuple { elements: vec![bind(a, Ty::Int), bind(b, Ty::Int)] }) };
                let body = binop(almide_ir::BinOp::AddInt, var(a, Ty::Int), var(b, Ty::Int), Ty::Int);
                let _ = vt;
                mk_match(subject, vec![arm(pat, body), IrMatchArm { pattern: IrPattern::None, guard: None, body: lit_int(0) }])
            };
            let mut vt = VarTable::new();
            let a = vt.alloc(sym("a"), Ty::Int, almide_ir::Mutability::Let, None);
            let b = vt.alloc(sym("b"), Ty::Int, almide_ir::Mutability::Let, None);
            let inner = IrExpr { kind: IrExprKind::Tuple { elements: vec![lit_int(3), lit_int(4)] },
                ty: Ty::Tuple(vec![Ty::Int, Ty::Int]), span: None, def_id: None };
            let some = IrExpr { kind: IrExprKind::OptionSome { expr: Box::new(inner) }, ty: opt_ty.clone(), span: None, def_id: None };
            let f = mk_func("main", Ty::Int, build(some, &vt, a, b));
            if let Some(r) = run_vt(&[f], &vt, "main") { assert_eq!(r, "7", "Some((a,b))=>a+b"); }
            let none = IrExpr { kind: IrExprKind::OptionNone, ty: opt_ty, span: None, def_id: None };
            let f = mk_func("main", Ty::Int, build(none, &vt, a, b));
            if let Some(r) = run_vt(&[f], &vt, "main") { assert_eq!(r, "0", "None → 0"); }
        }

        // mixed widths: (n: Int, s: String) => n + string.len(s)  over (5, "hi") → 7
        {
            let mut vt = VarTable::new();
            let n = vt.alloc(sym("n"), Ty::Int, almide_ir::Mutability::Let, None);
            let s = vt.alloc(sym("s"), Ty::String, almide_ir::Mutability::Let, None);
            let tup = IrExpr { kind: IrExprKind::Tuple { elements: vec![lit_int(5), lit_str("hi")] },
                ty: Ty::Tuple(vec![Ty::Int, Ty::String]), span: None, def_id: None };
            let slen = IrExpr { kind: IrExprKind::RuntimeCall { symbol: sym("almide_rt_string_len"),
                args: vec![var(s, Ty::String)] }, ty: Ty::Int, span: None, def_id: None };
            let body = binop(almide_ir::BinOp::AddInt, var(n, Ty::Int), slen, Ty::Int);
            let pat = IrPattern::Tuple { elements: vec![bind(n, Ty::Int), bind(s, Ty::String)] };
            let f = mk_func("main", Ty::Int, mk_match(tup, vec![arm(pat, body)]));
            if let Some(r) = run_vt(&[f], &vt, "main") { assert_eq!(r, "7", "(n,s)=>n+len(s) mixed widths"); }
        }
    }

    /// List concatenation: `([1,2] + [3])[2]` → 3.
    #[test]
    fn exec_list_concat() {
        let mk_list = |vals: Vec<i64>| IrExpr {
            kind: IrExprKind::List { elements: vals.into_iter().map(lit_int).collect() },
            ty: Ty::list(Ty::Int), span: None, def_id: None,
        };
        let concat = binop(almide_ir::BinOp::ConcatList, mk_list(vec![1, 2]), mk_list(vec![3]), Ty::list(Ty::Int));
        let index = IrExpr {
            kind: IrExprKind::IndexAccess { object: Box::new(concat), index: Box::new(lit_int(2)) },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, index);
        if let Some(r) = run(&[main], "main") {
            assert_eq!(r, "3", "([1,2]+[3])[2]");
        }
    }

    /// Range in a for-loop: `sum = 0; for x in 0..5 { sum = sum + x }; sum`
    /// → 0+1+2+3+4 = 10. Exercises __range + list iteration.
    #[test]
    fn exec_range_for_loop() {
        use almide_ir::{IrStmt, IrStmtKind, Mutability};
        let mut vt = VarTable::new();
        let sum = vt.alloc(sym("sum"), Ty::Int, Mutability::Var, None);
        let x = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
        let var = |id| IrExpr { kind: IrExprKind::Var { id }, ty: Ty::Int, span: None, def_id: None };

        let range = IrExpr {
            kind: IrExprKind::Range {
                start: Box::new(lit_int(0)), end: Box::new(lit_int(5)), inclusive: false,
            },
            ty: Ty::list(Ty::Int), span: None, def_id: None,
        };
        let for_body = vec![IrStmt {
            kind: IrStmtKind::Assign { var: sum, value: binop(almide_ir::BinOp::AddInt, var(sum), var(x), Ty::Int) },
            span: None,
        }];
        let for_expr = IrExpr {
            kind: IrExprKind::ForIn { var: x, var_tuple: None, iterable: Box::new(range), body: for_body },
            ty: Ty::Unit, span: None, def_id: None,
        };
        let block = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![
                    IrStmt { kind: IrStmtKind::Bind { var: sum, ty: Ty::Int, mutability: Mutability::Var, value: lit_int(0) }, span: None },
                    IrStmt { kind: IrStmtKind::Expr { expr: for_expr }, span: None },
                ],
                expr: Some(Box::new(var(sum))),
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, block);
        if let Some(r) = run_vt(&[main], &vt, "main") {
            assert_eq!(r, "10", "sum of 0..5");
        }
    }

    /// Closure with a captured variable: `let n = 10; (x) => x + n` invoked
    /// with 5 → 15. Exercises capture storage (ClosureCreate) and EnvLoad.
    #[test]
    fn exec_closure_capture() {
        use almide_ir::{IrParam, ParamBorrow, CallTarget, IrStmt, IrStmtKind, Mutability};
        let mut vt = VarTable::new();
        let env = vt.alloc(sym("env"), Ty::Unknown, Mutability::Let, None);
        let x = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
        let n = vt.alloc(sym("n"), Ty::Int, Mutability::Let, None);

        // __lam(env, x) -> Int = x + EnvLoad(env, 0)
        let env_load = IrExpr {
            kind: IrExprKind::EnvLoad { env_var: env, index: 0 },
            ty: Ty::Int, span: None, def_id: None,
        };
        let lam_body = binop(
            almide_ir::BinOp::AddInt,
            IrExpr { kind: IrExprKind::Var { id: x }, ty: Ty::Int, span: None, def_id: None },
            env_load, Ty::Int,
        );
        let mut lam = mk_func("__lam", Ty::Int, lam_body);
        lam.params = vec![
            IrParam { var: env, ty: Ty::Unknown, name: sym("env"), borrow: ParamBorrow::Own,
                      open_record: None, default: None, attrs: vec![] },
            IrParam { var: x, ty: Ty::Int, name: sym("x"), borrow: ParamBorrow::Own,
                      open_record: None, default: None, attrs: vec![] },
        ];

        // main = { let n = 10; closure_of(__lam capturing n)(5) }
        let closure = IrExpr {
            kind: IrExprKind::ClosureCreate { func_name: sym("__lam"), captures: vec![(n, Ty::Int)] },
            ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) },
            span: None, def_id: None,
        };
        let call = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(closure) },
                args: vec![lit_int(5)], type_args: vec![],
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main_body = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![IrStmt {
                    kind: IrStmtKind::Bind { var: n, ty: Ty::Int, mutability: Mutability::Let, value: lit_int(10) },
                    span: None,
                }],
                expr: Some(Box::new(call)),
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let main = mk_func("main", Ty::Int, main_body);
        if let Some(r) = run_vt(&[lam, main], &vt, "main") {
            assert_eq!(r, "15", "closure capturing n=10 called with 5");
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
