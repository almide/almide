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

use almide_ir::{IrExpr, IrExprKind, IrFunction, IrPattern, IrProgram, IrStmtKind, IrTopLet, VarId};
use almide_types::types::{Ty, TypeConstructorId};
use wasm_encoder::{
    CodeSection, ConstExpr, DataSection, EntityType, ExportKind, ExportSection, Function,
    FunctionSection, GlobalSection, GlobalType, ImportSection, MemArg, MemorySection, MemoryType,
    Module, TypeSection, ValType,
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

mod calls;
mod collect;
mod collections;
mod emitter;
mod patterns;
mod runtime;
mod types_table;

use collect::collect_binds;
use emitter::{Emitter, HOLD_I32_POOL, HOLD_I64_POOL};
use runtime::*;
use types_table::TypeTable;

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
const F_BUF_TO_BLOCK: u32 = 18;
const F_STR_LEN_CHARS: u32 = 19;
const F_SCAN_W64: u32 = 20;
const F_SCAN_W32: u32 = 21;
const F_SCAN_STR: u32 = 22;
/// First program-function index; `main` sits after every program function.
const F_FN_BASE: u32 = 23;
/// Fixed type indices: 0 print(ptr,len)→(), 1 block-print(i32)→(),
/// 2 append_copy, 3 append_i64, 4 main ()→(), 5 (i32,i32)→i32
/// (append_bool/concat/str_eq), 6 (i64)→i32 (itoa/int_to_string),
/// 7 (i32)→i32 (alloc); program-function types start after.
const T_MAIN: u32 = 4;
/// (i32,i64)→i32: list_get_8 / list_push_8; 9: (i32)→i64 str_len_chars;
/// 10: (i32,i32,i32,i64)→i32 scan_w64; 11: (i32,i32,i32,i32)→i32 scan_w32/str.
const T_FN_BASE: u32 = 12;
// Global 0 is the immutable line-buffer start (= align16(pool end)); it
// is emitted for inspectability but no instruction references it since
// the build cursor (global 2) took over.
/// Mutable i32 global: the bump-allocator head.
const G_HEAP: u32 = 1;
/// Mutable i32 global: the line-buffer BUILD CURSOR — stack-disciplined so
/// interpolation builds NEST (a value-position `"${...}"` inside another
/// build starts after the outer's partial content and restores on exit).
const G_LINE_CURSOR: u32 = 2;
/// Immutable i32 global: one past the line buffer (= heap start); the
/// append helpers trap LOUDLY on overflow instead of corrupting the heap.
const G_LINE_END: u32 = 3;

// ── slice value model ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Interned handle to an element `SliceTy` in the `TypeTable`'s arena.
/// Deduplicated on intern, so handle equality IS type equality — which
/// keeps `SliceTy`'s derived `==` exact across arbitrary nesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ETy(u32);

impl ETy {
    pub(crate) fn from_index(i: usize) -> ETy {
        ETy(i as u32)
    }

    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SliceTy {
    Scalar(Scalar),
    /// `Option[T]` — i32, NULL_ADDR = none; `some` is a block whose slot
    /// holds T's word. Nesting is fine: `some(none)` is a block holding
    /// NULL_ADDR, distinct from the outer NULL_ADDR.
    Option(ETy),
    /// `Result[ok, err]` — i32 tagged block.
    Result(ETy, ETy),
    /// `List[T]` — i32 block; payload = the element array, block len
    /// = count × stride (stride = T's slot size). No COW is needed
    /// because in-place mutation (IndexAssign) is refused — every list
    /// op yields a fresh block, so sharing is unobservable; the bind
    /// deep-copy copies ONE level, sound because inner blocks are never
    /// mutated in place (push needs a plain var, member/index are reads).
    List(ETy),
    /// `Map[K, V]` — i32 block; payload = INSERTION-ORDERED (k, v) entries
    /// (the oracle's semantics), entry layout from `pack_fields`. Keys are
    /// scalars (equality must be defined); values any slice type. Same
    /// no-in-place-mutation doctrine: binds deep-copy, `map.insert`'s mut
    /// form is a var write-back.
    Map(ETy, ETy),
    /// `Set[T]` — i32 block; an insertion-ordered element vector with the
    /// dedup-on-insert invariant (layout-identical to List[T]).
    Set(ETy),
    /// A tuple — i32 block of positional fields packed by `pack_fields`;
    /// the shape lives in the TypeTable's tuple arena (interned, so
    /// handle equality is shape equality).
    Tuple(u32),
    /// A user-defined record or variant — i32 block; the definition lives
    /// in the `TypeTable` at this index. Records are field blocks laid
    /// out by `almide_layout::pack_fields`; variants are tagged blocks
    /// (SUM_TAG + fields packed after SUM_FIELD's 8-byte pad). In-place
    /// field mutation (FieldAssign) is refused, so sharing addresses is
    /// unobservable — the same doctrine as List.
    Named(u32),
}

const STR: SliceTy = SliceTy::Scalar(Scalar::Str);
const INT: SliceTy = SliceTy::Scalar(Scalar::Int);
const BOOL: SliceTy = SliceTy::Scalar(Scalar::Bool);

impl SliceTy {
    fn val_type(self) -> ValType {
        match self {
            SliceTy::Scalar(s) => s.val_type(),
            SliceTy::Option(_)
            | SliceTy::Result(..)
            | SliceTy::List(_)
            | SliceTy::Map(..)
            | SliceTy::Set(_)
            | SliceTy::Tuple(_)
            | SliceTy::Named(_) => ValType::I32,
        }
    }

    /// Slot width of this value inside an aggregate (record field, variant
    /// case field, option slot, list element).
    fn slot_size(self) -> u32 {
        match self.val_type() {
            ValType::I64 => 8,
            _ => 4,
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

fn slice_ty_of(ty: &Ty, types: &TypeTable) -> Option<SliceTy> {
    if let Some(s) = scalar_of(ty) {
        return Some(SliceTy::Scalar(s));
    }
    match ty {
        Ty::Applied(TypeConstructorId::Option, args) if args.len() == 1 => {
            let e = slice_ty_of(&args[0], types)?;
            Some(SliceTy::Option(types.intern(e)))
        }
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => {
            let o = slice_ty_of(&args[0], types)?;
            let e = slice_ty_of(&args[1], types)?;
            Some(SliceTy::Result(types.intern(o), types.intern(e)))
        }
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            let e = slice_ty_of(&args[0], types)?;
            Some(SliceTy::List(types.intern(e)))
        }
        Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => {
            let k = slice_ty_of(&args[0], types)?;
            // Keys need defined equality — scalars only.
            let SliceTy::Scalar(_) = k else { return None };
            let v = slice_ty_of(&args[1], types)?;
            Some(SliceTy::Map(types.intern(k), types.intern(v)))
        }
        Ty::Applied(TypeConstructorId::Set, args) if args.len() == 1 => {
            let e = slice_ty_of(&args[0], types)?;
            let SliceTy::Scalar(_) = e else { return None };
            Some(SliceTy::Set(types.intern(e)))
        }
        Ty::Tuple(args) => {
            let mut elems = Vec::new();
            for a in args {
                elems.push(slice_ty_of(a, types)?);
            }
            Some(SliceTy::Tuple(types.tuple(elems)))
        }
        Ty::Named(name, args) if args.is_empty() => {
            types.by_name.get(name.as_str()).map(|&i| SliceTy::Named(i))
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

/// The immutable per-program context every function lowering shares.
pub(crate) struct Ctx<'a> {
    pub(crate) table: &'a FnTable,
    pub(crate) types: &'a TypeTable,
}

fn fn_signature(f: &IrFunction, types: &TypeTable) -> Result<(Vec<SliceTy>, Option<SliceTy>), String> {
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
        let Some(sty) = slice_ty_of(&p.ty, types) else {
            return Err(format!("param-ty:{}", ty_name(&p.ty)));
        };
        params.push(sty);
    }
    let ret = match &f.ret_ty {
        Ty::Unit => None,
        other => match slice_ty_of(other, types) {
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
    let types = TypeTable::build(ir);

    // Signature table first: call sites need indices and types up front.
    let mut table = FnTable { by_name: HashMap::new(), infos: Vec::new() };
    for (i, f) in program_fns.iter().enumerate() {
        let (params, ret, refuse) = match fn_signature(f, &types) {
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
        let ctx = Ctx { table: &table, types: &types };
        match lower_fn(&params, table.infos[i].ret, &f.body, &[], &ctx, &mut pool) {
            Ok(ok) => lowered.push(Ok(ok)),
            Err(EmitError::Unsupported(r)) => lowered.push(Err(r)),
        }
    }

    // `main`: top-lets as the eager prelude, then the body. Failure here is
    // fatal — main is always reachable.
    let ctx = Ctx { table: &table, types: &types };
    let (main_fn, main_calls) = lower_fn(&[], None, &main.body, &ir.top_lets, &ctx, &mut pool)?;

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
    types.ty().function([ValType::I32], [ValType::I64]); // 9: str_len_chars
    types.ty().function([ValType::I32, ValType::I32, ValType::I32, ValType::I64], [ValType::I32]); // 10: scan_w64
    types.ty().function([ValType::I32, ValType::I32, ValType::I32, ValType::I32], [ValType::I32]); // 11: scan_w32/str
    for (i, info) in table.infos.iter().enumerate() {
        // Refused functions keep a placeholder type — their stub body is
        // `unreachable` and no call site ever targets them.
        debug_assert_eq!(T_FN_BASE as usize + i, 12 + i);
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
    functions.function(5); // F_BUF_TO_BLOCK
    functions.function(9); // F_STR_LEN_CHARS
    functions.function(10); // F_SCAN_W64
    functions.function(11); // F_SCAN_W32
    functions.function(11); // F_SCAN_STR
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
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const(line_start as i32),
    );
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: false, shared: false },
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
    code.function(&emit_buf_to_block());
    code.function(&emit_str_len_chars());
    code.function(&emit_scan_w64());
    code.function(&emit_scan_w32());
    code.function(&emit_scan_str());
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
    ctx: &Ctx,
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
        let Some(sty) = slice_ty_of(&tl.ty, ctx.types) else {
            return unsup(&format!("bind-ty:{}", ty_name(&tl.ty)));
        };
        if seen.insert(tl.var) {
            binds.push((tl.var, sty));
        }
        collect_binds(&tl.value, &mut binds, &mut seen, ctx.types)?;
    }
    collect_binds(body, &mut binds, &mut seen, ctx.types)?;

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
            table: ctx.table,
            types: ctx.types,
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
            in_tail: false,
            f: &mut f,
        };
        for tl in top_lets {
            let (idx, declared) = em.locals[&tl.var];
            em.lower(&tl.value, Some(declared))?;
            if matches!(declared, SliceTy::List(_) | SliceTy::Map(..) | SliceTy::Set(_)) {
                em.f.instructions().call(F_BLOCK_COPY);
            }
            em.f.instructions().local_set(idx);
        }
        match ret {
            None => em.lower_stmt_expr(body)?,
            Some(want) => {
                em.lower_tail(body, Some(want))?;
            }
        }
    }
    f.instructions().end();
    Ok((f, calls))
}

// ── reason-string helpers ───────────────────────────────────────────────

fn pattern_irrefutable(p: &IrPattern) -> bool {
    match p {
        IrPattern::Wildcard | IrPattern::Bind { .. } => true,
        // A tuple of irrefutable positions always matches.
        IrPattern::Tuple { elements } => elements.iter().all(pattern_irrefutable),
        _ => false,
    }
}

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
