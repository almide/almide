//! Unit 6 stage 3: typed IR → structural wasm emission — the scalar-sum
//! slice (stage 2's scalar programs + Option/Result + match).
//!
//! Constitution (ARCHITECTURE.md §3/§6.6): binary emission via wasm-encoder
//! only — no WAT text, no string templates; block layout AND sum layout
//! derive from `almide-layout`; every module is validated before acceptance
//! (the wall, in the gate).
//!
//! Value model:
//!   - `Int`/`Int64` → wasm `i64`; `Bool` → `i32` (0/1); `String` → `i32`
//!     holding the BLOCK BASE address (payload/len derive from the layout
//!     crate — never a bare payload pointer, so header reads stay honest).
//!   - `Option[scalar]` → `i32`: `none` IS the layout's NULL_ADDR, `some`
//!     is a block whose payload holds the value slot (`OPTION_FIELD`).
//!     Nested Option is unrepresentable by design and refused.
//!   - `Result[scalar, scalar]` → `i32`: a tagged block (`SUM_TAG` 0=Ok
//!     1=Err, value at `SUM_FIELD`) — the shape user variants will
//!     generalise.
//!   - let/var binds and assigns become wasm locals; user functions with
//!     slice-typed signatures become real wasm functions (params = leading
//!     locals, direct calls, recursion free). A body that doesn't lower
//!     yet gets an `unreachable` stub; emission REFUSES the program iff
//!     such a stub is reachable from `main` (call-graph BFS).
//!   - `match` compiles to an if/else chain over pattern tests; arm-bind
//!     patterns load from the subject scratch local. Exhaustiveness is the
//!     checker's promise — the final arm still carries its test, with a
//!     LOUD `unreachable` if it ever fails. Guards are refused (own
//!     reason) until label-depth-free lowering for them lands.
//!   - `!` lowers to the ABORT form only (null/Err → trap): in a pure fn
//!     whose return is Option/Result, `!` PROPAGATES on the oracle
//!     (#1410 family), so those bodies are refused, never mis-lowered.
//!   - `and`/`or` SHORT-CIRCUIT via `if` blocks; the emitted-wasm itoa
//!     works in the NEGATIVE domain so `i64::MIN` never overflows;
//!     value-`if`/`match` result types are inferred before emission.
//!   - top-level lets lower as `main`'s eager prelude (observably
//!     identical while `main` is the only entry and cross-function global
//!     reads are refused).
//!
//! Type hints flow DOWN through `lower(e, want)`: `none` / `ok(x)` /
//! `err(x)` have no self-contained type, so binds, args, returns and match
//! arms pass their expectation into the expression.
//!
//! Memory map: `[0,12)` null guard · `[16,48)` itoa scratch · `[48,…)` the
//! literal pool · line buffer from `align16(pool_end)` (global 0) ·
//! bump-allocator heap after it (mutable global 1, `$alloc` grows memory,
//! OOM traps loud; blocks are never freed — sound for run-to-completion
//! programs).

use std::collections::{HashMap, HashSet};

use almide_ir::{
    BinOp, CallTarget, IrExpr, IrExprKind, IrFunction, IrMatchArm, IrPattern, IrProgram, IrStmt,
    IrStmtKind, IrStringPart, IrTopLet, UnOp, VarId,
};
use almide_types::types::{Ty, TypeConstructorId};
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
const F_STR_EQ: u32 = 11;
const F_LIST_GET_8: u32 = 12;
const F_LIST_GET_4: u32 = 13;
const F_LIST_PUSH_8: u32 = 14;
const F_LIST_PUSH_4: u32 = 15;
const F_LIST_JOIN: u32 = 16;
const F_BLOCK_COPY: u32 = 17;
/// First program-function index; `main` sits after every program function.
const F_FN_BASE: u32 = 18;
/// Fixed type indices: 0 print(ptr,len)→(), 1 block-print(i32)→(),
/// 2 append_copy, 3 append_i64, 4 main ()→(), 5 (i32,i32)→i32
/// (append_bool/concat/str_eq), 6 (i64)→i32 (itoa/int_to_string),
/// 7 (i32)→i32 (alloc); program-function types start after.
const T_MAIN: u32 = 4;
/// (i32,i64)→i32: list_get_8 / list_push_8.
const T_FN_BASE: u32 = 9;
/// Immutable i32 global: the line-buffer start (= align16(pool end)).
const G_LINE_START: u32 = 0;
/// Mutable i32 global: the bump-allocator head.
const G_HEAP: u32 = 1;

// ── slice value model ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scalar {
    /// Almide Int/Int64 — wasm i64.
    Int,
    /// Almide Bool — wasm i32, 0/1.
    Bool,
    /// Almide String — wasm i32 holding the block BASE address.
    Str,
}

impl Scalar {
    fn val_type(self) -> ValType {
        match self {
            Scalar::Int => ValType::I64,
            Scalar::Bool | Scalar::Str => ValType::I32,
        }
    }

