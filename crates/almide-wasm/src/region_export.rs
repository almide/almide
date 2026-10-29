//! A Region an `@export(wasm)` fn returns crosses as `(ptr: i32, len: i32)`
//! (ADR-0031 amendment A1), the wasm32 form of the `(base, length)` pair a
//! Region argument crosses an import as (D3).
//!
//! The IR rewrite (`extern_region::wrap_region_exports`) gives such a fn a
//! wrapper that returns `ffi.__export_view(f(…))`: the Region's block with
//! one share of it, and the view's offset and length in the block, so the
//! Region's own record is released by the ordinary ownership rules. Here the
//! emitter exports, under the declared name, an adapter that calls the
//! wrapper, frees the view record's shell (keeping the block's share), and
//! returns the view's data address and length through the multi-value return.
//!
//! The share stays with the host. The adapter files `(ptr, block)` in the
//! module's export table, three globals appended after every other one: the
//! table block, its entry count and its capacity, each entry two i32 words.
//! `__region_release(ptr)` finds the entry, gives the block's share back and
//! removes the entry; a `ptr` the table does not hold — never handed out, or
//! already released — traps. So a sub-region (`ffi.slice`) crosses as its own
//! address inside its parent's block, the parent lives until the host
//! releases it, and nothing is copied. An empty view crosses as `(0, 0)` and
//! keeps nothing; releasing 0 is a no-op.
//!
//! An `effect fn` export's wrapper answers a `Result` block: on `err` the
//! adapter writes the error to stderr and traps, since `(ptr, len)` has no
//! error channel.

use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::*;

/// The export the host hands a Region's address back through.
pub(crate) const RELEASE_EXPORT: &str = "__region_release";

/// One Region-returning export: the name the declaration gave it, the
/// wrapper the adapter calls, the wrapper's parameter types, whether the
/// wrapper answers a `Result` block (an `effect fn`), and the payload offsets
/// of the view record's `owner`, `off` and `len` fields.
pub(crate) struct RegionExport {
    pub(crate) name: String,
    pub(crate) inner: u32,
    pub(crate) params: Vec<ValType>,
    pub(crate) fallible: bool,
    pub(crate) owner: u32,
    pub(crate) off: u32,
    pub(crate) len: u32,
}

/// The export table's globals: the table block, its count and capacity.
pub(crate) const TABLE_GLOBALS: u32 = 3;

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
    /// wrapper: its adapter is recorded, and the caller skips the plain
    /// export. Either way the caller records the claim's owner.
    pub(crate) fn claim(
        &mut self,
        f: &almide_ir::IrFunction,
        info: &FnInfo,
        types: &crate::types_table::TypeTable,
        name: &str,
    ) -> Result<bool, EmitError> {
        self.names.push(name.to_string());
        if !crate::extern_region::is_region_export(f) {
            return Ok(false);
        }
        let (fallible, view) = match info.ret {
            Some(SliceTy::Result(ok, _)) => (true, types.el(ok)),
            Some(t) => (false, t),
            None => return unsup("region-export: the wrapper returns nothing"),
        };
        let SliceTy::Named(i) = view else { return unsup("region-export: the view is not a record") };
        let crate::types_table::NamedDef::Record(r) = types.def(i) else { return unsup("region-export: the view is not a record") };
        let at = |n: &str| r.fields.iter().find(|x| x.name == n).map(|x| x.offset);
        let (Some(owner_at), Some(off_at), Some(len_at)) = (at("owner"), at("off"), at("len")) else {
            return unsup("region-export: the view record lacks owner/off/len");
        };
        self.adapters.push(RegionExport {
            name: name.to_string(),
            inner: info.wasm_index,
            params: info.params.iter().map(|t| t.val_type()).collect(),
            fallible,
            owner: owner_at,
            off: off_at,
            len: len_at,
        });
        Ok(true)
    }
}

fn word(offset: u32) -> MemArg {
    MemArg { offset: u64::from(offset), align: 2, memory_index: 0 }
}

fn dword(offset: u32) -> MemArg {
    MemArg { offset: u64::from(offset), align: 3, memory_index: 0 }
}

