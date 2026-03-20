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
                        // Check if this is a variant constructor
                        if let Some((tag, is_unit)) = self.find_variant_ctor_tag(name) {
                            if is_unit && args.is_empty() {
                                // Unit variant: allocate [tag:i32]
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
                            } else if !is_unit {
                                // Tuple payload variant: [tag:i32][arg0][arg1]...
                                let mut total_size = 4u32; // tag
                                for arg in args { total_size += values::byte_size(&arg.ty); }
                                self.func.instruction(&Instruction::I32Const(total_size as i32));
                                self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                                let scratch = self.match_i32_base + self.match_depth;
                                self.match_depth += 1;
                                self.func.instruction(&Instruction::LocalSet(scratch));
                                // Write tag
                                self.func.instruction(&Instruction::LocalGet(scratch));
                                self.func.instruction(&Instruction::I32Const(tag as i32));
                                self.func.instruction(&Instruction::I32Store(MemArg {
                                    offset: 0, align: 2, memory_index: 0,
                                }));
                                // Write args
                                let mut offset = 4u32;
                                for arg in args {
                                    self.func.instruction(&Instruction::LocalGet(scratch));
                                    self.emit_expr(arg);
                                    self.emit_store_at(&arg.ty, offset);
                                    offset += values::byte_size(&arg.ty);
                                }
                                self.match_depth -= 1;
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
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.str_trim));
                    }
                    ("string", "to_upper") | ("string", "to_lower") => {
                        let is_upper = func == "to_upper";
                        self.emit_expr(&args[0]);
                        self.emit_str_case_convert(is_upper);
                    }
                    ("string", "starts_with") => {
                        // Store s → mem[0], prefix → mem[4]
                        let mem = super::expressions::mem;
                        self.func.instruction(&Instruction::I32Const(0));
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.emit_expr(&args[1]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));
                        // if s.len < prefix.len → false
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // prefix
                        self.func.instruction(&Instruction::I32Load(mem(0))); // prefix.len
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // s
                        self.func.instruction(&Instruction::I32Load(mem(0))); // s.len
                        self.func.instruction(&Instruction::I32GtU); // prefix.len > s.len
                        self.func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::Else);
                        // mem_eq(s+4, prefix+4, prefix.len)
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add); // s+4
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add); // prefix+4
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // prefix.len
                        self.func.instruction(&Instruction::Call(self.emitter.rt.mem_eq));
                        self.func.instruction(&Instruction::End);
                    }
                    ("string", "ends_with") => {
                        let mem = super::expressions::mem;
                        self.func.instruction(&Instruction::I32Const(0));
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.emit_expr(&args[1]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));
                        // if s.len < suffix.len → false
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32GtU);
                        self.func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::Else);
                        // mem_eq(s+4+(s.len-suffix.len), suffix+4, suffix.len)
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // s.len
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // suffix.len
                        self.func.instruction(&Instruction::I32Sub); // s+4+s.len-suffix.len
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add); // suffix+4
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // suffix.len
                        self.func.instruction(&Instruction::Call(self.emitter.rt.mem_eq));
                        self.func.instruction(&Instruction::End);
                    }
                    ("string", "repeat") | ("string", "reverse") | ("string", "replace")
                    | ("string", "split") | ("string", "join") | ("string", "slice")
                    | ("string", "get") | ("string", "count")
                    | ("string", "index_of")
                    | ("string", "pad_start") | ("string", "pad_end")
                    | ("string", "trim_start") | ("string", "trim_end") => {
                        self.emit_stub_call(args);
                    }
                    ("list", "map") => {
                        self.emit_list_map(&args[0], &args[1], _ret_ty);
                    }
                    ("list", "enumerate") => {
                        // enumerate(list) → list of (index, element) tuples
                        // Each tuple is heap-allocated: [Int(i64), element]
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let tuple_size = 8 + elem_size; // Int(8) + elem

                        let s = self.match_i32_base + self.match_depth;
                        let len_local = s;
                        let idx_local = s + 1;
                        let mem = super::expressions::mem;

                        // Store src → mem[0]
                        self.func.instruction(&Instruction::I32Const(0));
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // len
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::LocalSet(len_local));

                        // Alloc dst: [len] + len * ptr_size(4)
                        self.func.instruction(&Instruction::I32Const(8));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::LocalGet(len_local));
                        self.func.instruction(&Instruction::I32Const(4)); // each entry is a tuple ptr (i32)
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                        self.func.instruction(&Instruction::I32Store(mem(0)));
                        // dst = mem[8]

                        // Store len in dst
                        self.func.instruction(&Instruction::I32Const(8));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::LocalGet(len_local));
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // Loop: create tuples
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::LocalSet(idx_local));

                        self.func.instruction(&Instruction::Block(BlockType::Empty));
                        self.func.instruction(&Instruction::Loop(BlockType::Empty));
                        let saved = self.depth;
                        self.depth += 2;

                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::LocalGet(len_local));
                        self.func.instruction(&Instruction::I32GeU);
                        self.func.instruction(&Instruction::BrIf(1));

                        // Alloc tuple: [index:i64][element]
                        self.func.instruction(&Instruction::I32Const(tuple_size as i32));
                        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                        // tuple_ptr on stack. Store index.
                        let tuple_scratch = self.match_i64_base + self.match_depth;
                        // Can't use i64 local for i32... use mem[12] as temp
                        self.func.instruction(&Instruction::I32Const(12));
                        // swap: stack is [tuple_ptr, 12]. Need [12, tuple_ptr].
                        // Use local
                        self.func.instruction(&Instruction::Drop); // drop 12
                        // Store tuple_ptr to idx_local temporarily... no, it's in use.
                        // Use a different approach: store tuple to mem[12]
                        self.func.instruction(&Instruction::I32Const(12));
                        // Stack: [tuple_ptr, 12]... still wrong order.
                        // Just drop and re-approach: alloc then immediately store to mem[12]
                        self.func.instruction(&Instruction::Drop); // clean
                        self.func.instruction(&Instruction::Drop); // clean

                        // Re-alloc tuple
                        self.func.instruction(&Instruction::I32Const(12));
                        self.func.instruction(&Instruction::I32Const(tuple_size as i32));
                        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                        self.func.instruction(&Instruction::I32Store(mem(0))); // mem[12] = tuple_ptr

                        // tuple.index = idx (as i64)
                        self.func.instruction(&Instruction::I32Const(12));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // tuple_ptr
                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I64ExtendI32U);
                        self.func.instruction(&Instruction::I64Store(MemArg { offset: 0, align: 3, memory_index: 0 }));

                        // tuple.element = src[idx]
                        self.func.instruction(&Instruction::I32Const(12));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // tuple_ptr
                        // Load src element
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // src_ptr
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I32Const(elem_size as i32));
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);
                        self.emit_load_at(&elem_ty, 0);
                        self.emit_store_at(&elem_ty, 8); // store at tuple offset 8

                        // dst[idx] = tuple_ptr
                        self.func.instruction(&Instruction::I32Const(8));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // dst_ptr
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I32Const(4)); // tuple ptrs are i32
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::I32Const(12));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // tuple_ptr
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // idx++
                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I32Const(1));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalSet(idx_local));
                        self.func.instruction(&Instruction::Br(0));

                        self.depth = saved;
                        self.func.instruction(&Instruction::End);
                        self.func.instruction(&Instruction::End);

                        // Return dst
                        self.func.instruction(&Instruction::I32Const(8));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                    }
                    ("list", "get") => {
                        // list.get(list, index) → Option[T]
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let mem = super::expressions::mem;

                        // mem[0]=list, mem[4]=idx(i32)
                        self.func.instruction(&Instruction::I32Const(0));
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.emit_expr(&args[1]);
                        if matches!(&args[1].ty, Ty::Int) {
                            self.func.instruction(&Instruction::I32WrapI64);
                        }
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // bounds: idx >= len → none(0)
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // idx
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // list
                        self.func.instruction(&Instruction::I32Load(mem(0))); // len
                        self.func.instruction(&Instruction::I32GeU);
                        self.func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                        self.func.instruction(&Instruction::I32Const(0)); // none
                        self.func.instruction(&Instruction::Else);
                        // alloc → mem[8]
                        self.func.instruction(&Instruction::I32Const(8));
                        self.func.instruction(&Instruction::I32Const(elem_size as i32));
                        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                        self.func.instruction(&Instruction::I32Store(mem(0)));
                        // dst=mem[8], src=list+4+idx*elem_size
                        self.func.instruction(&Instruction::I32Const(8));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // dst
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // list
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // idx
                        self.func.instruction(&Instruction::I32Const(elem_size as i32));
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);
                        self.emit_load_at(&elem_ty, 0); // load elem
                        self.emit_store_at(&elem_ty, 0); // store at dst
                        self.func.instruction(&Instruction::I32Const(8));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // return ptr
                        self.func.instruction(&Instruction::End);
                    }
                    ("list", "filter") => {
                        // filter(list, fn) → new list with matching elements
                        // Alloc max size, fill matching, update len at end
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let s = self.match_i32_base + self.match_depth;
                        let len_local = s;
                        let idx_local = s + 1;
                        let mem = super::expressions::mem;

                        // mem[0]=src, mem[4]=fn, mem[8]=dst, mem[12]=out_idx
                        self.func.instruction(&Instruction::I32Const(0));
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.emit_expr(&args[1]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // len
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::LocalSet(len_local));

                        // alloc dst (max size = 4 + len * elem_size) → mem[8]
                        self.func.instruction(&Instruction::I32Const(8));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::LocalGet(len_local));
                        self.func.instruction(&Instruction::I32Const(elem_size as i32));
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // out_idx = 0 → mem[12]
                        self.func.instruction(&Instruction::I32Const(12));
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // idx = 0
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::LocalSet(idx_local));

                        // Loop
                        self.func.instruction(&Instruction::Block(BlockType::Empty));
                        self.func.instruction(&Instruction::Loop(BlockType::Empty));
                        let saved = self.depth; self.depth += 2;

                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::LocalGet(len_local));
                        self.func.instruction(&Instruction::I32GeU);
                        self.func.instruction(&Instruction::BrIf(1));

                        // Call predicate: fn(element) → bool (i32)
                        // Load closure
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
                        // Load element
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I32Const(elem_size as i32));
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);
                        self.emit_load_at(&elem_ty, 0);
                        // table_idx
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        // call_indirect
                        if let Ty::Fn { params, ret } = &args[1].ty {
                            let mut ct = vec![ValType::I32];
                            for p in params { if let Some(vt) = values::ty_to_valtype(p) { ct.push(vt); } }
                            let rt = values::ret_type(ret);
                            let ti = self.emitter.register_type(ct, rt);
                            self.func.instruction(&Instruction::CallIndirect { type_index: ti, table_index: 0 });
                        }
                        // If true, copy element to dst
                        self.func.instruction(&Instruction::If(BlockType::Empty));
                        // dst[out_idx] = src[idx]
                        self.func.instruction(&Instruction::I32Const(8));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::I32Const(12));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(elem_size as i32));
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);
                        // load src element
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I32Const(elem_size as i32));
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);
                        self.emit_load_at(&elem_ty, 0);
                        self.emit_store_at(&elem_ty, 0);
                        // out_idx++
                        self.func.instruction(&Instruction::I32Const(12));
                        self.func.instruction(&Instruction::I32Const(12));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(1));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::I32Store(mem(0)));
                        self.func.instruction(&Instruction::End); // end if

                        // idx++
                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I32Const(1));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalSet(idx_local));
                        self.func.instruction(&Instruction::Br(0));

                        self.depth = saved;
                        self.func.instruction(&Instruction::End);
                        self.func.instruction(&Instruction::End);

                        // Set dst.len = out_idx
                        self.func.instruction(&Instruction::I32Const(8));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(12));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // Return dst
                        self.func.instruction(&Instruction::I32Const(8));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                    }
                    ("list", "fold") => {
                        // fold(list, init, fn(acc, elem) → acc)
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let s = self.match_i32_base + self.match_depth;
                        let len_local = s;
                        let idx_local = s + 1;
                        let acc_local = self.match_i64_base + self.match_depth;
                        let mem = super::expressions::mem;

                        // mem[0]=list, mem[4]=fn
                        self.func.instruction(&Instruction::I32Const(0));
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));
                        // acc = init
                        self.emit_expr(&args[1]);
                        self.func.instruction(&Instruction::LocalSet(acc_local));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.emit_expr(&args[2]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // len
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::LocalSet(len_local));

                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::LocalSet(idx_local));

                        self.func.instruction(&Instruction::Block(BlockType::Empty));
                        self.func.instruction(&Instruction::Loop(BlockType::Empty));
                        let saved = self.depth; self.depth += 2;

                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::LocalGet(len_local));
                        self.func.instruction(&Instruction::I32GeU);
                        self.func.instruction(&Instruction::BrIf(1));

                        // acc = fn(acc, elem)
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(MemArg { offset: 4, align: 2, memory_index: 0 }));
                        self.func.instruction(&Instruction::LocalGet(acc_local));
                        // load elem
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I32Const(elem_size as i32));
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);
                        self.emit_load_at(&elem_ty, 0);
                        // table_idx
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        if let Ty::Fn { params, ret } = &args[2].ty {
                            let mut ct = vec![ValType::I32];
                            for p in params { if let Some(vt) = values::ty_to_valtype(p) { ct.push(vt); } }
                            let rt = values::ret_type(ret);
                            let ti = self.emitter.register_type(ct, rt);
                            self.func.instruction(&Instruction::CallIndirect { type_index: ti, table_index: 0 });
                        }
                        self.func.instruction(&Instruction::LocalSet(acc_local));

                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I32Const(1));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalSet(idx_local));
                        self.func.instruction(&Instruction::Br(0));

                        self.depth = saved;
                        self.func.instruction(&Instruction::End);
                        self.func.instruction(&Instruction::End);

                        self.func.instruction(&Instruction::LocalGet(acc_local));
                    }
                    ("list", "reverse") => {
                        // reverse(list) → new list with elements in reverse order
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let s = self.match_i32_base + self.match_depth;
                        let len_local = s;
                        let idx_local = s + 1;
                        let mem = super::expressions::mem;

                        // mem[0]=src
                        self.func.instruction(&Instruction::I32Const(0));
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // len
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::LocalSet(len_local));

                        // alloc dst → mem[4]
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::LocalGet(len_local));
                        self.func.instruction(&Instruction::I32Const(elem_size as i32));
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // dst.len = len
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::LocalGet(len_local));
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // Loop: dst[len-1-i] = src[i]
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::LocalSet(idx_local));
                        self.func.instruction(&Instruction::Block(BlockType::Empty));
                        self.func.instruction(&Instruction::Loop(BlockType::Empty));
                        let saved = self.depth; self.depth += 2;

                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::LocalGet(len_local));
                        self.func.instruction(&Instruction::I32GeU);
                        self.func.instruction(&Instruction::BrIf(1));

                        // dst addr: dst + 4 + (len-1-i) * elem_size
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalGet(len_local));
                        self.func.instruction(&Instruction::I32Const(1));
                        self.func.instruction(&Instruction::I32Sub);
                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I32Sub);
                        self.func.instruction(&Instruction::I32Const(elem_size as i32));
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);

                        // src elem: src + 4 + i * elem_size
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I32Const(elem_size as i32));
                        self.func.instruction(&Instruction::I32Mul);
                        self.func.instruction(&Instruction::I32Add);
                        self.emit_load_at(&elem_ty, 0);
                        self.emit_store_at(&elem_ty, 0);

                        self.func.instruction(&Instruction::LocalGet(idx_local));
                        self.func.instruction(&Instruction::I32Const(1));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalSet(idx_local));
                        self.func.instruction(&Instruction::Br(0));

                        self.depth = saved;
                        self.func.instruction(&Instruction::End);
                        self.func.instruction(&Instruction::End);

                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                    }
                    ("list", "sort") if false => { // disabled: validation issues
                        // Sort list (Int only, insertion sort on a copy)
                        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                            a.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        let elem_size = values::byte_size(&elem_ty);
                        let s = self.match_i32_base + self.match_depth;
                        let len_local = s;
                        let i_local = s + 1;
                        let mem = super::expressions::mem;

                        // mem[0]=src
                        self.func.instruction(&Instruction::I32Const(0));
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // len
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::LocalSet(len_local));

                        // Copy src to dst (alloc + byte copy) → mem[4]
                        let total_bytes_expr = |this: &mut Self| {
                            this.func.instruction(&Instruction::I32Const(4));
                            this.func.instruction(&Instruction::LocalGet(len_local));
                            this.func.instruction(&Instruction::I32Const(elem_size as i32));
                            this.func.instruction(&Instruction::I32Mul);
                            this.func.instruction(&Instruction::I32Add);
                        };
                        self.func.instruction(&Instruction::I32Const(4));
                        total_bytes_expr(self);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                        self.func.instruction(&Instruction::I32Store(mem(0)));

                        // Copy all bytes from src to dst
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // dst
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::I32Load(mem(0))); // src
                        total_bytes_expr(self);
                        // Use byte copy loop
                        self.func.instruction(&Instruction::I32Const(0));
                        self.func.instruction(&Instruction::LocalSet(i_local));
                        // stack: [dst, src, total]
                        // Store to mem[8]=dst, mem[12]=src, mem[16]=total
                        self.func.instruction(&Instruction::I32Const(16));
                        self.func.instruction(&Instruction::I32Store(mem(0))); // total→mem[16]
                        self.func.instruction(&Instruction::I32Const(12));
                        self.func.instruction(&Instruction::I32Store(mem(0))); // src→mem[12]
                        // dst already in mem[4]

                        self.func.instruction(&Instruction::Block(BlockType::Empty));
                        self.func.instruction(&Instruction::Loop(BlockType::Empty));
                        let saved = self.depth; self.depth += 2;
                        self.func.instruction(&Instruction::LocalGet(i_local));
                        self.func.instruction(&Instruction::I32Const(16));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I32GeU);
                        self.func.instruction(&Instruction::BrIf(1));
                        // dst[i] = src[i]
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::LocalGet(i_local));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::I32Const(12));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::LocalGet(i_local));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));
                        self.func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));
                        self.func.instruction(&Instruction::LocalGet(i_local));
                        self.func.instruction(&Instruction::I32Const(1));
                        self.func.instruction(&Instruction::I32Add);
                        self.func.instruction(&Instruction::LocalSet(i_local));
                        self.func.instruction(&Instruction::Br(0));
                        self.depth = saved;
                        self.func.instruction(&Instruction::End);
                        self.func.instruction(&Instruction::End);

                        // Now sort dst in-place using insertion sort (Int/i64 only)
                        // for i = 1; i < len; i++:
                        //   key = dst[i]; j = i-1
                        //   while j >= 0 && dst[j] > key: dst[j+1] = dst[j]; j--
                        //   dst[j+1] = key
                        if matches!(&elem_ty, Ty::Int) {
                            let j_local = self.match_i64_base + self.match_depth; // borrow i64 for j
                            // Can't use i64 for i32 index. Use mem scratch instead.
                            // Actually for sort we need: i, j, key. Use mem[8]=j, mem[12]=key(i64→8bytes at mem[12..20])
                            // This is getting complex. Use a simple bubble sort instead:
                            // for i in 0..len-1: for j in 0..len-1-i: if dst[j] > dst[j+1]: swap
                            self.func.instruction(&Instruction::I32Const(0));
                            self.func.instruction(&Instruction::LocalSet(i_local)); // i=0 (outer)
                            self.func.instruction(&Instruction::Block(BlockType::Empty));
                            self.func.instruction(&Instruction::Loop(BlockType::Empty));
                            let s2 = self.depth; self.depth += 2;
                            // if i >= len-1 break
                            self.func.instruction(&Instruction::LocalGet(i_local));
                            self.func.instruction(&Instruction::LocalGet(len_local));
                            self.func.instruction(&Instruction::I32Const(1));
                            self.func.instruction(&Instruction::I32Sub);
                            self.func.instruction(&Instruction::I32GeU);
                            self.func.instruction(&Instruction::BrIf(1));
                            // inner loop: j from 0 to len-1-i
                            self.func.instruction(&Instruction::I32Const(8)); // mem[8] = j = 0
                            self.func.instruction(&Instruction::I32Const(0));
                            self.func.instruction(&Instruction::I32Store(mem(0)));
                            self.func.instruction(&Instruction::Block(BlockType::Empty));
                            self.func.instruction(&Instruction::Loop(BlockType::Empty));
                            self.depth += 2;
                            // if j >= len-1-i break
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Load(mem(0))); // j
                            self.func.instruction(&Instruction::LocalGet(len_local));
                            self.func.instruction(&Instruction::I32Const(1));
                            self.func.instruction(&Instruction::I32Sub);
                            self.func.instruction(&Instruction::LocalGet(i_local));
                            self.func.instruction(&Instruction::I32Sub);
                            self.func.instruction(&Instruction::I32GeU);
                            self.func.instruction(&Instruction::BrIf(1));
                            // if dst[4+j*8] > dst[4+(j+1)*8]: swap
                            // Load dst[j]
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Load(mem(0))); // dst
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Load(mem(0))); // j
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Mul);
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I64Load(MemArg { offset: 0, align: 3, memory_index: 0 }));
                            // Load dst[j+1]
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Load(mem(0)));
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Load(mem(0)));
                            self.func.instruction(&Instruction::I32Const(1));
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Mul);
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I64Load(MemArg { offset: 0, align: 3, memory_index: 0 }));
                            // if dst[j] > dst[j+1]
                            self.func.instruction(&Instruction::I64GtS);
                            self.func.instruction(&Instruction::If(BlockType::Empty));
                            // Swap: store dst[j] to mem[12..20], dst[j+1]→dst[j], mem[12]→dst[j+1]
                            // Save dst[j] to mem[12]
                            self.func.instruction(&Instruction::I32Const(12));
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Load(mem(0)));
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Load(mem(0)));
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Mul);
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I64Load(MemArg { offset: 0, align: 3, memory_index: 0 }));
                            self.func.instruction(&Instruction::I64Store(MemArg { offset: 0, align: 3, memory_index: 0 }));
                            // dst[j] = dst[j+1]
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Load(mem(0)));
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Load(mem(0)));
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Mul);
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Load(mem(0)));
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Load(mem(0)));
                            self.func.instruction(&Instruction::I32Const(1));
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Mul);
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I64Load(MemArg { offset: 0, align: 3, memory_index: 0 }));
                            self.func.instruction(&Instruction::I64Store(MemArg { offset: 0, align: 3, memory_index: 0 }));
                            // dst[j+1] = mem[12]
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Load(mem(0)));
                            self.func.instruction(&Instruction::I32Const(4));
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Load(mem(0)));
                            self.func.instruction(&Instruction::I32Const(1));
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Mul);
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Const(12));
                            self.func.instruction(&Instruction::I64Load(MemArg { offset: 0, align: 3, memory_index: 0 }));
                            self.func.instruction(&Instruction::I64Store(MemArg { offset: 0, align: 3, memory_index: 0 }));
                            self.func.instruction(&Instruction::End); // end if swap
                            // j++
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Const(8));
                            self.func.instruction(&Instruction::I32Load(mem(0)));
                            self.func.instruction(&Instruction::I32Const(1));
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::I32Store(mem(0)));
                            self.func.instruction(&Instruction::Br(0));
                            self.depth -= 2;
                            self.func.instruction(&Instruction::End);
                            self.func.instruction(&Instruction::End);
                            // i++
                            self.func.instruction(&Instruction::LocalGet(i_local));
                            self.func.instruction(&Instruction::I32Const(1));
                            self.func.instruction(&Instruction::I32Add);
                            self.func.instruction(&Instruction::LocalSet(i_local));
                            self.func.instruction(&Instruction::Br(0));
                            self.depth = s2;
                            self.func.instruction(&Instruction::End);
                            self.func.instruction(&Instruction::End);
                        }

                        // Return dst
                        self.func.instruction(&Instruction::I32Const(4));
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                    }
                    ("list", "find") | ("list", "any") | ("list", "all")
                    | ("list", "count") | ("list", "sort_by") | ("list", "flat_map")
                    | ("list", "filter_map") | ("list", "get") | ("list", "drop")
                    | ("list", "take") | ("list", "zip")
                    | ("list", "contains") | ("list", "sort") => {
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
                    ("math", "pi") => {
                        self.func.instruction(&Instruction::F64Const(std::f64::consts::PI));
                    }
                    ("math", "e") => {
                        self.func.instruction(&Instruction::F64Const(std::f64::consts::E));
                    }
                    ("math", "sqrt") => {
                        self.emit_expr(&args[0]);
                        if matches!(&args[0].ty, Ty::Int) {
                            self.func.instruction(&Instruction::F64ConvertI64S);
                        }
                        self.func.instruction(&Instruction::F64Sqrt);
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
                    "sort" | "list.sort" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "sort".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "reverse" | "list.reverse" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "reverse".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "filter" | "list.filter" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone(), args[0].clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "filter".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "fold" | "list.fold" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        let fake = [(**object).clone(), args[0].clone(), args[1].clone()];
                        let target = CallTarget::Module { module: "list".into(), func: "fold".into() };
                        self.emit_call(&target, &fake, _ret_ty);
                    }
                    "map" | "list.map" if matches!(&object.ty, Ty::Applied(_, _)) => {
                        // .map(fn) → list.map(self, fn)
                        self.emit_list_map(object, &args[0], _ret_ty);
                    }
                    "trim" | "string.trim" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.str_trim));
                    }
                    "to_upper" | "string.to_upper" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.emit_str_case_convert(true);
                    }
                    "to_lower" | "string.to_lower" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.emit_str_case_convert(false);
                    }
                    "starts_with" | "string.starts_with" | "ends_with" | "string.ends_with" if matches!(object.ty, Ty::String) => {
                        // Delegate to Module call handler
                        self.emit_expr(object);
                        for arg in args { self.emit_expr(arg); }
                        self.func.instruction(&Instruction::Unreachable); // TODO: wire up properly
                    }
                    "contains" | "string.contains" if matches!(object.ty, Ty::String) => {
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

    /// ASCII case conversion. Expects string ptr on stack. Returns new string ptr.
    fn emit_str_case_convert(&mut self, is_upper: bool) {
        let mem = super::expressions::mem;
        // String ptr is on stack. Store to mem[0] via scratch.
        let scratch = self.match_i32_base + self.match_depth;
        self.func.instruction(&Instruction::LocalSet(scratch));
        self.func.instruction(&Instruction::I32Const(0));
        self.func.instruction(&Instruction::LocalGet(scratch));
        self.func.instruction(&Instruction::I32Store(mem(0)));
        // Alloc dst with same len → mem[4]
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::I32Const(0));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
        self.func.instruction(&Instruction::I32Store(mem(0)));
        // Store len in dst
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::I32Const(0));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::I32Store(mem(0)));
        // Loop: convert each byte
        let s = self.match_i32_base + self.match_depth;
        self.func.instruction(&Instruction::I32Const(0));
        self.func.instruction(&Instruction::LocalSet(s));
        self.func.instruction(&Instruction::Block(BlockType::Empty));
        self.func.instruction(&Instruction::Loop(BlockType::Empty));
        let saved = self.depth; self.depth += 2;
        self.func.instruction(&Instruction::LocalGet(s));
        self.func.instruction(&Instruction::I32Const(0));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::I32GeU);
        self.func.instruction(&Instruction::BrIf(1));
        // dst addr
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::LocalGet(s));
        self.func.instruction(&Instruction::I32Add);
        // src byte
        self.func.instruction(&Instruction::I32Const(0));
        self.func.instruction(&Instruction::I32Load(mem(0)));
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::LocalGet(s));
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));
        // Convert
        self.func.instruction(&Instruction::LocalSet(s + 1));
        if is_upper {
            self.func.instruction(&Instruction::LocalGet(s + 1));
            self.func.instruction(&Instruction::I32Const(97));
            self.func.instruction(&Instruction::I32GeU);
            self.func.instruction(&Instruction::LocalGet(s + 1));
            self.func.instruction(&Instruction::I32Const(122));
            self.func.instruction(&Instruction::I32LeU);
            self.func.instruction(&Instruction::I32And);
            self.func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
            self.func.instruction(&Instruction::LocalGet(s + 1));
            self.func.instruction(&Instruction::I32Const(32));
            self.func.instruction(&Instruction::I32Sub);
            self.func.instruction(&Instruction::Else);
            self.func.instruction(&Instruction::LocalGet(s + 1));
            self.func.instruction(&Instruction::End);
        } else {
            self.func.instruction(&Instruction::LocalGet(s + 1));
            self.func.instruction(&Instruction::I32Const(65));
            self.func.instruction(&Instruction::I32GeU);
            self.func.instruction(&Instruction::LocalGet(s + 1));
            self.func.instruction(&Instruction::I32Const(90));
            self.func.instruction(&Instruction::I32LeU);
            self.func.instruction(&Instruction::I32And);
            self.func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
            self.func.instruction(&Instruction::LocalGet(s + 1));
            self.func.instruction(&Instruction::I32Const(32));
            self.func.instruction(&Instruction::I32Add);
            self.func.instruction(&Instruction::Else);
            self.func.instruction(&Instruction::LocalGet(s + 1));
            self.func.instruction(&Instruction::End);
        }
        self.func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));
        self.func.instruction(&Instruction::LocalGet(s));
        self.func.instruction(&Instruction::I32Const(1));
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::LocalSet(s));
        self.func.instruction(&Instruction::Br(0));
        self.depth = saved;
        self.func.instruction(&Instruction::End);
        self.func.instruction(&Instruction::End);
        // Return dst
        self.func.instruction(&Instruction::I32Const(4));
        self.func.instruction(&Instruction::I32Load(mem(0)));
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
