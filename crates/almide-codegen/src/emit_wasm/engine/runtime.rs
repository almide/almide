//! Engine runtime — the minimal set of support functions that abstract
//! WasmIR ops resolve to, plus string primitives.
//!
//! These are themselves expressed in WasmIR (`WasmFunc`) and pass the same
//! stack-effect verifier as user code — there is no hand-written, unverified
//! runtime in the v2 engine. The module builder places them at fixed indices
//! (`0..COUNT`) before any user function, then `resolve_abstract_ops` rewrites
//! `Op::Alloc` / `Op::RcInc` / `Op::RcDec` / `Op::StringConcat` into `Op::Call`
//! targeting them.
//!
//! ## Allocator
//!
//! A bump allocator over a single linear memory. `__alloc(data_size)` reserves
//! `header(8) + data_size` bytes, writes the alloc header
//! (`[size @ base][rc=1 @ base+4]`), and returns the *data* pointer `base + 8`.
//!
//! ## RC and string literals
//!
//! String literals live in the data segment *below* `heap_start` and have no
//! alloc header. `__rc_inc` / `__rc_dec` therefore guard on `ptr >= heap_start`
//! and skip data-segment pointers entirely. Phase 2b still never frees:
//! `__rc_dec` only keeps the count accurate.

use super::ir::{Op, Const, WasmTy, WasmFunc, FuncIdx, BinOp as B, UnOp as U, LoadKind, StoreKind};
use super::layout::{self, LayoutRegistry, alloc};

/// Index of the mutable i32 global holding the bump pointer (next free byte).
pub const HEAP_GLOBAL: u32 = 0;

/// Number of imported functions occupying the lowest function indices. The
/// WASI `fd_write` import is always present at index 0, so every defined
/// runtime/user function is shifted up by this amount.
pub const IMPORT_COUNT: u32 = 1;

/// Resolved indices of the runtime functions. Defined functions follow the
/// imports, so their indices are offset by `IMPORT_COUNT`.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeFns {
    /// The wasi_snapshot_preview1.fd_write import (function index 0).
    pub fd_write: FuncIdx,
    pub alloc: FuncIdx,
    pub rc_inc: FuncIdx,
    pub rc_dec: FuncIdx,
    pub string_concat: FuncIdx,
    pub strlen: FuncIdx,
    pub byte_at: FuncIdx,
    pub int_to_string: FuncIdx,
    pub string_eq: FuncIdx,
    pub range: FuncIdx,
    pub list_concat: FuncIdx,
    pub starts_with: FuncIdx,
    pub ends_with: FuncIdx,
    pub string_slice: FuncIdx,
    pub string_get: FuncIdx,
    pub list_sort_int: FuncIdx,
    // Map[Int, Int] (Swiss-table-style, linear probing). entry = 16B (key i64,
    // val i64). functional `set` rebuilds (no in-place resize/tombstones).
    pub map_new: FuncIdx,
    pub map_put: FuncIdx,   // internal in-place insert (no resize)
    pub map_set: FuncIdx,   // functional set (rebuild)
    pub map_get: FuncIdx,
    pub map_get_or: FuncIdx,
    pub map_contains: FuncIdx,
    pub map_len: FuncIdx,
    pub map_hash: FuncIdx,
    pub map_key_eq: FuncIdx,
    pub map_collect: FuncIdx, // keys/values into a List
    pub map_remove: FuncIdx,
    pub map_merge: FuncIdx,
    pub print: FuncIdx,
    pub println: FuncIdx,
    pub to_case: FuncIdx,
    pub str_repeat: FuncIdx,
    pub str_contains: FuncIdx,
    pub float_to_string: FuncIdx,
    pub str_trim: FuncIdx,
    pub str_find_byte: FuncIdx,
    pub str_index_of: FuncIdx,
    pub str_last_index_of: FuncIdx,
    pub str_replace: FuncIdx,
    pub str_sub_bytes: FuncIdx,
    pub str_split: FuncIdx,
    pub str_join: FuncIdx,
    pub int_parse: FuncIdx,
    pub str_lines: FuncIdx,
    pub str_cmp: FuncIdx,
}

/// The number of *defined* runtime functions (they occupy function indices
/// `IMPORT_COUNT .. IMPORT_COUNT + COUNT`).
pub const COUNT: u32 = 44;

impl RuntimeFns {
    /// Defined runtime functions in code-section order, offset past the imports.
    pub const fn fixed() -> Self {
        RuntimeFns {
            fd_write: 0,
            alloc: IMPORT_COUNT,
            rc_inc: IMPORT_COUNT + 1,
            rc_dec: IMPORT_COUNT + 2,
            string_concat: IMPORT_COUNT + 3,
            strlen: IMPORT_COUNT + 4,
            byte_at: IMPORT_COUNT + 5,
            int_to_string: IMPORT_COUNT + 6,
            string_eq: IMPORT_COUNT + 7,
            range: IMPORT_COUNT + 8,
            list_concat: IMPORT_COUNT + 9,
            starts_with: IMPORT_COUNT + 10,
            ends_with: IMPORT_COUNT + 11,
            string_slice: IMPORT_COUNT + 12,
            string_get: IMPORT_COUNT + 13,
            list_sort_int: IMPORT_COUNT + 14,
            map_new: IMPORT_COUNT + 15,
            map_put: IMPORT_COUNT + 16,
            map_set: IMPORT_COUNT + 17,
            map_get: IMPORT_COUNT + 18,
            map_get_or: IMPORT_COUNT + 19,
            map_contains: IMPORT_COUNT + 20,
            map_len: IMPORT_COUNT + 21,
            map_hash: IMPORT_COUNT + 22,
            map_key_eq: IMPORT_COUNT + 23,
            map_collect: IMPORT_COUNT + 24,
            map_remove: IMPORT_COUNT + 25,
            map_merge: IMPORT_COUNT + 26,
            print: IMPORT_COUNT + 27,
            println: IMPORT_COUNT + 28,
            to_case: IMPORT_COUNT + 29,
            str_repeat: IMPORT_COUNT + 30,
            str_contains: IMPORT_COUNT + 31,
            float_to_string: IMPORT_COUNT + 32,
            str_trim: IMPORT_COUNT + 33,
            str_find_byte: IMPORT_COUNT + 34,
            str_index_of: IMPORT_COUNT + 35,
            str_last_index_of: IMPORT_COUNT + 36,
            str_replace: IMPORT_COUNT + 37,
            str_sub_bytes: IMPORT_COUNT + 38,
            str_split: IMPORT_COUNT + 39,
            str_join: IMPORT_COUNT + 40,
            int_parse: IMPORT_COUNT + 41,
            str_lines: IMPORT_COUNT + 42,
            str_cmp: IMPORT_COUNT + 43,
        }
    }

    /// Map of runtime function names to indices, for the build's name lookup.
    pub fn name_table(&self) -> [(&'static str, FuncIdx); 44] {
        [
            ("__alloc", self.alloc),
            ("__rc_inc", self.rc_inc),
            ("__rc_dec", self.rc_dec),
            ("__string_concat", self.string_concat),
            ("__strlen", self.strlen),
            ("__byte_at", self.byte_at),
            ("__int_to_string", self.int_to_string),
            ("__string_eq", self.string_eq),
            ("__range", self.range),
            ("__list_concat", self.list_concat),
            ("__string_starts_with", self.starts_with),
            ("__string_ends_with", self.ends_with),
            ("__string_slice", self.string_slice),
            ("__string_get", self.string_get),
            ("__list_sort_int", self.list_sort_int),
            ("__map_new", self.map_new),
            ("__map_put", self.map_put),
            ("__map_set", self.map_set),
            ("__map_get", self.map_get),
            ("__map_get_or", self.map_get_or),
            ("__map_contains", self.map_contains),
            ("__map_len", self.map_len),
            ("__map_hash", self.map_hash),
            ("__map_key_eq", self.map_key_eq),
            ("__map_collect", self.map_collect),
            ("__map_remove", self.map_remove),
            ("__map_merge", self.map_merge),
            ("__print", self.print),
            ("__println", self.println),
            ("__string_to_case", self.to_case),
            ("__string_repeat", self.str_repeat),
            ("__string_contains", self.str_contains),
            ("__float_to_string", self.float_to_string),
            ("__string_trim", self.str_trim),
            ("__string_find_byte", self.str_find_byte),
            ("__string_index_of", self.str_index_of),
            ("__string_last_index_of", self.str_last_index_of),
            ("__string_replace", self.str_replace),
            ("__string_sub_bytes", self.str_sub_bytes),
            ("__string_split", self.str_split),
            ("__string_join", self.str_join),
            ("__int_parse", self.int_parse),
            ("__string_lines", self.str_lines),
            ("__str_cmp", self.str_cmp),
        ]
    }
}

/// Build the runtime functions as verified WasmIR, in index order.
/// `heap_start` is the first heap byte (everything below it is the immutable
/// data segment) — baked into the RC guard.
pub fn runtime_funcs(reg: &LayoutRegistry, heap_start: i32) -> Vec<WasmFunc> {
    vec![
        build_alloc(reg),
        build_rc_inc(reg, heap_start),
        build_rc_dec(reg, heap_start),
        build_string_concat(),
        build_strlen(),
        build_byte_at(),
        build_int_to_string(),
        build_string_eq(),
        build_range(),
        build_list_concat(),
        build_prefix_cmp("__string_starts_with", false),
        build_prefix_cmp("__string_ends_with", true),
        build_string_slice(),
        build_string_get(),
        build_list_sort_int(),
        build_map_new(),
        build_map_put(),
        build_map_set(),
        build_map_get(),
        build_map_get_or(),
        build_map_contains(),
        build_map_len(),
        build_map_hash(),
        build_map_key_eq(),
        build_map_collect(),
        build_map_remove(),
        build_map_merge(),
        build_print(),
        build_println(),
        build_to_case(),
        build_str_repeat(),
        build_str_contains(),
        build_float_to_string(),
        build_string_trim(),
        build_string_find_byte(),
        build_string_index_of(),
        build_string_last_index_of(),
        build_string_replace(),
        build_string_sub_bytes(),
        build_string_split(),
        build_string_join(),
        build_int_parse(),
        build_string_lines(),
        build_str_cmp(),
    ]
}

/// `__str_cmp(a, b) -> i32` — lexicographic byte comparison of two strings.
/// Returns negative if a < b, 0 if equal, positive if a > b (memcmp-style):
/// compare min(a.len, b.len) bytes; on the first differing byte return its
/// difference, else return a.len - b.len. Matches the legacy emitter so the
/// differential gate stays green. String layout: [bytelen:i32@0][cap@4][bytes@8].
fn build_str_cmp() -> WasmFunc {
    const A: u32 = 0;
    const BP: u32 = 1;
    const MINL: u32 = 2;
    const IDX: u32 = 3;
    const CA: u32 = 4;
    const CB: u32 = 5;

    let loop_body = vec![
        // idx >= minl → break
        Op::LocalGet(IDX), Op::LocalGet(MINL), Op::BinOp(B::I32GeU), Op::BrIf(1),
        // ca = a[8+idx]
        Op::LocalGet(A), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(IDX), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8), Op::LocalSet(CA),
        // cb = b[8+idx]
        Op::LocalGet(BP), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(IDX), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8), Op::LocalSet(CB),
        // if ca != cb { return ca - cb }
        Op::LocalGet(CA), Op::LocalGet(CB), Op::BinOp(B::I32Ne),
        Op::IfVoid { then: vec![Op::LocalGet(CA), Op::LocalGet(CB), Op::BinOp(B::I32Sub), Op::Return], else_: vec![] },
        Op::LocalGet(IDX), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(IDX), Op::Br(0),
    ];
    let body = vec![
        // minl = min(a.len, b.len)
        Op::LocalGet(A), Op::Load(LoadKind::I32),
        Op::LocalGet(BP), Op::Load(LoadKind::I32),
        Op::BinOp(B::I32LeU),
        Op::If { ty: WasmTy::I32,
            then: vec![Op::LocalGet(A), Op::Load(LoadKind::I32)],
            else_: vec![Op::LocalGet(BP), Op::Load(LoadKind::I32)] },
        Op::LocalSet(MINL),
        Op::Const(Const::I32(0)), Op::LocalSet(IDX),
        Op::Block(vec![Op::Loop(loop_body)]),
        // equal prefix → return a.len - b.len
        Op::LocalGet(A), Op::Load(LoadKind::I32),
        Op::LocalGet(BP), Op::Load(LoadKind::I32),
        Op::BinOp(B::I32Sub),
    ];
    WasmFunc {
        name: "__str_cmp".into(), params: vec![WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // minl, idx, ca, cb
        body,
    }
}

/// `__string_to_case(s, upper: i32) -> i32` — ASCII case conversion (non-ASCII
/// bytes pass through). upper != 0 → uppercase, else lowercase.
fn build_to_case() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    const UPPER: u32 = 1;
    const BL: u32 = 2;
    const OUT: u32 = 3;
    const I: u32 = 4;
    const LO: u32 = 5;
    const DELTA: u32 = 6;
    const B: u32 = 7;

    let loop_body = vec![
        Op::LocalGet(I), Op::LocalGet(BL), Op::BinOp(B::I32GeU), Op::BrIf(1),
        // b = s[8+i]
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(I), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8), Op::LocalSet(B),
        // out[8+i] = ((b-lo) <u 26) ? b+delta : b
        Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(I), Op::BinOp(B::I32Add),
        Op::LocalGet(B), Op::LocalGet(LO), Op::BinOp(B::I32Sub), Op::Const(Const::I32(26)), Op::BinOp(B::I32LtU),
        Op::If {
            ty: WasmTy::I32,
            then: vec![Op::LocalGet(B), Op::LocalGet(DELTA), Op::BinOp(B::I32Add)],
            else_: vec![Op::LocalGet(B)],
        },
        Op::Store(StoreKind::I8),
        Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(I), Op::Br(0),
    ];

    let body = vec![
        Op::LocalGet(S), Op::Load(LoadKind::I32), Op::LocalSet(BL),
        // out = alloc(8 + bl) ; out.len = out.cap = bl
        Op::Const(Const::I32(8)), Op::LocalGet(BL), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::LocalGet(BL), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(BL), Op::Store(StoreKind::I32),
        // upper: lo='a'(97), delta=-32 ; lower: lo='A'(65), delta=+32
        Op::LocalGet(UPPER),
        Op::If { ty: WasmTy::I32, then: vec![Op::Const(Const::I32(97))], else_: vec![Op::Const(Const::I32(65))] },
        Op::LocalSet(LO),
        Op::LocalGet(UPPER),
        Op::If { ty: WasmTy::I32, then: vec![Op::Const(Const::I32(-32))], else_: vec![Op::Const(Const::I32(32))] },
        Op::LocalSet(DELTA),
        Op::Const(Const::I32(0)), Op::LocalSet(I),
        Op::Block(vec![Op::Loop(loop_body)]),
        Op::LocalGet(OUT),
    ];
    WasmFunc {
        name: "__string_to_case".into(), params: vec![WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // bl,out,i,lo,delta,b
        body,
    }
}

