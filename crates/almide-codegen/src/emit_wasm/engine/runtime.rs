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

/// Resolved indices of the runtime functions. Runtime functions always occupy
/// the first slots in the module, so these are stable.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeFns {
    pub alloc: FuncIdx,
    pub rc_inc: FuncIdx,
    pub rc_dec: FuncIdx,
    pub string_concat: FuncIdx,
    pub strlen: FuncIdx,
    pub byte_at: FuncIdx,
    pub int_to_string: FuncIdx,
    pub string_eq: FuncIdx,
}

/// The number of runtime functions (they occupy indices `0..COUNT`).
pub const COUNT: u32 = 8;

impl RuntimeFns {
    /// The runtime functions occupy the first `COUNT` indices, in this order.
    pub const fn fixed() -> Self {
        RuntimeFns {
            alloc: 0,
            rc_inc: 1,
            rc_dec: 2,
            string_concat: 3,
            strlen: 4,
            byte_at: 5,
            int_to_string: 6,
            string_eq: 7,
        }
    }

    /// Map of runtime function names to indices, for the build's name lookup.
    pub fn name_table(&self) -> [(&'static str, FuncIdx); 8] {
        [
            ("__alloc", self.alloc),
            ("__rc_inc", self.rc_inc),
            ("__rc_dec", self.rc_dec),
            ("__string_concat", self.string_concat),
            ("__strlen", self.strlen),
            ("__byte_at", self.byte_at),
            ("__int_to_string", self.int_to_string),
            ("__string_eq", self.string_eq),
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
    ]
}

/// `__alloc(size: i32) -> i32`
fn build_alloc(reg: &LayoutRegistry) -> WasmFunc {
    let hdr = reg.header_size(layout::ALLOC_HEADER) as i32; // 8
    let size_off = reg.fixed_offset(layout::ALLOC_HEADER, alloc::SIZE) as i32; // 0
    let rc_off = reg.fixed_offset(layout::ALLOC_HEADER, alloc::RC) as i32; // 4

    const SIZE: u32 = 0;
    const BASE: u32 = 1;

    let body = vec![
        // base = HEAP
        Op::GlobalGet(HEAP_GLOBAL),
        Op::LocalTee(BASE),
        // HEAP = base + ((hdr + size + 7) & ~7)
        Op::LocalGet(SIZE),
        Op::Const(Const::I32(hdr + 7)),
        Op::BinOp(B::I32Add),
        Op::Const(Const::I32(!7)),
        Op::BinOp(B::I32And),
        Op::BinOp(B::I32Add),
        Op::GlobalSet(HEAP_GLOBAL),
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
        locals: vec![WasmTy::I32], // base
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
