//! Unit 6 stage 2: typed IR → structural wasm emission — the scalar-program
//! slice (Bind + control flow + user functions).
//!
//! Constitution (ARCHITECTURE.md §3/§6.6): binary emission via wasm-encoder
//! only — no WAT text, no string templates; block layout derives from
//! `almide-layout` (string literals are laid out as REAL blocks,
//! `[rc][len][cap][payload]`); every module is validated before acceptance
//! (the wall, in the gate).
//!
//! Stage-2 value model (what the ×368 `stmt:Bind` wall demanded, plus the
//! walls behind it):
//!   - `Int`/`Int64` → wasm `i64`; `Bool` → `i32` (0/1); `String` → `i32`
//!     holding the BLOCK BASE address (payload/len derive from the layout
//!     crate — never a bare payload pointer, so header reads stay honest).
//!   - let/var binds and assigns become wasm locals (VarIds are unique per
//!     variable, so shadowing is already resolved upstream).
//!   - integer arithmetic/comparison, `and`/`or` with SHORT-CIRCUIT
//!     evaluation (an `if` block, not a strict bitop — right-operand traps
//!     must not fire when the left decides), `if` in value and statement
//!     position, `while`.
//!   - USER FUNCTIONS with scalar signatures become real wasm functions:
//!     params are the leading locals, calls are direct `call`s, recursion
//!     falls out for free. A function whose body doesn't lower yet gets an
//!     `unreachable` stub; emission then REFUSES the program iff such a
//!     function is reachable from `main` (call-graph BFS) — an unreachable
//!     stub can never fire.
//!   - top-level lets lower as an eager prelude in `main`: with `main` the
//!     only entry and cross-function global reads refused (`var:unmapped`),
//!     that is observably identical to the oracle's eager phase.
//!   - `println("${...}")` interpolation builds the line in a scratch line
//!     buffer via emitted runtime helpers: `$append_copy` (memory.copy),
//!     `$append_i64` (hand-written wasm itoa working in the NEGATIVE domain
//!     so `i64::MIN` never overflows) and `$append_bool`.
//!
//! Memory map: `[0,12)` null guard (layout NULL_ADDR stays dead) ·
//! `[16,48)` itoa scratch (32 B ≥ "-9223372036854775808") · `[48,…)` the
//! literal pool · line buffer from `align16(pool_end)` to the memory end —
//! overflow runs off memory and TRAPS instead of silently corrupting.
//! The line-buffer start is global 0, so bodies can reference it while the
//! pool is still growing (single lowering pass per function).

use std::collections::{HashMap, HashSet};

use almide_ir::{
    BinOp, CallTarget, IrExpr, IrExprKind, IrFunction, IrProgram, IrStmt, IrStmtKind,
    IrStringPart, IrTopLet, UnOp, VarId,
};
use almide_types::types::Ty;
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection,
    Function, FunctionSection, GlobalSection, GlobalType, ImportSection, MemArg, MemorySection,
    MemoryType, Module, TypeSection, ValType,
};

#[derive(Debug)]
pub enum EmitError {
    /// This IR shape is outside the current slice. The reason string feeds
    /// the burn-up histogram — precise, greppable, shrink-only.
    Unsupported(String),
}

fn unsup<T>(what: &str) -> Result<T, EmitError> {
    Err(EmitError::Unsupported(what.to_string()))
}

// ── fixed memory map ────────────────────────────────────────────────────

/// itoa scratch region: digits are written back-to-front ending here.
/// 32 bytes ≥ the longest rendering, `-9223372036854775808` (20 bytes).
const ITOA_END: u32 = 48;
/// The pool starts right after the scratch: null guard `[0,PAYLOAD)`,
/// padding to 16, scratch `[16,48)`.
const POOL_START: u32 = ITOA_END;
/// Minimum room the line buffer must have beyond the pool.
const LINE_BUF_MIN: u64 = 65536;

// ── function / type / global indices ────────────────────────────────────

const F_PRINTLN_IMPORT: u32 = 0;
const F_EPRINTLN_IMPORT: u32 = 1;
const F_PRINTLN_BLOCK: u32 = 2;
const F_EPRINTLN_BLOCK: u32 = 3;
const F_APPEND_COPY: u32 = 4;
const F_ITOA: u32 = 5;
const F_APPEND_I64: u32 = 6;
const F_APPEND_BOOL: u32 = 7;
const F_ALLOC: u32 = 8;
const F_INT_TO_STRING: u32 = 9;
const F_CONCAT: u32 = 10;
/// First program-function index; `main` sits after every program function.
const F_FN_BASE: u32 = 11;
/// Fixed type indices: 0 print(ptr,len)→(), 1 block-print(i32)→(),
/// 2 append_copy, 3 append_i64, 4 main ()→(), 5 (i32,i32)→i32
/// (append_bool/concat), 6 (i64)→i32 (itoa/int_to_string),
/// 7 (i32)→i32 (alloc); program-function types start after.
const T_MAIN: u32 = 4;
const T_FN_BASE: u32 = 8;
/// Immutable i32 global: the line-buffer start (= align16(pool end)).
const G_LINE_START: u32 = 0;
/// Mutable i32 global: the bump-allocator head (starts after the line
/// buffer; `$alloc` grows memory as needed — blocks are never freed in
/// this slice, which is sound for run-to-completion programs).
const G_HEAP: u32 = 1;

// ── slice value model ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SliceTy {
    /// Almide Int/Int64 — wasm i64.
    Int,
    /// Almide Bool — wasm i32, 0/1.
    Bool,
    /// Almide String — wasm i32 holding the block BASE address.
    Str,
}