/// `__string_repeat(s, n: i64) -> i32` — `s` concatenated `n` times (n<=0 → "").
fn build_str_repeat() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    const N: u32 = 1;  // i64
    const BL: u32 = 2;
    const NI: u32 = 3; // n as i32, clamped
    const TOTAL: u32 = 4;
    const OUT: u32 = 5;
    const K: u32 = 6;

    let loop_body = vec![
        Op::LocalGet(K), Op::LocalGet(NI), Op::BinOp(B::I32GeU), Op::BrIf(1),
        // memcpy(out + 8 + k*bl, s + 8, bl)
        Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(K), Op::LocalGet(BL), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(BL),
        Op::MemoryCopy,
        Op::LocalGet(K), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(K), Op::Br(0),
    ];
    let body = vec![
        Op::LocalGet(S), Op::Load(LoadKind::I32), Op::LocalSet(BL),
        // ni = n < 0 ? 0 : wrap(n)
        Op::LocalGet(N), Op::Const(Const::I64(0)), Op::BinOp(B::I64LtS),
        Op::If { ty: WasmTy::I32, then: vec![Op::Const(Const::I32(0))], else_: vec![Op::LocalGet(N), Op::UnOp(U::I32WrapI64)] },
        Op::LocalSet(NI),
        // total = bl * ni
        Op::LocalGet(BL), Op::LocalGet(NI), Op::BinOp(B::I32Mul), Op::LocalSet(TOTAL),
        Op::Const(Const::I32(8)), Op::LocalGet(TOTAL), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::LocalGet(TOTAL), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(TOTAL), Op::Store(StoreKind::I32),
        Op::Const(Const::I32(0)), Op::LocalSet(K),
        Op::Block(vec![Op::Loop(loop_body)]),
        Op::LocalGet(OUT),
    ];
    WasmFunc {
        name: "__string_repeat".into(), params: vec![WasmTy::I32, WasmTy::I64], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // bl,ni,total,out,k
        body,
    }
}

/// `__string_contains(s, sub) -> i32` — naive substring search.
fn build_str_contains() -> WasmFunc {
    const S: u32 = 0;
    const SUB: u32 = 1;
    const SL: u32 = 2;
    const SUBL: u32 = 3;
    const START: u32 = 4;
    const J: u32 = 5;
    const MATCHED: u32 = 6;

    // inner: compare subl bytes at `start`
    let mut inner = vec![
        Op::LocalGet(J), Op::LocalGet(SUBL), Op::BinOp(B::I32GeU), Op::BrIf(1),
        // if s[8+start+j] != sub[8+j] { matched=0; break inner }
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(START), Op::BinOp(B::I32Add), Op::LocalGet(J), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
        Op::LocalGet(SUB), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(J), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
        Op::BinOp(B::I32Ne),
        Op::IfVoid { then: vec![Op::Const(Const::I32(0)), Op::LocalSet(MATCHED), Op::Br(2)], else_: vec![] },
        Op::LocalGet(J), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(J), Op::Br(0),
    ];
    let mut outer = vec![
        // start > sl - subl → break outer (not found)
        Op::LocalGet(START), Op::LocalGet(SL), Op::LocalGet(SUBL), Op::BinOp(B::I32Sub), Op::BinOp(B::I32GtU), Op::BrIf(1),
        Op::Const(Const::I32(1)), Op::LocalSet(MATCHED),
        Op::Const(Const::I32(0)), Op::LocalSet(J),
        Op::Block(vec![Op::Loop(std::mem::take(&mut inner))]),
        // if matched: return 1
        Op::LocalGet(MATCHED), Op::IfVoid { then: vec![Op::Const(Const::I32(1)), Op::Return], else_: vec![] },
        Op::LocalGet(START), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(START), Op::Br(0),
    ];
    let body = vec![
        Op::LocalGet(S), Op::Load(LoadKind::I32), Op::LocalSet(SL),
        Op::LocalGet(SUB), Op::Load(LoadKind::I32), Op::LocalSet(SUBL),
        // if subl > sl: return 0
        Op::LocalGet(SUBL), Op::LocalGet(SL), Op::BinOp(B::I32GtU),
        Op::IfVoid { then: vec![Op::Const(Const::I32(0)), Op::Return], else_: vec![] },
        Op::Const(Const::I32(0)), Op::LocalSet(START),
        Op::Block(vec![Op::Loop(std::mem::take(&mut outer))]),
        Op::Const(Const::I32(0)), // not found
    ];
    WasmFunc {
        name: "__string_contains".into(), params: vec![WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // sl,subl,start,j,matched
        body,
    }
}

/// `__string_trim(s, mode: i32) -> i32` — strip ASCII whitespace. `mode & 1`
/// trims the start, `mode & 2` trims the end (trim = 3, trim_start = 1,
/// trim_end = 2). Whitespace = space or bytes 9..=13 (tab/LF/VT/FF/CR).
fn build_string_trim() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    const MODE: u32 = 1;
    const BL: u32 = 2;
    const START: u32 = 3;
    const END: u32 = 4;
    const NEWLEN: u32 = 5;
    const OUT: u32 = 6;
    const B: u32 = 7;

    // is_ws(B): (B == 32) | ((B - 9) <u 5)
    let is_ws = vec![
        Op::LocalGet(B), Op::Const(Const::I32(32)), Op::BinOp(B::I32Eq),
        Op::LocalGet(B), Op::Const(Const::I32(9)), Op::BinOp(B::I32Sub), Op::Const(Const::I32(5)), Op::BinOp(B::I32LtU),
        Op::BinOp(B::I32Or),
    ];
    let mut start_loop = vec![
        Op::LocalGet(START), Op::LocalGet(END), Op::BinOp(B::I32GeU), Op::BrIf(1),
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(START), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8), Op::LocalSet(B),
    ];
    start_loop.extend(is_ws.clone());
    start_loop.extend(vec![
        Op::UnOp(U::I32Eqz), Op::BrIf(1),
        Op::LocalGet(START), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(START), Op::Br(0),
    ]);
    let mut end_loop = vec![
        Op::LocalGet(END), Op::LocalGet(START), Op::BinOp(B::I32GtU), Op::UnOp(U::I32Eqz), Op::BrIf(1),
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(END), Op::BinOp(B::I32Add), Op::Const(Const::I32(1)), Op::BinOp(B::I32Sub), Op::Load(LoadKind::U8), Op::LocalSet(B),
    ];
    end_loop.extend(is_ws);
    end_loop.extend(vec![
        Op::UnOp(U::I32Eqz), Op::BrIf(1),
        Op::LocalGet(END), Op::Const(Const::I32(1)), Op::BinOp(B::I32Sub), Op::LocalSet(END), Op::Br(0),
    ]);

    let body = vec![
        Op::LocalGet(S), Op::Load(LoadKind::I32), Op::LocalSet(BL),
        Op::Const(Const::I32(0)), Op::LocalSet(START),
        Op::LocalGet(BL), Op::LocalSet(END),
        Op::LocalGet(MODE), Op::Const(Const::I32(1)), Op::BinOp(B::I32And),
        Op::IfVoid { then: vec![Op::Block(vec![Op::Loop(start_loop)])], else_: vec![] },
        Op::LocalGet(MODE), Op::Const(Const::I32(2)), Op::BinOp(B::I32And),
        Op::IfVoid { then: vec![Op::Block(vec![Op::Loop(end_loop)])], else_: vec![] },
        // newlen = end - start
        Op::LocalGet(END), Op::LocalGet(START), Op::BinOp(B::I32Sub), Op::LocalSet(NEWLEN),
        Op::Const(Const::I32(8)), Op::LocalGet(NEWLEN), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::LocalGet(NEWLEN), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(NEWLEN), Op::Store(StoreKind::I32),
        // memcpy(out + 8, s + 8 + start, newlen)
        Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(START), Op::BinOp(B::I32Add),
        Op::LocalGet(NEWLEN),
        Op::MemoryCopy,
        Op::LocalGet(OUT),
    ];
    WasmFunc {
        name: "__string_trim".into(), params: vec![WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // bl,start,end,newlen,out,b
        body,
    }
}

/// `__string_find_byte(s, sub, from: i32) -> i32` — lowest byte offset `>= from`
/// at which `sub` occurs, or `-1`. Empty needle matches at `from`. Shared search
/// primitive behind index_of / last_index_of / replace / split.
fn build_string_find_byte() -> WasmFunc {
    const S: u32 = 0;
    const SUB: u32 = 1;
    const FROM: u32 = 2;
    const SL: u32 = 3;
    const SUBL: u32 = 4;
    const START: u32 = 5;
    const J: u32 = 6;
    const MATCHED: u32 = 7;

    let inner = vec![
        Op::LocalGet(J), Op::LocalGet(SUBL), Op::BinOp(B::I32GeU), Op::BrIf(1),
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(START), Op::BinOp(B::I32Add), Op::LocalGet(J), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
        Op::LocalGet(SUB), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(J), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
        Op::BinOp(B::I32Ne),
        Op::IfVoid { then: vec![Op::Const(Const::I32(0)), Op::LocalSet(MATCHED), Op::Br(2)], else_: vec![] },
        Op::LocalGet(J), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(J), Op::Br(0),
    ];
    let outer = vec![
        Op::LocalGet(START), Op::LocalGet(SL), Op::LocalGet(SUBL), Op::BinOp(B::I32Sub), Op::BinOp(B::I32GtU), Op::BrIf(1),
        Op::Const(Const::I32(1)), Op::LocalSet(MATCHED),
        Op::Const(Const::I32(0)), Op::LocalSet(J),
        Op::Block(vec![Op::Loop(inner)]),
        Op::LocalGet(MATCHED), Op::IfVoid { then: vec![Op::LocalGet(START), Op::Return], else_: vec![] },
        Op::LocalGet(START), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(START), Op::Br(0),
    ];
    let body = vec![
        Op::LocalGet(S), Op::Load(LoadKind::I32), Op::LocalSet(SL),
        Op::LocalGet(SUB), Op::Load(LoadKind::I32), Op::LocalSet(SUBL),
        // empty needle → match at `from`
        Op::LocalGet(SUBL), Op::UnOp(U::I32Eqz),
        Op::IfVoid { then: vec![Op::LocalGet(FROM), Op::Return], else_: vec![] },
        // needle longer than haystack → not found
        Op::LocalGet(SUBL), Op::LocalGet(SL), Op::BinOp(B::I32GtU),
        Op::IfVoid { then: vec![Op::Const(Const::I32(-1)), Op::Return], else_: vec![] },
        Op::LocalGet(FROM), Op::LocalSet(START),
        Op::Block(vec![Op::Loop(outer)]),
        Op::Const(Const::I32(-1)),
    ];
    WasmFunc {
        name: "__string_find_byte".into(), params: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // sl,subl,start,j,matched
        body,
    }
}

/// Count UTF-8 code-point boundaries in `s[0, upto_byte)`, leaving the count on
/// the stack. Boundary = `(byte & 0xC0) != 0x80`.
fn cp_count_ops(s: u32, upto: u32, cp: u32, b: u32) -> Vec<Op> {
    let loop_body = vec![
        Op::LocalGet(b), Op::LocalGet(upto), Op::BinOp(B::I32GeU), Op::BrIf(1),
        Op::LocalGet(cp),
        Op::LocalGet(s), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(b), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
        Op::Const(Const::I32(0xC0)), Op::BinOp(B::I32And), Op::Const(Const::I32(0x80)), Op::BinOp(B::I32Ne),
        Op::BinOp(B::I32Add), Op::LocalSet(cp),
        Op::LocalGet(b), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(b), Op::Br(0),
    ];
    vec![
        Op::Const(Const::I32(0)), Op::LocalSet(cp),
        Op::Const(Const::I32(0)), Op::LocalSet(b),
        Op::Block(vec![Op::Loop(loop_body)]),
        Op::LocalGet(cp),
    ]
}

