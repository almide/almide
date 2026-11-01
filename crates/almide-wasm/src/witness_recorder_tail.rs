// ── witness.rs, the recorder's temporaries, decline and certificate, and the balance rule ──
//
// include!-spliced into `witness.rs` at module level (the 800-line file
// discipline). The imports are witness.rs's own.

impl WitnessRecorder {
    /// An owned temporary born here whose later events the site names by
    /// object (a `!` operand carrier: moved out on the err arm, released
    /// on the ok path).
    pub fn temp_born(&mut self) -> u32 {
        let o = self.new_obj();
        self.ops(o, "i");
        o
    }

    /// Record `ops` on an object a site holds by id ([`Self::temp_born`]).
    pub fn temp_ops(&mut self, o: u32, ops: &str) {
        self.ops(o, ops);
    }

    /// #4278: an arm's SHARE-OUT — on this path the arm answers with its
    /// argument's block as its owned result, after an `rc_inc` on it. The
    /// +1 is the result's credit (its line is born `i` by the consumer), so
    /// the argument's line records `s`: +0 there, a liveness probe on an
    /// owned line, and the RC byte check holds it to the `rc_inc` inside the
    /// arm's window read through `local`. `None`: the argument is no frame
    /// variable (a temporary, a view, a global) — its own line takes the `s`.
    pub fn arm_share_out(&mut self, local: Option<u32>) -> bool {
        match local {
            Some(l) => self.held_ops(l, "s"),
            None => {
                let o = self.view_obj();
                self.ops(o, "s");
                true
            }
        }
    }

    /// The share a borrowed `!` carrier takes before it propagates.
    pub fn share_local(&mut self, local: u32) -> bool {
        self.held_ops(local, "a")
    }

    /// Is the code being emitted right now unreachable on every path?
    pub fn dead(&self) -> bool {
        self.branches.dead()
    }

    /// Arm the next ReturnError exit as a recorded propagation.
    pub fn arm_err_exit(&mut self) {
        self.err_exit_armed = true;
    }

    /// Was the ReturnError exit being emitted armed? (Disarms it.)
    pub fn take_err_exit(&mut self) -> bool {
        std::mem::take(&mut self.err_exit_armed)
    }

    pub fn poison(&mut self) {
        self.poisoned = true;
    }

    /// Withdraw this frame's certificate: the first reason wins (the
    /// shape that turned the recorder away is the one to count).
    pub fn decline(&mut self, reason: &str) {
        if self.declined.is_none() {
            self.declined = Some(reason.to_string());
        }
    }

    /// One line per object, in object order — certificate v0. A poison
    /// outranks a decline: a hook disagreement is a bug even in a frame
    /// that withdrew.
    /// #2758: the recorded `(a, d, i)` events, every path together — one
    /// per emitted RC instruction (witness_helper.rs audits them).
    pub(crate) fn event_totals(&self) -> (usize, usize, usize) {
        let mut n = (0, 0, 0);
        for ev in &self.log {
            if let Ev::Op(_, c) | Ev::LOp(_, c) = ev {
                match c {
                    // A share-out (#4278) is an `rc_inc` too.
                    'a' | 's' => n.0 += 1,
                    'd' => n.1 += 1,
                    'i' => n.2 += 1,
                    _ => {}
                }
            }
        }
        n
    }

    pub fn certificate(&self) -> String {
        if self.poisoned {
            return "!poison\n".to_string();
        }
        if let Some(r) = &self.declined {
            return format!("{DECLINE_PREFIX}{r}\n");
        }
        if !self.branches.settled() {
            return "!poison\n".to_string();
        }
        match paths::render(&self.log, self.next_obj) {
            Ok(s) => s,
            Err(r) => format!("{DECLINE_PREFIX}{r}\n"),
        }
    }
}

/// The proven balance rule, mirrored: per stream, `i`/`a` = +1, `d`/`m` =
/// −1, `b` = +0 at a count above 0 (no read after the release), every
/// prefix nonnegative (no release at rc 0), final balance zero (no leak).
/// Arm braces are phase-B vocabulary — their presence here is out of subset
/// and fails.
pub fn balanced(cert: &str) -> bool {
    for line in cert.lines() {
        let mut bal: i64 = 0;
        for c in line.chars() {
            match c {
                'i' | 'a' => bal += 1,
                'd' | 'm' => bal -= 1,
                'b' if bal > 0 => {}
                // #4278: an arm's share-out moves no count on its line.
                's' => {}
                _ => return false,
            }
            if bal < 0 {
                return false;
            }
        }
        if bal != 0 {
            return false;
        }
    }
    true
}
