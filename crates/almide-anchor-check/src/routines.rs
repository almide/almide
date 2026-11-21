//! The routines a located site names, held to their shipped bodies.
//!
//! bounds.rs and rc.rs read a `judges` site as a call whose result has its
//! argument's count, and a `holds` site as a call that takes +1 on a block.
//! Which call that is was the anchor file's word. Here the function each
//! site calls is held to the one body that routine has, instruction for
//! instruction, and the calls inside it to the routines they must be:
//!
//! - a judge is `$cow` or `$cow_elems`;
//! - `$cow` hands back a static or uniquely held block as it is, and for any
//!   other calls `$block_copy` on it, releases it and hands back the copy;
//! - `$cow_elems` calls `$cow`, then `$inc_elems` on the result when the
//!   result is not its argument;
//! - `$block_copy` allocates at its argument's count through `$alloc` and
//!   copies the payload;
//! - `$inc_elems` calls `$inc` on each 4-byte slot of the payload;
//! - a hold is `$inc`: +1 on the reference count word of a heap block.
//!
//! A site that names any other function is refused.
//!
//! TRUSTED: that `alloc <index>` names `$alloc`; the two callees no body is
//! given for (`$cow`'s release and `$block_copy`'s copy routine), which are
//! taken not to write a count word; the block layout (the reference count
//! at 0, `LEN` at 4, the payload at 12).

use wasmparser::{BlockType, Operator as Op};

use crate::decode::Body;
use crate::At;

/// One instruction of a routine's body.
#[derive(Clone, Copy)]
enum P {
    Get(u32),
    Set(u32),
    Tee(u32),
    /// A read of a global (the heap floor).
    Global,
    LtU,
    LeU,
    GeU,
    Ne,
    Add,
    If,
    Block,
    Loop,
    Return,
    End,
    Br(u32),
    BrIf(u32),
    /// An `i32.load` at this offset.
    Load(u64),
    /// An `i32.store` at this offset.
    Store(u64),
    Const(i32),
    /// A call; `shape` answers its target.
    Call,
}

use P::{Add, Block, Br, BrIf, Call, Const, End, GeU, Get, Global, If, LeU, Load, Loop, LtU, Ne, Return, Set, Store, Tee};

const COW: &[P] = &[
    Get(0), Global, LtU, If, Get(0), Return, End,
    Get(0), Load(0), Const(1), LeU, If, Get(0), Return, End,
    Get(0), Call, Set(1), Get(0), Call, Get(1), End,
];
const COW_ELEMS: &[P] = &[Get(0), Call, Tee(1), Get(0), Ne, If, Get(1), Call, End, Get(1), End];
const BLOCK_COPY: &[P] = &[
    Get(0), Load(4), Set(1), Get(1), Call, Set(2),
    Get(2), Const(12), Add, Get(0), Const(12), Add, Get(1), Call, Get(2), End,
];
const INC_ELEMS: &[P] = &[
    Get(0), Const(12), Add, Set(1), Get(1), Get(0), Load(4), Add, Set(2),
    Block, Loop, Get(1), Get(2), GeU, BrIf(1), Get(1), Load(0), Call,
    Get(1), Const(4), Add, Set(1), Br(0), End, End, End,
];
const INC: &[P] = &[Get(0), Global, LtU, If, Return, End, Get(0), Get(0), Load(0), Const(1), Add, Store(0), End];