/// `__string_index_of(s, sub) -> i32` — `Some(cp_index)` of the first match, or
/// `None`. The payload is a code-point index (consistent with `string.slice`).
fn build_string_index_of() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    const SUB: u32 = 1;
    const POS: u32 = 2;  // byte offset of match, or -1
    const OPT: u32 = 3;
    const CP: u32 = 4;
    const BC: u32 = 5;    // byte cursor for cp count

    let mut body = vec![
        Op::Const(Const::I32(12)), Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OPT),
        Op::LocalGet(OPT), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
        Op::LocalGet(S), Op::LocalGet(SUB), Op::Const(Const::I32(0)),
        Op::Call { idx: rt.str_find_byte, pops: 3, pushes: 1 }, Op::LocalSet(POS),
        // pos < 0 → None
        Op::LocalGet(POS), Op::Const(Const::I32(0)), Op::BinOp(B::I32LtS),
        Op::IfVoid { then: vec![Op::LocalGet(OPT), Op::Return], else_: vec![] },
    ];
    // cp = code points in [0, pos)
    body.extend(cp_count_ops(S, POS, CP, BC));
    body.push(Op::LocalSet(CP));
    body.extend(vec![
        Op::LocalGet(OPT), Op::Const(Const::I32(1)), Op::Store(StoreKind::I32),
        Op::LocalGet(OPT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        Op::LocalGet(CP), Op::UnOp(U::I64ExtendI32S), Op::Store(StoreKind::I64),
        Op::LocalGet(OPT),
    ]);
    WasmFunc {
        name: "__string_index_of".into(), params: vec![WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // pos,opt,cp,bc
        body,
    }
}

/// `__string_last_index_of(s, sub) -> i32` — `Some(cp_index)` of the last match,
/// or `None`. Repeated forward search keeping the highest match offset.
fn build_string_last_index_of() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    const SUB: u32 = 1;
    const POS: u32 = 2;   // last match byte offset, or -1
    const CUR: u32 = 3;   // current search result
    const OPT: u32 = 4;
    const CP: u32 = 5;
    const BC: u32 = 6;

    let scan_loop = vec![
        // cur = find_byte(s, sub, cur + 1)? — first iteration uses cur=find(.,0)
        Op::LocalGet(CUR), Op::Const(Const::I32(0)), Op::BinOp(B::I32LtS), Op::BrIf(1), // cur<0 → stop
        Op::LocalGet(CUR), Op::LocalSet(POS),
        Op::LocalGet(S), Op::LocalGet(SUB), Op::LocalGet(CUR), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.str_find_byte, pops: 3, pushes: 1 }, Op::LocalSet(CUR),
        Op::Br(0),
    ];
    let mut body = vec![
        Op::Const(Const::I32(12)), Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OPT),
        Op::LocalGet(OPT), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
        Op::Const(Const::I32(-1)), Op::LocalSet(POS),
        Op::LocalGet(S), Op::LocalGet(SUB), Op::Const(Const::I32(0)),
        Op::Call { idx: rt.str_find_byte, pops: 3, pushes: 1 }, Op::LocalSet(CUR),
        Op::Block(vec![Op::Loop(scan_loop)]),
        // pos < 0 → None
        Op::LocalGet(POS), Op::Const(Const::I32(0)), Op::BinOp(B::I32LtS),
        Op::IfVoid { then: vec![Op::LocalGet(OPT), Op::Return], else_: vec![] },
    ];
    body.extend(cp_count_ops(S, POS, CP, BC));
    body.push(Op::LocalSet(CP));
    body.extend(vec![
        Op::LocalGet(OPT), Op::Const(Const::I32(1)), Op::Store(StoreKind::I32),
        Op::LocalGet(OPT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        Op::LocalGet(CP), Op::UnOp(U::I64ExtendI32S), Op::Store(StoreKind::I64),
        Op::LocalGet(OPT),
    ]);
    WasmFunc {
        name: "__string_last_index_of".into(), params: vec![WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // pos,cur,opt,cp,bc
        body,
    }
}

/// `__string_replace(s, from, to, all: i32) -> i32` — replace occurrences of
/// `from` with `to`. `all != 0` replaces every (non-overlapping) match, else
/// only the first. Empty `from` is a no-op (returns a copy of `s`). Two passes:
/// count matches to size the result, then copy segments + replacements.
fn build_string_replace() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    const FROM: u32 = 1;
    const TO: u32 = 2;
    const ALL: u32 = 3;
    const SL: u32 = 4;
    const FROML: u32 = 5;
    const TOL: u32 = 6;
    const COUNT: u32 = 7;
    const POS: u32 = 8;
    const NEWLEN: u32 = 9;
    const OUT: u32 = 10;
    const SRC: u32 = 11;
    const DST: u32 = 12;
    const REM: u32 = 13;
    const M: u32 = 14;
    const SEG: u32 = 15;

    // out + 8 + DST  (current write address)
    let dst_addr = || vec![
        Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(DST), Op::BinOp(B::I32Add),
    ];
    // s + 8 + SRC  (current read address)
    let src_addr = || vec![
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(SRC), Op::BinOp(B::I32Add),
    ];

    let count_loop = vec![
        Op::LocalGet(S), Op::LocalGet(FROM), Op::LocalGet(POS),
        Op::Call { idx: rt.str_find_byte, pops: 3, pushes: 1 }, Op::LocalSet(M),
        Op::LocalGet(M), Op::Const(Const::I32(0)), Op::BinOp(B::I32LtS), Op::BrIf(1),
        Op::LocalGet(COUNT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(COUNT),
        Op::LocalGet(M), Op::LocalGet(FROML), Op::BinOp(B::I32Add), Op::LocalSet(POS),
        Op::LocalGet(ALL), Op::UnOp(U::I32Eqz), Op::BrIf(1),
        Op::Br(0),
    ];

    let mut build_loop = vec![
        Op::LocalGet(REM), Op::UnOp(U::I32Eqz), Op::BrIf(1),
        Op::LocalGet(S), Op::LocalGet(FROM), Op::LocalGet(SRC),
        Op::Call { idx: rt.str_find_byte, pops: 3, pushes: 1 }, Op::LocalSet(M),
        Op::LocalGet(M), Op::Const(Const::I32(0)), Op::BinOp(B::I32LtS), Op::BrIf(1),
        // seg = m - src ; copy s[src..m]
        Op::LocalGet(M), Op::LocalGet(SRC), Op::BinOp(B::I32Sub), Op::LocalSet(SEG),
    ];
    build_loop.extend(dst_addr());
    build_loop.extend(src_addr());
    build_loop.extend(vec![
        Op::LocalGet(SEG), Op::MemoryCopy,
        Op::LocalGet(DST), Op::LocalGet(SEG), Op::BinOp(B::I32Add), Op::LocalSet(DST),
    ]);
    // copy `to`
    build_loop.extend(dst_addr());
    build_loop.extend(vec![
        Op::LocalGet(TO), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(TOL), Op::MemoryCopy,
        Op::LocalGet(DST), Op::LocalGet(TOL), Op::BinOp(B::I32Add), Op::LocalSet(DST),
        Op::LocalGet(M), Op::LocalGet(FROML), Op::BinOp(B::I32Add), Op::LocalSet(SRC),
        Op::LocalGet(REM), Op::Const(Const::I32(1)), Op::BinOp(B::I32Sub), Op::LocalSet(REM),
        Op::Br(0),
    ]);

    let mut body = vec![
        Op::LocalGet(S), Op::Load(LoadKind::I32), Op::LocalSet(SL),
        Op::LocalGet(FROM), Op::Load(LoadKind::I32), Op::LocalSet(FROML),
        Op::LocalGet(TO), Op::Load(LoadKind::I32), Op::LocalSet(TOL),
        // empty `from` → copy of s
        Op::LocalGet(FROML), Op::UnOp(U::I32Eqz),
        Op::IfVoid {
            then: vec![
                Op::Const(Const::I32(8)), Op::LocalGet(SL), Op::BinOp(B::I32Add),
                Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
                Op::LocalGet(OUT), Op::LocalGet(SL), Op::Store(StoreKind::I32),
                Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(SL), Op::Store(StoreKind::I32),
                Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
                Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
                Op::LocalGet(SL), Op::MemoryCopy,
                Op::LocalGet(OUT), Op::Return,
            ],
            else_: vec![],
        },
        // pass 1: count
        Op::Const(Const::I32(0)), Op::LocalSet(COUNT),
        Op::Const(Const::I32(0)), Op::LocalSet(POS),
        Op::Block(vec![Op::Loop(count_loop)]),
        // newlen = sl + count * (tol - froml)
        Op::LocalGet(SL),
        Op::LocalGet(COUNT), Op::LocalGet(TOL), Op::LocalGet(FROML), Op::BinOp(B::I32Sub), Op::BinOp(B::I32Mul),
        Op::BinOp(B::I32Add), Op::LocalSet(NEWLEN),
        Op::Const(Const::I32(8)), Op::LocalGet(NEWLEN), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::LocalGet(NEWLEN), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(NEWLEN), Op::Store(StoreKind::I32),
        // pass 2: build
        Op::Const(Const::I32(0)), Op::LocalSet(SRC),
        Op::Const(Const::I32(0)), Op::LocalSet(DST),
        Op::LocalGet(COUNT), Op::LocalSet(REM),
        Op::Block(vec![Op::Loop(build_loop)]),
    ];
    // tail: copy s[src..sl]
    body.extend(vec![Op::LocalGet(SL), Op::LocalGet(SRC), Op::BinOp(B::I32Sub), Op::LocalSet(SEG)]);
    body.extend(dst_addr());
    body.extend(src_addr());
    body.extend(vec![Op::LocalGet(SEG), Op::MemoryCopy, Op::LocalGet(OUT)]);

    WasmFunc {
        name: "__string_replace".into(),
        params: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![
            WasmTy::I32, WasmTy::I32, WasmTy::I32, // sl,froml,tol
            WasmTy::I32, WasmTy::I32, WasmTy::I32, // count,pos,newlen
            WasmTy::I32, WasmTy::I32, WasmTy::I32, // out,src,dst
            WasmTy::I32, WasmTy::I32, WasmTy::I32, // rem,m,seg
        ],
        body,
    }
}

/// `__string_sub_bytes(s, a, b) -> i32` — fresh String from the byte range
/// `s[a, b)` (callers pass valid offsets with `a <= b`). Shared by split.
fn build_string_sub_bytes() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    const A: u32 = 1;
    const B_: u32 = 2;
    const NB: u32 = 3;
    const OUT: u32 = 4;
    let body = vec![
        Op::LocalGet(B_), Op::LocalGet(A), Op::BinOp(B::I32Sub), Op::LocalSet(NB),
        Op::Const(Const::I32(8)), Op::LocalGet(NB), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::LocalGet(NB), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(NB), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(A), Op::BinOp(B::I32Add),
        Op::LocalGet(NB), Op::MemoryCopy,
        Op::LocalGet(OUT),
    ];
    WasmFunc {
        name: "__string_sub_bytes".into(), params: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32], // nb, out
        body,
    }
}

