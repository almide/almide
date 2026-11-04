//! Host text the guest must not trust (#4391). A String is valid UTF-8, and
//! there is no OsString, so bytes a host hands over for `env.args`,
//! `env.get` and `env.vars` are checked here, in the guest, whatever runtime
//! hosts it: the embedded host already answers what native answers, but a
//! stock preview-1 runtime may pass `args_get` / `environ_get` bytes raw,
//! and every String walker past this point trusts the encoding.
//!
//! The rule is native's (the Node / Deno line):
//! - `env.args`: an argument that is not UTF-8 is its lossy decode
//!   (`args_os` + `to_string_lossy`, one U+FFFD per maximal invalid subpart);
//! - `env.get`: such a value is `none` (`std::env::var(..).ok()`);
//! - `env.vars`: such an entry is left out.
//!
//! One check serves all three: `$utf8_lossy` of the block, compared with the
//! block itself. The decode equals its input exactly when the input is
//! UTF-8 (a valid sequence is copied, and the first invalid subpart becomes
//! EF BF BD where the input held something else), so no second walker is
//! linked. The helper ships only in a module that calls one of these ops.

use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::work::Helper;
use crate::*;

/// What a frames walk does with a frame that is not UTF-8.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostText {
    /// Kept as it came (the abort-site run's frames are the host's own).
    Trusted,
    /// Replaced by its lossy decode (`env.args`).
    Lossy,
    /// Left out (`env.vars`).
    Omitted,
}

impl Emitter<'_> {
    /// `raw` holds a String block of host bytes: its lossy decode lands in
    /// `dec` and whether the two are equal (the bytes were UTF-8) is left
    /// on the stack.
    pub(crate) fn host_text_check(&mut self, raw: u32, dec: u32) {
        let lossy = self.work.helper(Helper::Utf8Lossy);
        let mut i = self.f.instructions();
        i.local_get(raw).call(lossy).local_tee(dec).local_get(raw).call(F_STR_EQ);
    }

    /// Release the String block in `l`, an allocation of this lowering.
    pub(crate) fn host_text_release(&mut self, l: u32) {
        self.f.instructions().local_get(l).call(F_DEC_FLAT);
        // #4278 B1: the arm releases a block it allocated itself.
        self.arm_temp_released();
    }

    /// The frame in `line` (a fresh String block) under `mode`, pushed onto
    /// the list in `list` unless the mode leaves it out. `dec` is scratch.
    pub(crate) fn host_text_frame(&mut self, mode: HostText, (line, list, dec): (u32, u32, u32)) {
        let push = |em: &mut Self| {
            em.f.instructions().local_get(list).local_get(line).call(F_LIST_PUSH_4).local_set(list);
        };
        match mode {
            HostText::Trusted => push(self),
            HostText::Lossy => {
                self.host_text_check(line, dec);
                self.f.instructions().if_(BlockType::Empty);
                self.host_text_release(dec);
                self.f.instructions().else_();
                self.host_text_release(line);
                self.f.instructions().local_get(dec).local_set(line);
                self.f.instructions().end();
                push(self);
            }
            HostText::Omitted => {
                self.host_text_check(line, dec);
                self.host_text_release(dec);
                self.f.instructions().if_(BlockType::Empty);
                push(self);
                self.f.instructions().else_();
                self.host_text_release(line);
                self.f.instructions().end();
            }
        }
    }
}
