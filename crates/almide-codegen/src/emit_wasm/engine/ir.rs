//! WasmIR — typed mid-level IR for WASM compilation.
//!
//! Stack-machine semantics matching WASM, but with typed memory access,
//! collection iteration, and Perceus RC as first-class operations.
//!
//! The IR intentionally mirrors WASM's structured control flow (Block, Loop, If)
//! so that emission is a mechanical 1:1 translation with no CFG reconstruction.

use super::layout::{LayoutId, FieldId};

/// WASM local variable index.
pub type Local = u32;

/// WASM function index.
pub type FuncIdx = u32;

/// WASM type signature index.
pub type SigIdx = u32;

/// Primitive WASM value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmTy {
    I32,
    I64,
    F32,
    F64,
}

impl WasmTy {
    /// Convert to a wasm-encoder ValType for section encoding.
    pub fn to_valtype(self) -> wasm_encoder::ValType {
        match self {
            WasmTy::I32 => wasm_encoder::ValType::I32,
            WasmTy::I64 => wasm_encoder::ValType::I64,
            WasmTy::F32 => wasm_encoder::ValType::F32,
            WasmTy::F64 => wasm_encoder::ValType::F64,
        }
    }
}

/// A constant value.
#[derive(Debug, Clone, Copy)]
pub enum Const {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

/// Binary operations.
#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    // i32
    I32Add, I32Sub, I32Mul, I32DivU, I32DivS, I32RemS,
    I32And, I32Or, I32Xor, I32Shl, I32ShrU, I32ShrS,
    I32Eq, I32Ne, I32LtS, I32LeS, I32GtS, I32GeS, I32LtU, I32LeU, I32GtU, I32GeU,
    // i64
    I64Add, I64Sub, I64Mul, I64DivS, I64RemS,
    I64Eq, I64Ne, I64LtS, I64LeS, I64GtS, I64GeS,
    // f64
    F64Add, F64Sub, F64Mul, F64Div,
    F64Eq, F64Ne, F64Lt, F64Le, F64Gt, F64Ge,
}

/// Unary operations.
#[derive(Debug, Clone, Copy)]
pub enum UnOp {
    I32Eqz, I64Eqz,
    I32WrapI64, I64ExtendI32S, I64ExtendI32U,
    F64ConvertI64S, I64TruncF64S,
    F64Sqrt, F64Abs, F64Neg, F64Ceil, F64Floor,
}

/// Load width for memory reads.
#[derive(Debug, Clone, Copy)]
pub enum LoadKind {
    I32,
    I64,
    F32,
    F64,
    U8,   // i32.load8_u
    I8S,  // i32.load8_s
    U16,  // i32.load16_u
}

impl LoadKind {
    pub const fn align_exp(self) -> u32 {
        match self {
            Self::I32 | Self::F32 => 2,
            Self::I64 | Self::F64 => 3,
            Self::U8 | Self::I8S => 0,
            Self::U16 => 1,
        }
    }
}

/// Store width for memory writes.
#[derive(Debug, Clone, Copy)]
pub enum StoreKind {
    I32,
    I64,
    F32,
    F64,
    I8,  // i32.store8
    I16, // i32.store16
}

impl StoreKind {
    pub const fn align_exp(self) -> u32 {
        match self {
            Self::I32 | Self::F32 => 2,
            Self::I64 | Self::F64 => 3,
            Self::I8 => 0,
            Self::I16 => 1,
        }
    }
}

/// A part of string interpolation.
#[derive(Debug, Clone)]
pub enum StringPart {
    Lit(u32),          // interned string offset
    Expr(WasmTy),      // value on stack, type determines to_string conversion
}

/// The core IR. Each node either pushes values onto the WASM stack,
/// performs side effects, or structures control flow.
#[derive(Debug, Clone)]
pub enum Op {
    // ── Stack operations ──
    LocalGet(Local),
    LocalSet(Local),
    LocalTee(Local),
    GlobalGet(u32),
    GlobalSet(u32),
    Const(Const),
    Drop,