/// `__string_split(s, sep) -> i32` — split into a `List[String]` (4-byte element
/// slots). Empty separator yields a single-element list `[s]`. Two passes:
/// count the pieces (matches + 1) to size the list, then slice each piece.
fn build_string_split() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    const SEP: u32 = 1;
    const SL: u32 = 2;
    const SEPL: u32 = 3;
    const COUNT: u32 = 4;
    const POS: u32 = 5;
    const LIST: u32 = 6;
    const IDX: u32 = 7;
    const STARTB: u32 = 8;
    const M: u32 = 9;

    // list element address: LIST + 8 + IDX*4
    let elem_addr = || vec![
        Op::LocalGet(LIST), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(IDX), Op::Const(Const::I32(4)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
    ];

    let count_loop = vec![
        Op::LocalGet(S), Op::LocalGet(SEP), Op::LocalGet(POS),
        Op::Call { idx: rt.str_find_byte, pops: 3, pushes: 1 }, Op::LocalSet(M),
        Op::LocalGet(M), Op::Const(Const::I32(0)), Op::BinOp(B::I32LtS), Op::BrIf(1),
        Op::LocalGet(COUNT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(COUNT),
        Op::LocalGet(M), Op::LocalGet(SEPL), Op::BinOp(B::I32Add), Op::LocalSet(POS),
        Op::Br(0),
    ];

    let mut build_loop = vec![
        // last piece handled after the loop: stop when idx == count-1
        Op::LocalGet(IDX), Op::LocalGet(COUNT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Sub), Op::BinOp(B::I32Eq), Op::BrIf(1),
        Op::LocalGet(S), Op::LocalGet(SEP), Op::LocalGet(POS),
        Op::Call { idx: rt.str_find_byte, pops: 3, pushes: 1 }, Op::LocalSet(M),
    ];
    build_loop.extend(elem_addr());
    build_loop.extend(vec![
        Op::LocalGet(S), Op::LocalGet(STARTB), Op::LocalGet(M),
        Op::Call { idx: rt.str_sub_bytes, pops: 3, pushes: 1 },
        Op::Store(StoreKind::I32),
        Op::LocalGet(IDX), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(IDX),
        Op::LocalGet(M), Op::LocalGet(SEPL), Op::BinOp(B::I32Add), Op::LocalSet(STARTB),
        Op::LocalGet(STARTB), Op::LocalSet(POS),
        Op::Br(0),
    ]);

    let mut body = vec![
        Op::LocalGet(S), Op::Load(LoadKind::I32), Op::LocalSet(SL),
        Op::LocalGet(SEP), Op::Load(LoadKind::I32), Op::LocalSet(SEPL),
        Op::Const(Const::I32(1)), Op::LocalSet(COUNT),
        Op::Const(Const::I32(0)), Op::LocalSet(POS),
        // count pieces only when separator is non-empty
        Op::LocalGet(SEPL),
        Op::IfVoid { then: vec![Op::Block(vec![Op::Loop(count_loop)])], else_: vec![] },
        // list = alloc(8 + count*4) ; len = cap = count
        Op::Const(Const::I32(8)), Op::LocalGet(COUNT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(LIST),
        Op::LocalGet(LIST), Op::LocalGet(COUNT), Op::Store(StoreKind::I32),
        Op::LocalGet(LIST), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(COUNT), Op::Store(StoreKind::I32),
        Op::Const(Const::I32(0)), Op::LocalSet(IDX),
        Op::Const(Const::I32(0)), Op::LocalSet(STARTB),
        Op::Const(Const::I32(0)), Op::LocalSet(POS),
        Op::LocalGet(SEPL),
        Op::IfVoid { then: vec![Op::Block(vec![Op::Loop(build_loop)])], else_: vec![] },
    ];
    // last piece = sub_bytes(s, startb, sl) at slot idx (== count-1)
    body.extend(elem_addr());
    body.extend(vec![
        Op::LocalGet(S), Op::LocalGet(STARTB), Op::LocalGet(SL),
        Op::Call { idx: rt.str_sub_bytes, pops: 3, pushes: 1 },
        Op::Store(StoreKind::I32),
        Op::LocalGet(LIST),
    ]);
    WasmFunc {
        name: "__string_split".into(), params: vec![WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // sl,sepl,count,pos,list,idx,startb,m
        body,
    }
}

/// `__string_join(list, sep) -> i32` — concatenate a `List[String]` (4-byte
/// element slots) with `sep` between elements. Two passes: sum byte lengths to
/// size the result, then copy each element interleaved with the separator.
fn build_string_join() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const LIST: u32 = 0;
    const SEP: u32 = 1;
    const N: u32 = 2;
    const SEPL: u32 = 3;
    const TOTAL: u32 = 4;
    const OUT: u32 = 5;
    const DST: u32 = 6;
    const I: u32 = 7;
    const E: u32 = 8;
    const EL: u32 = 9;

    // e = list[8 + i*4]
    let load_elem = || vec![
        Op::LocalGet(LIST), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(I), Op::Const(Const::I32(4)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::Load(LoadKind::I32),
    ];
    // out + 8 + dst
    let dst_addr = || vec![
        Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(DST), Op::BinOp(B::I32Add),
    ];

    let mut sum_loop = vec![
        Op::LocalGet(I), Op::LocalGet(N), Op::BinOp(B::I32GeU), Op::BrIf(1),
    ];
    sum_loop.push(Op::LocalGet(TOTAL));
    sum_loop.extend(load_elem());
    sum_loop.extend(vec![
        Op::Load(LoadKind::I32), Op::BinOp(B::I32Add), Op::LocalSet(TOTAL),
        Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(I), Op::Br(0),
    ]);

    let mut copy_loop = vec![
        Op::LocalGet(I), Op::LocalGet(N), Op::BinOp(B::I32GeU), Op::BrIf(1),
    ];
    // if i > 0: copy separator
    let mut sep_copy = dst_addr();
    sep_copy.extend(vec![
        Op::LocalGet(SEP), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(SEPL), Op::MemoryCopy,
        Op::LocalGet(DST), Op::LocalGet(SEPL), Op::BinOp(B::I32Add), Op::LocalSet(DST),
    ]);
    copy_loop.extend(vec![
        Op::LocalGet(I), Op::IfVoid { then: sep_copy, else_: vec![] },
    ]);
    copy_loop.extend(load_elem());
    copy_loop.push(Op::LocalSet(E));
    copy_loop.extend(vec![Op::LocalGet(E), Op::Load(LoadKind::I32), Op::LocalSet(EL)]);
    copy_loop.extend(dst_addr());
    copy_loop.extend(vec![
        Op::LocalGet(E), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(EL), Op::MemoryCopy,
        Op::LocalGet(DST), Op::LocalGet(EL), Op::BinOp(B::I32Add), Op::LocalSet(DST),
        Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(I), Op::Br(0),
    ]);

    let body = vec![
        Op::LocalGet(LIST), Op::Load(LoadKind::I32), Op::LocalSet(N),
        Op::LocalGet(SEP), Op::Load(LoadKind::I32), Op::LocalSet(SEPL),
        // total = (n == 0) ? 0 : (n-1)*sepl
        Op::LocalGet(N), Op::UnOp(U::I32Eqz),
        Op::If {
            ty: WasmTy::I32,
            then: vec![Op::Const(Const::I32(0))],
            else_: vec![Op::LocalGet(N), Op::Const(Const::I32(1)), Op::BinOp(B::I32Sub), Op::LocalGet(SEPL), Op::BinOp(B::I32Mul)],
        },
        Op::LocalSet(TOTAL),
        // total += sum of element byte lengths
        Op::Const(Const::I32(0)), Op::LocalSet(I),
        Op::Block(vec![Op::Loop(sum_loop)]),
        // out = alloc(8 + total) ; len = cap = total
        Op::Const(Const::I32(8)), Op::LocalGet(TOTAL), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::LocalGet(TOTAL), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(TOTAL), Op::Store(StoreKind::I32),
        Op::Const(Const::I32(0)), Op::LocalSet(DST),
        Op::Const(Const::I32(0)), Op::LocalSet(I),
        Op::Block(vec![Op::Loop(copy_loop)]),
        Op::LocalGet(OUT),
    ];
    WasmFunc {
        name: "__string_join".into(), params: vec![WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // n,sepl,total,out,dst,i,e,el
        body,
    }
}

/// `__int_parse(s, errmsg) -> i32` — parse a decimal integer (optional leading
/// `-`) into `Result[Int, String]` (12B: [tag@0][payload@4]; Ok=0 i64, Err=1
/// errmsg ptr). Empty input, a lone `-`, or any non-digit byte → Err(errmsg).
fn build_int_parse() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    const ERR: u32 = 1;
    const BL: u32 = 2;
    const I: u32 = 3;
    const NEG: u32 = 4;
    const ACC: u32 = 5;   // i64
    const ANY: u32 = 6;
    const RES: u32 = 7;

    // Err(errmsg): res = alloc(12); res[0]=1; res[4]=errmsg; return res
    let mk_err = || vec![
        Op::Const(Const::I32(12)), Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(RES),
        Op::LocalGet(RES), Op::Const(Const::I32(1)), Op::Store(StoreKind::I32),
        Op::LocalGet(RES), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(ERR), Op::Store(StoreKind::I32),
        Op::LocalGet(RES), Op::Return,
    ];

    let digit_loop = vec![
        Op::LocalGet(I), Op::LocalGet(BL), Op::BinOp(B::I32GeU), Op::BrIf(1),
        // d = s[8+i] - 48 ; if d >=u 10 → Err (d stays on the stack for the test)
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(I), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
        Op::Const(Const::I32(48)), Op::BinOp(B::I32Sub),
        Op::Const(Const::I32(10)), Op::BinOp(B::I32GeU),
        Op::IfVoid { then: mk_err(), else_: vec![] },
        // acc = acc*10 + d  (d currently in NEG temp — but NEG is the sign flag!). Recompute d.
        Op::LocalGet(ACC), Op::Const(Const::I64(10)), Op::BinOp(B::I64Mul),
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(I), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
        Op::Const(Const::I32(48)), Op::BinOp(B::I32Sub), Op::UnOp(U::I64ExtendI32U),
        Op::BinOp(B::I64Add), Op::LocalSet(ACC),
        Op::Const(Const::I32(1)), Op::LocalSet(ANY),
        Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(I), Op::Br(0),
    ];

    let body = vec![
        Op::LocalGet(S), Op::Load(LoadKind::I32), Op::LocalSet(BL),
        Op::Const(Const::I32(0)), Op::LocalSet(I),
        Op::Const(Const::I32(0)), Op::LocalSet(NEG),
        Op::Const(Const::I64(0)), Op::LocalSet(ACC),
        Op::Const(Const::I32(0)), Op::LocalSet(ANY),
        // empty → Err
        Op::LocalGet(BL), Op::UnOp(U::I32Eqz), Op::IfVoid { then: mk_err(), else_: vec![] },
        // leading '-'
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8), Op::Const(Const::I32(45)), Op::BinOp(B::I32Eq),
        Op::IfVoid { then: vec![Op::Const(Const::I32(1)), Op::LocalSet(NEG), Op::Const(Const::I32(1)), Op::LocalSet(I)], else_: vec![] },
        Op::Block(vec![Op::Loop(digit_loop)]),
        // no digits consumed → Err
        Op::LocalGet(ANY), Op::UnOp(U::I32Eqz), Op::IfVoid { then: mk_err(), else_: vec![] },
        // apply sign
        Op::LocalGet(NEG), Op::IfVoid { then: vec![Op::Const(Const::I64(0)), Op::LocalGet(ACC), Op::BinOp(B::I64Sub), Op::LocalSet(ACC)], else_: vec![] },
        // Ok(acc)
        Op::Const(Const::I32(12)), Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(RES),
        Op::LocalGet(RES), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
        Op::LocalGet(RES), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(ACC), Op::Store(StoreKind::I64),
        Op::LocalGet(RES),
    ];
    WasmFunc {
        name: "__int_parse".into(), params: vec![WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I64, WasmTy::I32, WasmTy::I32], // bl,i,neg,acc,any,res
        body,
    }
}

/// `__string_lines(s) -> i32` — split into a `List[String]` on `\n`, with no
/// trailing empty line for a final newline (`"a\n"` → `["a"]`). Line count =
/// newline count, plus one when the last byte is not a newline.
fn build_string_lines() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    const BL: u32 = 1;
    const NLINES: u32 = 2;
    const LIST: u32 = 3;
    const IDX: u32 = 4;
    const STARTB: u32 = 5;
    const I: u32 = 6;

    // byte s[8+i] == '\n'
    let is_nl = |i_local: u32| vec![
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(i_local), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
        Op::Const(Const::I32(10)), Op::BinOp(B::I32Eq),
    ];
    let elem_addr = || vec![
        Op::LocalGet(LIST), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(IDX), Op::Const(Const::I32(4)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
    ];

    let count_loop = {
        let mut v = vec![Op::LocalGet(I), Op::LocalGet(BL), Op::BinOp(B::I32GeU), Op::BrIf(1)];
        v.extend(is_nl(I));
        v.push(Op::IfVoid { then: vec![Op::LocalGet(NLINES), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(NLINES)], else_: vec![] });
        v.extend(vec![Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(I), Op::Br(0)]);
        v
    };

    let build_loop = {
        let mut emit = is_nl(I);
        // on newline: list[idx] = sub_bytes(s, startb, i); idx++; startb = i+1
        let mut on_nl = elem_addr();
        on_nl.extend(vec![
            Op::LocalGet(S), Op::LocalGet(STARTB), Op::LocalGet(I),
            Op::Call { idx: rt.str_sub_bytes, pops: 3, pushes: 1 }, Op::Store(StoreKind::I32),
            Op::LocalGet(IDX), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(IDX),
            Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(STARTB),
        ]);
        let mut v = vec![Op::LocalGet(I), Op::LocalGet(BL), Op::BinOp(B::I32GeU), Op::BrIf(1)];
        v.append(&mut emit);
        v.push(Op::IfVoid { then: on_nl, else_: vec![] });
        v.extend(vec![Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(I), Op::Br(0)]);
        v
    };

    let mut body = vec![
        Op::LocalGet(S), Op::Load(LoadKind::I32), Op::LocalSet(BL),
        Op::Const(Const::I32(0)), Op::LocalSet(NLINES),
        Op::Const(Const::I32(0)), Op::LocalSet(I),
        Op::Block(vec![Op::Loop(count_loop)]),
        // if bl>0 and last byte != '\n' → nlines++
        Op::LocalGet(BL),
        Op::IfVoid {
            then: {
                let mut t = vec![Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(BL), Op::BinOp(B::I32Add), Op::Const(Const::I32(1)), Op::BinOp(B::I32Sub), Op::Load(LoadKind::U8), Op::Const(Const::I32(10)), Op::BinOp(B::I32Ne)];
                t.push(Op::IfVoid { then: vec![Op::LocalGet(NLINES), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(NLINES)], else_: vec![] });
                t
            },
            else_: vec![],
        },
        // list = alloc(8 + nlines*4) ; len = cap = nlines
        Op::Const(Const::I32(8)), Op::LocalGet(NLINES), Op::Const(Const::I32(4)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(LIST),
        Op::LocalGet(LIST), Op::LocalGet(NLINES), Op::Store(StoreKind::I32),
        Op::LocalGet(LIST), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(NLINES), Op::Store(StoreKind::I32),
        Op::Const(Const::I32(0)), Op::LocalSet(IDX),
        Op::Const(Const::I32(0)), Op::LocalSet(STARTB),
        Op::Const(Const::I32(0)), Op::LocalSet(I),
        Op::Block(vec![Op::Loop(build_loop)]),
        // final piece if idx < nlines (content after the last newline)
        Op::LocalGet(IDX), Op::LocalGet(NLINES), Op::BinOp(B::I32LtU),
    ];
    let mut final_piece = elem_addr();
    final_piece.extend(vec![
        Op::LocalGet(S), Op::LocalGet(STARTB), Op::LocalGet(BL),
        Op::Call { idx: rt.str_sub_bytes, pops: 3, pushes: 1 }, Op::Store(StoreKind::I32),
    ]);
    body.push(Op::IfVoid { then: final_piece, else_: vec![] });
    body.push(Op::LocalGet(LIST));

    WasmFunc {
        name: "__string_lines".into(), params: vec![WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // bl,nlines,list,idx,startb,i
        body,
    }
}

/// `__float_to_string(v: f64) -> i32` — fixed 6-decimal formatting with
/// trailing zeros trimmed (minimum one fractional digit). Display-only: not a
/// shortest round-trip (no scientific notation, no NaN/Inf handling).
fn build_float_to_string() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const V: u32 = 0;     // f64 param
    const NEG: u32 = 1;   // i32
    const AV: u32 = 2;    // f64 (abs)
    const IP: u32 = 3;    // i64 integer part (>= 0)
    const FR: u32 = 4;    // i64 frac scaled to 6 digits, then trimmed
    const FD: u32 = 5;    // i32 frac digit count
    const ID: u32 = 6;    // i32 int digit count
    const T: u32 = 7;     // i64 temp
    const TOTAL: u32 = 8; // i32 byte length
    const OUT: u32 = 9;   // i32 string ptr
    const P: u32 = 10;    // i32 fill cursor
    const CNT: u32 = 11;  // i32 digit loop counter

    // base = OUT + 8 + NEG  (first int-digit byte)
    let int_base = || vec![
        Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(NEG), Op::BinOp(B::I32Add),
    ];

    // Trim trailing zeros: while FD >u 1 && FR % 10 == 0 { FR /= 10; FD-- }
    let trim_loop = vec![
        Op::LocalGet(FD), Op::Const(Const::I32(1)), Op::BinOp(B::I32GtU), Op::UnOp(U::I32Eqz), Op::BrIf(1),
        Op::LocalGet(FR), Op::Const(Const::I64(10)), Op::BinOp(B::I64RemS), Op::Const(Const::I64(0)), Op::BinOp(B::I64Ne), Op::BrIf(1),
        Op::LocalGet(FR), Op::Const(Const::I64(10)), Op::BinOp(B::I64DivS), Op::LocalSet(FR),
        Op::LocalGet(FD), Op::Const(Const::I32(1)), Op::BinOp(B::I32Sub), Op::LocalSet(FD),
        Op::Br(0),
    ];
    // Count int digits: ID=1; T=IP; while { T/=10; if T==0 break; ID++ }
    let id_loop = vec![
        Op::LocalGet(T), Op::Const(Const::I64(10)), Op::BinOp(B::I64DivS), Op::LocalSet(T),
        Op::LocalGet(T), Op::Const(Const::I64(0)), Op::BinOp(B::I64Eq), Op::BrIf(1),
        Op::LocalGet(ID), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(ID),
        Op::Br(0),
    ];
    // Write `count` (ID or FD) digits backward from cursor P, sourcing T.
    let digit_loop = |count: u32| vec![
        Op::LocalGet(CNT), Op::LocalGet(count), Op::BinOp(B::I32GeU), Op::BrIf(1),
        Op::LocalGet(P), Op::Const(Const::I32(1)), Op::BinOp(B::I32Sub), Op::LocalSet(P),
        Op::LocalGet(P),
        Op::Const(Const::I32(48)), Op::LocalGet(T), Op::Const(Const::I64(10)), Op::BinOp(B::I64RemS), Op::UnOp(U::I32WrapI64), Op::BinOp(B::I32Add),
        Op::Store(StoreKind::I8),
        Op::LocalGet(T), Op::Const(Const::I64(10)), Op::BinOp(B::I64DivS), Op::LocalSet(T),
        Op::LocalGet(CNT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(CNT),
        Op::Br(0),
    ];

    let mut body = vec![
        // NEG = v < 0 ; AV = |v|
        Op::LocalGet(V), Op::Const(Const::F64(0.0)), Op::BinOp(B::F64Lt), Op::LocalSet(NEG),
        Op::LocalGet(V), Op::UnOp(U::F64Abs), Op::LocalSet(AV),
        // IP = trunc(AV)
        Op::LocalGet(AV), Op::UnOp(U::I64TruncF64S), Op::LocalSet(IP),
        // FR = trunc((AV - (f64)IP) * 1e6 + 0.5)
        Op::LocalGet(AV), Op::LocalGet(IP), Op::UnOp(U::F64ConvertI64S), Op::BinOp(B::F64Sub),
        Op::Const(Const::F64(1_000_000.0)), Op::BinOp(B::F64Mul), Op::Const(Const::F64(0.5)), Op::BinOp(B::F64Add),
        Op::UnOp(U::I64TruncF64S), Op::LocalSet(FR),
        // carry: FR == 1e6 (rounded up) → IP++, FR=0
        Op::LocalGet(FR), Op::Const(Const::I64(1_000_000)), Op::BinOp(B::I64GeS),
        Op::IfVoid {
            then: vec![
                Op::LocalGet(IP), Op::Const(Const::I64(1)), Op::BinOp(B::I64Add), Op::LocalSet(IP),
                Op::LocalGet(FR), Op::Const(Const::I64(1_000_000)), Op::BinOp(B::I64Sub), Op::LocalSet(FR),
            ],
            else_: vec![],
        },
        // FD = 6 ; trim
        Op::Const(Const::I32(6)), Op::LocalSet(FD),
        Op::Block(vec![Op::Loop(trim_loop)]),
        // ID = count digits of IP
        Op::Const(Const::I32(1)), Op::LocalSet(ID),
        Op::LocalGet(IP), Op::LocalSet(T),
        Op::Block(vec![Op::Loop(id_loop)]),
        // TOTAL = NEG + ID + 1 + FD
        Op::LocalGet(NEG), Op::LocalGet(ID), Op::BinOp(B::I32Add), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalGet(FD), Op::BinOp(B::I32Add), Op::LocalSet(TOTAL),
        // OUT = alloc(8 + TOTAL) ; len = cap = TOTAL
        Op::Const(Const::I32(8)), Op::LocalGet(TOTAL), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::LocalGet(TOTAL), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(TOTAL), Op::Store(StoreKind::I32),
        // sign byte
        Op::LocalGet(NEG),
        Op::IfVoid {
            then: vec![Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::Const(Const::I32(45)), Op::Store(StoreKind::I8)],
            else_: vec![],
        },
    ];
    // int digits: P = int_base + ID ; T = IP ; write ID digits backward
    body.extend(int_base());
    body.extend(vec![Op::LocalGet(ID), Op::BinOp(B::I32Add), Op::LocalSet(P)]);
    body.extend(vec![Op::LocalGet(IP), Op::LocalSet(T), Op::Const(Const::I32(0)), Op::LocalSet(CNT)]);
    body.push(Op::Block(vec![Op::Loop(digit_loop(ID))]));
    // '.' at int_base + ID
    body.extend(int_base());
    body.extend(vec![Op::LocalGet(ID), Op::BinOp(B::I32Add), Op::Const(Const::I32(46)), Op::Store(StoreKind::I8)]);
    // frac digits: P = int_base + ID + 1 + FD ; T = FR ; write FD digits backward
    body.extend(int_base());
    body.extend(vec![
        Op::LocalGet(ID), Op::BinOp(B::I32Add), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalGet(FD), Op::BinOp(B::I32Add), Op::LocalSet(P),
        Op::LocalGet(FR), Op::LocalSet(T), Op::Const(Const::I32(0)), Op::LocalSet(CNT),
    ]);
    body.push(Op::Block(vec![Op::Loop(digit_loop(FD))]));
    body.push(Op::LocalGet(OUT));

    WasmFunc {
        name: "__float_to_string".into(), params: vec![WasmTy::F64], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::F64, WasmTy::I64, WasmTy::I64, WasmTy::I32, WasmTy::I32, WasmTy::I64, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32],
        body,
    }
}

// ── WASI stdout (fd_write) ───────────────────────────────────────────
//
// A 16-byte scratch region in the null page (below DATA_BASE=16) holds the
// iovec and the bytes-written slot for fd_write — print is synchronous so reuse
// is safe. iov.ptr @0, iov.len @4, nwritten @8, newline byte @12.

/// `__print(s: i32)` — write the string's bytes to stdout via fd_write(1, …).
fn build_print() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    WasmFunc {
        name: "__print".into(), params: vec![WasmTy::I32], results: vec![],
        locals: vec![],
        body: vec![
            // iov.ptr = s + 8
            Op::Const(Const::I32(0)), Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::Store(StoreKind::I32),
            // iov.len = s.len (byte length)
            Op::Const(Const::I32(4)), Op::LocalGet(S), Op::Load(LoadKind::I32), Op::Store(StoreKind::I32),
            // fd_write(1, iov=0, iovcnt=1, nwritten=8) ; ignore errno
            Op::Const(Const::I32(1)), Op::Const(Const::I32(0)), Op::Const(Const::I32(1)), Op::Const(Const::I32(8)),
            Op::Call { idx: rt.fd_write, pops: 4, pushes: 1 },
            Op::Drop,
        ],
    }
}

/// `__println(s: i32)` — print `s` then a newline.
fn build_println() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    WasmFunc {
        name: "__println".into(), params: vec![WasmTy::I32], results: vec![],
        locals: vec![],
        body: vec![
            Op::LocalGet(S), Op::Call { idx: rt.print, pops: 1, pushes: 0 },
            // newline byte at scratch+12, iov over it
            Op::Const(Const::I32(12)), Op::Const(Const::I32(10)), Op::Store(StoreKind::I8),
            Op::Const(Const::I32(0)), Op::Const(Const::I32(12)), Op::Store(StoreKind::I32),
            Op::Const(Const::I32(4)), Op::Const(Const::I32(1)), Op::Store(StoreKind::I32),
            Op::Const(Const::I32(1)), Op::Const(Const::I32(0)), Op::Const(Const::I32(1)), Op::Const(Const::I32(8)),
            Op::Call { idx: rt.fd_write, pops: 4, pushes: 1 },
            Op::Drop,
        ],
    }
}

/// `__map_merge(a, b, kind) -> i32` — all of `a`, then all of `b` (b wins on
/// duplicate keys). Functional: builds a fresh table.
fn build_map_merge() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const A: u32 = 0;
    const B_: u32 = 1;
    const KIND: u32 = 2;
    const NEWCAP: u32 = 3;
    const OUT: u32 = 4;
    const SLOT: u32 = 5;
    const SRC: u32 = 6;   // map being copied
    const SRCCAP: u32 = 7;
    const EA: u32 = 8;

    // copy all occupied entries of SRC (cap in SRCCAP) into OUT
    let mut copy = Vec::new();
    copy.extend([Op::LocalGet(SLOT), Op::LocalGet(SRCCAP), Op::BinOp(B::I32GeU), Op::BrIf(1)]);
    copy.extend(map_tag_addr(SRC, SLOT)); copy.push(Op::Load(LoadKind::U8));
    let put = {
        let mut t = Vec::new();
        t.extend(map_entry_addr(SRC, SRCCAP, SLOT)); t.push(Op::LocalSet(EA));
        t.push(Op::LocalGet(OUT));
        t.push(Op::LocalGet(EA)); t.push(Op::Load(LoadKind::I64));
        t.push(Op::LocalGet(EA)); t.push(Op::Const(Const::I32(8))); t.push(Op::BinOp(B::I32Add)); t.push(Op::Load(LoadKind::I64));
        t.push(Op::LocalGet(KIND));
        t.push(Op::Call { idx: rt.map_put, pops: 4, pushes: 0 });
        t
    };
    copy.push(Op::IfVoid { then: put, else_: vec![] });
    copy.extend([Op::LocalGet(SLOT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(SLOT), Op::Br(0)]);
    // a small helper to run the copy loop over a given (src, srccap) — emitted twice
    let copy_loop = |src: u32, cap: u32| {
        vec![
            Op::LocalGet(src), Op::LocalSet(SRC),
            Op::LocalGet(cap), Op::LocalSet(SRCCAP),
            Op::Const(Const::I32(0)), Op::LocalSet(SLOT),
            Op::Block(vec![Op::Loop(copy.clone())]),
        ]
    };

    const ACAP: u32 = 9;
    const BCAP: u32 = 10;
    let mut body = vec![
        Op::LocalGet(A), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I32), Op::LocalSet(ACAP),
        Op::LocalGet(B_), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I32), Op::LocalSet(BCAP),
        // newcap = (len(a) + len(b)) * 2 ; min 4
        Op::LocalGet(A), Op::Load(LoadKind::I32), Op::LocalGet(B_), Op::Load(LoadKind::I32), Op::BinOp(B::I32Add),
        Op::Const(Const::I32(2)), Op::BinOp(B::I32Mul), Op::LocalSet(NEWCAP),
        Op::LocalGet(NEWCAP), Op::Const(Const::I32(4)), Op::BinOp(B::I32LtU),
        Op::IfVoid { then: vec![Op::Const(Const::I32(4)), Op::LocalSet(NEWCAP)], else_: vec![] },
        Op::Const(Const::I32(8)), Op::LocalGet(NEWCAP), Op::BinOp(B::I32Add),
        Op::LocalGet(NEWCAP), Op::Const(Const::I32(MAP_ENTRY)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(NEWCAP), Op::Store(StoreKind::I32),
    ];
    body.extend(copy_loop(A, ACAP));
    body.extend(copy_loop(B_, BCAP));
    body.push(Op::LocalGet(OUT));

    WasmFunc {
        name: "__map_merge".into(), params: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![
            WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, // newcap,out,slot,src,srccap
            WasmTy::I32, WasmTy::I32, WasmTy::I32, // ea, acap, bcap
        ],
        body,
    }
}

