//! The `coverage` record of a version-8 bundle: which function each defined
//! body of the shipped wasm module is, so that no function the artifact
//! contains is left out of the verdict.
//!
//! ```text
//! coverage <positions> <unshipped>
//! <one line per defined body of the artifact, in code-section order:
//!  `*`, or the function names (tab-separated) whose body it is>
//! <one line per unshipped function: a name whose body the build pruned>
//! ```
//!
//! What the verifier checks, against the artifact's own bytes:
//! - the number of position lines is the number of entries of the artifact's
//!   code section (read here: section framing and one LEB128, no decoder);
//! - every name in the record is a function the bundle has a witness or an
//!   `uncertified` record for;
//! - every function with an `ownership` witness is in the record, at a
//!   position or as unshipped.
//!
//! What it trusts: a `*` position is the producer's claim that the body is a
//! runtime routine (an allocator, a release, a helper the emitter writes),
//! not a source function. A function placed `unshipped` is the producer's
//! claim that its body was pruned. Neither can be checked without decoding
//! the bodies.

/// A parsed `coverage` record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Coverage {
    /// Per defined body of the artifact, the functions it is (empty: `*`).
    pub positions: Vec<Vec<String>>,
    /// Functions whose bodies did not ship.
    pub unshipped: Vec<String>,
}

/// Parse a `coverage` header's two counts.
pub(crate) fn header(rest: &str) -> Option<(usize, usize)> {
    let (n, m) = rest.split_once(' ')?;
    Some((n.parse().ok()?, m.parse().ok()?))
}

/// One position line: `*`, or tab-separated names (none empty).
pub(crate) fn position(line: &str) -> Option<Vec<String>> {
    if line == "*" {
        return Some(Vec::new());
    }
    let names: Vec<String> = line.split('\t').map(str::to_string).collect();
    names.iter().all(|n| !n.is_empty() && n != "*").then_some(names)
}

/// The number of defined function bodies of a core wasm module, or `None`
/// when `bytes` is not a well-framed core module.
pub(crate) fn defined_bodies(bytes: &[u8]) -> Option<usize> {
    if bytes.get(..8)? != b"\0asm\x01\0\0\0" {
        return None;
    }
    let mut at = 8;
    let mut count = 0;
    while at < bytes.len() {
        let id = bytes[at];
        let (size, n) = leb(bytes, at + 1)?;
        let start = at + 1 + n;
        let end = start.checked_add(size)?;
        if end > bytes.len() {
            return None;
        }
        if id == 10 {
            count = leb(bytes, start)?.0;
        }
        at = end;
    }
    Some(count)
}

/// An unsigned LEB128 (at most 5 bytes) at `at`: its value and its length.
fn leb(bytes: &[u8], at: usize) -> Option<(usize, usize)> {
    let mut v: u64 = 0;
    for i in 0..5 {
        let b = *bytes.get(at + i)?;
        v |= u64::from(b & 0x7f) << (7 * i);
        if b & 0x80 == 0 {
            return usize::try_from(v).ok().map(|v| (v, i + 1));
        }
    }
    None
}

/// Is `bytes` a wasm binary (any version)?
pub fn is_wasm(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\0asm")
}

/// Judge the coverage of `bundle` over `artifact` (the shipped bytes, when
/// the caller has them). `Err` names why the bundle does not cover it.
///
/// A non-wasm artifact (a native certificate's binary) has no code section
/// to cover and is not judged here. A wasm artifact that is not a core
/// module, or a bundle over one with no `coverage` record, is refused.
pub fn judge(bundle: &crate::bundle::Bundle, artifact: Option<&[u8]>) -> Result<(), String> {
    use crate::Property;
    if let Some(a) = artifact {
        if !is_wasm(a) {
            return Ok(());
        }
    }
    let Some(cov) = bundle.coverage.as_ref() else {
        return match artifact {
            Some(_) => Err("the bundle names no coverage of the wasm module it pins (a version-8 `coverage` record)".into()),
            None => Ok(()),
        };
    };
    if let Some(a) = artifact {
        let n = defined_bodies(a).ok_or("the pinned artifact is not a well-framed core wasm module")?;
        if n != cov.positions.len() {
            return Err(format!("the coverage names {} bodies; the artifact defines {n}", cov.positions.len()));
        }
    }
    let mut known: std::collections::HashSet<&str> = bundle.uncertified.iter().map(|(f, _)| f.as_str()).collect();
    known.extend(bundle.witnesses.iter().filter(|w| matches!(w.property, Property::Ownership | Property::RcSig)).map(|w| w.function.as_str()));
    let placed: std::collections::HashSet<&str> =
        cov.positions.iter().flatten().chain(cov.unshipped.iter()).map(String::as_str).collect();
    if let Some(n) = placed.iter().find(|n| !known.contains(**n)) {
        return Err(format!("the coverage names `{n}`, which no witness or uncertified record covers"));
    }
    if let Some(w) = bundle.witnesses.iter().find(|w| w.property == Property::Ownership && !placed.contains(w.function.as_str())) {
        return Err(format!("`{}` is certified but the coverage places it at no body", w.function));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_code_section_count_is_read_off_the_framing() {
        // A type section, then a code section of two (empty) bodies.
        let m = b"\0asm\x01\0\0\0\x01\x04\x01\x60\0\0\x0a\x07\x02\x02\0\x0b\x02\0\x0b";
        assert_eq!(defined_bodies(m), Some(2));
        assert_eq!(defined_bodies(b"\0asm\x01\0\0\0"), Some(0));
        assert_eq!(defined_bodies(b"\0asm\x0d\0\x01\0"), None, "a component is not a core module");
        assert_eq!(defined_bodies(&m[..m.len() - 1]), None, "a section past the end");
    }
}
