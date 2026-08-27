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
use wasm_encoder::{Function, MemArg, ValType};

#[derive(Debug)]
pub enum EmitError {
    /// This IR shape is outside the current slice. The reason string feeds
    /// the burn-up histogram — precise, greppable, shrink-only.
    Unsupported(String),
}

fn unsup<T>(what: &str) -> Result<T, EmitError> {
    Err(EmitError::Unsupported(what.to_string()))
}

mod bytes;
mod bytes_rw;
mod calls;
mod cells;
mod unroll;
mod collect;
mod collections;
mod collections_hof;
mod collections_set;
mod emit;
pub use emit::emit_program;
mod emitter;
mod emitter_vars;
mod patterns;
mod prim;
mod runtime;
mod runtime_alloc;
mod runtime_str;
mod scalar_ext;
mod data;
mod assembly;
pub(crate) use assembly::*;
mod equality;
pub(crate) mod func;
pub(crate) use func::*;
pub(crate) mod ty;
pub(crate) use ty::*;
mod list;
mod list_comb;
mod list_edit;
mod list_search;
mod list_fuse;
mod list_mut;
mod list_order;
mod list_sort;
mod string_scan;
mod stmts;
mod string_ext;
pub(crate) mod work;
pub(crate) use work::*;
mod display;
mod matrix;
mod matrix_kernels;
mod matrix_load;
mod matrix_rope;
mod matrix_scalars;
mod binop;
mod fan;
mod fs;
mod fs_meta;
mod host_env;
mod json_path_helpers;
mod fuel;
mod ranges;
mod rc_ownership;
mod sums;
mod tco;
mod types_table;
mod value;
mod utf8_helpers;
mod value_helpers;
mod whitelist;

use collect::collect_binds;
use emitter::{HOLD_F64_POOL, HOLD_I32_POOL, HOLD_I64_POOL};
use runtime::*;
use runtime_str::*;
use types_table::TypeTable;

// ── fixed memory map ────────────────────────────────────────────────────

/// itoa scratch region: digits are written back-to-front ending here.
/// 32 bytes ≥ the longest rendering, `-9223372036854775808` (20 bytes).
const ITOA_END: u32 = 48;
/// Free-list heads (RC-2): 16 size classes × 4B at `[48,112)`. Class c
/// holds freed blocks whose TOTAL (header+payload, 4-aligned) is in
/// `[16<<c, 32<<c)` — filed by floor, taken by ceil, so a taken block
/// always fits the request without rounding the bump path.
const FREELIST_BASE: u32 = ITOA_END;
const FREELIST_CLASSES: u32 = 16;
/// The pool starts right after the scratch + free-list table: null
/// guard `[0,PAYLOAD)`, padding to 16, scratch `[16,48)`, free-list
/// heads `[48,112)`.
const POOL_START: u32 = FREELIST_BASE + FREELIST_CLASSES * 4;
/// Minimum room the line buffer must have beyond the pool.
const LINE_BUF_MIN: u64 = 65536;

// ── function / type / global indices ────────────────────────────────────

