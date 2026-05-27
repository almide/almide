//! Almide WASM Engine — type-aware compilation pipeline.
//!
//! Replaces the hand-written assembler approach with a layered architecture:
//!
//!   AlmideIR → [lower] → WasmIR → [optimize] → WasmIR → [emit] → WASM binary
//!
//! Key properties:
//! - **Layout-safe**: All memory offsets resolved through LayoutRegistry
//! - **Perceus-native**: RC ops are first-class IR nodes, optimizable
//! - **Swiss Table-aware**: Map iteration is a single IR op, not 30 lines

pub mod layout;
pub mod ir;
pub mod emit;
