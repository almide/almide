//! The collection sink's READ side (#2754, #2759), split from witness.rs for
//! the file budget: the pass / shipped / context markers the emitter pushes,
//! and the `take*` readers that split the drained sink back into frames by
//! emission pass.

use super::{push, sink};

/// Every frame the sweep collected, over EVERY emission pass (the pass
/// markers are stripped).
pub fn take() -> Vec<(String, String)> {
    take_with_shipped().0
}

/// A recorded frame: `(pass, function name, certificate, convention
/// context)` — the context is empty for a frame the gate turned away.
pub type PassFrame = (usize, String, String, String);

/// The raw sink split into frames: every frame with its pass and context,
/// and the shipped pass's marker value (`None`: nothing shipped).
fn parse(raw: Vec<(String, String)>) -> (Vec<PassFrame>, Option<String>) {
    let shipped = raw.iter().rev().find(|(n, _)| n == SHIPPED_MARK).map(|(_, p)| p.clone());
    let mut out = Vec::new();
    let mut pass = 0usize;
    let mut ctx = String::new();
    for (name, cert) in raw {
        match name.as_str() {
            PASS_MARK => pass = cert.parse().unwrap_or(0),
            SHIPPED_MARK => {}
            CONTEXT_MARK => ctx = cert,
            _ => out.push((pass, name, cert, std::mem::take(&mut ctx))),
        }
    }
    (out, shipped)
}

fn drain() -> Vec<(String, String)> {
    sink().lock().expect("witness sink").take().unwrap_or_default()
}

/// The sink's pass boundaries (#2754). `emit_program` emits in up to three
/// passes — the first over the WHOLE linked registry graph, the next over
/// the reachable set, a third when the bounded-line rewrites fired — and
/// ships one of them. A frame only an earlier pass emitted (a dead linked
/// stdlib body) is not in the artifact, so a per-program verdict reads the
/// SHIPPED pass. The markers use a name no function can spell.
const PASS_MARK: &str = "\u{0}pass";
const SHIPPED_MARK: &str = "\u{0}shipped";
pub(super) const CONTEXT_MARK: &str = "\u{0}context";

/// Both markers go through `push`, a no-op unless a sweep collects.
pub(crate) fn mark_pass(pass: usize) {
    crate::witness::modes::begin_pass(pass);
    crate::witness::bounds::begin_pass(pass);
    push(PASS_MARK, pass.to_string());
}

pub(crate) fn mark_shipped(pass: usize) {
    push(SHIPPED_MARK, pass.to_string());
}

/// `(function name, certificate)` pairs, in emission order.
pub type Frames = Vec<(String, String)>;

/// `(every frame of every pass, the frames of the pass that shipped)`. The
/// second is empty when no pass was marked as shipped (a refused program).
pub fn take_with_shipped() -> (Frames, Frames) {
    let (all, shipped) = take_by_pass();
    (all.into_iter().map(|(_, n, c)| (n, c)).collect(), shipped)
}

/// The frames of the pass that shipped, with that pass's number (#2759:
/// the bundle producer pairs them with the same pass's call-mode witness).
/// `None` when no pass was marked as shipped.
pub fn take_shipped() -> Option<(usize, Frames)> {
    let (all, shipped) = parse(drain());
    let shipped: usize = shipped?.parse().ok()?;
    let frames = all.into_iter().filter(|f| f.0 == shipped).map(|(_, n, c, _)| (n, c)).collect();
    Some((shipped, frames))
}

/// The pass `emit_program` emits WITHOUT the bounded-line rewrites (#2312,
/// `line_bounded.rs`): its code differs from passes 1 and 2 by design (a
/// `println(int.to_string(x))` builds and releases a block there instead of
/// printing from the itoa scratch), so a sweep holds only the passes of one
/// configuration to agreeing certificates.
pub const CHECKED_PASS: usize = 3;

/// `(every frame of every pass WITH its pass number, the frames of the pass
/// that shipped)`. Pass numbers are those `emit_program` marks (1, 2,
/// [`CHECKED_PASS`]); a frame pushed before any marker reads as pass 0.
pub fn take_by_pass() -> (Vec<(usize, String, String)>, Frames) {
    let (all, shipped) = take_by_pass_with_context();
    (all.into_iter().map(|(p, n, c, _)| (p, n, c)).collect(), shipped)
}

/// [`take_by_pass`] with each frame's convention context (the floor test's
/// cross-pass comparison keys on it).
pub fn take_by_pass_with_context() -> (Vec<PassFrame>, Frames) {
    let (all, shipped) = parse(drain());
    let shipped: Option<usize> = shipped.and_then(|p| p.parse().ok());
    let in_shipped =
        all.iter().filter(|f| Some(f.0) == shipped).map(|(_, n, c, _)| (n.clone(), c.clone())).collect();
    (all, in_shipped)
}