const F_PRINTLN_IMPORT: u32 = 0;
const F_EPRINTLN_IMPORT: u32 = 1;
const F_EXIT_IMPORT: u32 = 2;
/// `almide.fs_call(op, a_ptr, a_len, b_ptr, b_len) -> i64` — the fs host
/// boundary: the HARNESS runs the same std::fs code the native runtime
/// runs (io_err = Display, so error strings match verbatim) and parks
/// result bytes in its buffer; negative return = err, else the payload
/// meaning is per-op (len / flag / scalar).
const F_FS_CALL: u32 = 3;
/// `almide.host_read(dst_ptr)` — copy the parked result buffer into
/// guest memory (the guest allocated `len` bytes first).
const F_HOST_READ: u32 = 4;
const F_PRINTLN_BLOCK: u32 = 5;
const F_EPRINTLN_BLOCK: u32 = 6;
const F_APPEND_COPY: u32 = 7;
const F_ITOA: u32 = 8;
const F_APPEND_I64: u32 = 9;
const F_APPEND_BOOL: u32 = 10;
const F_ALLOC: u32 = 11;
const F_INT_TO_STRING: u32 = 12;
const F_CONCAT: u32 = 13;
const F_STR_EQ: u32 = 14;
const F_LIST_GET_8: u32 = 15;
const F_LIST_GET_4: u32 = 16;
const F_LIST_PUSH_8: u32 = 17;
const F_LIST_PUSH_4: u32 = 18;
const F_LIST_JOIN: u32 = 19;
const F_BLOCK_COPY: u32 = 20;
const F_BUF_TO_BLOCK: u32 = 21;
const F_STR_LEN_CHARS: u32 = 22;
const F_SCAN_W64: u32 = 23;
const F_SCAN_W32: u32 = 24;
const F_SCAN_STR: u32 = 25;
const F_F16_TO_F64: u32 = 26;
const F_CP_OFF: u32 = 27;
const F_STR_SLICE: u32 = 28;
const F_STR_REPEAT: u32 = 29;
const F_STR_CMP: u32 = 30;
const F_STR_REPLACE: u32 = 31;
/// `$copy(dst, src, len)` — the tiny-aware copy: len < 16 walks bytes
/// (wasmtime's memory.copy is an out-of-line libcall whose fixed cost
/// dwarfs small moves), else one memory.copy.
const F_COPY: u32 = 32;
/// `$free(block)` — file a dead block into its size-class free list
/// (RC-2). Callers must OWN the block outright; the only emitters today
/// are the sort machinery's private scratch buffers.
const F_FREE: u32 = 33;
/// `$inc(block)` / `$dec_flat(block)` — the RC-3 ownership pair: inc on
/// borrow-shares, dec at binding death; both no-op below the heap floor
/// so pool statics are untouchable. dec frees FLAT blocks at rc zero.
const F_INC: u32 = 34;
const F_DEC_FLAT: u32 = 35;
/// `$cow(block)` — the copy-on-write judge at in-place mutation entries
/// (RC-5): shared blocks copy, unique ones pass through.
const F_COW: u32 = 36;
/// `$str_append(dst, src) -> i32`: the growing-accumulator window —
/// `acc = acc + s` appends IN PLACE when the accumulator is an owned heap
/// block (rc == 1) with class-slack headroom, else concats and releases
/// the outgrown block. Ownership transfers through the call (the C-132
/// write-back shape the Assign dec-skip was built for).
const F_STR_APPEND: u32 = 37;
/// First program-function index; `main` sits after every program function.
const F_FN_BASE: u32 = 38;
/// Fixed type indices: 0 print(ptr,len)→(), 1 block-print(i32)→(),
/// 2 append_copy, 3 append_i64, 4 main ()→(), 5 (i32,i32)→i32
/// (append_bool/concat/str_eq), 6 (i64)→i32 (itoa/int_to_string),
/// 7 (i32)→i32 (alloc); program-function types start after.
const T_MAIN: u32 = 4;
/// (i32,i64)→i32: list_get_8 / list_push_8; 9: (i32)→i64 str_len_chars;
/// 10: (i32,i32,i32,i64)→i32 scan_w64; 11: (i32,i32,i32,i32)→i32 scan_w32/str;
/// 12: (i32)→f64 f16_to_f64; 13: (i32,i64)→i32 cp_off/str_repeat;
/// 14: (i32,i64,i64)→i32 str_slice.
const T_FN_BASE: u32 = 18;
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
/// Deterministic meter (ALS-DT2, mirrors the interp's det_* cells):
/// remaining fuel units (i64, starts at i64::MAX — outside a region the
/// wrapping decrements never cut).
const G_DET_FUEL: u32 = 4;
/// The active region's entry units (i64).
const G_DET_ENTRY: u32 = 5;
/// T5-1 wall deadline (i64, i64::MAX = unarmed), the hit flag (i32)
/// and the persisted verdict (i64) — globals 9/10/11.
const G_T_DEADLINE: u32 = 9;
const G_T_HIT: u32 = 10;
const G_T_VERDICT: u32 = 11;
/// Last region's exhausted verdict (i64 0/1).
const G_DET_VERDICT: u32 = 6;
/// Last region's consumed units (i64).
const G_DET_SPEND: u32 = 7;
/// Region nesting depth (i32) — the cut condition needs depth > 0.
const G_DET_DEPTH: u32 = 8;
/// Fixed runtime globals above; top-let globals start here.
const G_FIXED_COUNT: u32 = 12;

