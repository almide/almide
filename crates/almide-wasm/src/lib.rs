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
mod calls;
mod collect;
mod collections;
mod emitter;
mod patterns;
mod prim;
mod runtime;
mod data;
mod assembly;
pub(crate) use assembly::*;
mod equality;
pub(crate) mod func;
pub(crate) use func::*;
pub(crate) mod ty;
pub(crate) use ty::*;
mod list;
mod stmts;
pub(crate) mod work;
pub(crate) use work::*;
mod display;
mod matrix;
mod ranges;
mod sums;
mod types_table;
mod value;

use collect::collect_binds;
use emitter::{HOLD_F64_POOL, HOLD_I32_POOL, HOLD_I64_POOL};
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
const F_EXIT_IMPORT: u32 = 2;
const F_PRINTLN_BLOCK: u32 = 3;
const F_EPRINTLN_BLOCK: u32 = 4;
const F_APPEND_COPY: u32 = 5;
const F_ITOA: u32 = 6;
const F_APPEND_I64: u32 = 7;
const F_APPEND_BOOL: u32 = 8;
const F_ALLOC: u32 = 9;
const F_INT_TO_STRING: u32 = 10;
const F_CONCAT: u32 = 11;
const F_STR_EQ: u32 = 12;
const F_LIST_GET_8: u32 = 13;
const F_LIST_GET_4: u32 = 14;
const F_LIST_PUSH_8: u32 = 15;
const F_LIST_PUSH_4: u32 = 16;
const F_LIST_JOIN: u32 = 17;
const F_BLOCK_COPY: u32 = 18;
const F_BUF_TO_BLOCK: u32 = 19;
const F_STR_LEN_CHARS: u32 = 20;
const F_SCAN_W64: u32 = 21;
const F_SCAN_W32: u32 = 22;
const F_SCAN_STR: u32 = 23;
const F_F16_TO_F64: u32 = 24;
const F_CP_OFF: u32 = 25;
const F_STR_SLICE: u32 = 26;
const F_STR_REPEAT: u32 = 27;
const F_STR_CMP: u32 = 28;
const F_STR_REPLACE: u32 = 29;
/// First program-function index; `main` sits after every program function.
const F_FN_BASE: u32 = 30;
/// Fixed type indices: 0 print(ptr,len)→(), 1 block-print(i32)→(),
/// 2 append_copy, 3 append_i64, 4 main ()→(), 5 (i32,i32)→i32
/// (append_bool/concat/str_eq), 6 (i64)→i32 (itoa/int_to_string),
/// 7 (i32)→i32 (alloc); program-function types start after.
const T_MAIN: u32 = 4;
/// (i32,i64)→i32: list_get_8 / list_push_8; 9: (i32)→i64 str_len_chars;
/// 10: (i32,i32,i32,i64)→i32 scan_w64; 11: (i32,i32,i32,i32)→i32 scan_w32/str;
/// 12: (i32)→f64 f16_to_f64; 13: (i32,i64)→i32 cp_off/str_repeat;
/// 14: (i32,i64,i64)→i32 str_slice.
const T_FN_BASE: u32 = 15;
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
/// Fixed runtime globals above; top-let globals start here.
const G_FIXED_COUNT: u32 = 4;

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
    /// Top-let vars (root + module) as WASM GLOBALS: VarId → (global
    /// index, slice type). Functions read them across function
    /// boundaries — the class main-local top-lets could never serve.
    pub(crate) globals: &'a HashMap<VarId, (u32, SliceTy)>,
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
    pub(crate) captures: Vec<(VarId, SliceTy, u32)>,
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

