/// Post-emit RC balance verification.
///
/// Counts call(rc_dec) in each compiled function's call_targets and compares
/// with the IR-level RcDec count. Extra rc_dec calls (beyond typed child
/// drops) indicate a compiler bug.
fn verify_rc_balance(program: &IrProgram, emitter: &WasmEmitter) {
    use almide_ir::{IrStmtKind};
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};

    let rc_dec_fn = emitter.rt.rc_dec;

    // Count IR-level RcDec statements per function
    struct RcDecCounter { count: usize }
    impl IrVisitor for RcDecCounter {
        fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
            if matches!(&stmt.kind, IrStmtKind::RcDec { .. }) {
                self.count += 1;
            }
            walk_stmt(self, stmt);
        }
        fn visit_expr(&mut self, expr: &almide_ir::IrExpr) {
            walk_expr(self, expr);
        }
    }

    for (i, func) in program.functions.iter().enumerate() {
        // Count IR RcDec
        let mut counter = RcDecCounter { count: 0 };
        counter.visit_expr(&func.body);
        let ir_dec_count = counter.count;

        // Find compiled function's call targets
        // User functions start after runtime functions in compiled[]
        // We match by name via func_map
        let func_name = func.name.to_string();
        if let Some(&func_idx) = emitter.func_map.get(&func_name) {
            // Count call(rc_dec) in call_targets
            let compiled_idx = func_idx as usize - emitter.num_imports as usize;
            if compiled_idx < emitter.compiled.len() {
                let wasm_dec_count = emitter.compiled[compiled_idx]
                    .call_targets.iter()
                    .filter(|&&t| t == rc_dec_fn)
                    .count();

                // The WASM may have MORE rc_dec calls than the IR because
                // emit_typed_rc_dec generates child drops. But it should
                // never have FEWER (that would be a leak, caught by
                // PerceusVerifyPass). We log mismatches for debugging.
                if wasm_dec_count < ir_dec_count {
                    // The IR (Verified by PerceusVerifyPass) specifies the
                    // balance; an emission that DROPS a Dec is a leak the
                    // belt already certified against. No warn-mode (§10).
                    eprintln!("error: [COMPILER BUG] WASM emission dropped RC decrements");
                    eprintln!(
                        "  `{}` has {} IR RcDec statement(s) but only {} emitted rc_dec call(s).",
                        func_name, ir_dec_count, wasm_dec_count,
                    );
                    eprintln!("  Please report this at https://github.com/almide/almide/issues");
                    std::process::exit(1);
                }
            }
        }
    }
}

