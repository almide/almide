//! Mut param lowering for WASM: rewrite `mut` parameter functions to return
//! mutated values, and rewrite call sites to assign them back.
//!
//! WASM has no pass-by-reference — this is the C-132 move-mode write-back
//! convention. The rewrite itself is target-independent and lives in
//! `almide_ir::mut_param` (ONE truth): the v1 MIR pipeline applies the same
//! transform pre-lowering on both its legs. This pass is the v0 wasm
//! pipeline's invocation of it.

use crate::pass::{NanoPass, PassResult, Target};
use almide_ir::IrProgram;

#[derive(Debug)]
pub struct MutParamLoweringPass;

impl NanoPass for MutParamLoweringPass {
    fn name(&self) -> &str {
        "MutParamLowering"
    }
    fn targets(&self) -> Option<Vec<Target>> {
        Some(vec![Target::Wasm])
    }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let changed = almide_ir::mut_param::lower_mut_params_move_mode(&mut program);
        PassResult { program, changed }
    }
}
