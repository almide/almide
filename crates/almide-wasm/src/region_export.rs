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
//!
//! #4519 (amendment A1c): an export declared `@export(wasm, "sym", returns:
//! address)` crosses as the address alone, one result in the address width,
//! for a host that already knows the length it asked for. The adapter is the
//! same one: it keeps a share, answers 0 for an empty Region and traps on
//! `err`; it leaves the length off the results.
//!
//! Amendment A1d: such an export holds ONE host share, a lease, in two
//! globals of its own (the address and the block) appended after the table
//! globals, instead of an entry in the table. A result whose address and
//! block are the lease's already gives its new share back at once; any other
//! result ends the previous lease (its block's share is given back) and
//! takes its place; an empty result ends it too. So a host that never calls
//! `__region_release` holds at most one block per address-only export, and an
//! address stays valid until its export's next call. `__region_release(ptr)`
//! of the current lease's address ends it early; an address its export has
//! since superseded is held by nothing, and releasing it traps like any
//! other address the module does not hold.
//!
//! #4376: an `@export(wasm)` `effect fn` whose declared return is a scalar
//! (`Int`, `Float`, `Bool`, `Unit`, the sized numbers) gets an adapter too.
//! It calls the fn, traps on `err` the same way, and returns the ok value
//! itself (nothing for `Unit`), so every host reads the scalar off the raw
//! export instead of a `Result` block address it would have to decode and
//! release.

use wasm_encoder::{BlockType, Function, MemArg, ValType};

use crate::*;

/// The module's memory is 32-bit (wasm32); the assembler declares it from
/// this. An address or length crossing
/// the host boundary (a Region's `(base, len)`, ADR-0031 D3 / A1, #4354)
/// takes this memory's width: `i32` here, the emitter's `i64` on memory64.
pub(crate) const MEMORY64: bool = false;

/// The export the host hands a Region's address back through.
pub(crate) const RELEASE_EXPORT: &str = "__region_release";

/// One export the emitter puts an adapter in front of: the name the
/// declaration gave it, the wrapper the adapter calls, the wrapper's
/// parameter types, and the positions among them that cross in the memory's
/// address width (a lent Region's `(base, len)`, #4354). `view` is set for an
/// export returning a Region: the adapter splits the view record into
/// `(ptr, len)`. Without it the adapter only narrows the parameters and
/// returns the wrapper's own results (`results`).
pub(crate) struct RegionExport {
    pub(crate) name: String,
    pub(crate) inner: u32,
    pub(crate) params: Vec<ValType>,
    pub(crate) narrow: Vec<u32>,
    pub(crate) view: Option<ViewAt>,
    /// #4376: the wrapper answers a `Result` block over a scalar, and the
    /// adapter returns the ok value itself. `Some(None)` is a `Unit` ok.
    pub(crate) ok_value: Option<Option<ValType>>,
    pub(crate) results: Vec<ValType>,
}

/// Whether the wrapper answers a `Result` block (an `effect fn`), and the
/// payload offsets of the view record's `owner`, `off` and `len` fields.
pub(crate) struct ViewAt {
    pub(crate) fallible: bool,
    /// #4519: the export answers the address alone (`returns: address`).
    pub(crate) address_only: bool,
    /// A1d: the export's lease slot among the address-only exports (its two
    /// globals follow the table's), meaningful when `address_only`.
    pub(crate) lease: u32,
    pub(crate) owner: u32,
    pub(crate) off: u32,
    pub(crate) len: u32,
}

impl RegionExport {
    /// The adapter's parameter types: an address or length is an `i32` on a
    /// 32-bit memory, and stays the emitter's `i64` scalar on memory64.
    pub(crate) fn export_params(&self) -> Vec<ValType> {
        self.params
            .iter()
            .enumerate()
            .map(|(k, v)| if self.narrowed(k as u32) { ValType::I32 } else { *v })
            .collect()
    }