/// `__map_collect(m, field_off, elem_size) -> i32` — collect each occupied
/// entry's key (field_off 0) or value (field_off 8) into a List of `elem_size`
/// elements. Backs both map.keys and map.values.
fn build_map_collect() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const M: u32 = 0;
    const FOFF: u32 = 1;
    const ES: u32 = 2;
    const CAP: u32 = 3;
    const LEN: u32 = 4;
    const OUT: u32 = 5;
    const SLOT: u32 = 6;
    const OC: u32 = 7;
    const ADDR: u32 = 8;
    const VAL: u32 = 9; // i64

    let store = vec![
        // addr = out + 8 + oc*es
        Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(OC), Op::LocalGet(ES), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::LocalSet(ADDR),
        // val = entry[slot] + foff  (i64)
        Op::LocalGet(M), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(CAP), Op::BinOp(B::I32Add),
        Op::LocalGet(SLOT), Op::Const(Const::I32(MAP_ENTRY)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::LocalGet(FOFF), Op::BinOp(B::I32Add), Op::Load(LoadKind::I64), Op::LocalSet(VAL),
        // store es bytes: i64 if es==8 else i32(wrap)
        Op::LocalGet(ES), Op::Const(Const::I32(8)), Op::BinOp(B::I32Eq),
        Op::IfVoid {
            then: vec![Op::LocalGet(ADDR), Op::LocalGet(VAL), Op::Store(StoreKind::I64)],
            else_: vec![Op::LocalGet(ADDR), Op::LocalGet(VAL), Op::UnOp(U::I32WrapI64), Op::Store(StoreKind::I32)],
        },
        Op::LocalGet(OC), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(OC),
    ];
    let mut loop_body = vec![
        Op::LocalGet(SLOT), Op::LocalGet(CAP), Op::BinOp(B::I32GeU), Op::BrIf(1),
    ];
    loop_body.extend(map_tag_addr(M, SLOT)); loop_body.push(Op::Load(LoadKind::U8));
    loop_body.push(Op::IfVoid { then: store, else_: vec![] });
    loop_body.extend([Op::LocalGet(SLOT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(SLOT), Op::Br(0)]);

    let body = vec![
        Op::LocalGet(M), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I32), Op::LocalSet(CAP),
        Op::LocalGet(M), Op::Load(LoadKind::I32), Op::LocalSet(LEN),
        // out = alloc(8 + len*es) ; out.len=out.cap=len
        Op::Const(Const::I32(8)), Op::LocalGet(LEN), Op::LocalGet(ES), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::LocalGet(LEN), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(LEN), Op::Store(StoreKind::I32),
        Op::Const(Const::I32(0)), Op::LocalSet(SLOT),
        Op::Const(Const::I32(0)), Op::LocalSet(OC),
        Op::Block(vec![Op::Loop(loop_body)]),
        Op::LocalGet(OUT),
    ];
    WasmFunc {
        name: "__map_collect".into(), params: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I64], // cap,len,out,slot,oc,addr,val
        body,
    }
}

/// `__map_remove(m, k, kind) -> i32` — functional: rebuild without key `k`.
fn build_map_remove() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const M: u32 = 0;
    const K: u32 = 1;
    const KIND: u32 = 2;
    const OLDCAP: u32 = 3;
    const NEWCAP: u32 = 4;
    const OUT: u32 = 5;
    const SLOT: u32 = 6;
    const EA: u32 = 7;

    let mut copy = Vec::new();
    copy.extend([Op::LocalGet(SLOT), Op::LocalGet(OLDCAP), Op::BinOp(B::I32GeU), Op::BrIf(1)]);
    copy.extend(map_tag_addr(M, SLOT)); copy.push(Op::Load(LoadKind::U8));
    let put_unless_match = {
        let mut t = Vec::new();
        t.extend(map_entry_addr(M, OLDCAP, SLOT)); t.push(Op::LocalSet(EA));
        // skip if key matches k
        t.push(Op::LocalGet(EA)); t.push(Op::Load(LoadKind::I64));
        t.push(Op::LocalGet(K)); t.push(Op::LocalGet(KIND));
        t.push(Op::Call { idx: rt.map_key_eq, pops: 3, pushes: 1 });
        t.push(Op::UnOp(U::I32Eqz)); // not equal → keep
        let keep = vec![
            Op::LocalGet(OUT),
            Op::LocalGet(EA), Op::Load(LoadKind::I64),
            Op::LocalGet(EA), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I64),
            Op::LocalGet(KIND),
            Op::Call { idx: rt.map_put, pops: 4, pushes: 0 },
        ];
        t.push(Op::IfVoid { then: keep, else_: vec![] });
        t
    };
    copy.push(Op::IfVoid { then: put_unless_match, else_: vec![] });
    copy.extend([Op::LocalGet(SLOT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(SLOT), Op::Br(0)]);

    let body = vec![
        Op::LocalGet(M), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I32), Op::LocalSet(OLDCAP),
        // newcap = len*2 ; min 4  (len entries, removing at most one)
        Op::LocalGet(M), Op::Load(LoadKind::I32), Op::Const(Const::I32(2)), Op::BinOp(B::I32Mul), Op::LocalSet(NEWCAP),
        Op::LocalGet(NEWCAP), Op::Const(Const::I32(4)), Op::BinOp(B::I32LtU),
        Op::IfVoid { then: vec![Op::Const(Const::I32(4)), Op::LocalSet(NEWCAP)], else_: vec![] },
        Op::Const(Const::I32(8)), Op::LocalGet(NEWCAP), Op::BinOp(B::I32Add),
        Op::LocalGet(NEWCAP), Op::Const(Const::I32(MAP_ENTRY)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(NEWCAP), Op::Store(StoreKind::I32),
        Op::Const(Const::I32(0)), Op::LocalSet(SLOT),
        Op::Block(vec![Op::Loop(copy)]),
        Op::LocalGet(OUT),
    ];
    WasmFunc {
        name: "__map_remove".into(), params: vec![WasmTy::I32, WasmTy::I64, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // oldcap,newcap,out,slot,ea
        body,
    }
}

/// `__map_hash(key: i64, kind: i32) -> i32` — kind 0 = Int (low 32 bits),
/// kind 1 = String (FNV-1a over the bytes; key is the string pointer).
fn build_map_hash() -> WasmFunc {
    const KEY: u32 = 0;
    const KIND: u32 = 1;
    const P: u32 = 2;
    const LEN: u32 = 3;
    const H: u32 = 4;
    const I: u32 = 5;

    let mut fnv_loop = vec![
        Op::LocalGet(I), Op::LocalGet(LEN), Op::BinOp(B::I32GeU), Op::BrIf(1),
        // h = (h ^ byte[p+8+i]) * 16777619
        Op::LocalGet(H),
        Op::LocalGet(P), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(I), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
        Op::BinOp(B::I32Xor),
        Op::Const(Const::I32(16777619)), Op::BinOp(B::I32Mul),
        Op::LocalSet(H),
        Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(I), Op::Br(0),
    ];
    let string_hash = {
        let mut t = vec![
            Op::LocalGet(KEY), Op::UnOp(U::I32WrapI64), Op::LocalSet(P),
            Op::LocalGet(P), Op::Load(LoadKind::I32), Op::LocalSet(LEN),
            Op::Const(Const::I32(-2128831035)), Op::LocalSet(H), // 2166136261 (FNV offset)
            Op::Const(Const::I32(0)), Op::LocalSet(I),
        ];
        t.push(Op::Block(vec![Op::Loop(std::mem::take(&mut fnv_loop))]));
        t.push(Op::LocalGet(H));
        t
    };
    WasmFunc {
        name: "__map_hash".into(), params: vec![WasmTy::I64, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // p, len, h, i
        body: vec![
            Op::LocalGet(KIND), Op::UnOp(U::I32Eqz),
            Op::If {
                ty: WasmTy::I32,
                then: vec![Op::LocalGet(KEY), Op::UnOp(U::I32WrapI64)],
                else_: string_hash,
            },
        ],
    }
}

/// `__map_key_eq(a: i64, b: i64, kind: i32) -> i32` — Int: i64 eq; String:
/// __string_eq over the two pointers.
fn build_map_key_eq() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const A: u32 = 0;
    const B_: u32 = 1;
    const KIND: u32 = 2;
    WasmFunc {
        name: "__map_key_eq".into(), params: vec![WasmTy::I64, WasmTy::I64, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![],
        body: vec![
            Op::LocalGet(KIND), Op::UnOp(U::I32Eqz),
            Op::If {
                ty: WasmTy::I32,
                then: vec![Op::LocalGet(A), Op::LocalGet(B_), Op::BinOp(B::I64Eq)],
                else_: vec![
                    Op::LocalGet(A), Op::UnOp(U::I32WrapI64),
                    Op::LocalGet(B_), Op::UnOp(U::I32WrapI64),
                    Op::Call { idx: rt.string_eq, pops: 2, pushes: 1 },
                ],
            },
        ],
    }
}

/// `__map_get_or(m, k, default: i64, kind: i32) -> i64` — value or default.
/// Returns the value directly (no Option alloc); target of the `?? d` fusion.
fn build_map_get_or() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const M: u32 = 0;
    const K: u32 = 1;
    const DEF: u32 = 2;
    const KIND: u32 = 3;
    const CAP: u32 = 4;
    const SLOT: u32 = 5;
    const EA: u32 = 6;

    let mut loop_body = Vec::new();
    loop_body.extend(map_tag_addr(M, SLOT)); loop_body.push(Op::Load(LoadKind::U8));
    loop_body.push(Op::UnOp(U::I32Eqz));
    loop_body.push(Op::IfVoid { then: vec![Op::LocalGet(DEF), Op::Return], else_: vec![] });
    loop_body.extend(map_entry_addr(M, CAP, SLOT)); loop_body.push(Op::LocalSet(EA));
    loop_body.push(Op::LocalGet(EA)); loop_body.push(Op::Load(LoadKind::I64));
    loop_body.push(Op::LocalGet(K)); loop_body.push(Op::LocalGet(KIND));
    loop_body.push(Op::Call { idx: rt.map_key_eq, pops: 3, pushes: 1 });
    loop_body.push(Op::IfVoid {
        then: vec![Op::LocalGet(EA), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I64), Op::Return],
        else_: vec![],
    });
    loop_body.extend([Op::LocalGet(SLOT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalGet(CAP), Op::BinOp(B::I32RemU), Op::LocalSet(SLOT), Op::Br(0)]);

    let body = vec![
        Op::LocalGet(M), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I32), Op::LocalSet(CAP),
        Op::LocalGet(CAP), Op::UnOp(U::I32Eqz),
        Op::IfVoid { then: vec![Op::LocalGet(DEF), Op::Return], else_: vec![] },
        Op::LocalGet(K), Op::LocalGet(KIND), Op::Call { idx: rt.map_hash, pops: 2, pushes: 1 },
        Op::LocalGet(CAP), Op::BinOp(B::I32RemU), Op::LocalSet(SLOT),
        Op::Loop(loop_body),
        Op::LocalGet(DEF),
    ];
    WasmFunc {
        name: "__map_get_or".into(), params: vec![WasmTy::I32, WasmTy::I64, WasmTy::I64, WasmTy::I32], results: vec![WasmTy::I64],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32], // cap, slot, ea
        body,
    }
}

// ── Map[Int, Int] runtime (linear-probed open addressing) ────────────
//
// Layout: [len:i32 @0][cap:i32 @4][tags:u8[cap] @8][entries @ 8+cap], entry =
// [key:i64][val:i64] (16 bytes). tag 0 = empty, 1 = occupied. Fresh __alloc
// memory is zero (allocator never frees), so new tables start all-empty.
// `set` is functional: it rebuilds at capacity 2*(len+1), so load factor < 1
// always holds and a probe is guaranteed to hit an empty slot.

