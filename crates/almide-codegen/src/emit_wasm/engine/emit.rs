//! WasmIR → wasm-encoder emission.
//!
//! This is the ONLY module that knows raw WASM instructions.
//! Every other module works with typed WasmIR ops.

use wasm_encoder::Function;
use super::layout::{LayoutRegistry, LayoutId, FieldId, FieldOffset};
use super::ir::*;

/// Emit a sequence of WasmIR ops into a wasm-encoder Function.
pub fn emit_ops(ops: &[Op], f: &mut Function, reg: &LayoutRegistry) {
    for op in ops {
        emit_op(op, f, reg);
    }
}

/// Emit a single WasmIR op.
pub fn emit_op(op: &Op, f: &mut Function, reg: &LayoutRegistry) {
    use wasm_encoder::Instruction::*;
    match op {
        // ── Stack ──
        Op::LocalGet(l) => { f.instruction(&LocalGet(*l)); }
        Op::LocalSet(l) => { f.instruction(&LocalSet(*l)); }
        Op::LocalTee(l) => { f.instruction(&LocalTee(*l)); }
        Op::GlobalGet(g) => { f.instruction(&GlobalGet(*g)); }
        Op::GlobalSet(g) => { f.instruction(&GlobalSet(*g)); }
        Op::Const(c) => emit_const(c, f),
        Op::Drop => { f.instruction(&Drop); }

        // ── Arithmetic ──
        Op::BinOp(b) => emit_binop(b, f),
        Op::UnOp(u) => emit_unop(u, f),

        // ── Typed memory access ──
        Op::FieldLoad { layout, field, kind } => {
            // Stack: [base_ptr] → [value]
            let offset = reg.fixed_offset(*layout, *field);
            emit_load_at_offset(*kind, offset, f);
        }
        Op::FieldStore { layout, field, kind } => {
            // Stack: [base_ptr, value] → []
            let offset = reg.fixed_offset(*layout, *field);
            emit_store_at_offset(*kind, offset, f);
        }
        Op::ElemAddr { layout, field, stride } => {
            // Stack: [base_ptr, index] → [elem_addr]
            // addr = base + field_offset + index * stride
            let offset = reg.fixed_offset(*layout, *field);
            // Reorder: we need base + offset + idx * stride
            // Stack has [base, idx]. We need: base + offset first, then + idx*stride
            // Use: i32.const(stride); i32.mul; — now stack has [base, idx*stride]
            // But we need to add offset to base first...
            // Better approach: emit as separate ops in the lowering phase.
            // For now: assume stack is [base_ptr, index]
            f.instruction(&I32Const(*stride as i32));
            f.instruction(&I32Mul);
            // Stack: [base_ptr, index*stride]
            f.instruction(&I32Add);
            // Stack: [base_ptr + index*stride]
            f.instruction(&I32Const(offset as i32));
            f.instruction(&I32Add);
            // Stack: [base_ptr + offset + index*stride]
        }
        Op::DynFieldAddr { layout, field } => {
            // Stack: [base_ptr] → [field_addr]
            let mem_field = reg.field(*layout, *field);
            match &mem_field.offset {
                FieldOffset::Fixed(n) => {
                    f.instruction(&I32Const(*n as i32));
                    f.instruction(&I32Add);
                }
                FieldOffset::AfterDynamic { base, size_field } => {
                    // addr = base_ptr + base + base_ptr[size_field_offset]
                    // We need base_ptr twice: once for the add, once for loading cap.
                    // Caller should have base_ptr on stack. We tee it.
                    // Actually, we need a local. The lowering should handle this.
                    // For now, panic — this should be lowered to explicit ops.
                    panic!("DynFieldAddr should be lowered to explicit ops before emission");
                }
            }
        }
        Op::Load(kind) => emit_load(*kind, f),
        Op::Store(kind) => emit_store(*kind, f),

        // ── Collection iteration ──
        Op::ListForEach { list, elem_local, elem_stride, body } => {
            emit_list_foreach(*list, *elem_local, *elem_stride, body, f, reg);
        }
        Op::MapForEach { map, entry_local, entry_stride, body } => {
            emit_map_foreach(*map, *entry_local, *entry_stride, body, f, reg);
        }

        // ── Allocation ──
        Op::Alloc => {
            // Size on stack → ptr on stack.
            // The alloc function index must be resolved externally.
            // For now, this is a placeholder.
            panic!("Op::Alloc must be resolved to a Call during lowering");
        }

        // ── Perceus RC ──
        Op::RcInc => {
            panic!("Op::RcInc must be resolved to a Call during lowering");
        }
        Op::RcDec { .. } => {
            panic!("Op::RcDec must be resolved to a Call during lowering");
        }
        Op::CowCheck { .. } => {
            panic!("Op::CowCheck not yet implemented");
        }

        // ── String ops ──
        Op::StringConcat => {
            panic!("Op::StringConcat must be resolved to a Call during lowering");
        }
        Op::StringInterp { .. } => {
            panic!("Op::StringInterp not yet implemented in emit phase");
        }

        // ── Deep equality ──
        Op::DeepEq { .. } => {
            panic!("Op::DeepEq must be resolved during lowering");
        }

        // ── Control flow ──
        Op::Block(body) => {
            f.instruction(&Block(wasm_encoder::BlockType::Empty));
            emit_ops(body, f, reg);
            f.instruction(&End);
        }
        Op::Loop(body) => {
            f.instruction(&Loop(wasm_encoder::BlockType::Empty));
            emit_ops(body, f, reg);
            f.instruction(&End);
        }
        Op::If { then, else_ } => {
            f.instruction(&If(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I32)));
            emit_ops(then, f, reg);
            f.instruction(&Else);
            emit_ops(else_, f, reg);
            f.instruction(&End);
        }
        Op::IfVoid { then, else_ } => {
            f.instruction(&If(wasm_encoder::BlockType::Empty));
            emit_ops(then, f, reg);
            if !else_.is_empty() {
                f.instruction(&Else);
                emit_ops(else_, f, reg);
            }
            f.instruction(&End);
        }
        Op::Br(depth) => { f.instruction(&Br(*depth)); }
        Op::BrIf(depth) => { f.instruction(&BrIf(*depth)); }
        Op::Return => { f.instruction(&Return); }
        Op::Unreachable => { f.instruction(&Unreachable); }

        // ── Calls ──
        Op::Call(idx) => { f.instruction(&Call(*idx)); }
        Op::CallIndirect { sig } => {
            f.instruction(&CallIndirect { type_index: *sig, table_index: 0 });
        }

        // ── Memory ──
        Op::MemoryCopy => {
            f.instruction(&MemoryCopy { src_mem: 0, dst_mem: 0 });
        }
        Op::MemorySize => { f.instruction(&MemorySize(0)); }
        Op::MemoryGrow => { f.instruction(&MemoryGrow(0)); }

        // ── Sequence ──
        Op::Seq(ops) => emit_ops(ops, f, reg),

        Op::AllocCollection { .. } => {
            panic!("Op::AllocCollection not yet implemented");
        }
    }
}