/// The adapter: `(params…) -> (i32 ptr, i32 len)`. `g` is the first of the
/// [`TABLE_GLOBALS`].
pub(crate) fn adapter(e: &RegionExport, g: u32) -> Function {
    let (tb, tn, tc) = (g, g + 1, g + 2);
    let n = e.params.len() as u32;
    let (view, blk, off, len, nb) = (n, n + 1, n + 2, n + 3, n + 4);
    let p = almide_layout::PAYLOAD;
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    for k in 0..n {
        i.local_get(k);
    }
    i.call(e.inner).local_set(view);
    if e.fallible {
        // tag 0 = ok; an err's String sits in the same slot.
        i.local_get(view).i32_load(word(p + almide_layout::SUM_TAG));
        i.if_(BlockType::Empty);
        i.local_get(view).i32_load(word(p + almide_layout::SUM_FIELD)).call(F_EPRINTLN_BLOCK);
        i.unreachable();
        i.end();
        // Take the view out of the box, then free the box alone.
        i.local_get(view).i32_load(word(p + almide_layout::SUM_FIELD)).local_set(blk);
        i.local_get(view).call(F_DEC_FLAT);
        i.local_get(blk).local_set(view);
    }
    // The view record: keep its block's share, free its shell.
    i.local_get(view).i32_load(word(p + e.owner)).local_set(blk);
    i.local_get(view).i64_load(dword(p + e.off)).i32_wrap_i64().local_set(off);
    i.local_get(view).i64_load(dword(p + e.len)).i32_wrap_i64().local_set(len);
    i.local_get(view).call(F_DEC_FLAT);
    // An empty view has no data: (0, 0), and the share is released here.
    i.local_get(blk).i32_eqz().local_get(len).i32_eqz().i32_or().if_(BlockType::Empty);
    i.local_get(blk).if_(BlockType::Empty);
    i.local_get(blk).call(F_DEC_FLAT);
    i.end();
    i.i32_const(0).i32_const(0).return_();
    i.end();
    // Grow the table when it is full (capacity doubles, from 8).
    i.global_get(tn).global_get(tc).i32_eq().if_(BlockType::Empty);
    i.global_get(tc).i32_const(1).i32_shl().i32_const(8).global_get(tc).select().local_set(nb);
    // (tc == 0 picks 8: `select` keeps the first operand when the test is nonzero)
    i.local_get(nb).i32_const(3).i32_shl().call(F_ALLOC).local_set(view);
    i.global_get(tb).if_(BlockType::Empty);
    i.local_get(view).i32_const(p as i32).i32_add();
    i.global_get(tb).i32_const(p as i32).i32_add();
    i.global_get(tn).i32_const(3).i32_shl();
    i.call(F_COPY);
    i.global_get(tb).call(F_DEC_FLAT);
    i.end();
    i.local_get(view).global_set(tb);
    i.local_get(nb).global_set(tc);
    i.end();
    // The data address, filed against the block.
    i.local_get(blk).i32_const(p as i32).i32_add().local_get(off).i32_add().local_set(off);
    i.global_get(tb).global_get(tn).i32_const(3).i32_shl().i32_add().local_tee(nb);
    i.local_get(off).i32_store(word(p));
    i.local_get(nb).local_get(blk).i32_store(word(p + 4));
    i.global_get(tn).i32_const(1).i32_add().global_set(tn);
    i.local_get(off);
    i.local_get(len);
    i.end();
    f
}

/// `__region_release(ptr: i32)`: give back the share an adapter filed under
/// `ptr`, and remove the entry (the last one takes its place). 0 (an empty
/// view) releases nothing; any other address the table does not hold traps.
pub(crate) fn release(g: u32) -> Function {
    let (tb, tn) = (g, g + 1);
    let p = almide_layout::PAYLOAD;
    let (k, at, last) = (1u32, 2u32, 3u32);
    let mut f = Function::new([(3, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(0).i32_eqz().if_(BlockType::Empty);
    i.return_();
    i.end();
    i.i32_const(0).local_set(k);
    i.loop_(BlockType::Empty);
    i.local_get(k).global_get(tn).i32_ge_u().if_(BlockType::Empty);
    i.unreachable();
    i.end();
    i.global_get(tb).local_get(k).i32_const(3).i32_shl().i32_add().local_tee(at);
    i.i32_load(word(p)).local_get(0).i32_eq().if_(BlockType::Empty);
    // Found: release the block's share, move the last entry here.
    i.local_get(at).i32_load(word(p + 4)).call(F_DEC_FLAT);
    i.global_get(tn).i32_const(1).i32_sub().global_set(tn);
    i.global_get(tb).global_get(tn).i32_const(3).i32_shl().i32_add().local_set(last);
    i.local_get(at).local_get(last).i64_load(dword(p)).i64_store(dword(p));
    i.return_();
    i.end();
    i.local_get(k).i32_const(1).i32_add().local_set(k);
    i.br(0);
    i.end();
    i.end();
    f
}
