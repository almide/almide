//! The rc-glue helpers' ABI and bodies (#2010): the signature assembly promises
//! for each `Helper`, its result type, and which emitter builds its body. Split
//! from runtime_alloc.rs, which keeps the emitters themselves.

use wasm_encoder::{Function, ValType};

use super::*;
use crate::work::FnWork;

/// The signatures assembly promises for the rc-glue helpers (#2010).
pub(crate) fn helper_params(h: &Helper) -> Option<Vec<ValType>> {
    match h {
        Helper::IncEntries { .. } | Helper::DeepDropPush { .. } => return Some(vec![ValType::I32, ValType::I32]),
        Helper::DeepDropDrain { .. } => return Some(Vec::new()),
        _ => {}
    }
    matches!(
        h,
        Helper::DropList { .. }
            | Helper::IncElems
            | Helper::CopyElems { .. }
            | Helper::CowElems { .. }
            | Helper::DropShape { .. }
            | Helper::IncShape { .. }
            | Helper::DropMapSpine { .. }
            | Helper::DropEntries { .. }
            | Helper::CopyEntries { .. }
            | Helper::DropEnv { .. }
            | Helper::DropFn { .. }
            | Helper::DropCell { .. }
            | Helper::DropHttpCall
    )
    .then(|| vec![ValType::I32])
}

/// The result type of a helper: the drops and the credit walk return
/// nothing; the copy variants hand the block back; everything else i32
/// unless assembly says f64.
pub(crate) fn helper_result(h: &Helper) -> Option<ValType> {
    match h {
        Helper::DropList { .. }
        | Helper::IncElems
        | Helper::DropShape { .. }
        | Helper::IncShape { .. }
        | Helper::DropMapSpine { .. }
        | Helper::DropEntries { .. }
        | Helper::IncEntries { .. }
        | Helper::DropEnv { .. }
        | Helper::DropFn { .. }
        | Helper::DropCell { .. }
        | Helper::DeepDropPush { .. }
        | Helper::DeepDropDrain { .. }
        | Helper::DropHttpCall => None,
        _ => Some(ValType::I32),
    }
}

/// The rc-glue helper bodies; a `DropShape` body was built at registration
/// (`work.drop_bodies`) — a missing one is a loud stub.
pub(crate) fn helper_body(h: &Helper, work: &FnWork) -> Option<Function> {
    Some(match h {
        Helper::DropList { elem_dec } => emit_drop_list(*elem_dec),
        Helper::IncElems => emit_inc_elems(),
        Helper::CopyElems { inc_elems } => emit_copy_elems(*inc_elems),
        Helper::DropMapSpine { side_clear } => emit_drop_map_spine(*side_clear),
        Helper::DropEntries { stride, slots, side_clear } => emit_drop_entries(*stride, *slots, *side_clear),
        Helper::IncEntries { stride, slots } => emit_inc_entries(*stride, *slots),
        Helper::CopyEntries { inc_entries } => emit_copy_entries(*inc_entries),
        Helper::DropEnv { slots } => emit_drop_shape(slots, None),
        Helper::DropFn { ti } => emit_drop_fn(*ti),
        Helper::DropHttpCall => emit_drop_http_call(),
        Helper::DeepDropPush { .. } | Helper::DeepDropDrain { .. } => crate::deep_walk::emit_deep_drop_shared(h),
        Helper::DropCell { elem_dec } => emit_drop_shape(&elem_dec.map(|d| (0, d)).into_iter().collect::<Vec<_>>(), None),
        Helper::DropShape { .. } | Helper::IncShape { .. } => {
            let mut bodies = work.drop_bodies.borrow_mut();
            let built = bodies.iter_mut().find(|(k, _)| k == h).and_then(|(_, f)| f.take());
            built.unwrap_or_else(|| {
                let mut f = Function::new([]);
                f.instructions().unreachable().end();
                f
            })
        }
        _ => return None,
    })
}