impl SliceTy {
    fn val_type(self) -> ValType {
        match self {
            SliceTy::Int => ValType::I64,
            SliceTy::Bool | SliceTy::Str => ValType::I32,
        }
    }
}

fn slice_ty_of(ty: &Ty) -> Option<SliceTy> {
    match ty {
        Ty::Int | Ty::Int64 => Some(SliceTy::Int),
        Ty::Bool => Some(SliceTy::Bool),
        Ty::String => Some(SliceTy::Str),
        _ => None,
    }
}

// ── literal pool ────────────────────────────────────────────────────────

/// String literals placed in linear memory as REAL layout blocks.
struct Pool {
    data: Vec<u8>,
    interned: HashMap<String, u32>,
}

impl Pool {
    fn new() -> Self {
        // Reserve the null guard + itoa scratch: the layout's NULL_ADDR (0)
        // must never name a live block, and the scratch must not overlap
        // pool blocks.
        Pool { data: vec![0; POOL_START as usize], interned: HashMap::new() }
    }

    /// Intern `s` as a block; returns the block BASE address (deduped).
    fn intern(&mut self, s: &str) -> u32 {
        if let Some(&base) = self.interned.get(s) {
            return base;
        }
        let base = self.data.len() as u32;
        let bytes = s.as_bytes();
        let len = bytes.len() as u32;
        let mut header = vec![0u8; almide_layout::PAYLOAD as usize];
        header[almide_layout::RC.offset as usize..][..4].copy_from_slice(&1u32.to_le_bytes());
        header[almide_layout::LEN.offset as usize..][..4].copy_from_slice(&len.to_le_bytes());
        header[almide_layout::CAP.offset as usize..][..4].copy_from_slice(&len.to_le_bytes());
        self.data.extend_from_slice(&header);
        self.data.extend_from_slice(bytes);
        base
    }
}

fn len_memarg() -> MemArg {
    MemArg { offset: u64::from(almide_layout::LEN.offset), align: 2, memory_index: 0 }
}

// ── program-function table ──────────────────────────────────────────────

struct FnInfo {
    wasm_index: u32,
    params: Vec<SliceTy>,
    ret: Option<SliceTy>,
    /// Why call sites must refuse this function (None = callable).
    refuse: Option<String>,
}

struct FnTable {
    by_name: HashMap<String, usize>,
    infos: Vec<FnInfo>,
}

fn fn_signature(f: &IrFunction) -> Result<(Vec<SliceTy>, Option<SliceTy>), String> {
    if f.is_effect {
        return Err("effect".into());
    }
    if f.generics.is_some() {
        return Err("generic".into());
    }
    let mut params = Vec::new();
    for p in &f.params {
        if p.is_mut {
            return Err("mut-param".into());
        }
        let Some(sty) = slice_ty_of(&p.ty) else {
            return Err(format!("param-ty:{}", ty_name(&p.ty)));
        };
        params.push(sty);
    }
    let ret = match &f.ret_ty {
        Ty::Unit => None,
        other => match slice_ty_of(other) {
            Some(sty) => Some(sty),
            None => return Err(format!("ret-ty:{}", ty_name(other))),
        },
    };
    Ok((params, ret))
}

// ── entry ───────────────────────────────────────────────────────────────

