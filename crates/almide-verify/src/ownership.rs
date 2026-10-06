//! The ownership checker — the Rust mirror of `proofs/OwnershipChecker.v`'s
//! `check_xc` (certificate format v6: flat events, `(…)` loops, `[…|…]`
//! conditional loops, `{…|…}` one-shot branches, the `x` arm-exit marker and
//! the `t` arm-abort terminal), with the owned-line resurrection rule (#3229:
//! on a line born by a top-level `i`, every later `a` must meet a live count).
//!
//! One line per reference-counted object. Every function below is a
//! transcription of the Gallina definition named in its comment, including
//! the defensive parse behaviours (a dangling bracket at end of line, a `(`
//! that restarts an open loop body, a `{…}` closed while a loop is open); the
//! differential leg of `proofs/gate.sh` holds the two to the same verdict.

/// `Op` — the per-object event alphabet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Inc,
    Alias,
    Dec,
    MoveOut,
    Reuse,
    Borrow,
}

/// `CertItem` — one element of a parsed certificate line.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    Op(Op),
    Loop(Vec<Op>),
    CondLoop(Vec<Op>, Vec<Op>),
    Branch(Vec<Op>, Vec<Op>),
    /// `CBranchRet`: `true` = the THEN arm is the returning one.
    BranchRet(bool, Vec<Op>, Vec<Op>),
    /// `CBranchAbort`: `true` = the THEN arm ends the process (format v6).
    BranchAbort(bool, Vec<Op>, Vec<Op>),
    Poison,
}

/// `parse_byte`: the op alphabet, either case; every other byte is skipped.
fn parse_byte(b: u8) -> Option<Op> {
    match b.to_ascii_lowercase() {
        b'i' => Some(Op::Inc),
        b'a' => Some(Op::Alias),
        b'd' => Some(Op::Dec),
        b'm' => Some(Op::MoveOut),
        b'r' => Some(Op::Reuse),
        b'b' => Some(Op::Borrow),
        _ => None,
    }
}

fn is_xmark(b: u8) -> bool {
    b == b'x' || b == b'X'
}

/// `is_tmark`: the arm-abort terminal (format v6).
fn is_tmark(b: u8) -> bool {
    b == b't' || b == b'T'
}

/// `exec`: fold the refcount; `None` is a fault (a release at 0, a reuse of a
/// shared or dead object, a borrow of a dead one).
fn exec(ops: &[Op], mut rc: i64) -> Option<i64> {
    for op in ops {
        rc = match op {
            Op::Inc | Op::Alias => rc + 1,
            Op::Dec | Op::MoveOut if rc <= 0 => return None,
            Op::Dec | Op::MoveOut => rc - 1,
            Op::Reuse if rc == 1 => 0,
            Op::Reuse => return None,
            Op::Borrow if rc <= 0 => return None,
            Op::Borrow => rc,
        };
    }
    Some(rc)
}

/// `exec_line`: loops must PRESERVE the entry count, a one-shot branch's arms
/// must AGREE, a returning arm must reach exactly 0 on its own.
fn exec_line(items: &[Item], mut rc: i64) -> Option<i64> {
    for item in items {
        rc = match item {
            Item::Op(o) => exec(std::slice::from_ref(o), rc)?,
            Item::Loop(body) => (exec(body, rc)? == rc).then_some(rc)?,
            Item::CondLoop(then_b, else_b) => {
                let (rt, re) = (exec(then_b, rc)?, exec(else_b, rc)?);
                (rt == rc && re == rc).then_some(rc)?
            }
            Item::Branch(then_b, else_b) => {
                let (rt, re) = (exec(then_b, rc)?, exec(else_b, rc)?);
                (rt == re).then_some(rt)?
            }
            Item::BranchRet(ret_then, then_b, else_b) => {
                let (rt, re) = (exec(then_b, rc)?, exec(else_b, rc)?);
                let (exiting, surviving) = if *ret_then { (rt, re) } else { (re, rt) };
                (exiting == 0).then_some(surviving)?
            }
            // The aborting arm need only be fault-free: the process ends
            // there, discharging what it holds. The line continues from the
            // surviving arm alone.
            Item::BranchAbort(ab_then, then_b, else_b) => {
                let (rt, re) = (exec(then_b, rc)?, exec(else_b, rc)?);
                if *ab_then { re } else { rt }
            }
            Item::Poison => return None,
        };
    }
    Some(rc)
}

