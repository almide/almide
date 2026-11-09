//! ADR-0033 D7: the init witness's FAMILY records against the shipped bytes
//! — the carve ops of crates/almide-wasm/src/carve.rs, each one fixed,
//! contiguous instruction sequence, held here instruction by instruction.
//!
//! - `k F` (the root): `i32.wrap_i64; i32.const 4; i32.add; call $alloc;
//!   local.set R` [a], `local.get R; i32.const F; i32.store offset=12` [a]
//!   — the carve cursor starts at F, right after the root's header and
//!   cursor word (F = 16).
//! - `c H w W n N s0 .. s(N-1)` (a literal carve) and `c H w W v K f K t W`
//!   (a computed one): the cursor load [a] and the root's len load [a]; the
//!   ROOM TEST `cur + size >u 12 + len` in i64 (so nothing wraps), opening
//!   an `if` with no `else` whose body ends in `unreachable` [a, at its
//!   `end`]; then — after it, so the test dominates every store — the
//!   cursor store `cur + size` [a], `base = root + cur` [a], the three
//!   header stores through `local.get base` at 0 / 4 / 8 [a ×3] (the rc
//!   word is `0x8000_0000 | cur`, the interior word), and the payload: N
//!   zero stores at `12 + 4i` [a each], or the countdown over K whose
//!   cursor starts at `base + 12` and steps 4 with one zero store [a] per
//!   step, then `local.get base` [a]. For the computed form, before the
//!   room test: the length guard `len >u 0x7FFF_0000` aborts [a, at its
//!   `end`], and `K = max(1, (len + 3) >> 2)`, `size = 12 + 4K` are
//!   computed exactly so, so the header's cap is `4K >= len` and the size
//!   the room test checks is the extent the stores cover. A literal carve's
//!   len and cap are its constants: `cap = 4N`, `0 <= len <= cap`.
//! - `e H` (the seal): `local.get R; i32.load offset=12` [a], `local.get R;
//!   i32.load offset=4` [a], `i32.const H; i32.add; i32.ne`, then the
//!   aborting `if` [a, at its `end`]. Every family is followed, in the same
//!   function, at its control depth with nothing leaving it in between, by
//!   a seal of a local whose last write before it stored the family's
//!   result.
//!
//! The root, cursor, size, base, count and fill-cursor locals are distinct.
//! A record anchored `-` throughout is counted, not checked.

use wasmparser::{BlockType, Operator as Op};

use crate::decode::Body;
use crate::{At, Ctx};