// ── slice value model ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Scalar {
    /// Almide Int/Int64 — wasm i64.
    Int,
    /// Almide Float — wasm f64 (bit-exact with the oracle's f64; the
    /// numeric-determinism obligation is carried by using the SAME
    /// self-hosted Dragon4 formatting the oracle uses, never a host
    /// printf).
    Float,
    /// Almide Bool — wasm i32, 0/1.
    Bool,
    /// Almide String — wasm i32 holding the block BASE address.
    Str,
    /// Almide Bytes — a byte-packed block (String's twin without the
    /// UTF-8 reading); len = byte count. In-place `set_*` is sound under
    /// the bind-deep-copy doctrine (a local's block is uniquely its own).
    Bytes,
}

impl Scalar {
    fn val_type(self) -> ValType {
        match self {
            Scalar::Int => ValType::I64,
            Scalar::Float => ValType::F64,
            Scalar::Bool | Scalar::Str | Scalar::Bytes => ValType::I32,
        }
    }

    /// Byte width of this scalar's slot inside a sum block.
    fn slot_size(self) -> u32 {
        match self {
            Scalar::Int | Scalar::Float => 8,
            Scalar::Bool | Scalar::Str | Scalar::Bytes => 4,
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
    /// A function VALUE — an i32 funcref-table slot (+1-biased so 0 is
    /// the null/trap slot, W-1). The signature lives in the TypeTable's
    /// fn-sig arena. Only value-referenced functions enter the table.
    Fn(u32),
    /// `Unit` as a VALUE (a bind, a param, an effect ok payload): one
    /// i32 zero. Pure fns returning Unit keep the void convention (ret
    /// None) — this variant only appears where Unit must FLOW.
    Unit,
    /// The dynamic `Value` (Codec/json data model) — a 16-byte tagged
    /// block in THIS backend's layout (ratified rebuild; the incumbent's
    /// len-as-tag convention stays behind). See value.rs.
    Value,
    /// A matrix — a FLAT block `[rows:i32][cols:i32][f64 data…]` (see
    /// matrix.rs; no row-pointer array).
    Matrix,
}

const STR: SliceTy = SliceTy::Scalar(Scalar::Str);
const INT: SliceTy = SliceTy::Scalar(Scalar::Int);
const BOOL: SliceTy = SliceTy::Scalar(Scalar::Bool);
const FLOAT: SliceTy = SliceTy::Scalar(Scalar::Float);

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
            | SliceTy::Named(_)
            | SliceTy::Fn(_)
            | SliceTy::Unit
            | SliceTy::Value
            | SliceTy::Matrix => ValType::I32,
        }
    }

    /// Slot width of this value inside an aggregate (record field, variant
    /// case field, option slot, list element). Every 8-byte VALUE type is
    /// an 8-byte slot — f64 included (the two-way version silently gave
    /// Float slots 4 bytes and the parity net caught zeroed float lists).
    fn slot_size(self) -> u32 {
        match self.val_type() {
            ValType::I64 | ValType::F64 => 8,
            _ => 4,
        }
    }
}



// ── literal pool ────────────────────────────────────────────────────────



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
    /// Module functions ALSO indexed by their simple name, for the
    /// self-host registry's surface→implementation resolution.
    impl_index: HashMap<String, usize>,
    infos: Vec<FnInfo>,
}

