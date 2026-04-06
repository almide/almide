//! Macro for concise WASM instruction emission.
//!
//! Usage (FuncCompiler — pass `self.func`):
//! ```ignore
//! wasm!(self.func, {
//!     i32_const(4);
//!     i32_load(0);
//!     i32_add;
//!     local_get(0);
//!     i64_store(4);
//!     call(some_idx);
//!     br_if(1);
//!     block_empty;
//!     loop_empty;
//!     end;
//! });
//! ```
//!
//! Usage (runtime — pass local `f`):
//! ```ignore
//! wasm!(f, {
//!     global_get(heap_ptr);
//!     local_set(1);
//! });
//! ```

/// Emit a sequence of WASM instructions concisely.
/// First argument is any expression with `.instruction()` method.
macro_rules! wasm {
    ($f:expr, { $($tt:tt)* }) => {
        wasm!(@emit $f, $($tt)*)
    };

    // Terminal
    (@emit $f:expr,) => {};

    // ── Const ──
    (@emit $f:expr, i32_const($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Const($v));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_const($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Const($v));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_const($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Const($v));
        wasm!(@emit $f, $($rest)*)
    };

    // ── Variables ──
    (@emit $f:expr, local_get($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::LocalGet($v));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, local_set($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::LocalSet($v));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, local_tee($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::LocalTee($v));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, global_get($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::GlobalGet($v));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, global_set($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::GlobalSet($v));
        wasm!(@emit $f, $($rest)*)
    };

    // ── Memory (i32) ──
    // Two-arg form: i32_load(offset, mem_index)
    (@emit $f:expr, i32_load($off:expr, $mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Load(wasm_encoder::MemArg {
            offset: $off as u64, align: 2, memory_index: $mem,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    // One-arg form: i32_load(offset) → memory 0
    (@emit $f:expr, i32_load($off:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Load(wasm_encoder::MemArg {
            offset: $off as u64, align: 2, memory_index: 0,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_store($off:expr, $mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Store(wasm_encoder::MemArg {
            offset: $off as u64, align: 2, memory_index: $mem,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_store($off:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Store(wasm_encoder::MemArg {
            offset: $off as u64, align: 2, memory_index: 0,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_load8_u($off:expr, $mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Load8U(wasm_encoder::MemArg {
            offset: $off as u64, align: 0, memory_index: $mem,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_load8_u($off:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Load8U(wasm_encoder::MemArg {
            offset: $off as u64, align: 0, memory_index: 0,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_store8($off:expr, $mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Store8(wasm_encoder::MemArg {
            offset: $off as u64, align: 0, memory_index: $mem,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_store8($off:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Store8(wasm_encoder::MemArg {
            offset: $off as u64, align: 0, memory_index: 0,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_load16_u($off:expr, $mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Load16U(wasm_encoder::MemArg {
            offset: $off as u64, align: 1, memory_index: $mem,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_load16_u($off:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Load16U(wasm_encoder::MemArg {
            offset: $off as u64, align: 1, memory_index: 0,
        }));
        wasm!(@emit $f, $($rest)*)
    };

    // ── Memory (i64) ──
    (@emit $f:expr, i64_load($off:expr, $mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Load(wasm_encoder::MemArg {
            offset: $off as u64, align: 3, memory_index: $mem,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_load($off:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Load(wasm_encoder::MemArg {
            offset: $off as u64, align: 3, memory_index: 0,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_store($off:expr, $mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Store(wasm_encoder::MemArg {
            offset: $off as u64, align: 3, memory_index: $mem,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_store($off:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Store(wasm_encoder::MemArg {
            offset: $off as u64, align: 3, memory_index: 0,
        }));
        wasm!(@emit $f, $($rest)*)
    };

    // ── Memory (f64) ──
    (@emit $f:expr, f64_load($off:expr, $mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Load(wasm_encoder::MemArg {
            offset: $off as u64, align: 3, memory_index: $mem,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_load($off:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Load(wasm_encoder::MemArg {
            offset: $off as u64, align: 3, memory_index: 0,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_store($off:expr, $mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Store(wasm_encoder::MemArg {
            offset: $off as u64, align: 3, memory_index: $mem,
        }));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_store($off:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Store(wasm_encoder::MemArg {
            offset: $off as u64, align: 3, memory_index: 0,
        }));
        wasm!(@emit $f, $($rest)*)
    };

    // ── Arithmetic (i32) ──
    (@emit $f:expr, i32_add; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Add); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_sub; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Sub); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_mul; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Mul); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_div_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32DivU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_rem_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32RemU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_eq; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Eq); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_ne; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Ne); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_eqz; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Eqz); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_ge_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32GeU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_gt_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32GtU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_lt_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32LtS); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_gt_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32GtS); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_le_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32LeS); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_ge_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32GeS); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_lt_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32LtU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_le_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32LeU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_and; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32And); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_or; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Or); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_shl; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32Shl); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_shr_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32ShrU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i32_wrap_i64; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I32WrapI64); wasm!(@emit $f, $($rest)*)
    };

    // ── Arithmetic (i64) ──
    (@emit $f:expr, i64_add; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Add); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_sub; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Sub); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_mul; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Mul); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_div_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64DivS); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_rem_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64RemS); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_div_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64DivU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_rem_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64RemU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_eq; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Eq); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_ne; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Ne); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_eqz; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Eqz); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_lt_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64LtS); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_gt_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64GtS); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_le_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64LeS); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_ge_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64GeS); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_ge_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64GeU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_and; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64And); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_or; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Or); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_xor; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Xor); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_shl; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Shl); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_shr_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64ShrS); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_extend_i32_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64ExtendI32U); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_extend_i32_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64ExtendI32S); wasm!(@emit $f, $($rest)*)
    };

    // ── Float ──
    (@emit $f:expr, f64_add; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Add); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_sub; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Sub); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_mul; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Mul); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_div; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Div); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_eq; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Eq); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_ne; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Ne); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_lt; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Lt); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_gt; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Gt); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_le; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Le); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_ge; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Ge); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_sqrt; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Sqrt); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_abs; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Abs); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_neg; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Neg); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_convert_i64_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64ConvertI64S); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_trunc_f64_s; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64TruncF64S); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_floor; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Floor); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_ceil; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Ceil); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_nearest; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Nearest); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_copysign; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Copysign); wasm!(@emit $f, $($rest)*)
    };

    // ── Conversion ──
    (@emit $f:expr, f64_reinterpret_i64; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64ReinterpretI64); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_reinterpret_f64; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64ReinterpretF64); wasm!(@emit $f, $($rest)*)
    };

    // ── Control flow ──
    (@emit $f:expr, call($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Call($v)); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, call_indirect($ty:expr, $tbl:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::CallIndirect { type_index: $ty, table_index: $tbl });
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, return_call($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::ReturnCall($v)); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, return_call_indirect($ty:expr, $tbl:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::ReturnCallIndirect { type_index: $ty, table_index: $tbl });
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, br($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Br($v)); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, br_if($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::BrIf($v)); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, return_; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Return); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, unreachable; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Unreachable); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, drop; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Drop); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, select; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Select); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, end; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::End); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, if_empty; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, if_i32; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I32)));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, if_i64; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I64)));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, if_f64; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::If(wasm_encoder::BlockType::Result(wasm_encoder::ValType::F64)));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, else_; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Else); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, block_empty; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Block(wasm_encoder::BlockType::Empty));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, block_i32; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Block(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I32)));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, block_i64; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Block(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I64)));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, loop_empty; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Loop(wasm_encoder::BlockType::Empty));
        wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, nop; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::Nop); wasm!(@emit $f, $($rest)*)
    };
    // ── Bitwise (i64) ──
    (@emit $f:expr, i64_shl; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Shl); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_shr_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64ShrU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_xor; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Xor); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_rem_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64RemU); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, i64_eqz; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::I64Eqz); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_convert_i64_u; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64ConvertI64U); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64_const($v:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64Const($v)); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, memory_size($mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::MemorySize($mem)); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, memory_grow($mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::MemoryGrow($mem)); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, memory_copy($src:expr, $dst:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::MemoryCopy { src_mem: $src, dst_mem: $dst }); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, memory_copy; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::MemoryCopy { src_mem: 0, dst_mem: 0 }); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, memory_fill($mem:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::MemoryFill($mem)); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, memory_fill; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::MemoryFill(0)); wasm!(@emit $f, $($rest)*)
    };
    // ── SIMD (v128) instructions ──
    (@emit $f:expr, v128_load($offset:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::V128Load(wasm_encoder::MemArg { offset: $offset, align: 4, memory_index: 0 })); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, v128_store($offset:expr); $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::V128Store(wasm_encoder::MemArg { offset: $offset, align: 4, memory_index: 0 })); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64x2_mul; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64x2Mul); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64x2_add; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64x2Add); wasm!(@emit $f, $($rest)*)
    };
    (@emit $f:expr, f64x2_splat; $($rest:tt)*) => {
        $f.instruction(&wasm_encoder::Instruction::F64x2Splat); wasm!(@emit $f, $($rest)*)
    };
}

pub(super) use wasm;
