//! WasmIR → wasm-encoder emission.
//!
//! Emits `Op` trees (from ir.rs) into wasm-encoder Functions.
//! Used by the IR-based pipeline (future optimization passes).
//!
//! For direct codegen, prefer `WasmBuilder` (builder.rs) which emits
//! layout-safe instructions without an intermediate IR.

use wasm_encoder::Function;
use super::layout::{LayoutRegistry, FieldOffset};
use super::ir::*;

/// Emit a sequence of WasmIR ops.
pub fn emit_ops(ops: &[Op], f: &mut Function, reg: &LayoutRegistry) {
    for op in ops {
        emit_op(op, f, reg);
    }
}

/// Emit a single WasmIR op.
pub fn emit_op(op: &Op, f: &mut Function, reg: &LayoutRegistry) {
    use wasm_encoder::Instruction::*;
    match op {
        Op::LocalGet(l) => { f.instruction(&LocalGet(*l)); }
        Op::LocalSet(l) => { f.instruction(&LocalSet(*l)); }
        Op::LocalTee(l) => { f.instruction(&LocalTee(*l)); }
        Op::GlobalGet(g) => { f.instruction(&GlobalGet(*g)); }
        Op::GlobalSet(g) => { f.instruction(&GlobalSet(*g)); }
        Op::Const(c) => match c {
            Const::I32(v) => { f.instruction(&I32Const(*v)); }
            Const::I64(v) => { f.instruction(&I64Const(*v)); }
            Const::F32(v) => { f.instruction(&F32Const(*v)); }
            Const::F64(v) => { f.instruction(&F64Const(*v)); }
        },
        Op::Drop => { f.instruction(&Drop); }

        Op::BinOp(b) => { emit_binop(b, f); }
        Op::UnOp(u) => { emit_unop(u, f); }

        Op::FieldLoad { layout, field, kind } => {
            let offset = reg.fixed_offset(*layout, *field);
            emit_load(offset, *kind, f);
        }
        Op::FieldStore { layout, field, kind } => {
            let offset = reg.fixed_offset(*layout, *field);
            emit_store(offset, *kind, f);
        }
        Op::ElemAddr { layout, field, stride } => {
            let offset = reg.fixed_offset(*layout, *field);
            f.instruction(&I32Const(*stride as i32));
            f.instruction(&I32Mul);
            f.instruction(&I32Add);
            if offset != 0 {
                f.instruction(&I32Const(offset as i32));
                f.instruction(&I32Add);
            }
        }
        Op::DynFieldAddr { layout, field } => {
            let mf = reg.field(*layout, *field);
            match &mf.offset {
                FieldOffset::Fixed(n) => {
                    f.instruction(&I32Const(*n as i32));
                    f.instruction(&I32Add);
                }
                FieldOffset::AfterDynamic { .. } => {
                    panic!("DynFieldAddr(AfterDynamic) requires a temp local — use WasmBuilder::dyn_field_addr instead");
                }
            }
        }
        Op::Load(kind) => emit_load(0, *kind, f),
        Op::Store(kind) => emit_store(0, *kind, f),

        Op::ListForEach { list, elem_local, elem_stride, body } => {
            emit_list_foreach(*list, *elem_local, *elem_stride, body, f, reg);
        }
        Op::MapForEach { map, entry_local, entry_stride, body } => {
            emit_map_foreach(*map, *entry_local, *entry_stride, body, f, reg);
        }

        // Abstract ops — resolve at lowering or use WasmBuilder.
        Op::Alloc => panic!("Op::Alloc: use WasmBuilder::alloc or resolve to Op::Call"),
        Op::AllocCollection { .. } => panic!("Op::AllocCollection: use WasmBuilder::alloc_collection"),
        Op::RcInc => panic!("Op::RcInc: use WasmBuilder::rc_inc or resolve to Op::Call"),
        Op::RcDec { .. } => panic!("Op::RcDec: use WasmBuilder::rc_dec or resolve to Op::Call"),
        Op::CowCheck { .. } => panic!("Op::CowCheck: use WasmBuilder::cow_check"),
        Op::StringConcat => panic!("Op::StringConcat: resolve to Op::Call(concat_str)"),
        Op::StringInterp { .. } => panic!("Op::StringInterp: use WasmBuilder for string interpolation"),
        Op::DeepEq { wasm_ty } => match wasm_ty {
            WasmTy::I32 => { f.instruction(&I32Eq); }
            WasmTy::I64 => { f.instruction(&I64Eq); }
            WasmTy::F64 => { f.instruction(&F64Eq); }
            WasmTy::F32 => { f.instruction(&F32Eq); }
        },

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
        Op::Br(d) => { f.instruction(&Br(*d)); }
        Op::BrIf(d) => { f.instruction(&BrIf(*d)); }
        Op::Return => { f.instruction(&Return); }
        Op::Unreachable => { f.instruction(&Unreachable); }

        Op::Call(idx) => { f.instruction(&Call(*idx)); }
        Op::CallIndirect { sig } => {
            f.instruction(&CallIndirect { type_index: *sig, table_index: 0 });
        }

        Op::MemoryCopy => { f.instruction(&MemoryCopy { src_mem: 0, dst_mem: 0 }); }
        Op::MemorySize => { f.instruction(&MemorySize(0)); }
        Op::MemoryGrow => { f.instruction(&MemoryGrow(0)); }

        Op::Seq(ops) => emit_ops(ops, f, reg),
    }
}

// ── Helpers ──

fn ma(offset: u32, kind_align: u32) -> wasm_encoder::MemArg {
    wasm_encoder::MemArg { offset: offset as u64, align: kind_align, memory_index: 0 }
}