/// Assemble all sections into a final WASM binary.
fn assemble(emitter: &mut WasmEmitter) -> Vec<u8> {
    let mut module = Module::new();

    // ── Type section ──
    let mut types = TypeSection::new();
    for (params, results) in &emitter.types {
        types.ty().function(params.iter().copied(), results.iter().copied());
    }
    module.section(&types);

    // ── Import section ──
    let mut imports = ImportSection::new();
    for info in &emitter.imports {
        imports.import(
            &info.module,
            &info.name,
            wasm_encoder::EntityType::Function(info.type_idx),
        );
    }
    module.section(&imports);

    // ── Function section (type indices for defined functions) ──
    let mut functions = FunctionSection::new();
    for cf in &emitter.compiled {
        functions.function(cf.type_idx);
    }
    module.section(&functions);

    // ── Table section (for call_indirect / FnRef) ──
    if !emitter.func_table.is_empty() {
        let mut tables = TableSection::new();
        tables.table(TableType {
            element_type: RefType::FUNCREF,
            minimum: emitter.func_table.len() as u64,
            maximum: Some(emitter.func_table.len() as u64),
            table64: false,
            shared: false,
        });
        module.section(&tables);
    }

    // ── Memory section ──
    // Single memory layout (iOS-Safari compatible):
    //   [data segment][heap ...]
    // The heap grows upward via `__alloc`. There is no reserved scratch
    // region — string interpolation builds results inline directly on the
    // heap (see `calls_string::emit_string_interp`).
    // Data layout: [data bytes][8-byte alignment][heap...]. The active data
    // segment (newline + embedded case tables + interned string literals) is
    // written into linear memory at instantiation, so the INITIAL memory must
    // already cover it — derive the page count from the heap start (>= data_end)
    // rather than a fixed 2 pages. The ~51KB case tables roughly halve the
    // literal headroom of the old fixed 128KB minimum, so a large-literal
    // case-folding program could otherwise overrun it and fail to instantiate.
    let data_end = NEWLINE_OFFSET + emitter.data_bytes.len() as u32;
    let heap_start_aligned = (data_end + 7) & !7;
    const WASM_PAGE_BYTES: u32 = 65536;
    let min_pages = heap_start_aligned.div_ceil(WASM_PAGE_BYTES).max(2);
    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: min_pages as u64,  // covers the full data region; allocator grows from here
        maximum: Some(65536),   // 4GB max (WASM32 hard limit) — explicit so V8 doesn't apply a smaller default
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memory);

    // ── Global section ──
    let mut globals = GlobalSection::new();
    // Global 0: heap pointer (memory 0)
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(heap_start_aligned as i32),
    );
    // Global 1: free list head (Perceus reuse, 0 = empty)
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(0),
    );
    // Global 2: preopen table pointer
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(0),
    );
    // Global 3: preopen count (set by __init_preopen_dirs at startup)
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(0),
    );
    // Global 4: heap_start (immutable) — pointers below this are data section, not heap
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: false,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(heap_start_aligned as i32),
    );
    emitter.rt.heap_start_global = runtime::HEAP_START_GLOBAL_IDX;

    // Top-level let globals. POSITIONAL invariant (#526): the section
    // ignores each entry's recorded index and emits by Vec order after the
    // five fixed globals — assert the two agree, or one alloc site that
    // bumped next_global without pushing here silently shifts EVERY later
    // global (the preopen/free-list collision class).
    for (i, &(recorded_idx, _, _)) in emitter.top_let_init.iter().enumerate() {
        let landing = runtime::HEAP_START_GLOBAL_IDX + 1 + i as u32;
        assert_eq!(
            recorded_idx, landing,
            "[ICE] top-let global recorded at index {} emitting at {} (#526)",
            recorded_idx, landing
        );
    }
    for &(_, vt, bits) in &emitter.top_let_init {
        let init = match vt {
            ValType::I64 => wasm_encoder::ConstExpr::i64_const(bits),
            ValType::F64 => wasm_encoder::ConstExpr::f64_const(f64::from_bits(bits as u64).into()),
            ValType::I32 => wasm_encoder::ConstExpr::i32_const(bits as i32),
            _ => wasm_encoder::ConstExpr::i32_const(0),
        };
        globals.global(
            GlobalType { val_type: vt, mutable: true, shared: false },
            &init,
        );
    }
    module.section(&globals);

    // ── Export section ──
    let mut exports = ExportSection::new();
    exports.export("memory", wasm_encoder::ExportKind::Memory, 0);
    if let Some(&runner_idx) = emitter.func_map.get("__main_runner") {
        // Void wrapper around `main` — keeps `_start` a clean WASI command.
        exports.export("_start", wasm_encoder::ExportKind::Func, runner_idx);
    } else if let Some(&main_idx) = emitter.func_map.get("main") {
        exports.export("_start", wasm_encoder::ExportKind::Func, main_idx);
    } else if let Some(&runner_idx) = emitter.func_map.get("__test_runner") {
        exports.export("_start", wasm_encoder::ExportKind::Func, runner_idx);
    } else if let Some(&init_idx) = emitter.func_map.get("__init_globals") {
        exports.export("_start", wasm_encoder::ExportKind::Func, init_idx);
    }
    // Export __alloc for FFI callers to allocate WASM linear memory
    if let Some(&alloc_idx) = emitter.func_map.get("__alloc") {
        exports.export("__alloc", wasm_encoder::ExportKind::Func, alloc_idx);
    }
    // Export __heap_save / __heap_restore so JS-side wrappers can implement
    // scoped (arena-style) cleanup after each foreign call. Without these
    // the bump allocator never frees and long-running benchmarks OOM.
    if let Some(&idx) = emitter.func_map.get("__heap_save") {
        exports.export("__heap_save", wasm_encoder::ExportKind::Func, idx);
    }
    if let Some(&idx) = emitter.func_map.get("__heap_restore") {
        exports.export("__heap_restore", wasm_encoder::ExportKind::Func, idx);
    }
    // Export public user functions (collected during emit)
    for (export_name, internal_name) in &emitter.user_exports {
        if let Some(&idx) = emitter.func_map.get(internal_name.as_str()) {
            exports.export(export_name, wasm_encoder::ExportKind::Func, idx);
        }
    }
    module.section(&exports);

    // ── Element section (populate function table, must come before Code) ──
    if !emitter.func_table.is_empty() {
        let mut elements = ElementSection::new();
        elements.active(
            Some(0),
            &wasm_encoder::ConstExpr::i32_const(0),
            Elements::Functions(std::borrow::Cow::Borrowed(&emitter.func_table)),
        );
        module.section(&elements);
    }

    // ── Code section ──
    let mut codes = CodeSection::new();
    for cf in &emitter.compiled {
        if let Some(ref patched) = cf.patched_body {
            codes.raw(patched);
        } else {
            codes.function(&cf.func);
        }
    }
    module.section(&codes);

    // ── Data section ──
    let mut data = DataSection::new();
    // Newline byte + string literals, starting at NEWLINE_OFFSET
    if !emitter.data_bytes.is_empty() {
        data.active(
            0,
            &wasm_encoder::ConstExpr::i32_const(NEWLINE_OFFSET as i32),
            emitter.data_bytes.iter().copied(),
        );
    }
    module.section(&data);

    // ── Custom `name` section ──
    // Attribute functions by name so a trap (e.g. a `RuntimeError: unreachable`
    // surfaced in the browser playground) points at a named function instead of
    // an anonymous `wasm-function[N]`. Built from func_map sorted by index, so it
    // is host-deterministic (same as the rest of the module).
    let mut fn_index_names: Vec<(u32, &str)> =
        emitter.func_map.iter().map(|(name, &idx)| (idx, name.as_str())).collect();
    fn_index_names.sort_by_key(|(idx, _)| *idx);
    fn_index_names.dedup_by_key(|(idx, _)| *idx);
    if !fn_index_names.is_empty() {
        let mut fn_names = NameMap::new();
        for (idx, name) in &fn_index_names {
            fn_names.append(*idx, name);
        }
        let mut names = NameSection::new();
        names.functions(&fn_names);
        module.section(&names);
    }

    module.finish()
}