/// One expected instruction.
#[derive(Clone, Copy)]
enum P {
    /// `local.get` / `local.set` / `local.tee` of the local named by the
    /// letter (bound at first sight, then held to it).
    Get(char),
    Set(char),
    Tee(char),
    I32(i32),
    /// Any `i32.const`, its value captured under the letter.
    Cap(char),
    I64(i64),
    /// `i32.load` / `i32.store` at the offset.
    Ld(u64),
    St(u64),
    Is(fn(&Op<'_>) -> bool),
}

/// The bindings a match produced.
#[derive(Default)]
pub(crate) struct Bind {
    locals: Vec<(char, u32)>,
    consts: Vec<(char, i32)>,
}

impl Bind {
    pub(crate) fn local(&self, c: char) -> Option<u32> {
        self.locals.iter().find(|(k, _)| *k == c).map(|(_, v)| *v)
    }
    fn konst(&self, c: char) -> Option<i32> {
        self.consts.iter().find(|(k, _)| *k == c).map(|(_, v)| *v)
    }
    fn bind(&mut self, c: char, l: u32) -> bool {
        match self.local(c) {
            Some(v) => v == l,
            None => {
                if self.locals.iter().any(|(_, v)| *v == l) {
                    return false; // the named locals are distinct
                }
                self.locals.push((c, l));
                true
            }
        }
    }
}

fn is_if_empty(o: &Op<'_>) -> bool {
    matches!(o, Op::If { blockty: BlockType::Empty })
}

/// The aborting `if` the ops emit after a condition: `if; i32.const;
/// call; i32.const; call; unreachable; end` — no `else`, and the body
/// cannot fall through. The `end` is the anchor.
const ABORT: [(P, bool); 7] = [
    (P::Is(is_if_empty), false),
    (P::Is(is_i32_const), false),
    (P::Is(is_call), false),
    (P::Is(is_i32_const), false),
    (P::Is(is_call), false),
    (P::Is(is_unreachable), false),
    (P::Is(is_end), true),
];

fn is_i32_const(o: &Op<'_>) -> bool {
    matches!(o, Op::I32Const { .. })
}
fn is_call(o: &Op<'_>) -> bool {
    matches!(o, Op::Call { .. })
}
fn is_unreachable(o: &Op<'_>) -> bool {
    matches!(o, Op::Unreachable)
}
fn is_end(o: &Op<'_>) -> bool {
    matches!(o, Op::End)
}

fn op_is(p: P, o: &Op<'_>, bind: &mut Bind) -> bool {
    match (p, o) {
        (P::Get(c), Op::LocalGet { local_index }) => bind.bind(c, *local_index),
        (P::Set(c), Op::LocalSet { local_index }) => bind.bind(c, *local_index),
        (P::Tee(c), Op::LocalTee { local_index }) => bind.bind(c, *local_index),
        (P::I32(v), Op::I32Const { value }) => *value == v,
        (P::Cap(c), Op::I32Const { value }) => {
            bind.consts.push((c, *value));
            true
        }
        (P::I64(v), Op::I64Const { value }) => *value == v,
        (P::Ld(off), Op::I32Load { memarg }) => memarg.offset == off,
        (P::St(off), Op::I32Store { memarg }) => memarg.offset == off,
        (P::Is(f), o) => f(o),
        _ => false,
    }
}

impl<'a> Ctx<'_, 'a> {
    /// Match `pat` at `start` of `b`, the marked instructions at `anchors`
    /// in order (all in function `func`).
    fn run(
        &self,
        b: &Body<'a>,
        at: At,
        pat: &[(P, bool)],
        anchors: &[At],
        what: &str,
    ) -> Result<Bind, String> {
        let (func, start) = (at.func, at.op);
        let mut bind = Bind::default();
        let bind = &mut bind;
        let mut next = 0;
        for (k, &(p, marked)) in pat.iter().enumerate() {
            let j = start + k;
            let op = b.ops.get(j).ok_or_else(|| format!("{what}: the body ends at instruction {j}"))?;
            if !op_is(p, op, bind) {
                let window: Vec<String> = b.ops[start..(j + 2).min(b.ops.len())].iter().map(|o| format!("{o:?}")).collect();
                return Err(format!("{what}: instruction {j} (step {k} of the shape) is {op:?}; from {start}: {}", window.join("; ")));
            }
            if marked {
                let a = anchors.get(next).ok_or_else(|| format!("{what}: fewer anchors than the shape marks"))?;
                if a.func != func || a.op != j {
                    return Err(format!("{what}: anchor {next} is {}:{}, the shape puts it at {func}:{j}", a.func, a.op));
                }
                next += 1;
            }
        }
        if next != anchors.len() {
            return Err(format!("{what}: {} anchors for {next} marked instructions", anchors.len()));
        }
        Ok(std::mem::take(bind))
    }

    /// `k F`, anchored at the root's `local.set` and the cursor store.
    /// Answers where the op pushes its result (the root's handle).
    pub(crate) fn family(&self, first: i32, anchors: &[At]) -> Result<At, String> {
        let a0 = *anchors.first().ok_or("family: no anchors")?;
        let b = self.body(a0)?;
        let alloc = self.alloc;
        let start = a0.op.checked_sub(4).ok_or("family: no room for its allocation")?;
        let pat: Vec<(P, bool)> = vec![
            (P::Is(|o| matches!(o, Op::I32WrapI64)), false),
            (P::I32(4), false),
            (P::Is(|o| matches!(o, Op::I32Add)), false),
            (P::Is(|_| true), false), // the call, held below
            (P::Set('R'), true),
            (P::Get('R'), false),
            (P::I32(first), false),
            (P::St(12), true),
            (P::Get('R'), false),
        ];
        self.run(b, At { func: a0.func, op: start }, &pat, anchors, "family")?;
        if !matches!(b.ops[start + 3], Op::Call { function_index } if function_index == alloc) {
            return Err(format!("family: instruction {} is not call $alloc", start + 3));
        }
        Ok(At { func: a0.func, op: start + 8 })
    }

    /// `e H`, anchored at the two loads and the abort's `end`. Answers
    /// the local the seal reads the root from and where its shape starts.
    pub(crate) fn seal(&self, hdr: i32, anchors: &[At]) -> Result<(At, u32), String> {
        let a0 = *anchors.first().ok_or("seal: no anchors")?;
        let b = self.body(a0)?;
        let start = a0.op.checked_sub(3).ok_or("seal: no room")?;
        let mut pat: Vec<(P, bool)> = vec![
            (P::Get('F'), false),
            (P::Set('R'), false),
            (P::Get('R'), false),
            (P::Ld(12), true),
            (P::Get('R'), false),
            (P::Ld(4), true),
            (P::I32(hdr), false),
            (P::Is(|o| matches!(o, Op::I32Add)), false),
            (P::Is(|o| matches!(o, Op::I32Ne)), false),
        ];
        pat.extend(ABORT);
        let bind = self.run(b, At { func: a0.func, op: start }, &pat, anchors, "seal")?;
        Ok((At { func: a0.func, op: start }, bind.local('F').ok_or("seal: unbound")?))
    }

    /// A family's seal: in the family's function, after it, at its control
    /// depth with nothing in between leaving it, reading a local whose last
    /// write before the seal stored the family's result (the operand
    /// provenance of decode.rs: the write pops the value the op pushed).
    pub(crate) fn sealed(&self, fam: At, seals: &[(At, u32)]) -> Result<(), String> {
        let b = self.body(fam)?;
        let d0 = b.depth[fam.op];
        let ok = seals.iter().any(|&(s, l)| {
            let last = (fam.op + 1..s.op).rev().find(|&k| Self::writes(&b.ops[k], l));
            s.func == fam.func
                && s.op > fam.op
                && b.depth[s.op] == d0
                && self.stays(b, fam.op, s.op, d0, "family").is_ok()
                && last.is_some_and(|k| b.args[k].first().copied().flatten() == Some(fam.op))
        });
        if ok {
            Ok(())
        } else {
            Err(format!("family at {}:{}: no seal of the local its root is stored to, after it in its block", fam.func, fam.op))
        }
    }

    /// The carve head shared by both forms, from the root's `local.get`:
    /// the cursor load, the room test, the cursor store, the base, the rc
    /// word. `size` pushes the block size as i64 (`i64`) and as i32 (`i32`).
    fn carve_head(size_i64: &[P], size_i32: &[P]) -> Vec<(P, bool)> {
        let mut p = vec![(P::Get('R'), false), (P::Ld(12), true), (P::Set('C'), false), (P::Get('C'), false)];
        p.push((P::Is(|o| matches!(o, Op::I64ExtendI32U)), false));
        p.extend(size_i64.iter().map(|&x| (x, false)));
        p.extend([
            (P::Is(|o| matches!(o, Op::I64Add)), false),
            (P::Get('R'), false),
            (P::Ld(4), true),
            (P::Is(|o| matches!(o, Op::I64ExtendI32U)), false),
            (P::I64(12), false),
            (P::Is(|o| matches!(o, Op::I64Add)), false),
            (P::Is(|o| matches!(o, Op::I64GtU)), false),
        ]);
        p.extend(ABORT);
        p.extend([(P::Get('R'), false), (P::Get('C'), false)]);
        p.extend(size_i32.iter().map(|&x| (x, false)));
        p.extend([
            (P::Is(|o| matches!(o, Op::I32Add)), false),
            (P::St(12), true),
            (P::Get('R'), false),
            (P::Get('C'), false),
            (P::Is(|o| matches!(o, Op::I32Add)), false),
            (P::Set('B'), true),
            (P::Get('B'), false),
            (P::Get('C'), false),
            (P::I32(i32::MIN), false), // 0x8000_0000, the interior bit
            (P::Is(|o| matches!(o, Op::I32Or)), false),
            (P::St(0), true),
        ]);
        p
    }

    /// `c H w W n N s0 .. s(N-1)`: anchors cursor load, len load, abort end,
    /// cursor store, base set, the three header stores, N payload stores.
    pub(crate) fn carve_fixed(&self, hdr: u64, w: u64, n: u64, stores: &[u64], anchors: &[At]) -> Result<(), String> {
        if hdr != 12 || w != 4 || n == 0 || stores.iter().copied().ne(0..n) {
            return Err(format!("carve: the literal shape is `c12 w4 nN s0 .. s(N-1)`, not h{hdr} w{w} n{n} {stores:?}"));
        }
        let a0 = *anchors.first().ok_or("carve: no anchors")?;
        let b = self.body(a0)?;
        let start = a0.op.checked_sub(1).ok_or("carve: no room")?;
        let size = 12 + 4 * n as i64;
        let mut pat = Self::carve_head(&[P::I64(size)], &[P::I32(size as i32)]);
        pat.extend([(P::Get('B'), false), (P::Cap('L'), false), (P::St(4), true)]);
        pat.extend([(P::Get('B'), false), (P::I32(4 * n as i32), false), (P::St(8), true)]);
        for i in 0..n {
            pat.extend([(P::Get('B'), false), (P::I32(0), false), (P::St(12 + 4 * i), true)]);
        }
        pat.push((P::Get('B'), false));
        let bind = self.run(b, At { func: a0.func, op: start }, &pat, anchors, "carve")?;
        let len = bind.konst('L').ok_or("carve: no len")?;
        if len < 0 || i64::from(len) > 4 * n as i64 {
            return Err(format!("carve: len {len} outside its cap {}", 4 * n));
        }
        Ok(())
    }

    /// `c H w W v K f K t W`: anchors the guard's abort end, then as the
    /// literal form's head and header, the loop's store and the base read
    /// after the loop.
    pub(crate) fn carve_fill(&self, hdr: u64, w: u64, k: u32, l: u32, t: u64, anchors: &[At]) -> Result<(), String> {
        if hdr != 12 || w != 4 || t != 4 || k != l {
            return Err(format!("carve: the computed shape is `c12 w4 vK fK t4`, not c{hdr} w{w} v{k} f{l} t{t}"));
        }
        let a0 = *anchors.first().ok_or("carve: no anchors")?;
        let b = self.body(a0)?;
        // The guard: `local.tee T; i64.const MAX; i64.gt_u` then the abort.
        let start = a0.op.checked_sub(9).ok_or("carve: no room")?;
        let mut pat: Vec<(P, bool)> = vec![
            (P::Tee('T'), false),
            (P::I64(0x7FFF_0000), false),
            (P::Is(|o| matches!(o, Op::I64GtU)), false),
        ];
        pat.extend(ABORT);
        pat.extend([
            (P::Get('T'), false),
            (P::Is(|o| matches!(o, Op::I32WrapI64)), false),
            (P::Set('N'), false),
            // K = (N + 3) >> 2
            (P::Get('N'), false),
            (P::I32(3), false),
            (P::Is(|o| matches!(o, Op::I32Add)), false),
            (P::I32(2), false),
            (P::Is(|o| matches!(o, Op::I32ShrU)), false),
            (P::Set('K'), false),
            // K = K ? K : 1
            (P::Get('K'), false),
            (P::I32(1), false),
            (P::Get('K'), false),
            (P::Is(|o| matches!(o, Op::Select)), false),
            (P::Set('K'), false),
            // S = (K << 2) + 12
            (P::Get('K'), false),
            (P::I32(2), false),
            (P::Is(|o| matches!(o, Op::I32Shl)), false),
            (P::I32(12), false),
            (P::Is(|o| matches!(o, Op::I32Add)), false),
            (P::Set('S'), false),
        ]);
        pat.extend(Self::carve_head(
            &[P::Get('S'), P::Is(|o| matches!(o, Op::I64ExtendI32U))],
            &[P::Get('S')],
        ));
        pat.extend([
            (P::Get('B'), false),
            (P::Get('N'), false),
            (P::St(4), true),
            (P::Get('B'), false),
            (P::Get('K'), false),
            (P::I32(2), false),
            (P::Is(|o| matches!(o, Op::I32Shl)), false),
            (P::St(8), true),
            // the countdown: P = B + 12; while K != 0 { *P = 0; P += 4; K -= 1 }
            (P::Get('B'), false),
            (P::I32(12), false),
            (P::Is(|o| matches!(o, Op::I32Add)), false),
            (P::Set('P'), false),
            (P::Is(|o| matches!(o, Op::Block { blockty: BlockType::Empty })), false),
            (P::Is(|o| matches!(o, Op::Loop { blockty: BlockType::Empty })), false),
            (P::Get('K'), false),
            (P::Is(|o| matches!(o, Op::I32Eqz)), false),
            (P::Is(|o| matches!(o, Op::BrIf { relative_depth: 1 })), false),
            (P::Get('P'), false),
            (P::I32(0), false),
            (P::St(0), true),
            (P::Get('P'), false),
            (P::I32(4), false),
            (P::Is(|o| matches!(o, Op::I32Add)), false),
            (P::Set('P'), false),
            (P::Get('K'), false),
            (P::I32(1), false),
            (P::Is(|o| matches!(o, Op::I32Sub)), false),
            (P::Set('K'), false),
            (P::Is(|o| matches!(o, Op::Br { relative_depth: 0 })), false),
            (P::Is(|o| matches!(o, Op::End)), false),
            (P::Is(|o| matches!(o, Op::End)), false),
            (P::Get('B'), true),
        ]);
        let bind = self.run(b, At { func: a0.func, op: start }, &pat, anchors, "carve")?;
        if bind.local('K') != Some(k) {
            return Err(format!("carve: the record names count local {k}, the loop counts {:?}", bind.local('K')));
        }
        Ok(())
    }
}
