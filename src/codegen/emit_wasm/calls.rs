//! Function call emission — emit_call and related helpers.

use crate::ir::{CallTarget, IrExpr, IrStringPart};
use crate::types::Ty;
use wasm_encoder::{BlockType, Instruction, MemArg, ValType};

use super::FuncCompiler;
use super::values;

impl FuncCompiler<'_> {
    pub(super) fn emit_call(&mut self, target: &CallTarget, args: &[IrExpr], _ret_ty: &Ty) {
        match target {
            CallTarget::Named { name } => {
                match name.as_str() {
                    "println" => {
                        let arg = &args[0];
                        match &arg.ty {
                            Ty::String => {
                                self.emit_expr(arg);
                                self.func.instruction(&Instruction::Call(self.emitter.rt.println_str));
                            }
                            Ty::Int => {
                                self.emit_expr(arg);
                                self.func.instruction(&Instruction::Call(self.emitter.rt.println_int));
                            }
                            Ty::Bool => {
                                // Convert bool to "true"/"false"
                                self.emit_expr(arg);
                                let true_str = self.emitter.intern_string("true");
                                let false_str = self.emitter.intern_string("false");
                                self.func.instruction(&Instruction::If(BlockType::Result(wasm_encoder::ValType::I32)));
                                self.func.instruction(&Instruction::I32Const(true_str as i32));
                                self.func.instruction(&Instruction::Else);
                                self.func.instruction(&Instruction::I32Const(false_str as i32));
                                self.func.instruction(&Instruction::End);
                                self.func.instruction(&Instruction::Call(self.emitter.rt.println_str));
                            }
                            Ty::Float => {
                                // Phase 1: print float as int (truncated)
                                self.emit_expr(arg);
                                self.func.instruction(&Instruction::I64TruncF64S);
                                self.func.instruction(&Instruction::Call(self.emitter.rt.println_int));
                            }
                            _ => {
                                // Unsupported type: skip arg and print "<unsupported>"
                                let s = self.emitter.intern_string("<unsupported>");
                                self.func.instruction(&Instruction::I32Const(s as i32));
                                self.func.instruction(&Instruction::Call(self.emitter.rt.println_str));
                            }
                        }
                    }
                    "assert_eq" => {
                        self.emit_assert_eq(&args[0], &args[1]);
                    }
                    "assert" => {
                        // assert(cond) or assert(cond, msg) — trap if false
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Eqz);
                        self.func.instruction(&Instruction::If(BlockType::Empty));
                        self.func.instruction(&Instruction::Unreachable);
                        self.func.instruction(&Instruction::End);
                        // Drop message arg if present (evaluated but unused)
                    }
                    "assert_ne" => {
                        // assert_ne(left, right) — trap if equal
                        self.emit_eq(&args[0], &args[1], false);
                        // If equal → trap
                        self.func.instruction(&Instruction::If(BlockType::Empty));
                        self.func.instruction(&Instruction::Unreachable);
                        self.func.instruction(&Instruction::End);
                    }
                    _ => {
                        // Check if this is a unit variant constructor (e.g., Red, None)
                        if args.is_empty() {
                            if let Some(tag) = self.find_unit_variant_tag(name) {
                                // Allocate [tag:i32]
                                self.func.instruction(&Instruction::I32Const(4));
                                self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                                let scratch = self.match_i32_base + self.match_depth;
                                self.func.instruction(&Instruction::LocalSet(scratch));
                                self.func.instruction(&Instruction::LocalGet(scratch));
                                self.func.instruction(&Instruction::I32Const(tag as i32));
                                self.func.instruction(&Instruction::I32Store(MemArg {
                                    offset: 0, align: 2, memory_index: 0,
                                }));
                                self.func.instruction(&Instruction::LocalGet(scratch));
                                return;
                            }
                        }
                        // User-defined function call
                        for arg in args {
                            self.emit_expr(arg);
                        }
                        if let Some(&func_idx) = self.emitter.func_map.get(name.as_str()) {
                            self.func.instruction(&Instruction::Call(func_idx));
                        } else {
                            self.func.instruction(&Instruction::Unreachable);
                        }
                    }
                }
            }

            CallTarget::Module { module, func } => {
                match (module.as_str(), func.as_str()) {
                    ("int", "to_string") => {
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    ("float", "to_string") => {
                        // Phase 1: truncate to int, then int_to_string
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I64TruncF64S);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    ("string", "length") | ("string", "len") => {
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Load(super::expressions::mem(0)));
                        self.func.instruction(&Instruction::I64ExtendI32U);
                    }
                    ("int", "parse") => {
                        // Stub: return ok(0) as Result[Int, String]
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::Drop);
                        // Allocate Result: [tag=0 (ok), value=0 (i64)]
                        self.func.instruction(&Instruction::I32Const(12)); // 4 tag + 8 i64
                        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                        let scratch = self.match_i32_base + self.match_depth;
                        self.func.instruction(&Instruction::LocalSet(scratch));
                        self.func.instruction(&Instruction::LocalGet(scratch));
                        self.func.instruction(&Instruction::I32Const(0)); // tag = ok
                        self.func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));
                        self.func.instruction(&Instruction::LocalGet(scratch));
                        self.func.instruction(&Instruction::I64Const(0)); // value = 0
                        self.func.instruction(&Instruction::I64Store(MemArg { offset: 4, align: 3, memory_index: 0 }));
                        self.func.instruction(&Instruction::LocalGet(scratch));
                    }
                    ("string", "contains") => {
                        // string.contains(haystack, needle) -> bool
                        // Brute force: O(n*m) substring search
                        self.emit_expr(&args[0]); // haystack ptr
                        self.emit_expr(&args[1]); // needle ptr
                        self.func.instruction(&Instruction::Call(self.emitter.rt.str_contains));
                    }
                    ("string", "trim") => {
                        // Stub: return the string as-is (no whitespace handling)
                        self.emit_expr(&args[0]);
                    }
                    ("string", "to_upper") | ("string", "to_lower") => {
                        // Stub: return as-is
                        self.emit_expr(&args[0]);
                    }
                    ("string", "repeat") | ("string", "reverse") | ("string", "replace")
                    | ("string", "split") | ("string", "join") | ("string", "slice")
                    | ("string", "get") | ("string", "count") | ("string", "starts_with")
                    | ("string", "ends_with") | ("string", "index_of")
                    | ("string", "pad_start") | ("string", "pad_end")
                    | ("string", "trim_start") | ("string", "trim_end") => {
                        self.emit_stub_call(args);
                    }
                    ("list", "map") => {
                        self.emit_list_map(&args[0], &args[1], _ret_ty);
                    }
                    ("list", "filter") | ("list", "fold")
                    | ("list", "reverse") | ("list", "find") | ("list", "any") | ("list", "all")
                    | ("list", "count") | ("list", "sort_by") | ("list", "flat_map")
                    | ("list", "filter_map") | ("list", "get") | ("list", "drop")
                    | ("list", "take") | ("list", "zip")
                    | ("list", "enumerate") | ("list", "contains") | ("list", "sort") => {
                        self.emit_stub_call(args);
                    }
                    ("map", "len") | ("map", "length") | ("map", "size") => {
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Load(super::expressions::mem(0)));
                        self.func.instruction(&Instruction::I64ExtendI32U);
                    }
                    ("list", "len") | ("list", "length") => {
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Load(super::expressions::mem(0)));
                        self.func.instruction(&Instruction::I64ExtendI32U);
                    }
                    ("math", "abs") => {
                        self.emit_expr(&args[0]);
                        match &args[0].ty {
                            Ty::Int => {
                                // abs(x) = if x < 0 then -x else x
                                let s = self.match_i64_base + self.match_depth;
                                self.func.instruction(&Instruction::LocalSet(s));
                                self.func.instruction(&Instruction::LocalGet(s));
                                self.func.instruction(&Instruction::I64Const(0));
                                self.func.instruction(&Instruction::I64LtS);
                                self.func.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
                                self.func.instruction(&Instruction::I64Const(0));
                                self.func.instruction(&Instruction::LocalGet(s));
                                self.func.instruction(&Instruction::I64Sub);
                                self.func.instruction(&Instruction::Else);
                                self.func.instruction(&Instruction::LocalGet(s));
                                self.func.instruction(&Instruction::End);
                            }
                            Ty::Float => {
                                self.func.instruction(&Instruction::F64Abs);
                            }
                            _ => {}
                        }
                    }
                    ("math", "max") | ("math", "min") => {
                        self.emit_expr(&args[0]);
                        self.emit_expr(&args[1]);
                        match (func.as_str(), &args[0].ty) {
                            ("max", Ty::Int) => {
                                let s = self.match_i64_base + self.match_depth;
                                // a b on stack. if a > b then a else b
                                self.func.instruction(&Instruction::LocalSet(s));
                                let s2 = s + 1; // need second i64
                                self.func.instruction(&Instruction::LocalSet(s2));
                                self.func.instruction(&Instruction::LocalGet(s2));
                                self.func.instruction(&Instruction::LocalGet(s));
                                self.func.instruction(&Instruction::I64GtS);
                                self.func.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
                                self.func.instruction(&Instruction::LocalGet(s2));
                                self.func.instruction(&Instruction::Else);
                                self.func.instruction(&Instruction::LocalGet(s));
                                self.func.instruction(&Instruction::End);
                            }
                            ("min", Ty::Int) => {
                                let s = self.match_i64_base + self.match_depth;
                                self.func.instruction(&Instruction::LocalSet(s));
                                let s2 = s + 1;
                                self.func.instruction(&Instruction::LocalSet(s2));
                                self.func.instruction(&Instruction::LocalGet(s2));
                                self.func.instruction(&Instruction::LocalGet(s));
                                self.func.instruction(&Instruction::I64LtS);
                                self.func.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
                                self.func.instruction(&Instruction::LocalGet(s2));
                                self.func.instruction(&Instruction::Else);
                                self.func.instruction(&Instruction::LocalGet(s));
                                self.func.instruction(&Instruction::End);
                            }
                            ("max", _) => { self.func.instruction(&Instruction::F64Max); }
                            ("min", _) => { self.func.instruction(&Instruction::F64Min); }
                            _ => {}
                        }
                    }
                    ("float", "round") => {
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::F64Nearest);
                    }
                    ("float", "floor") => {
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::F64Floor);
                    }
                    ("float", "ceil") => {
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::F64Ceil);
                    }
                    _ => {
                        self.emit_stub_call(args);
                    }
                }
            }

            CallTarget::Method { object, method } => {
                // UFCS method calls: obj.method(args)
                match method.as_str() {
                    "to_string" if matches!(object.ty, Ty::Int) => {
                        self.emit_expr(object);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    "len" | "length" | "string.len" | "list.len" | "map.len" => {
                        // .len() for String, List, Map — all store length at offset 0
                        self.emit_expr(object);
                        self.func.instruction(&Instruction::I32Load(super::expressions::mem(0)));
                        self.func.instruction(&Instruction::I64ExtendI32U);
                    }
                    "to_string" if matches!(object.ty, Ty::Float) => {
                        self.emit_expr(object);
                        self.func.instruction(&Instruction::I64TruncF64S);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    "map" | "list.map" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        // .map(fn) → list.map(self, fn)
                        self.emit_list_map(object, &args[0], _ret_ty);
                    }
                    "contains" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.str_contains));
                    }
                    _ => {
                        self.emit_expr(object);
                        if values::ty_to_valtype(&object.ty).is_some() {
                            self.func.instruction(&Instruction::Drop);
                        }
                        self.emit_stub_call(args);
                    }
                }
            }

            CallTarget::Computed { callee } => {
                // Closure call: callee is a closure ptr [table_idx: i32][env_ptr: i32]
                // Stack order for call_indirect: [env_ptr, args..., table_idx]
                let scratch = self.match_i32_base + self.match_depth;

                // Evaluate callee → closure ptr
                self.emit_expr(callee);
                self.func.instruction(&Instruction::LocalSet(scratch));

                // Push env_ptr (first hidden arg)
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Load(MemArg {
                    offset: 4, align: 2, memory_index: 0,
                }));

                // Push declared args
                for arg in args {
                    self.emit_expr(arg);
                }

                // Push table_idx (on top of stack for call_indirect)
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Load(MemArg {
                    offset: 0, align: 2, memory_index: 0,
                }));

                // Closure calling convention type: (env: i32, params...) -> ret
                if let Ty::Fn { params, ret } = &callee.ty {
                    let mut closure_params = vec![ValType::I32]; // env_ptr
                    for p in params {
                        if let Some(vt) = values::ty_to_valtype(p) {
                            closure_params.push(vt);
                        }
                    }
                    let ret_types = values::ret_type(ret);
                    let type_idx = self.emitter.register_type(closure_params, ret_types);
                    self.func.instruction(&Instruction::CallIndirect {
                        type_index: type_idx,
                        table_index: 0,
                    });
                } else {
                    self.func.instruction(&Instruction::Unreachable);
                }
            }
        }
    }

    /// Emit a FnRef as a closure: allocate [wrapper_table_idx, 0] on heap.
    pub(super) fn emit_fn_ref_closure(&mut self, name: &str) {
        if let Some(&wrapper_table_idx) = self.emitter.fn_ref_wrappers.get(name) {
            // Allocate closure: [table_idx: i32][env_ptr: i32] = 8 bytes
            self.func.instruction(&Instruction::I32Const(8));
            self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
            let scratch = self.match_i32_base + self.match_depth;
            self.func.instruction(&Instruction::LocalSet(scratch));

            // Store table_idx
            self.func.instruction(&Instruction::LocalGet(scratch));
            self.func.instruction(&Instruction::I32Const(wrapper_table_idx as i32));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

            // Store env_ptr = 0
            self.func.instruction(&Instruction::LocalGet(scratch));
            self.func.instruction(&Instruction::I32Const(0));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

            // Return closure ptr
            self.func.instruction(&Instruction::LocalGet(scratch));
        } else {
            eprintln!("WARNING: FnRef wrapper not found for '{}', using direct table entry", name);
            self.func.instruction(&Instruction::Unreachable);
        }
    }

    /// Emit a lambda as a closure: allocate env + closure on heap.
    pub(super) fn emit_lambda_closure(&mut self, _params: &[(crate::ir::VarId, Ty)], _body: &IrExpr) {
        let lambda_idx = self.emitter.lambda_counter.get();
        self.emitter.lambda_counter.set(lambda_idx + 1);

        if lambda_idx >= self.emitter.lambdas.len() {
            self.func.instruction(&Instruction::Unreachable);
            return;
        }

        let table_idx = self.emitter.lambdas[lambda_idx].table_idx;
        let captures = self.emitter.lambdas[lambda_idx].captures.clone();

        let scratch = self.match_i32_base + self.match_depth;

        if captures.is_empty() {
            // No captures: allocate closure [table_idx, 0]
            self.func.instruction(&Instruction::I32Const(8));
            self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
            self.func.instruction(&Instruction::LocalSet(scratch));

            self.func.instruction(&Instruction::LocalGet(scratch));
            self.func.instruction(&Instruction::I32Const(table_idx as i32));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

            self.func.instruction(&Instruction::LocalGet(scratch));
            self.func.instruction(&Instruction::I32Const(0));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

            self.func.instruction(&Instruction::LocalGet(scratch));
        } else {
            // Allocate env: each capture gets 8 bytes (padded for alignment)
            let env_size = (captures.len() as u32) * 8;
            self.func.instruction(&Instruction::I32Const(env_size as i32));
            self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
            let env_scratch = scratch; // reuse for env_ptr
            self.func.instruction(&Instruction::LocalSet(env_scratch));

            // Store each captured variable into env
            for (ci, (vid, ty)) in captures.iter().enumerate() {
                let offset = (ci as u32) * 8;
                self.func.instruction(&Instruction::LocalGet(env_scratch));
                if let Some(&local_idx) = self.var_map.get(&vid.0) {
                    self.func.instruction(&Instruction::LocalGet(local_idx));
                } else {
                    // Variable not in scope — shouldn't happen
                    self.func.instruction(&Instruction::I32Const(0));
                }
                self.emit_store_at(ty, offset);
            }

            // Allocate closure: [table_idx, env_ptr]
            self.func.instruction(&Instruction::I32Const(8));
            self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
            let closure_scratch = scratch + 1; // second i32 scratch slot
            self.func.instruction(&Instruction::LocalSet(closure_scratch));

            self.func.instruction(&Instruction::LocalGet(closure_scratch));
            self.func.instruction(&Instruction::I32Const(table_idx as i32));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

            self.func.instruction(&Instruction::LocalGet(closure_scratch));
            self.func.instruction(&Instruction::LocalGet(env_scratch));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

            self.func.instruction(&Instruction::LocalGet(closure_scratch));
        }
    }

    /// Emit a stub for an unimplemented call: evaluate args (for side effects), drop values, unreachable.
    pub(super) fn emit_stub_call(&mut self, args: &[IrExpr]) {
        for arg in args {
            self.emit_expr(arg);
            // Only drop if the arg produces a value
            if values::ty_to_valtype(&arg.ty).is_some() {
                self.func.instruction(&Instruction::Drop);
            }
        }
        self.func.instruction(&Instruction::Unreachable);
    }

    /// Emit assert_eq(left, right): compare values, trap if not equal.
    pub(super) fn emit_assert_eq(&mut self, left: &IrExpr, right: &IrExpr) {
        // Use the same equality logic as BinOp::Eq
        self.emit_eq(left, right, false);
        // If not equal (result == 0), trap
        self.func.instruction(&Instruction::I32Eqz);
        self.func.instruction(&Instruction::If(BlockType::Empty));
        self.func.instruction(&Instruction::Unreachable);
        self.func.instruction(&Instruction::End);
    }

    /// Concatenate two strings on the heap via __concat_str runtime.
    pub(super) fn emit_concat_str(&mut self, left: &IrExpr, right: &IrExpr) {
        self.emit_expr(left);
        self.emit_expr(right);
        self.func.instruction(&Instruction::Call(self.emitter.rt.concat_str));
    }

    /// String interpolation: convert each part to string, then concat.
    pub(super) fn emit_string_interp(&mut self, parts: &[IrStringPart]) {
        if parts.is_empty() {
            let empty = self.emitter.intern_string("");
            self.func.instruction(&Instruction::I32Const(empty as i32));
            return;
        }

        // Emit first part as a string
        self.emit_string_part(&parts[0]);

        // For each subsequent part: emit it, then concat with accumulator
        for part in &parts[1..] {
            self.emit_string_part(part);
            self.func.instruction(&Instruction::Call(self.emitter.rt.concat_str));
        }
    }

    /// Emit a single string interpolation part as a string (i32 pointer).
    pub(super) fn emit_string_part(&mut self, part: &IrStringPart) {
        match part {
            IrStringPart::Lit { value } => {
                let offset = self.emitter.intern_string(value);
                self.func.instruction(&Instruction::I32Const(offset as i32));
            }
            IrStringPart::Expr { expr } => {
                match &expr.ty {
                    Ty::String => self.emit_expr(expr),
                    Ty::Int => {
                        self.emit_expr(expr);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    Ty::Bool => {
                        self.emit_expr(expr);
                        let t = self.emitter.intern_string("true");
                        let f = self.emitter.intern_string("false");
                        self.func.instruction(&Instruction::If(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I32)));
                        self.func.instruction(&Instruction::I32Const(t as i32));
                        self.func.instruction(&Instruction::Else);
                        self.func.instruction(&Instruction::I32Const(f as i32));
                        self.func.instruction(&Instruction::End);
                    }
                    Ty::Float => {
                        self.emit_expr(expr);
                        self.func.instruction(&Instruction::I64TruncF64S);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    _ => {
                        // Fallback: emit the expression (already a string pointer or unsupported)
                        self.emit_expr(expr);
                    }
                }
            }
        }
    }

    /// Emit list.map(list, fn) → new list.
    /// Uses memory scratch [0..12] for src_ptr/fn_ptr/dst_ptr.
    /// Key insight: compute dst address BEFORE call_indirect so result goes
    /// directly onto the stack in the right position for store.
    fn emit_list_map(&mut self, list_arg: &IrExpr, fn_arg: &IrExpr, ret_ty: &Ty) {
        let in_elem_ty = if let Ty::Applied(_, args) = &list_arg.ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int };
        let out_elem_ty = if let Ty::Applied(_, args) = ret_ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int };
        let in_size = values::byte_size(&in_elem_ty);
        let out_size = values::byte_size(&out_elem_ty);

        let s = self.match_i32_base + self.match_depth;
        let len_local = s;
        let idx_local = s + 1;
        let mem = super::expressions::mem;

        // Store src_ptr → mem[0], fn_closure → mem[4]
        self.func.instruction(&Instruction::I32Const(0));
        self.emit_expr(list_arg);
        self.func.instruction(&Instruction::I32Store(mem(0)));
        self.func.instruction(&Instruction::I32Const(4));
        self.emit_expr(fn_arg);
        self.func.instruction(&Instruction::I32Store(mem(0)));

        // len = mem[0].len (load src_ptr, load len)
        self.func.instruction(&Instruction::I32Const(0));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::LocalSet(len_local));

        // Alloc dst: 4 + len * out_size → store to mem[8]
        self.func.instruction(&Instruction::I32Const(8));
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::LocalGet(len_local));
        self.func.instruction(&Instruction::I32Const(out_size as i32));
        self.func.instruction(&Instruction::I32Mul);
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
        self.func.instruction(&Instruction::I32Store(mem(0)));

        // dst.len = len
        self.func.instruction(&Instruction::I32Const(8));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::LocalGet(len_local));
        self.func.instruction(&Instruction::I32Store(mem(0)));

        // idx = 0
        self.func.instruction(&Instruction::I32Const(0));
        self.func.instruction(&Instruction::LocalSet(idx_local));

        // Loop
        self.func.instruction(&Instruction::Block(BlockType::Empty));
        self.func.instruction(&Instruction::Loop(BlockType::Empty));
        let saved = self.depth;
        self.depth += 2;

        // break if idx >= len
        self.func.instruction(&Instruction::LocalGet(idx_local));
        self.func.instruction(&Instruction::LocalGet(len_local));
        self.func.instruction(&Instruction::I32GeU);
        self.func.instruction(&Instruction::BrIf(1));

        // ── Compute dst addr FIRST (stays on stack under call result) ──
        // dst_ptr + 4 + idx * out_size
        self.func.instruction(&Instruction::I32Const(8));
        self.func.instruction(&Instruction::I32Load(mem(0))); // dst
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::LocalGet(idx_local));
        self.func.instruction(&Instruction::I32Const(out_size as i32));
        self.func.instruction(&Instruction::I32Mul);
        self.func.instruction(&Instruction::I32Add);
        // Stack: [dst_elem_addr]

        // ── Call fn(element) ──
        // Load closure from mem[4]
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        // env_ptr = closure[4]
        self.func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
        // Stack: [dst_elem_addr, env_ptr]

        // Load src element: src_ptr + 4 + idx * in_size
        self.func.instruction(&Instruction::I32Const(0));
        self.func.instruction(&Instruction::I32Load(mem(0))); // src
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::LocalGet(idx_local));
        self.func.instruction(&Instruction::I32Const(in_size as i32));
        self.func.instruction(&Instruction::I32Mul);
        self.func.instruction(&Instruction::I32Add);
        self.emit_load_at(&in_elem_ty, 0);
        // Stack: [dst_elem_addr, env_ptr, element]

        // table_idx = closure[0]
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::I32Load(mem(0))); // closure
        self.func.instruction(&Instruction::I32Load(mem(0))); // table_idx
        // Stack: [dst_elem_addr, env_ptr, element, table_idx]

        // call_indirect (env, element) → result
        if let Ty::Fn { params, ret } = &fn_arg.ty {
            let mut ct = vec![ValType::I32]; // env
            for p in params { if let Some(vt) = values::ty_to_valtype(p) { ct.push(vt); } }
            let rt = values::ret_type(ret);
            let ti = self.emitter.register_type(ct, rt);
            self.func.instruction(&Instruction::CallIndirect { type_index: ti, table_index: 0 });
        }
        // Stack: [dst_elem_addr, result]

        // ── Store result at dst addr ──
        self.emit_store_at(&out_elem_ty, 0);
        // Stack: []

        // idx++
        self.func.instruction(&Instruction::LocalGet(idx_local));
        self.func.instruction(&Instruction::I32Const(1));
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::LocalSet(idx_local));
        self.func.instruction(&Instruction::Br(0));

        self.depth = saved;
        self.func.instruction(&Instruction::End);
        self.func.instruction(&Instruction::End);

        // Return dst_ptr from mem[8]
        self.func.instruction(&Instruction::I32Const(8));
        self.func.instruction(&Instruction::I32Load(mem(0)));
    }
}