    // ── Arithmetic ──
    BinOp(BinOp),
    UnOp(UnOp),

    // ── Typed memory access (layout-safe) ──
    /// Load a scalar field: push base, resolve offset, load.
    FieldLoad { layout: LayoutId, field: FieldId, kind: LoadKind },
    /// Store a scalar field: push base, push value, resolve offset, store.
    FieldStore { layout: LayoutId, field: FieldId, kind: StoreKind },
    /// Compute address of array element: base + field_offset + index * stride.
    /// Pushes the element address onto the stack.
    ElemAddr { layout: LayoutId, field: FieldId, stride: u32 },
    /// Raw load (for computed addresses already on stack).
    Load(LoadKind),
    /// Raw store.
    Store(StoreKind),
    /// Compute dynamic field address (e.g., SwissMap entries = tags_offset + cap).
    /// Pushes the address onto the stack. Base ptr must be on stack.
    DynFieldAddr { layout: LayoutId, field: FieldId },

    // ── Collection iteration ──
    /// Iterate over list elements. Body receives element address in `elem_local`.
    ListForEach { list: Local, elem_local: Local, elem_stride: u32, body: Vec<Op> },
    /// Iterate over occupied Swiss Table entries.
    /// Body receives entry address in `entry_local`.
    MapForEach { map: Local, entry_local: Local, entry_stride: u32, body: Vec<Op> },

    // ── Allocation ──
    /// Bump-allocate n bytes. Size expression is on stack. Pushes ptr.
    Alloc,
    /// Allocate a collection with header. Pushes ptr with len written.
    AllocCollection { layout: LayoutId, len: Local, elem_stride: u32 },

    // ── Perceus RC ──
    /// Increment reference count: ptr on stack.
    RcInc,
    /// Decrement reference count and free if zero. ptr on stack.
    /// Layout is needed to recursively dec children.
    RcDec { layout: LayoutId },
    /// COW check: if rc > 1, execute clone_body to produce a unique copy.
    CowCheck { ptr: Local, clone_body: Vec<Op> },

    // ── String operations ──
    /// Concatenate two strings (ptrs on stack). Pushes result ptr.
    StringConcat,
    /// String interpolation from parts. Each Lit is an interned offset,
    /// each Expr expects its value on stack before this op.
    StringInterp { parts: Vec<StringPart> },

    // ── Deep equality ──
    /// Compare two values of the same type. Both on stack. Pushes i32 (0/1).
    DeepEq { wasm_ty: WasmTy },

    // ── Control flow ──
    Block(Vec<Op>),
    Loop(Vec<Op>),
    /// `if` producing a single value of type `ty` (both branches push one `ty`).
    If { ty: WasmTy, then: Vec<Op>, else_: Vec<Op> },
    IfVoid { then: Vec<Op>, else_: Vec<Op> },
    Br(u32),
    BrIf(u32),
    Return,
    Unreachable,

    // ── Calls ──
    Call { idx: FuncIdx, pops: u8, pushes: u8 },
    CallIndirect { sig: SigIdx, pops: u8, pushes: u8 },

    // ── Memory ──
    MemoryCopy,
    MemorySize,
    MemoryGrow,

    // ── Sequence ──
    /// A flat sequence of ops (for grouping without adding a WASM block).
    Seq(Vec<Op>),
}

// ── Stack-effect verification ─────────────────────────────────────────

/// Stack effect: how many values an op consumes and produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackEffect {
    /// Normal op: pops N values, pushes M values.
    Normal { pops: u8, pushes: u8 },
    /// Divergent: unreachable after this (br, return, unreachable).
    Divergent,
    /// Compound: must verify children recursively (Block, Loop, If, Seq).
    Compound,
}