/// The targets of `b`'s calls, in order, when `b` is `pat` instruction for
/// instruction.
fn shape(b: &Body<'_>, pat: &[P]) -> Option<Vec<u32>> {
    if b.ops.len() != pat.len() {
        return None;
    }
    let mut calls = Vec::new();
    for (op, p) in b.ops.iter().zip(pat) {
        let same = match (p, op) {
            (P::Get(l), Op::LocalGet { local_index }) | (P::Set(l), Op::LocalSet { local_index }) | (P::Tee(l), Op::LocalTee { local_index }) => {
                l == local_index
            }
            (P::Br(d), Op::Br { relative_depth }) | (P::BrIf(d), Op::BrIf { relative_depth }) => d == relative_depth,
            (P::Load(o), Op::I32Load { memarg }) | (P::Store(o), Op::I32Store { memarg }) => *o == memarg.offset,
            (P::Const(v), Op::I32Const { value }) => v == value,
            (P::Call, Op::Call { function_index }) => {
                calls.push(*function_index);
                true
            }
            (P::If, Op::If { blockty }) | (P::Block, Op::Block { blockty }) | (P::Loop, Op::Loop { blockty }) => *blockty == BlockType::Empty,
            (P::Global, Op::GlobalGet { .. })
            | (P::LtU, Op::I32LtU)
            | (P::LeU, Op::I32LeU)
            | (P::GeU, Op::I32GeU)
            | (P::Ne, Op::I32Ne)
            | (P::Add, Op::I32Add)
            | (P::Return, Op::Return)
            | (P::End, Op::End) => true,
            _ => false,
        };
        if !same {
            return None;
        }
    }
    Some(calls)
}

/// The module's functions, as a `call` names them.
struct Routines<'b, 'a> {
    bodies: &'b [Body<'a>],
    imports: u32,
    alloc: u32,
}

impl Routines<'_, '_> {
    /// The function the call at `at` names.
    fn callee(&self, at: At) -> Option<u32> {
        match self.bodies.get(at.func)?.ops.get(at.op)? {
            Op::Call { function_index } => Some(*function_index),
            _ => None,
        }
    }

    /// The targets of function `f`'s calls when its body is `pat`.
    fn calls(&self, f: u32, pat: &[P]) -> Option<Vec<u32>> {
        let at = usize::try_from(f.checked_sub(self.imports)?).ok()?;
        shape(self.bodies.get(at)?, pat)
    }

    fn inc(&self, f: u32) -> bool {
        self.calls(f, INC).is_some()
    }

    fn inc_elems(&self, f: u32) -> bool {
        self.calls(f, INC_ELEMS).is_some_and(|c| c.first().is_some_and(|&i| self.inc(i)))
    }

    fn block_copy(&self, f: u32) -> bool {
        self.calls(f, BLOCK_COPY).is_some_and(|c| c.first() == Some(&self.alloc))
    }

    fn cow(&self, f: u32) -> bool {
        self.calls(f, COW).is_some_and(|c| c.first().is_some_and(|&b| self.block_copy(b)))
    }

    fn cow_elems(&self, f: u32) -> bool {
        self.calls(f, COW_ELEMS).is_some_and(|c| match c.as_slice() {
            [cow, inc_elems] => self.cow(*cow) && self.inc_elems(*inc_elems),
            _ => false,
        })
    }
}

/// Hold every `judges` site's callee to the copy-on-write judge's body and
/// every `holds` site's to `$inc`'s. `imports` is the module's count of
/// imported functions and `alloc` the function index of `$alloc`.
pub fn check_named_routines(bodies: &[Body<'_>], imports: u32, alloc: u32, judges: &[At], holds: &[At]) -> Result<(), String> {
    let r = Routines { bodies, imports, alloc };
    for at in judges {
        let f = r.callee(*at).ok_or_else(|| format!("judge {}:{} is not a call", at.func, at.op))?;
        if !(r.cow(f) || r.cow_elems(f)) {
            return Err(format!("judge {}:{} calls function {f}, whose body is not the copy-on-write judge's", at.func, at.op));
        }
    }
    for at in holds {
        let f = r.callee(*at).ok_or_else(|| format!("hold {}:{} is not a call", at.func, at.op))?;
        if !r.inc(f) {
            return Err(format!("hold {}:{} calls function {f}, whose body is not the +1 on a block's reference count", at.func, at.op));
        }
    }
    Ok(())
}
