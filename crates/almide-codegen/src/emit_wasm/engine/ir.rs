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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmTy {
    I32,
    I64,
    F32,
    F64,
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
    If { then: Vec<Op>, else_: Vec<Op> },
    IfVoid { then: Vec<Op>, else_: Vec<Op> },
    Br(u32),
    BrIf(u32),
    Return,
    Unreachable,

    // ── Calls ──
    Call(FuncIdx),
    CallIndirect { sig: SigIdx },

    // ── Memory ──
    MemoryCopy,
    MemorySize,
    MemoryGrow,

    // ── Sequence ──
    /// A flat sequence of ops (for grouping without adding a WASM block).
    Seq(Vec<Op>),
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