impl Op {
    /// Returns the stack effect of this op.
    pub fn stack_effect(&self) -> StackEffect {
        use StackEffect::*;
        match self {
            // ── Stack: push 1 ──
            Op::LocalGet(_) | Op::GlobalGet(_) | Op::Const(_) | Op::MemorySize
                => Normal { pops: 0, pushes: 1 },

            // ── Stack: pop 1 ──
            Op::LocalSet(_) | Op::GlobalSet(_) | Op::Drop
                => Normal { pops: 1, pushes: 0 },

            // ── Stack: pop 1, push 1 ──
            Op::LocalTee(_) | Op::UnOp(_) | Op::Load(_) | Op::MemoryGrow
                => Normal { pops: 1, pushes: 1 },

            // ── Memory: pop 2, push 0 ──
            Op::Store(_) | Op::FieldStore { .. }
                => Normal { pops: 2, pushes: 0 },

            // ── Arithmetic: pop 2, push 1 ──
            Op::BinOp(_)
                => Normal { pops: 2, pushes: 1 },

            // ── Layout-safe memory: pop 1 (base), push 1 (value/addr) ──
            Op::FieldLoad { .. } | Op::DynFieldAddr { .. }
                => Normal { pops: 1, pushes: 1 },

            // ── ElemAddr: pop 2 (base + index), push 1 (addr) ──
            Op::ElemAddr { .. }
                => Normal { pops: 2, pushes: 1 },

            // ── Iteration: no net stack effect (uses locals) ──
            Op::ListForEach { .. } | Op::MapForEach { .. }
                => Compound,

            // ── Abstract allocation: pop 1 (size), push 1 (ptr) ──
            Op::Alloc => Normal { pops: 1, pushes: 1 },
            Op::AllocCollection { .. } => Normal { pops: 0, pushes: 1 },

            // ── Perceus RC: pop 1 (ptr) ──
            Op::RcInc => Normal { pops: 1, pushes: 0 },
            Op::RcDec { .. } => Normal { pops: 1, pushes: 0 },
            Op::CowCheck { .. } => Compound,

            // ── String ops ──
            Op::StringConcat => Normal { pops: 2, pushes: 1 },
            Op::StringInterp { parts } => {
                let expr_count = parts.iter()
                    .filter(|p| matches!(p, StringPart::Expr(_)))
                    .count() as u8;
                Normal { pops: expr_count, pushes: 1 }
            }

            // ── Deep equality: pop 2, push 1 (i32 bool) ──
            Op::DeepEq { .. } => Normal { pops: 2, pushes: 1 },

            // ── Control flow ──
            Op::Block(_) | Op::Loop(_) | Op::Seq(_) => Compound,
            Op::If { .. } => Compound,   // pops 1 (cond) + body effect
            Op::IfVoid { .. } => Compound, // pops 1 (cond)
            Op::BrIf(_) => Normal { pops: 1, pushes: 0 },
            Op::Br(_) | Op::Return | Op::Unreachable => Divergent,

            // ── Calls ──
            Op::Call { pops, pushes, .. } | Op::CallIndirect { pops, pushes, .. }
                => Normal { pops: *pops, pushes: *pushes },

            // ── Memory bulk ──
            Op::MemoryCopy => Normal { pops: 3, pushes: 0 },
        }
    }
}

/// Verify stack balance of a WasmIR function.
///
/// Returns Ok(()) if every block and the function body have correct
/// stack balance. Returns Err with diagnostic on first violation.
pub fn verify_func_stack(func: &WasmFunc) -> Result<(), String> {
    let expected = func.results.len() as i32;
    verify_ops(&func.body, expected, &func.name)
}

fn verify_ops(ops: &[Op], expected_net: i32, ctx: &str) -> Result<(), String> {
    let mut depth: i32 = 0;

    for (i, op) in ops.iter().enumerate() {
        match op.stack_effect() {
            StackEffect::Normal { pops, pushes } => {
                depth -= pops as i32;
                if depth < 0 {
                    return Err(format!(
                        "[StackVerify] {}: stack underflow at op #{} ({:?}), depth={}",
                        ctx, i, op_name(op), depth,
                    ));
                }
                depth += pushes as i32;
            }
            StackEffect::Divergent => {
                // After divergent op, remaining ops in this sequence are unreachable.
                // Stack balance is satisfied (divergent code can't violate it).
                return Ok(());
            }
            StackEffect::Compound => {
                verify_compound(op, &mut depth, ctx, i)?;
            }
        }
    }

    if depth != expected_net {
        return Err(format!(
            "[StackVerify] {}: stack imbalance — expected net {}, got {}",
            ctx, expected_net, depth,
        ));
    }
    Ok(())
}