const MAP_ENTRY: i32 = 16;

/// Tag address `m + 8 + slot`.
fn map_tag_addr(m: u32, slot: u32) -> Vec<Op> {
    vec![Op::LocalGet(m), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(slot), Op::BinOp(B::I32Add)]
}
/// Entry address `m + 8 + cap + slot*16` (cap in local `cap`).
fn map_entry_addr(m: u32, cap: u32, slot: u32) -> Vec<Op> {
    vec![
        Op::LocalGet(m), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(cap), Op::BinOp(B::I32Add),
        Op::LocalGet(slot), Op::Const(Const::I32(MAP_ENTRY)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
    ]
}

/// `__map_new() -> i32` — empty map (len = cap = 0).
fn build_map_new() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const OUT: u32 = 0;
    WasmFunc {
        name: "__map_new".into(), params: vec![], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32],
        body: vec![
            Op::Const(Const::I32(8)), Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
            Op::LocalGet(OUT), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
            Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
            Op::LocalGet(OUT),
        ],
    }
}

/// `__map_put(m, k: i64, v: i64, kind: i32)` — insert/overwrite into a table
/// with spare capacity (build-time; never resizes). Bumps len for new keys.
fn build_map_put() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const M: u32 = 0;
    const K: u32 = 1;
    const V: u32 = 2;
    const KIND: u32 = 3;
    const CAP: u32 = 4;
    const SLOT: u32 = 5;
    const EA: u32 = 6;

    let mut loop_body = Vec::new();
    loop_body.extend(map_tag_addr(M, SLOT));
    loop_body.push(Op::Load(LoadKind::U8));
    loop_body.push(Op::UnOp(U::I32Eqz));
    let insert = {
        let mut t = Vec::new();
        t.extend(map_tag_addr(M, SLOT)); t.push(Op::Const(Const::I32(1))); t.push(Op::Store(StoreKind::I8));
        t.extend(map_entry_addr(M, CAP, SLOT)); t.push(Op::LocalGet(K)); t.push(Op::Store(StoreKind::I64));
        t.extend(map_entry_addr(M, CAP, SLOT)); t.push(Op::Const(Const::I32(8))); t.push(Op::BinOp(B::I32Add));
        t.push(Op::LocalGet(V)); t.push(Op::Store(StoreKind::I64));
        t.push(Op::LocalGet(M)); t.push(Op::LocalGet(M)); t.push(Op::Load(LoadKind::I32));
        t.push(Op::Const(Const::I32(1))); t.push(Op::BinOp(B::I32Add)); t.push(Op::Store(StoreKind::I32));
        t.push(Op::Return);
        t
    };
    loop_body.push(Op::IfVoid { then: insert, else_: vec![] });
    // occupied: if key matches (kind-dispatched), overwrite value, return
    loop_body.extend(map_entry_addr(M, CAP, SLOT)); loop_body.push(Op::LocalSet(EA));
    loop_body.push(Op::LocalGet(EA)); loop_body.push(Op::Load(LoadKind::I64));
    loop_body.push(Op::LocalGet(K)); loop_body.push(Op::LocalGet(KIND));
    loop_body.push(Op::Call { idx: rt.map_key_eq, pops: 3, pushes: 1 });
    let overwrite = vec![
        Op::LocalGet(EA), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(V), Op::Store(StoreKind::I64), Op::Return,
    ];
    loop_body.push(Op::IfVoid { then: overwrite, else_: vec![] });
    loop_body.extend([Op::LocalGet(SLOT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalGet(CAP), Op::BinOp(B::I32RemU), Op::LocalSet(SLOT), Op::Br(0)]);

    let body = vec![
        Op::LocalGet(M), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I32), Op::LocalSet(CAP),
        // slot = hash(k, kind) % cap
        Op::LocalGet(K), Op::LocalGet(KIND), Op::Call { idx: rt.map_hash, pops: 2, pushes: 1 },
        Op::LocalGet(CAP), Op::BinOp(B::I32RemU), Op::LocalSet(SLOT),
        Op::Loop(loop_body),
    ];
    WasmFunc {
        name: "__map_put".into(), params: vec![WasmTy::I32, WasmTy::I64, WasmTy::I64, WasmTy::I32], results: vec![],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32], // cap, slot, ea
        body,
    }
}

/// `__map_set(m, k, v) -> i32` — functional insert: rebuild at cap 2*(len+1),
/// re-inserting all existing entries plus (k, v).
fn build_map_set() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const M: u32 = 0;
    const K: u32 = 1;
    const V: u32 = 2;
    const KIND: u32 = 3;
    const OLDCAP: u32 = 4;
    const NEWCAP: u32 = 5;
    const OUT: u32 = 6;
    const SLOT: u32 = 7;
    const EA: u32 = 8;

    // copy loop over old slots (re-put with the same kind)
    let mut copy = Vec::new();
    copy.extend([Op::LocalGet(SLOT), Op::LocalGet(OLDCAP), Op::BinOp(B::I32GeU), Op::BrIf(1)]);
    copy.extend(map_tag_addr(M, SLOT)); copy.push(Op::Load(LoadKind::U8));
    let put_old = {
        let mut t = Vec::new();
        t.extend(map_entry_addr(M, OLDCAP, SLOT)); t.push(Op::LocalSet(EA));
        t.push(Op::LocalGet(OUT));
        t.push(Op::LocalGet(EA)); t.push(Op::Load(LoadKind::I64));            // key
        t.push(Op::LocalGet(EA)); t.push(Op::Const(Const::I32(8))); t.push(Op::BinOp(B::I32Add)); t.push(Op::Load(LoadKind::I64)); // val
        t.push(Op::LocalGet(KIND));
        t.push(Op::Call { idx: rt.map_put, pops: 4, pushes: 0 });
        t
    };
    copy.push(Op::IfVoid { then: put_old, else_: vec![] });
    copy.extend([Op::LocalGet(SLOT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(SLOT), Op::Br(0)]);

    let body = vec![
        Op::LocalGet(M), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I32), Op::LocalSet(OLDCAP),
        Op::LocalGet(M), Op::Load(LoadKind::I32), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add),
        Op::Const(Const::I32(2)), Op::BinOp(B::I32Mul), Op::LocalSet(NEWCAP),
        Op::LocalGet(NEWCAP), Op::Const(Const::I32(4)), Op::BinOp(B::I32LtU),
        Op::IfVoid { then: vec![Op::Const(Const::I32(4)), Op::LocalSet(NEWCAP)], else_: vec![] },
        Op::Const(Const::I32(8)), Op::LocalGet(NEWCAP), Op::BinOp(B::I32Add),
        Op::LocalGet(NEWCAP), Op::Const(Const::I32(MAP_ENTRY)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(NEWCAP), Op::Store(StoreKind::I32),
        Op::Const(Const::I32(0)), Op::LocalSet(SLOT),
        Op::Block(vec![Op::Loop(copy)]),
        // put (k, v)
        Op::LocalGet(OUT), Op::LocalGet(K), Op::LocalGet(V), Op::LocalGet(KIND), Op::Call { idx: rt.map_put, pops: 4, pushes: 0 },
        Op::LocalGet(OUT),
    ];
    WasmFunc {
        name: "__map_set".into(), params: vec![WasmTy::I32, WasmTy::I64, WasmTy::I64, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // oldcap,newcap,out,slot,ea
        body,
    }
}

/// `__map_get(m, k, kind) -> i32` — Option (payload i64): Some(value) or None.
fn build_map_get() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const M: u32 = 0;
    const K: u32 = 1;
    const KIND: u32 = 2;
    const CAP: u32 = 3;
    const SLOT: u32 = 4;
    const OPT: u32 = 5;
    const EA: u32 = 6;

    let mut loop_body = Vec::new();
    loop_body.extend(map_tag_addr(M, SLOT)); loop_body.push(Op::Load(LoadKind::U8));
    loop_body.push(Op::UnOp(U::I32Eqz));
    loop_body.push(Op::IfVoid { then: vec![Op::LocalGet(OPT), Op::Return], else_: vec![] });
    loop_body.extend(map_entry_addr(M, CAP, SLOT)); loop_body.push(Op::LocalSet(EA));
    loop_body.push(Op::LocalGet(EA)); loop_body.push(Op::Load(LoadKind::I64));
    loop_body.push(Op::LocalGet(K)); loop_body.push(Op::LocalGet(KIND));
    loop_body.push(Op::Call { idx: rt.map_key_eq, pops: 3, pushes: 1 });
    let found = vec![
        Op::LocalGet(OPT), Op::Const(Const::I32(1)), Op::Store(StoreKind::I32),
        Op::LocalGet(OPT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        Op::LocalGet(EA), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I64),
        Op::Store(StoreKind::I64),
        Op::LocalGet(OPT), Op::Return,
    ];
    loop_body.push(Op::IfVoid { then: found, else_: vec![] });
    loop_body.extend([Op::LocalGet(SLOT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalGet(CAP), Op::BinOp(B::I32RemU), Op::LocalSet(SLOT), Op::Br(0)]);

    let body = vec![
        Op::Const(Const::I32(12)), Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OPT),
        Op::LocalGet(OPT), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32),
        Op::LocalGet(M), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I32), Op::LocalSet(CAP),
        Op::LocalGet(CAP), Op::UnOp(U::I32Eqz),
        Op::IfVoid { then: vec![Op::LocalGet(OPT), Op::Return], else_: vec![] },
        Op::LocalGet(K), Op::LocalGet(KIND), Op::Call { idx: rt.map_hash, pops: 2, pushes: 1 },
        Op::LocalGet(CAP), Op::BinOp(B::I32RemU), Op::LocalSet(SLOT),
        Op::Loop(loop_body),
        Op::LocalGet(OPT),
    ];
    WasmFunc {
        name: "__map_get".into(), params: vec![WasmTy::I32, WasmTy::I64, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // cap, slot, opt, ea
        body,
    }
}

/// `__map_contains(m, k, kind) -> i32`.
fn build_map_contains() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const M: u32 = 0;
    const K: u32 = 1;
    const KIND: u32 = 2;
    const CAP: u32 = 3;
    const SLOT: u32 = 4;
    const EA: u32 = 5;

    let mut loop_body = Vec::new();
    loop_body.extend(map_tag_addr(M, SLOT)); loop_body.push(Op::Load(LoadKind::U8));
    loop_body.push(Op::UnOp(U::I32Eqz));
    loop_body.push(Op::IfVoid { then: vec![Op::Const(Const::I32(0)), Op::Return], else_: vec![] });
    loop_body.extend(map_entry_addr(M, CAP, SLOT)); loop_body.push(Op::LocalSet(EA));
    loop_body.push(Op::LocalGet(EA)); loop_body.push(Op::Load(LoadKind::I64));
    loop_body.push(Op::LocalGet(K)); loop_body.push(Op::LocalGet(KIND));
    loop_body.push(Op::Call { idx: rt.map_key_eq, pops: 3, pushes: 1 });
    loop_body.push(Op::IfVoid { then: vec![Op::Const(Const::I32(1)), Op::Return], else_: vec![] });
    loop_body.extend([Op::LocalGet(SLOT), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalGet(CAP), Op::BinOp(B::I32RemU), Op::LocalSet(SLOT), Op::Br(0)]);

    let body = vec![
        Op::LocalGet(M), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::Load(LoadKind::I32), Op::LocalSet(CAP),
        Op::LocalGet(CAP), Op::UnOp(U::I32Eqz),
        Op::IfVoid { then: vec![Op::Const(Const::I32(0)), Op::Return], else_: vec![] },
        Op::LocalGet(K), Op::LocalGet(KIND), Op::Call { idx: rt.map_hash, pops: 2, pushes: 1 },
        Op::LocalGet(CAP), Op::BinOp(B::I32RemU), Op::LocalSet(SLOT),
        Op::Loop(loop_body),
        Op::Const(Const::I32(0)),
    ];
    WasmFunc {
        name: "__map_contains".into(), params: vec![WasmTy::I32, WasmTy::I64, WasmTy::I32], results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32], // cap, slot, ea
        body,
    }
}

/// `__map_len(m) -> i64`.
fn build_map_len() -> WasmFunc {
    WasmFunc {
        name: "__map_len".into(), params: vec![WasmTy::I32], results: vec![WasmTy::I64],
        locals: vec![],
        body: vec![Op::LocalGet(0), Op::Load(LoadKind::I32), Op::UnOp(U::I64ExtendI32U)],
    }
}

/// `__list_sort_int(xs: List[Int]) -> List[Int]` — ascending selection sort on
/// a fresh copy (i64 elements, 8-byte stride). O(n²), fine for typical sizes.
fn build_list_sort_int() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const XS: u32 = 0;
    const LEN: u32 = 1;
    const OUT: u32 = 2;
    const I: u32 = 3;
    const J: u32 = 4;
    const MN: u32 = 5;   // index of current minimum
    const TMP: u32 = 6;  // i64 swap temp

    // address of out element k: out + 8 + k*8
    let addr = |k: u32| vec![
        Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(k), Op::Const(Const::I32(8)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
    ];

    // inner loop: find min index in [i+1, len)
    let mut inner = vec![
        Op::LocalGet(J), Op::LocalGet(LEN), Op::BinOp(B::I32GeU), Op::BrIf(1),
        // if out[j] < out[mn] { mn = j }
    ];
    inner.extend(addr(J)); inner.push(Op::Load(LoadKind::I64));
    inner.extend(addr(MN)); inner.push(Op::Load(LoadKind::I64));
    inner.push(Op::BinOp(B::I64LtS));
    inner.push(Op::IfVoid { then: vec![Op::LocalGet(J), Op::LocalSet(MN)], else_: vec![] });
    inner.extend([Op::LocalGet(J), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(J), Op::Br(0)]);

    // outer loop body
    let mut outer = vec![
        Op::LocalGet(I), Op::LocalGet(LEN), Op::BinOp(B::I32GeU), Op::BrIf(1),
        Op::LocalGet(I), Op::LocalSet(MN),
        Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(J),
        Op::Block(vec![Op::Loop(inner)]),
    ];
    // swap out[i] and out[mn]: tmp = out[i]; out[i] = out[mn]; out[mn] = tmp
    outer.extend(addr(I)); outer.push(Op::Load(LoadKind::I64)); outer.push(Op::LocalSet(TMP));
    outer.extend(addr(I)); outer.extend(addr(MN)); outer.push(Op::Load(LoadKind::I64)); outer.push(Op::Store(StoreKind::I64));
    outer.extend(addr(MN)); outer.push(Op::LocalGet(TMP)); outer.push(Op::Store(StoreKind::I64));
    outer.extend([Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(I), Op::Br(0)]);

    let body = vec![
        Op::LocalGet(XS), Op::Load(LoadKind::I32), Op::LocalSet(LEN),
        // out = __alloc(8 + len*8); out.len = out.cap = len
        Op::Const(Const::I32(8)), Op::LocalGet(LEN), Op::Const(Const::I32(8)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::LocalGet(LEN), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(LEN), Op::Store(StoreKind::I32),
        // memcpy(out+8, xs+8, len*8)
        Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(XS), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(LEN), Op::Const(Const::I32(8)), Op::BinOp(B::I32Mul),
        Op::MemoryCopy,
        // i = 0 ; selection sort
        Op::Const(Const::I32(0)), Op::LocalSet(I),
        Op::Block(vec![Op::Loop(outer)]),
        Op::LocalGet(OUT),
    ];

    WasmFunc {
        name: "__list_sort_int".into(),
        params: vec![WasmTy::I32],
        results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I64], // len,out,i,j,mn,tmp
        body,
    }
}