/// Emit a core wasm module for `ir`, or say precisely why not yet.
pub fn emit_program(ir: &IrProgram) -> Result<Vec<u8>, EmitError> {
    let Some(main) = ir.functions.iter().find(|f| f.name.as_str() == "main") else {
        return unsup("no main function");
    };
    let program_fns: Vec<&IrFunction> =
        ir.functions.iter().filter(|f| !f.is_test && f.name.as_str() != "main").collect();

    // Signature table first: call sites need indices and types up front.
    let mut table = FnTable { by_name: HashMap::new(), infos: Vec::new() };
    for (i, f) in program_fns.iter().enumerate() {
        let (params, ret, refuse) = match fn_signature(f) {
            Ok((p, r)) => (p, r, None),
            Err(reason) => (Vec::new(), None, Some(reason)),
        };
        table.by_name.insert(f.name.as_str().to_string(), i);
        table.infos.push(FnInfo { wasm_index: F_FN_BASE + i as u32, params, ret, refuse });
    }
    let main_index = F_FN_BASE + program_fns.len() as u32;

    let mut pool = Pool::new();
    // Interned eagerly so $append_bool can carry their fixed addresses.
    let true_base = pool.intern("true");
    let false_base = pool.intern("false");

    // Lower every callable function; a body that doesn't lower yet is
    // recorded (not fatal) — fatal only if `main` can reach it.
    let mut lowered: Vec<Result<(Function, HashSet<String>), String>> = Vec::new();
    for (i, f) in program_fns.iter().enumerate() {
        if let Some(r) = &table.infos[i].refuse {
            lowered.push(Err(r.clone()));
            continue;
        }
        let params: Vec<(VarId, SliceTy)> =
            f.params.iter().zip(&table.infos[i].params).map(|(p, &t)| (p.var, t)).collect();
        match lower_fn(&params, table.infos[i].ret, &f.body, &[], &table, &mut pool) {
            Ok(ok) => lowered.push(Ok(ok)),
            Err(EmitError::Unsupported(r)) => lowered.push(Err(r)),
        }
    }

    // `main`: top-lets as the eager prelude, then the body. Failure here is
    // fatal — main is always reachable.
    let (main_fn, main_calls) =
        lower_fn(&[], None, &main.body, &ir.top_lets, &table, &mut pool)?;

    // Reachability: refuse the program iff a call chain from `main` lands
    // on a function whose body did not lower (its stub would trap).
    let mut queue: Vec<String> = main_calls.iter().cloned().collect();
    let mut visited: HashSet<String> = HashSet::new();
    while let Some(name) = queue.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let Some(&i) = table.by_name.get(&name) else { continue };
        match &lowered[i] {
            Err(reason) => return unsup(reason),
            Ok((_, calls)) => queue.extend(calls.iter().cloned()),
        }
    }

    // ── assemble the module structurally ────────────────────────────────
    let line_start = (pool.data.len() as u32 + 15) & !15;
    let heap_start = u64::from(line_start) + LINE_BUF_MIN;
    let pages = heap_start.div_ceil(65536);

    let mut types = TypeSection::new();
    types.ty().function([ValType::I32, ValType::I32], []); // 0: print import (ptr, len)
    types.ty().function([ValType::I32], []); // 1: block-print(base)
    types.ty().function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]); // 2: append_copy
    types.ty().function([ValType::I32, ValType::I64], [ValType::I32]); // 3: append_i64
    types.ty().function([], []); // 4: main
    types.ty().function([ValType::I32, ValType::I32], [ValType::I32]); // 5: append_bool / concat
    types.ty().function([ValType::I64], [ValType::I32]); // 6: itoa / int_to_string
    types.ty().function([ValType::I32], [ValType::I32]); // 7: alloc
    for (i, info) in table.infos.iter().enumerate() {
        // Refused functions keep a placeholder type — their stub body is
        // `unreachable` and no call site ever targets them.
        debug_assert_eq!(T_FN_BASE as usize + i, 8 + i);
        if info.refuse.is_some() {
            types.ty().function([], []);
        } else {
            let params: Vec<ValType> = info.params.iter().map(|t| t.val_type()).collect();
            let results: Vec<ValType> = info.ret.iter().map(|t| t.val_type()).collect();
            types.ty().function(params, results);
        }
    }

    let mut imports = ImportSection::new();
    imports.import("almide", "println", EntityType::Function(0));
    imports.import("almide", "eprintln", EntityType::Function(0));

    let mut functions = FunctionSection::new();
    functions.function(1); // F_PRINTLN_BLOCK
    functions.function(1); // F_EPRINTLN_BLOCK
    functions.function(2); // F_APPEND_COPY
    functions.function(6); // F_ITOA
    functions.function(3); // F_APPEND_I64
    functions.function(5); // F_APPEND_BOOL
    functions.function(7); // F_ALLOC
    functions.function(6); // F_INT_TO_STRING
    functions.function(5); // F_CONCAT
    for i in 0..table.infos.len() {
        functions.function(T_FN_BASE + i as u32);
    }
    functions.function(T_MAIN); // main, last

    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: pages,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });

    let mut globals = GlobalSection::new();
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: false, shared: false },
        &ConstExpr::i32_const(line_start as i32),
    );
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const(heap_start as i32),
    );

    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("main", ExportKind::Func, main_index);

    let mut code = CodeSection::new();
    code.function(&emit_block_print(F_PRINTLN_IMPORT));
    code.function(&emit_block_print(F_EPRINTLN_IMPORT));
    code.function(&emit_append_copy());
    code.function(&emit_itoa());
    code.function(&emit_append_i64());
    code.function(&emit_append_bool(true_base, false_base));
    code.function(&emit_alloc());
    code.function(&emit_int_to_string());
    code.function(&emit_concat());
    for l in &lowered {
        match l {
            Ok((f, _)) => {
                code.function(f);
            }
            Err(_) => {
                // Unreachable-from-main stub (the BFS above guarantees it).
                let mut stub = Function::new([]);
                stub.instructions().unreachable().end();
                code.function(&stub);
            }
        }
    }
    code.function(&main_fn);

    let mut data = DataSection::new();
    data.active(0, &ConstExpr::i32_const(0), pool.data.iter().copied());

    let mut module = Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&memories)
        .section(&globals)
        .section(&exports)
        .section(&code)
        .section(&data);
    Ok(module.finish())
}

/// Lower one function body (used for `main` and every program function):
/// params become the leading locals, collected Binds follow, then the
/// interp-build cursor and a scratch i32.
fn lower_fn(
    params: &[(VarId, SliceTy)],
    ret: Option<SliceTy>,
    body: &IrExpr,
    top_lets: &[IrTopLet],
    table: &FnTable,
    pool: &mut Pool,
) -> Result<(Function, HashSet<String>), EmitError> {
    let mut locals: HashMap<VarId, (u32, SliceTy)> = HashMap::new();
    let mut seen: HashSet<VarId> = HashSet::new();
    for (i, (var, ty)) in params.iter().enumerate() {
        locals.insert(*var, (i as u32, *ty));
        seen.insert(*var);
    }

    let mut binds: Vec<(VarId, SliceTy)> = Vec::new();
    for tl in top_lets {
        let Some(sty) = slice_ty_of(&tl.ty) else {
            return unsup(&format!("bind-ty:{}", ty_name(&tl.ty)));
        };
        if seen.insert(tl.var) {
            binds.push((tl.var, sty));
        }
        collect_binds(&tl.value, &mut binds, &mut seen)?;
    }
    collect_binds(body, &mut binds, &mut seen)?;

    let mut local_decls: Vec<(u32, ValType)> = Vec::new();
    for (i, (var, ty)) in binds.iter().enumerate() {
        locals.insert(*var, ((params.len() + i) as u32, *ty));
        local_decls.push((1, ty.val_type()));
    }
    let cursor_local = (params.len() + binds.len()) as u32;
    let tmp_i32_local = cursor_local + 1;
    local_decls.push((1, ValType::I32));
    local_decls.push((1, ValType::I32));

    let mut f = Function::new(local_decls);
    let mut calls: HashSet<String> = HashSet::new();
    {
        let mut em = Emitter {
            pool,
            locals: &locals,
            table,
            calls: &mut calls,
            cursor_local,
            tmp_i32_local,
            f: &mut f,
        };
        for tl in top_lets {
            let (idx, declared) = em.locals[&tl.var];
            let got = em.lower_value(&tl.value)?;
            if got != declared {
                return unsup(&format!("top-let:ty-mismatch:{got:?}-vs-{declared:?}"));
            }
            em.f.instructions().local_set(idx);
        }
        match ret {
            None => em.lower_stmt_expr(body)?,
            Some(want) => em.expect(body, want)?,
        }
    }
    f.instructions().end();
    Ok((f, calls))
}