    /// The adapter's results: `(ptr, len)` for a Region, the ok value for a
    /// scalar effect fn (#4376), else the wrapper's.
    pub(crate) fn export_results(&self) -> Vec<ValType> {
        match (&self.view, self.ok_value) {
            (Some(at), _) if at.address_only => vec![ValType::I32],
            (Some(_), _) => vec![ValType::I32, ValType::I32],
            (None, Some(v)) => v.into_iter().collect(),
            (None, None) => self.results.clone(),
        }
    }

    fn narrowed(&self, k: u32) -> bool {
        !MEMORY64 && self.narrow.contains(&k) && self.params.get(k as usize) == Some(&ValType::I64)
    }

    /// Does the adapter hand out a share the host gives back?
    pub(crate) fn hands_out_share(&self) -> bool {
        self.view.is_some()
    }
}

/// What a claim made of an export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Claimed {
    /// An ordinary export: the caller exports the fn itself.
    Plain,
    /// A Region return: the adapter answers `(ptr, len)`.
    Region,
    /// A Region return declared `returns: address` (#4519): the adapter
    /// answers the address alone.
    RegionAddress,
    /// Lent Region parameters only: the adapter narrows them.
    Narrowed,
    /// A scalar `effect fn` (#4376): the adapter returns the ok value and
    /// traps on `err` (and narrows any lent Region parameters).
    OkValue,
}

/// The export table's globals: the table block, its count and capacity.
pub(crate) const TABLE_GLOBALS: u32 = 3;

/// A1d: the lease slots, one per address-only export; each is two i32
/// globals (address, block) after the [`TABLE_GLOBALS`].
pub(crate) fn lease_slots(adapters: &[RegionExport]) -> u32 {
    adapters.iter().filter(|a| a.view.as_ref().is_some_and(|v| v.address_only)).count() as u32
}

/// The globals of lease slot `k`: its address and its block.
fn lease_globals(g: u32, k: u32) -> (u32, u32) {
    (g + TABLE_GLOBALS + 2 * k, g + TABLE_GLOBALS + 2 * k + 1)
}

/// End the lease in `(la, lb)`: give its block's share back, clear the slot.
fn end_lease(i: &mut wasm_encoder::InstructionSink<'_>, (la, lb): (u32, u32)) {
    i.global_get(lb).if_(BlockType::Empty);
    i.global_get(lb).call(F_DEC_FLAT);
    i.end();
    i.i32_const(0).global_set(la);
    i.i32_const(0).global_set(lb);
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

    /// Record the claim of `name` by `f`. A Region export's wrapper, or an
    /// export whose parameters cross in the address width, gets its adapter
    /// recorded here, and the caller skips the plain export. Either way the
    /// caller records the claim's owner.
    pub(crate) fn claim(
        &mut self,
        f: &almide_ir::IrFunction,
        info: &FnInfo,
        types: &crate::types_table::TypeTable,
        name: &str,
    ) -> Result<Claimed, EmitError> {
        self.names.push(name.to_string());
        let narrow = crate::extern_region::address_params(f);
        let region = crate::extern_region::is_region_export(f);
        let ok_value = if region { None } else { scalar_ok(f, info, types) };
        if !region && narrow.is_empty() && ok_value.is_none() {
            return Ok(Claimed::Plain);
        }
        let view = if region {
            let (fallible, view) = match info.ret {
                Some(SliceTy::Result(ok, _)) => (true, types.el(ok)),
                Some(t) => (false, t),
                None => return unsup("region-export: the wrapper returns nothing"),
            };
            let SliceTy::Named(i) = view else { return unsup("region-export: the view is not a record") };
            let crate::types_table::NamedDef::Record(r) = types.def(i) else { return unsup("region-export: the view is not a record") };
            let at = |n: &str| r.fields.iter().find(|x| x.name == n).map(|x| x.offset);
            let (Some(owner), Some(off), Some(len)) = (at("owner"), at("off"), at("len")) else {
                return unsup("region-export: the view record lacks owner/off/len");
            };
            // The declaration's own word, read from the export the wrapper
            // carries (`wrap_region_exports` keeps the wasm attr on it).
            let address_only = f.export_attrs.iter().any(|a| a.target.as_str() == "wasm" && a.returns_address);
            let lease = lease_slots(&self.adapters);
            Some(ViewAt { fallible, address_only, lease, owner, off, len })
        } else {
            None
        };
        let address_only = view.as_ref().is_some_and(|v| v.address_only);
        self.adapters.push(RegionExport {
            name: name.to_string(),
            inner: info.wasm_index,
            params: info.params.iter().map(|t| t.val_type()).collect(),
            narrow,
            view,
            ok_value,
            results: info.ret.iter().map(|t| t.val_type()).collect(),
        });
        Ok(match (region, ok_value) {
            (true, _) if address_only => Claimed::RegionAddress,
            (true, _) => Claimed::Region,
            (false, Some(_)) => Claimed::OkValue,
            (false, None) => Claimed::Narrowed,
        })
    }
}