/// Emit a core wasm module for `ir`, or say precisely why not yet.
pub fn emit_program(ir: &IrProgram) -> Result<Vec<u8>, EmitError> {
    let Some(main) = ir.functions.iter().find(|f| f.name.as_str() == "main") else {
        return unsup("no main function");
    };
    // Program functions PLUS every linked module's functions — module fns
    // register under their QUALIFIED name ("url.encode_component"), which
    // is exactly the `CallTarget::Module` lookup key. A module carrying
    // top-level lets is excluded whole (its init order is a later slice).
    let program_fns = collect_program_fns(ir);
    let types = TypeTable::build(ir);

    let mut table =
        FnTable { by_name: HashMap::new(), impl_index: HashMap::new(), infos: Vec::new() };
    for (i, (f, qual)) in program_fns.iter().enumerate() {
        let (params, ret, refuse) = match fn_signature(f, &types) {
            Ok((p, r)) => (p, r, None),
            Err(reason) => (Vec::new(), None, Some(reason)),
        };
        let key = qual.clone().unwrap_or_else(|| f.name.as_str().to_string());
        // impl_index carries ONLY registry implementation symbols — a
        // global simple-name index over ALL module fns collides across
        // modules (two self-host modules both defining __len_loop made
        // cross_module fixtures call the WRONG module's helper).
        if qual.is_some() && registry_impl_names().contains(f.name.as_str()) {
            table.impl_index.insert(f.name.as_str().to_string(), i);
        }
        table.by_name.insert(key, i);
        table.infos.push(FnInfo { wasm_index: F_FN_BASE + i as u32, params, ret, refuse });
    }
    let main_index = F_FN_BASE + program_fns.len() as u32;

    let mut pool = Pool::new();
    // Interned eagerly so $append_bool can carry their fixed addresses.
    let true_base = pool.intern("true");
    let false_base = pool.intern("false");

    let (global_map, global_decls, init_lets) = build_globals(ir, &types);

    // Function-VALUE work shared by every lowering below (funcref table,
    // call_indirect types, lifted lambdas).
    let work = FnWork::default();
    work.itype_base.set(15 + table.infos.len() as u32);
    work.helper_base.set(F_FN_BASE + table.infos.len() as u32 + 1);

    // Lower every callable function; a body that doesn't lower yet is
    // recorded (not fatal) — fatal only if `main` can reach it.
    let mut lowered: Vec<Result<(Function, HashSet<usize>), String>> = Vec::new();
    for (i, (f, qual)) in program_fns.iter().enumerate() {
        if let Some(r) = &table.infos[i].refuse {
            lowered.push(Err(r.clone()));
            continue;
        }
        let params: Vec<(VarId, SliceTy)> =
            f.params.iter().zip(&table.infos[i].params).map(|(p, &t)| (p.var, t)).collect();
        let ctx = Ctx { table: &table, types: &types, work: &work, globals: &global_map };
        let cur_module = qual.as_ref().and_then(|q| q.split('.').next());
        let effect_raw = if f.is_effect {
            match slice_ty_of(&f.ret_ty, &types) {
                Some(SliceTy::Unit) => Some(SliceTy::Unit),
                // A declared-Result effect fn is SINGLE-layer (probe:
                // `wrap_sum(p)!` strips once to Int): the body yields the
                // Result value itself via ok()/err() — no wrap. Declared-
                // Option and raw-T bodies yield the raw value and wrap
                // (call sites are annotated Result[T?, E] / Result[T, E]).
                Some(SliceTy::Result(..)) => None,
                other => other,
            }
        } else {
            None
        };
        let plan = FnPlan {
            ret: table.infos[i].ret,
            cur_module: cur_module.map(str::to_string),
            effect_raw,
            in_main: false,
            env_captures: None,
        };
        match lower_fn(&params, plan, &f.body, &[], &ctx, &mut pool) {
            Ok(ok) => lowered.push(Ok(ok)),
            Err(EmitError::Unsupported(r)) => lowered.push(Err(r)),
        }
    }

    // `main`: top-lets as the eager prelude, then the body. Failure here is
    // fatal — main is always reachable.
    let ctx = Ctx { table: &table, types: &types, work: &work, globals: &global_map };
    let main_plan = FnPlan {
        ret: None,
        cur_module: None,
        effect_raw: None,
        in_main: true,
        env_captures: None,
    };
    let (main_fn, main_calls) =
        lower_fn(&[], main_plan, &main.body, &init_lets, &ctx, &mut pool)?;

    // Lift lambdas to extra functions (they may register further lambdas
    // or table entries — iterate to the fixed point).
    let mut lifted_fns: Vec<LoweredLifted> = Vec::new();
    loop {
        let pending: Vec<LiftedLambda> = {
            let all = work.lifted.borrow();
            all[lifted_fns.len()..].to_vec()
        };
        if pending.is_empty() {
            break;
        }
        for ll in pending {
            let plan = FnPlan {
                ret: ll.ret,
                cur_module: None,
                effect_raw: ll.effect_raw,
                in_main: false,
                env_captures: Some(ll.captures.clone()),
            };
            let (f, calls) = lower_fn(&ll.params, plan, &ll.body, &[], &ctx, &mut pool)?;
            // Uniform convention: env i32 leads every table signature.
            let mut ps: Vec<ValType> = vec![ValType::I32];
            ps.extend(ll.params.iter().map(|(_, t)| t.val_type()));
            lifted_fns.push((ps, ll.ret.map(SliceTy::val_type), f, calls));
        }
    }

    // Reachability: refuse the program iff a call chain from `main` lands
    // on a function whose body did not lower (its stub would trap).
    let mut queue: Vec<usize> = main_calls.iter().copied().collect();
    for (_, _, _, calls) in &lifted_fns {
        queue.extend(calls.iter().copied());
    }
    for e in work.entries.borrow().iter() {
        match e {
            TableEntry::Fn(i) | TableEntry::Adapter { target: i, .. } => queue.push(*i),
            TableEntry::Lambda(_) => {}
        }
    }
    let mut visited: HashSet<usize> = HashSet::new();
    while let Some(i) = queue.pop() {
        if !visited.insert(i) {
            continue;
        }
        match &lowered[i] {
            Err(reason) => return unsup(reason),
            Ok((_, calls)) => queue.extend(calls.iter().copied()),
        }
    }

    // Extra functions (ok-wrap adapters + lifted lambdas) resolve BEFORE
    // the type section is built — their call_indirect/type interning must
    // land inside it. Indices start right after main.
    let (extra_fns, entry_fn_indices) = resolve_extras(&table, &work, &lifted_fns);

    assemble_module(AssembleIn {
        table: &table,
        work: &work,
        pool: &pool,
        lowered: &lowered,
        main_fn: &main_fn,
        entry_fn_indices: &entry_fn_indices,
        extra_fns: &extra_fns,
        global_decls: &global_decls,
        main_index,
        true_base,
        false_base,
    })
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

/// Top-lets (root + module) as wasm globals + the dependency init order
/// (split from emit_program for the complexity budget).
#[allow(clippy::type_complexity)]
fn build_globals(
    ir: &IrProgram,
    types: &TypeTable,
) -> (HashMap<VarId, (u32, SliceTy)>, Vec<(VarId, SliceTy)>, Vec<IrTopLet>) {
    // Top-lets (root + module) become wasm globals — zero-initialized,
    // then set by main's prelude in DEPENDENCY order (C-077: the same
    // `dependency_init_order` the interp uses, so the order matches by
    // construction).
    let mut global_map: HashMap<VarId, (u32, SliceTy)> = HashMap::new();
    let mut global_decls: Vec<(VarId, SliceTy)> = Vec::new();
    {
        let mut next = G_FIXED_COUNT;
        let mut add = |tl: &IrTopLet,
                       map: &mut HashMap<VarId, (u32, SliceTy)>,
                       decls: &mut Vec<(VarId, SliceTy)>| {
            if let Some(sty) = slice_ty_of(&tl.ty, types) {
                map.insert(tl.var, (next, sty));
                decls.push((tl.var, sty));
                next += 1;
            }
            // An unsliceable top-let stays out of the pool; reading it
            // refuses at the var site with its own honest reason.
        };
        for tl in &ir.top_lets {
            add(tl, &mut global_map, &mut global_decls);
        }
        for m in &ir.modules {
            for tl in &m.top_lets {
                add(tl, &mut global_map, &mut global_decls);
            }
        }
    }
    let init_order: Vec<VarId> = {
        use almide_ir::top_let_storage::{
            build_global_tables, dependency_init_order, top_let_inputs,
        };
        let mut inputs = Vec::new();
        for tl in &ir.top_lets {
            inputs.push(top_let_inputs(tl));
        }
        for m in &ir.modules {
            for tl in &m.top_lets {
                inputs.push(top_let_inputs(tl));
            }
        }
        let (_info, alias, _off) = build_global_tables(&inputs, &ir.var_table);
        dependency_init_order(ir, &alias)
    };
    let mut init_by_var: HashMap<VarId, &IrTopLet> = HashMap::new();
    for tl in &ir.top_lets {
        init_by_var.insert(tl.var, tl);
    }
    for m in &ir.modules {
        for tl in &m.top_lets {
            init_by_var.insert(tl.var, tl);
        }
    }
    let init_lets: Vec<IrTopLet> =
        init_order.iter().filter_map(|v| init_by_var.get(v).map(|tl| (*tl).clone())).collect();
    (global_map, global_decls, init_lets)
}


/// The callable-fn flattening (entry fns + module fns under qualified
/// names, Hole-bodied surfaces excluded) — split from emit_program.
fn collect_program_fns(ir: &IrProgram) -> Vec<(&IrFunction, Option<String>)> {
    let mut program_fns: Vec<(&IrFunction, Option<String>)> = ir
        .functions
        .iter()
        .filter(|f| !f.is_test && f.name.as_str() != "main")
        .map(|f| (f, None))
        .collect();
    for m in &ir.modules {
        for f in &m.functions {
            // A Hole body is a bodyless SURFACE decl (`= _`) — a bridge
            // boundary, not an implementation. Registering it would
            // shadow the self-host registry's real implementation with
            // an unlowersble stub (found by the burn-up: expr:Hole ×70).
            let is_surface = matches!(f.body.kind, IrExprKind::Hole);
            if !f.is_test && !is_surface {
                program_fns
                    .push((f, Some(format!("{}.{}", m.name.as_str(), f.name.as_str()))));
            }
        }
    }

    // Signature table first: call sites need indices and types up front.
    program_fns
}

/// Emitted helpers + table-entry extras (shims, adapters, lifted
/// lambdas) — split from emit_program for the complexity budget.
fn resolve_extras(
    table: &FnTable,
    work: &FnWork,
    lifted_fns: &[LoweredLifted],
) -> (Vec<(u32, Function)>, Vec<u32>) {
    let extra_base = F_FN_BASE + table.infos.len() as u32 + 1;
    let mut extra_fns: Vec<(u32, Function)> = Vec::new();
    // Emitted helpers assemble FIRST — their indices were promised during
    // lowering; the table-entry extras follow.
    let helper_snapshot: Vec<Helper> = work.helpers.borrow().clone();
    for h in &helper_snapshot {
        let params = match h {
            Helper::ValueKeys => vec![ValType::I32],
            _ => vec![ValType::I32, ValType::I32],
        };
        let ti = work.itype(params, Some(ValType::I32));
        let f = match h {
            Helper::JsonValue { float_to_string, frags } => value::emit_json_value_helper(
                work.helper_base.get(),
                &helper_snapshot,
                *float_to_string,
                *frags,
            ),
            Helper::JsonQuote { frags } => value::emit_json_quote_helper(*frags),
            Helper::ValueField => value::emit_value_field_helper(),
            Helper::ValueKeys => value::emit_value_keys_helper(),
            Helper::StringSplit => value::emit_string_split_helper(),
        };
        extra_fns.push((ti, f));
    }
    let mut entry_fn_indices: Vec<u32> = Vec::new();
    for e in work.entries.borrow().clone() {
        match e {
            TableEntry::Fn(i) => {
                // Uniform closure convention: env leads — a plain fn's
                // table slot holds a forwarding shim.
                let info = &table.infos[i];
                let mut ps: Vec<ValType> = vec![ValType::I32];
                ps.extend(info.params.iter().map(|t| t.val_type()));
                let ti = work.itype(ps, info.ret.map(SliceTy::val_type));
                let idx = extra_base + extra_fns.len() as u32;
                extra_fns.push((
                    ti,
                    emit_env_shim(F_FN_BASE + i as u32, &info.params, info.ret, false),
                ));
                entry_fn_indices.push(idx);
            }
            TableEntry::Adapter { target, raw } => {
                let info = &table.infos[target];
                let mut ps: Vec<ValType> = vec![ValType::I32];
                ps.extend(info.params.iter().map(|t| t.val_type()));
                let ti = work.itype(ps, Some(ValType::I32));
                let idx = extra_base + extra_fns.len() as u32;
                let _ = raw;
                extra_fns.push((
                    ti,
                    emit_env_shim(F_FN_BASE + target as u32, &info.params, info.ret, true),
                ));
                entry_fn_indices.push(idx);
            }
            TableEntry::Lambda(j) => {
                let (ps, r, f, _) = &lifted_fns[j as usize];
                let ti = work.itype(ps.clone(), *r);
                let idx = extra_base + extra_fns.len() as u32;
                extra_fns.push((ti, f.clone()));
                entry_fn_indices.push(idx);
            }
        }
    }
    (extra_fns, entry_fn_indices)
}