fn verify_compound(op: &Op, depth: &mut i32, ctx: &str, idx: usize) -> Result<(), String> {
    match op {
        Op::Block(body) | Op::Loop(body) | Op::Seq(body) => {
            // Block/Loop/Seq: body must have net 0 effect (no result type in current usage)
            verify_ops(body, 0, ctx)?;
        }
        Op::If { then, else_, .. } => {
            // Pops condition (i32)
            *depth -= 1;
            if *depth < 0 {
                return Err(format!(
                    "[StackVerify] {}: stack underflow at If condition, op #{}", ctx, idx,
                ));
            }
            // Both branches must produce exactly 1 value (i32 result type)
            verify_ops(then, 1, ctx)?;
            verify_ops(else_, 1, ctx)?;
            *depth += 1; // result
        }
        Op::IfVoid { then, else_ } => {
            *depth -= 1;
            if *depth < 0 {
                return Err(format!(
                    "[StackVerify] {}: stack underflow at IfVoid condition, op #{}", ctx, idx,
                ));
            }
            verify_ops(then, 0, ctx)?;
            if !else_.is_empty() {
                verify_ops(else_, 0, ctx)?;
            }
        }
        Op::ListForEach { body, .. } => {
            // Body runs per element, must have net 0 effect
            verify_ops(body, 0, ctx)?;
        }
        Op::MapForEach { body, .. } => {
            verify_ops(body, 0, ctx)?;
        }
        Op::CowCheck { clone_body, .. } => {
            // COW check: ptr on stack, if rc>1 run clone_body which must push 1 (new ptr)
            // Net effect on outer stack: pop 1 (ptr), push 1 (ptr or clone)
            *depth -= 1;
            if *depth < 0 {
                return Err(format!(
                    "[StackVerify] {}: stack underflow at CowCheck, op #{}", ctx, idx,
                ));
            }
            verify_ops(clone_body, 1, ctx)?;
            *depth += 1;
        }
        _ => {} // non-compound ops handled by caller
    }
    Ok(())
}

fn op_name(op: &Op) -> &'static str {
    match op {
        Op::LocalGet(_) => "LocalGet", Op::LocalSet(_) => "LocalSet",
        Op::LocalTee(_) => "LocalTee", Op::GlobalGet(_) => "GlobalGet",
        Op::GlobalSet(_) => "GlobalSet", Op::Const(_) => "Const",
        Op::Drop => "Drop", Op::BinOp(_) => "BinOp", Op::UnOp(_) => "UnOp",
        Op::FieldLoad { .. } => "FieldLoad", Op::FieldStore { .. } => "FieldStore",
        Op::ElemAddr { .. } => "ElemAddr", Op::Load(_) => "Load", Op::Store(_) => "Store",
        Op::DynFieldAddr { .. } => "DynFieldAddr",
        Op::ListForEach { .. } => "ListForEach", Op::MapForEach { .. } => "MapForEach",
        Op::Alloc => "Alloc", Op::AllocCollection { .. } => "AllocCollection",
        Op::RcInc => "RcInc", Op::RcDec { .. } => "RcDec",
        Op::CowCheck { .. } => "CowCheck",
        Op::StringConcat => "StringConcat", Op::StringInterp { .. } => "StringInterp",
        Op::DeepEq { .. } => "DeepEq",
        Op::Block(_) => "Block", Op::Loop(_) => "Loop",
        Op::If { .. } => "If", Op::IfVoid { .. } => "IfVoid",
        Op::Br(_) => "Br", Op::BrIf(_) => "BrIf",
        Op::Return => "Return", Op::Unreachable => "Unreachable",
        Op::Call { .. } => "Call", Op::CallIndirect { .. } => "CallIndirect",
        Op::MemoryCopy => "MemoryCopy", Op::MemorySize => "MemorySize",
        Op::MemoryGrow => "MemoryGrow", Op::Seq(_) => "Seq",
    }
}