/// `check_line`: fault-free and back to 0.
fn check_line(items: &[Item]) -> bool {
    exec_line(items, 0) == Some(0)
}

/// The in-progress `{ then | else }` region (`brxst`).
#[derive(Default)]
struct Brx {
    in_else: bool,
    then_exited: bool,
    else_exited: bool,
    then_aborted: bool,
    else_aborted: bool,
    malformed: bool,
    then_ops: Vec<Op>,
    else_ops: Vec<Op>,
}

impl Brx {
    /// `close_brx`: malformed or both arms terminal → poison; one exit-marked
    /// arm → `CBranchRet`; one abort-marked arm → `CBranchAbort`; no marker
    /// → the ordinary `CBranch`.
    fn close(self) -> Item {
        let then_term = self.then_exited || self.then_aborted;
        let else_term = self.else_exited || self.else_aborted;
        if self.malformed || (then_term && else_term) {
            Item::Poison
        } else if self.then_exited {
            Item::BranchRet(true, self.then_ops, self.else_ops)
        } else if self.else_exited {
            Item::BranchRet(false, self.then_ops, self.else_ops)
        } else if self.then_aborted {
            Item::BranchAbort(true, self.then_ops, self.else_ops)
        } else if self.else_aborted {
            Item::BranchAbort(false, self.then_ops, self.else_ops)
        } else {
            Item::Branch(self.then_ops, self.else_ops)
        }
    }

    /// One byte inside `{…}` (not `}`, not a newline).
    fn step(&mut self, b: u8) {
        if b == b'|' {
            self.in_else = true;
        } else if is_xmark(b) || is_tmark(b) {
            // A second mark on the same arm is malformed.
            let abort = is_tmark(b);
            if self.in_else {
                self.malformed |= self.else_exited || self.else_aborted;
                self.else_exited |= !abort;
                self.else_aborted |= abort;
            } else {
                self.malformed |= self.then_exited || self.then_aborted;
                self.then_exited |= !abort;
                self.then_aborted |= abort;
            }
        } else if let Some(op) = parse_byte(b) {
            // An op after the current arm's mark: `x` / `t` must be
            // arm-terminal (nothing runs after an exit or an abort).
            if self.in_else {
                self.malformed |= self.else_exited || self.else_aborted;
                self.else_ops.push(op);
            } else {
                self.malformed |= self.then_exited || self.then_aborted;
                self.then_ops.push(op);
            }
        }
    }
}

/// The in-progress `[ then | else ]` conditional loop (`condst`).
#[derive(Default)]
struct Cond {
    in_else: bool,
    then_ops: Vec<Op>,
    else_ops: Vec<Op>,
}

impl Cond {
    /// One byte inside `[…]` (not a newline); `true` when it is the closing `]`.
    fn step(&mut self, b: u8) -> bool {
        match b {
            b'|' => self.in_else = true,
            b']' => return true,
            _ => {
                if let Some(op) = parse_byte(b) {
                    let arm = if self.in_else { &mut self.else_ops } else { &mut self.then_ops };
                    arm.push(op);
                }
            }
        }
        false
    }
}

/// The parser state of one line (`parse_xc`'s `cur`, `lp`, `cp`, `bp`).
#[derive(Default)]
struct LineState {
    items: Vec<Item>,
    loop_body: Option<Vec<Op>>,
    cond: Option<Cond>,
    branch: Option<Brx>,
}

impl LineState {
    /// `finish_line_x` / `finish_line_c`: flush, defensively closing ONE open
    /// region — an open branch wins over an open conditional loop, which wins
    /// over an open loop (the others are dropped, as in the proof).
    fn finish(mut self) -> Vec<Item> {
        if let Some(brx) = self.branch {
            self.items.push(brx.close());
        } else if let Some(cond) = self.cond {
            self.items.push(Item::CondLoop(cond.then_ops, cond.else_ops));
        } else if let Some(body) = self.loop_body {
            self.items.push(Item::Loop(body));
        }
        self.items
    }

    /// One byte that is not a newline.
    fn step(&mut self, b: u8) {
        if let Some(brx) = self.branch.as_mut() {
            if b == b'}' {
                let closed = self.branch.take().map(Brx::close);
                self.items.extend(closed);
            } else {
                brx.step(b);
            }
        } else if is_xmark(b) || is_tmark(b) {
            // An exit or abort marker outside a `{…}` branch poisons the line.
            self.items.push(Item::Poison);
        } else if let Some(cond) = self.cond.as_mut() {
            if cond.step(b) {
                let cond = self.cond.take().unwrap_or_default();
                self.items.push(Item::CondLoop(cond.then_ops, cond.else_ops));
            }
        } else {
            self.step_plain(b);
        }
    }

