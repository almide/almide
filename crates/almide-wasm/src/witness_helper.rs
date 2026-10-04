//! #2758 (#1696 step 4): the structural witness of an Emitter-built HELPER
//! frame — a recursive type's display body, its equality / ordering body
//! (display.rs `build_helper_body`).
//!
//! A helper is called with blocks its caller holds for the whole call, so
//! its block params are LENT: known objects with no credit here
//! (`param_lent`). Its body is a type walk the emitter writes without the
//! gate's pre-scan, so the recorder alone cannot show it heard every RC
//! site. The certificate is therefore AUDITED against the bytes: the
//! helper's emitted `call`s to the RC entry points are counted — `F_INC`
//! against the recorded shares (`a`), the release functions (`$dec_flat`
//! and every drop helper) against the recorded releases (`d`), the table
//! functions that hand over a droppable result against the recorded
//! births (`i`). Each recorded event is one emitted instruction, so equal
//! totals mean no RC instruction went unrecorded; any difference withdraws
//! the certificate (`<kind>:rc-unrecorded`). A helper emits no exit and no
//! branch hook, so its log is flat, and every event it records is on a
//! temporary born and released at one site.

use super::{WitnessRecorder, push_decline, push_recorded};
use crate::emitter::Emitter;

/// A helper frame's witness, armed only while a sweep collects.
pub(crate) struct HelperWitness {
    name: String,
    kind: &'static str,
    lent: &'static [u32],
}

impl HelperWitness {
    /// `lent`: the helper's block params, in local order.
    pub(crate) fn new(name: String, kind: &'static str, lent: &'static [u32]) -> Option<Self> {
        super::collecting().then_some(Self { name, kind, lent })
    }

    pub(crate) fn arm(&self) -> WitnessRecorder {
        let mut w = WitnessRecorder::new();
        for &l in self.lent {
            w.param_lent(l);
        }
        w
    }
}

/// The function indices whose calls are RC events, read off the helper's
/// Emitter once its body is written (a drop helper may be created by it).
pub(crate) struct Footprint {
    dec: Vec<u32>,
    owned: Vec<u32>,
}

pub(crate) fn footprint(em: &Emitter<'_>) -> Footprint {
    use crate::work::Helper as H;
    let base = em.work.helper_base.get();
    let mut dec: Vec<u32> = em
        .work
        .helpers
        .borrow()
        .iter()
        .enumerate()
        .filter(|(_, h)| {
            matches!(
                h,
                H::DropList { .. }
                    | H::DropShape { .. }
                    | H::DropMapSpine { .. }
                    | H::DropEntries { .. }
                    | H::DropFn { .. }
                    | H::DropCell { .. }
                    | H::DropHttpCall
            )
        })
        .map(|(p, _)| base + p as u32)
        .collect();
    dec.push(crate::F_DEC_FLAT);
    let owned = em
        .calls
        .iter()
        .map(|&i| &em.table.infos[i])
        .filter(|info| info.ret.is_some_and(|t| em.rc_droppable(t)))
        .map(|info| info.wasm_index)
        .collect();
    Footprint { dec, owned }
}

/// The `(a, d, i)` RC calls in the closed function's bytes; `None` when the
/// body cannot be read back.
fn rc_calls(f: &wasm_encoder::Function, fp: &Footprint) -> Option<(usize, usize, usize)> {
    use wasm_encoder::Encode;
    let mut encoded = Vec::new();
    f.encode(&mut encoded);
    // Skip the LEB128 size prefix.
    let pos = encoded.iter().position(|b| b & 0x80 == 0)? + 1;
    let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(&encoded[pos..], 0));
    let mut reader = body.get_operators_reader().ok()?;
    let mut n = (0, 0, 0);
    while !reader.eof() {
        if let wasmparser::Operator::Call { function_index: k } = reader.read().ok()? {
            if k == crate::F_INC {
                n.0 += 1;
            } else if fp.dec.contains(&k) {
                n.1 += 1;
            } else if fp.owned.contains(&k) {
                n.2 += 1;
            }
        }
    }
    Some(n)
}

/// Do the closed body's RC calls match the recorded events exactly?
fn audited(w: &WitnessRecorder, fp: &Footprint, f: &wasm_encoder::Function) -> bool {
    rc_calls(f, fp) == Some(w.event_totals())
}

/// The helper's certificate, or its withdrawal when the bytes hold an RC
/// call the recorder did not hear.
pub(crate) fn finish(hw: Option<HelperWitness>, w: Option<WitnessRecorder>, fp: Option<Footprint>, f: &wasm_encoder::Function) {
    let (Some(hw), Some(w), Some(fp)) = (hw, w, fp) else { return };
    if audited(&w, &fp, f) {
        push_recorded(&hw.name, &w);
    } else {
        push_decline(&hw.name, &format!("{}:rc-unrecorded", hw.kind));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(calls: &[u32]) -> wasm_encoder::Function {
        let mut f = wasm_encoder::Function::new([]);
        for &k in calls {
            f.instructions().i32_const(0).call(k);
        }
        f.instructions().end();
        f
    }

    fn verdict(calls: &[u32], w: &WitnessRecorder) -> bool {
        let fp = Footprint { dec: vec![crate::F_DEC_FLAT], owned: vec![900] };
        audited(w, &fp, &body(calls))
    }

    #[test]
    fn the_certificate_stands_only_when_the_bytes_hold_exactly_the_recorded_rc_calls() {
        let hw = HelperWitness { name: String::new(), kind: "display", lent: &[0] };
        let mut w = hw.arm();
        w.temp_discarded();
        assert_eq!(w.certificate(), "\nid\n");
        // An owned call result released at the site: `call 900` (i) and a
        // `$dec_flat` (d), both recorded.
        assert!(verdict(&[900, crate::F_DEC_FLAT], &w));
        // A release, a share or an owned result the recorder never heard:
        // the certificate is withdrawn, never under-counted.
        for extra in [crate::F_DEC_FLAT, crate::F_INC, 900] {
            assert!(!verdict(&[900, crate::F_DEC_FLAT, extra], &w), "an unrecorded call {extra}");
        }
        // A recorded event with no instruction behind it is withdrawn too.
        assert!(!verdict(&[900], &w));
        // Calls that are no RC event leave the audit alone.
        assert!(verdict(&[7, 900, 8, crate::F_DEC_FLAT], &w));
    }
}
