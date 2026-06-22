impl FuncCompiler<'_> {
    pub(super) fn emit_cmp_instruction(&mut self, ty: &Ty, kind: CmpKind) {
        match (ty, kind) {
            (Ty::Int | Ty::Int64, CmpKind::Lt) => { wasm!(self.func, { i64_lt_s; }); }
            (Ty::Int | Ty::Int64, CmpKind::Gt) => { wasm!(self.func, { i64_gt_s; }); }
            (Ty::Int | Ty::Int64, CmpKind::Lte) => { wasm!(self.func, { i64_le_s; }); }
            (Ty::Int | Ty::Int64, CmpKind::Gte) => { wasm!(self.func, { i64_ge_s; }); }
            (Ty::UInt64, cmp_kind) => {
                use wasm_encoder::Instruction;
                self.func.instruction(&match cmp_kind {
                    CmpKind::Lt => Instruction::I64LtU,
                    CmpKind::Gt => Instruction::I64GtU,
                    CmpKind::Lte => Instruction::I64LeU,
                    CmpKind::Gte => Instruction::I64GeU,
                });
            }
            // Narrow sized ints ride in WASM i32. Sign is preserved by
            // the upstream `i32_load<N>_s/u`; at compare time, treat
            // signed variants as signed and unsigned as unsigned.
            // Bool rides in WASM i32 as 0/1; `false < true` (unsigned).
            (Ty::Bool, CmpKind::Lt) => { wasm!(self.func, { i32_lt_u; }); }
            (Ty::Bool, CmpKind::Gt) => { wasm!(self.func, { i32_gt_u; }); }
            (Ty::Bool, CmpKind::Lte) => { wasm!(self.func, { i32_le_u; }); }
            (Ty::Bool, CmpKind::Gte) => { wasm!(self.func, { i32_ge_u; }); }
            (Ty::Int8 | Ty::Int16 | Ty::Int32, CmpKind::Lt) => { wasm!(self.func, { i32_lt_s; }); }
            (Ty::Int8 | Ty::Int16 | Ty::Int32, CmpKind::Gt) => { wasm!(self.func, { i32_gt_s; }); }
            (Ty::Int8 | Ty::Int16 | Ty::Int32, CmpKind::Lte) => { wasm!(self.func, { i32_le_s; }); }
            (Ty::Int8 | Ty::Int16 | Ty::Int32, CmpKind::Gte) => { wasm!(self.func, { i32_ge_s; }); }
            (Ty::UInt8 | Ty::UInt16 | Ty::UInt32, CmpKind::Lt) => { wasm!(self.func, { i32_lt_u; }); }
            (Ty::UInt8 | Ty::UInt16 | Ty::UInt32, CmpKind::Gt) => { wasm!(self.func, { i32_gt_u; }); }
            (Ty::UInt8 | Ty::UInt16 | Ty::UInt32, CmpKind::Lte) => { wasm!(self.func, { i32_le_u; }); }
            (Ty::UInt8 | Ty::UInt16 | Ty::UInt32, CmpKind::Gte) => { wasm!(self.func, { i32_ge_u; }); }
            (Ty::Float | Ty::Float64, CmpKind::Lt) => { wasm!(self.func, { f64_lt; }); }
            (Ty::Float | Ty::Float64, CmpKind::Gt) => { wasm!(self.func, { f64_gt; }); }
            (Ty::Float | Ty::Float64, CmpKind::Lte) => { wasm!(self.func, { f64_le; }); }
            (Ty::Float | Ty::Float64, CmpKind::Gte) => { wasm!(self.func, { f64_ge; }); }
            (Ty::Float32, cmp_kind) => {
                use wasm_encoder::Instruction;
                self.func.instruction(&match cmp_kind {
                    CmpKind::Lt => Instruction::F32Lt,
                    CmpKind::Gt => Instruction::F32Gt,
                    CmpKind::Lte => Instruction::F32Le,
                    CmpKind::Gte => Instruction::F32Ge,
                });
            }
            (Ty::String, CmpKind::Lt) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(0); i32_lt_s; });
            }
            (Ty::String, CmpKind::Gt) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(0); i32_gt_s; });
            }
            (Ty::String, CmpKind::Lte) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(0); i32_le_s; });
            }
            (Ty::String, CmpKind::Gte) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(0); i32_ge_s; });
            }
            // Variant (enum-like) comparison: each value is a pointer to a heap
            // block whose first i32 is the discriminant tag. For `Ord`-derived
            // variants, `Low < Medium` means `Low.tag < Medium.tag`, so we load
            // both tags and compare them as unsigned i32s.
            //
            // Stack on entry: [left_ptr, right_ptr]. We must load tags from both
            // pointers and compare, preserving WASM's strict stack discipline.
            // Use scratch locals to hold the pointers since WASM has no swap op.
            (Ty::Named(name, _), cmp_kind) if self.emitter.variant_info.contains_key(name.as_str()) => {
                let right = self.scratch.alloc_i32();
                let left = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(right);
                    local_set(left);
                    local_get(left); i32_load(0);
                    local_get(right); i32_load(0);
                });
                match cmp_kind {
                    CmpKind::Lt => { wasm!(self.func, { i32_lt_u; }); }
                    CmpKind::Gt => { wasm!(self.func, { i32_gt_u; }); }
                    CmpKind::Lte => { wasm!(self.func, { i32_le_u; }); }
                    CmpKind::Gte => { wasm!(self.func, { i32_ge_u; }); }
                }
                self.scratch.free_i32(left);
                self.scratch.free_i32(right);
            }
            // TypeVar/Unknown (#517): the AllTypesConcrete hard gate refuses
            // any build whose expression types are unresolved, and comparison
            // operands ARE expressions — nothing live can reach this arm. The
            // former runtime trap could bury a future gate hole as a silent
            // late crash; fail the build instead.
            (ty, _) if ty.is_unresolved() => {
                panic!(
                    "[ICE] emit_wasm: comparison on unresolved type `{:?}` reached emission — \
                     the AllTypesConcrete gate should have refused this build (#517)",
                    ty
                );
            }
            (l, _) => {
                // A RESOLVED type with no comparison emission is a
                // compiler bug — fail the build (§5: no silent traps).
                panic!(
                    "[ICE] emit_wasm: no equality/comparison emission for type `{:?}` — \
                     add an arm or reject upstream in the checker",
                    l
                );
            }
        }
    }

    /// Emit a type-directed *total order* comparison. Consumes `[a, b]` from the
    /// stack and leaves an i32 three-way result: negative if `a < b`, zero if
    /// `a == b`, positive if `a > b` — matching the native oracle, Rust's
    /// derived `Ord` (`xs.iter().min()` / `.sort()`).
    ///
    /// This is the ordering twin of [`emit_eq_typed`]: every WASM stdlib that
    /// needs a total order (`list.min`/`list.max`, and by routing `list.sort`)
    /// goes through this one emitter, so an unsupported element type is an
    /// emit-time ICE — never a silent wrong comparison. Recursive for the
    /// compound types (lexicographic for Tuple/List, `None < Some` for Option,
    /// `Ok < Err` for Result, tag-then-field for variants).
    ///
    /// For pointer-shaped leaves (`String`) the inputs are i32 pointers; for
    /// value leaves (`Int`, `Float`, `Bool`, sized ints) they are the scalar
    /// values themselves. Compound inputs are i32 heap pointers.
    pub(super) fn emit_ord_cmp3(&mut self, ty: &Ty) {
        use almide_lang::types::constructor::TypeConstructorId;
        match ty {
            // String pointers: the runtime cmp already returns sign(-/0/+).
            Ty::String | Ty::Bytes => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); });
            }
            // Scalar leaves: derive sign as `(a > b) - (a < b)`. The two
            // comparisons each consume the operand pair, so stash them in
            // typed scratch first and reload for each test.
            Ty::Int | Ty::Int64 | Ty::UInt64
            | Ty::Int8 | Ty::Int16 | Ty::Int32
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32
            | Ty::Bool => {
                self.emit_scalar_ord_sign(ty);
            }
            // Float total order (C-055): `f64` is not totally ordered by the
            // native `<`/`>` (NaN compares false on every axis, `-0.0 == +0.0`),
            // so list min/max/sort/sort_by-float-key route through IEEE-754
            // totalOrder — the ordering twin of native `f64::total_cmp`. Map
            // each f64 to a monotone i64 KEY (sign-magnitude bit flip) and take
            // the signed `(a > b) - (a < b)` sign on the keys; NaN lands at the
            // top, `-NaN` at the bottom, `-0.0 < +0.0`.
            Ty::Float | Ty::Float64 | Ty::Float32 => {
                self.emit_float_total_order_sign();
            }
            // Variant comparison reduces to tag order, then field order. The
            // existing variant `emit_cmp_instruction(Lt/Gt)` only compares
            // tags; for `Ord`-derived enums with payloads we must also tie-break
            // on the fields. We special-case the two builtin variant shapes
            // (Option/Result) below and fall back to tag-only for user enums,
            // which is exactly what native derives when every case is unit and
            // is the conservative choice otherwise.
            Ty::Applied(TypeConstructorId::Option, args) => {
                let inner = args.first().cloned().unwrap_or(Ty::Int);
                self.emit_option_ord_cmp3(&inner);
            }
            Ty::Applied(TypeConstructorId::Result, args) => {
                let ok = args.first().cloned().unwrap_or(Ty::Int);
                let err = args.get(1).cloned().unwrap_or(Ty::String);
                self.emit_result_ord_cmp3(&ok, &err);
            }
            Ty::Applied(TypeConstructorId::List, args) => {
                let elem = args.first().cloned().unwrap_or(Ty::Int);
                self.emit_list_ord_cmp3(&elem);
            }
            Ty::Tuple(elems) => {
                let elems = elems.clone();
                self.emit_tuple_ord_cmp3(&elems);
            }
            // User variants: order by discriminant tag (unsigned). Matches the
            // native derive for all-unit enums; payload tie-break is not yet
            // modelled, so we ICE if a payload-carrying user variant reaches an
            // ordering site (it cannot today — min/max/sort restrict to the
            // shapes above) rather than silently mis-order.
            Ty::Named(name, _) if self.emitter.variant_info.contains_key(name.as_str()) => {
                let cases = self.emitter.variant_info.get(name.as_str()).cloned().unwrap_or_default();
                let has_payload = cases.iter().any(|c| !c.fields.is_empty());
                if has_payload {
                    panic!(
                        "[ICE] emit_wasm: total-order comparison of payload-carrying \
                         variant `{}` is not modelled — extend emit_ord_cmp3",
                        name
                    );
                }
                // [a_ptr, b_ptr] → sign(a.tag - b.tag), tags are i32 at offset 0.
                let b = self.scratch.alloc_i32();
                let a = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(b); local_set(a);
                    local_get(a); i32_load(0);
                    local_get(b); i32_load(0); i32_gt_u;
                    local_get(a); i32_load(0);
                    local_get(b); i32_load(0); i32_lt_u;
                    i32_sub;
                });
                self.scratch.free_i32(a);
                self.scratch.free_i32(b);
            }
            _ => panic!(
                "[ICE] emit_wasm: no total-order comparison for element type \
                 `{:?}` — extend emit_ord_cmp3",
                ty
            ),
        }
    }

    /// Scalar `sign(a - b)` for a value-typed leaf already on the stack as
    /// `[a, b]`. Spills both operands to a typed scratch slot so the two
    /// directional comparisons can reload them (WASM stack has no `dup`).
    fn emit_scalar_ord_sign(&mut self, ty: &Ty) {
        let vt = values::ty_to_valtype(ty).unwrap_or(ValType::I32);
        let b = self.scratch.alloc(vt);
        let a = self.scratch.alloc(vt);
        wasm!(self.func, { local_set(b); local_set(a); });
        // (a > b)
        wasm!(self.func, { local_get(a); local_get(b); });
        self.emit_cmp_instruction(ty, CmpKind::Gt);
        // (a < b)
        wasm!(self.func, { local_get(a); local_get(b); });
        self.emit_cmp_instruction(ty, CmpKind::Lt);
        // sign = (a>b) - (a<b)  ∈ {-1, 0, 1}
        wasm!(self.func, { i32_sub; });
        self.scratch.free(a, vt);
        self.scratch.free(b, vt);
    }

    /// IEEE-754 totalOrder sign for two f64s on the stack as `[a, b]`. Maps
    /// each f64 to a monotone i64 KEY, then takes the signed key sign
    /// `(ka > kb) - (ka < kb)` ∈ {-1, 0, 1}. The KEY transform is the exact
    /// twin of `f64::total_cmp` (so native == wasm byte-for-byte):
    ///
    /// ```text
    /// key = bits ^ ((bits >>_s 63) >>_u 1)
    /// ```
    ///
    /// A non-negative bit pattern is unchanged; a negative one has its lower
    /// 63 bits flipped (sign bit kept), which makes more-negative values sort
    /// below less-negative ones and places `-0.0` just below `+0.0`. NaN keeps
    /// its sign-extremal position. See C-055.
    fn emit_float_total_order_sign(&mut self) {
        let kb = self.scratch.alloc_i64();
        let ka = self.scratch.alloc_i64();
        // [a, b] → key(b) into kb, key(a) into ka.
        self.emit_f64_total_order_key();
        wasm!(self.func, { local_set(kb); });
        self.emit_f64_total_order_key();
        wasm!(self.func, { local_set(ka); });
        // sign = (ka > kb) - (ka < kb)
        wasm!(self.func, {
            local_get(ka); local_get(kb); i64_gt_s;
            local_get(ka); local_get(kb); i64_lt_s;
            i32_sub;
        });
        self.scratch.free_i64(ka);
        self.scratch.free_i64(kb);
    }

    /// `[f64]` → `[i64 total-order key]`. `key = bits ^ ((bits >>_s 63) >>_u 1)`,
    /// the same transform `f64::total_cmp` applies. (`>>_s` = arithmetic, `>>_u`
    /// = logical, per the Rust std source.) Also used by `sort_by` with a Float
    /// key, which pre-transforms each key so the parallel key array can ride the
    /// plain i64 storage/compare/swap path (C-055).
    pub(super) fn emit_f64_total_order_key(&mut self) {
        let bits = self.scratch.alloc_i64();
        wasm!(self.func, {
            i64_reinterpret_f64; local_set(bits);
            local_get(bits);
            // mask = (bits >>_s 63) >>_u 1
            local_get(bits); i64_const(63); i64_shr_s; i64_const(1); i64_shr_u;
            i64_xor;
        });
        self.scratch.free_i64(bits);
    }

    /// Option ordering: `None < Some(_)`; two `Some` recurse on the inner type.
    /// Inputs `[a_ptr, b_ptr]` are nullable pointers (`0` == none).
    fn emit_option_ord_cmp3(&mut self, inner_ty: &Ty) {
        let b = self.scratch.alloc_i32();
        let a = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(b); local_set(a);
            // a == none?
            local_get(a); i32_eqz;
            if_i32;
              // a none: none < some, none == none
              local_get(b); i32_eqz; if_i32; i32_const(0); else_; i32_const(-1); end;
            else_;
              // a some
              local_get(b); i32_eqz;
              if_i32; i32_const(1); // some > none
              else_;
                // both some: recurse on inner (loaded at offset 0)
                local_get(a);
        });
        self.emit_load_at(inner_ty, 0);
        wasm!(self.func, { local_get(b); });
        self.emit_load_at(inner_ty, 0);
        let inner = inner_ty.clone();
        self.emit_ord_cmp3(&inner);
        wasm!(self.func, {
              end;
            end;
        });
        self.scratch.free_i32(a);
        self.scratch.free_i32(b);
    }

    /// Result ordering: derived `Ord` puts `Ok` before `Err` (variant index 0
    /// vs 1). Inputs `[a_ptr, b_ptr]` are pointers to `[tag:i32][payload]`,
    /// tag 0 == ok, tag 4-offset payload.
    fn emit_result_ord_cmp3(&mut self, ok_ty: &Ty, err_ty: &Ty) {
        let b = self.scratch.alloc_i32();
        let a = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(b); local_set(a);
            // tags differ → order by tag (ok=0 < err=1)
            local_get(a); i32_load(0);
            local_get(b); i32_load(0);
            i32_ne;
            if_i32;
              local_get(a); i32_load(0);
              local_get(b); i32_load(0); i32_gt_u;
              local_get(a); i32_load(0);
              local_get(b); i32_load(0); i32_lt_u;
              i32_sub;
            else_;
              // same tag: recurse on the matching payload
              local_get(a); i32_load(0); i32_eqz;
              if_i32;
        });
        // ok payload at offset 4
        if matches!(ok_ty, Ty::Unit) {
            wasm!(self.func, { i32_const(0); });
        } else {
            wasm!(self.func, { local_get(a); });
            self.emit_load_at(ok_ty, 4);
            wasm!(self.func, { local_get(b); });
            self.emit_load_at(ok_ty, 4);
            let ok = ok_ty.clone();
            self.emit_ord_cmp3(&ok);
        }
        wasm!(self.func, { else_; });
        // err payload at offset 4
        if matches!(err_ty, Ty::Unit) {
            wasm!(self.func, { i32_const(0); });
        } else {
            wasm!(self.func, { local_get(a); });
            self.emit_load_at(err_ty, 4);
            wasm!(self.func, { local_get(b); });
            self.emit_load_at(err_ty, 4);
            let err = err_ty.clone();
            self.emit_ord_cmp3(&err);
        }
        wasm!(self.func, {
              end;
            end;
        });
        self.scratch.free_i32(a);
        self.scratch.free_i32(b);
    }

    /// Tuple ordering: lexicographic over fields, left to right. Inputs
    /// `[a_ptr, b_ptr]` are pointers to the packed field layout.
    fn emit_tuple_ord_cmp3(&mut self, elems: &[Ty]) {
        let res = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let a = self.scratch.alloc_i32();
        wasm!(self.func, { local_set(b); local_set(a); i32_const(0); local_set(res); });
        let mut offset = 0u32;
        // A single block we break out of on the first non-equal field.
        wasm!(self.func, { block_empty; });
        for (i, ety) in elems.iter().enumerate() {
            wasm!(self.func, { local_get(a); });
            self.emit_load_at(ety, offset);
            wasm!(self.func, { local_get(b); });
            self.emit_load_at(ety, offset);
            let ety_c = ety.clone();
            self.emit_ord_cmp3(&ety_c);
            wasm!(self.func, { local_set(res); });
            // if res != 0, break (last field need not test — falls through).
            if i + 1 < elems.len() {
                wasm!(self.func, { local_get(res); br_if(0); });
            }
            offset += values::byte_size(ety);
        }
        wasm!(self.func, { end; local_get(res); });
        self.scratch.free_i32(a);
        self.scratch.free_i32(b);
        self.scratch.free_i32(res);
    }

    /// List ordering: lexicographic over elements, then shorter list first on a
    /// common prefix (matching `Vec`'s derived `Ord`). Inputs `[a_ptr, b_ptr]`
    /// point to `[len:i32][cap:i32][data...]`.
    fn emit_list_ord_cmp3(&mut self, elem_ty: &Ty) {
        use super::engine::layout::{LIST, list as ll};
        let data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let es = values::byte_size(elem_ty) as i32;
        let res = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let alen = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let minlen = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let a = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(b); local_set(a);
            i32_const(0); local_set(res);
            local_get(a); i32_load(0); local_set(alen);
            local_get(b); i32_load(0); local_set(blen);
            local_get(alen); local_get(blen); i32_lt_u;
            if_i32; local_get(alen); else_; local_get(blen); end;
            local_set(minlen);
            i32_const(0); local_set(i);
            block_empty; loop_empty;                                  // [A] exit, [B] loop
              local_get(i); local_get(minlen); i32_ge_u; br_if(1);    // i >= minlen → break
              local_get(a); i32_const(data_off); i32_add; local_get(i); i32_const(es); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        wasm!(self.func, {
              local_get(b); i32_const(data_off); i32_add; local_get(i); i32_const(es); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        let elem_c = elem_ty.clone();
        self.emit_ord_cmp3(&elem_c);
        wasm!(self.func, {
              local_set(res);
              local_get(res); br_if(1);                               // non-equal element → break with res
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;                                                 // close B, A
            // Common prefix equal: order by length (sign(alen - blen)).
            local_get(res); i32_eqz;
            if_i32;
              local_get(alen); local_get(blen); i32_gt_u;
              local_get(alen); local_get(blen); i32_lt_u;
              i32_sub;
            else_;
              local_get(res);
            end;
        });
        self.scratch.free_i32(a);
        self.scratch.free_i32(b);
        self.scratch.free_i32(minlen);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(alen);
        self.scratch.free_i32(i);
        self.scratch.free_i32(res);
    }
}
