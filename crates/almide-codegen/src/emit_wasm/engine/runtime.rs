//! Engine runtime — the minimal set of support functions that abstract
//! WasmIR ops resolve to.
//!
//! These are themselves expressed in WasmIR (`WasmFunc`) and pass the same
//! stack-effect verifier as user code — there is no hand-written, unverified
//! runtime in the v2 engine. The module builder places them at fixed indices
//! before any user function, then `resolve_abstract_ops` rewrites
//! `Op::Alloc` / `Op::RcInc` / `Op::RcDec` into `Op::Call` targeting them.
//!
//! ## Allocator
//!
//! A bump allocator over a single linear memory. `__alloc(data_size)` reserves
//! `header(8) + data_size` bytes, writes the alloc header
//! (`[size @ base][rc=1 @ base+4]`), and returns the *data* pointer `base + 8`.
//! The header layout is taken from `LayoutRegistry` (`ALLOC_HEADER`), so the
//! runtime stays consistent with every other emission site.
//!
//! Phase 2a does not reclaim memory: `__rc_dec` decrements the count but never
//! frees (memory-safe, not space-optimal). A free-list reuse path and typed,
//! recursive child-dec are deferred to later phases.

use super::ir::{Op, Const, WasmTy, WasmFunc, FuncIdx, BinOp as B, LoadKind, StoreKind};
use super::layout::{self, LayoutRegistry, alloc};

/// Index of the mutable i32 global holding the bump pointer (next free byte).
pub const HEAP_GLOBAL: u32 = 0;

/// Initial heap pointer. Leaves the low region free (null guard + room for a
/// future data segment of string literals).
pub const HEAP_BASE: i32 = 1024;

/// Resolved indices of the runtime functions within the assembled module.
/// Runtime functions always occupy the first slots, so these are stable.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeFns {
    pub alloc: FuncIdx,
    pub rc_inc: FuncIdx,
    pub rc_dec: FuncIdx,
}

/// The number of runtime functions (they occupy indices `0..COUNT`).
pub const COUNT: u32 = 3;

impl RuntimeFns {
    /// The runtime functions occupy the first `COUNT` indices, in this order.
    pub const fn fixed() -> Self {
        RuntimeFns { alloc: 0, rc_inc: 1, rc_dec: 2 }
    }
}

/// Build the runtime functions as verified WasmIR, in index order
/// (`alloc`, `rc_inc`, `rc_dec`).
pub fn runtime_funcs(reg: &LayoutRegistry) -> Vec<WasmFunc> {
    vec![build_alloc(reg), build_rc_inc(reg), build_rc_dec(reg)]
}