    /// Byte width of this scalar's slot inside a sum block.
    fn slot_size(self) -> u32 {
        match self {
            Scalar::Int => 8,
            Scalar::Bool | Scalar::Str => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SliceTy {
    Scalar(Scalar),
    /// `Option[scalar]` — i32, NULL_ADDR = none.
    Option(Scalar),
    /// `Result[ok, err]` — i32 tagged block.
    Result(Scalar, Scalar),
    /// `List[scalar]` — i32 block; payload = the element array, block len
    /// = count × stride (stride = the scalar's slot size). No COW is
    /// needed because in-place mutation (IndexAssign) is refused — every
    /// list op yields a fresh block, so sharing is unobservable.
    List(Scalar),
}

const STR: SliceTy = SliceTy::Scalar(Scalar::Str);
const INT: SliceTy = SliceTy::Scalar(Scalar::Int);
const BOOL: SliceTy = SliceTy::Scalar(Scalar::Bool);

impl SliceTy {
    fn val_type(self) -> ValType {
        match self {
            SliceTy::Scalar(s) => s.val_type(),
            SliceTy::Option(_) | SliceTy::Result(..) | SliceTy::List(_) => ValType::I32,
        }
    }
}

fn scalar_of(ty: &Ty) -> Option<Scalar> {
    match ty {
        Ty::Int | Ty::Int64 => Some(Scalar::Int),
        Ty::Bool => Some(Scalar::Bool),
        Ty::String => Some(Scalar::Str),
        _ => None,
    }
}

fn slice_ty_of(ty: &Ty) -> Option<SliceTy> {
    if let Some(s) = scalar_of(ty) {
        return Some(SliceTy::Scalar(s));
    }
    match ty {
        Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => {
            scalar_of(&args[0]).map(SliceTy::Option)
        }
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
            match (scalar_of(&args[0]), scalar_of(&args[1])) {
                (Some(o), Some(e)) => Some(SliceTy::Result(o, e)),
                _ => None,
            }
        }
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            scalar_of(&args[0]).map(SliceTy::List)
        }
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

/// Payload-relative slot address as an absolute-from-base MemArg. All
/// block bases and slots are 4-aligned by the allocator, so align hint 2.
fn slot_memarg(payload_relative: u32) -> MemArg {
    MemArg {
        offset: u64::from(almide_layout::PAYLOAD + payload_relative),
        align: 2,
        memory_index: 0,
    }
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
    let (main_fn, main_calls) = lower_fn(&[], None, &main.body, &ir.top_lets, &table, &mut pool)?;

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
    types.ty().function([ValType::I32, ValType::I32], [ValType::I32]); // 5: append_bool / concat / str_eq
    types.ty().function([ValType::I64], [ValType::I32]); // 6: itoa / int_to_string
    types.ty().function([ValType::I32], [ValType::I32]); // 7: alloc
    types.ty().function([ValType::I32, ValType::I64], [ValType::I32]); // 8: list_get/push (8-byte)
    for (i, info) in table.infos.iter().enumerate() {
        // Refused functions keep a placeholder type — their stub body is
        // `unreachable` and no call site ever targets them.
        debug_assert_eq!(T_FN_BASE as usize + i, 9 + i);
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
    functions.function(5); // F_STR_EQ
    functions.function(8); // F_LIST_GET_8
    functions.function(8); // F_LIST_GET_4 (idx is ALWAYS i64 — only the slot width differs)
    functions.function(8); // F_LIST_PUSH_8
    functions.function(5); // F_LIST_PUSH_4
    functions.function(5); // F_LIST_JOIN
    functions.function(7); // F_BLOCK_COPY
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
    code.function(&emit_str_eq());
    code.function(&emit_list_get(Scalar::Int));
    code.function(&emit_list_get(Scalar::Str));
    code.function(&emit_list_push(Scalar::Int));
    code.function(&emit_list_push(Scalar::Str));
    code.function(&emit_list_join());
    code.function(&emit_block_copy());
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
/// scratch locals (interp cursor, tmp i32, match/unwrap subjects).
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
    let base = (params.len() + binds.len()) as u32;
    let (cursor_local, tmp_i32_local, scr_i32_local, scr_i64_local) =
        (base, base + 1, base + 2, base + 3);
    local_decls.push((3, ValType::I32)); // cursor, tmp, scr_i32
    local_decls.push((1, ValType::I64)); // scr_i64
    // Hold pools: stack-disciplined scratch for constructs that must keep
    // an address/counter live ACROSS sub-expression lowering (list
    // literals, index bases, for-in state). Depth beyond the pool is an
    // honest unsup, never a corruption.
    let hold_i32_base = base + 4;
    let hold_i64_base = base + 4 + HOLD_I32_POOL;
    local_decls.push((HOLD_I32_POOL, ValType::I32));
    local_decls.push((HOLD_I64_POOL, ValType::I64));

    let mut f = Function::new(local_decls);
    let mut calls: HashSet<String> = HashSet::new();
    {
        let mut em = Emitter {
            pool,
            locals: &locals,
            table,
            calls: &mut calls,
            fn_ret: ret,
            cursor_local,
            tmp_i32_local,
            scr_i32_local,
            scr_i64_local,
            hold_i32_base,
            hold_i32_depth: 0,
            hold_i64_base,
            hold_i64_depth: 0,
            f: &mut f,
        };
        for tl in top_lets {
            let (idx, declared) = em.locals[&tl.var];
            em.lower(&tl.value, Some(declared))?;
            if matches!(declared, SliceTy::List(_)) {
                em.f.instructions().call(F_BLOCK_COPY);
            }
            em.f.instructions().local_set(idx);
        }
        match ret {
            None => em.lower_stmt_expr(body)?,
            Some(want) => {
                em.lower(body, Some(want))?;
            }
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

/// `$str_eq(a: i32, b: i32) -> i32`: byte equality of two blocks.
fn emit_str_eq() -> Function {
    // params: 0=a i32, 1=b i32; locals: 2=la i32, 3=i i32
    let (a, b, la, idx) = (0u32, 1u32, 2u32, 3u32);
    let byte = MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 };
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(a).i32_load(len_memarg()).local_set(la);
    i.local_get(la).local_get(b).i32_load(len_memarg()).i32_ne().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.i32_const(0).local_set(idx);
    i.loop_(BlockType::Empty);
    i.local_get(idx).local_get(la).i32_ge_u().if_(BlockType::Empty);
    i.i32_const(1).return_();
    i.end();
    i.local_get(a).local_get(idx).i32_add().i32_load8_u(byte);
    i.local_get(b).local_get(idx).i32_add().i32_load8_u(byte);
    i.i32_ne().if_(BlockType::Empty);
    i.i32_const(0).return_();
    i.end();
    i.local_get(idx).i32_const(1).i32_add().local_set(idx);
    i.br(0);
    i.end();
    i.unreachable();
    i.end();
    f
}

/// `$list_get_{8,4}(list: i32, idx: i64) -> i32`: `list.get` — a fresh
/// `some` block holding element `idx`, or NULL_ADDR when out of bounds.
fn emit_list_get(s: Scalar) -> Function {
    // params: 0=list i32, 1=idx i64; locals: 2=base i32
    let (list, idx, base) = (0u32, 1u32, 2u32);
    let stride = s.slot_size();
    let mut f = Function::new([(1, ValType::I32)]);
    let mut i = f.instructions();
    // out of bounds (idx < 0 or idx >= count) → none
    i.local_get(idx).i64_const(0).i64_lt_s();
    i.local_get(idx);
    i.local_get(list).i32_load(len_memarg()).i32_const(stride as i32).i32_div_u();
    i.i64_extend_i32_u().i64_ge_s();
    i.i32_or().if_(BlockType::Empty);
    i.i32_const(almide_layout::NULL_ADDR as i32).return_();
    i.end();
    // some(element)
    i.i32_const(stride as i32).call(F_ALLOC).local_set(base);
    i.local_get(base);
    i.local_get(list).i64_extend_i32_u().local_get(idx).i64_const(i64::from(stride)).i64_mul().i64_add().i32_wrap_i64();
    match s {
        Scalar::Int => i.i64_load(slot_memarg(almide_layout::OPTION_FIELD)),
        _ => i.i32_load(slot_memarg(almide_layout::OPTION_FIELD)),
    };
    match s {
        Scalar::Int => i.i64_store(slot_memarg(almide_layout::OPTION_FIELD)),
        _ => i.i32_store(slot_memarg(almide_layout::OPTION_FIELD)),
    };
    i.local_get(base);
    i.end();
    f
}

/// `$list_push_{8,4}(list: i32, v) -> i32`: append with AMORTIZED growth.
/// In-place when `cap - len >= stride` (sound because every List bind/
/// assign deep-copies — a local's block is uniquely its own, and the
/// checker only lets `push` target mut vars); otherwise a fresh block
/// with doubled capacity. Returns the (possibly new) base for write-back.
fn emit_list_push(s: Scalar) -> Function {
    // params: 0=list i32, 1=v; locals: 2=la i32, 3=cap i32, 4=base i32
    let (list, v, la, cap, base) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let stride = s.slot_size();
    let payload = almide_layout::PAYLOAD as i32;
    let word = |offset: u32| MemArg { offset: u64::from(offset), align: 2, memory_index: 0 };
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(list).i32_load(len_memarg()).local_set(la);
    i.local_get(list).i32_load(word(almide_layout::CAP.offset)).local_set(cap);
    // fast path: room in cap → store at len, bump len, return same block
    i.local_get(cap).local_get(la).i32_sub().i32_const(stride as i32).i32_ge_u().if_(BlockType::Empty);
    i.local_get(list).local_get(la).i32_add();
    i.local_get(v);
    match s {
        Scalar::Int => i.i64_store(slot_memarg(0)),
        _ => i.i32_store(slot_memarg(0)),
    };
    i.local_get(list).local_get(la).i32_const(stride as i32).i32_add().i32_store(word(almide_layout::LEN.offset));
    i.local_get(list).return_();
    i.end();
    // grow: newcap = max(cap * 2, 4 * stride)
    i.local_get(cap).i32_const(1).i32_shl().local_set(cap);
    i.local_get(cap).i32_const((4 * stride) as i32).i32_lt_u().if_(BlockType::Empty);
    i.i32_const((4 * stride) as i32).local_set(cap);
    i.end();
    i.local_get(cap).call(F_ALLOC).local_set(base); // len = cap = newcap for now
    i.local_get(base).i32_const(payload).i32_add();
    i.local_get(list).i32_const(payload).i32_add();
    i.local_get(la);
    i.memory_copy(0, 0);
    i.local_get(base).local_get(la).i32_add();
    i.local_get(v);
    match s {
        Scalar::Int => i.i64_store(slot_memarg(0)),
        _ => i.i32_store(slot_memarg(0)),
    };
    // live len = old len + stride (cap field keeps newcap from $alloc)
    i.local_get(base).local_get(la).i32_const(stride as i32).i32_add().i32_store(word(almide_layout::LEN.offset));
    i.local_get(base);
    i.end();
    f
}

/// `$block_copy(src: i32) -> i32`: a fresh block with src's live bytes —
/// the deep copy behind List value semantics at every bind/assign.
fn emit_block_copy() -> Function {
    // params: 0=src i32; locals: 1=len i32, 2=base i32
    let (src, len, base) = (0u32, 1u32, 2u32);
    let payload = almide_layout::PAYLOAD as i32;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(src).i32_load(len_memarg()).local_set(len);
    i.local_get(len).call(F_ALLOC).local_set(base);
    i.local_get(base).i32_const(payload).i32_add();
    i.local_get(src).i32_const(payload).i32_add();
    i.local_get(len);
    i.memory_copy(0, 0);
    i.local_get(base);
    i.end();
    f
}

/// `$list_join(list: i32, sep: i32) -> i32`: join a List[String]'s blocks
/// with `sep` — repeated `$concat` (quadratic, fine for fixture scale).
fn emit_list_join() -> Function {
    // params: 0=list i32, 1=sep i32; locals: 2=n i32, 3=i i32, 4=acc i32
    let (list, sep, n, idx, acc) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(list).i32_load(len_memarg()).i32_const(4).i32_div_u().local_set(n);
    i.i32_const(0).call(F_ALLOC).local_set(acc); // ""
    i.i32_const(0).local_set(idx);
    i.loop_(BlockType::Empty);
    i.local_get(idx).local_get(n).i32_ge_u().if_(BlockType::Empty);
    i.local_get(acc).return_();
    i.end();
    i.local_get(idx).i32_const(0).i32_ne().if_(BlockType::Empty);
    i.local_get(acc).local_get(sep).call(F_CONCAT).local_set(acc);
    i.end();
    i.local_get(acc);
    i.local_get(list).local_get(idx).i32_const(4).i32_mul().i32_add().i32_load(slot_memarg(0));
    i.call(F_CONCAT).local_set(acc);
    i.local_get(idx).i32_const(1).i32_add().local_set(idx);
    i.br(0);
    i.end();
    i.unreachable();
    i.end();
    f
}

// ── pre-pass: Binds → locals ────────────────────────────────────────────

/// Collect every Bind the lowering traversal can reach, in first-bind
/// order — statement binds AND match-pattern binds. Mirrors `Emitter`'s
/// traversal: a Bind the lowering CAN reach but this pass misses would
/// surface as the honest `bind:unmapped` reason, never a bad module.
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
        IrExprKind::ForIn { var, iterable, body, .. } => {
            // The loop variable is a local; its type comes from the
            // iterable's checker annotation (Range iterates Int).
            let var_ty = if matches!(iterable.kind, IrExprKind::Range { .. }) {
                Some(INT)
            } else {
                match slice_ty_of(&iterable.ty) {
                    Some(SliceTy::List(s)) => Some(SliceTy::Scalar(s)),
                    _ => None,
                }
            };
            let Some(var_ty) = var_ty else {
                return unsup(&format!("forin-iter-ty:{}", ty_name(&iterable.ty)));
            };
            if seen.insert(*var) {
                out.push((*var, var_ty));
            }
            collect_binds(iterable, out, seen)?;
            for s in body {
                collect_binds_stmt(s, out, seen)?;
            }
            Ok(())
        }
        IrExprKind::List { elements } => {
            for el in elements {
                collect_binds(el, out, seen)?;
            }
            Ok(())
        }
        IrExprKind::IndexAccess { object, index } => {
            collect_binds(object, out, seen)?;
            collect_binds(index, out, seen)
        }
        IrExprKind::Range { start, end, .. } => {
            collect_binds(start, out, seen)?;
            collect_binds(end, out, seen)
        }
        IrExprKind::Match { subject, arms } => {
            collect_binds(subject, out, seen)?;
            for arm in arms {
                collect_pattern_binds(&arm.pattern, out, seen)?;
                if let Some(g) = &arm.guard {
                    collect_binds(g, out, seen)?;
                }
                collect_binds(&arm.body, out, seen)?;
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
        IrExprKind::OptionSome { expr }
        | IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr }
        | IrExprKind::Unwrap { expr } => collect_binds(expr, out, seen),
        IrExprKind::UnwrapOr { expr, fallback } => {
            collect_binds(expr, out, seen)?;
            collect_binds(fallback, out, seen)
        }
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

fn collect_pattern_binds(
    p: &IrPattern,
    out: &mut Vec<(VarId, SliceTy)>,
    seen: &mut HashSet<VarId>,
) -> Result<(), EmitError> {
    match p {
        IrPattern::Bind { var, ty } => {
            let Some(sty) = slice_ty_of(ty) else {
                return unsup(&format!("bind-ty:{}", ty_name(ty)));
            };
            if seen.insert(*var) {
                out.push((*var, sty));
            }
            Ok(())
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            collect_pattern_binds(inner, out, seen)
        }
        _ => Ok(()), // lowering unsups unsupported pattern shapes first
    }
}

// ── body lowering ───────────────────────────────────────────────────────

struct Emitter<'a> {
    pool: &'a mut Pool,
    locals: &'a HashMap<VarId, (u32, SliceTy)>,
    table: &'a FnTable,
    calls: &'a mut HashSet<String>,
    /// The containing function's return slice type — `!` PROPAGATES (not
    /// aborts) in pure fns returning Option/Result, so those are refused.
    fn_ret: Option<SliceTy>,
    cursor_local: u32,
    tmp_i32_local: u32,
    /// Match/unwrap subject scratch. Shared across nesting levels — safe
    /// because a subject is only read during its own tests, which finish
    /// before any nested match/unwrap in a SELECTED arm's body runs.
    scr_i32_local: u32,
    scr_i64_local: u32,
    hold_i32_base: u32,
    hold_i32_depth: u32,
    hold_i64_base: u32,
    hold_i64_depth: u32,
    f: &'a mut Function,
}

/// Hold-pool sizes: nesting deeper than this is refused, never corrupted.
const HOLD_I32_POOL: u32 = 8;
const HOLD_I64_POOL: u32 = 4;

impl Emitter<'_> {
    fn hold_i32(&mut self) -> Result<u32, EmitError> {
        if self.hold_i32_depth >= HOLD_I32_POOL {
            return unsup("hold-depth-i32");
        }
        let idx = self.hold_i32_base + self.hold_i32_depth;
        self.hold_i32_depth += 1;
        Ok(idx)
    }

    fn release_i32(&mut self) {
        self.hold_i32_depth -= 1;
    }

    fn hold_i64(&mut self) -> Result<u32, EmitError> {
        if self.hold_i64_depth >= HOLD_I64_POOL {
            return unsup("hold-depth-i64");
        }
        let idx = self.hold_i64_base + self.hold_i64_depth;
        self.hold_i64_depth += 1;
        Ok(idx)
    }

    fn release_i64(&mut self) {
        self.hold_i64_depth -= 1;
    }

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
                if self.lower_call(target, args)?.is_some() {
                    self.f.instructions().drop();
                }
                Ok(())
            }
            // Unit-position `if`: both arms are statement bodies.
            IrExprKind::If { cond, then, else_ } => {
                self.lower(cond, Some(BOOL))?;
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
                self.lower(cond, Some(BOOL))?;
                self.f.instructions().i32_eqz().br_if(1);
                for s in body {
                    self.lower_stmt(s)?;
                }
                self.f.instructions().br(0).end().end();
                Ok(())
            }
            IrExprKind::Match { subject, arms } => self.lower_match(subject, arms, None).map(|_| ()),
            // for x in <list> / for i in a..b
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                if var_tuple.is_some() {
                    return unsup("forin-tuple");
                }
                let Some(&(var_idx, var_ty)) = self.locals.get(var) else {
                    return unsup("bind:unmapped");
                };
                if let IrExprKind::Range { start, end, inclusive } = &iterable.kind {
                    // Range: var runs start..end directly, no list at all.
                    if var_ty != INT {
                        return unsup("forin-range-nonint");
                    }
                    self.lower(start, Some(INT))?;
                    self.f.instructions().local_set(var_idx);
                    self.lower(end, Some(INT))?;
                    let stop = self.hold_i64()?;
                    self.f.instructions().local_set(stop);
                    self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                    self.f.instructions().local_get(var_idx).local_get(stop);
                    if *inclusive {
                        self.f.instructions().i64_gt_s();
                    } else {
                        self.f.instructions().i64_ge_s();
                    }
                    self.f.instructions().br_if(1);
                    for st in body {
                        self.lower_stmt(st)?;
                    }
                    self.f
                        .instructions()
                        .local_get(var_idx)
                        .i64_const(1)
                        .i64_add()
                        .local_set(var_idx)
                        .br(0)
                        .end()
                        .end();
                    self.release_i64();
                    return Ok(());
                }
                let elem = match self.lower(iterable, None)? {
                    SliceTy::List(s) => s,
                    other => return unsup(&format!("forin-iter:{other:?}")),
                };
                if var_ty != SliceTy::Scalar(elem) {
                    return unsup("forin-var-ty");
                }
                let stride = elem.slot_size();
                let base = self.hold_i32()?;
                let count = self.hold_i32()?;
                let cur = self.hold_i32()?;
                self.f.instructions().local_set(base);
                self.f
                    .instructions()
                    .local_get(base)
                    .i32_load(len_memarg())
                    .i32_const(stride as i32)
                    .i32_div_u()
                    .local_set(count)
                    .i32_const(0)
                    .local_set(cur);
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.f.instructions().local_get(cur).local_get(count).i32_ge_u().br_if(1);
                self.f
                    .instructions()
                    .local_get(base)
                    .local_get(cur)
                    .i32_const(stride as i32)
                    .i32_mul()
                    .i32_add();
                self.load_slot(elem, 0);
                self.f.instructions().local_set(var_idx);
                for st in body {
                    self.lower_stmt(st)?;
                }
                self.f
                    .instructions()
                    .local_get(cur)
                    .i32_const(1)
                    .i32_add()
                    .local_set(cur)
                    .br(0)
                    .end()
                    .end();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(())
            }
            IrExprKind::Unit => Ok(()),
            other => unsup(&format!("expr:{}", expr_kind_name(other))),
        }
    }

    fn lower_stmt(&mut self, s: &IrStmt) -> Result<(), EmitError> {
        match &s.kind {
            IrStmtKind::Bind { var, value, .. } => {
                let Some(&(idx, declared)) = self.locals.get(var) else {
                    return unsup("bind:unmapped");
                };
                self.lower(value, Some(declared))?;
                // List value semantics: every bind owns a fresh block, so
                // in-place push growth can never be observed via aliases.
                if matches!(declared, SliceTy::List(_)) {
                    self.f.instructions().call(F_BLOCK_COPY);
                }
                self.f.instructions().local_set(idx);
                Ok(())
            }
            IrStmtKind::Assign { var, value } => {
                let Some(&(idx, declared)) = self.locals.get(var) else {
                    return unsup("assign:unmapped");
                };
                self.lower(value, Some(declared))?;
                if matches!(declared, SliceTy::List(_)) {
                    self.f.instructions().call(F_BLOCK_COPY);
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
    /// (None = Unit). `println`/`eprintln` are the special forms.
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
                    self.lower(a, Some(want))?;
                }
                self.calls.insert(name.to_string());
                self.f.instructions().call(index);
                Ok(ret)
            }
            // Stdlib special forms the runtime helpers cover directly.
            CallTarget::Module { module, func, .. }
                if module.as_str() == "int" && func.as_str() == "to_string" && args.len() == 1 =>
            {
                self.lower(&args[0], Some(INT))?;
                self.f.instructions().call(F_INT_TO_STRING);
                Ok(Some(STR))
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "list" => {
                self.lower_list_call(func.as_str(), args)
            }
            CallTarget::Module { module, func, .. } => {
                unsup(&format!("call:{}.{}", module.as_str(), func.as_str()))
            }
            _ => unsup("call:computed-or-method"),
        }
    }

    /// `list.*` special forms over the runtime helpers.
    fn lower_list_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
            ("len", [xs]) => {
                let elem = match self.lower(xs, None)? {
                    SliceTy::List(s) => s,
                    other => return unsup(&format!("list-len-of:{other:?}")),
                };
                self.f
                    .instructions()
                    .i32_load(len_memarg())
                    .i32_const(elem.slot_size() as i32)
                    .i32_div_u()
                    .i64_extend_i32_u();
                Ok(Some(INT))
            }
            ("get", [xs, idx]) => {
                let elem = match self.lower(xs, None)? {
                    SliceTy::List(s) => s,
                    other => return unsup(&format!("list-get-of:{other:?}")),
                };
                self.lower(idx, Some(INT))?;
                let helper = match elem.slot_size() {
                    8 => F_LIST_GET_8,
                    _ => F_LIST_GET_4,
                };
                self.f.instructions().call(helper);
                Ok(Some(SliceTy::Option(elem)))
            }
            ("get_or", [xs, idx, default]) => {
                // (xs.get(idx)) ?? default, inlined via the get helper.
                let elem = match self.lower(xs, None)? {
                    SliceTy::List(s) => s,
                    other => return unsup(&format!("list-get-of:{other:?}")),
                };
                self.lower(idx, Some(INT))?;
                let helper = match elem.slot_size() {
                    8 => F_LIST_GET_8,
                    _ => F_LIST_GET_4,
                };
                self.f
                    .instructions()
                    .call(helper)
                    .local_tee(self.scr_i32_local)
                    .i32_eqz()
                    .if_(BlockType::Result(elem.val_type()));
                self.lower(default, Some(SliceTy::Scalar(elem)))?;
                self.f.instructions().else_().local_get(self.scr_i32_local);
                self.load_slot(elem, almide_layout::OPTION_FIELD);
                self.f.instructions().end();
                Ok(Some(SliceTy::Scalar(elem)))
            }
            // `list.push` MUTATES through its `mut` param on the oracle
            // (the growth fixture pushes as bare statements). Lowered as a
            // write-back: var = $push(var, v). Requires a plain var arg.
            ("push", [xs, v]) => {
                let IrExprKind::Var { id } = &xs.kind else {
                    return unsup("list-push-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                let SliceTy::List(elem) = var_ty else {
                    return unsup(&format!("list-push-of:{var_ty:?}"));
                };
                self.f.instructions().local_get(var_idx);
                self.lower(v, Some(SliceTy::Scalar(elem)))?;
                let helper = match elem.slot_size() {
                    8 => F_LIST_PUSH_8,
                    _ => F_LIST_PUSH_4,
                };
                self.f.instructions().call(helper).local_set(var_idx);
                Ok(None)
            }
            ("join", [xs, sep]) => {
                match self.lower(xs, None)? {
                    SliceTy::List(Scalar::Str) => {}
                    other => return unsup(&format!("list-join-of:{other:?}")),
                }
                self.lower(sep, Some(STR))?;
                self.f.instructions().call(F_LIST_JOIN);
                Ok(Some(STR))
            }
            _ => unsup(&format!("call:list.{func}")),
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
                        match self.lower(expr, None)? {
                            INT => {
                                self.f
                                    .instructions()
                                    .call(F_APPEND_I64)
                                    .local_set(self.cursor_local);
                            }
                            STR => {
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
                            BOOL => {
                                self.f
                                    .instructions()
                                    .call(F_APPEND_BOOL)
                                    .local_set(self.cursor_local);
                            }
                            other => return unsup(&format!("interp-part:{other:?}")),
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
        self.lower(arg, Some(STR))?;
        self.f.instructions().call(block_print);
        Ok(())
    }

    /// Value position: leaves exactly one value on the stack. `want` is
    /// the downward type hint — REQUIRED by `none`/`ok`/`err` (which have
    /// no self-contained type) and verified against everything else.
    fn lower(&mut self, e: &IrExpr, want: Option<SliceTy>) -> Result<SliceTy, EmitError> {
        let got = match &e.kind {
            IrExprKind::LitInt { value } => {
                self.f.instructions().i64_const(*value);
                INT
            }
            IrExprKind::LitBool { value } => {
                self.f.instructions().i32_const(i32::from(*value));
                BOOL
            }
            IrExprKind::LitStr { value } => {
                let base = self.pool.intern(value);
                self.f.instructions().i32_const(base as i32);
                STR
            }
            IrExprKind::Var { id } => {
                let Some(&(idx, ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                self.f.instructions().local_get(idx);
                ty
            }
            IrExprKind::Call { target, args, .. } => match self.lower_call(target, args)? {
                Some(ty) => ty,
                None => return unsup("call-unit-in-value"),
            },
            IrExprKind::UnOp { op, operand } => match op {
                UnOp::NegInt => {
                    self.f.instructions().i64_const(0);
                    self.lower(operand, Some(INT))?;
                    self.f.instructions().i64_sub();
                    INT
                }
                UnOp::Not => {
                    self.lower(operand, Some(BOOL))?;
                    self.f.instructions().i32_eqz();
                    BOOL
                }
                UnOp::NegFloat => return unsup("unop:NegFloat"),
            },
            IrExprKind::BinOp { op, left, right } => self.lower_binop(*op, left, right)?,
            IrExprKind::Block { stmts, expr } => {
                for s in stmts {
                    self.lower_stmt(s)?;
                }
                let Some(tail) = expr else { return unsup("expr:Block-no-tail") };
                self.lower(tail, want)?
            }
            // Value-position `if`: the arm type comes from the hint or is
            // inferred WITHOUT emitting (wasm wants the block type up
            // front), then both arms are lowered against it.
            IrExprKind::If { cond, then, else_ } => {
                self.lower(cond, Some(BOOL))?;
                let ty = match want {
                    Some(w) => w,
                    None => self.infer(e)?,
                };
                self.f.instructions().if_(BlockType::Result(ty.val_type()));
                self.lower(then, Some(ty))?;
                self.f.instructions().else_();
                self.lower(else_, Some(ty))?;
                self.f.instructions().end();
                ty
            }
            IrExprKind::Match { subject, arms } => {
                let ty = match want {
                    Some(w) => w,
                    None => self.infer(e)?,
                };
                self.lower_match(subject, arms, Some(ty))?;
                ty
            }
            // Sum constructors — `none`/`ok`/`err` REQUIRE the hint.
            IrExprKind::OptionNone => match want.map_or_else(|| self.infer(e), Ok)? {
                SliceTy::Option(s) => {
                    self.f.instructions().i32_const(almide_layout::NULL_ADDR as i32);
                    SliceTy::Option(s)
                }
                other => return unsup(&format!("ty-mismatch:none-vs-{other:?}")),
            },
            IrExprKind::OptionSome { expr } => {
                let s = match want.map_or_else(|| self.infer(e), Ok)? {
                    SliceTy::Option(s) => s,
                    other => return unsup(&format!("ty-mismatch:some-vs-{other:?}")),
                };
                // tmp cannot be clobbered by the inner lowering: nested
                // allocating constructors would need Option[Option]/
                // Result-in-sum types, which slice_ty_of refuses.
                self.f
                    .instructions()
                    .i32_const(s.slot_size() as i32)
                    .call(F_ALLOC)
                    .local_tee(self.tmp_i32_local);
                self.lower(expr, Some(SliceTy::Scalar(s)))?;
                self.store_slot(s, almide_layout::OPTION_FIELD);
                self.f.instructions().local_get(self.tmp_i32_local);
                SliceTy::Option(s)
            }
            IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } => {
                let is_ok = matches!(&e.kind, IrExprKind::ResultOk { .. });
                let (o, er) = match want.map_or_else(|| self.infer(e), Ok)? {
                    SliceTy::Result(o, er) => (o, er),
                    other => return unsup(&format!("ty-mismatch:result-vs-{other:?}")),
                };
                let side = if is_ok { o } else { er };
                self.f
                    .instructions()
                    .i32_const(16)
                    .call(F_ALLOC)
                    .local_tee(self.tmp_i32_local)
                    .i32_const(i32::from(!is_ok))
                    .i32_store(slot_memarg(almide_layout::SUM_TAG));
                self.f.instructions().local_get(self.tmp_i32_local);
                self.lower(expr, Some(SliceTy::Scalar(side)))?;
                self.store_slot(side, almide_layout::SUM_FIELD);
                self.f.instructions().local_get(self.tmp_i32_local);
                SliceTy::Result(o, er)
            }
            // `!` — ABORT form only. In a pure fn returning Option/Result
            // the oracle PROPAGATES instead (#1410 family): refuse those.
            IrExprKind::Unwrap { expr } => {
                if matches!(self.fn_ret, Some(SliceTy::Option(_) | SliceTy::Result(..))) {
                    return unsup("unwrap-propagating");
                }
                match self.lower(expr, None)? {
                    SliceTy::Option(s) => {
                        self.f
                            .instructions()
                            .local_tee(self.scr_i32_local)
                            .i32_eqz()
                            .if_(BlockType::Empty)
                            .unreachable()
                            .end()
                            .local_get(self.scr_i32_local);
                        self.load_slot(s, almide_layout::OPTION_FIELD);
                        SliceTy::Scalar(s)
                    }
                    SliceTy::Result(o, _) => {
                        self.f
                            .instructions()
                            .local_tee(self.scr_i32_local)
                            .i32_load(slot_memarg(almide_layout::SUM_TAG))
                            .i32_const(0)
                            .i32_ne()
                            .if_(BlockType::Empty)
                            .unreachable()
                            .end()
                            .local_get(self.scr_i32_local);
                        self.load_slot(o, almide_layout::SUM_FIELD);
                        SliceTy::Scalar(o)
                    }
                    other => return unsup(&format!("unwrap-of:{other:?}")),
                }
            }
            // `??` — fallback on none/Err. The fallback branch may clobber
            // the scratch, but the branch that reads the scratch is the
            // exclusive other path.
            IrExprKind::UnwrapOr { expr, fallback } => match self.lower(expr, None)? {
                SliceTy::Option(s) => {
                    self.f
                        .instructions()
                        .local_tee(self.scr_i32_local)
                        .i32_eqz()
                        .if_(BlockType::Result(s.val_type()));
                    self.lower(fallback, Some(SliceTy::Scalar(s)))?;
                    self.f.instructions().else_().local_get(self.scr_i32_local);
                    self.load_slot(s, almide_layout::OPTION_FIELD);
                    self.f.instructions().end();
                    SliceTy::Scalar(s)
                }
                SliceTy::Result(o, _) => {
                    self.f
                        .instructions()
                        .local_tee(self.scr_i32_local)
                        .i32_load(slot_memarg(almide_layout::SUM_TAG))
                        .i32_const(0)
                        .i32_ne()
                        .if_(BlockType::Result(o.val_type()));
                    self.lower(fallback, Some(SliceTy::Scalar(o)))?;
                    self.f.instructions().else_().local_get(self.scr_i32_local);
                    self.load_slot(o, almide_layout::SUM_FIELD);
                    self.f.instructions().end();
                    SliceTy::Scalar(o)
                }
                other => return unsup(&format!("unwrap-or-of:{other:?}")),
            },
            // List literal: alloc, then store each element through a hold
            // local (kept live across element lowering — the pool makes
            // nesting safe by construction).
            IrExprKind::List { elements } => {
                let elem = match want.map_or_else(|| self.infer(e), Ok)? {
                    SliceTy::List(s) => s,
                    other => return unsup(&format!("ty-mismatch:list-vs-{other:?}")),
                };
                let stride = elem.slot_size();
                let hold = self.hold_i32()?;
                self.f
                    .instructions()
                    .i32_const((elements.len() as u32 * stride) as i32)
                    .call(F_ALLOC)
                    .local_set(hold);
                for (i, el) in elements.iter().enumerate() {
                    self.f.instructions().local_get(hold);
                    self.lower(el, Some(SliceTy::Scalar(elem)))?;
                    self.store_slot(elem, i as u32 * stride);
                }
                self.f.instructions().local_get(hold);
                self.release_i32();
                SliceTy::List(elem)
            }
            // xs[i]: bounds-checked element load. Out of bounds aborts on
            // the oracle — the trap lands in the abort-parity bucket.
            IrExprKind::IndexAccess { object, index } => {
                let elem = match self.lower(object, None)? {
                    SliceTy::List(s) => s,
                    other => return unsup(&format!("index-of:{other:?}")),
                };
                let stride = elem.slot_size();
                let hold = self.hold_i32()?;
                self.f.instructions().local_set(hold);
                self.lower(index, Some(INT))?;
                let idx = self.hold_i64()?;
                self.f.instructions().local_tee(idx);
                // idx < 0 || idx >= count → trap
                let mut i = self.f.instructions();
                i.i64_const(0).i64_lt_s();
                i.local_get(idx);
                i.local_get(hold).i32_load(len_memarg()).i32_const(stride as i32).i32_div_u();
                i.i64_extend_i32_u().i64_ge_s();
                i.i32_or().if_(BlockType::Empty).unreachable().end();
                // element address: hold + idx*stride, slot at offset PAYLOAD
                i.local_get(hold);
                i.local_get(idx).i32_wrap_i64().i32_const(stride as i32).i32_mul().i32_add();
                self.load_slot(elem, 0);
                self.release_i64();
                self.release_i32();
                SliceTy::Scalar(elem)
            }
            other => return unsup(&format!("expr:{}", expr_kind_name(other))),
        };
        if let Some(w) = want
            && got != w
        {
            return unsup(&format!("ty-mismatch:{got:?}-vs-{w:?}"));
        }
        Ok(got)
    }

    fn store_slot(&mut self, s: Scalar, payload_relative: u32) {
        let m = slot_memarg(payload_relative);
        match s {
            Scalar::Int => self.f.instructions().i64_store(m),
            Scalar::Bool | Scalar::Str => self.f.instructions().i32_store(m),
        };
    }

    fn load_slot(&mut self, s: Scalar, payload_relative: u32) {
        let m = slot_memarg(payload_relative);
        match s {
            Scalar::Int => self.f.instructions().i64_load(m),
            Scalar::Bool | Scalar::Str => self.f.instructions().i32_load(m),
        };
    }

    // ── match lowering ──────────────────────────────────────────────────

    /// `result`: Some(ty) = value position, None = statement position.
    fn lower_match(
        &mut self,
        subject: &IrExpr,
        arms: &[IrMatchArm],
        result: Option<SliceTy>,
    ) -> Result<(), EmitError> {
        if arms.is_empty() {
            return unsup("match:no-arms");
        }
        if arms.iter().any(|a| a.guard.is_some()) {
            return unsup("match-guard");
        }
        let subj_ty = self.lower(subject, None)?;
        let scr = match subj_ty.val_type() {
            ValType::I64 => self.scr_i64_local,
            _ => self.scr_i32_local,
        };
        self.f.instructions().local_set(scr);
        self.lower_arm_chain(arms, subj_ty, scr, result)
    }

    fn lower_arm_chain(
        &mut self,
        arms: &[IrMatchArm],
        subj_ty: SliceTy,
        scr: u32,
        result: Option<SliceTy>,
    ) -> Result<(), EmitError> {
        let arm = &arms[0];
        if pattern_irrefutable(&arm.pattern) {
            // Selected unconditionally; later arms are dead (checker-
            // verified reachability aside, the oracle picks the first).
            self.emit_pattern_binds(&arm.pattern, subj_ty, scr)?;
            return self.lower_arm_body(&arm.body, result);
        }
        self.emit_pattern_test(&arm.pattern, subj_ty, scr)?;
        let bt = match result {
            Some(t) => BlockType::Result(t.val_type()),
            None => BlockType::Empty,
        };
        self.f.instructions().if_(bt);
        self.emit_pattern_binds(&arm.pattern, subj_ty, scr)?;
        self.lower_arm_body(&arm.body, result)?;
        self.f.instructions().else_();
        if arms.len() > 1 {
            self.lower_arm_chain(&arms[1..], subj_ty, scr, result)?;
        } else {
            // The checker promises exhaustiveness — if it's ever wrong,
            // trap LOUDLY instead of silently misbehaving.
            self.f.instructions().unreachable();
        }
        self.f.instructions().end();
        Ok(())
    }

    fn lower_arm_body(&mut self, body: &IrExpr, result: Option<SliceTy>) -> Result<(), EmitError> {
        match result {
            Some(ty) => self.lower(body, Some(ty)).map(|_| ()),
            None => self.lower_stmt_expr(body),
        }
    }

    /// Push an i32 bool: does the subject (in `scr`) match `p`?
    fn emit_pattern_test(
        &mut self,
        p: &IrPattern,
        subj_ty: SliceTy,
        scr: u32,
    ) -> Result<(), EmitError> {
        match (p, subj_ty) {
            (IrPattern::Literal { expr }, SliceTy::Scalar(s)) => {
                self.f.instructions().local_get(scr);
                self.lower(expr, Some(SliceTy::Scalar(s)))?;
                match s {
                    Scalar::Int => self.f.instructions().i64_eq(),
                    Scalar::Bool => self.f.instructions().i32_eq(),
                    Scalar::Str => self.f.instructions().call(F_STR_EQ),
                };
                Ok(())
            }
            (IrPattern::None, SliceTy::Option(_)) => {
                self.f.instructions().local_get(scr).i32_eqz();
                Ok(())
            }
            (IrPattern::Some { inner }, SliceTy::Option(s)) => {
                if pattern_irrefutable(inner) {
                    self.f.instructions().local_get(scr).i32_const(0).i32_ne();
                    return Ok(());
                }
                // some(<literal>): non-null AND field == literal.
                let IrPattern::Literal { expr } = inner.as_ref() else {
                    return unsup(&format!("pattern:some-{}", pattern_name(inner)));
                };
                self.f.instructions().local_get(scr).if_(BlockType::Result(ValType::I32));
                self.f.instructions().local_get(scr);
                self.load_slot(s, almide_layout::OPTION_FIELD);
                self.lower(expr, Some(SliceTy::Scalar(s)))?;
                match s {
                    Scalar::Int => self.f.instructions().i64_eq(),
                    Scalar::Bool => self.f.instructions().i32_eq(),
                    Scalar::Str => self.f.instructions().call(F_STR_EQ),
                };
                self.f.instructions().else_().i32_const(0).end();
                Ok(())
            }
            (IrPattern::Ok { inner }, SliceTy::Result(o, _))
            | (IrPattern::Err { inner }, SliceTy::Result(_, o)) => {
                let want_tag = i32::from(matches!(p, IrPattern::Err { .. }));
                self.f
                    .instructions()
                    .local_get(scr)
                    .i32_load(slot_memarg(almide_layout::SUM_TAG))
                    .i32_const(want_tag)
                    .i32_eq();
                if pattern_irrefutable(inner) {
                    return Ok(());
                }
                let IrPattern::Literal { expr } = inner.as_ref() else {
                    return unsup(&format!("pattern:sum-{}", pattern_name(inner)));
                };
                // tag matches AND field == literal.
                self.f.instructions().if_(BlockType::Result(ValType::I32));
                self.f.instructions().local_get(scr);
                self.load_slot(o, almide_layout::SUM_FIELD);
                self.lower(expr, Some(SliceTy::Scalar(o)))?;
                match o {
                    Scalar::Int => self.f.instructions().i64_eq(),
                    Scalar::Bool => self.f.instructions().i32_eq(),
                    Scalar::Str => self.f.instructions().call(F_STR_EQ),
                };
                self.f.instructions().else_().i32_const(0).end();
                Ok(())
            }
            _ => unsup(&format!("pattern:{}", pattern_name(p))),
        }
    }

    /// Bind pattern variables from the subject (in `scr`).
    fn emit_pattern_binds(
        &mut self,
        p: &IrPattern,
        subj_ty: SliceTy,
        scr: u32,
    ) -> Result<(), EmitError> {
        match p {
            IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => Ok(()),
            IrPattern::Bind { var, .. } => {
                let Some(&(idx, _)) = self.locals.get(var) else {
                    return unsup("bind:unmapped");
                };
                self.f.instructions().local_get(scr).local_set(idx);
                Ok(())
            }
            IrPattern::Some { inner } => {
                let SliceTy::Option(s) = subj_ty else {
                    return unsup("pattern:some-on-non-option");
                };
                self.bind_inner(inner, s, almide_layout::OPTION_FIELD, scr)
            }
            IrPattern::Ok { inner } => {
                let SliceTy::Result(o, _) = subj_ty else {
                    return unsup("pattern:ok-on-non-result");
                };
                self.bind_inner(inner, o, almide_layout::SUM_FIELD, scr)
            }
            IrPattern::Err { inner } => {
                let SliceTy::Result(_, e) = subj_ty else {
                    return unsup("pattern:err-on-non-result");
                };
                self.bind_inner(inner, e, almide_layout::SUM_FIELD, scr)
            }
            other => unsup(&format!("pattern:{}", pattern_name(other))),
        }
    }

    fn bind_inner(
        &mut self,
        inner: &IrPattern,
        s: Scalar,
        field: u32,
        scr: u32,
    ) -> Result<(), EmitError> {
        match inner {
            IrPattern::Wildcard | IrPattern::Literal { .. } => Ok(()),
            IrPattern::Bind { var, .. } => {
                let Some(&(idx, _)) = self.locals.get(var) else {
                    return unsup("bind:unmapped");
                };
                self.f.instructions().local_get(scr);
                self.load_slot(s, field);
                self.f.instructions().local_set(idx);
                Ok(())
            }
            other => unsup(&format!("pattern:inner-{}", pattern_name(other))),
        }
    }

    // ── inference (non-emitting) ────────────────────────────────────────

    /// Non-emitting slice-type resolution — used where wasm needs a block
    /// type before an arm is lowered. Reads the CHECKER's annotation
    /// (`IrExpr.ty`), the authoritative type on every node; an unmappable
    /// annotation is an honest reason, and `lower`'s own result is still
    /// verified against the hint afterwards (defense in depth).
    fn infer(&self, e: &IrExpr) -> Result<SliceTy, EmitError> {
        match slice_ty_of(&e.ty) {
            Some(t) => Ok(t),
            None => unsup(&format!("infer-ty:{}", ty_name(&e.ty))),
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
                self.lower(left, Some(INT))?;
                self.lower(right, Some(INT))?;
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
                Ok(INT)
            }
            Lt | Gt | Lte | Gte => {
                self.lower(left, Some(INT))?;
                self.lower(right, Some(INT))?;
                let mut i = self.f.instructions();
                match op {
                    Lt => i.i64_lt_s(),
                    Gt => i.i64_gt_s(),
                    Lte => i.i64_le_s(),
                    Gte => i.i64_ge_s(),
                    _ => unreachable!(),
                };
                Ok(BOOL)
            }
            Eq | Neq => {
                let lt = self.lower(left, None)?;
                self.lower(right, Some(lt))?;
                match lt {
                    INT => {
                        self.f.instructions().i64_eq();
                    }
                    BOOL => {
                        self.f.instructions().i32_eq();
                    }
                    // Block byte-equality: strings, and lists of ints/
                    // bools (their bytes ARE their values). List[String]
                    // holds addresses — identity is NOT equality: refuse.
                    STR | SliceTy::List(Scalar::Int) | SliceTy::List(Scalar::Bool) => {
                        self.f.instructions().call(F_STR_EQ);
                    }
                    other => return unsup(&format!("binop:eq-{other:?}")),
                }
                if matches!(op, Neq) {
                    self.f.instructions().i32_eqz();
                }
                Ok(BOOL)
            }
            // SHORT-CIRCUIT: the right operand must not evaluate (and
            // possibly trap) when the left already decides — an `if`
            // yielding i32, never a strict bitop.
            And => {
                self.lower(left, Some(BOOL))?;
                self.f.instructions().if_(BlockType::Result(ValType::I32));
                self.lower(right, Some(BOOL))?;
                self.f.instructions().else_().i32_const(0).end();
                Ok(BOOL)
            }
            Or => {
                self.lower(left, Some(BOOL))?;
                self.f.instructions().if_(BlockType::Result(ValType::I32)).i32_const(1).else_();
                self.lower(right, Some(BOOL))?;
                self.f.instructions().end();
                Ok(BOOL)
            }
            ConcatStr => {
                self.lower(left, Some(STR))?;
                self.lower(right, Some(STR))?;
                self.f.instructions().call(F_CONCAT);
                Ok(STR)
            }
            // List ++ List: byte-concat of the element arrays IS element
            // concat (same stride both sides).
            ConcatList => {
                let lt = self.lower(left, None)?;
                let SliceTy::List(_) = lt else {
                    return unsup(&format!("concat-list-of:{lt:?}"));
                };
                self.lower(right, Some(lt))?;
                self.f.instructions().call(F_CONCAT);
                Ok(lt)
            }
            other => unsup(&format!("binop:{other:?}")),
        }
    }
}

fn pattern_irrefutable(p: &IrPattern) -> bool {
    matches!(p, IrPattern::Wildcard | IrPattern::Bind { .. })
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

fn pattern_name(p: &IrPattern) -> String {
    let dbg = format!("{p:?}");
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}

fn ty_name(t: &Ty) -> String {
    let dbg = format!("{t:?}");
    dbg.split(&[' ', '(', '{'][..]).next().unwrap_or("?").to_string()
}
