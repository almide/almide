impl FuncCompiler<'_> {
    // ── Compact-ordered-dict hash helpers ──

    /// Split hash on stack into h1 (bucket index) → idx_local and h2 (tag) → h2_local.
    pub(super) fn emit_h1_h2(&mut self, cap: u32, idx_local: u32, h2_local: u32) {
        use super::engine::layout::map as lm;
        let ht = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_tee(ht);
            local_get(cap); i32_const(1); i32_sub; i32_and;
            local_set(idx_local);
            local_get(ht);
            i32_const(lm::H2_SHIFT as i32); i32_shr_u; i32_const(lm::H2_MASK as i32); i32_and;
            local_tee(h2_local);
            i32_eqz;
            if_empty; i32_const(1); local_set(h2_local); end; // avoid 0 (empty)
        });
        self.scratch.free_i32(ht);
    }

    pub(super) fn emit_hash_key(&mut self, key_ty: &Ty) {
        use super::engine::layout::{STRING, string as ls};
        let str_data_off = self.emitter.layout_reg.fixed_offset(STRING, ls::DATA) as i32;
        match key_ty {
            // #600: wide ints share the i64 key hash (the value is already i64
            // on the stack); the get_or/from_list paths call emit_hash_key with
            // the raw key_ty, so Int64/UInt64 must hash here exactly like Int.
            Ty::Int | Ty::Int64 | Ty::UInt64 => {
                let tmp = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_tee(tmp);
                    i64_const(32); i64_shr_u;
                    local_get(tmp); i64_xor;
                    i64_const(0x9E3779B97F4A7C15u64 as i64); i64_mul;
                    i64_const(32); i64_shr_u;
                    i32_wrap_i64;
                });
                self.scratch.free_i64(tmp);
            }
            Ty::String => {
                let s = self.scratch.alloc_i32();
                let h = self.scratch.alloc_i32();
                let slen = self.scratch.alloc_i32();
                let si = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(s);
                    i32_const(0x811C9DC5u32 as i32); local_set(h);
                    local_get(s); i32_load(0); local_set(slen);
                    i32_const(0); local_set(si);
                    block_empty; loop_empty;
                      local_get(si); local_get(slen); i32_ge_u; br_if(1);
                      local_get(h);
                      local_get(s); i32_const(str_data_off); i32_add;
                      local_get(si); i32_add; i32_load8_u(0);
                      i32_xor;
                      i32_const(0x01000193u32 as i32); i32_mul;
                      local_set(h);
                      local_get(si); i32_const(1); i32_add; local_set(si);
                      br(0);
                    end; end;
                    local_get(h);
                });
                self.scratch.free_i32(si);
                self.scratch.free_i32(slen);
                self.scratch.free_i32(h);
                self.scratch.free_i32(s);
            }
            Ty::Bool => {} // identity hash: 0 or 1, already i32
            // Tuple keys, and records/Named-records whose fields include a heap
            // POINTER (e.g. a String or nested list field), must hash by VALUE so
            // two structurally-equal keys built from distinct allocations land in
            // the same bucket — otherwise probing never finds the match even with
            // a correct value-equality. Walk the fields and fold each field's
            // value-hash recursively (`emit_hash_value`). Records with only inline
            // value fields keep the cheaper byte-FNV path below.
            Ty::Tuple(_) => {
                let fields = self.key_struct_fields(key_ty).unwrap_or_default();
                self.emit_hash_fields(&fields);
            }
            Ty::Named(_, _) | Ty::Record { .. } | Ty::Variant { .. } => {
                let struct_fields = self.key_struct_fields(key_ty);
                let has_ptr = struct_fields.as_ref()
                    .map(|fs| fs.iter().any(|(_, t)| !Self::ty_is_inline_value(t)))
                    .unwrap_or(false);
                if let (true, Some(fields)) = (has_ptr, struct_fields) {
                    // Record/Named-record with pointer field(s): value-structural hash.
                    self.emit_hash_fields(&fields);
                } else {
                    // Records and variants are heap structs; FNV-1a over their content
                    // bytes (dereferencing the pointer). For variants this is the tag —
                    // hashing the POINTER would be identity-on-allocation and break value
                    // equality, since each constructor call allocates a fresh struct.
                    let size = self.key_content_size(key_ty);
                    if size == 0 {
                        // No known content layout: identity hash (the key value itself).
                    } else {
                        let ptr = self.scratch.alloc_i32();
                        let h = self.scratch.alloc_i32();
                        wasm!(self.func, {
                            local_set(ptr);
                            i32_const(0x811C9DC5u32 as i32); local_set(h);
                        });
                        for b in 0..size {
                            wasm!(self.func, {
                                local_get(h);
                                local_get(ptr); i32_load8_u(b);
                                i32_xor;
                                i32_const(0x01000193u32 as i32); i32_mul;
                                local_set(h);
                            });
                        }
                        wasm!(self.func, { local_get(h); });
                        self.scratch.free_i32(h);
                        self.scratch.free_i32(ptr);
                    }
                }
            }
            _ => {} // other pointers: identity hash
        }
    }

    /// True if a value of `ty` is stored INLINE (no heap pointer). Strings, lists,
    /// records, tuples, etc. are pointers; ints/floats/bools/narrow-ints are inline.
    /// Used to decide whether a compound key needs value-structural hashing/eq.
    fn ty_is_inline_value(ty: &Ty) -> bool {
        matches!(ty,
            Ty::Int | Ty::Float | Ty::Bool | Ty::Unit
            | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
            | Ty::Float32 | Ty::Float64)
    }

    /// Flat (name, ty) fields of a struct-like key (Tuple or Record/Named-record),
    /// laid out sequentially in memory. None for variants and non-struct keys.
    /// Single source of truth so hashing and equality see identical layout.
    fn key_struct_fields(&self, key_ty: &Ty) -> Option<Vec<(String, Ty)>> {
        match key_ty {
            Ty::Tuple(elems) => Some(
                elems.iter().enumerate()
                    .map(|(i, t)| (format!("_{i}"), t.clone()))
                    .collect()),
            Ty::Record { fields } => Some(
                fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect()),
            Ty::Named(name, _) if !self.emitter.variant_info.contains_key(name.as_str()) => {
                self.emitter.record_fields.get(name.as_str()).cloned()
            }
            _ => None,
        }
    }

    /// Value-structural FNV-1a hash of a struct-like key. Consumes the struct
    /// pointer on the stack, leaves an i32 hash. Folds each field's value-hash
    /// (`emit_hash_value`) at its sequential offset so structurally-equal keys —
    /// even with pointer fields like Strings — hash identically.
    fn emit_hash_fields(&mut self, fields: &[(String, Ty)]) {
        let ptr = self.scratch.alloc_i32();
        let h = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(ptr);
            i32_const(0x811C9DC5u32 as i32); local_set(h);
        });
        let mut offset = 0u32;
        for (_, fty) in fields {
            // h = (h ^ field_hash) * FNV_prime
            wasm!(self.func, {
                local_get(ptr);
            });
            self.emit_load_at(fty, offset);
            self.emit_hash_value(fty);
            wasm!(self.func, {
                local_get(h); i32_xor;
                i32_const(0x01000193u32 as i32); i32_mul;
                local_set(h);
            });
            offset += values::byte_size(fty);
        }
        wasm!(self.func, { local_get(h); });
        self.scratch.free_i32(h);
        self.scratch.free_i32(ptr);
    }

    /// Value-structural hash of a single value of `ty` on the stack → i32.
    /// Mirrors `emit_eq_typed`'s notion of equality so hash and eq never disagree:
    /// ints by value, strings by content bytes, tuples/records by their fields.
    fn emit_hash_value(&mut self, ty: &Ty) {
        match ty {
            // Int: reuse the i64 mix from emit_hash_key.
            Ty::Int | Ty::Int64 | Ty::UInt64 => self.emit_hash_key(&Ty::Int),
            // Narrow ints ride in i32 already — fold their value directly.
            Ty::Int8 | Ty::Int16 | Ty::Int32
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::Bool => { /* value already i32 */ }
            // Float bits → i32 mix (reinterpret then fold high/low halves).
            Ty::Float | Ty::Float64 => {
                let tmp = self.scratch.alloc_i64();
                wasm!(self.func, {
                    i64_reinterpret_f64; local_tee(tmp);
                    i64_const(32); i64_shr_u; local_get(tmp); i64_xor;
                    i32_wrap_i64;
                });
                self.scratch.free_i64(tmp);
            }
            Ty::Float32 => { wasm!(self.func, { i32_reinterpret_f32; }); }
            Ty::String | Ty::Bytes => self.emit_hash_key(&Ty::String),
            Ty::Tuple(_) | Ty::Record { .. } | Ty::Named(_, _) | Ty::Variant { .. } => {
                self.emit_hash_key(ty);
            }
            // Other heap types (List, Option, …): identity hash of the pointer.
            // Rare as nested key fields; equality still recurses structurally, so a
            // weaker hash only costs probe collisions, never correctness.
            _ => {}
        }
    }

    // ── Map type helpers ──

    pub(super) fn map_kv_sizes_from(&self, k: &Ty, v: &Ty) -> (u32, u32) {
        (values::byte_size(k), values::byte_size(v))
    }

    pub(super) fn map_kv_sizes(&self, ty: &Ty) -> (u32, u32) {
        if let Ty::Applied(_, args) = ty {
            (args.first().map(|t| values::byte_size(t)).unwrap_or(4),
             args.get(1).map(|t| values::byte_size(t)).unwrap_or(4))
        } else { (4, 4) }
    }
    pub(super) fn map_val_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty { args.get(1).cloned().unwrap_or(Ty::Int) } else { Ty::Int }
    }
    pub(super) fn map_key_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty { args.first().cloned().unwrap_or(Ty::String) } else { Ty::String }
    }
    pub(super) fn emit_key_load(&mut self, key_ty: &Ty, offset: u32) {
        // #600: wide ints (Int64/UInt64) share the 8-byte i64 key slot with Int —
        // emit_hash_key already hashes them via the i64 path, so the access width
        // must match or the wasm validator trips (expected i32, found i64).
        match key_ty {
            Ty::Int | Ty::Int64 | Ty::UInt64 => { wasm!(self.func, { i64_load(offset); }); }
            _ => { wasm!(self.func, { i32_load(offset); }); }
        }
    }
    pub(super) fn emit_key_store(&mut self, key_ty: &Ty, offset: u32) {
        match key_ty {
            Ty::Int | Ty::Int64 | Ty::UInt64 => { wasm!(self.func, { i64_store(offset); }); }
            _ => { wasm!(self.func, { i32_store(offset); }); }
        }
    }
    pub(super) fn emit_key_eq(&mut self, key_ty: &Ty) {
        match key_ty {
            Ty::Int | Ty::Int64 | Ty::UInt64 => { wasm!(self.func, { i64_eq; }); }
            Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
            // Tuple keys, and records/Named-records with a heap POINTER field
            // (String / nested list), need STRUCTURAL equality — `mem_eq` would
            // compare the field's pointer bytes, not its contents, so two equal-
            // content keys built from distinct allocations would miss. Route through
            // the shared type-directed deep equality (matching `emit_hash_key`, which
            // hashes these by value too, so hash and eq stay consistent).
            Ty::Tuple(_) => { self.emit_eq_typed(key_ty); }
            Ty::Named(_, _) | Ty::Record { .. } | Ty::Variant { .. } => {
                let struct_fields = self.key_struct_fields(key_ty);
                let has_ptr = struct_fields.as_ref()
                    .map(|fs| fs.iter().any(|(_, t)| !Self::ty_is_inline_value(t)))
                    .unwrap_or(false);
                if has_ptr {
                    self.emit_eq_typed(key_ty);
                } else {
                    // Compare the dereferenced content (matching emit_hash_key's coverage so
                    // hash and equality stay consistent): full record bytes, or a variant's
                    // tag. byte_size(record/variant) is only the pointer size (4), so the old
                    // mem_eq(4) compared just the first 4 content bytes. When the layout is
                    // unknown, fall back to pointer identity (matching the identity hash).
                    let size = self.key_content_size(key_ty);
                    if size == 0 {
                        wasm!(self.func, { i32_eq; });
                    } else {
                        wasm!(self.func, { i32_const(size as i32); call(self.emitter.rt.mem_eq); });
                    }
                }
            }
            _ => { wasm!(self.func, { i32_eq; }); }
        }
    }
    /// Resolve a record/Named key's fields (name, type), or empty if the layout is
    /// unknown. Single source of truth for hashing AND equality so they can't drift.
    fn record_key_fields(&self, key_ty: &Ty) -> Vec<(String, Ty)> {
        if let Ty::Record { fields } = key_ty {
            fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect::<Vec<_>>()
        } else if let Ty::Named(name, _) = key_ty {
            self.emitter.fields_of(name.as_str())
        } else if let Some(fs) = self.emitter.record_fields.get("") {
            fs.clone()
        } else {
            vec![]
        }
    }
    /// Byte count of a heap key's content for hashing/equality (0 if unknown).
    /// Variants hash/compare their TAG only: each constructor allocates a fresh
    /// struct (so the pointer is not stable) and the payload padding to the
    /// variant's max size is uninitialized (so comparing it is non-deterministic);
    /// the tag uniquely identifies a nullary case. Records use their full field size.
    fn key_content_size(&self, key_ty: &Ty) -> u32 {
        match key_ty {
            Ty::Variant { .. } => 4,
            Ty::Named(name, _) if self.emitter.variant_info.contains_key(name.as_str()) => 4,
            _ => self.record_key_fields(key_ty).iter().map(|(_, t)| super::values::byte_size(t)).sum(),
        }
    }
    pub(super) fn emit_search_key_store(&mut self, key_ty: &Ty, s32: u32, s64: u32) {
        match key_ty { Ty::Int | Ty::Int64 | Ty::UInt64 => { wasm!(self.func, { local_set(s64); }); } _ => { wasm!(self.func, { local_set(s32); }); } }
    }
    pub(super) fn emit_search_key_load(&mut self, key_ty: &Ty, s32: u32, s64: u32) {
        match key_ty { Ty::Int | Ty::Int64 | Ty::UInt64 => { wasm!(self.func, { local_get(s64); }); } _ => { wasm!(self.func, { local_get(s32); }); } }
    }
    pub(super) fn key_valtype(key_ty: &Ty) -> ValType {
        match key_ty { Ty::Int | Ty::Int64 | Ty::UInt64 => ValType::I64, _ => ValType::I32 }
    }
    pub(super) fn emit_elem_copy_sized(&mut self, size: u32, dup: bool) {
        // `dup` = the 4-byte slot is a HEAP POINTER being SHARED (source
        // survives) — the copy must own its own reference. Callers decide
        // via is_heap_type on the actual K/V Ty; size alone cannot (Int32/
        // Bool are 4-byte scalars).
        match size {
            8 => { wasm!(self.func, { i64_load(0); i64_store(0); }); }
            4 if dup => { wasm!(self.func, { i32_load(0); call(self.emitter.rt.rc_inc); i32_store(0); }); }
            4 => { wasm!(self.func, { i32_load(0); i32_store(0); }); }
            _ => { wasm!(self.func, { i32_load(0); i32_store(0); }); }
        }
    }
}