/// `__string_get(s, i: i64) -> i32` — `Some(s[i])` (a 1-code-point String) or
/// None. Built on `__string_slice(s, i, i+1)`: a non-empty slice is Some.
fn build_string_get() -> WasmFunc {
    let rt = RuntimeFns::fixed();
    const S: u32 = 0;
    const I: u32 = 1;       // i64
    const SLICED: u32 = 2;
    const OPT: u32 = 3;

    let body = vec![
        // sliced = __string_slice(s, i, i+1)
        Op::LocalGet(S), Op::LocalGet(I),
        Op::LocalGet(I), Op::Const(Const::I64(1)), Op::BinOp(B::I64Add),
        Op::Call { idx: rt.string_slice, pops: 3, pushes: 1 },
        Op::LocalSet(SLICED),
        // opt = __alloc(12)
        Op::Const(Const::I32(12)),
        Op::Call { idx: rt.alloc, pops: 1, pushes: 1 },
        Op::LocalSet(OPT),
        // if sliced byte length != 0 → Some(sliced) else None
        Op::LocalGet(SLICED), Op::Load(LoadKind::I32),
        Op::IfVoid {
            then: vec![
                Op::LocalGet(OPT), Op::Const(Const::I32(1)), Op::Store(StoreKind::I32),
                Op::LocalGet(OPT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
                Op::LocalGet(SLICED), Op::Store(StoreKind::I32),
            ],
            else_: vec![Op::LocalGet(OPT), Op::Const(Const::I32(0)), Op::Store(StoreKind::I32)],
        },
        Op::LocalGet(OPT),
    ];

    WasmFunc {
        name: "__string_get".into(),
        params: vec![WasmTy::I32, WasmTy::I64],
        results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32], // sliced, opt
        body,
    }
}

/// `__string_slice(s, start: i64, end: i64) -> i32`
///
/// Returns the substring covering code points `[start, end)`. Converts the
/// code-point indices to byte offsets by scanning UTF-8 boundaries, then copies
/// that byte range into a fresh String. Out-of-range indices clamp to the ends.
fn build_string_slice() -> WasmFunc {
    let alloc_fn = RuntimeFns::fixed().alloc;
    const S: u32 = 0;      // string ptr
    const START: u32 = 1;  // i64
    const END: u32 = 2;    // i64
    const BL: u32 = 3;     // byte length
    const SI: u32 = 4;     // start cp index (i32)
    const EI: u32 = 5;     // end cp index (i32)
    const SB: u32 = 6;     // start byte offset
    const EB: u32 = 7;     // end byte offset
    const B_: u32 = 8;     // byte cursor
    const CP: u32 = 9;     // code-point counter
    const OUT: u32 = 10;   // result ptr
    const NB: u32 = 11;    // result byte count

    // byte[b] is a code-point boundary: (byte & 0xC0) != 0x80
    let is_boundary = vec![
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(B_), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
        Op::Const(Const::I32(0xC0)), Op::BinOp(B::I32And),
        Op::Const(Const::I32(0x80)), Op::BinOp(B::I32Ne),
    ];
    // on boundary: if cp==si {sb=b}; if cp==ei {eb=b}; cp++
    let on_boundary = vec![
        Op::LocalGet(CP), Op::LocalGet(SI), Op::BinOp(B::I32Eq),
        Op::IfVoid { then: vec![Op::LocalGet(B_), Op::LocalSet(SB)], else_: vec![] },
        Op::LocalGet(CP), Op::LocalGet(EI), Op::BinOp(B::I32Eq),
        Op::IfVoid { then: vec![Op::LocalGet(B_), Op::LocalSet(EB)], else_: vec![] },
        Op::LocalGet(CP), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(CP),
    ];

    let mut loop_body = vec![
        Op::LocalGet(B_), Op::LocalGet(BL), Op::BinOp(B::I32GeU), Op::BrIf(1),
    ];
    loop_body.extend(is_boundary);
    loop_body.push(Op::IfVoid { then: on_boundary, else_: vec![] });
    loop_body.extend([Op::LocalGet(B_), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(B_), Op::Br(0)]);

    let body = vec![
        Op::LocalGet(S), Op::Load(LoadKind::I32), Op::LocalSet(BL),
        // si = wrap(start) ; ei = wrap(end) ; default sb=eb=bl
        Op::LocalGet(START), Op::UnOp(U::I32WrapI64), Op::LocalSet(SI),
        Op::LocalGet(END), Op::UnOp(U::I32WrapI64), Op::LocalSet(EI),
        Op::LocalGet(BL), Op::LocalSet(SB),
        Op::LocalGet(BL), Op::LocalSet(EB),
        Op::Const(Const::I32(0)), Op::LocalSet(B_),
        Op::Const(Const::I32(0)), Op::LocalSet(CP),
        Op::Block(vec![Op::Loop(loop_body)]),
        // nb = eb - sb ; if nb < 0 → 0
        Op::LocalGet(EB), Op::LocalGet(SB), Op::BinOp(B::I32Sub), Op::LocalTee(NB),
        Op::Const(Const::I32(0)), Op::BinOp(B::I32LtS),
        Op::If {
            ty: WasmTy::I32,
            then: vec![Op::Const(Const::I32(0))],
            else_: vec![Op::LocalGet(NB)],
        },
        Op::LocalSet(NB),
        // out = __alloc(8 + nb) ; out.len = out.cap = nb
        Op::Const(Const::I32(8)), Op::LocalGet(NB), Op::BinOp(B::I32Add),
        Op::Call { idx: alloc_fn, pops: 1, pushes: 1 }, Op::LocalSet(OUT),
        Op::LocalGet(OUT), Op::LocalGet(NB), Op::Store(StoreKind::I32),
        Op::LocalGet(OUT), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(NB), Op::Store(StoreKind::I32),
        // memcpy(out+8, s+8+sb, nb)
        Op::LocalGet(OUT), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(SB), Op::BinOp(B::I32Add),
        Op::LocalGet(NB),
        Op::MemoryCopy,
        Op::LocalGet(OUT),
    ];

    WasmFunc {
        name: "__string_slice".into(),
        params: vec![WasmTy::I32, WasmTy::I64, WasmTy::I64],
        results: vec![WasmTy::I32],
        locals: vec![
            WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, // bl, si, ei, sb, eb
            WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32, // b, cp, out, nb (+1 spare)
        ],
        body,
    }
}

/// `__string_starts_with(s, p)` / `__string_ends_with(s, p)` -> i32.
///
/// Byte-compares `p` against the start (or end) of `s`. UTF-8 safe: matching
/// whole encoded substrings is a pure byte comparison. `from_end` shifts the
/// `s` offset by `len(s) - len(p)`.
fn build_prefix_cmp(name: &str, from_end: bool) -> WasmFunc {
    const S: u32 = 0;
    const P: u32 = 1;
    const SL: u32 = 2; // len(s)
    const PL: u32 = 3; // len(p)
    const I: u32 = 4;
    const OFF: u32 = 5; // base offset into s's data

    // s byte address: s + 8 + off + i ; p byte address: p + 8 + i
    let s_byte = vec![
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(OFF), Op::BinOp(B::I32Add),
        Op::LocalGet(I), Op::BinOp(B::I32Add),
        Op::Load(LoadKind::U8),
    ];
    let p_byte = vec![
        Op::LocalGet(P), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(I), Op::BinOp(B::I32Add),
        Op::Load(LoadKind::U8),
    ];

    let mut body = vec![
        Op::LocalGet(S), Op::Load(LoadKind::I32), Op::LocalSet(SL),
        Op::LocalGet(P), Op::Load(LoadKind::I32), Op::LocalSet(PL),
        // if pl > sl: return 0
        Op::LocalGet(PL), Op::LocalGet(SL), Op::BinOp(B::I32GtU),
        Op::IfVoid { then: vec![Op::Const(Const::I32(0)), Op::Return], else_: vec![] },
        // off = from_end ? sl - pl : 0
    ];
    if from_end {
        body.extend([Op::LocalGet(SL), Op::LocalGet(PL), Op::BinOp(B::I32Sub), Op::LocalSet(OFF)]);
    } else {
        body.extend([Op::Const(Const::I32(0)), Op::LocalSet(OFF)]);
    }
    body.extend([Op::Const(Const::I32(0)), Op::LocalSet(I)]);

    let mut loop_body = vec![
        Op::LocalGet(I), Op::LocalGet(PL), Op::BinOp(B::I32GeU), Op::BrIf(1),
    ];
    loop_body.extend(s_byte);
    loop_body.extend(p_byte);
    loop_body.push(Op::BinOp(B::I32Ne));
    loop_body.push(Op::IfVoid { then: vec![Op::Const(Const::I32(0)), Op::Return], else_: vec![] });
    loop_body.extend([Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(I), Op::Br(0)]);
    body.push(Op::Block(vec![Op::Loop(loop_body)]));
    body.push(Op::Const(Const::I32(1)));

    WasmFunc {
        name: name.into(),
        params: vec![WasmTy::I32, WasmTy::I32],
        results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // sl, pl, i, off
        body,
    }
}

/// `__alloc(size: i32) -> i32`
fn build_alloc(reg: &LayoutRegistry) -> WasmFunc {
    let hdr = reg.header_size(layout::ALLOC_HEADER) as i32; // 8
    let size_off = reg.fixed_offset(layout::ALLOC_HEADER, alloc::SIZE) as i32; // 0
    let rc_off = reg.fixed_offset(layout::ALLOC_HEADER, alloc::RC) as i32; // 4

    const SIZE: u32 = 0;
    const BASE: u32 = 1;
    const NH: u32 = 2;     // new heap pointer
    const NEEDED: u32 = 3; // pages required to cover NH

    let body = vec![
        // base = HEAP
        Op::GlobalGet(HEAP_GLOBAL),
        Op::LocalSet(BASE),
        // new_heap = base + ((hdr + size + 7) & ~7)
        Op::LocalGet(BASE),
        Op::LocalGet(SIZE),
        Op::Const(Const::I32(hdr + 7)),
        Op::BinOp(B::I32Add),
        Op::Const(Const::I32(!7)),
        Op::BinOp(B::I32And),
        Op::BinOp(B::I32Add),
        Op::LocalSet(NH),
        Op::LocalGet(NH),
        Op::GlobalSet(HEAP_GLOBAL),
        // Grow linear memory if the bump pointer now exceeds it. Without this
        // the next store would trap once allocation crosses the initial pages.
        // needed_pages = ceil(new_heap / 65536) = (new_heap + 65535) >> 16
        Op::LocalGet(NH),
        Op::Const(Const::I32(65535)),
        Op::BinOp(B::I32Add),
        Op::Const(Const::I32(16)),
        Op::BinOp(B::I32ShrU),
        Op::LocalSet(NEEDED),
        // if needed > memory.size { memory.grow(needed - memory.size); drop }
        Op::LocalGet(NEEDED),
        Op::MemorySize,
        Op::BinOp(B::I32GtU),
        Op::IfVoid {
            then: vec![
                Op::LocalGet(NEEDED),
                Op::MemorySize,
                Op::BinOp(B::I32Sub),
                Op::MemoryGrow,
                Op::Drop,
            ],
            else_: vec![],
        },
        // *(base + size_off) = size
        Op::LocalGet(BASE),
        Op::Const(Const::I32(size_off)),
        Op::BinOp(B::I32Add),
        Op::LocalGet(SIZE),
        Op::Store(StoreKind::I32),
        // *(base + rc_off) = 1
        Op::LocalGet(BASE),
        Op::Const(Const::I32(rc_off)),
        Op::BinOp(B::I32Add),
        Op::Const(Const::I32(1)),
        Op::Store(StoreKind::I32),
        // return base + hdr
        Op::LocalGet(BASE),
        Op::Const(Const::I32(hdr)),
        Op::BinOp(B::I32Add),
    ];

    WasmFunc {
        name: "__alloc".into(),
        params: vec![WasmTy::I32],
        results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32], // base, new_heap, needed
        body,
    }
}

/// Emit `*(ptr - rc_neg) op= 1` guarded by `ptr >= heap_start`.
fn rc_update(rc_neg: i32, heap_start: i32, delta_op: B) -> WasmFunc {
    const PTR: u32 = 0;
    let update = vec![
        // addr (for store)
        Op::LocalGet(PTR),
        Op::Const(Const::I32(rc_neg)),
        Op::BinOp(B::I32Sub),
        // current rc
        Op::LocalGet(PTR),
        Op::Const(Const::I32(rc_neg)),
        Op::BinOp(B::I32Sub),
        Op::Load(LoadKind::I32),
        Op::Const(Const::I32(1)),
        Op::BinOp(delta_op),
        Op::Store(StoreKind::I32),
    ];
    let body = vec![
        // if ptr >= heap_start { update }
        Op::LocalGet(PTR),
        Op::Const(Const::I32(heap_start)),
        Op::BinOp(B::I32GeU),
        Op::IfVoid { then: update, else_: vec![] },
    ];
    WasmFunc {
        name: String::new(), // set by caller
        params: vec![WasmTy::I32],
        results: vec![],
        locals: vec![],
        body,
    }
}

/// `__rc_inc(ptr: i32)` — increment refcount, skipping data-segment pointers.
fn build_rc_inc(reg: &LayoutRegistry, heap_start: i32) -> WasmFunc {
    let rc_neg = reg.alloc_header_neg_offset(alloc::RC) as i32;
    let mut f = rc_update(rc_neg, heap_start, B::I32Add);
    f.name = "__rc_inc".into();
    f
}

/// `__rc_dec(ptr: i32)` — decrement refcount (no free yet), skipping data-segment.
fn build_rc_dec(reg: &LayoutRegistry, heap_start: i32) -> WasmFunc {
    let rc_neg = reg.alloc_header_neg_offset(alloc::RC) as i32;
    let mut f = rc_update(rc_neg, heap_start, B::I32Sub);
    f.name = "__rc_dec".into();
    f
}

/// `__string_concat(a: i32, b: i32) -> i32`
///
/// Allocates a fresh heap String `[len][cap][bytes]` holding a's bytes
/// followed by b's. Reads only the source strings' len/data, so the sources
/// may be heap- or data-segment-resident.
fn build_string_concat() -> WasmFunc {
    let alloc_fn = RuntimeFns::fixed().alloc;
    const A: u32 = 0;
    const B_: u32 = 1;
    const LA: u32 = 2;
    const LB: u32 = 3;
    const S: u32 = 4;

    let body = vec![
        // la = len(a) ; lb = len(b)
        Op::LocalGet(A), Op::Load(LoadKind::I32), Op::LocalSet(LA),
        Op::LocalGet(B_), Op::Load(LoadKind::I32), Op::LocalSet(LB),
        // s = __alloc(8 + la + lb)
        Op::Const(Const::I32(8)),
        Op::LocalGet(LA), Op::BinOp(B::I32Add),
        Op::LocalGet(LB), Op::BinOp(B::I32Add),
        Op::Call { idx: alloc_fn, pops: 1, pushes: 1 },
        Op::LocalSet(S),
        // s.len = la + lb
        Op::LocalGet(S),
        Op::LocalGet(LA), Op::LocalGet(LB), Op::BinOp(B::I32Add),
        Op::Store(StoreKind::I32),
        // s.cap = la + lb
        Op::LocalGet(S), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        Op::LocalGet(LA), Op::LocalGet(LB), Op::BinOp(B::I32Add),
        Op::Store(StoreKind::I32),
        // memcpy(s+8, a+8, la)
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(A), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(LA),
        Op::MemoryCopy,
        // memcpy(s+8+la, b+8, lb)
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(LA), Op::BinOp(B::I32Add),
        Op::LocalGet(B_), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(LB),
        Op::MemoryCopy,
        // return s
        Op::LocalGet(S),
    ];

    WasmFunc {
        name: "__string_concat".into(),
        params: vec![WasmTy::I32, WasmTy::I32],
        results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32], // la, lb, s
        body,
    }
}

