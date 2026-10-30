//! #4278 (B1): a runtime helper's RC signature, declared where the helper's
//! body is built and checked against that body.
//!
//! A helper that takes a block (`$list_push`, `$bytes_push`, `$str_append`,
//! `$cow`, …) moves reference counts inside itself. Its builder states, per
//! wasm parameter, one mode that holds on every path — `B` (borrow: the
//! caller keeps its reference) or `C` (consume: the caller hands its one
//! reference in, and every path releases, stores or returns it) — and logs,
//! as it emits each instruction that moves a count, the event that
//! instruction is. One log renders both halves of the helper's certificate:
//!
//! - the bundle's `rc-sig` record (`<modes>|<one line per parameter>\n<further
//!   lines>`), which the extracted checker, the Rocq kernel
//!   (`HelperSig.check_sig_cert`) and almide-verify judge: each parameter's
//!   line from the entry count its mode declares, every other line (the
//!   result, a temporary, an element the helper shares) as an owned object;
//! - the anchor file's `rc` line for the helper's body, which
//!   `almide-anchor-check` holds to the shipped bytes: every `rc_inc` and
//!   release call in the body is one logged event on the value it names, and
//!   every event one such call.
//!
//! A caller's credit that a helper consumes is then matched, at the call,
//! through the helper's checked signature (anchor-check `rc.rs`), never by
//! reading the callee's body for a reason to believe it.
//!
//! The log is the helper's own: the builder calls it next to the instruction
//! it describes, so the declaration lives in the definition — there is no
//! table of helper names anywhere.

/// A parameter's declared mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// The caller keeps its reference; the helper's own events on the
    /// parameter net 0 on every path.
    B,
    /// The caller hands its reference in; every path releases, stores or
    /// returns it.
    C,
}

/// One helper's declared signature and the log of its body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HelperSig {
    modes: Vec<u8>,
    /// Per object (the parameters first, in order), its ownership line.
    lines: Vec<String>,
    /// Per object, the local its events name (`None`: a temporary).
    names: Vec<Option<u32>>,
    /// The anchor events, in emission order.
    events: Vec<String>,
}

impl HelperSig {
    /// A helper with these parameter modes, in wasm parameter order (a
    /// scalar parameter is `B`: it holds no reference). Each parameter is
    /// an object named by its local.
    pub(crate) fn new(params: &[Mode]) -> Self {
        let mut s = HelperSig::default();
        for (k, m) in params.iter().enumerate() {
            s.modes.push(u8::from(*m == Mode::C));
            s.lines.push(String::new());
            s.names.push(Some(k as u32));
        }
        s
    }

    /// A further object (the helper's result, a temporary it allocates, the
    /// elements it shares), named by `name` in the anchor events.
    pub(crate) fn object(&mut self, name: Option<u32>) -> usize {
        self.lines.push(String::new());
        self.names.push(name);
        self.lines.len() - 1
    }

    /// An `rc_inc` (`a`) or release (`d`) call on object `obj`: an event the
    /// anchor check holds to that call, and the op on the object's line.
    pub(crate) fn rc(&mut self, obj: usize, kind: char) {
        debug_assert!(matches!(kind, 'a' | 'd'));
        self.lines[obj].push(kind);
        self.events.push(match self.names[obj] {
            Some(l) => format!("{kind}{l}"),
            None => format!("{kind}-"),
        });
    }

    /// A non-RC ownership op on `obj`'s line: `i` (a fresh block from
    /// `$alloc`), `m` (the reference stored into a block, or returned).
    pub(crate) fn op(&mut self, obj: usize, c: char) {
        debug_assert!(matches!(c, 'i' | 'm'));
        self.lines[obj].push(c);
    }

    fn mark_lines(&mut self, c: char) {
        for l in &mut self.lines {
            l.push(c);
        }
    }

    /// A one-shot branch opens (its THEN arm follows).
    pub(crate) fn branch(&mut self) {
        self.mark_lines('{');
        self.events.push("(".into());
    }

    /// The ELSE arm of the open branch begins.
    pub(crate) fn arm(&mut self) {
        self.mark_lines('|');
        self.events.push("|".into());
    }

    /// The arm just logged ends the function (`return`): it is the arm's
    /// last event on every line.
    pub(crate) fn exit(&mut self) {
        self.mark_lines('x');
    }

    /// The open branch joins.
    pub(crate) fn join(&mut self) {
        self.mark_lines('}');
        self.events.push(")".into());
    }

    /// A loop body opens / closes (any number of iterations). Loops are
    /// logged at the top level only: the line format's branch arms are flat.
    pub(crate) fn loop_open(&mut self) {
        self.mark_lines('(');
    }

    pub(crate) fn loop_close(&mut self) {
        self.mark_lines(')');
    }

    /// The bundle's `rc-sig` record bytes (proofs/HelperSig.v).
    pub(crate) fn record(&self) -> String {
        let modes: Vec<String> = self.modes.iter().map(u8::to_string).collect();
        format!("{}|{}", modes.join(" "), self.lines.join("\n"))
    }

    /// The anchor file's events for the helper's body.
    pub(crate) fn events(&self) -> String {
        self.events.join(" ")
    }
}

/// A static helper's slot with its body and declared signature.
pub(crate) fn signed(idx: u32, (f, sig): (wasm_encoder::Function, HelperSig)) -> (u32, wasm_encoder::Function, Option<HelperSig>) {
    (idx, f, Some(sig))
}

/// A per-program helper's body when its builder declares a signature.
pub(crate) fn per_program(h: &crate::work::Helper) -> Option<(wasm_encoder::Function, HelperSig)> {
    match h {
        crate::work::Helper::CowElems { inc_elems } => Some(crate::runtime_alloc::emit_cow_elems(*inc_elems)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_push_log_renders_its_record_and_its_events() {
        let mut s = HelperSig::new(&[Mode::C, Mode::C]);
        let base = s.object(None);
        s.branch();
        s.op(1, 'm');
        s.op(0, 'm');
        s.exit();
        s.arm();
        s.join();
        s.op(base, 'i');
        s.op(1, 'm');
        s.rc(0, 'd');
        s.op(base, 'm');
        assert_eq!(s.record(), "1 1|{mx|}d\n{mx|}m\n{x|}im");
        assert_eq!(s.events(), "( | ) d0");
    }
}
