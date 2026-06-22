//! List stdlib helper methods for WASM codegen.
//!
//! Utility functions used by both calls_list.rs and calls_list_closure.rs:
//! list_elem_ty, emit_elem_copy, emit_elem_store.

use super::FuncCompiler;
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use wasm_encoder::{Instruction, ValType};
use super::engine::layout::{LIST, list as ll};

/// Upper bound for `emit_clamp_count_to_i32` — the value a too-large `Int`
/// count saturates to (mirrors native's `min(n, len)` / capacity-ceiling).
#[derive(Clone, Copy)]
pub(super) enum ClampHi {
    /// A runtime list length held in this i32 local (always >= 0).
    LenLocal(u32),
    /// A compile-time non-negative element-count ceiling (e.g. `repeat`'s
    /// byte-budget cap). The relationship to its byte budget is named at the
    /// call site (`MAX_REPEAT_*` / element size), never a raw literal.
    Const(i64),
}

/// Distinguishes the element shapes supported by `emit_list_sort_generic`.
/// Each variant knows its element size, load/store width, and comparison strategy.
enum SortKind {
    /// i64 elements, 8 bytes, inline `i64_le_s` comparison.
    Int,
    /// f64 elements, 8 bytes, inline `f64_le` comparison. NaNs compare false
    /// on any axis; insertion sort tolerates this by leaving them in place.
    Float,
    /// i32 string-pointer elements, 4 bytes, `__str_cmp` call + `i32_le_s`.
    String,
    /// i32 List[String]-pointer elements, 4 bytes, `__list_list_str_cmp` call + `i32_le_s`.
    ListString,
    /// Any totally-ordered element type whose width comes from `byte_size` and
    /// whose comparison routes through the shared `emit_ord_cmp3` total-order
    /// emitter. Covers Bool, Tuple, Option, nested List, variants — everything
    /// `emit_ord_cmp3` handles — so `list.sort` is no longer an ICE for those.
    Ord(Ty),
}

impl SortKind {
    /// Whether the sorted elements are heap POINTERS the result list must
    /// own (drives the post-build dup walk under real frees).
    fn elems_are_heap(&self) -> bool {
        match self {
            SortKind::Int | SortKind::Float => false,
            SortKind::String | SortKind::ListString => true,
            SortKind::Ord(t) => crate::pass_perceus::is_heap_type(t),
        }
    }
}