// ── Test runner ─────────────────────────────────────────────────

/// Compile the __init_globals function.
#[allow(dead_code)] // Will be activated when top-let WASM codegen is wired up
fn compile_init_globals(emitter: &mut WasmEmitter, program: &IrProgram) {
    // C-007 by construction (§4 stage 3): this function evaluates every
    // top-let initializer EXACTLY in `global_init_order` — the same vector the
    // native main wrapper derives its eager forces from. The order is
    // dependency-respecting (#632): an imported module's heap global is
    // initialized before any importing top-let reads it. We index the
    // initializers by VarId (root + every module flatten into
    // `program.var_table` via UnifyVarTablesPass) and emit in that one order,
    // so a reorder of either side stays a single source of truth, not an
    // eager-vs-init cross-target drift.
    let init_exprs: HashMap<u32, &almide_ir::IrExpr> = program.top_lets.iter()
        .map(|tl| (tl.var.0, &tl.value))
        .chain(program.modules.iter().flat_map(|m| m.top_lets.iter().map(|tl| (tl.var.0, &tl.value))))
        .collect();
    {
        // Every decl in the order must have a known initializer, and the order
        // must cover exactly the declared top-lets — else the emission would
        // silently skip or double-init a global.
        debug_assert_eq!(
            program.codegen_annotations.global_init_order.len(), init_exprs.len(),
            "[COMPILER BUG] global_init_order does not cover the declared top-lets (C-007)"
        );
    }
    let void_type = emitter.register_type(vec![], vec![]);

    let mut local_decls = Vec::new();
    // ScratchAllocator locals
    // Generous fixed scratch caps — see functions.rs note (#417).
    let scratch_i32_cap = 64usize;
    let scratch_i64_cap = 48usize;
    let scratch_f64_cap = 48usize;
    let scratch_i32_base = local_decls.len() as u32;
    for _ in 0..scratch_i32_cap { local_decls.push((1, ValType::I32)); }
    let scratch_i64_base = local_decls.len() as u32;
    for _ in 0..scratch_i64_cap { local_decls.push((1, ValType::I64)); }
    let scratch_f64_base = local_decls.len() as u32;
    for _ in 0..scratch_f64_cap { local_decls.push((1, ValType::F64)); }
    let scratch_v128_cap = 8usize;
    let scratch_v128_base = local_decls.len() as u32;
    for _ in 0..scratch_v128_cap { local_decls.push((1, ValType::V128)); }

    let wasm_func = TrackedFunction::new(local_decls);
    let compiled_func = {
        let mut scratch_alloc = scratch::ScratchAllocator::new();
        scratch_alloc.set_bases_with_capacity(scratch_i32_base, scratch_i32_cap, scratch_i64_base, scratch_i64_cap, scratch_f64_base, scratch_f64_cap);
        scratch_alloc.set_v128_base(scratch_v128_base);
        let mut compiler = FuncCompiler {
            emitter: &mut *emitter,
            func: wasm_func,
            var_map: HashMap::new(),
            depth: 0,
            loop_stack: Vec::new(),
            scratch: scratch_alloc,
            // UnifyVarTablesPass flattens every module top-let VarId into the
            // root var_table, so one table resolves both root and module decls.
            var_table: &program.var_table,
            stub_ret_ty: Ty::Unit,
            current_module_name: None,
                live_heap: Vec::new(),
        };

        // Emit each initializer in dependency-respecting `global_init_order`
        // (#632), so an imported module's heap global is set before any
        // importing top-let reads it.
        for &decl in &program.codegen_annotations.global_init_order {
            let Some(&value) = init_exprs.get(&decl.0) else { continue };
            compiler.emit_expr(value);
            if let Some(&(global_idx, _)) = compiler.emitter.top_let_globals.get(&decl.0) {
                compiler.func.instruction(&wasm_encoder::Instruction::GlobalSet(global_idx));
            } else if let Some(&(global_idx, _)) = compiler.emitter.top_let_globals_by_name
                .get(program.var_table.get(decl).name.as_str())
            {
                compiler.func.instruction(&wasm_encoder::Instruction::GlobalSet(global_idx));
            } else {
                compiler.func.instruction(&wasm_encoder::Instruction::Drop);
            }
        }
        compiler.func
    };
    let compiled_func = {
        let mut f = compiled_func;
        f.instruction(&wasm_encoder::Instruction::End);
        f
    };

    emitter.add_compiled(CompiledFunc::tracked(void_type, compiled_func));
}

