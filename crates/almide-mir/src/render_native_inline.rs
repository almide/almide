//! Small straight-line functions are inlined at their call sites (#4661).
//!
//! A derived `T.decode` hands every field's `Result` through a helper that
//! returns it as it is unless it is an `Err` (`T.__erratw_string`). The
//! helper's error arm builds a message, and with it the body is past what
//! the optimizer inlines on its own: every field read paid a call, and its
//! `Result` a move in and a move out — 10 ns of a 92 ns decode, where the
//! standard codegen spells the same step as a `map_err` closure that only
//! the error path runs.
//!
//! The rule names no function: a body of at most [`SMALL`] ops with no loop
//! and no call to itself is marked `#[inline(always)]`. `main` is not (it is
//! called once), and a function with a narrow-integer signature already
//! renders as an inlined lane behind its boundary (`narrow_abi_wrapper`).
//! The attribute goes ahead of the finished text, so a keyed function's body
//! (`NAME__k`, `render_native_keyed`) takes it and its wrapper stays plain.

use crate::render_native::NativeSigs;
use crate::{MirFunction, Op};

/// The most ops a body may hold and still be inlined everywhere.
const SMALL: usize = 32;

/// The attribute line `func` renders under, or nothing.
pub(crate) fn attr(func: &MirFunction, sigs: &NativeSigs) -> &'static str {
    let straight = !func.ops.iter().any(|op| matches!(op, Op::LoopStart));
    let calls_itself = func.ops.iter().any(|op| matches!(op, Op::CallFn { name, .. } if *name == func.name));
    let narrow = sigs.get(func.name.as_str()).is_some_and(|(ps, ret)| ps.iter().chain(ret.iter()).any(|k| k.narrow_rust().is_some()));
    if func.name != "main" && func.ops.len() <= SMALL && straight && !calls_itself && !narrow {
        "#[inline(always)]\n"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ValueId;

    fn func(name: &str, ops: Vec<Op>) -> MirFunction {
        MirFunction { name: name.into(), ops, ..Default::default() }
    }

    #[test]
    fn only_a_small_straight_non_recursive_body_is_inlined() {
        let sigs = NativeSigs::default();
        let small = vec![Op::ConstInt { dst: ValueId(1), value: 1 }];
        assert_eq!(attr(&func("helper", small.clone()), &sigs), "#[inline(always)]\n");
        assert_eq!(attr(&func("main", small.clone()), &sigs), "");
        let looping = vec![Op::LoopStart, Op::LoopEnd];
        assert_eq!(attr(&func("helper", looping), &sigs), "");
        let recursive = vec![Op::CallFn { dst: None, name: "helper".into(), args: Vec::new(), result: None }];
        assert_eq!(attr(&func("helper", recursive), &sigs), "");
        let large: Vec<Op> = (0..=SMALL as u32).map(|k| Op::ConstInt { dst: ValueId(k), value: 0 }).collect();
        assert_eq!(attr(&func("helper", large), &sigs), "");
    }
}