// ── emitted runtime helpers ─────────────────────────────────────────────

/// `$*_block(base: i32)`: derive (payload, len) from the layout and call
/// the given host import — the ONLY place a block is unpacked for printing.
fn emit_block_print(import: u32) -> Function {
    let mut f = Function::new([]);
    f.instructions()
        .local_get(0)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add() // payload ptr
        .local_get(0)
        .i32_load(len_memarg()) // len from the header
        .call(import)
        .end();
    f
}

/// `$append_copy(cur: i32, src: i32, len: i32) -> i32`: memory.copy bytes
/// to the cursor, return the advanced cursor.
fn emit_append_copy() -> Function {
    let mut f = Function::new([]);
    f.instructions()
        .local_get(0)
        .local_get(1)
        .local_get(2)
        .memory_copy(0, 0)
        .local_get(0)
        .local_get(2)
        .i32_add()
        .end();
    f
}

/// `$itoa(v: i64) -> i32`: decimal-render `v` into the scratch region
/// (digits back-to-front, ending at ITOA_END) and return the byte length —
/// the rendering starts at `ITOA_END - len`. Works in the NEGATIVE domain
/// so `i64::MIN` never overflows: `u = v < 0 ? v : -v`, digit =
/// `-(u % 10)`, and `u / 10` truncates toward zero so the loop terminates.
fn emit_itoa() -> Function {
    // params: 0=v i64; locals: 1=p i32, 2=u i64, 3=neg i32
    let (v, p, u, neg) = (0u32, 1u32, 2u32, 3u32);
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64), (1, ValType::I32)]);
    let byte = MemArg { offset: 0, align: 0, memory_index: 0 };
    let mut i = f.instructions();
    // p = ITOA_END; neg = v < 0; u = neg ? v : 0 - v
    i.i32_const(ITOA_END as i32).local_set(p);
    i.local_get(v).i64_const(0).i64_lt_s().local_set(neg);
    i.local_get(neg).if_(BlockType::Empty);
    i.local_get(v).local_set(u);
    i.else_();
    i.i64_const(0).local_get(v).i64_sub().local_set(u);
    i.end();
    // do-while: write digits back-to-front (always at least one → renders 0)
    i.loop_(BlockType::Empty);
    i.local_get(p).i32_const(1).i32_sub().local_set(p);
    i.local_get(p);
    i.i64_const(i64::from(b'0'));
    i.i64_const(0).local_get(u).i64_const(10).i64_rem_s().i64_sub(); // -(u%10) ∈ 0..=9
    i.i64_add().i32_wrap_i64().i32_store8(byte);
    i.local_get(u).i64_const(10).i64_div_s().local_set(u);
    i.local_get(u).i64_const(0).i64_ne().br_if(0);
    i.end();
    // sign
    i.local_get(neg).if_(BlockType::Empty);
    i.local_get(p).i32_const(1).i32_sub().local_set(p);
    i.local_get(p).i32_const(i32::from(b'-')).i32_store8(byte);
    i.end();
    // return ITOA_END - p
    i.i32_const(ITOA_END as i32).local_get(p).i32_sub();
    i.end();
    f
}

/// `$append_i64(cur: i32, v: i64) -> i32`: itoa then copy to the cursor;
/// returns the advanced cursor.
fn emit_append_i64() -> Function {
    // params: 0=cur i32, 1=v i64; locals: 2=len i32
    let (cur, v, len) = (0u32, 1u32, 2u32);
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).call(F_ITOA).local_set(len);
    i.local_get(cur);
    i.i32_const(ITOA_END as i32).local_get(len).i32_sub(); // src
    i.local_get(len);
    i.memory_copy(0, 0);
    i.local_get(cur).local_get(len).i32_add();
    i.end();
    f
}