/// Compile a test runner function that calls each test, printing results.
fn compile_test_runner(emitter: &mut WasmEmitter, tests: &[(u32, String)], init_globals: Option<u32>) {
    let void_type = emitter.register_type(vec![], vec![]);
    let mut f = TrackedFunction::new([]);

    for (func_idx, test_name) in tests {
        // Re-initialize module globals before EVERY test: the native harness
        // stores module `var`s in `thread_local!`s and libtest runs each test
        // on its own thread, so each native test sees PRISTINE module state.
        // A single shared init here would leak one test's global mutations
        // into the next — an isolation (and order-sensitivity) divergence
        // from native (module_var_index_test's cross-test corruption class).
        if let Some(init_idx) = init_globals {
            f.instruction(&wasm_encoder::Instruction::Call(init_idx));
        }

        // Print test name
        let name_str = emitter.intern_string(&format!("test: {} ... ", test_name));
        f.instruction(&wasm_encoder::Instruction::I32Const(name_str as i32));
        f.instruction(&wasm_encoder::Instruction::Call(emitter.rt.println_str));

        // Call the test function (it will trap on assert_eq failure)
        f.instruction(&wasm_encoder::Instruction::Call(*func_idx));

        // If we get here, test passed
        let pass_str = emitter.intern_string("ok");
        f.instruction(&wasm_encoder::Instruction::I32Const(pass_str as i32));
        f.instruction(&wasm_encoder::Instruction::Call(emitter.rt.println_str));
    }

    f.instruction(&wasm_encoder::Instruction::End);
    emitter.add_compiled(CompiledFunc::tracked(void_type, f));
}

