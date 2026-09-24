//! The ownership checker — the Rust mirror of `proofs/OwnershipChecker.v`'s
//! `check_xc` (certificate format v5: flat events, `(…)` loops, `[…|…]`
//! conditional loops, `{…|…}` one-shot branches, the `x` arm-exit marker).
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
    malformed: bool,
    then_ops: Vec<Op>,
    else_ops: Vec<Op>,
}

impl Brx {
    /// `close_brx`: malformed or both arms exiting → poison; one marked arm →
    /// `CBranchRet`; no marker → the ordinary `CBranch`.
    fn close(self) -> Item {
        if self.malformed || (self.then_exited && self.else_exited) {
            Item::Poison
        } else if self.then_exited {
            Item::BranchRet(true, self.then_ops, self.else_ops)
        } else if self.else_exited {
            Item::BranchRet(false, self.then_ops, self.else_ops)
        } else {
            Item::Branch(self.then_ops, self.else_ops)
        }
    }

    /// One byte inside `{…}` (not `}`, not a newline).
    fn step(&mut self, b: u8) {
        if b == b'|' {
            self.in_else = true;
        } else if is_xmark(b) {
            // A second mark on the same arm is malformed.
            if self.in_else {
                self.malformed |= self.else_exited;
                self.else_exited = true;
            } else {
                self.malformed |= self.then_exited;
                self.then_exited = true;
            }
        } else if let Some(op) = parse_byte(b) {
            // An op after the current arm's mark: `x` must be arm-terminal.
            if self.in_else {
                self.malformed |= self.else_exited;
                self.else_ops.push(op);
            } else {
                self.malformed |= self.then_exited;
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
        } else if is_xmark(b) {
            // An exit marker outside a `{…}` branch poisons the line.
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

/// `check_xc`: every line of the certificate is fault-free and balanced.
pub(crate) fn check_xc(witness: &[u8]) -> bool {
    parse(witness).iter().all(|line| check_line(line))
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
