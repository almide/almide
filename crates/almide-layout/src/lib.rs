//! Block layout — the SINGLE SOURCE (ARCHITECTURE.md §6.6 obligation).
//!
//! Until 2026-08-19 the `[rc][len@4][cap@8][payload@12]` layout lived as
//! comments in the interpreter's heap and in the incumbent wasm renderer's
//! head — and both this session and the incumbent session made mistakes
//! working from those comments. From unit 6 onward, every consumer — the
//! wasm backend's emission, the interpreter's arena, audit digests, docs —
//! derives from the definitions in this crate. A layout change is therefore
//! one edit here plus the deliberate re-pin of `LAYOUT_DIGEST` below;
//! anything else drifting is a compile error or a red gate, never a stale
//! comment.

/// One header field of a heap block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    /// Byte offset from the block base.
    pub offset: u32,
    /// Width in bytes.
    pub width: u32,
    pub doc: &'static str,
}

/// Reference count. Blocks are born with rc = 1.
pub const RC: Field = Field { name: "rc", offset: 0, width: 4, doc: "reference count; blocks are born with rc = 1" };
/// Payload length in bytes (for Str/Bytes: the live byte count).
pub const LEN: Field = Field { name: "len", offset: 4, width: 4, doc: "payload length in bytes" };
/// Payload capacity in bytes (>= len).
pub const CAP: Field = Field { name: "cap", offset: 8, width: 4, doc: "payload capacity in bytes (>= len)" };

/// The full header, in offset order. New fields append here — every
/// consumer that iterates this table picks them up or fails its gate.
pub const HEADER: &[Field] = &[RC, LEN, CAP];

/// First payload byte: the header ends here.
pub const PAYLOAD: u32 = 12;

/// The null/none address: never handed out for a live block, so 0 stays a
/// recognisable null on every consumer (interp arena reserves it; the wasm
/// backend must too).
pub const NULL_ADDR: u32 = 0;

// ── sum layout (payload-relative) ───────────────────────────────────────
//
// Option[T]: `none` IS `NULL_ADDR` — no block, no tag. `some(v)` is a block
// whose payload holds the value slot directly at `OPTION_FIELD`.
// (Consequence, by design: nested Option cannot be represented and must be
// refused by any consumer until a tagged Option layout is ratified.)
//
// Result[T, E]: a block with a 4-byte tag at `SUM_TAG` (0 = Ok, 1 = Err)
// and the value slot at `SUM_FIELD` (8-byte slot, uniform for both sides).
// This shape — tag word, padding, fields — is the one user-defined
// variants will generalise.

/// Option's value slot, payload-relative.
pub const OPTION_FIELD: u32 = 0;
/// Sum tag word, payload-relative (Result: 0 = Ok, 1 = Err).
pub const SUM_TAG: u32 = 0;
/// Tagged sum's value slot, payload-relative (leaves the tag an 8-byte
/// aligned pad so an i64 slot never straddles it).
pub const SUM_FIELD: u32 = 8;

/// Stable digest of the layout definition. Any change to the fields above
/// changes this value and fails the pin test — re-pin ONLY as a deliberate,
/// reviewed layout change (an intentional-change-protocol event).
pub fn layout_digest() -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    for f in HEADER {
        f.name.hash(&mut h);
        f.offset.hash(&mut h);
        f.width.hash(&mut h);
    }
    PAYLOAD.hash(&mut h);
    NULL_ADDR.hash(&mut h);
    OPTION_FIELD.hash(&mut h);
    SUM_TAG.hash(&mut h);
    SUM_FIELD.hash(&mut h);
    h.finish()
}

/// The layout as a markdown table — the docs consumer.
pub fn layout_doc() -> String {
    let mut out = String::from("| field | offset | width | doc |\n|---|---|---|---|\n");
    for f in HEADER {
        out.push_str(&format!("| {} | {} | {} | {} |\n", f.name, f.offset, f.width, f.doc));
    }
    out.push_str(&format!("| payload | {PAYLOAD} | len | first payload byte |\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Header fields are contiguous, non-overlapping, and end at PAYLOAD.
    #[test]
    fn header_is_contiguous_and_ends_at_payload() {
        let mut cursor = 0;
        for f in HEADER {
            assert_eq!(f.offset, cursor, "{} not contiguous", f.name);
            cursor += f.width;
        }
        assert_eq!(cursor, PAYLOAD, "header does not end at PAYLOAD");
    }

    /// The deliberate pin: a layout change must re-pin this constant in the
    /// same commit, making every layout change loud and reviewed.
    #[test]
    fn digest_is_pinned() {
        // Re-pinned 2026-08-19: sum layout added (OPTION_FIELD / SUM_TAG /
        // SUM_FIELD) for unit-6 stage 3 — an intentional-change event, see
        // PORTLOG.md.
        assert_eq!(layout_digest(), 6642309021484683773, "layout changed — re-pin deliberately");
    }

    #[test]
    fn doc_names_every_field() {
        let doc = layout_doc();
        for f in HEADER {
            assert!(doc.contains(f.name));
        }
    }
}
