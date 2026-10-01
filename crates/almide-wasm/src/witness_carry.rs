//! Loop-carried locals in the structural witness (#2755), split from
//! witness_paths.rs for the file budget.

use std::collections::BTreeSet;

use super::{Ev, Node, FRAME_HELD};

/// #2755: the LOOP-CARRIED locals. A var bound outside a loop and rebound
/// inside it (`list.push(xs, v)` in a `for`, `acc = acc + [x]` in a
/// `while`) holds a different block at each loop head, which one frame
/// line cannot follow. The loop is made a HOLDER of the var's block, as a
/// heap `list.fold` accumulator is (`carried_owned`):
///
/// - before the loop, the var's credit moves into it (`m` on the frame
///   line's object);
/// - each iteration receives one block with one credit (`i`, a fresh object
///   bound to the var at the body's start) and hands the var's block on at
///   its end (`m`, `end_iteration`) — so a rebind that forgets the old
///   block's release leaves the received object unbalanced, and one that
///   releases it twice goes below 0;
/// - after the loop, the var holds the block the loop hands back (`i`, a
///   fresh object), released by the var's own epilogue or next rebind.
///
/// The hand-ons are bookkeeping, not instructions (the var's slot is the
/// carrier): each line is still one holder's account of one block, which
/// is all the decomposition needs. Synthetic objects are numbered from
/// `next` on. Loop levels count from 1 at the frame's outermost loop, as
/// the recorder's loop depth does.
pub(super) fn carry(seq: &mut Vec<Node>, level: u32, next: &mut u32) {
    let mut out = Vec::with_capacity(seq.len());
    for node in std::mem::take(seq) {
        match node {
            Node::Branch(mut arms) => {
                arms.iter_mut().for_each(|a| carry(a, level, next));
                out.push(Node::Branch(arms));
            }
            Node::Loop(mut body, _) => {
                carry(&mut body, level + 1, next);
                let mut locals = BTreeSet::new();
                carried_in(&body, level + 1, &mut locals);
                let mut head = Vec::new();
                let mut after = Vec::new();
                for &l in &locals {
                    let (arrived, left) = (*next, *next + 1);
                    *next += 2;
                    out.push(Node::Ev(Ev::LOp(l, 'm')));
                    head.extend([Ev::Birth(arrived), Ev::Op(arrived, 'i'), Ev::Bind { local: l, obj: arrived, owner: true }].map(Node::Ev));
                    after.extend([Ev::Birth(left), Ev::Op(left, 'i'), Ev::Bind { local: l, obj: left, owner: true }].map(Node::Ev));
                }
                body.splice(0..0, head);
                out.push(Node::Loop(body, locals.into_iter().collect()));
                out.extend(after);
            }
            ev => out.push(ev),
        }
    }
    *seq = out;
}

/// #2755 / #2758: the frame's OUTER holders — a top-let global, or the
/// C-319 cell a lifted lambda captured — that the frame WRITES. The holder
/// keeps one credit on its occupant for its whole life; the frame borrows it
/// for its run: it receives the occupant with that credit at its start
/// (`i`, a fresh object bound to the holder's pseudo local, prepended here)
/// and hands the occupant then held back at each of its ends (`m`,
/// witness_paths.rs `hand_back`; an abort's terminal discharges it). A
/// write in between is an ordinary rebind of the pseudo local: the old
/// occupant released (`d`), the new one held (`i`). A write that forgets
/// the release leaves the received object unbalanced. Returns the holders.
pub(super) fn frame_carry(tree: &mut Vec<Node>, next: &mut u32) -> Vec<u32> {
    fn collect(seq: &[Node], set: &mut BTreeSet<u32>) {
        for node in seq {
            match node {
                Node::Ev(Ev::Carry { local, depth }) if *depth == FRAME_HELD => {
                    set.insert(*local);
                }
                Node::Branch(arms) => arms.iter().for_each(|a| collect(a, set)),
                Node::Loop(body, _) => collect(body, set),
                Node::Ev(_) => {}
            }
        }
    }
    let mut held = BTreeSet::new();
    collect(tree, &mut held);
    let mut head = Vec::new();
    for &l in &held {
        let o = *next;
        *next += 1;
        head.extend([Ev::Birth(o), Ev::Op(o, 'i'), Ev::Bind { local: l, obj: o, owner: true }].map(Node::Ev));
    }
    tree.splice(0..0, head);
    held.into_iter().collect()
}