// ── Helpers ──

fn mem_arg(offset: u32, align: u32) -> wasm_encoder::MemArg {
    wasm_encoder::MemArg { offset: offset as u64, align, memory_index: 0 }
}

fn emit_const(c: &Const, f: &mut Function) {
    use wasm_encoder::Instruction::*;
    match c {
        Const::I32(v) => { f.instruction(&I32Const(*v)); }
        Const::I64(v) => { f.instruction(&I64Const(*v)); }
        Const::F32(v) => { f.instruction(&F32Const(*v)); }
        Const::F64(v) => { f.instruction(&F64Const(*v)); }
    }
}

fn emit_load_at_offset(kind: LoadKind, offset: u32, f: &mut Function) {
    use wasm_encoder::Instruction::*;
    let (align, instr): (u32, _) = match kind {
        LoadKind::I32 => (2, I32Load(mem_arg(offset, 2))),
        LoadKind::I64 => (3, I64Load(mem_arg(offset, 3))),
        LoadKind::F32 => (2, F32Load(mem_arg(offset, 2))),
        LoadKind::F64 => (3, F64Load(mem_arg(offset, 3))),
        LoadKind::U8 => (0, I32Load8U(mem_arg(offset, 0))),
        LoadKind::I8S => (0, I32Load8S(mem_arg(offset, 0))),
        LoadKind::U16 => (1, I32Load16U(mem_arg(offset, 1))),
    };
    let _ = align;
    f.instruction(&instr);
}

fn emit_load(kind: LoadKind, f: &mut Function) {
    emit_load_at_offset(kind, 0, f);
}