/// `$alloc(len: i32) -> i32`: bump-allocate a layout-true block (header
/// rc=1/len/cap=len + payload), growing memory when needed; returns the
/// block BASE. Blocks are never freed in this slice.
fn emit_alloc() -> Function {
    // params: 0=len i32; locals: 1=base i32, 2=next i32
    let (len, base, next) = (0u32, 1u32, 2u32);
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    // base = G_HEAP; next = (base + PAYLOAD + len + 3) & !3
    i.global_get(G_HEAP).local_set(base);
    i.local_get(base)
        .i32_const(almide_layout::PAYLOAD as i32)
        .i32_add()
        .local_get(len)
        .i32_add()
        .i32_const(3)
        .i32_add()
        .i32_const(-4)
        .i32_and()
        .local_set(next);
    // if next > memory.size * 64Ki: grow just enough; a failed grow traps.
    i.local_get(next).memory_size(0).i32_const(16).i32_shl().i32_gt_u().if_(BlockType::Empty);
    i.local_get(next)
        .memory_size(0)
        .i32_const(16)
        .i32_shl()
        .i32_sub()
        .i32_const(65535)
        .i32_add()
        .i32_const(16)
        .i32_shr_u()
        .memory_grow(0)
        .i32_const(0)
        .i32_lt_s()
        .if_(BlockType::Empty)
        .unreachable()
        .end();
    i.end();
    // header: rc = 1, len, cap = len; advance the bump head
    i.local_get(base).i32_const(1).i32_store(word(almide_layout::RC.offset));
    i.local_get(base).local_get(len).i32_store(word(almide_layout::LEN.offset));
    i.local_get(base).local_get(len).i32_store(word(almide_layout::CAP.offset));
    i.local_get(next).global_set(G_HEAP);
    i.local_get(base);
    i.end();
    f
}

/// `$int_to_string(v: i64) -> i32`: itoa into the scratch, then a fresh
/// layout block holding the rendering; returns the block BASE.
fn emit_int_to_string() -> Function {
    // params: 0=v i64; locals: 1=len i32, 2=base i32
    let (v, len, base) = (0u32, 1u32, 2u32);
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).call(F_ITOA).local_set(len);
    i.local_get(len).call(F_ALLOC).local_set(base);
    i.local_get(base).i32_const(almide_layout::PAYLOAD as i32).i32_add(); // dst
    i.i32_const(ITOA_END as i32).local_get(len).i32_sub(); // src
    i.local_get(len);
    i.memory_copy(0, 0);
    i.local_get(base);
    i.end();
    f
}

/// `$concat(a: i32, b: i32) -> i32`: fresh block holding a's bytes then
/// b's bytes; returns the block BASE.
fn emit_concat() -> Function {
    // params: 0=a i32, 1=b i32; locals: 2=la i32, 3=lb i32, 4=base i32
    let (a, b, la, lb, base) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let payload = almide_layout::PAYLOAD as i32;
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(a).i32_load(len_memarg()).local_set(la);
    i.local_get(b).i32_load(len_memarg()).local_set(lb);
    i.local_get(la).local_get(lb).i32_add().call(F_ALLOC).local_set(base);
    // copy a
    i.local_get(base).i32_const(payload).i32_add();
    i.local_get(a).i32_const(payload).i32_add();
    i.local_get(la);
    i.memory_copy(0, 0);
    // copy b (dst offset by la)
    i.local_get(base).i32_const(payload).i32_add().local_get(la).i32_add();
    i.local_get(b).i32_const(payload).i32_add();
    i.local_get(lb);
    i.memory_copy(0, 0);
    i.local_get(base);
    i.end();
    f
}

/// `$append_bool(cur: i32, b: i32) -> i32`: append `"true"`/`"false"`
/// (interned pool blocks) at the cursor, return the advanced cursor.
fn emit_append_bool(true_base: u32, false_base: u32) -> Function {
    let payload = |base: u32| (base + almide_layout::PAYLOAD) as i32;
    let mut f = Function::new([]);
    f.instructions()
        .local_get(0)
        .i32_const(payload(true_base))
        .i32_const(payload(false_base))
        .local_get(1)
        .select()
        .i32_const("true".len() as i32)
        .i32_const("false".len() as i32)
        .local_get(1)
        .select()
        .call(F_APPEND_COPY)
        .end();
    f
}

// ── pre-pass: Binds → locals ────────────────────────────────────────────