/// A compiled WASM function in WasmIR form.
#[derive(Debug, Clone)]
pub struct WasmFunc {
    pub name: String,
    pub params: Vec<WasmTy>,
    pub results: Vec<WasmTy>,
    pub locals: Vec<WasmTy>,
    pub body: Vec<Op>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn void_func(body: Vec<Op>) -> WasmFunc {
        WasmFunc { name: "test".into(), params: vec![], results: vec![], locals: vec![WasmTy::I32; 4], body }
    }

    fn i32_func(body: Vec<Op>) -> WasmFunc {
        WasmFunc { name: "test".into(), params: vec![], results: vec![WasmTy::I32], locals: vec![WasmTy::I32; 4], body }
    }

    #[test]
    fn empty_void_function_is_valid() {
        assert!(verify_func_stack(&void_func(vec![])).is_ok());
    }

    #[test]
    fn const_return_is_valid() {
        let f = i32_func(vec![Op::Const(Const::I32(42))]);
        assert!(verify_func_stack(&f).is_ok());
    }

    #[test]
    fn missing_return_value_detected() {
        let f = i32_func(vec![]); // needs 1 value but body is empty
        assert!(verify_func_stack(&f).is_err());
    }

    #[test]
    fn leftover_value_detected() {
        let f = void_func(vec![Op::Const(Const::I32(1))]); // void fn but pushes 1
        assert!(verify_func_stack(&f).is_err());
    }

    #[test]
    fn stack_underflow_detected() {
        let f = void_func(vec![Op::Drop]); // nothing to drop
        assert!(verify_func_stack(&f).is_err());
    }

    #[test]
    fn binop_balance() {
        // Push 2, binop produces 1, total net = 1
        let f = i32_func(vec![
            Op::Const(Const::I32(1)),
            Op::Const(Const::I32(2)),
            Op::BinOp(BinOp::I32Add),
        ]);
        assert!(verify_func_stack(&f).is_ok());
    }

    #[test]
    fn local_get_set_balance() {
        let f = void_func(vec![
            Op::Const(Const::I32(1)),
            Op::LocalSet(0),
        ]);
        assert!(verify_func_stack(&f).is_ok());
    }

    #[test]
    fn if_void_balance() {
        let f = void_func(vec![
            Op::Const(Const::I32(1)), // condition
            Op::IfVoid { then: vec![], else_: vec![] },
        ]);
        assert!(verify_func_stack(&f).is_ok());
    }

    #[test]
    fn if_result_balance() {
        let f = i32_func(vec![
            Op::Const(Const::I32(1)), // condition
            Op::If {
                ty: WasmTy::I32,
                then: vec![Op::Const(Const::I32(10))],
                else_: vec![Op::Const(Const::I32(20))],
            },
        ]);
        assert!(verify_func_stack(&f).is_ok());
    }

    #[test]
    fn call_with_correct_effects() {
        // Call pops 2, pushes 1 → net -1
        let f = i32_func(vec![
            Op::Const(Const::I32(1)),
            Op::Const(Const::I32(2)),
            Op::Call { idx: 0, pops: 2, pushes: 1 },
        ]);
        assert!(verify_func_stack(&f).is_ok());
    }

    #[test]
    fn divergent_return_ok() {
        // After Return, stack state doesn't matter
        let f = void_func(vec![
            Op::Return,
        ]);
        assert!(verify_func_stack(&f).is_ok());
    }

    #[test]
    fn seq_flattened() {
        let f = void_func(vec![
            Op::Seq(vec![
                Op::Const(Const::I32(1)),
                Op::Drop,
            ]),
        ]);
        assert!(verify_func_stack(&f).is_ok());
    }
}
