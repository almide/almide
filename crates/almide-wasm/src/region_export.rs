//! A Region an `@export(wasm)` fn returns crosses as `(ptr: i32, len: i32)`
//! (ADR-0031 amendment A1), the wasm32 form of the `(base, length)` pair a
//! Region argument crosses an import as (D3).
//!
//! The IR rewrite (`extern_region::wrap_region_exports`) gives such a fn a
//! wrapper that returns the Region's block — `ffi.__export_owner(f(…))` — so
//! the Region's own record is released by the ordinary ownership rules and the
//! wrapper's caller holds one share of the block. Here the emitter exports,
//! under the declared name, an adapter that calls the wrapper and returns the
//! block's data address and length with the multi-value return. The share
//! stays with the host: the block, and every byte the host writes into it,
//! lives until the host hands the address back to the module's
//! `__region_release` export, however the program's own copies of the Region
//! come and go. An empty region crosses as `(0, 0)` and keeps nothing.
//!
//! An `effect fn` export's wrapper answers a `Result` block: on `err` the
//! adapter writes the error to stderr and traps, since `(ptr, len)` has no
//! error channel.

use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::*;

/// The export the host hands a Region's address back through.
pub(crate) const RELEASE_EXPORT: &str = "__region_release";

/// One Region-returning export: the name the declaration gave it, the
/// wrapper the adapter calls, the wrapper's parameter types, and whether the
/// wrapper answers a `Result` block (an `effect fn`).
pub(crate) struct RegionExport {
    pub(crate) name: String,
    pub(crate) inner: u32,
    pub(crate) params: Vec<ValType>,
    pub(crate) fallible: bool,
}

/// Every export name claimed so far, aligned with the emitter's
/// `export_owners`, and the Region adapters among them: plain exports and
/// adapters share the one export namespace.
#[derive(Default)]
pub(crate) struct Claims {
    names: Vec<String>,
    pub(crate) adapters: Vec<RegionExport>,
}

impl Claims {
    /// The position in `export_owners` of an earlier claim of `name`.
    pub(crate) fn prior(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|e| e == name)
    }

    /// Record the claim of `name` by `f`. True when `f` is a Region export's
    /// wrapper: its adapter is recorded (and its owner), and the caller skips
    /// the plain export, which pushes its owner itself.
    pub(crate) fn claim(&mut self, f: &almide_ir::IrFunction, info: &FnInfo, name: &str, owners: &mut Vec<String>, owner: &str) -> bool {
        self.names.push(name.to_string());
        if !crate::extern_region::is_region_export(f) {
            return false;
        }
        owners.push(owner.to_string());
        self.adapters.push(RegionExport {
            name: name.to_string(),
            inner: info.wasm_index,
            params: info.params.iter().map(|t| t.val_type()).collect(),
            fallible: matches!(info.ret, Some(SliceTy::Result(..))),
        });
        true
    }
}

fn word(offset: u32) -> MemArg {
    MemArg { offset: u64::from(offset), align: 2, memory_index: 0 }
}

/// The adapter: `(params…) -> (i32 ptr, i32 len)`.
pub(crate) fn adapter(e: &RegionExport) -> Function {
    let n = e.params.len() as u32;
    let (blk, len) = (n, n + 1);
    let mut f = Function::new([(2, ValType::I32)]);
    let mut i = f.instructions();
    for p in 0..n {
        i.local_get(p);
    }
    i.call(e.inner).local_set(blk);
    if e.fallible {
        // tag 0 = ok; an err's String sits in the same slot.
        i.local_get(blk).i32_load(word(almide_layout::PAYLOAD + almide_layout::SUM_TAG));
        i.if_(BlockType::Empty);
        i.local_get(blk).i32_load(word(almide_layout::PAYLOAD + almide_layout::SUM_FIELD)).call(F_EPRINTLN_BLOCK);
        i.unreachable();
        i.end();
        // Take the Bytes out of the box, then free the box alone.
        i.local_get(blk).i32_load(word(almide_layout::PAYLOAD + almide_layout::SUM_FIELD)).local_set(len);
        i.local_get(blk).call(F_DEC_FLAT);
        i.local_get(len).local_set(blk);
    }
    // An empty block has no data: (0, 0), and its share is released here.
    i.local_get(blk).if_(BlockType::Empty);
    i.local_get(blk).i32_load(word(almide_layout::LEN.offset)).local_set(len);
    i.end();
    i.local_get(blk).i32_eqz().local_get(len).i32_eqz().i32_or().if_(BlockType::Empty);
    i.local_get(blk).if_(BlockType::Empty);
    i.local_get(blk).call(F_DEC_FLAT);
    i.end();
    i.i32_const(0).i32_const(0).return_();
    i.end();
    i.local_get(blk).i32_const(almide_layout::PAYLOAD as i32).i32_add();
    i.local_get(len);
    i.end();
    f
}

/// `__region_release(ptr: i32)`: give back the share an adapter handed out.
/// `ptr` is a data address an adapter returned, so its block starts
/// `PAYLOAD` bytes before it; 0 (an empty region) releases nothing.
pub(crate) fn release() -> Function {
    let mut f = Function::new([]);
    let mut i = f.instructions();
    i.local_get(0).if_(BlockType::Empty);
    i.local_get(0).i32_const(almide_layout::PAYLOAD as i32).i32_sub().call(F_DEC_FLAT);
    i.end();
    i.end();
    f
}