/// The immutable per-program context every function lowering shares.
pub(crate) struct Ctx<'a> {
    pub(crate) table: &'a FnTable,
    pub(crate) types: &'a TypeTable,
    pub(crate) work: &'a FnWork,
    /// Top-let vars (root + module) as WASM GLOBALS: (space, VarId) →
    /// (global index, slice type). Functions read them across function
    /// boundaries — the class main-local top-lets could never serve.
    pub(crate) globals: &'a HashMap<GVar, (u32, SliceTy)>,
}

/// Function-VALUE work discovered during lowering: funcref-table entries
/// (+1-biased slots), call_indirect type interning (indices assigned
/// eagerly after the per-fn types), and non-capturing lambdas lifted into
/// extra functions (lowered by the fixed-point loop in `emit_program`).
#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) enum TableEntry {
    /// A program function referenced as a value — its table slot holds a
    /// SHIM `(env, params...) -> ret` forwarding to the plain fn (the
    /// uniform closure convention: env is arg 0 everywhere, W-2).
    Fn(usize),
    /// An ok-wrapping adapter: a PURE fn filling an EFFECT slot (C-221).
    Adapter { target: usize, raw: SliceTy },
    /// A lifted non-capturing lambda (index into `FnWork::lifted`).
    Lambda(u32),
}

#[derive(Clone)]
pub(crate) struct LiftedLambda {
    pub(crate) params: Vec<(VarId, SliceTy)>,
    pub(crate) ret: Option<SliceTy>,
    pub(crate) effect_raw: Option<SliceTy>,
    pub(crate) body: IrExpr,
    /// Captured outer locals: (var, type, closure-block payload offset).
    /// The lifted fn's prelude loads each from the env param (raw param
    /// slot 0) into a fresh local — by-value snapshot semantics.
    pub(crate) captures: Vec<(VarId, SliceTy, u32, bool)>,
    /// The variable space the body's VarIds index (the lifting fn's own
    /// space — a lambda inside a module fn reads module-space globals).
    pub(crate) var_space: u32,
}

/// A call_indirect signature at the wasm value-type level.
type WasmSig = (Vec<ValType>, Option<ValType>);
/// A lifted lambda's lowered form: wasm sig halves, body, callee set.
type LoweredLifted = (Vec<ValType>, Option<ValType>, Function, HashSet<usize>);

/// The registry's implementation-symbol set (unique by design — they are
/// the self-host linkage names).
fn registry_impl_names() -> &'static std::collections::HashSet<&'static str> {
    use std::sync::OnceLock;
    static NAMES: OnceLock<std::collections::HashSet<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| {
        almide_types::self_host_registry::self_host_runtime()
            .iter()
            .flat_map(|(_, maps)| maps.iter())
            .map(|(impl_fn, _)| *impl_fn)
            .collect()
    })
}


// ── entry ───────────────────────────────────────────────────────────────




// ── reason-string helpers ───────────────────────────────────────────────

fn pattern_irrefutable(p: &IrPattern) -> bool {
    match p {
        IrPattern::Wildcard | IrPattern::Bind { .. } => true,
        // A tuple of irrefutable positions always matches.
        IrPattern::Tuple { elements } => elements.iter().all(pattern_irrefutable),
        _ => false,
    }
}

pub(crate) use almide_ir::top_let_storage::GVar;

/// One top-let initializer for main's prelude: the space its body's VarIds
/// index and the module whose fns it may call by bare name (None = root).
#[derive(Clone)]
pub(crate) struct InitLet {
    pub(crate) space: u32,
    pub(crate) module: Option<String>,
    pub(crate) tl: IrTopLet,
}