/// Compile `__main_runner`: call `main` and drop its result so the exported
/// `_start` is a void WASI command. `drop_count` is `main`'s wasm result arity
/// (0 for a void `main`, 1 for an effect fn returning a `Result`). `main` runs
/// `__init_globals` itself, so the runner does nothing else.
fn compile_main_runner(emitter: &mut WasmEmitter, main_idx: u32, drop_count: usize, is_effect: bool) {
    use wasm_encoder::Instruction as Ins;
    fn m(offset: u64) -> wasm_encoder::MemArg {
        wasm_encoder::MemArg { offset, align: 2, memory_index: 0 }
    }
    let void_type = emitter.register_type(vec![], vec![]);

    // Non-effect `fn main` (returns `Unit`) cannot fail: call and drop its result.
    if !is_effect {
        let mut f = TrackedFunction::new([]);
        f.instruction(&Ins::Call(main_idx));
        for _ in 0..drop_count {
            f.instruction(&Ins::Drop);
        }
        f.instruction(&Ins::End);
        emitter.add_compiled(CompiledFunc::tracked(void_type, f));
        return;
    }

    // `effect fn main` returns `Result<Unit, String>` (`[tag:i32@0][payload@4]`).
    // On `Err`, write `Error: <msg>\n` to stderr (fd 2) and `proc_exit(1)` so the
    // wasm command's failure (non-zero exit + stderr) matches native byte-for-byte
    // (native's `fn main` wrapper emits the same `Error: <msg>` via Display + exit).
    // On `Ok` (tag 0), fall through and return normally → exit 0.
    let err_prefix = emitter.intern_string("Error: ") as i32;
    let newline = emitter.intern_string("\n") as i32;
    let data_off = emitter.layout_reg.fixed_offset(
        engine::layout::STRING, engine::layout::string::DATA) as i32;
    let concat = emitter.rt.concat_str;
    let fd_write = emitter.rt.fd_write;
    let proc_exit = emitter.rt.proc_exit;

    // locals: 0 = main's Result ptr, 1 = composed "Error: <msg>\n" string ptr
    let mut f = TrackedFunction::new([(2u32, ValType::I32)]);
    f.instruction(&Ins::Call(main_idx));
    f.instruction(&Ins::LocalSet(0));
    f.instruction(&Ins::LocalGet(0));
    f.instruction(&Ins::I32Load(m(0)));                    // tag
    f.instruction(&Ins::If(wasm_encoder::BlockType::Empty));
    //   msg = "Error: " ++ <err String @ payload> ++ "\n"
    f.instruction(&Ins::I32Const(err_prefix));
    f.instruction(&Ins::LocalGet(0));
    f.instruction(&Ins::I32Load(m(4)));                    // err String ptr
    f.instruction(&Ins::Call(concat));
    f.instruction(&Ins::I32Const(newline));
    f.instruction(&Ins::Call(concat));
    f.instruction(&Ins::LocalSet(1));
    //   iov[0] = { buf: msg + DATA, len: *msg } at scratch [0..8); nwritten at 8
    f.instruction(&Ins::I32Const(0));
    f.instruction(&Ins::LocalGet(1));
    f.instruction(&Ins::I32Const(data_off));
    f.instruction(&Ins::I32Add);
    f.instruction(&Ins::I32Store(m(0)));
    f.instruction(&Ins::I32Const(4));
    f.instruction(&Ins::LocalGet(1));
    f.instruction(&Ins::I32Load(m(0)));
    f.instruction(&Ins::I32Store(m(0)));
    //   fd_write(fd=2 stderr, iovs=0, iovs_len=1, nwritten=8)
    f.instruction(&Ins::I32Const(2));
    f.instruction(&Ins::I32Const(0));
    f.instruction(&Ins::I32Const(1));
    f.instruction(&Ins::I32Const(8));
    f.instruction(&Ins::Call(fd_write));
    f.instruction(&Ins::Drop);
    f.instruction(&Ins::I32Const(1));
    f.instruction(&Ins::Call(proc_exit));
    f.instruction(&Ins::End);                              // end if
    f.instruction(&Ins::End);                              // end function
    emitter.add_compiled(CompiledFunc::tracked(void_type, f));
}

