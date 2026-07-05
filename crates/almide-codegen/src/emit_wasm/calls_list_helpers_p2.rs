impl FuncCompiler<'_> {
    /// Emit `dst[a] <= dst[b]` for the merge-sort comparison, consuming the two
    /// loaded element values on the stack and leaving an i32 boolean. The fast
    /// kinds compare inline; `Ord(ty)` routes through the shared total-order
    /// emitter (`emit_ord_cmp3` returns sign, `<= 0` means `a <= b`).
    fn emit_sort_le_cmp(&mut self, kind: &SortKind) {
        match kind {
            SortKind::Int => { wasm!(self.func, { i64_le_s; }); }
            // Float sort uses IEEE-754 totalOrder, NOT `f64_le` (which is false
            // for any NaN pair and treats -0.0 == +0.0), so it matches native
            // `f64::total_cmp` byte-for-byte: `total_cmp(a,b) <= 0` ⟺ a <= b.
            // C-055.
            SortKind::Float => {
                self.emit_ord_cmp3(&Ty::Float);
                wasm!(self.func, { i32_const(0); i32_le_s; });
            }
            SortKind::String => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(0); i32_le_s; });
            }
            SortKind::ListString => {
                wasm!(self.func, { call(self.emitter.rt.list_list_str_cmp); i32_const(0); i32_le_s; });
            }
            SortKind::Ord(ty) => {
                let ty = ty.clone();
                self.emit_ord_cmp3(&ty);
                wasm!(self.func, { i32_const(0); i32_le_s; });
            }
        }
    }

    /// Emit list.sort (insertion sort for List[Int], List[String], and
    /// List[List[String]] via lexicographic inner-list comparison).
    pub(super) fn emit_list_sort(&mut self, args: &[IrExpr]) {
        // Resolve the element type aggressively — use the expression type
        // first, then fall back to VarTable when the expression was left
        // generic by inference.
        let mut elem_ty = self.resolve_list_elem(&args[0], None);
        if elem_ty.is_unresolved() {
            if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                let vt = self.var_table.get(*id).ty.clone();
                if let Ty::Applied(_, inner) = vt {
                    if let Some(t) = inner.first().cloned() {
                        if !t.is_unresolved() {
                            elem_ty = t;
                        }
                    }
                }
            }
        }
        match &elem_ty {
            Ty::Int => self.emit_list_sort_generic(args, SortKind::Int),
            Ty::Float => self.emit_list_sort_generic(args, SortKind::Float),
            Ty::String => self.emit_list_sort_generic(args, SortKind::String),
            // `List[List[T]]` lex sort: when T is String or unresolved (the
            // common fold-accumulator case where type inference leaves `A`
            // unconcretized), treat inner elements as string pointers.
            Ty::Applied(almide_lang::types::TypeConstructorId::List, inner)
                if inner.first().is_some_and(|t| matches!(t, Ty::String) || t.is_unresolved()) =>
            {
                self.emit_list_sort_generic(args, SortKind::ListString)
            }
            // Everything else totally-ordered (Bool, Tuple, Option, Result,
            // nested List, variants) sorts through the shared `emit_ord_cmp3`
            // comparator — the same total order the native `Ord` derive uses.
            // An unresolved element type still ICEs (we cannot pick a width or a
            // comparison for it) rather than emit a wrong-typed sort.
            t if !t.is_unresolved() => {
                let kt = (*t).clone();
                self.emit_list_sort_generic(args, SortKind::Ord(kt))
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `list.sort` with \
                 unresolved element type `{:?}` — type inference must \
                 concretize it before codegen",
                elem_ty
            ),
        }
    }

    /// Parameterized insertion sort. Three element kinds share the same
    /// algorithm; only element size, load/store width, and comparison differ.
    fn emit_list_sort_generic(&mut self, args: &[IrExpr], kind: SortKind) {
        let es = kind.elem_size();
        let xs_ptr = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let tmp = self.scratch.alloc_i32(); // merge temp buffer
        let width = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let left = self.scratch.alloc_i32();
        let mid = self.scratch.alloc_i32();
        let right = self.scratch.alloc_i32();
        let li = self.scratch.alloc_i32();
        let ri = self.scratch.alloc_i32();
        let k = self.scratch.alloc_i32();

        // 1. Alloc dst + pre-scan source for asc/desc detection.
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(xs_ptr);
            local_get(xs_ptr); i32_load(0); local_set(len);
            // alloc dst
            i32_const(self.emitter.layout_reg.header_size(LIST) as i32); local_get(len); i32_const(es as i32); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(dst);
            local_get(dst); local_get(len); i32_store(0);
        });

        // 2. Pre-scan SOURCE (xs_ptr) for asc/desc before copying.
        let is_asc = self.scratch.alloc_i32();
        let is_desc = self.scratch.alloc_i32();
        let scan_done = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_get(len); i32_const(2); i32_lt_u;
            if_empty;
              // len < 2: nothing to sort, but still copy the 0/1 source elements
              // to dst. The sort-proper path (scan_done==0) copies src→dst, but
              // this short-circuit skipped it — so dst's data stayed the zeroed
              // alloc and a singleton sort returned a zeroed element.
              local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
              local_get(xs_ptr); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
              local_get(len); i32_const(es as i32); i32_mul;
              memory_copy;
              i32_const(1); local_set(scan_done);
            else_;
              i32_const(1); local_set(is_asc);
              i32_const(1); local_set(is_desc);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_const(1); i32_sub; i32_ge_u; br_if(1);
                // Load xs[i] and xs[i+1]
                local_get(xs_ptr); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                local_get(i); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // xs[i]
        wasm!(self.func, {
                local_get(xs_ptr); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                local_get(i); i32_const(1); i32_add; i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // xs[i+1]
        // Check: if dst[i] > dst[i+1] → not ascending
        // We need both values for two comparisons. Duplicate via locals.
        // Actually, emit_le_cmp consumes both. Let me do two separate scans? No, too slow.
        // Simpler: just check dst[i] <= dst[i+1] for ascending.
        self.emit_sort_le_cmp(&kind); // dst[i] <= dst[i+1]
        wasm!(self.func, {
                i32_eqz;
                if_empty; i32_const(0); local_set(is_asc); end;
                // Check descending: xs[i] >= xs[i+1]
                local_get(xs_ptr); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                local_get(i); i32_const(1); i32_add; i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // xs[i+1]
        wasm!(self.func, {
                local_get(xs_ptr); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                local_get(i); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // xs[i]
        self.emit_sort_le_cmp(&kind); // dst[i+1] <= dst[i]
        wasm!(self.func, {
                i32_eqz;
                if_empty; i32_const(0); local_set(is_desc); end;
                // Early exit if neither
                local_get(is_asc); local_get(is_desc); i32_or; i32_eqz;
                br_if(1); // break scan loop
                local_get(i); i32_const(1); i32_add; local_set(i); br(0);
              end; end;
              // Determine result
              local_get(is_asc);
              if_empty;
                // Already sorted: just bulk copy
                local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                local_get(xs_ptr); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                local_get(len); i32_const(es as i32); i32_mul;
                memory_copy;
                i32_const(1); local_set(scan_done);
              else_;
                local_get(is_desc);
                if_empty;
                  // Reverse copy: dst[i] = src[len-1-i] (1 pass, no swap)
                  i32_const(0); local_set(i);
                  block_empty; loop_empty;
                    local_get(i); local_get(len); i32_ge_u; br_if(1);
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                    local_get(i); i32_const(es as i32); i32_mul; i32_add;
                    local_get(xs_ptr); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                    local_get(len); i32_const(1); i32_sub; local_get(i); i32_sub;
                    i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_copy_one(&mut self.func);
        wasm!(self.func, {
                    local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                  end; end;
                  i32_const(1); local_set(scan_done);
                else_;
                  i32_const(0); local_set(scan_done);
                end;
              end;
            end;
        });
        self.scratch.free_i32(is_desc);
        self.scratch.free_i32(is_asc);

        // 3. Bottom-up merge sort (only if scan_done == 0).
        wasm!(self.func, {
            local_get(scan_done); i32_eqz;
            if_empty;
            // Copy source to dst + alloc tmp for merge
            local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
            local_get(xs_ptr); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
            local_get(len); i32_const(es as i32); i32_mul;
            memory_copy;
            local_get(len); i32_const(es as i32); i32_mul;
            call(self.emitter.rt.alloc); local_set(tmp);
            i32_const(1); local_set(width);
            block_empty; loop_empty;
              local_get(width); local_get(len); i32_ge_u; br_if(1);
              // for i = 0; i < len; i += width*2
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                // left = i, mid = min(i+width, len), right = min(i+2*width, len)
                local_get(i); local_set(left);
                local_get(i); local_get(width); i32_add; local_set(mid);
                local_get(mid); local_get(len); i32_gt_u;
                if_empty; local_get(len); local_set(mid); end;
                local_get(i); local_get(width); i32_const(2); i32_mul; i32_add; local_set(right);
                local_get(right); local_get(len); i32_gt_u;
                if_empty; local_get(len); local_set(right); end;
                // merge dst[left..mid] and dst[mid..right] into tmp[left..right]
                local_get(left); local_set(li);
                local_get(mid); local_set(ri);
                local_get(left); local_set(k);
                block_empty; loop_empty;
                  local_get(k); local_get(right); i32_ge_u; br_if(1);
                  // if li < mid && (ri >= right || dst[li] <= dst[ri])
                  local_get(li); local_get(mid); i32_lt_u;
                  if_i32;
                    local_get(ri); local_get(right); i32_ge_u;
                    if_i32;
                      i32_const(1); // ri exhausted, use left
                    else_;
                      // compare dst[li] <= dst[ri]
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add; local_get(li); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // load dst[li]
        wasm!(self.func, {
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add; local_get(ri); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // load dst[ri]
        self.emit_sort_le_cmp(&kind); // dst[li] <= dst[ri]
        wasm!(self.func, {
                    end;
                  else_;
                    i32_const(0); // li exhausted, use right
                  end;
                  // if result: copy from left (li), else copy from right (ri)
                  if_empty;
                    // tmp[k] = dst[li]; li++
                    local_get(tmp); local_get(k); i32_const(es as i32); i32_mul; i32_add;
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add; local_get(li); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_copy_one(&mut self.func);
        wasm!(self.func, {
                    local_get(li); i32_const(1); i32_add; local_set(li);
                  else_;
                    // tmp[k] = dst[ri]; ri++
                    local_get(tmp); local_get(k); i32_const(es as i32); i32_mul; i32_add;
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add; local_get(ri); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_copy_one(&mut self.func);
        wasm!(self.func, {
                    local_get(ri); i32_const(1); i32_add; local_set(ri);
                  end;
                  local_get(k); i32_const(1); i32_add; local_set(k);
                  br(0);
                end; end;
                // copy tmp[left..right] back to dst[left..right]
                local_get(left); local_set(k);
                block_empty; loop_empty;
                  local_get(k); local_get(right); i32_ge_u; br_if(1);
                  local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add; local_get(k); i32_const(es as i32); i32_mul; i32_add;
                  local_get(tmp); local_get(k); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_copy_one(&mut self.func);
        wasm!(self.func, {
                  local_get(k); i32_const(1); i32_add; local_set(k);
                  br(0);
                end; end;
                // i += width * 2
                local_get(i); local_get(width); i32_const(2); i32_mul; i32_add; local_set(i);
                br(0);
              end; end;
              // width *= 2
              local_get(width); i32_const(2); i32_mul; local_set(width);
              br(0);
            end; end;
            end; // end if scan_done == 0
            local_get(dst);
        });

        // 3.5 SHARE dup: every element pointer was copied from the SOURCE
        // list exactly once across the four dst-build paths (len<2 memcpy,
        // ascending memcpy, descending copy, merge-initial memcpy; the merge
        // passes only permute within dst) and the source survives with its
        // own deep Dec — one inc per element, or sorting a List[List[..]]
        // frees the inner lists the result shares (list_total_order hang).
        if kind.elems_are_heap() {
            let di = self.scratch.alloc_i32();
            let dl = self.scratch.alloc_i32();
            let data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
            wasm!(self.func, {
                local_set(di); // borrow di as the result holder briefly
                local_get(di); i32_load(0); local_set(dl);
                local_get(di); local_set(dst);
                i32_const(0); local_set(di);
                block_empty; loop_empty;
                    local_get(di); local_get(dl); i32_ge_u; br_if(1);
                    local_get(dst); i32_const(data_off); i32_add;
                    local_get(di); i32_const(4); i32_mul; i32_add;
                    i32_load(0); call(self.emitter.rt.rc_inc); drop;
                    local_get(di); i32_const(1); i32_add; local_set(di);
                    br(0);
                end; end;
                local_get(dst);
            });
            self.scratch.free_i32(dl);
            self.scratch.free_i32(di);
        }

        // 4. Free scratch.
        self.scratch.free_i32(scan_done);
        self.scratch.free_i32(k);
        self.scratch.free_i32(ri);
        self.scratch.free_i32(li);
        self.scratch.free_i32(right);
        self.scratch.free_i32(mid);
        self.scratch.free_i32(left);
        self.scratch.free_i32(i);
        self.scratch.free_i32(width);
        self.scratch.free_i32(tmp);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(len);
        self.scratch.free_i32(xs_ptr);
    }
}
