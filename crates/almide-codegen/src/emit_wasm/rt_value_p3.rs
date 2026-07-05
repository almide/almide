// ── JsonPath runtime functions ──────────────────────────────────────
//
// JsonPath WASM memory layout (tagged heap pointer):
//   JpRoot:  [tag:i32=0]                              (4 bytes)
//   JpField: [tag:i32=1][parent_ptr:i32][name_str:i32] (12 bytes)
//   JpIndex: [tag:i32=2][parent_ptr:i32][idx:i32]      (12 bytes)
//
// The path is a linked list from leaf to root. Runtime functions linearize
// it into a flat segment array before traversal.

/// __json_get_path(value: i32, path: i32) -> i32 (Option[Value]: 0=none, ptr=some)
///
/// Linearize path, then walk value following each segment.
/// For field segments: value must be object (tag=6), find matching key.
/// For index segments: value must be array (tag=5), bounds-check index.
fn compile_json_get_path(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.json_get_path];
    let alloc = emitter.rt.alloc;
    let str_eq = emitter.rt.string.eq;

    // Locals: param 0=value, param 1=path
    // 2=seg_count, 3=cur_path, 4=segs_arr, 5=i, 6=seg_ptr, 7=seg_tag
    // 8=cur_val, 9=list, 10=len, 11=j, 12=pair_ptr, 13=found
    let mut f = Function::new([(12, ValType::I32)]);

    // --- Phase 1: Count segments ---
    // Walk path from leaf to root, counting non-root nodes.
    wasm!(f, {
        i32_const(0); local_set(2);     // seg_count = 0
        local_get(1); local_set(3);     // cur_path = path
        block_empty; loop_empty;
          local_get(3); i32_load(0);    // tag
          i32_eqz; br_if(1);           // tag==0 (root) → done
          local_get(2); i32_const(1); i32_add; local_set(2);
          local_get(3); i32_load(4); local_set(3); // cur_path = parent
          br(0);
        end; end;
    });

    // --- Phase 2: Allocate segments array and fill in reverse ---
    // segs_arr = alloc(seg_count * 4), each slot is a path node ptr.
    wasm!(f, {
        local_get(2); i32_eqz;
        if_empty;
          // Empty path → return some(value): alloc option box.
          // SHARE: the box holds a second reference to the input tree.
          i32_const(4); call(alloc); local_set(13);
          local_get(13); local_get(0); call(emitter.rt.rc_inc); i32_store(0);
          local_get(13);
          return_;
        end;
        local_get(2); i32_const(4); i32_mul; call(alloc); local_set(4); // segs_arr
        local_get(2); local_set(5); // i = seg_count (fill from end)
        local_get(1); local_set(3); // cur_path = path (start from leaf)
        block_empty; loop_empty;
          local_get(3); i32_load(0); i32_eqz; br_if(1); // root → done
          local_get(5); i32_const(1); i32_sub; local_set(5); // i--
          local_get(4); local_get(5); i32_const(4); i32_mul; i32_add;
          local_get(3); i32_store(0); // segs_arr[i] = cur_path
          local_get(3); i32_load(4); local_set(3); // cur_path = parent
          br(0);
        end; end;
    });

    // --- Phase 3: Walk value following segments ---
    // cur_val = value
    wasm!(f, {
        local_get(0); local_set(8); // cur_val = value
        i32_const(0); local_set(5); // i = 0
        block_empty; loop_empty;
          local_get(5); local_get(2); i32_ge_u; br_if(1); // i >= seg_count → done
          // Load segment
          local_get(4); local_get(5); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(6); // seg_ptr
          local_get(6); i32_load(0); local_set(7); // seg_tag
    });

    // --- Field segment (tag=1) ---
    wasm!(f, {
          local_get(7); i32_const(1); i32_eq;
          if_empty;
            // cur_val must be object (tag=6)
            local_get(8); i32_load(0); i32_const(6); i32_ne;
            if_empty; i32_const(0); return_; end; // not object → none
            local_get(8); i32_load(4); local_set(9); // list (pairs)
            local_get(9); i32_load(0); local_set(10); // len
            i32_const(0); local_set(11); // j = 0
            i32_const(0); local_set(13); // found = 0
            block_empty; loop_empty;
              local_get(11); local_get(10); i32_ge_u; br_if(1);
              local_get(9); i32_const(list_data_off()); i32_add;
              local_get(11); i32_const(4); i32_mul; i32_add;
              i32_load(0); local_set(12); // pair_ptr
              local_get(12); i32_load(0); // pair key
              local_get(6); i32_load(8); // segment field name
              call(str_eq);
              if_empty;
                local_get(12); i32_load(4); local_set(8); // cur_val = pair value
                i32_const(1); local_set(13); // found = 1
                br(2);
              end;
              local_get(11); i32_const(1); i32_add; local_set(11);
              br(0);
            end; end;
            local_get(13); i32_eqz;
            if_empty; i32_const(0); return_; end; // key not found → none
          end;
    });

    // --- Index segment (tag=2) ---
    wasm!(f, {
          local_get(7); i32_const(2); i32_eq;
          if_empty;
            // cur_val must be array (tag=5)
            local_get(8); i32_load(0); i32_const(VTAG_ARRAY); i32_ne;
            if_empty; i32_const(0); return_; end; // not array → none
            local_get(8); i32_load(4); local_set(9); // list
            local_get(9); i32_load(0); local_set(10); // len
            local_get(6); i32_load(8); local_set(11); // index value
    });
    emit_normalize_neg_index(&mut f, 11, 10); // native: i<0 → len+i
    wasm!(f, {
            // Bounds check (still-negative-after-normalize counts as OOB)
            local_get(11); i32_const(0); i32_lt_s;
            local_get(11); local_get(10); i32_ge_s;
            i32_or;
            if_empty; i32_const(0); return_; end; // out of bounds → none
            // cur_val = list[index]
            local_get(9); i32_const(list_data_off()); i32_add;
            local_get(11); i32_const(4); i32_mul; i32_add;
            i32_load(0); local_set(8);
          end;
    });

    // --- Next segment ---
    wasm!(f, {
          local_get(5); i32_const(1); i32_add; local_set(5);
          br(0);
        end; end;
    });

    // --- Return some(cur_val): alloc Option box ---
    // SHARE: cur_val is an interior pointer into the surviving input tree.
    wasm!(f, {
        i32_const(4); call(alloc); local_set(13);
        local_get(13); local_get(8); call(emitter.rt.rc_inc); i32_store(0);
        local_get(13);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.json_get_path, type_idx, f));
}

