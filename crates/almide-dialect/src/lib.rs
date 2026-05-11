//! Almide MLIR dialect schema — pure-Rust, FFI-free.
//!
//! Models MLIR's Region/Block/Operation hierarchy as Rust types.
//! When the real MLIR backend arrives (melior FFI), these types
//! become the source-of-truth schema: the converter produces them,
//! and a thin FFI layer serializes them into MLIR C API calls.
//!
//! Design:
//!   IrProgram → lower::lower_program() → Module (this crate)
//!   Module contains FuncOp, each containing Blocks of Operations.

pub mod types;
pub mod ops;
pub mod lower;
pub mod verify;

use almide_base::intern::Sym;

/// A Module is the top-level container (corresponds to `mlir::ModuleOp`).
#[derive(Debug, Clone)]
pub struct Module {
    pub name: Option<Sym>,
    pub functions: Vec<ops::FuncOp>,
    pub type_decls: Vec<ops::TypeDeclOp>,
    pub globals: Vec<ops::GlobalOp>,
}

/// SSA value reference — every computed value gets a unique ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueId(pub u32);

/// Block label for control flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

/// A Block contains a sequence of Operations and block arguments.
#[derive(Debug, Clone)]
pub struct Block {
    pub id: BlockId,
    pub args: Vec<(ValueId, types::DialectType)>,
    pub ops: Vec<ops::Operation>,
    pub terminator: ops::Terminator,
}

/// Generator for fresh ValueIds and BlockIds.
#[derive(Debug, Default)]
pub struct IdGen {
    next_value: u32,
    next_block: u32,
}

impl IdGen {
    pub fn fresh_value(&mut self) -> ValueId {
        let id = ValueId(self.next_value);
        self.next_value += 1;
        id
    }

    pub fn fresh_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        id
    }
}