fn emit_store_at_offset(kind: StoreKind, offset: u32, f: &mut Function) {
    use wasm_encoder::Instruction::*;
    let instr = match kind {
        StoreKind::I32 => I32Store(mem_arg(offset, 2)),
        StoreKind::I64 => I64Store(mem_arg(offset, 3)),
        StoreKind::F32 => F32Store(mem_arg(offset, 2)),
        StoreKind::F64 => F64Store(mem_arg(offset, 3)),
        StoreKind::I8 => I32Store8(mem_arg(offset, 0)),
        StoreKind::I16 => I32Store16(mem_arg(offset, 1)),
    };
    f.instruction(&instr);
}

fn emit_store(kind: StoreKind, f: &mut Function) {
    emit_store_at_offset(kind, 0, f);
}

fn emit_binop(b: &BinOp, f: &mut Function) {
    use wasm_encoder::Instruction::*;
    let instr = match b {
        BinOp::I32Add => I32Add, BinOp::I32Sub => I32Sub, BinOp::I32Mul => I32Mul,
        BinOp::I32DivU => I32DivU, BinOp::I32DivS => I32DivS, BinOp::I32RemS => I32RemS,
        BinOp::I32And => I32And, BinOp::I32Or => I32Or, BinOp::I32Xor => I32Xor,
        BinOp::I32Shl => I32Shl, BinOp::I32ShrU => I32ShrU, BinOp::I32ShrS => I32ShrS,
        BinOp::I32Eq => I32Eq, BinOp::I32Ne => I32Ne,
        BinOp::I32LtS => I32LtS, BinOp::I32LeS => I32LeS,
        BinOp::I32GtS => I32GtS, BinOp::I32GeS => I32GeS,
        BinOp::I32LtU => I32LtU, BinOp::I32LeU => I32LeU,
        BinOp::I32GtU => I32GtU, BinOp::I32GeU => I32GeU,
        BinOp::I64Add => I64Add, BinOp::I64Sub => I64Sub, BinOp::I64Mul => I64Mul,
        BinOp::I64DivS => I64DivS, BinOp::I64RemS => I64RemS,
        BinOp::I64Eq => I64Eq, BinOp::I64Ne => I64Ne,
        BinOp::I64LtS => I64LtS, BinOp::I64LeS => I64LeS,
        BinOp::I64GtS => I64GtS, BinOp::I64GeS => I64GeS,
        BinOp::F64Add => F64Add, BinOp::F64Sub => F64Sub,
        BinOp::F64Mul => F64Mul, BinOp::F64Div => F64Div,
        BinOp::F64Eq => F64Eq, BinOp::F64Ne => F64Ne,
        BinOp::F64Lt => F64Lt, BinOp::F64Le => F64Le,
        BinOp::F64Gt => F64Gt, BinOp::F64Ge => F64Ge,
    };
    f.instruction(&instr);
}

fn emit_unop(u: &UnOp, f: &mut Function) {
    use wasm_encoder::Instruction::*;
    let instr = match u {
        UnOp::I32Eqz => I32Eqz, UnOp::I64Eqz => I64Eqz,
        UnOp::I32WrapI64 => I32WrapI64,
        UnOp::I64ExtendI32S => I64ExtendI32S, UnOp::I64ExtendI32U => I64ExtendI32U,
        UnOp::F64ConvertI64S => F64ConvertI64S, UnOp::I64TruncF64S => I64TruncF64S,
        UnOp::F64Sqrt => F64Sqrt, UnOp::F64Abs => F64Abs,
        UnOp::F64Neg => F64Neg, UnOp::F64Ceil => F64Ceil, UnOp::F64Floor => F64Floor,
    };
    f.instruction(&instr);
}

