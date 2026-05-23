//! List stdlib helper methods for WASM codegen.
//!
//! Utility functions used by both calls_list.rs and calls_list_closure.rs:
//! list_elem_ty, emit_elem_copy, emit_elem_store.

use super::{FuncCompiler, WasmEmitter};
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use wasm_encoder::{Function, Instruction, ValType};

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
}

impl SortKind {
    fn elem_size(&self) -> u32 {
        match self { SortKind::Int | SortKind::Float => 8, _ => 4 }
    }
    fn emit_load(&self, f: &mut super::TrackedFunction) {
        match self {
            SortKind::Int => { f.instruction(&Instruction::I64Load(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            SortKind::Float => { f.instruction(&Instruction::F64Load(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            _ => { f.instruction(&Instruction::I32Load(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 })); }
        }
    }
    fn emit_store(&self, f: &mut super::TrackedFunction) {
        match self {
            SortKind::Int => { f.instruction(&Instruction::I64Store(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            SortKind::Float => { f.instruction(&Instruction::F64Store(wasm_encoder::MemArg { offset: 0, align: 3, memory_index: 0 })); }
            _ => { f.instruction(&Instruction::I32Store(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 })); }
        }
    }
    fn emit_copy_one(&self, f: &mut super::TrackedFunction) {
        self.emit_load(f);
        self.emit_store(f);
    }
    /// Emit swap: *left_addr ↔ *right_addr using tmp_ptr as scratch.
    fn emit_swap(&self, f: &mut super::TrackedFunction, left: u32, right: u32, tmp: u32) {
        match self {
            SortKind::Int => {
                // tmp_i64 = *left; *left = *right; *right = tmp_i64
                // Use tmp memory for i64
                wasm!(f, {
                    local_get(tmp); local_get(left); i64_load(0); i64_store(0);
                    local_get(left); local_get(right); i64_load(0); i64_store(0);
                    local_get(right); local_get(tmp); i64_load(0); i64_store(0);
                });
            }
            SortKind::Float => {
                wasm!(f, {
                    local_get(tmp); local_get(left); f64_load(0); f64_store(0);
                    local_get(left); local_get(right); f64_load(0); f64_store(0);
                    local_get(right); local_get(tmp); f64_load(0); f64_store(0);
                });
            }
            _ => {
                wasm!(f, {
                    local_get(tmp); local_get(left); i32_load(0); i32_store(0);
                    local_get(left); local_get(right); i32_load(0); i32_store(0);
                    local_get(right); local_get(tmp); i32_load(0); i32_store(0);
                });
            }
        }
    }

    /// Emit `dst[j] <= key` comparison, leaving an i32 boolean on the stack.
    fn emit_le_cmp(&self, f: &mut super::TrackedFunction, emitter: &WasmEmitter) {
        match self {
            SortKind::Int => { f.instruction(&Instruction::I64LeS); }
            SortKind::Float => { f.instruction(&Instruction::F64Le); }
            SortKind::String => {
                f.instruction(&Instruction::Call(emitter.rt.string.cmp));
                f.instruction(&Instruction::I32Const(0));
                f.instruction(&Instruction::I32LeS);
            }
            SortKind::ListString => {
                f.instruction(&Instruction::Call(emitter.rt.list_list_str_cmp));
                f.instruction(&Instruction::I32Const(0));
                f.instruction(&Instruction::I32LeS);
            }
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

    /// Copy one element from [stack: dst_addr, src_addr] based on type.
    pub(super) fn emit_elem_copy(&mut self, ty: &Ty) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => { wasm!(self.func, { i64_load(0); i64_store(0); }); }
            Some(ValType::F64) => { wasm!(self.func, { f64_load(0); f64_store(0); }); }
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
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `list.sort` with \
                 unsupported element type `{:?}` — extend emit_list_sort_*",
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

        // 1. Copy list header + payload into dst.
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(xs_ptr);
            local_get(xs_ptr); i32_load(0); local_set(len);
            // alloc dst
            i32_const(super::list_layout::HEADER_SIZE); local_get(len); i32_const(es as i32); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(dst);
            local_get(dst); local_get(len); i32_store(0);
            // alloc tmp (same size, no header needed — just data)
            local_get(len); i32_const(es as i32); i32_mul;
            call(self.emitter.rt.alloc); local_set(tmp);
            // bulk copy data with memory.copy
            local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add;
            local_get(xs_ptr); i32_const(super::list_layout::DATA_OFFSET); i32_add;
            local_get(len); i32_const(es as i32); i32_mul;
            memory_copy;
        });

        // 2. Pre-scan: detect sorted (asc) or reverse-sorted (desc).
        //    If asc → skip sort. If desc → reverse in O(n). Else → merge sort.
        //    This handles TimSort-like run detection for the common cases.
        let is_asc = self.scratch.alloc_i32();
        let is_desc = self.scratch.alloc_i32();
        let scan_done = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_get(len); i32_const(2); i32_lt_u;
            if_empty;
              i32_const(1); local_set(scan_done); // trivially sorted
            else_;
              i32_const(1); local_set(is_asc);
              i32_const(1); local_set(is_desc);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_const(1); i32_sub; i32_ge_u; br_if(1);
                // Load dst[i] and dst[i+1], compare
                local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                local_get(i); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // dst[i]
        wasm!(self.func, {
                local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                local_get(i); i32_const(1); i32_add; i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // dst[i+1]
        // Check: if dst[i] > dst[i+1] → not ascending
        // We need both values for two comparisons. Duplicate via locals.
        // Actually, emit_le_cmp consumes both. Let me do two separate scans? No, too slow.
        // Simpler: just check dst[i] <= dst[i+1] for ascending.
        kind.emit_le_cmp(&mut self.func, self.emitter); // dst[i] <= dst[i+1]
        wasm!(self.func, {
                i32_eqz;
                if_empty; i32_const(0); local_set(is_asc); end;
                // Check descending: dst[i] >= dst[i+1]
                // Reload and compare reverse
                local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                local_get(i); i32_const(1); i32_add; i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // dst[i+1]
        wasm!(self.func, {
                local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                local_get(i); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // dst[i]
        kind.emit_le_cmp(&mut self.func, self.emitter); // dst[i+1] <= dst[i]
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
                i32_const(1); local_set(scan_done); // already sorted
              else_;
                local_get(is_desc);
                if_empty;
                  // Reverse in place: swap dst[j] and dst[len-1-j] for j in 0..len/2
                  i32_const(0); local_set(i);
                  block_empty; loop_empty;
                    local_get(i); local_get(len); i32_const(1); i32_shr_u; i32_ge_u; br_if(1);
                    // swap dst[i] and dst[len-1-i]
                    local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                    local_get(i); i32_const(es as i32); i32_mul; i32_add;
                    local_set(left);
                    local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                    local_get(len); i32_const(1); i32_sub; local_get(i); i32_sub;
                    i32_const(es as i32); i32_mul; i32_add;
                    local_set(right);
        });
        // Emit type-specific swap: tmp = *left; *left = *right; *right = tmp
        kind.emit_swap(&mut self.func, left, right, tmp);
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
        // width = 1; while width < len { merge passes; width *= 2 }
        wasm!(self.func, {
            local_get(scan_done); i32_eqz;
            if_empty;
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
                      local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add; local_get(li); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // load dst[li]
        wasm!(self.func, {
                      local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add; local_get(ri); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_load(&mut self.func); // load dst[ri]
        kind.emit_le_cmp(&mut self.func, self.emitter); // dst[li] <= dst[ri]
        wasm!(self.func, {
                    end;
                  else_;
                    i32_const(0); // li exhausted, use right
                  end;
                  // if result: copy from left (li), else copy from right (ri)
                  if_empty;
                    // tmp[k] = dst[li]; li++
                    local_get(tmp); local_get(k); i32_const(es as i32); i32_mul; i32_add;
                    local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add; local_get(li); i32_const(es as i32); i32_mul; i32_add;
        });
        kind.emit_copy_one(&mut self.func);
        wasm!(self.func, {
                    local_get(li); i32_const(1); i32_add; local_set(li);
                  else_;
                    // tmp[k] = dst[ri]; ri++
                    local_get(tmp); local_get(k); i32_const(es as i32); i32_mul; i32_add;
                    local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add; local_get(ri); i32_const(es as i32); i32_mul; i32_add;
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
                  local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add; local_get(k); i32_const(es as i32); i32_mul; i32_add;
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

    /// Emit list.index_of(xs, x) → Option[Int].
    pub(super) fn emit_list_index_of(&mut self, args: &[IrExpr]) {
        let elem_ty = self.resolve_list_elem(&args[0], None);
        let elem_size = values::byte_size(&elem_ty);
        let xs_ptr = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let found_ptr = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let search_val_i64 = self.scratch.alloc_i64();
        let search_val_i32 = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(xs_ptr); });
        // Store search value
        match values::ty_to_valtype(&elem_ty) {
            Some(ValType::I64) => {
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(search_val_i64); });
            }
            _ => {
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(search_val_i32); });
            }
        }
        wasm!(self.func, {
            i32_const(0); local_set(i); // i
            i32_const(0); local_set(result); // result (default: none)
            block_empty; loop_empty;
              local_get(i);
              local_get(xs_ptr); i32_load(0); // len
              i32_ge_u; br_if(1);
        });
        // Compare element
        match values::ty_to_valtype(&elem_ty) {
            Some(ValType::I64) => {
                wasm!(self.func, {
                    local_get(xs_ptr); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                    local_get(i); i32_const(8); i32_mul; i32_add;
                    i64_load(0);
                    local_get(search_val_i64); i64_eq;
                    if_empty;
                      // Found: store some(i) and break
                      i32_const(super::list_layout::HEADER_SIZE); call(self.emitter.rt.alloc); local_set(found_ptr);
                      local_get(found_ptr); local_get(i); i64_extend_i32_u; i64_store(0);
                      local_get(found_ptr); local_set(result); br(2);
                    end;
                });
            }
            _ => {
                wasm!(self.func, {
                    local_get(xs_ptr); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                    local_get(i); i32_const(elem_size as i32); i32_mul; i32_add;
                    i32_load(0);
                    local_get(search_val_i32);
                });
                // String eq or i32 eq
                if matches!(&elem_ty, Ty::String) {
                    wasm!(self.func, { call(self.emitter.rt.string.eq); });
                } else {
                    wasm!(self.func, { i32_eq; });
                }
                wasm!(self.func, {
                    if_empty;
                      i32_const(super::list_layout::HEADER_SIZE); call(self.emitter.rt.alloc); local_set(found_ptr);
                      local_get(found_ptr); local_get(i); i64_extend_i32_u; i64_store(0);
                      local_get(found_ptr); local_set(result); br(2);
                    end;
                });
            }
        }
        wasm!(self.func, {
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
            local_get(result); // result (none if not found)
        });

        self.scratch.free_i32(search_val_i32);
        self.scratch.free_i64(search_val_i64);
        self.scratch.free_i32(result);
        self.scratch.free_i32(found_ptr);
        self.scratch.free_i32(i);
        self.scratch.free_i32(xs_ptr);
    }

    /// Emit list.unique(xs) → List[A]: O(n²) dedup.
    pub(super) fn emit_list_unique(&mut self, args: &[IrExpr]) {
        let elem_ty = self.resolve_list_elem(&args[0], None);
        let es = values::byte_size(&elem_ty) as i32;
        let src = self.scratch.alloc_i32();
        let src_len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let j = self.scratch.alloc_i32();
        let found = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(src);
            local_get(src); i32_load(0); local_set(src_len); // src_len
            i32_const(super::list_layout::HEADER_SIZE); local_get(src_len); i32_const(es); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(dst); // dst
            local_get(dst); i32_const(0); i32_store(0);
            i32_const(0); local_set(i); // i
            block_empty; loop_empty;
              local_get(i); local_get(src_len); i32_ge_u; br_if(1);
              // Check if src[i] already in dst
              i32_const(0); local_set(j); // j
              i32_const(0); local_set(found); // found
              block_empty; loop_empty;
                local_get(j); local_get(dst); i32_load(0); i32_ge_u; br_if(1);
                local_get(src); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                local_get(i); i32_const(es); i32_mul; i32_add;
                i32_load(0);
                local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                local_get(j); i32_const(es); i32_mul; i32_add;
                i32_load(0);
        });
        match &elem_ty {
            Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
            _ => { wasm!(self.func, { i32_eq; }); }
        }
        wasm!(self.func, {
                if_empty; i32_const(1); local_set(found); br(2); end;
                local_get(j); i32_const(1); i32_add; local_set(j);
                br(0);
              end; end;
              local_get(found); i32_eqz;
              if_empty;
                local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                local_get(dst); i32_load(0); i32_const(es); i32_mul; i32_add;
                local_get(src); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                local_get(i); i32_const(es); i32_mul; i32_add;
        });
        self.emit_elem_copy(&elem_ty);
        wasm!(self.func, {
                local_get(dst);
                local_get(dst); i32_load(0); i32_const(1); i32_add;
                i32_store(0);
              end;
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
            local_get(dst);
        });

        self.scratch.free_i32(found);
        self.scratch.free_i32(j);
        self.scratch.free_i32(i);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(src_len);
        self.scratch.free_i32(src);
    }

    /// Emit list.enumerate(xs) → List[(Int, A)].
    pub(super) fn emit_list_enumerate(&mut self, args: &[IrExpr]) {
        let elem_ty = self.resolve_list_elem(&args[0], None);
        let elem_size = values::byte_size(&elem_ty);
        let tuple_size = 8 + elem_size; // Int(8) + elem

        let src_ptr = self.scratch.alloc_i32();
        let len_local = self.scratch.alloc_i32();
        let idx_local = self.scratch.alloc_i32();
        let dst_ptr = self.scratch.alloc_i32();
        let tuple_ptr = self.scratch.alloc_i32();

        // Store src
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(src_ptr);
            // len
            local_get(src_ptr);
            i32_load(0);
            local_set(len_local);
            // Alloc dst: [len] + len * ptr_size(4)
            i32_const(super::list_layout::HEADER_SIZE);
            local_get(len_local);
            i32_const(4); // each entry is a tuple ptr (i32)
            i32_mul;
            i32_add;
            call(self.emitter.rt.alloc);
            local_set(dst_ptr);
            // Store len in dst
            local_get(dst_ptr);
            local_get(len_local);
            i32_store(0);
            // Loop: create tuples
            i32_const(0);
            local_set(idx_local);
            block_empty;
            loop_empty;
        });
        let depth_guard = self.depth_push_n(2);

        wasm!(self.func, {
            local_get(idx_local);
            local_get(len_local);
            i32_ge_u;
            br_if(1);
            // Alloc tuple: [index:i64][element]
            i32_const(tuple_size as i32);
            call(self.emitter.rt.alloc);
            local_set(tuple_ptr); // tuple_ptr
            // tuple.index = idx (as i64)
            local_get(tuple_ptr);
            local_get(idx_local);
            i64_extend_i32_u;
            i64_store(0);
            // tuple.element = src[idx]
            local_get(tuple_ptr);
            // Load src element
            local_get(src_ptr);
            i32_const(super::list_layout::DATA_OFFSET);
            i32_add;
            local_get(idx_local);
            i32_const(elem_size as i32);
            i32_mul;
            i32_add;
        });
        self.emit_load_at(&elem_ty, 0);
        self.emit_store_at(&elem_ty, 8); // store at tuple offset 8

        wasm!(self.func, {
            // dst[idx] = tuple_ptr
            local_get(dst_ptr);
            i32_const(super::list_layout::DATA_OFFSET);
            i32_add;
            local_get(idx_local);
            i32_const(4); // tuple ptrs are i32
            i32_mul;
            i32_add;
            local_get(tuple_ptr);
            i32_store(0);
            // idx++
            local_get(idx_local);
            i32_const(1);
            i32_add;
            local_set(idx_local);
            br(0);
        });

        self.depth_pop(depth_guard);
        wasm!(self.func, {
            end;
            end;
            // Return dst
            local_get(dst_ptr);
        });

        self.scratch.free_i32(tuple_ptr);
        self.scratch.free_i32(dst_ptr);
        self.scratch.free_i32(idx_local);
        self.scratch.free_i32(len_local);
        self.scratch.free_i32(src_ptr);
    }
}