/// The locals a loop body at `level` carries: every `Carry` in it, nested
/// sites and loops included, of a local bound above that level.
fn carried_in(seq: &[Node], level: u32, set: &mut BTreeSet<u32>) {
    for node in seq {
        match node {
            Node::Ev(Ev::Carry { local, depth }) if *depth < level => {
                set.insert(*local);
            }
            Node::Branch(arms) => arms.iter().for_each(|a| carried_in(a, level, set)),
            Node::Loop(body, _) => carried_in(body, level, set),
            Node::Ev(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::render;
    use super::super::Ev::*;
    use super::super::Ev;

    fn cert(log: &[Ev], n: u32) -> String {
        render(log, n).expect("renders")
    }

    fn bind(local: u32, obj: u32) -> Ev {
        Bind { local, obj, owner: true }
    }

    #[test]
    fn a_var_rebound_inside_a_loop_is_carried_by_it() {
        // var xs = f(); for .. { xs = g(xs) (release the old block) } ; d xs
        let rebind = [Carry { local: 3, depth: 0 }, LOp(3, 'd'), Birth(1), Op(1, 'i'), bind(3, 1)];
        let mut log = vec![Birth(0), Op(0, 'i'), bind(3, 0), LoopOpen];
        log.extend(rebind.clone());
        log.extend([LoopClose, LOp(3, 'd')]);
        // xs's first block moves into the loop; the block an iteration builds
        // is handed on; the received one is released; the loop hands one
        // back, which the epilogue releases.
        assert_eq!(cert(&log, 2), "im\n\nim\n\nid\nid\n");
        // A rebind that forgets the old block leaves the received one held.
        let mut leak = vec![Birth(0), Op(0, 'i'), bind(3, 0), LoopOpen];
        leak.extend([Carry { local: 3, depth: 0 }, Birth(1), Op(1, 'i'), bind(3, 1), LoopClose, LOp(3, 'd')]);
        assert_eq!(cert(&leak, 2), "im\n\nim\n\ni\nid\n");
        // A rebind on one path only: the other hands the received block on.
        let mut one_arm = vec![Birth(0), Op(0, 'i'), bind(3, 0), LoopOpen, Open];
        one_arm.extend(rebind);
        one_arm.extend([Arm, Close, LoopClose, LOp(3, 'd')]);
        assert_eq!(cert(&one_arm, 2), "im\n\n{|im}\n\n{id|im}\nid\n");
    }

    #[test]
    fn a_var_carried_by_an_inner_loop_only_is_released_by_the_outer_iteration() {
        // for .. { var t = f(); for .. { t = g(t) } }
        let log = [
            LoopOpen,
            DecOld(4), Birth(0), Op(0, 'i'), bind(4, 0),
            LoopOpen, Carry { local: 4, depth: 1 }, LOp(4, 'd'), Birth(1), Op(1, 'i'), bind(4, 1), LoopClose,
            LoopClose,
            LOp(4, 'd'),
        ];
        // t's own block moves into the inner loop; the inner loop's
        // hand-back is released at the outer iteration's end.
        assert_eq!(cert(&log, 2), "\nim\n\nim\n\nid\n\nid\n");
    }

    #[test]
    fn an_outer_holder_lends_its_block_for_the_frame_and_gets_one_back() {
        const H: u32 = 0xC000_0001;
        let held = Carry { local: H, depth: super::super::FRAME_HELD };
        // g = f() (releasing the old occupant): the occupant the frame
        // received is released, the new one handed back at the end.
        let log = [held.clone(), LOp(H, 'd'), Birth(0), Op(0, 'i'), bind(H, 0)];
        assert_eq!(cert(&log, 1), "im\nid\n");
        // A write that forgets the old occupant leaves it held.
        let leak = [held.clone(), Birth(0), Op(0, 'i'), bind(H, 0)];
        assert_eq!(cert(&leak, 1), "im\ni\n");
        // An exit hands the occupant back on its own path too.
        let exit = [held, Open, Exit, Arm, Close, LOp(H, 'd'), Birth(0), Op(0, 'i'), bind(H, 0)];
        assert_eq!(cert(&exit, 1), "{|im}\n{id|im}\n");
    }
}