impl SortKind {
    fn elem_size(&self) -> u32 {
        match self {
            SortKind::Int | SortKind::Float => 8,
            SortKind::Ord(ty) => values::byte_size(ty),
            _ => 4,
        }
    }
    /// WASM load width for one element. For `Ord(ty)` it follows the type's
    /// natural bucket (i64 for Int, f64 for Float, i32 for Bool/pointers).
    fn emit_load(&self, f: &mut super::TrackedFunction) {
        match self {
            SortKind::Int => { f.instruction(&Instruction::I64Load(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            SortKind::Float => { f.instruction(&Instruction::F64Load(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            SortKind::Ord(ty) => Self::emit_ld_for(f, ty),
            _ => { f.instruction(&Instruction::I32Load(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 })); }
        }
    }
    fn emit_store(&self, f: &mut super::TrackedFunction) {
        match self {
            SortKind::Int => { f.instruction(&Instruction::I64Store(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            SortKind::Float => { f.instruction(&Instruction::F64Store(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            SortKind::Ord(ty) => Self::emit_st_for(f, ty),
            _ => { f.instruction(&Instruction::I32Store(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 })); }
        }
    }
    fn emit_copy_one(&self, f: &mut super::TrackedFunction) {
        self.emit_load(f);
        self.emit_store(f);
    }

    /// Free function variants of load/store keyed on the WASM bucket of `ty`.
    /// Static so they don't need a `FuncCompiler` borrow inside `SortKind`.
    fn emit_ld_for(f: &mut super::TrackedFunction, ty: &Ty) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => { f.instruction(&Instruction::I64Load(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            Some(ValType::F64) => { f.instruction(&Instruction::F64Load(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            Some(ValType::F32) => { f.instruction(&Instruction::F32Load(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 })); }
            _ => { f.instruction(&Instruction::I32Load(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 })); }
        }
    }
    fn emit_st_for(f: &mut super::TrackedFunction, ty: &Ty) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => { f.instruction(&Instruction::I64Store(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            Some(ValType::F64) => { f.instruction(&Instruction::F64Store(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            Some(ValType::F32) => { f.instruction(&Instruction::F32Store(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 })); }
            _ => { f.instruction(&Instruction::I32Store(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 })); }
        }
    }
}

impl FuncCompiler<'_> {
    pub(super) fn list_elem_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int }
    }

    /// Resolve the element type of a list expression.
    ///
    /// After the `ConcretizeTypes` pass runs, `list_expr.ty` is reliably a
    /// concrete `Applied(List, [T])`, so the happy path is a single lookup.
    /// The remaining branches are safety nets for IR paths that ConcretizeTypes
    /// may not touch (e.g. error recovery paths, edge cases in lifted closures).
    pub(super) fn resolve_list_elem(&self, list_expr: &IrExpr, fn_expr: Option<&IrExpr>) -> Ty {
        // Primary: the expression's type (set by ConcretizeTypes)
        if let Ty::Applied(_, args) = &list_expr.ty {
            if let Some(t) = args.first().filter(|t| !t.has_unresolved_deep()) {
                return t.clone();
            }
        }
        // Safety net: VarTable for Var / EnvLoad
        let vt_ty = match &list_expr.kind {
            almide_ir::IrExprKind::Var { id } => Some(&self.var_table.get(*id).ty),
            almide_ir::IrExprKind::EnvLoad { env_var, .. } => Some(&self.var_table.get(*env_var).ty),
            _ => None,
        };
        if let Some(Ty::Applied(_, a)) = vt_ty {
            if let Some(t) = a.first().filter(|t| !t.has_unresolved_deep()) {
                return t.clone();
            }
        }
        // Safety net: closure/lambda first param (for map/filter/each)
        if let Some(fn_e) = fn_expr {
            if let Ty::Fn { params, .. } = &fn_e.ty {
                if let Some(t) = params.first().filter(|t| !t.has_unresolved_deep()) {
                    return t.clone();
                }
            }
            if let almide_ir::IrExprKind::Lambda { params, .. } = &fn_e.kind {
                if let Some((_, t)) = params.first().filter(|(_, t)| !t.has_unresolved_deep()) {
                    return t.clone();
                }
            }
        }
        // Final fallback: Int (best-effort, likely produces wrong but sized code)
        Ty::Int
    }

    /// Resolve the concrete return type of a closure argument. Handles the case
    /// where the closure's `Ty::Fn.ret` is Unknown/TypeVar by falling back to:
    /// 1. Lambda body's own `.ty` (pre-closure-conversion)
    /// 2. The lifted WASM function's registered return ValType (post-closure-conversion)
    ///
    /// The ValType result is coarser than a `Ty` (it can't distinguish String
    /// from List or other heap types) but is sufficient for sizing decisions
    /// and for picking the correct call_indirect signature.
    pub(super) fn resolve_closure_ret_valtype(&self, fn_expr: &IrExpr) -> Option<ValType> {
        // 1. Fn type's ret
        if let Ty::Fn { ret, .. } = &fn_expr.ty {
            if !ret.is_unresolved() {
                return values::ty_to_valtype(ret);
            }
        }
        // 2. Lambda body's type
        if let almide_ir::IrExprKind::Lambda { body, .. } = &fn_expr.kind {
            if !body.ty.is_unresolved() {
                return values::ty_to_valtype(&body.ty);
            }
        }
        // 3. ClosureCreate: look up the lifted function's registered WASM type
        if let almide_ir::IrExprKind::ClosureCreate { func_name, .. } = &fn_expr.kind {
            if let Some(&func_idx) = self.emitter.func_map.get(func_name.as_str()) {
                if let Some(&type_idx) = self.emitter.func_type_indices.get(&func_idx) {
                    if let Some((_params, results)) = self.emitter.types.get(type_idx as usize) {
                        return results.first().copied();
                    }
                }
            }
        }
        None
    }

    /// Resolve the concrete return *type* of a closure argument as a `Ty`.
    ///
    /// Unlike `resolve_closure_ret_valtype` (which collapses to a coarse
    /// `ValType`), this keeps the source-level `Ty` so callers can size the
    /// result *and* pick the right load/store width and `call_indirect`
    /// signature. Resolution order mirrors `emit_list_map`:
    ///   1. `Ty::Fn.ret`            (post-inference, usually concrete)
    ///   2. Lambda body's own `.ty` (pre-closure-conversion)
    ///   3. lifted closure's registered WASM return ValType → placeholder `Ty`
    /// Falls back to `fallback` when every source is unresolved.
    pub(super) fn resolve_closure_ret_ty(&self, fn_expr: &IrExpr, fallback: &Ty) -> Ty {
        let mut ret = fallback.clone();
        if let Ty::Fn { ret: r, .. } = &fn_expr.ty {
            if !r.is_unresolved() {
                ret = (**r).clone();
            }
        }
        if ret.is_unresolved() {
            if let almide_ir::IrExprKind::Lambda { body, .. } = &fn_expr.kind {
                if !body.ty.is_unresolved() {
                    ret = body.ty.clone();
                }
            }
        }
        // Final reconciliation with the lifted closure's actual WASM signature,
        // same as emit_list_map: if the registered ret valtype disagrees with
        // the `Ty` we derived (e.g. inference left it Unknown→i32 but the
        // closure really returns i64), trust the registered ABI width.
        let ret_vt = values::ty_to_valtype(&ret);
        match self.resolve_closure_ret_valtype(fn_expr) {
            Some(actual) if Some(actual) != ret_vt => values::vt_to_placeholder_ty(actual),
            _ => ret,
        }
    }

    /// Resolve the concrete type of the first non-env parameter of a closure
    /// argument. Like `resolve_closure_ret_valtype` but for the input side.
    /// Used to size the `param_ty`/`in_elem_ty` in `emit_list_map` etc. when
    /// type inference left the list element type unresolved.
    pub(super) fn resolve_closure_param_valtype(&self, fn_expr: &IrExpr, idx: usize) -> Option<ValType> {
        if let Ty::Fn { params, .. } = &fn_expr.ty {
            if let Some(p) = params.get(idx) {
                if !p.is_unresolved() { return values::ty_to_valtype(p); }
            }
        }
        if let almide_ir::IrExprKind::Lambda { params, .. } = &fn_expr.kind {
            if let Some((_, pty)) = params.get(idx) {
                if !pty.is_unresolved() { return values::ty_to_valtype(pty); }
            }
        }
        if let almide_ir::IrExprKind::ClosureCreate { func_name, .. } = &fn_expr.kind {
            if let Some(&func_idx) = self.emitter.func_map.get(func_name.as_str()) {
                if let Some(&type_idx) = self.emitter.func_type_indices.get(&func_idx) {
                    if let Some((params, _results)) = self.emitter.types.get(type_idx as usize) {
                        // WASM param layout for a lifted closure: [env_i32, user_params...].
                        // `idx` is the user-level param index (0-based), so skip env.
                        return params.get(idx + 1).copied();
                    }
                }
            }
        }
        None
    }

    /// Narrow an i64 element-COUNT to an i32 SATURATED to `[0, hi]` **before**
    /// the `i32_wrap_i64` narrowing — the only correct order.
    ///
    /// Almide `Int` is i64; list/string sizing arithmetic (alloc size,
    /// `len - n`, copy bounds) runs in i32. A bare `i32_wrap_i64` of a count
    /// like `2^32-1`, `2^32`, or a negative `-1` truncates *before* any clamp,
    /// so the downstream `len - n` underflows (OOB read → trap or a corrupt
    /// length exposing uninitialized heap), silently no-ops, or — for `chunk`'s
    /// `i32_div_u` — divides by a wrapped 0.
    ///
    /// Native is the oracle, and it treats a count as **UNSIGNED**: it casts the
    /// i64 to `usize` (`n as usize`) and then does `min(n, len)` (or the
    /// equivalent `n as usize >= len` short-circuit in `take_end`/`drop_end`).
    /// A negative i64 has its sign bit set, so as `u64`/`usize` it is enormous
    /// and saturates to `hi` — NOT to 0. (`take(-1)` returns the WHOLE list,
    /// `drop(-1)` returns `[]`; `chunk(-1)` groups into one chunk.) The earlier
    /// `max(count, 0)` lo-clamp was wrong: it mapped `-1` to 0 (empty / div-by-0)
    /// instead of to `hi`.
    ///
    /// So the clamp is a single UNSIGNED minimum, `min_u(count, hi)`, computed on
    /// the full i64 (mirrors the C-034 `with_capacity` rule and is the same
    /// operation as `emit_clamp_index_to_len_i32`: a count and an unsigned index
    /// saturate identically). After it the value is in `[0, hi]` and
    /// `i32_wrap_i64` is lossless. See C-054.
    ///
    /// Stack: `[count_i64]` → `[clamped_i32]`. The non-negative upper bound is
    /// chosen by `hi`: a runtime list length (`ClampHi::LenLocal`) or a
    /// compile-time element-count ceiling (`ClampHi::Const`, e.g. `repeat`'s
    /// byte-budget cap, or `chunk`/`windows`'s `i32::MAX` "huge" sentinel).
    pub(super) fn emit_clamp_count_to_i32(&mut self, hi: ClampHi) {
        let count = self.scratch.alloc_i64();
        wasm!(self.func, { local_set(count); });
        // min_u(count, hi): `[count, hi, count <_u hi]` selects `count` when it
        // fits and `hi` otherwise. The comparison is UNSIGNED so a negative i64
        // (huge as u64) saturates to `hi` — matching native's `n as usize`.
        // `hi` is non-negative; widening it via `i64_extend_i32_u` is lossless.
        wasm!(self.func, { local_get(count); });
        self.emit_push_clamp_hi_i64(&hi);
        wasm!(self.func, { local_get(count); });
        self.emit_push_clamp_hi_i64(&hi);
        wasm!(self.func, {
            i64_lt_u; select;
            // Now in [0, hi]: the wrap is lossless.
            i32_wrap_i64;
        });
        self.scratch.free_i64(count);
    }

    /// Narrow an i64 BYTE/CHAR INDEX to an i32 clamped to `[0, hi]` with SIGNED
    /// saturation — `(idx.max(0)).min(hi)` — **before** the `i32_wrap_i64`.
    ///
    /// This is the SIGNED twin of `emit_clamp_count_to_i32`. It exists because
    /// `string.slice` is the one op whose native oracle clamps SIGNED, not
    /// unsigned: `almide_rt_string_slice` does `(start.max(0) as usize).min(len)`
    /// (and the symmetric expression for `end`), so a NEGATIVE start maps to `0`
    /// (then `if s >= e {""}` only triggers when the END is also small) — NOT to
    /// `len`. The unsigned `min_u` used by counts and by `list.slice` (whose
    /// oracle is `start as usize`) would instead send a negative start to `len`,
    /// which is the wrong result for `string.slice`. A start `>= 2^32` is huge as
    /// i64 and `.min(hi)` clamps it to `hi`, so the truncation class is still
    /// closed. See C-054.
    ///
    /// Stack: `[idx_i64]` → `[clamped_i32]` (always in `[0, hi]`).
    pub(super) fn emit_clamp_count_signed_i32(&mut self, hi: ClampHi) {
        let idx = self.scratch.alloc_i64();
        wasm!(self.func, {
            local_set(idx);
            // lo-clamp: max(idx, 0). `[idx, 0, idx >= 0]` selects `idx` when
            // non-negative and `0` for a negative i64 (matches `idx.max(0)`).
            local_get(idx); i64_const(0);
              local_get(idx); i64_const(0); i64_ge_s; select;
            local_set(idx);
        });
        // hi-clamp: min(idx, hi). `idx` is now non-negative, so signed and
        // unsigned compare agree; `i64_le_s` is fine.
        wasm!(self.func, { local_get(idx); });
        self.emit_push_clamp_hi_i64(&hi);
        wasm!(self.func, { local_get(idx); });
        self.emit_push_clamp_hi_i64(&hi);
        wasm!(self.func, {
            i64_le_s; select;
            i32_wrap_i64;
        });
        self.scratch.free_i64(idx);
    }

    /// Push the clamp ceiling as an i64 (non-negative).
    fn emit_push_clamp_hi_i64(&mut self, hi: &ClampHi) {
        match *hi {
            ClampHi::LenLocal(idx) => { wasm!(self.func, { local_get(idx); i64_extend_i32_u; }); }
            ClampHi::Const(n) => { wasm!(self.func, { i64_const(n); }); }
        }
    }

    /// Clamp an i64 list INDEX (interpreted UNSIGNED, matching native's
    /// `i as usize`) to `[0, len]` before narrowing — `min_u(i, len)`. A
    /// negative i64 has its sign bit set, so as u64 it is huge and saturates
    /// to `len` (native `(neg as usize).min(len)` also gives `len`); a count
    /// past 2^32 no longer wraps to a small in-range index. Used by `insert`,
    /// whose out-of-range index appends at the end (C-054).
    ///
    /// Stack: `[idx_i64]` → `[clamped_i32]` (always in `[0, len]`).
    pub(super) fn emit_clamp_index_to_len_i32(&mut self, len_local: u32) {
        let idx = self.scratch.alloc_i64();
        wasm!(self.func, {
            local_set(idx);
            local_get(idx);
            local_get(len_local); i64_extend_i32_u;
              local_get(idx); local_get(len_local); i64_extend_i32_u; i64_lt_u; select;
            i32_wrap_i64;
        });
        self.scratch.free_i64(idx);
    }

    /// Narrow an i64 list INDEX (interpreted UNSIGNED) for a bounds-checked op
    /// (`set` / `get_or` / `remove_at` whose OOB case is a no-op / default).
    /// Writes the in-bounds predicate `(idx_u < len)` — computed on the FULL
    /// i64, so an index >= 2^32 is correctly rejected instead of wrapping to a
    /// small in-range index — into `in_bounds_local`, and leaves the narrowed
    /// idx SATURATED to `[0, len]` on the stack (so any address arithmetic is
    /// in-range even when out of bounds; the flag gates the actual access).
    /// See C-054.
    ///
    /// Stack: `[idx_i64]` → `[saturated_idx_i32]`; sets `in_bounds_local: i32`.
    pub(super) fn emit_checked_index_i32(&mut self, len_local: u32, in_bounds_local: u32) {
        let idx = self.scratch.alloc_i64();
        wasm!(self.func, {
            local_set(idx);
            // in_bounds = idx_u < len_u  (on the full i64, no truncation)
            local_get(idx); local_get(len_local); i64_extend_i32_u; i64_lt_u;
            local_set(in_bounds_local);
            // saturated idx = min_u(idx, len) so addressing never goes OOB
            local_get(idx);
            local_get(len_local); i64_extend_i32_u;
              local_get(idx); local_get(len_local); i64_extend_i32_u; i64_lt_u; select;
            i32_wrap_i64;
        });
        self.scratch.free_i64(idx);
    }

    /// Trap with the UNIFIED abort ("Error: index out of bounds\n" + exit 1)
    /// unless `0 <= idx < len`, checking the FULL i64 so a >= 2^32 index can
    /// never truncate into range (#554/C-072). `idx64_local` holds the i64
    /// index; `ptr_local` points at the list/bytes header whose len is at
    /// offset 0. Leaves the stack unchanged.
    pub(super) fn emit_index_bound_guard(&mut self, idx64_local: u32, ptr_local: u32, oob_msg: i32, div_trap: u32) {
        wasm!(self.func, {
            // if (idx as u64) >= (len as u64) { abort }  — a negative i64 wraps
            // to a huge u64 and is caught by the same compare.
            local_get(idx64_local);
            local_get(ptr_local); i32_load(0); i64_extend_i32_u;
            i64_ge_u;
            if_empty;
              i32_const(oob_msg);
              call(div_trap);
            end;
        });
    }

    /// Copy one element from [stack: dst_addr, src_addr] based on type.
    pub(super) fn emit_elem_copy(&mut self, ty: &Ty) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => { wasm!(self.func, { i64_load(0); i64_store(0); }); }
            Some(ValType::F64) => { wasm!(self.func, { f64_load(0); f64_store(0); }); }
            _ => { wasm!(self.func, { i32_load(0); i32_store(0); }); }
        }
    }

    /// Copy one element from [stack: dst_addr, src_addr], taking a NEW owning
    /// reference to it (the SHARE case). Use this when the destination structure
    /// will outlive a *still-live* source that retains its own reference to the
    /// element (e.g. a collection op copying out of a borrowed argument): without
    /// the extra reference, the source's scope-end `Dec` would deep-free the value
    /// the destination now holds.
    ///
    /// For heap elements (the i32 pointer branch) this is `emit_elem_copy` plus a
    /// stack-neutral `rc_inc` inserted between the load and the store — `rc_inc`
    /// returns its pointer unchanged, and its `heap_start` guard makes it a runtime
    /// no-op on data-section pointers and scalar i32 (Bool), so the i32 branch can
    /// increment unconditionally. Int/Float (i64/f64) are scalars and copy exactly
    /// as `emit_elem_copy` does.
    ///
    /// NOT for intra-structure MOVES (grow-and-abandon, where the source buffer is
    /// discarded): incrementing there would leak one reference per grow.
    pub(super) fn emit_elem_copy_owned(&mut self, ty: &Ty) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => { wasm!(self.func, { i64_load(0); i64_store(0); }); }
            Some(ValType::F64) => { wasm!(self.func, { f64_load(0); f64_store(0); }); }
            // The inc is gated on HEAP-ness, not on "rides in i32": Int32/
            // UInt32/Float32 are 4-byte SCALARS whose values can exceed
            // heap_start — an unconditional rc_inc would write +1 into a
            // fake header at an arbitrary address (live landmine under
            // frees). Bool's data-section guard was masking this for the
            // other 4-byte scalars only by luck of small values.
            _ if crate::pass_perceus::is_heap_type(ty) => {
                wasm!(self.func, { i32_load(0); call(self.emitter.rt.rc_inc); i32_store(0); });
            }
            _ => { wasm!(self.func, { i32_load(0); i32_store(0); }); }
        }
    }

    /// Store one element: [stack: dst_addr, value].
    pub(super) fn emit_elem_store(&mut self, ty: &Ty) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => { wasm!(self.func, { i64_store(0); }); }
            Some(ValType::F64) => { wasm!(self.func, { f64_store(0); }); }
            _ => { wasm!(self.func, { i32_store(0); }); }
        }
    }

    /// Register a `call_indirect` type and emit the instruction.
    ///
    /// `param_types` includes env (I32) as the first element.
    /// `ret_types` is the WASM return type list (empty for void, single element otherwise).
    ///
    /// This is the canonical helper for all closure `call_indirect` patterns.
    /// Higher-level wrappers like `emit_closure_call` delegate here.
    pub(super) fn emit_call_indirect(&mut self, param_types: Vec<ValType>, ret_types: Vec<ValType>) {
        let ti = self.emitter.register_type(param_types, ret_types);
        wasm!(self.func, { call_indirect(ti, 0); });
    }

    /// Emit `call_indirect` for a simple closure call: `(env [, param]) → ret`.
    ///
    /// Builds param types as `[I32]` + optional `ty_to_valtype(param_ty)`.
    /// Return type is derived from `ret_ty` via `values::ret_type`, except
    /// `Ty::Unknown` and `Ty::Bool` are forced to `vec![I32]`.
    pub(super) fn emit_closure_call(&mut self, param_ty: &Ty, ret_ty: &Ty) {
        let mut ct = vec![ValType::I32]; // env
        if let Some(vt) = values::ty_to_valtype(param_ty) {
            ct.push(vt);
        }
        let rt = if ret_ty == &Ty::Unknown || ret_ty == &Ty::Bool {
            // Unknown: return i32 (ptr). Bool: i32.
            vec![ValType::I32]
        } else {
            values::ret_type(ret_ty)
        };
        self.emit_call_indirect(ct, rt);
    }

    /// Try to resolve a direct function call index from a closure expression.
    /// Returns Some(func_idx) if the closure is a no-capture ClosureCreate.
    pub(super) fn try_resolve_direct_call(&self, fn_arg: &IrExpr) -> Option<u32> {
        if let almide_ir::IrExprKind::ClosureCreate { func_name, captures } = &fn_arg.kind {
            if captures.is_empty() {
                return self.emitter.func_map.get(func_name.as_str()).copied();
            }
        }
        None
    }
}

include!("calls_list_helpers_p2.rs");
include!("calls_list_helpers_p3.rs");
