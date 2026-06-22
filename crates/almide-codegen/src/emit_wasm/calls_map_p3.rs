impl FuncCompiler<'_> {
    // ── Compact-ordered-dict addressing — every offset by name, zero magic numbers.
    // Layout (SWISS_MAP id, header_size=8):
    //   [len@0][cap@4][tags:u8[cap]@8][index:i32[cap]@8+cap][entries:(K,V)[cap]@8+cap+cap*INDEX_SLOT_SIZE]
    // tags = h2 fast-reject (0=empty); index 1-based (slot v → entries[v-1], 0=empty);
    // entries dense, insertion order [0..len], stride es=ks+vs, key@0 val@ks.

    /// Push the INDEX region base `map + header + cap` (slots start after the tags).
    pub(super) fn emit_dict_index_base(&mut self, map: u32, cap: u32) {
        let hdr = self.emitter.layout_reg.header_size(super::engine::layout::SWISS_MAP) as i32;
        wasm!(self.func, { local_get(map); i32_const(hdr); i32_add; local_get(cap); i32_add; });
    }

    /// Push the dense ENTRIES base `map + header + cap + cap*INDEX_SLOT_SIZE`.
    pub(super) fn emit_dict_entries_base(&mut self, map: u32, cap: u32) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let hdr = self.emitter.layout_reg.header_size(SWISS_MAP) as i32;
        wasm!(self.func, {
            local_get(map); i32_const(hdr); i32_add;
            local_get(cap); i32_add;
            local_get(cap); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
        });
    }

    /// Allocate + zero-init a COD table of `cap` slots (entry stride `es`) into `out`.
    /// total = header + cap*(tag(1) + INDEX_SLOT_SIZE + es); len=0; cap=cap; tags+index zeroed
    /// (the allocator reuses a free list, so it does NOT return zeroed memory).
    pub(super) fn emit_dict_alloc(&mut self, out: u32, cap: u32, es: u32) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let hdr = self.emitter.layout_reg.header_size(SWISS_MAP) as i32;
        let cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
        let per_slot = 1 + lm::INDEX_SLOT_SIZE as i32 + es as i32;
        let tag_plus_index = 1 + lm::INDEX_SLOT_SIZE as i32;
        wasm!(self.func, {
            i32_const(hdr); local_get(cap); i32_const(per_slot); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(out);
            local_get(out); i32_const(0); i32_store(0);             // len = 0
            local_get(out); local_get(cap); i32_store(cap_off, 0);  // cap
            // zero tags+index: memory_fill(out+header, 0, cap*(1+INDEX_SLOT_SIZE))
            local_get(out); i32_const(hdr); i32_add;
            i32_const(0);
            local_get(cap); i32_const(tag_plus_index); i32_mul;
            memory_fill(0);
        });
    }

    /// Grow `cap_out` (a local) to the smallest pow2 ≥ INITIAL_CAP that keeps
    /// `n` entries under the load factor: n*LOAD_DEN ≤ cap*LOAD_NUM.
    pub(super) fn emit_dict_fit_cap(&mut self, n: u32, cap_out: u32) {
        use super::engine::layout::map as lm;
        wasm!(self.func, {
            i32_const(lm::INITIAL_CAP as i32); local_set(cap_out);
            block_empty; loop_empty;
              local_get(n); i32_const(lm::LOAD_DEN as i32); i32_mul;
              local_get(cap_out); i32_const(lm::LOAD_NUM as i32); i32_mul;
              i32_le_u; br_if(1);
              local_get(cap_out); i32_const(1); i32_shl; local_set(cap_out);
              br(0);
            end; end;
        });
    }

    /// Rebuild the hash index (tags + index slots) of a COD table whose dense
    /// entries[0..len] are already populated and whose tags+index are zeroed.
    /// For each dense entry i: hash its key, probe for an empty slot, write
    /// tags[slot]=h2 and index[slot]=i+1 (1-based pointer back to the entry).
    pub(super) fn emit_dict_rebuild_index(&mut self, map: u32, cap: u32, es: u32, key_ty: &Ty) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let tags_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let eb = self.scratch.alloc_i32();
        let ib = self.scratch.alloc_i32();
        let idx = self.scratch.alloc_i32();
        let h2 = self.scratch.alloc_i32();
        wasm!(self.func, { local_get(map); i32_load(0); local_set(len); });
        self.emit_dict_index_base(map, cap);
        wasm!(self.func, { local_set(ib); });
        self.emit_dict_entries_base(map, cap);
        wasm!(self.func, {
            local_set(eb);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
              local_get(i); local_get(len); i32_ge_u; br_if(1);
              local_get(eb); local_get(i); i32_const(es as i32); i32_mul; i32_add;
        });
        self.emit_key_load(key_ty, 0);
        self.emit_hash_key(key_ty);
        self.emit_h1_h2(cap, idx, h2);
        wasm!(self.func, {
              block_empty; loop_empty;
                local_get(map); i32_const(tags_off); i32_add;
                local_get(idx); i32_add; i32_load8_u(0); i32_eqz; br_if(1);
                local_get(idx); i32_const(1); i32_add;
                local_get(cap); i32_const(1); i32_sub; i32_and; local_set(idx); br(0);
              end; end;
              local_get(map); i32_const(tags_off); i32_add; local_get(idx); i32_add;
              local_get(h2); i32_store8(0);
              local_get(ib); local_get(idx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
              local_get(i); i32_const(1); i32_add; i32_store(0);
              local_get(i); i32_const(1); i32_add; local_set(i); br(0);
            end; end;
        });
        self.scratch.free_i32(h2);
        self.scratch.free_i32(idx);
        self.scratch.free_i32(ib);
        self.scratch.free_i32(eb);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
    }

    /// Copy `src`'s dense entries (src.len of them) into the freshly-alloced `dst`
    /// table, set dst.len = src.len, and rebuild dst's index. `dst` must already
    /// be `emit_dict_alloc`-ed at `dst_cap` (entries stride `es`, tags+index zeroed).
    pub(super) fn emit_dict_recap(&mut self, src: u32, src_cap: u32, dst: u32, dst_cap: u32, es: u32, key_ty: &Ty, dup: Option<(&Ty, &Ty)>) {
        let len = self.scratch.alloc_i32();
        wasm!(self.func, { local_get(src); i32_load(0); local_set(len); });
        self.emit_dict_entries_base(dst, dst_cap); // memory_copy dest
        self.emit_dict_entries_base(src, src_cap); // memory_copy src
        wasm!(self.func, {
            local_get(len); i32_const(es as i32); i32_mul; memory_copy;
            local_get(dst); local_get(len); i32_store(0); // dst.len = src.len
        });
        // SHARE vs MOVE: `dup = Some((K, V))` when the SOURCE dict survives
        // (map.set / map.merge build a sibling) — every copied heap key/val
        // pointer needs its own reference. `None` for grow (the old table is
        // abandoned undecremented: a MOVE; dup would leak per grow).
        if let Some((k_ty, v_ty)) = dup {
            let kh = crate::pass_perceus::is_heap_type(k_ty);
            let vh = crate::pass_perceus::is_heap_type(v_ty);
            if kh || vh {
                let (ks, _) = self.map_kv_sizes_from(k_ty, v_ty);
                let di = self.scratch.alloc_i32();
                let de = self.scratch.alloc_i32();
                self.emit_dict_entries_base(dst, dst_cap);
                wasm!(self.func, { local_set(de); i32_const(0); local_set(di); });
                wasm!(self.func, {
                    block_empty; loop_empty;
                        local_get(di); local_get(len); i32_ge_u; br_if(1);
                });
                if kh {
                    wasm!(self.func, {
                        local_get(de); local_get(di); i32_const(es as i32); i32_mul; i32_add;
                        i32_load(0); call(self.emitter.rt.rc_inc); drop;
                    });
                }
                if vh {
                    wasm!(self.func, {
                        local_get(de); local_get(di); i32_const(es as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                        i32_load(0); call(self.emitter.rt.rc_inc); drop;
                    });
                }
                wasm!(self.func, {
                        local_get(di); i32_const(1); i32_add; local_set(di);
                        br(0);
                    end; end;
                });
                self.scratch.free_i32(de);
                self.scratch.free_i32(di);
            }
        }
        self.emit_dict_rebuild_index(dst, dst_cap, es, key_ty);
        self.scratch.free_i32(len);
    }

    /// Grow a COD table in place: quadruple `cap`, recap into the bigger table,
    /// and update the `map`/`cap` locals to point at the new table.
    pub(super) fn emit_dict_grow(&mut self, map: u32, cap: u32, es: u32, key_ty: &Ty) {
        use super::engine::layout::map as lm;
        let nc = self.scratch.alloc_i32();
        let nm = self.scratch.alloc_i32();
        wasm!(self.func, { local_get(cap); i32_const(lm::GROWTH_SHIFT as i32); i32_shl; local_set(nc); });
        self.emit_dict_alloc(nm, nc, es);
        self.emit_dict_recap(map, cap, nm, nc, es, key_ty, None);
        wasm!(self.func, { local_get(nm); local_set(map); local_get(nc); local_set(cap); });
        self.scratch.free_i32(nm);
        self.scratch.free_i32(nc);
    }

    /// The single COD insertion workhorse. `src` points to a contiguous `(key,val)`
    /// entry (key@0, val@ks, total `es` bytes). Probe `map`'s index for the key:
    /// existing → overwrite its value in place (dense position kept); new → append
    /// at entries[len], write tags[slot]=h2, index[slot]=len+1, bump len. Assumes
    /// the caller has reserved capacity (no grow). `ib`/`eb` are the index/entries bases.
    pub(super) fn emit_dict_put_entry(&mut self, map: u32, cap: u32, ib: u32, eb: u32, src: u32, es: u32, ks: u32, vs: u32, key_ty: &Ty, val_ty: &Ty) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let tags_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
        let idx = self.scratch.alloc_i32();
        let h2 = self.scratch.alloc_i32();
        let tg = self.scratch.alloc_i32();
        let ei = self.scratch.alloc_i32();
        wasm!(self.func, { local_get(src); });
        self.emit_key_load(key_ty, 0);
        self.emit_hash_key(key_ty);
        self.emit_h1_h2(cap, idx, h2);
        wasm!(self.func, {
            block_empty; loop_empty;
              local_get(map); i32_const(tags_off); i32_add; local_get(idx); i32_add; i32_load8_u(0); local_set(tg);
              local_get(tg); i32_eqz; br_if(1);              // empty slot → new key
              local_get(tg); local_get(h2); i32_eq;
              if_empty;
                local_get(ib); local_get(idx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
                i32_load(0); i32_const(1); i32_sub; local_set(ei);   // ei = index[idx]-1
                local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add;
        });
        self.emit_key_load(key_ty, 0);          // existing entry key
        wasm!(self.func, { local_get(src); });
        self.emit_key_load(key_ty, 0);          // src key
        self.emit_key_eq(key_ty);
        wasm!(self.func, {
                br_if(2);                        // equal → existing, ei set, exit probe
              end;
              local_get(idx); i32_const(1); i32_add;
              local_get(cap); i32_const(1); i32_sub; i32_and; local_set(idx); br(0);
            end; end;
            // New key (tg==0): append at dense entries[len].
            local_get(tg); i32_eqz;
            if_empty;
              local_get(map); i32_load(0); local_set(ei);          // ei = len
              local_get(map); i32_const(tags_off); i32_add; local_get(idx); i32_add;
              local_get(h2); i32_store8(0);                        // tags[idx] = h2
              local_get(ib); local_get(idx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
              local_get(ei); i32_const(1); i32_add; i32_store(0);  // index[idx] = ei+1
              local_get(map); local_get(ei); i32_const(1); i32_add; i32_store(0); // map.len = ei+1
              local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add;
              local_get(src); i32_const(ks as i32); memory_copy;   // copy key bytes
        });
        // SHARE: a NEW heap key was just copied (by reference) from the borrowed
        // source — dup it so the dict owns its own reference, else the source's
        // scope-end Dec deep-frees the key the dict now holds (double-free). Only on
        // the new-key path (an existing key keeps the reference it already owns).
        if Self::is_heap_type(key_ty) {
            wasm!(self.func, {
              local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add;
              i32_load(0); call(self.emitter.rt.rc_inc); drop;
            });
        }
        wasm!(self.func, {
            end;
            // Copy value bytes into entries[ei]+ks (both new and existing).
            local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add; i32_const(ks as i32); i32_add;
            local_get(src); i32_const(ks as i32); i32_add;
            i32_const(vs as i32); memory_copy;
        });
        // SHARE: the heap value was copied (by reference) from the borrowed source —
        // dup it for the same reason. (Overwriting an existing heap value leaks the
        // old one — a separate, non-crashing gap, not a double-free.)
        if Self::is_heap_type(val_ty) {
            wasm!(self.func, {
              local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add; i32_const(ks as i32); i32_add;
              i32_load(0); call(self.emitter.rt.rc_inc); drop;
            });
        }
        self.scratch.free_i32(ei);
        self.scratch.free_i32(tg);
        self.scratch.free_i32(h2);
        self.scratch.free_i32(idx);
    }

    /// Push a closure pair's env pointer (field load via the CLOSURE_PAIR layout).
    pub(super) fn emit_closure_env(&mut self, closure: u32) {
        use super::engine::layout::{CLOSURE_PAIR, closure as lc};
        let off = self.emitter.layout_reg.fixed_offset(CLOSURE_PAIR, lc::ENV_PTR);
        wasm!(self.func, { local_get(closure); i32_load(off); });
    }

    /// Push a closure pair's function-table index.
    pub(super) fn emit_closure_table_idx(&mut self, closure: u32) {
        use super::engine::layout::{CLOSURE_PAIR, closure as lc};
        let off = self.emitter.layout_reg.fixed_offset(CLOSURE_PAIR, lc::TABLE_IDX);
        wasm!(self.func, { local_get(closure); i32_load(off); });
    }
}