/// __json_set_path(value: i32, path: i32, new_val: i32) -> i32 (Result[Value, String])
///
/// Linearize path, then iteratively walk down saving values at each depth,
/// then rebuild from leaf to root replacing at the target path.
fn compile_json_set_path(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.json_set_path];
    let alloc = emitter.rt.alloc;
    let str_eq = emitter.rt.string.eq;

    // set_path is now FULLY INFALLIBLE, mirroring native `set_at_steps`: an Index
    // step over a non-array or an OOB index is a local no-op, and a Field step
    // over a non-object node AUTOVIVIFIES (replaces it with a fresh single-key
    // object). No path produces an Err — the prior "expected array" / "index out
    // of bounds" / "expected object" strings were all removed.

    // Locals: param 0=value, param 1=path, param 2=new_val
    // 3=seg_count, 4=cur_path, 5=segs_arr, 6=depth, 7=seg_ptr, 8=seg_tag
    // 9=cur_val, 10=list, 11=len, 12=j, 13=pair_ptr, 14=result
    // 15=new_list, 16=val_stack, 17=found, 18=idx
    let mut f = Function::new([(16, ValType::I32)]);

    // --- Phase 1: Count segments ---
    wasm!(f, {
        i32_const(0); local_set(3);
        local_get(1); local_set(4);
        block_empty; loop_empty;
          local_get(4); i32_load(0); i32_eqz; br_if(1);
          local_get(3); i32_const(1); i32_add; local_set(3);
          local_get(4); i32_load(4); local_set(4);
          br(0);
        end; end;
    });

    // --- Phase 2: Allocate and fill segments array ---
    wasm!(f, {
        local_get(3); i32_eqz;
        if_empty;
          // Empty path → ok(new_val)
          i32_const(8); call(alloc); local_set(14);
          local_get(14); i32_const(0); i32_store(0);
          local_get(14); local_get(2); i32_store(4);
          local_get(14);
          return_;
        end;
        local_get(3); i32_const(4); i32_mul; call(alloc); local_set(5);
        local_get(3); local_set(6);
        local_get(1); local_set(4);
        block_empty; loop_empty;
          local_get(4); i32_load(0); i32_eqz; br_if(1);
          local_get(6); i32_const(1); i32_sub; local_set(6);
          local_get(5); local_get(6); i32_const(4); i32_mul; i32_add;
          local_get(4); i32_store(0);
          local_get(4); i32_load(4); local_set(4);
          br(0);
        end; end;
    });

    // --- Phase 3: Walk forward saving values at each depth ---
    // val_stack = alloc((seg_count+1) * 4): val_stack[d] = value at depth d
    wasm!(f, {
        local_get(3); i32_const(1); i32_add; i32_const(4); i32_mul;
        call(alloc); local_set(16); // val_stack
        local_get(16); local_get(0); i32_store(0); // val_stack[0] = value
        i32_const(0); local_set(6); // depth = 0
        block_empty; loop_empty;
          local_get(6); local_get(3); i32_const(1); i32_sub; i32_ge_u; br_if(1); // depth >= seg_count-1 → done
          local_get(5); local_get(6); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(7); // seg_ptr = segs_arr[depth]
          local_get(7); i32_load(0); local_set(8); // seg_tag
          // Load cur_val from val_stack[depth]
          local_get(16); local_get(6); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(9);
    });

    // Navigate field during forward walk.
    //
    // Native `set_at_steps` Field match (runtime/rs/src/json.rs:277-289) is
    // INFALLIBLE: an object recurses into the matching/absent key (seeding the
    // absent key with `Object(vec![])`), and a NON-object node is REPLACED by a
    // fresh single-key object built from `Object(vec![])` too. So both
    // "key absent in object" and "node is not an object" descend into an empty
    // object — i.e. the placeholder for the next depth is `{}`, never `null` and
    // never an Err. The prior `path error: expected object` Err diverged: native
    // autovivifies here instead of failing.
    wasm!(f, {
          local_get(8); i32_const(1); i32_eq;
          if_empty;
            i32_const(0); local_set(17); // found = 0
            // Only scan pairs when cur_val is actually an object; otherwise it
            // is a non-object that native replaces (autoviv) — leave found = 0.
            local_get(9); i32_load(0); i32_const(VTAG_OBJECT); i32_eq;
            if_empty;
              local_get(9); i32_load(4); local_set(10); // pairs list
              local_get(10); i32_load(0); local_set(11); // len
              i32_const(0); local_set(12);
              block_empty; loop_empty;
                local_get(12); local_get(11); i32_ge_u; br_if(1);
                local_get(10); i32_const(list_data_off()); i32_add;
                local_get(12); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(13); // pair
                local_get(13); i32_load(0);
                local_get(7); i32_load(8);
                call(str_eq);
                if_empty;
                  local_get(16); local_get(6); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
                  local_get(13); i32_load(4); i32_store(0);
                  i32_const(1); local_set(17);
                  br(2);
                end;
                local_get(12); i32_const(1); i32_add; local_set(12);
                br(0);
              end; end;
            end;
            // Key absent (or cur_val was a non-object): seed the next depth with
            // a fresh empty object, mirroring native `Object(vec![])`.
            local_get(17); i32_eqz;
            if_empty;
    });
    emit_make_empty_object(&mut f, 17, alloc);
    wasm!(f, {
              local_get(16); local_get(6); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
              local_get(17); i32_store(0);
            end;
          end;
    });

    // Navigate index during forward walk. Native `set_at_steps` Index match
    // (runtime/rs/src/json.rs:290-300) makes a non-array node OR an OOB index a
    // LOCAL no-op (`j.clone()` / array unchanged) — it does NOT abort the outer
    // operation. The leaf-to-root rebuild's Index branch already reproduces that
    // local no-op (it discards `cur_built` and re-emits `orig_val` = the node at
    // this depth), so the forward walk must NOT hard-return the original root:
    // doing so erased any autovivification that happened at a shallower depth
    // (e.g. `.x[0].y` over a scalar root → native `{"x":{}}`, not the untouched
    // root). The forward walk only needs a valid placeholder for the next depth;
    // the rebuild throws it away at this Index level. Use an empty object so the
    // placeholder is never garbage and matches native's `Object(vec![])` seed.
    let emit_index_noop_placeholder = |f: &mut Function| {
        // val_stack[depth+1] = {}  (don't-care value; rebuild discards it here)
        emit_make_empty_object(f, 17, alloc);
        wasm!(f, {
            local_get(16); local_get(6); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
            local_get(17); i32_store(0);
        });
    };
    wasm!(f, {
          local_get(8); i32_const(2); i32_eq;
          if_empty;
            local_get(9); i32_load(0); i32_const(VTAG_ARRAY); i32_ne;
            if_empty;
    });
    emit_index_noop_placeholder(&mut f);
    wasm!(f, {
            else_;
            local_get(9); i32_load(4); local_set(10); // list
            local_get(10); i32_load(0); local_set(11); // len
            local_get(7); i32_load(8); local_set(18); // idx
    });
    emit_normalize_neg_index(&mut f, 18, 11);
    wasm!(f, {
            local_get(18); i32_const(0); i32_lt_s;
            local_get(18); local_get(11); i32_ge_s;
            i32_or;
            if_empty;
    });
    emit_index_noop_placeholder(&mut f);
    wasm!(f, {
            else_;
            local_get(16); local_get(6); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
            local_get(10); i32_const(list_data_off()); i32_add;
            local_get(18); i32_const(4); i32_mul; i32_add;
            i32_load(0); i32_store(0);
            end;
            end;
          end;
    });

    // Next depth in forward walk
    wasm!(f, {
          local_get(6); i32_const(1); i32_add; local_set(6);
          br(0);
        end; end;
    });

    // --- Phase 4: Rebuild from leaf to root ---
    // cur_built starts as new_val, then we wrap it at each level going backwards.
    wasm!(f, {
        local_get(2); local_set(9); // cur_built = new_val
        local_get(3); i32_const(1); i32_sub; local_set(6); // depth = seg_count - 1
        block_empty; loop_empty;
          local_get(6); i32_const(0); i32_lt_s; br_if(1);
          local_get(5); local_get(6); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(7); // seg
          local_get(7); i32_load(0); local_set(8); // seg_tag
          // orig_val at this depth
          local_get(16); local_get(6); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(14);
    });

    // Rebuild for field segment
    wasm!(f, {
          local_get(8); i32_const(1); i32_eq;
          if_empty;
            local_get(14); i32_load(0); i32_const(VTAG_OBJECT); i32_eq;
            if_empty;
              // Clone pairs, replacing matching key
              local_get(14); i32_load(4); local_set(10); // old pairs
              local_get(10); i32_load(0); local_set(11); // old len
              // Check if key exists
              i32_const(0); local_set(17);
              i32_const(0); local_set(12);
              block_empty; loop_empty;
                local_get(12); local_get(11); i32_ge_u; br_if(1);
                local_get(10); i32_const(list_data_off()); i32_add;
                local_get(12); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(13);
                local_get(13); i32_load(0);
                local_get(7); i32_load(8);
                call(str_eq);
                if_empty; i32_const(1); local_set(17); end;
                local_get(12); i32_const(1); i32_add; local_set(12);
                br(0);
              end; end;
              // new_len = old_len + (found ? 0 : 1)
              local_get(11); local_get(17); i32_eqz; i32_add; local_set(18);
              // Alloc new pairs list
              i32_const(list_data_off()); local_get(18); i32_const(4); i32_mul; i32_add;
              call(alloc); local_set(15);
              local_get(15); local_get(18); i32_store(0);
              // Copy, replacing match
              i32_const(0); local_set(12);
              block_empty; loop_empty;
                local_get(12); local_get(11); i32_ge_u; br_if(1);
                local_get(10); i32_const(list_data_off()); i32_add;
                local_get(12); i32_const(4); i32_mul; i32_add;
                i32_load(0); local_set(13);
                local_get(13); i32_load(0);
                local_get(7); i32_load(8);
                call(str_eq);
                if_empty;
                  // Replace value — the kept KEY string is shared from the
                  // old pair the source tree still owns: dup.
                  i32_const(8); call(alloc); local_set(17);
                  local_get(17); local_get(13); i32_load(0); call(emitter.rt.rc_inc); i32_store(0);
                  local_get(17); local_get(9); i32_store(4);
                  local_get(15); i32_const(list_data_off()); i32_add;
                  local_get(12); i32_const(4); i32_mul; i32_add;
                  local_get(17); i32_store(0);
                else_;
                  // Unchanged pair: shared between old and new object — dup.
                  local_get(15); i32_const(list_data_off()); i32_add;
                  local_get(12); i32_const(4); i32_mul; i32_add;
                  local_get(13); call(emitter.rt.rc_inc); i32_store(0);
                end;
                local_get(12); i32_const(1); i32_add; local_set(12);
                br(0);
              end; end;
              // Append new pair if key was not found
              local_get(18); local_get(11); i32_gt_u;
              if_empty;
                i32_const(8); call(alloc); local_set(17);
                local_get(17); local_get(7); i32_load(8); call(emitter.rt.rc_inc); i32_store(0);
                local_get(17); local_get(9); i32_store(4);
                local_get(15); i32_const(list_data_off()); i32_add;
                local_get(11); i32_const(4); i32_mul; i32_add;
                local_get(17); i32_store(0);
              end;
              // Build object
              i32_const(VALUE_BOX_SIZE); call(alloc); local_set(9);
              local_get(9); i32_const(VTAG_OBJECT); i32_store(0);
              local_get(9); local_get(15); i32_store(4);
            else_;
              // Not an object → AUTOVIVIFY: replace it with a fresh single-key
              // object {seg_key: cur_built}, mirroring native `set_at_steps`
              // Field-over-non-object (json.rs:288).
              i32_const(list_hdr() + 4); call(alloc); local_set(15); // pairs list: 1 slot
              local_get(15); i32_const(1); i32_store(0);
              i32_const(VALUE_BOX_SIZE); call(alloc); local_set(17); // pair [key][val]
              local_get(17); local_get(7); i32_load(8); call(emitter.rt.rc_inc); i32_store(0);
              local_get(17); local_get(9); i32_store(4);
              local_get(15); i32_const(list_data_off()); i32_add; local_get(17); i32_store(0);
              i32_const(VALUE_BOX_SIZE); call(alloc); local_set(9);
              local_get(9); i32_const(VTAG_OBJECT); i32_store(0);
              local_get(9); local_get(15); i32_store(4);
            end;
          end;
    });

    // Rebuild for index segment. Native `set_at_steps` Index match:
    //   - non-array → j.clone() (no-op); OOB → array unchanged (no-op).
    // So when orig is not an array OR the (normalized) index is OOB, keep the
    // original value as the rebuilt node instead of fabricating an array.
    wasm!(f, {
          local_get(8); i32_const(2); i32_eq;
          if_empty;
            // Non-array → no-op: cur_built = orig_val.
            local_get(14); i32_load(0); i32_const(VTAG_ARRAY); i32_ne;
            if_empty;
              local_get(14); call(emitter.rt.rc_inc); local_set(9);
            else_;
              local_get(14); i32_load(4); local_set(10); // old list
              local_get(10); i32_load(0); local_set(11); // len
              local_get(7); i32_load(8); local_set(18); // idx
    });
    emit_normalize_neg_index(&mut f, 18, 11);
    wasm!(f, {
              // OOB (incl. still-negative) → no-op: cur_built = orig_val.
              local_get(18); i32_const(0); i32_lt_s;
              local_get(18); local_get(11); i32_ge_s;
              i32_or;
              if_empty;
                local_get(14); call(emitter.rt.rc_inc); local_set(9);
              else_;
                // Clone list replacing at idx
                i32_const(list_hdr()); local_get(11); i32_const(4); i32_mul; i32_add;
                call(alloc); local_set(15);
                local_get(15); local_get(11); i32_store(0);
                i32_const(0); local_set(12);
                block_empty; loop_empty;
                  local_get(12); local_get(11); i32_ge_u; br_if(1);
                  local_get(15); i32_const(list_data_off()); i32_add;
                  local_get(12); i32_const(4); i32_mul; i32_add;
                  local_get(12); local_get(18); i32_eq;
                  if_i32; local_get(9);
                  else_;
                    // Unchanged element: shared between old and new array — dup.
                    local_get(10); i32_const(list_data_off()); i32_add;
                    local_get(12); i32_const(4); i32_mul; i32_add;
                    i32_load(0); call(emitter.rt.rc_inc);
                  end;
                  i32_store(0);
                  local_get(12); i32_const(1); i32_add; local_set(12);
                  br(0);
                end; end;
                i32_const(8); call(alloc); local_set(9);
                local_get(9); i32_const(VTAG_ARRAY); i32_store(0);
                local_get(9); local_get(15); i32_store(4);
              end;
            end;
          end;
    });

    // Next depth upward
    wasm!(f, {
          local_get(6); i32_const(1); i32_sub; local_set(6);
          br(0);
        end; end;
    });

    // Return ok(result)
    wasm!(f, {
        i32_const(8); call(alloc); local_set(14);
        local_get(14); i32_const(0); i32_store(0);
        local_get(14); local_get(9); i32_store(4);
        local_get(14);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.json_set_path, type_idx, f));
}