/// Emit list iteration: for each element in list[0..len], invoke body.
fn emit_list_foreach(
    list: Local, elem_local: Local, elem_stride: u32,
    body: &[Op], f: &mut Function, reg: &LayoutRegistry,
) {
    use wasm_encoder::Instruction::*;
    use super::layout;

    let len_offset = reg.fixed_offset(layout::LIST, layout::list::LEN);
    let data_offset = reg.fixed_offset(layout::LIST, layout::list::DATA);

    // i = 0
    f.instruction(&I32Const(0));
    f.instruction(&LocalSet(elem_local + 1)); // use elem_local+1 as index (convention)
    // TODO: proper local allocation for index

    // block { loop {
    f.instruction(&Block(wasm_encoder::BlockType::Empty));
    f.instruction(&Loop(wasm_encoder::BlockType::Empty));

    // if i >= len: br 1
    f.instruction(&LocalGet(elem_local + 1));
    f.instruction(&LocalGet(list));
    f.instruction(&I32Load(mem_arg(len_offset, 2)));
    f.instruction(&I32GeU);
    f.instruction(&BrIf(1));

    // elem_local = list + data_offset + i * stride
    f.instruction(&LocalGet(list));
    f.instruction(&I32Const(data_offset as i32));
    f.instruction(&I32Add);
    f.instruction(&LocalGet(elem_local + 1));
    f.instruction(&I32Const(elem_stride as i32));
    f.instruction(&I32Mul);
    f.instruction(&I32Add);
    f.instruction(&LocalSet(elem_local));

    // body
    emit_ops(body, f, reg);

    // i++
    f.instruction(&LocalGet(elem_local + 1));
    f.instruction(&I32Const(1));
    f.instruction(&I32Add);
    f.instruction(&LocalSet(elem_local + 1));
    f.instruction(&Br(0));

    // } }
    f.instruction(&End);
    f.instruction(&End);
}

/// Emit Swiss Table map iteration: for each occupied slot, invoke body.
/// Body receives entry address in `entry_local`.
fn emit_map_foreach(
    map: Local, entry_local: Local, entry_stride: u32,
    body: &[Op], f: &mut Function, reg: &LayoutRegistry,
) {
    use wasm_encoder::Instruction::*;
    use super::layout;

    let cap_offset = reg.fixed_offset(layout::SWISS_MAP, layout::map::CAP);
    let tags_offset = reg.fixed_offset(layout::SWISS_MAP, layout::map::TAGS);

    // Scratch locals: cap=entry_local+1, eb=entry_local+2, i=entry_local+3
    // TODO: proper local allocation
    let cap_local = entry_local + 1;
    let eb_local = entry_local + 2;
    let i_local = entry_local + 3;

    // cap = map[CAP_OFFSET]
    f.instruction(&LocalGet(map));
    f.instruction(&I32Load(mem_arg(cap_offset, 2)));
    f.instruction(&LocalSet(cap_local));

    // eb (entry base) = map + TAGS_OFFSET + cap
    f.instruction(&LocalGet(map));
    f.instruction(&I32Const(tags_offset as i32));
    f.instruction(&I32Add);
    f.instruction(&LocalGet(cap_local));
    f.instruction(&I32Add);
    f.instruction(&LocalSet(eb_local));

    // i = 0
    f.instruction(&I32Const(0));
    f.instruction(&LocalSet(i_local));

    // block { loop {
    f.instruction(&Block(wasm_encoder::BlockType::Empty));
    f.instruction(&Loop(wasm_encoder::BlockType::Empty));

    // if i >= cap: br 1
    f.instruction(&LocalGet(i_local));
    f.instruction(&LocalGet(cap_local));
    f.instruction(&I32GeU);
    f.instruction(&BrIf(1));

    // tag = map[TAGS_OFFSET + i]; if tag == 0: skip
    f.instruction(&LocalGet(map));
    f.instruction(&I32Const(tags_offset as i32));
    f.instruction(&I32Add);
    f.instruction(&LocalGet(i_local));
    f.instruction(&I32Add);
    f.instruction(&I32Load8U(mem_arg(0, 0)));
    f.instruction(&I32Eqz);
    f.instruction(&If(wasm_encoder::BlockType::Empty));
    f.instruction(&LocalGet(i_local));
    f.instruction(&I32Const(1));
    f.instruction(&I32Add);
    f.instruction(&LocalSet(i_local));
    f.instruction(&Br(1)); // br(1) from inside if = loop start
    f.instruction(&End);

    // entry_local = eb + i * entry_stride
    f.instruction(&LocalGet(eb_local));
    f.instruction(&LocalGet(i_local));
    f.instruction(&I32Const(entry_stride as i32));
    f.instruction(&I32Mul);
    f.instruction(&I32Add);
    f.instruction(&LocalSet(entry_local));

    // body
    emit_ops(body, f, reg);

    // i++
    f.instruction(&LocalGet(i_local));
    f.instruction(&I32Const(1));
    f.instruction(&I32Add);
    f.instruction(&LocalSet(i_local));
    f.instruction(&Br(0));

    // } }
    f.instruction(&End);
    f.instruction(&End);
}