/// #4376: does `f`, a DECLARED `@export(wasm)` `effect fn` whose declared
/// return is a scalar, cross as its ok value? Its wrapper answers a `Result`
/// block whose ok slot is an `Int` / `Float` / `Bool` (the sized numbers
/// among them) or `Unit`. `Some(None)` is a `Unit` ok, which crosses as no
/// result. A declared `Result` / `Option` return, a heap ok (a `String`, a
/// block) and an undeclared (implicit `pub`) export keep the block.
fn scalar_ok(f: &almide_ir::IrFunction, info: &FnInfo, types: &crate::types_table::TypeTable) -> Option<Option<ValType>> {
    use almide_types::types::{TypeConstructorId, Ty};
    if !f.is_effect || !f.export_attrs.iter().any(|a| a.target.as_str() == "wasm") {
        return None;
    }
    if matches!(&f.ret_ty, Ty::Applied(c, _) if matches!(c, TypeConstructorId::Result | TypeConstructorId::Option)) {
        return None;
    }
    let Some(SliceTy::Result(ok, _)) = info.ret else { return None };
    match types.el(ok) {
        SliceTy::Scalar(s @ (Scalar::Int | Scalar::Float | Scalar::Bool)) => Some(Some(s.val_type())),
        SliceTy::Unit => Some(None),
        _ => None,
    }
}

/// On `err` (tag nonzero) write the err String in the Result block `res` to
/// stderr and trap: the export's results have no error channel.
fn trap_on_err(i: &mut wasm_encoder::InstructionSink<'_>, res: u32) {
    let p = almide_layout::PAYLOAD;
    i.local_get(res).i32_load(word(p + almide_layout::SUM_TAG));
    i.if_(BlockType::Empty);
    i.local_get(res).i32_load(word(p + almide_layout::SUM_FIELD)).call(F_EPRINTLN_BLOCK);
    i.unreachable();
    i.end();
}

fn word(offset: u32) -> MemArg {
    MemArg { offset: u64::from(offset), align: 2, memory_index: 0 }
}

fn dword(offset: u32) -> MemArg {
    MemArg { offset: u64::from(offset), align: 3, memory_index: 0 }
}

