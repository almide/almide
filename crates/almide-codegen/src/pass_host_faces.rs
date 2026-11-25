//! HostFacePass: how each param crosses a package fn's host face.
//!
//! Target: Rust only. Runs at the end, on the final param modes.
//!
//! A package's pub fn that native code calls back keeps the host's std
//! spelling at its face (`walker/extern_c.rs`, `render_callback_face`): a list
//! the host passes or receives is a std `Vec`, copied at the boundary into the
//! shared-block list the fn's body uses. How a param is copied depends on its
//! final borrow mode — an owned value is converted, a `&mut` one is copied in
//! and back, a slice's elements are copied into a local list — so the choice is
//! made here, after `BorrowLowering` fixed the modes, and published as
//! `host_face_params`; the walker spells the conversion it names.

use std::collections::HashMap;
use almide_ir::annotations::HostFaceParam;
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId as C};
use super::pass::{NanoPass, PassResult, Target};

#[derive(Debug)]
pub struct HostFacePass;

impl NanoPass for HostFacePass {
    fn name(&self) -> &str { "HostFace" }
    fn targets(&self) -> Option<Vec<Target>> { Some(vec![Target::Rust]) }
    /// Reads every param's FINAL borrow mode, so every pass declared before
    /// it stays before it, the shuffle included.
    fn barrier(&self) -> bool { true }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut out: HashMap<VarId, HostFaceParam> = HashMap::new();
        let module_fns = program.modules.iter().flat_map(|m| m.functions.iter());
        for func in program.functions.iter().chain(module_fns) {
            for p in &func.params {
                out.insert(p.var, face_param(p));
            }
        }
        program.codegen_annotations.host_face_params = out;
        PassResult { program, changed: false }
    }
}

/// The conversion `p` takes at a host face.
fn face_param(p: &IrParam) -> HostFaceParam {
    if matches!(p.ty, Ty::Fn { .. }) || matches!(p.borrow, ParamBorrow::RefStr) || !host_spelling_differs(&p.ty) {
        return HostFaceParam::Direct;
    }
    match (p.borrow, &p.ty) {
        (ParamBorrow::Own, Ty::Applied(C::List, a)) if !host_spelling_differs(&a[0]) => HostFaceParam::OwnList,
        (ParamBorrow::Own, _) => HostFaceParam::OwnCopy,
        (ParamBorrow::RefMut, _) => HostFaceParam::MutCopyBack,
        (ParamBorrow::RefSlice, Ty::Applied(_, a)) if !a.is_empty() => HostFaceParam::SliceCopy,
        _ => HostFaceParam::RefCopy,
    }
}

/// Whether the host spells `ty` differently from generated code: a list (and
/// `Bytes`) is a std `Vec` at the host, so is any type holding one.
pub(crate) fn host_spelling_differs(ty: &Ty) -> bool {
    match ty {
        Ty::Bytes | Ty::Applied(C::List, _) => true,
        Ty::Applied(C::Set | C::Option | C::Result | C::Map, args) => args.iter().any(host_spelling_differs),
        Ty::Tuple(elems) => elems.iter().any(host_spelling_differs),
        _ => false,
    }
}