/// __json_remove_path(value: i32, path: i32) -> i32 (Value)
///
/// Linearize path, walk to target, rebuild without the target element.
fn compile_json_remove_path(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.json_remove_path];
    let alloc = emitter.rt.alloc;
    let str_eq = emitter.rt.string.eq;

    // Locals: param 0=value, param 1=path
    // 2=seg_count, 3=cur_path, 4=segs_arr, 5=depth, 6=seg_ptr, 7=seg_tag
    // 8=cur_val, 9=list, 10=len, 11=j, 12=pair_ptr, 13=found
    // 14=val_stack, 15=new_list, 16=cur_built, 17=idx, 18=dst
    let mut f = Function::new([(17, ValType::I32)]);

    // --- Phase 1: Count segments ---
    wasm!(f, {
        i32_const(0); local_set(2);
        local_get(1); local_set(3);
        block_empty; loop_empty;
          local_get(3); i32_load(0); i32_eqz; br_if(1);
          local_get(2); i32_const(1); i32_add; local_set(2);
          local_get(3); i32_load(4); local_set(3);
          br(0);
        end; end;
    });

    // --- Phase 2: Allocate and fill segments array ---
    wasm!(f, {
        local_get(2); i32_eqz;
        if_empty;
          // Empty path → return null (removing root itself)
          i32_const(4); call(alloc); local_set(16);
          local_get(16); i32_const(0); i32_store(0);
          local_get(16);
          return_;
        end;
        local_get(2); i32_const(4); i32_mul; call(alloc); local_set(4);
        local_get(2); local_set(5);
        local_get(1); local_set(3);
        block_empty; loop_empty;
          local_get(3); i32_load(0); i32_eqz; br_if(1);
          local_get(5); i32_const(1); i32_sub; local_set(5);
          local_get(4); local_get(5); i32_const(4); i32_mul; i32_add;
          local_get(3); i32_store(0);
          local_get(3); i32_load(4); local_set(3);
          br(0);
        end; end;
    });

    // --- Phase 3: Walk forward saving values at each depth (all but last) ---
    wasm!(f, {
        local_get(2); i32_const(4); i32_mul; call(alloc); local_set(14); // val_stack
        local_get(14); local_get(0); i32_store(0); // val_stack[0] = value
        i32_const(0); local_set(5); // depth = 0
        block_empty; loop_empty;
          local_get(5); local_get(2); i32_const(1); i32_sub; i32_ge_u; br_if(1);
          local_get(4); local_get(5); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(6);
          local_get(6); i32_load(0); local_set(7);
          local_get(14); local_get(5); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(8);
    });

    // Navigate field for walk
    wasm!(f, {
          local_get(7); i32_const(1); i32_eq;
          if_empty;
            local_get(8); i32_load(0); i32_const(6); i32_ne;
            if_empty; local_get(0); call(emitter.rt.rc_inc); return_; end;
            local_get(8); i32_load(4); local_set(9);
            local_get(9); i32_load(0); local_set(10);
            i32_const(0); local_set(11);
            i32_const(0); local_set(13);
            block_empty; loop_empty;
              local_get(11); local_get(10); i32_ge_u; br_if(1);
              local_get(9); i32_const(list_data_off()); i32_add;
              local_get(11); i32_const(4); i32_mul; i32_add;
              i32_load(0); local_set(12);
              local_get(12); i32_load(0);
              local_get(6); i32_load(8);
              call(str_eq);
              if_empty;
                local_get(14); local_get(5); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
                local_get(12); i32_load(4); i32_store(0);
                i32_const(1); local_set(13);
                br(2);
              end;
              local_get(11); i32_const(1); i32_add; local_set(11);
              br(0);
            end; end;
            local_get(13); i32_eqz;
            if_empty; local_get(0); call(emitter.rt.rc_inc); return_; end;
          end;
    });

    // Navigate index for walk
    wasm!(f, {
          local_get(7); i32_const(2); i32_eq;
          if_empty;
            local_get(8); i32_load(0); i32_const(VTAG_ARRAY); i32_ne;
            if_empty; local_get(0); call(emitter.rt.rc_inc); return_; end; // non-array → no-op (orig)
            local_get(8); i32_load(4); local_set(9);
            local_get(9); i32_load(0); local_set(10);
            local_get(6); i32_load(8); local_set(17);
    });
    emit_normalize_neg_index(&mut f, 17, 10);
    wasm!(f, {
            local_get(17); i32_const(0); i32_lt_s;
            local_get(17); local_get(10); i32_ge_s;
            i32_or;
            if_empty; local_get(0); call(emitter.rt.rc_inc); return_; end; // OOB → no-op (orig)
            local_get(14); local_get(5); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
            local_get(9); i32_const(list_data_off()); i32_add;
            local_get(17); i32_const(4); i32_mul; i32_add;
            i32_load(0); i32_store(0);
          end;
    });

    wasm!(f, {
          local_get(5); i32_const(1); i32_add; local_set(5);
          br(0);
        end; end;
    });

    // --- Phase 4: Remove at the last segment ---
    // Load last segment and value at that depth
    wasm!(f, {
        local_get(4); local_get(2); i32_const(1); i32_sub; i32_const(4); i32_mul; i32_add;
        i32_load(0); local_set(6);
        local_get(6); i32_load(0); local_set(7);
        local_get(14); local_get(2); i32_const(1); i32_sub; i32_const(4); i32_mul; i32_add;
        i32_load(0); local_set(8);
    });

    // Remove field from object
    wasm!(f, {
        local_get(7); i32_const(1); i32_eq;
        if_empty;
          local_get(8); i32_load(0); i32_const(6); i32_ne;
          if_empty; local_get(0); call(emitter.rt.rc_inc); return_; end;
          local_get(8); i32_load(4); local_set(9);
          local_get(9); i32_load(0); local_set(10);
          // Alloc new list (worst case same size)
          i32_const(list_data_off()); local_get(10); i32_const(4); i32_mul; i32_add;
          call(alloc); local_set(15);
          i32_const(0); local_set(11); // src
          i32_const(0); local_set(18); // dst
          block_empty; loop_empty;
            local_get(11); local_get(10); i32_ge_u; br_if(1);
            local_get(9); i32_const(list_data_off()); i32_add;
            local_get(11); i32_const(4); i32_mul; i32_add;
            i32_load(0); local_set(12);
            local_get(12); i32_load(0);
            local_get(6); i32_load(8);
            call(str_eq);
            i32_eqz;
            if_empty;
              // Surviving pair: shared with the source object — dup.
              local_get(15); i32_const(list_data_off()); i32_add;
              local_get(18); i32_const(4); i32_mul; i32_add;
              local_get(12); call(emitter.rt.rc_inc); i32_store(0);
              local_get(18); i32_const(1); i32_add; local_set(18);
            end;
            local_get(11); i32_const(1); i32_add; local_set(11);
            br(0);
          end; end;
          local_get(15); local_get(18); i32_store(0); // set actual len
          i32_const(8); call(alloc); local_set(16);
          local_get(16); i32_const(6); i32_store(0);
          local_get(16); local_get(15); i32_store(4);
        end;
    });

    // Remove index from array
    wasm!(f, {
        local_get(7); i32_const(2); i32_eq;
        if_empty;
          local_get(8); i32_load(0); i32_const(VTAG_ARRAY); i32_ne;
          if_empty; local_get(0); call(emitter.rt.rc_inc); return_; end; // non-array → no-op (orig)
          local_get(8); i32_load(4); local_set(9);
          local_get(9); i32_load(0); local_set(10);
          local_get(6); i32_load(8); local_set(17);
    });
    emit_normalize_neg_index(&mut f, 17, 10);
    wasm!(f, {
          local_get(17); i32_const(0); i32_lt_s;
          local_get(17); local_get(10); i32_ge_s;
          i32_or;
          if_empty; local_get(0); call(emitter.rt.rc_inc); return_; end; // OOB → no-op (orig)
          // Alloc new list (len - 1)
          local_get(10); i32_const(1); i32_sub; local_set(13);
          i32_const(list_hdr()); local_get(13); i32_const(4); i32_mul; i32_add;
          call(alloc); local_set(15);
          local_get(15); local_get(13); i32_store(0);
          i32_const(0); local_set(11); // src
          i32_const(0); local_set(18); // dst
          block_empty; loop_empty;
            local_get(11); local_get(10); i32_ge_u; br_if(1);
            local_get(11); local_get(17); i32_ne;
            if_empty;
              local_get(15); i32_const(list_data_off()); i32_add;
              local_get(18); i32_const(4); i32_mul; i32_add;
              local_get(9); i32_const(list_data_off()); i32_add;
              local_get(11); i32_const(4); i32_mul; i32_add;
              i32_load(0); i32_store(0);
              local_get(18); i32_const(1); i32_add; local_set(18);
            end;
            local_get(11); i32_const(1); i32_add; local_set(11);
            br(0);
          end; end;
          i32_const(8); call(alloc); local_set(16);
          local_get(16); i32_const(VTAG_ARRAY); i32_store(0);
          local_get(16); local_get(15); i32_store(4);
        end;
    });

    // --- Phase 5: Rebuild upward from seg_count-2 to 0 ---
    // cur_built is in local 16
    wasm!(f, {
        local_get(2); i32_const(2); i32_sub; local_set(5); // depth = seg_count - 2
        block_empty; loop_empty;
          local_get(5); i32_const(0); i32_lt_s; br_if(1);
          local_get(4); local_get(5); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(6);
          local_get(6); i32_load(0); local_set(7);
          local_get(14); local_get(5); i32_const(4); i32_mul; i32_add;
          i32_load(0); local_set(8); // orig val
    });

    // Rebuild field
    wasm!(f, {
          local_get(7); i32_const(1); i32_eq;
          if_empty;
            local_get(8); i32_load(4); local_set(9);
            local_get(9); i32_load(0); local_set(10);
            i32_const(list_data_off()); local_get(10); i32_const(4); i32_mul; i32_add;
            call(alloc); local_set(15);
            local_get(15); local_get(10); i32_store(0);
            i32_const(0); local_set(11);
            block_empty; loop_empty;
              local_get(11); local_get(10); i32_ge_u; br_if(1);
              local_get(9); i32_const(list_data_off()); i32_add;
              local_get(11); i32_const(4); i32_mul; i32_add;
              i32_load(0); local_set(12);
              local_get(12); i32_load(0);
              local_get(6); i32_load(8);
              call(str_eq);
              if_empty;
                i32_const(8); call(alloc); local_set(13);
                local_get(13); local_get(12); i32_load(0); call(emitter.rt.rc_inc); i32_store(0);
                local_get(13); local_get(16); i32_store(4);
                local_get(15); i32_const(list_data_off()); i32_add;
                local_get(11); i32_const(4); i32_mul; i32_add;
                local_get(13); call(emitter.rt.rc_inc); i32_store(0);
              else_;
                local_get(15); i32_const(list_data_off()); i32_add;
                local_get(11); i32_const(4); i32_mul; i32_add;
                local_get(12); call(emitter.rt.rc_inc); i32_store(0);
              end;
              local_get(11); i32_const(1); i32_add; local_set(11);
              br(0);
            end; end;
            i32_const(8); call(alloc); local_set(16);
            local_get(16); i32_const(6); i32_store(0);
            local_get(16); local_get(15); i32_store(4);
          end;
    });

    // Rebuild index (intermediate Index segment, e.g. xs[2].name). The forward
    // walk already validated this level is an array with an in-bounds index, so
    // a non-array here is a defensive no-op; the normalization mirrors the
    // forward walk so a negative intermediate index targets the same slot.
    wasm!(f, {
          local_get(7); i32_const(2); i32_eq;
          if_empty;
            local_get(8); i32_load(0); i32_const(VTAG_ARRAY); i32_ne;
            if_empty;
              local_get(8); local_set(16); // non-array → keep orig as built node
            else_;
              local_get(8); i32_load(4); local_set(9);
              local_get(9); i32_load(0); local_set(10);
              local_get(6); i32_load(8); local_set(17);
    });
    emit_normalize_neg_index(&mut f, 17, 10);
    wasm!(f, {
              i32_const(list_hdr()); local_get(10); i32_const(4); i32_mul; i32_add;
              call(alloc); local_set(15);
              local_get(15); local_get(10); i32_store(0);
              i32_const(0); local_set(11);
              block_empty; loop_empty;
                local_get(11); local_get(10); i32_ge_u; br_if(1);
                local_get(15); i32_const(list_data_off()); i32_add;
                local_get(11); i32_const(4); i32_mul; i32_add;
                local_get(11); local_get(17); i32_eq;
                if_i32; local_get(16);
                else_;
                  // Unchanged element: shared between old and new array — dup.
                  local_get(9); i32_const(list_data_off()); i32_add;
                  local_get(11); i32_const(4); i32_mul; i32_add;
                  i32_load(0); call(emitter.rt.rc_inc);
                end;
                i32_store(0);
                local_get(11); i32_const(1); i32_add; local_set(11);
                br(0);
              end; end;
              i32_const(8); call(alloc); local_set(16);
              local_get(16); i32_const(VTAG_ARRAY); i32_store(0);
              local_get(16); local_get(15); i32_store(4);
            end;
          end;
    });

    wasm!(f, {
          local_get(5); i32_const(1); i32_sub; local_set(5);
          br(0);
        end; end;
    });

    // Return cur_built
    wasm!(f, {
        local_get(16);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.json_remove_path, type_idx, f));
}
