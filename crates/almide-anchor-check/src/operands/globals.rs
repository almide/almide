//! Which globals a call may set. Two reads of one global are the same value
//! only when nothing between them can write it, and a `call` between them
//! runs code that may: its callee's own `global.set`s, and those of every
//! function that callee calls.
//!
//! TRUSTED: an imported function sets no global of this module. It holds no
//! reference to one (a global is reached by index, from this module's own
//! code), so it could only do so by calling back into the module while the
//! call is in progress, which the hosts this compiler targets do not do.

use std::collections::HashSet;
use std::rc::Rc;

use wasmparser::Operator as Op;

use crate::decode::Body;

/// For each defined function, the globals a call of it may set; `None` for
/// one that may set any (it reaches an indirect call, whose callee is not
/// known here).
#[derive(Clone)]
pub(crate) struct GlobalSets {
    imports: u32,
    sets: Rc<Vec<Option<HashSet<u32>>>>,
}

impl GlobalSets {
    /// Instruction `op` is a call that may set global `g`.
    pub(crate) fn call_may_set(&self, op: &Op<'_>, g: u32) -> bool {
        match op {
            Op::Call { function_index } | Op::ReturnCall { function_index } => match function_index.checked_sub(self.imports) {
                // An imported function (module header, TRUSTED).
                None => false,
                Some(d) => self.sets.get(d as usize).is_none_or(|s| s.as_ref().is_none_or(|s| s.contains(&g))),
            },
            Op::CallIndirect { .. } | Op::ReturnCallIndirect { .. } => true,
            _ => false,
        }
    }
}

/// The least fixpoint: a function sets what its own `global.set`s name and
/// what each defined function it calls sets; an indirect call makes it one
/// that may set any global, and an imported callee adds nothing (module
/// header, TRUSTED).
pub(crate) fn global_sets(bodies: &[Body<'_>], imports: u32) -> GlobalSets {
    let mut sets: Vec<Option<HashSet<u32>>> = bodies
        .iter()
        .map(|b| {
            Some(
                b.ops
                    .iter()
                    .filter_map(|op| match op {
                        Op::GlobalSet { global_index } => Some(*global_index),
                        _ => None,
                    })
                    .collect(),
            )
        })
        .collect();
    loop {
        let mut changed = false;
        for i in 0..bodies.len() {
            if sets[i].is_none() {
                continue;
            }
            let mut any = false;
            let mut add: HashSet<u32> = HashSet::new();
            for op in &bodies[i].ops {
                match op {
                    Op::Call { function_index } | Op::ReturnCall { function_index } => {
                        if let Some(d) = function_index.checked_sub(imports) {
                            match sets.get(d as usize) {
                                Some(Some(s)) => add.extend(s.iter().copied()),
                                _ => any = true,
                            }
                        }
                    }
                    Op::CallIndirect { .. } | Op::ReturnCallIndirect { .. } => any = true,
                    _ => {}
                }
            }
            if any {
                sets[i] = None;
                changed = true;
            } else if let Some(own) = sets[i].as_mut() {
                let before = own.len();
                own.extend(add);
                changed |= own.len() != before;
            }
        }
        if !changed {
            break;
        }
    }
    GlobalSets { imports, sets: Rc::new(sets) }
}