/// The adapter: `(params…) -> (i32 ptr, i32 len)`, or `(params…) -> i32 ptr`
/// for an address-only export (#4519). `g` is the first of the
/// [`TABLE_GLOBALS`].
pub(crate) fn adapter(e: &RegionExport, g: u32) -> Function {
    let n = e.params.len() as u32;
    let Some(at) = &e.view else {
        // Widen each address parameter and call. Narrowing only: return the
        // wrapper's results. A scalar effect fn (#4376): trap on err, else
        // return the ok value and free the Result block (its ok is no heap).
        let res = n;
        let mut f = Function::new(if e.ok_value.is_some() { vec![(1, ValType::I32)] } else { vec![] });
        let mut i = f.instructions();
        for k in 0..n {
            i.local_get(k);
            if e.narrowed(k) {
                i.i64_extend_i32_u();
            }
        }
        i.call(e.inner);
        if let Some(val) = e.ok_value {
            let at = almide_layout::PAYLOAD + almide_layout::SUM_FIELD;
            i.local_set(res);
            trap_on_err(&mut i, res);
            match val {
                Some(ValType::I64) => {
                    i.local_get(res).i64_load(dword(at));
                }
                Some(ValType::F64) => {
                    i.local_get(res).f64_load(dword(at));
                }
                Some(_) => {
                    i.local_get(res).i32_load(word(at));
                }
                None => {}
            }
            i.local_get(res).call(F_DEC_FLAT);
        }
        i.end();
        return f;
    };
    let (tb, tn, tc) = (g, g + 1, g + 2);
    let (view, blk, off, len, nb) = (n, n + 1, n + 2, n + 3, n + 4);
    let p = almide_layout::PAYLOAD;
    let mut f = Function::new([(5, ValType::I32)]);
    let mut i = f.instructions();
    for k in 0..n {
        i.local_get(k);
        if e.narrowed(k) {
            i.i64_extend_i32_u();
        }
    }
    i.call(e.inner).local_set(view);
    if at.fallible {
        // tag 0 = ok; an err's String sits in the same slot.
        trap_on_err(&mut i, view);
        // Take the view out of the box, then free the box alone.
        i.local_get(view).i32_load(word(p + almide_layout::SUM_FIELD)).local_set(blk);
        i.local_get(view).call(F_DEC_FLAT);
        i.local_get(blk).local_set(view);
    }
    // The view record: keep its block's share, free its shell.
    i.local_get(view).i32_load(word(p + at.owner)).local_set(blk);
    i.local_get(view).i64_load(dword(p + at.off)).i32_wrap_i64().local_set(off);
    i.local_get(view).i64_load(dword(p + at.len)).i32_wrap_i64().local_set(len);
    i.local_get(view).call(F_DEC_FLAT);
    if at.address_only {
        lease_tail(&mut i, lease_globals(g, at.lease), (blk, off, len));
        i.end();
        return f;
    }
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
    i.local_get(off).local_get(len);
    i.end();
    f
}

/// A1d: an address-only export's result, held as its lease. `blk` holds the
/// new result's one share, `off` its offset in the block; the tail leaves the
/// address (0 for an empty result) as the function's one result.
fn lease_tail(i: &mut wasm_encoder::InstructionSink<'_>, (la, lb): (u32, u32), (blk, off, len): (u32, u32, u32)) {
    let p = almide_layout::PAYLOAD;
    // An empty result has no data: its share goes back, the previous lease
    // ends (its validity is "until the next call"), and the answer is 0.
    i.local_get(blk).i32_eqz().local_get(len).i32_eqz().i32_or().if_(BlockType::Empty);
    i.local_get(blk).if_(BlockType::Empty);
    i.local_get(blk).call(F_DEC_FLAT);
    i.end();
    end_lease(i, (la, lb));
    i.i32_const(0).return_();
    i.end();
    i.local_get(blk).i32_const(p as i32).i32_add().local_get(off).i32_add().local_set(off);
    // The lease's own address and block again: the one share it holds is
    // enough, so the new one goes straight back.
    i.local_get(off).global_get(la).i32_eq().local_get(blk).global_get(lb).i32_eq().i32_and().if_(BlockType::Empty);
    i.local_get(blk).call(F_DEC_FLAT);
    i.local_get(off).return_();
    i.end();
    // Any other result supersedes the lease.
    end_lease(i, (la, lb));
    i.local_get(off).global_set(la);
    i.local_get(blk).global_set(lb);
    i.local_get(off);
}

/// `__region_release(ptr: i32)`: give back the share an adapter filed under
/// `ptr`, and remove the entry (the last one takes its place). 0 (an empty
/// view) releases nothing. An address the table does not hold may be one of
/// the `leases` slots' (A1d): that lease ends early. Any other address traps.
pub(crate) fn release(g: u32, leases: u32) -> Function {
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
    for slot in 0..leases {
        let (la, lb) = lease_globals(g, slot);
        i.global_get(la).local_get(0).i32_eq().if_(BlockType::Empty);
        end_lease(&mut i, (la, lb));
        i.return_();
        i.end();
    }
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