/// `__alloc(size: i32) -> i32`
///
/// ```text
///   base = HEAP                       ; alloc-header start
///   HEAP = base + align8(8 + size)    ; bump
///   *base       = size                ; header.size
///   *(base + 4) = 1                   ; header.rc
///   return base + 8                   ; data pointer
/// ```
fn build_alloc(reg: &LayoutRegistry) -> WasmFunc {
    let hdr = reg.header_size(layout::ALLOC_HEADER) as i32; // 8
    let size_off = reg.fixed_offset(layout::ALLOC_HEADER, alloc::SIZE) as i32; // 0
    let rc_off = reg.fixed_offset(layout::ALLOC_HEADER, alloc::RC) as i32; // 4

    // param: size = local 0 ; scratch: base = local 1
    const SIZE: u32 = 0;
    const BASE: u32 = 1;

    let body = vec![
        // base = HEAP
        Op::GlobalGet(HEAP_GLOBAL),
        Op::LocalTee(BASE),
        // HEAP = base + align8(hdr + size) = base + ((hdr + size + 7) & ~7)
        Op::LocalGet(SIZE),
        Op::Const(Const::I32(hdr + 7)),
        Op::BinOp(B::I32Add),
        Op::Const(Const::I32(!7)), // ~7 = 0xFFFFFFF8
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

/// `__rc_inc(ptr: i32)` — increment the reference count at `ptr - rc_neg`.
fn build_rc_inc(reg: &LayoutRegistry) -> WasmFunc {
    let rc_neg = reg.alloc_header_neg_offset(alloc::RC) as i32; // 4
    const PTR: u32 = 0;

    let body = vec![
        // addr = ptr - rc_neg  (computed twice: once for store target, once for load)
        Op::LocalGet(PTR),
        Op::Const(Const::I32(rc_neg)),
        Op::BinOp(B::I32Sub),
        // load current rc
        Op::LocalGet(PTR),
        Op::Const(Const::I32(rc_neg)),
        Op::BinOp(B::I32Sub),
        Op::Load(LoadKind::I32),
        Op::Const(Const::I32(1)),
        Op::BinOp(B::I32Add),
        // store rc + 1 at addr
        Op::Store(StoreKind::I32),
    ];

    WasmFunc {
        name: "__rc_inc".into(),
        params: vec![WasmTy::I32],
        results: vec![],
        locals: vec![],
        body,
    }
}

/// `__rc_dec(ptr: i32)` — decrement the reference count at `ptr - rc_neg`.
///
/// Phase 2a never frees: it only keeps the count accurate so COW checks and
/// future reclamation see correct values.
fn build_rc_dec(reg: &LayoutRegistry) -> WasmFunc {
    let rc_neg = reg.alloc_header_neg_offset(alloc::RC) as i32;
    const PTR: u32 = 0;

    let body = vec![
        Op::LocalGet(PTR),
        Op::Const(Const::I32(rc_neg)),
        Op::BinOp(B::I32Sub),
        Op::LocalGet(PTR),
        Op::Const(Const::I32(rc_neg)),
        Op::BinOp(B::I32Sub),
        Op::Load(LoadKind::I32),
        Op::Const(Const::I32(1)),
        Op::BinOp(B::I32Sub),
        Op::Store(StoreKind::I32),
    ];

    WasmFunc {
        name: "__rc_dec".into(),
        params: vec![WasmTy::I32],
        results: vec![],
        locals: vec![],
        body,
    }
}

// ── Abstract-op resolution ───────────────────────────────────────────

/// Rewrite abstract allocation/RC ops into concrete `Call`s to the runtime.
///
/// The rewrite is stack-effect preserving (`Alloc` is pop1/push1, the runtime
/// `__alloc` is too; `RcInc`/`RcDec` are pop1/push0 like their runtime fns),
/// so a function that verified before resolution still verifies after.
///
/// Ops that have no runtime yet (`StringConcat`, `StringInterp`,
/// `AllocCollection`, `CowCheck`) are left untouched — the module builder's
/// abstract-op check rejects them with a clear diagnostic.
pub fn resolve_abstract_ops(ops: &mut Vec<Op>, rt: &RuntimeFns) {
    for op in ops.iter_mut() {
        match op {
            Op::Alloc => {
                *op = Op::Call { idx: rt.alloc, pops: 1, pushes: 1 };
            }
            Op::RcInc => {
                *op = Op::Call { idx: rt.rc_inc, pops: 1, pushes: 0 };
            }
            Op::RcDec { .. } => {
                *op = Op::Call { idx: rt.rc_dec, pops: 1, pushes: 0 };
            }
            // Recurse into compound bodies.
            Op::Block(body) | Op::Loop(body) | Op::Seq(body) => resolve_abstract_ops(body, rt),
            Op::If { then, else_ } | Op::IfVoid { then, else_ } => {
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
        for f in runtime_funcs(&reg) {
            verify_func_stack(&f).unwrap_or_else(|e| panic!("{} failed: {}", f.name, e));
        }
    }

    #[test]
    fn resolve_rewrites_alloc_and_rc() {
        let rt = RuntimeFns::fixed();
        let mut ops = vec![
            Op::Const(Const::I32(16)),
            Op::Alloc,
            Op::RcInc,
            Op::Block(vec![Op::RcDec { layout: layout::ALLOC_HEADER }]),
        ];
        resolve_abstract_ops(&mut ops, &rt);
        assert!(matches!(ops[1], Op::Call { idx: 0, .. }));
        assert!(matches!(ops[2], Op::Call { idx: 1, .. }));
        if let Op::Block(inner) = &ops[3] {
            assert!(matches!(inner[0], Op::Call { idx: 2, .. }));
        } else {
            panic!("block not preserved");
        }
    }
}