fn emit_load(offset: u32, kind: LoadKind, f: &mut Function) {
    use wasm_encoder::Instruction::*;
    let a = ma(offset, kind.align_exp());
    match kind {
        LoadKind::I32 => { f.instruction(&I32Load(a)); }
        LoadKind::I64 => { f.instruction(&I64Load(a)); }
        LoadKind::F32 => { f.instruction(&F32Load(a)); }
        LoadKind::F64 => { f.instruction(&F64Load(a)); }
        LoadKind::U8 => { f.instruction(&I32Load8U(a)); }
        LoadKind::I8S => { f.instruction(&I32Load8S(a)); }
        LoadKind::U16 => { f.instruction(&I32Load16U(a)); }
    }
}

fn emit_store(offset: u32, kind: StoreKind, f: &mut Function) {
    use wasm_encoder::Instruction::*;
    let a = ma(offset, kind.align_exp());
    match kind {
        StoreKind::I32 => { f.instruction(&I32Store(a)); }
        StoreKind::I64 => { f.instruction(&I64Store(a)); }
        StoreKind::F32 => { f.instruction(&F32Store(a)); }
        StoreKind::F64 => { f.instruction(&F64Store(a)); }
        StoreKind::I8 => { f.instruction(&I32Store8(a)); }
        StoreKind::I16 => { f.instruction(&I32Store16(a)); }
    }
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

fn emit_list_foreach(
    list: Local, elem: Local, stride: u32,
    body: &[Op], f: &mut Function, reg: &LayoutRegistry,
) {
    use wasm_encoder::Instruction::*;
    use super::layout;
    let len_off = reg.fixed_offset(layout::LIST, layout::list::LEN);
    let len_align = reg.field(layout::LIST, layout::list::LEN).ty.align_exp();
    let data_off = reg.fixed_offset(layout::LIST, layout::list::DATA);
    let idx = elem + 1;

    f.instruction(&I32Const(0));
    f.instruction(&LocalSet(idx));
    f.instruction(&Block(wasm_encoder::BlockType::Empty));
    f.instruction(&Loop(wasm_encoder::BlockType::Empty));
    f.instruction(&LocalGet(idx));
    f.instruction(&LocalGet(list));
    f.instruction(&I32Load(ma(len_off, len_align)));
    f.instruction(&I32GeU);
    f.instruction(&BrIf(1));
    f.instruction(&LocalGet(list));
    f.instruction(&I32Const(data_off as i32));
    f.instruction(&I32Add);
    f.instruction(&LocalGet(idx));
    f.instruction(&I32Const(stride as i32));
    f.instruction(&I32Mul);
    f.instruction(&I32Add);
    f.instruction(&LocalSet(elem));
    emit_ops(body, f, reg);
    f.instruction(&LocalGet(idx));
    f.instruction(&I32Const(1));
    f.instruction(&I32Add);
    f.instruction(&LocalSet(idx));
    f.instruction(&Br(0));
    f.instruction(&End);
    f.instruction(&End);
}

fn emit_map_foreach(
    map: Local, entry: Local, stride: u32,
    body: &[Op], f: &mut Function, reg: &LayoutRegistry,
) {
    use wasm_encoder::Instruction::*;
    use super::layout;
    let cap_off = reg.fixed_offset(layout::SWISS_MAP, layout::map::CAP);
    let cap_align = reg.field(layout::SWISS_MAP, layout::map::CAP).ty.align_exp();
    let tags_off = reg.fixed_offset(layout::SWISS_MAP, layout::map::TAGS);
    let tag_align = reg.field(layout::SWISS_MAP, layout::map::TAGS).ty.align_exp();
    let cap_l = entry + 1;
    let eb_l = entry + 2;
    let idx = entry + 3;

    f.instruction(&LocalGet(map));
    f.instruction(&I32Load(ma(cap_off, cap_align)));
    f.instruction(&LocalSet(cap_l));
    f.instruction(&LocalGet(map));
    f.instruction(&I32Const(tags_off as i32));
    f.instruction(&I32Add);
    f.instruction(&LocalGet(cap_l));
    f.instruction(&I32Add);
    f.instruction(&LocalSet(eb_l));
    f.instruction(&I32Const(0));
    f.instruction(&LocalSet(idx));
    f.instruction(&Block(wasm_encoder::BlockType::Empty));
    f.instruction(&Loop(wasm_encoder::BlockType::Empty));
    f.instruction(&LocalGet(idx));
    f.instruction(&LocalGet(cap_l));
    f.instruction(&I32GeU);
    f.instruction(&BrIf(1));
    f.instruction(&LocalGet(map));
    f.instruction(&I32Const(tags_off as i32));
    f.instruction(&I32Add);
    f.instruction(&LocalGet(idx));
    f.instruction(&I32Add);
    f.instruction(&I32Load8U(ma(0, tag_align)));
    f.instruction(&I32Eqz);
    f.instruction(&If(wasm_encoder::BlockType::Empty));
    f.instruction(&LocalGet(idx));
    f.instruction(&I32Const(1));
    f.instruction(&I32Add);
    f.instruction(&LocalSet(idx));
    f.instruction(&Br(1));
    f.instruction(&End);
    f.instruction(&LocalGet(eb_l));
    f.instruction(&LocalGet(idx));
    f.instruction(&I32Const(stride as i32));
    f.instruction(&I32Mul);
    f.instruction(&I32Add);
    f.instruction(&LocalSet(entry));
    emit_ops(body, f, reg);
    f.instruction(&LocalGet(idx));
    f.instruction(&I32Const(1));
    f.instruction(&I32Add);
    f.instruction(&LocalSet(idx));
    f.instruction(&Br(0));
    f.instruction(&End);
    f.instruction(&End);
}