/// `__strlen(p: i32) -> i64` — byte length of a String (zero-extended).
fn build_strlen() -> WasmFunc {
    WasmFunc {
        name: "__strlen".into(),
        params: vec![WasmTy::I32],
        results: vec![WasmTy::I64],
        locals: vec![],
        body: vec![
            Op::LocalGet(0),
            Op::Load(LoadKind::I32),
            Op::UnOp(U::I64ExtendI32U),
        ],
    }
}

/// `__byte_at(p: i32, i: i64) -> i64` — the byte at index `i` (zero-extended).
fn build_byte_at() -> WasmFunc {
    WasmFunc {
        name: "__byte_at".into(),
        params: vec![WasmTy::I32, WasmTy::I64],
        results: vec![WasmTy::I64],
        locals: vec![],
        body: vec![
            // addr = p + 8 + (i as i32)
            Op::LocalGet(0),
            Op::Const(Const::I32(8)),
            Op::BinOp(B::I32Add),
            Op::LocalGet(1),
            Op::UnOp(U::I32WrapI64),
            Op::BinOp(B::I32Add),
            Op::Load(LoadKind::U8),
            Op::UnOp(U::I64ExtendI32U),
        ],
    }
}

/// `__int_to_string(n: i64) -> i32`
///
/// Renders a signed integer as a decimal String (heap-allocated). Works for 0
/// and negatives. Two passes: count digits, then fill backward from the end.
fn build_int_to_string() -> WasmFunc {
    let alloc_fn = RuntimeFns::fixed().alloc;
    const N: u32 = 0;   // param: value (i64)
    const NEG: u32 = 1; // i32: 1 if negative
    const V: u32 = 2;   // i64: |n|
    const ND: u32 = 3;  // i32: digit count
    const TOT: u32 = 4; // i32: total chars (digits + sign)
    const S: u32 = 5;   // i32: string ptr
    const P: u32 = 6;   // i32: write cursor
    const T: u32 = 7;   // i64: working value

    let body = vec![
        // neg = n < 0
        Op::LocalGet(N), Op::Const(Const::I64(0)), Op::BinOp(B::I64LtS),
        Op::LocalSet(NEG),
        // v = neg ? (0 - n) : n
        Op::LocalGet(NEG),
        Op::If {
            ty: WasmTy::I64,
            then: vec![Op::Const(Const::I64(0)), Op::LocalGet(N), Op::BinOp(B::I64Sub)],
            else_: vec![Op::LocalGet(N)],
        },
        Op::LocalSet(V),
        // nd = 1; t = v
        Op::Const(Const::I32(1)), Op::LocalSet(ND),
        Op::LocalGet(V), Op::LocalSet(T),
        // count digits: while (t /= 10) != 0 { nd++ }
        Op::Block(vec![Op::Loop(vec![
            Op::LocalGet(T), Op::Const(Const::I64(10)), Op::BinOp(B::I64DivS), Op::LocalTee(T),
            Op::Const(Const::I64(0)), Op::BinOp(B::I64Eq), Op::BrIf(1),
            Op::LocalGet(ND), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(ND),
            Op::Br(0),
        ])]),
        // total = nd + neg
        Op::LocalGet(ND), Op::LocalGet(NEG), Op::BinOp(B::I32Add), Op::LocalSet(TOT),
        // s = __alloc(8 + total); s.len = s.cap = total
        Op::Const(Const::I32(8)), Op::LocalGet(TOT), Op::BinOp(B::I32Add),
        Op::Call { idx: alloc_fn, pops: 1, pushes: 1 }, Op::LocalSet(S),
        Op::LocalGet(S), Op::LocalGet(TOT), Op::Store(StoreKind::I32),
        Op::LocalGet(S), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(TOT), Op::Store(StoreKind::I32),
        // if neg { *(s+8) = '-' }
        Op::LocalGet(NEG),
        Op::IfVoid {
            then: vec![
                Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
                Op::Const(Const::I32(45)), Op::Store(StoreKind::I8),
            ],
            else_: vec![],
        },
        // p = s + 8 + total ; t = v
        Op::LocalGet(S), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(TOT), Op::BinOp(B::I32Add), Op::LocalSet(P),
        Op::LocalGet(V), Op::LocalSet(T),
        // fill digits backward until t == 0
        Op::Block(vec![Op::Loop(vec![
            // p--
            Op::LocalGet(P), Op::Const(Const::I32(1)), Op::BinOp(B::I32Sub), Op::LocalSet(P),
            // *p = '0' + (t % 10)
            Op::LocalGet(P),
            Op::LocalGet(T), Op::Const(Const::I64(10)), Op::BinOp(B::I64RemS),
            Op::UnOp(U::I32WrapI64), Op::Const(Const::I32(48)), Op::BinOp(B::I32Add),
            Op::Store(StoreKind::I8),
            // t /= 10 ; if t == 0 break
            Op::LocalGet(T), Op::Const(Const::I64(10)), Op::BinOp(B::I64DivS), Op::LocalTee(T),
            Op::Const(Const::I64(0)), Op::BinOp(B::I64Eq), Op::BrIf(1),
            Op::Br(0),
        ])]),
        // return s
        Op::LocalGet(S),
    ];

    WasmFunc {
        name: "__int_to_string".into(),
        params: vec![WasmTy::I64],
        results: vec![WasmTy::I32],
        // neg(i32), v(i64), nd(i32), total(i32), s(i32), p(i32), t(i64)
        locals: vec![
            WasmTy::I32, WasmTy::I64, WasmTy::I32, WasmTy::I32,
            WasmTy::I32, WasmTy::I32, WasmTy::I64,
        ],
        body,
    }
}

/// `__string_eq(a: i32, b: i32) -> i32` — 1 if the strings are byte-equal.
fn build_string_eq() -> WasmFunc {
    const A: u32 = 0;
    const B_: u32 = 1;
    const LA: u32 = 2; // i32: len(a)
    const I: u32 = 3;  // i32: loop index

    let body = vec![
        // if len(a) != len(b) return 0
        Op::LocalGet(A), Op::Load(LoadKind::I32), Op::LocalTee(LA),
        Op::LocalGet(B_), Op::Load(LoadKind::I32),
        Op::BinOp(B::I32Ne),
        Op::IfVoid { then: vec![Op::Const(Const::I32(0)), Op::Return], else_: vec![] },
        // i = 0; while i < la { if a[8+i] != b[8+i] return 0; i++ }
        Op::Const(Const::I32(0)), Op::LocalSet(I),
        Op::Block(vec![Op::Loop(vec![
            Op::LocalGet(I), Op::LocalGet(LA), Op::BinOp(B::I32GeU), Op::BrIf(1),
            // a[8+i]
            Op::LocalGet(A), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
            Op::LocalGet(I), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
            // b[8+i]
            Op::LocalGet(B_), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
            Op::LocalGet(I), Op::BinOp(B::I32Add), Op::Load(LoadKind::U8),
            Op::BinOp(B::I32Ne),
            Op::IfVoid { then: vec![Op::Const(Const::I32(0)), Op::Return], else_: vec![] },
            Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(I),
            Op::Br(0),
        ])]),
        // all bytes matched
        Op::Const(Const::I32(1)),
    ];

    WasmFunc {
        name: "__string_eq".into(),
        params: vec![WasmTy::I32, WasmTy::I32],
        results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32], // la, i
        body,
    }
}

/// `__range(start: i64, end: i64, inclusive: i32) -> i32`
///
/// Builds a `List[Int]` of `[start, start+1, …]` up to `end` (exclusive, or
/// inclusive when the flag is set). Empty when the span is non-positive.
fn build_range() -> WasmFunc {
    let alloc_fn = RuntimeFns::fixed().alloc;
    const START: u32 = 0; // i64
    const END: u32 = 1;   // i64
    const INCL: u32 = 2;  // i32
    const N64: u32 = 3;   // i64: signed element count
    const N: u32 = 4;     // i32: clamped count
    const LIST: u32 = 5;  // i32
    const I: u32 = 6;     // i32: fill index

    let body = vec![
        // n64 = (end - start) + inclusive
        Op::LocalGet(END), Op::LocalGet(START), Op::BinOp(B::I64Sub),
        Op::LocalGet(INCL), Op::UnOp(U::I64ExtendI32U), Op::BinOp(B::I64Add),
        Op::LocalSet(N64),
        // n = n64 < 0 ? 0 : wrap(n64)
        Op::LocalGet(N64), Op::Const(Const::I64(0)), Op::BinOp(B::I64LtS),
        Op::If {
            ty: WasmTy::I32,
            then: vec![Op::Const(Const::I32(0))],
            else_: vec![Op::LocalGet(N64), Op::UnOp(U::I32WrapI64)],
        },
        Op::LocalSet(N),
        // list = __alloc(8 + n*8) ; list.len = list.cap = n
        Op::Const(Const::I32(8)),
        Op::LocalGet(N), Op::Const(Const::I32(8)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::Call { idx: alloc_fn, pops: 1, pushes: 1 }, Op::LocalSet(LIST),
        Op::LocalGet(LIST), Op::LocalGet(N), Op::Store(StoreKind::I32),
        Op::LocalGet(LIST), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add), Op::LocalGet(N), Op::Store(StoreKind::I32),
        // i = 0; while i < n { list[8 + i*8] = start + i; i++ }
        Op::Const(Const::I32(0)), Op::LocalSet(I),
        Op::Block(vec![Op::Loop(vec![
            Op::LocalGet(I), Op::LocalGet(N), Op::BinOp(B::I32GeU), Op::BrIf(1),
            // addr = list + 8 + i*8
            Op::LocalGet(LIST), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
            Op::LocalGet(I), Op::Const(Const::I32(8)), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
            // value = start + (i as i64)
            Op::LocalGet(START),
            Op::LocalGet(I), Op::UnOp(U::I64ExtendI32U), Op::BinOp(B::I64Add),
            Op::Store(StoreKind::I64),
            Op::LocalGet(I), Op::Const(Const::I32(1)), Op::BinOp(B::I32Add), Op::LocalSet(I),
            Op::Br(0),
        ])]),
        Op::LocalGet(LIST),
    ];

    WasmFunc {
        name: "__range".into(),
        params: vec![WasmTy::I64, WasmTy::I64, WasmTy::I32],
        results: vec![WasmTy::I32],
        locals: vec![WasmTy::I64, WasmTy::I32, WasmTy::I32, WasmTy::I32], // n64, n, list, i
        body,
    }
}

/// `__list_concat(a: i32, b: i32, elem_size: i32) -> i32`
///
/// Concatenates two lists of `elem_size`-byte elements into a fresh list.
fn build_list_concat() -> WasmFunc {
    let alloc_fn = RuntimeFns::fixed().alloc;
    const A: u32 = 0;
    const B_: u32 = 1;
    const ES: u32 = 2;  // elem_size
    const LA: u32 = 3;  // len(a)
    const LB: u32 = 4;  // len(b)
    const C: u32 = 5;   // result
    const AB: u32 = 6;  // la*elem_size (a's byte length)

    let body = vec![
        Op::LocalGet(A), Op::Load(LoadKind::I32), Op::LocalSet(LA),
        Op::LocalGet(B_), Op::Load(LoadKind::I32), Op::LocalSet(LB),
        // ab = la * elem_size
        Op::LocalGet(LA), Op::LocalGet(ES), Op::BinOp(B::I32Mul), Op::LocalSet(AB),
        // c = __alloc(8 + (la+lb)*elem_size)
        Op::Const(Const::I32(8)),
        Op::LocalGet(LA), Op::LocalGet(LB), Op::BinOp(B::I32Add),
        Op::LocalGet(ES), Op::BinOp(B::I32Mul), Op::BinOp(B::I32Add),
        Op::Call { idx: alloc_fn, pops: 1, pushes: 1 }, Op::LocalSet(C),
        // c.len = c.cap = la + lb
        Op::LocalGet(C), Op::LocalGet(LA), Op::LocalGet(LB), Op::BinOp(B::I32Add), Op::Store(StoreKind::I32),
        Op::LocalGet(C), Op::Const(Const::I32(4)), Op::BinOp(B::I32Add),
        Op::LocalGet(LA), Op::LocalGet(LB), Op::BinOp(B::I32Add), Op::Store(StoreKind::I32),
        // memcpy(c+8, a+8, ab)
        Op::LocalGet(C), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(A), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(AB),
        Op::MemoryCopy,
        // memcpy(c+8+ab, b+8, lb*elem_size)
        Op::LocalGet(C), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add), Op::LocalGet(AB), Op::BinOp(B::I32Add),
        Op::LocalGet(B_), Op::Const(Const::I32(8)), Op::BinOp(B::I32Add),
        Op::LocalGet(LB), Op::LocalGet(ES), Op::BinOp(B::I32Mul),
        Op::MemoryCopy,
        Op::LocalGet(C),
    ];

    WasmFunc {
        name: "__list_concat".into(),
        params: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32],
        results: vec![WasmTy::I32],
        locals: vec![WasmTy::I32, WasmTy::I32, WasmTy::I32, WasmTy::I32], // la, lb, c, ab
        body,
    }
}

// ── Abstract-op resolution ───────────────────────────────────────────

/// Rewrite abstract allocation / RC / string ops into concrete `Call`s.
///
/// Stack-effect preserving: `Alloc` (1→1) ↔ `__alloc`; `RcInc`/`RcDec` (1→0)
/// ↔ their fns; `StringConcat` (2→1) ↔ `__string_concat`. So a function that
/// verified before resolution still verifies after.
///
/// `StringInterp`, `AllocCollection`, `CowCheck` are left untouched and the
/// module builder's abstract-op check rejects any that remain.
pub fn resolve_abstract_ops(ops: &mut Vec<Op>, rt: &RuntimeFns) {
    for op in ops.iter_mut() {
        match op {
            Op::Alloc => *op = Op::Call { idx: rt.alloc, pops: 1, pushes: 1 },
            Op::RcInc => *op = Op::Call { idx: rt.rc_inc, pops: 1, pushes: 0 },
            Op::RcDec { .. } => *op = Op::Call { idx: rt.rc_dec, pops: 1, pushes: 0 },
            Op::StringConcat => *op = Op::Call { idx: rt.string_concat, pops: 2, pushes: 1 },
            // Recurse into compound bodies.
            Op::Block(body) | Op::Loop(body) | Op::Seq(body) => resolve_abstract_ops(body, rt),
            Op::If { then, else_, .. } | Op::IfVoid { then, else_ } => {
                resolve_abstract_ops(then, rt);
                resolve_abstract_ops(else_, rt);
            }
            Op::ListForEach { body, .. } | Op::MapForEach { body, .. } => {
                resolve_abstract_ops(body, rt)
            }
            Op::CowCheck { clone_body, .. } => resolve_abstract_ops(clone_body, rt),
            _ => {}
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::ir::verify_func_stack;

    #[test]
    fn runtime_functions_verify() {
        let reg = LayoutRegistry::new();
        for f in runtime_funcs(&reg, 1024) {
            verify_func_stack(&f).unwrap_or_else(|e| panic!("{} failed: {}", f.name, e));
        }
    }

    #[test]
    fn resolve_rewrites_alloc_rc_and_concat() {
        let rt = RuntimeFns::fixed();
        let mut ops = vec![
            Op::Const(Const::I32(16)),
            Op::Alloc,
            Op::RcInc,
            Op::StringConcat,
            Op::Block(vec![Op::RcDec { layout: layout::ALLOC_HEADER }]),
        ];
        resolve_abstract_ops(&mut ops, &rt);
        assert!(matches!(ops[1], Op::Call { idx, .. } if idx == rt.alloc));
        assert!(matches!(ops[2], Op::Call { idx, .. } if idx == rt.rc_inc));
        assert!(matches!(ops[3], Op::Call { idx, .. } if idx == rt.string_concat));
        if let Op::Block(inner) = &ops[4] {
            assert!(matches!(inner[0], Op::Call { idx, .. } if idx == rt.rc_dec));
        } else {
            panic!("block not preserved");
        }
    }
}