    /// One byte outside any `{…}` / `[…]` region.
    fn step_plain(&mut self, b: u8) {
        match b {
            b'{' => self.branch = Some(Brx::default()),
            b'[' => self.cond = Some(Cond::default()),
            // `(` starts a FRESH body, discarding an unclosed one.
            b'(' => self.loop_body = Some(Vec::new()),
            b')' => {
                if let Some(body) = self.loop_body.take() {
                    self.items.push(Item::Loop(body));
                }
            }
            _ => {
                if let Some(op) = parse_byte(b) {
                    match self.loop_body.as_mut() {
                        Some(body) => body.push(op),
                        None => self.items.push(Item::Op(op)),
                    }
                }
            }
        }
    }
}

/// `parse_xc`: bytes → one item list per newline-separated line (the final
/// line is flushed at end of input, so a trailing newline yields an empty,
/// trivially balanced last line).
fn parse(witness: &[u8]) -> Vec<Vec<Item>> {
    witness
        .split(|&b| b == b'\n')
        .map(|line| {
            let mut state = LineState::default();
            for &b in line {
                state.step(b);
            }
            state.finish()
        })
        .collect()
}

/// `guard_ops`: a liveness probe (`Borrow`) before every `Alias`.
fn guard_ops(ops: &[Op]) -> Vec<Op> {
    ops.iter()
        .flat_map(|o| match o {
            Op::Alias => vec![Op::Borrow, Op::Alias],
            other => vec![*other],
        })
        .collect()
}

/// `guard_item`: the probe inside every op, loop body and arm.
fn guard_item(item: &Item) -> Vec<Item> {
    match item {
        Item::Op(o) => guard_ops(std::slice::from_ref(o)).into_iter().map(Item::Op).collect(),
        Item::Loop(body) => vec![Item::Loop(guard_ops(body))],
        Item::CondLoop(t, e) => vec![Item::CondLoop(guard_ops(t), guard_ops(e))],
        Item::Branch(t, e) => vec![Item::Branch(guard_ops(t), guard_ops(e))],
        Item::BranchRet(f, t, e) => vec![Item::BranchRet(*f, guard_ops(t), guard_ops(e))],
        Item::BranchAbort(f, t, e) => vec![Item::BranchAbort(*f, guard_ops(t), guard_ops(e))],
        Item::Poison => vec![Item::Poison],
    }
}

/// `guard_line` (#3229): a line whose first item is a top-level `Inc` is an
/// OWNED object, dead once its count returns to 0, so every later `Alias`
/// must meet a live count. Any other line is returned unchanged.
fn guard_line(items: &[Item]) -> Vec<Item> {
    match items.split_first() {
        Some((Item::Op(Op::Inc), rest)) => std::iter::once(Item::Op(Op::Inc))
            .chain(rest.iter().flat_map(guard_item))
            .collect(),
        _ => items.to_vec(),
    }
}

/// `check_xc`: every line of the certificate, with the owned-line
/// resurrection probe, is fault-free and balanced.
pub(crate) fn check_xc(witness: &[u8]) -> bool {
    parse(witness).iter().all(|line| check_line(&guard_line(line)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loop_opened_inside_a_branch_region_is_not_a_loop() {
        // Inside `{…}` the paren bytes are skipped, exactly as parse_xc does.
        assert_eq!(parse(b"{(i)|}"), vec![vec![Item::Branch(vec![Op::Inc], vec![])]]);
    }

    #[test]
    fn a_branch_closed_inside_an_open_loop_lands_before_the_loop() {
        assert_eq!(
            parse(b"(i{d|d})"),
            vec![vec![Item::Branch(vec![Op::Dec], vec![Op::Dec]), Item::Loop(vec![Op::Inc])]]
        );
    }

    #[test]
    fn a_second_open_paren_restarts_the_body() {
        assert_eq!(parse(b"(i(d)"), vec![vec![Item::Loop(vec![Op::Dec])]]);
    }

    #[test]
    fn end_of_line_closes_the_branch_and_drops_the_open_loop() {
        assert_eq!(parse(b"(i{d"), vec![vec![Item::Branch(vec![Op::Dec], vec![])]]);
    }
}