/// Top-lets (root + module) as wasm globals + the dependency init order
/// (split from emit_program for the complexity budget).
#[allow(clippy::type_complexity)]
fn build_globals(
    ir: &IrProgram,
    types: &TypeTable,
) -> (HashMap<GVar, (u32, SliceTy)>, Vec<(VarId, SliceTy)>, Vec<InitLet>) {
    // Top-lets (root + module) become wasm globals — zero-initialized,
    // then set by main's prelude in DEPENDENCY order (C-077: the same
    // `dependency_init_order` the interp uses, so the order matches by
    // construction). Identities are SPACED (#1596): separately-lowered
    // modules each carry their own VarTable starting at VarId 0, so a
    // bare-VarId key collides across tables — the (space, var) pair is
    // the unambiguous name, and every use-site alias (the entry reading
    // `m.SYSTEM`) maps to its declaration's slot.
    use almide_ir::top_let_storage::{
        build_global_tables_spaced, dependency_init_order_spaced,
    };
    let mut global_map: HashMap<GVar, (u32, SliceTy)> = HashMap::new();
    let mut global_decls: Vec<(VarId, SliceTy)> = Vec::new();
    {
        let mut next = G_FIXED_COUNT;
        let mut add = |space: u32,
                       tl: &IrTopLet,
                       map: &mut HashMap<GVar, (u32, SliceTy)>,
                       decls: &mut Vec<(VarId, SliceTy)>| {
            if let Some(sty) = slice_ty_of(&tl.ty, types) {
                map.insert((space, tl.var), (next, sty));
                decls.push((tl.var, sty));
                next += 1;
            }
            // An unsliceable top-let stays out of the pool; reading it
            // refuses at the var site with its own honest reason.
        };
        for tl in &ir.top_lets {
            add(0, tl, &mut global_map, &mut global_decls);
        }
        for (i, m) in ir.modules.iter().enumerate() {
            for tl in &m.top_lets {
                add(i as u32 + 1, tl, &mut global_map, &mut global_decls);
            }
        }
    }
    let (_info, alias, _off) = build_global_tables_spaced(ir);
    // Use-site aliases resolve to the declaration's slot — one lookup at
    // the Var site whatever space the read happens in.
    for (&use_g, &decl_g) in &alias {
        if let Some(&slot) = global_map.get(&decl_g) {
            global_map.insert(use_g, slot);
        }
    }
    let init_order = dependency_init_order_spaced(ir, &alias);
    let mut init_by_var: HashMap<GVar, InitLet> = HashMap::new();
    for tl in &ir.top_lets {
        init_by_var.insert((0, tl.var), InitLet { space: 0, module: None, tl: tl.clone() });
    }
    for (i, m) in ir.modules.iter().enumerate() {
        for tl in &m.top_lets {
            init_by_var.insert(
                (i as u32 + 1, tl.var),
                InitLet { space: i as u32 + 1, module: Some(m.name.as_str().to_string()), tl: tl.clone() },
            );
        }
    }
    let init_lets: Vec<InitLet> =
        init_order.iter().filter_map(|g| init_by_var.get(g).cloned()).collect();
    (global_map, global_decls, init_lets)
}


/// The callable-fn flattening (entry fns + module fns under qualified
/// names, Hole-bodied surfaces excluded) — split from emit_program. The
/// third field is the fn's variable SPACE (0 = entry program, i+1 =
/// `ir.modules[i]` — #1596): module fn bodies index their own VarTable.
fn collect_program_fns(ir: &IrProgram) -> Vec<(&IrFunction, Option<String>, u32)> {
    let mut program_fns: Vec<(&IrFunction, Option<String>, u32)> = ir
        .functions
        .iter()
        .filter(|f| !f.is_test && f.name.as_str() != "main")
        .map(|f| (f, None, 0))
        .collect();
    for (i, m) in ir.modules.iter().enumerate() {
        for f in &m.functions {
            // A Hole body is a bodyless SURFACE decl (`= _`) — a bridge
            // boundary, not an implementation. Registering it would
            // shadow the self-host registry's real implementation with
            // an unlowersble stub (found by the burn-up: expr:Hole ×70).
            let is_surface = matches!(f.body.kind, IrExprKind::Hole);
            if !f.is_test && !is_surface {
                program_fns.push((
                    f,
                    Some(format!("{}.{}", m.name.as_str(), f.name.as_str())),
                    i as u32 + 1,
                ));
            }
        }
    }

    // Signature table first: call sites need indices and types up front.
    program_fns
}