/// Pre-register variant deep-equality functions for all variant types with pointer fields.
/// Must be called before Phase 2 (compilation) so func_idx is known at emit time.
fn register_variant_eq_funcs(emitter: &mut WasmEmitter) {
    let type_idx = emitter.register_type(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    // Collect variant names that need deep eq (have pointer fields).
    // Sort: variant_info is a HashMap, and its iteration order (host-dependent —
    // hash seed + usize bucket layout) here determines the func indices these
    // __eq_* functions reserve. Unsorted, a 32-bit host (wasm32) reserves them
    // in a different order than a 64-bit host, shifting every later function's
    // index and producing a divergent, trapping module. Sorting makes index
    // reservation a pure function of the program, and must match the (also
    // sorted) compile order in compile_variant_eq_funcs.
    let mut names: Vec<String> = emitter.variant_info.iter()
        .filter(|(_, cases)| {
            cases.iter().any(|c| c.fields.iter().any(|(_, ft)| {
                !matches!(ft, almide_lang::types::Ty::Int | almide_lang::types::Ty::Float | almide_lang::types::Ty::Bool | almide_lang::types::Ty::Unit)
            }))
        })
        .map(|(name, _)| name.clone())
        .collect();
    names.sort();
    for name in names {
        let func_idx = emitter.register_func(&format!("__eq_{}", name), type_idx);
        emitter.eq_funcs.insert(name, func_idx);
    }
}

/// Compile variant deep-equality function bodies.
/// Each function: (a: i32, b: i32) -> i32 — compares tag then dispatches to per-case field comparison.
fn compile_variant_eq_funcs(emitter: &mut WasmEmitter, var_table: &almide_ir::VarTable) {
    // Collect eq_funcs entries (name → func_idx) and corresponding cases.
    // Sort by name so body-emission order matches the (sorted) index-reservation
    // order in register_variant_eq_funcs. add_compiled pushes bodies positionally
    // (function index = num_imports + push position), so the two orders MUST
    // agree, and both must be host-independent — otherwise a 32-bit host places
    // __eq_Foo's body at __eq_Bar's index and the module traps.
    let mut eq_entries: Vec<(String, u32)> = emitter.eq_funcs.iter()
        .map(|(n, &idx)| (n.clone(), idx))
        .collect();
    eq_entries.sort();

    for (name, _func_idx) in &eq_entries {
        let cases = match emitter.variant_info.get(name.as_str()) {
            Some(c) => c.clone(),
            None => continue,
        };

        let type_idx = emitter.register_type(
            vec![ValType::I32, ValType::I32],
            vec![ValType::I32],
        );

        // Build function body with its own FuncCompiler
        let mut local_decls = Vec::new();
        let scratch_i32_cap = 16usize;
        let scratch_i64_cap = 8usize;
        let scratch_f64_cap = 2usize;
        let scratch_i32_base = 2u32; // after 2 params
        for _ in 0..scratch_i32_cap { local_decls.push((1, ValType::I32)); }
        let scratch_i64_base = scratch_i32_base + scratch_i32_cap as u32;
        for _ in 0..scratch_i64_cap { local_decls.push((1, ValType::I64)); }
        let scratch_f64_base = scratch_i64_base + scratch_i64_cap as u32;
        for _ in 0..scratch_f64_cap { local_decls.push((1, ValType::F64)); }

        let wasm_func = TrackedFunction::new(local_decls);
        let mut scratch_alloc = scratch::ScratchAllocator::new();
        scratch_alloc.set_bases_with_capacity(
            scratch_i32_base, scratch_i32_cap,
            scratch_i64_base, scratch_i64_cap,
            scratch_f64_base, scratch_f64_cap,
        );

        let compiled_func = {
            let mut compiler = FuncCompiler {
                emitter: &mut *emitter,
                func: wasm_func,
                var_map: std::collections::HashMap::new(),
                depth: 0,
                loop_stack: Vec::new(),
                scratch: scratch_alloc,
                var_table,
                stub_ret_ty: almide_lang::types::Ty::Unit,
                current_module_name: None,
                live_heap: Vec::new(),
            };

            // Compare tags
            wasm!(compiler.func, {
                local_get(0); i32_load(0);
                local_get(1); i32_load(0);
                i32_ne;
                if_empty; i32_const(0); return_; end;
            });

            // Branch on tag for each case
            let non_empty: Vec<_> = cases.iter().filter(|c| !c.fields.is_empty()).collect();
            if non_empty.is_empty() {
                wasm!(compiler.func, { i32_const(1); });
            } else {
                for case in &non_empty {
                    wasm!(compiler.func, {
                        local_get(0); i32_load(0);
                        i32_const(case.tag as i32);
                        i32_eq;
                        if_i32;
                    });
                    // Compare fields (AND results together)
                    let mut offset = 4u32;
                    for (fi, (_, field_ty)) in case.fields.iter().enumerate() {
                        let field_size = values::byte_size(field_ty);
                        wasm!(compiler.func, { local_get(0); });
                        compiler.emit_load_at(field_ty, offset);
                        wasm!(compiler.func, { local_get(1); });
                        compiler.emit_load_at(field_ty, offset);
                        let ft = field_ty.clone();
                        compiler.emit_eq_typed(&ft);
                        if fi > 0 {
                            wasm!(compiler.func, { i32_and; });
                        }
                        offset += field_size;
                    }
                    wasm!(compiler.func, { else_; });
                }
                wasm!(compiler.func, { i32_const(1); }); // default: unit case → equal
                for _ in 0..non_empty.len() {
                    wasm!(compiler.func, { end; });
                }
            }

            compiler.func.instruction(&wasm_encoder::Instruction::End);
            compiler.func
        };

        emitter.add_compiled(CompiledFunc::tracked(type_idx, compiled_func));
    }
}