/// Collect every Bind the lowering traversal can reach, in first-bind
/// order. Mirrors `Emitter`'s traversal: a Bind the lowering CAN reach but
/// this pass misses would surface as the honest `bind:unmapped` reason,
/// never a bad module.
fn collect_binds(
    e: &IrExpr,
    out: &mut Vec<(VarId, SliceTy)>,
    seen: &mut HashSet<VarId>,
) -> Result<(), EmitError> {
    match &e.kind {
        IrExprKind::Block { stmts, expr } => {
            for s in stmts {
                collect_binds_stmt(s, out, seen)?;
            }
            if let Some(tail) = expr {
                collect_binds(tail, out, seen)?;
            }
            Ok(())
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_binds(cond, out, seen)?;
            collect_binds(then, out, seen)?;
            collect_binds(else_, out, seen)
        }
        IrExprKind::While { cond, body } => {
            collect_binds(cond, out, seen)?;
            for s in body {
                collect_binds_stmt(s, out, seen)?;
            }
            Ok(())
        }
        IrExprKind::Call { args, .. } => {
            for a in args {
                collect_binds(a, out, seen)?;
            }
            Ok(())
        }
        IrExprKind::BinOp { left, right, .. } => {
            collect_binds(left, out, seen)?;
            collect_binds(right, out, seen)
        }
        IrExprKind::UnOp { operand, .. } => collect_binds(operand, out, seen),
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p {
                    collect_binds(expr, out, seen)?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn collect_binds_stmt(
    s: &IrStmt,
    out: &mut Vec<(VarId, SliceTy)>,
    seen: &mut HashSet<VarId>,
) -> Result<(), EmitError> {
    match &s.kind {
        IrStmtKind::Bind { var, ty, value, .. } => {
            let Some(sty) = slice_ty_of(ty) else {
                return unsup(&format!("bind-ty:{}", ty_name(ty)));
            };
            if seen.insert(*var) {
                out.push((*var, sty));
            }
            collect_binds(value, out, seen)
        }
        IrStmtKind::Assign { value, .. } => collect_binds(value, out, seen),
        IrStmtKind::Expr { expr } => collect_binds(expr, out, seen),
        _ => Ok(()), // lowering unsups these before any local is needed
    }
}

// ── body lowering ───────────────────────────────────────────────────────

struct Emitter<'a> {
    pool: &'a mut Pool,
    locals: &'a HashMap<VarId, (u32, SliceTy)>,
    table: &'a FnTable,
    calls: &'a mut HashSet<String>,
    cursor_local: u32,
    tmp_i32_local: u32,
    f: &'a mut Function,
}

impl Emitter<'_> {
    /// Statement position: Unit-typed shapes only (blocks, calls, control).
    fn lower_stmt_expr(&mut self, e: &IrExpr) -> Result<(), EmitError> {
        match &e.kind {
            IrExprKind::Block { stmts, expr } => {
                for s in stmts {
                    self.lower_stmt(s)?;
                }
                if let Some(tail) = expr {
                    self.lower_stmt_expr(tail)?;
                }
                Ok(())
            }
            IrExprKind::Call { target, args, .. } => {
                // Unit-position call: a value-returning callee's result is
                // dropped (a bare non-Unit call statement is legal IR).
                if let Some(ret) = self.lower_call(target, args)? {
                    let _ = ret;
                    self.f.instructions().drop();
                }
                Ok(())
            }
            // Unit-position `if`: both arms are statement bodies.
            IrExprKind::If { cond, then, else_ } => {
                self.expect(cond, SliceTy::Bool)?;
                self.f.instructions().if_(BlockType::Empty);
                self.lower_stmt_expr(then)?;
                self.f.instructions().else_();
                self.lower_stmt_expr(else_)?;
                self.f.instructions().end();
                Ok(())
            }
            // `while`: block { loop { !cond → br out; body; br loop } }.
            // Break/Continue would need label-depth tracking — they surface
            // as their own honest reasons until that lands.
            IrExprKind::While { cond, body } => {
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.expect(cond, SliceTy::Bool)?;
                self.f.instructions().i32_eqz().br_if(1);
                for s in body {
                    self.lower_stmt(s)?;
                }
                self.f.instructions().br(0).end().end();
                Ok(())
            }
            IrExprKind::Unit => Ok(()),
            other => unsup(&format!("expr:{}", expr_kind_name(other))),
        }
    }

    fn lower_stmt(&mut self, s: &IrStmt) -> Result<(), EmitError> {
        match &s.kind {
            IrStmtKind::Bind { var, ty, value, .. } => {
                let Some(&(idx, declared)) = self.locals.get(var) else {
                    return unsup("bind:unmapped");
                };
                debug_assert_eq!(slice_ty_of(ty), Some(declared));
                let got = self.lower_value(value)?;
                if got != declared {
                    return unsup(&format!("bind:ty-mismatch:{got:?}-vs-{declared:?}"));
                }
                self.f.instructions().local_set(idx);
                Ok(())
            }
            IrStmtKind::Assign { var, value } => {
                let Some(&(idx, declared)) = self.locals.get(var) else {
                    return unsup("assign:unmapped");
                };
                let got = self.lower_value(value)?;
                if got != declared {
                    return unsup(&format!("assign:ty-mismatch:{got:?}-vs-{declared:?}"));
                }
                self.f.instructions().local_set(idx);
                Ok(())
            }
            IrStmtKind::Expr { expr } => self.lower_stmt_expr(expr),
            IrStmtKind::Comment { .. } => Ok(()),
            other => unsup(&format!("stmt:{}", stmt_kind_name(other))),
        }
    }

    /// A call in any position. Returns the callee's slice return type
    /// (None = Unit). `println` is the one special form.
    fn lower_call(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        match target {
            CallTarget::Named { name } if name.as_str() == "println" && args.len() == 1 => {
                self.lower_print(&args[0], F_PRINTLN_IMPORT, F_PRINTLN_BLOCK)?;
                Ok(None)
            }
            CallTarget::Named { name } if name.as_str() == "eprintln" && args.len() == 1 => {
                self.lower_print(&args[0], F_EPRINTLN_IMPORT, F_EPRINTLN_BLOCK)?;
                Ok(None)
            }
            CallTarget::Named { name } => {
                let name = name.as_str();
                let Some(&i) = self.table.by_name.get(name) else {
                    return unsup(&format!("call:{name}"));
                };
                let info = &self.table.infos[i];
                if let Some(r) = &info.refuse {
                    return unsup(&format!("call-fn:{name}:{r}"));
                }
                if args.len() != info.params.len() {
                    return unsup(&format!("call-arity:{name}"));
                }
                let (index, ret, params) = (info.wasm_index, info.ret, info.params.clone());
                for (a, want) in args.iter().zip(params) {
                    self.expect(a, want)?;
                }
                self.calls.insert(name.to_string());
                self.f.instructions().call(index);
                Ok(ret)
            }
            // Stdlib special forms the runtime helpers cover directly.
            CallTarget::Module { module, func, .. }
                if module.as_str() == "int" && func.as_str() == "to_string" && args.len() == 1 =>
            {
                self.expect(&args[0], SliceTy::Int)?;
                self.f.instructions().call(F_INT_TO_STRING);
                Ok(Some(SliceTy::Str))
            }
            CallTarget::Module { module, func, .. } => {
                unsup(&format!("call:{}.{}", module.as_str(), func.as_str()))
            }
            _ => unsup("call:computed-or-method"),
        }
    }

    /// `println`/`eprintln`: interpolations build in the line buffer;
    /// everything else must lower to a String block and goes through the
    /// stream's block-print helper.
    fn lower_print(&mut self, arg: &IrExpr, import: u32, block_print: u32) -> Result<(), EmitError> {
        if let IrExprKind::StringInterp { parts } = &arg.kind {
            // cursor = line_start
            self.f.instructions().global_get(G_LINE_START).local_set(self.cursor_local);
            for part in parts {
                match part {
                    IrStringPart::Lit { value } => {
                        if value.is_empty() {
                            continue;
                        }
                        let base = self.pool.intern(value);
                        let len = value.len() as i32;
                        self.f
                            .instructions()
                            .local_get(self.cursor_local)
                            .i32_const((base + almide_layout::PAYLOAD) as i32)
                            .i32_const(len)
                            .call(F_APPEND_COPY)
                            .local_set(self.cursor_local);
                    }
                    IrStringPart::Expr { expr } => {
                        self.f.instructions().local_get(self.cursor_local);
                        match self.lower_value(expr)? {
                            SliceTy::Int => {
                                self.f
                                    .instructions()
                                    .call(F_APPEND_I64)
                                    .local_set(self.cursor_local);
                            }
                            SliceTy::Str => {
                                // stack: cur, base → cur, payload, len
                                self.f
                                    .instructions()
                                    .local_tee(self.tmp_i32_local)
                                    .i32_const(almide_layout::PAYLOAD as i32)
                                    .i32_add()
                                    .local_get(self.tmp_i32_local)
                                    .i32_load(len_memarg())
                                    .call(F_APPEND_COPY)
                                    .local_set(self.cursor_local);
                            }
                            SliceTy::Bool => {
                                self.f
                                    .instructions()
                                    .call(F_APPEND_BOOL)
                                    .local_set(self.cursor_local);
                            }
                        }
                    }
                }
            }
            // print(line_start, cursor - line_start)
            self.f
                .instructions()
                .global_get(G_LINE_START)
                .local_get(self.cursor_local)
                .global_get(G_LINE_START)
                .i32_sub()
                .call(import);
            return Ok(());
        }
        match self.lower_value(arg)? {
            SliceTy::Str => {
                self.f.instructions().call(block_print);
                Ok(())
            }
            other => unsup(&format!("println-arg-ty:{other:?}")),
        }
    }

    /// Value position: leaves exactly one value on the stack, returns its
    /// slice type.
    fn lower_value(&mut self, e: &IrExpr) -> Result<SliceTy, EmitError> {
        match &e.kind {
            IrExprKind::LitInt { value } => {
                self.f.instructions().i64_const(*value);
                Ok(SliceTy::Int)
            }
            IrExprKind::LitBool { value } => {
                self.f.instructions().i32_const(i32::from(*value));
                Ok(SliceTy::Bool)
            }
            IrExprKind::LitStr { value } => {
                let base = self.pool.intern(value);
                self.f.instructions().i32_const(base as i32);
                Ok(SliceTy::Str)
            }
            IrExprKind::Var { id } => {
                let Some(&(idx, ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                self.f.instructions().local_get(idx);
                Ok(ty)
            }
            IrExprKind::Call { target, args, .. } => match self.lower_call(target, args)? {
                Some(ty) => Ok(ty),
                None => unsup("call-unit-in-value"),
            },
            IrExprKind::UnOp { op, operand } => match op {
                UnOp::NegInt => {
                    self.f.instructions().i64_const(0);
                    self.expect(operand, SliceTy::Int)?;
                    self.f.instructions().i64_sub();
                    Ok(SliceTy::Int)
                }
                UnOp::Not => {
                    self.expect(operand, SliceTy::Bool)?;
                    self.f.instructions().i32_eqz();
                    Ok(SliceTy::Bool)
                }
                UnOp::NegFloat => unsup("unop:NegFloat"),
            },
            IrExprKind::BinOp { op, left, right } => self.lower_binop(*op, left, right),
            IrExprKind::Block { stmts, expr } => {
                for s in stmts {
                    self.lower_stmt(s)?;
                }
                let Some(tail) = expr else { return unsup("expr:Block-no-tail") };
                self.lower_value(tail)
            }
            // Value-position `if`: the arm type is inferred WITHOUT
            // emitting (wasm wants the block type up front), then both
            // arms are lowered against it.
            IrExprKind::If { cond, then, else_ } => {
                self.expect(cond, SliceTy::Bool)?;
                let ty = self.infer(then)?;
                self.f.instructions().if_(BlockType::Result(ty.val_type()));
                self.expect(then, ty)?;
                self.f.instructions().else_();
                self.expect(else_, ty)?;
                self.f.instructions().end();
                Ok(ty)
            }
            other => unsup(&format!("expr:{}", expr_kind_name(other))),
        }
    }

    /// Non-emitting slice-type inference — used where wasm needs a block
    /// type before the arm is lowered. Must agree with `lower_value`; a
    /// disagreement surfaces as a lowering unsup, never a bad module.
    fn infer(&self, e: &IrExpr) -> Result<SliceTy, EmitError> {
        match &e.kind {
            IrExprKind::LitInt { .. } => Ok(SliceTy::Int),
            IrExprKind::LitBool { .. } => Ok(SliceTy::Bool),
            IrExprKind::LitStr { .. } => Ok(SliceTy::Str),
            IrExprKind::Var { id } => match self.locals.get(id) {
                Some(&(_, ty)) => Ok(ty),
                None => unsup("var:unmapped"),
            },
            IrExprKind::UnOp { op, .. } => match op {
                UnOp::NegInt => Ok(SliceTy::Int),
                UnOp::Not => Ok(SliceTy::Bool),
                UnOp::NegFloat => unsup("unop:NegFloat"),
            },
            IrExprKind::BinOp { op, .. } => match op.result_ty().as_ref().and_then(slice_ty_of) {
                Some(ty) => Ok(ty),
                None => unsup(&format!("binop:{op:?}")),
            },
            IrExprKind::Call { target, .. } => match target {
                CallTarget::Named { name } => {
                    let name = name.as_str();
                    let Some(&i) = self.table.by_name.get(name) else {
                        return unsup(&format!("call:{name}"));
                    };
                    let info = &self.table.infos[i];
                    if let Some(r) = &info.refuse {
                        return unsup(&format!("call-fn:{name}:{r}"));
                    }
                    match info.ret {
                        Some(ty) => Ok(ty),
                        None => unsup("call-unit-in-value"),
                    }
                }
                CallTarget::Module { module, func, .. }
                    if module.as_str() == "int" && func.as_str() == "to_string" =>
                {
                    Ok(SliceTy::Str)
                }
                CallTarget::Module { module, func, .. } => {
                    unsup(&format!("call:{}.{}", module.as_str(), func.as_str()))
                }
                _ => unsup("call:computed-or-method"),
            },
            IrExprKind::If { then, .. } => self.infer(then),
            IrExprKind::Block { expr, .. } => match expr {
                Some(tail) => self.infer(tail),
                None => unsup("expr:Block-no-tail"),
            },
            other => unsup(&format!("expr:{}", expr_kind_name(other))),
        }
    }

    fn lower_binop(
        &mut self,
        op: BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        use BinOp::*;
        match op {
            AddInt | SubInt | MulInt | DivInt | ModInt => {
                self.expect(left, SliceTy::Int)?;
                self.expect(right, SliceTy::Int)?;
                let mut i = self.f.instructions();
                match op {
                    AddInt => i.i64_add(),
                    SubInt => i.i64_sub(),
                    MulInt => i.i64_mul(),
                    // wasm traps on /0 and MIN/-1 exactly where native
                    // aborts — abort-parity (exit + stderr) is a later
                    // slice, so those fixtures are gate-classified, not
                    // claimed.
                    DivInt => i.i64_div_s(),
                    ModInt => i.i64_rem_s(),
                    _ => unreachable!(),
                };
                Ok(SliceTy::Int)
            }
            Lt | Gt | Lte | Gte => {
                self.expect(left, SliceTy::Int)?;
                self.expect(right, SliceTy::Int)?;
                let mut i = self.f.instructions();
                match op {
                    Lt => i.i64_lt_s(),
                    Gt => i.i64_gt_s(),
                    Lte => i.i64_le_s(),
                    Gte => i.i64_ge_s(),
                    _ => unreachable!(),
                };
                Ok(SliceTy::Bool)
            }
            Eq | Neq => {
                let lt = self.lower_value(left)?;
                self.expect(right, lt)?;
                let mut i = self.f.instructions();
                match (lt, op) {
                    (SliceTy::Int, Eq) => i.i64_eq(),
                    (SliceTy::Int, Neq) => i.i64_ne(),
                    (SliceTy::Bool, Eq) => i.i32_eq(),
                    (SliceTy::Bool, Neq) => i.i32_ne(),
                    (SliceTy::Str, _) => return unsup("binop:eq-str"),
                    _ => unreachable!(),
                };
                Ok(SliceTy::Bool)
            }
            ConcatStr => {
                self.expect(left, SliceTy::Str)?;
                self.expect(right, SliceTy::Str)?;
                self.f.instructions().call(F_CONCAT);
                Ok(SliceTy::Str)
            }
            // SHORT-CIRCUIT: the right operand must not evaluate (and
            // possibly trap) when the left already decides — an `if`
            // yielding i32, never a strict bitop.
            And => {
                self.expect(left, SliceTy::Bool)?;
                self.f.instructions().if_(BlockType::Result(ValType::I32));
                self.expect(right, SliceTy::Bool)?;
                self.f.instructions().else_().i32_const(0).end();
                Ok(SliceTy::Bool)
            }
            Or => {
                self.expect(left, SliceTy::Bool)?;
                self.f.instructions().if_(BlockType::Result(ValType::I32)).i32_const(1).else_();
                self.expect(right, SliceTy::Bool)?;
                self.f.instructions().end();
                Ok(SliceTy::Bool)
            }
            other => unsup(&format!("binop:{other:?}")),
        }
    }

    /// Lower `e` and require its slice type.
    fn expect(&mut self, e: &IrExpr, want: SliceTy) -> Result<(), EmitError> {
        let got = self.lower_value(e)?;
        if got != want {
            return unsup(&format!("ty-mismatch:{got:?}-vs-{want:?}"));
        }
        Ok(())
    }
}

// ── reason-string helpers ───────────────────────────────────────────────

fn expr_kind_name(k: &IrExprKind) -> String {
    let dbg = format!("{k:?}");
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}

fn stmt_kind_name(k: &IrStmtKind) -> String {
    let dbg = format!("{k:?}");
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}

fn ty_name(t: &Ty) -> String {
    let dbg = format!("{t:?}");
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}
