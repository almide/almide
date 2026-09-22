//! The certificate bundle — every witness one program's build produced, in
//! one file, so a single `almide-verify bundle` run judges the whole program.
//!
//! Format (version 1), line-oriented, byte-exact:
//!
//! ```text
//! almide-certificate-bundle 1
//! producer <free text>                      metadata, before the first record
//! source <free text>
//! witness <property> <byte-length> <function>
//! <exactly byte-length bytes — the witness, newlines included>
//! uncertified <function> <reason…>
//! ```
//!
//! A `witness` record's bytes are followed by exactly one `\n`. The length
//! prefix, not a delimiter, ends the witness, so no witness byte can be
//! mistaken for framing. Metadata is echoed and never trusted: nothing a
//! producer writes outside a witness can change a verdict. Anything this
//! version does not understand — an unknown metadata key, an unknown
//! property, a short read — is a malformed bundle, never a silent accept.

use crate::{check, Property};

/// The first line of every bundle this verifier reads.
pub const MAGIC: &str = "almide-certificate-bundle 1";

/// The metadata keys version 1 defines.
const METADATA_KEYS: &[&str] = &["producer", "source"];

/// One witness to judge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Witness {
    pub property: Property,
    pub function: String,
    pub bytes: Vec<u8>,
}

/// A parsed bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bundle {
    pub metadata: Vec<(String, String)>,
    pub witnesses: Vec<Witness>,
    /// Functions the producer declares it could NOT certify, with its reason.
    /// A producer claim like the metadata: reported, never judged.
    pub uncertified: Vec<(String, String)>,
}

/// Why a bundle could not be read. Every variant is a refusal to judge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "malformed certificate bundle (line {}): {}", self.line, self.message)
    }
}

/// The verdict of a whole bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Every witness accepted and nothing declared uncertified.
    Certified,
    /// At least one witness rejected — or none present, which certifies
    /// nothing and so cannot be an accept.
    Rejected,
    /// Every witness accepted, but the producer declared functions it could
    /// not certify: the accepted part holds, the whole program is not covered.
    Incomplete,
}

impl Outcome {
    /// The process exit code the CLI reports for this outcome.
    pub fn exit_code(self) -> i32 {
        match self {
            Outcome::Certified => 0,
            Outcome::Rejected => 1,
            Outcome::Incomplete => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Outcome::Certified => "CERTIFIED",
            Outcome::Rejected => "REJECTED",
            Outcome::Incomplete => "INCOMPLETE",
        }
    }
}

/// One judged witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub property: Property,
    pub function: String,
    pub accepted: bool,
}

/// Judge every witness of a bundle.
pub fn verify(bundle: &Bundle) -> (Vec<Verdict>, Outcome) {
    let verdicts: Vec<Verdict> = bundle
        .witnesses
        .iter()
        .map(|w| Verdict {
            property: w.property,
            function: w.function.clone(),
            accepted: check(w.property, &w.bytes),
        })
        .collect();
    let outcome = if verdicts.is_empty() || verdicts.iter().any(|v| !v.accepted) {
        Outcome::Rejected
    } else if !bundle.uncertified.is_empty() {
        Outcome::Incomplete
    } else {
        Outcome::Certified
    };
    (verdicts, outcome)
}

/// A cursor over the bundle bytes that counts lines for error messages.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: usize,
}

impl<'a> Reader<'a> {
    fn err(&self, message: impl Into<String>) -> BundleError {
        BundleError { line: self.line, message: message.into() }
    }

    /// The next `\n`-terminated line (without the `\n`), or `None` at end of
    /// input. A final line without its `\n` is malformed.
    fn next_line(&mut self) -> Result<Option<&'a str>, BundleError> {
        if self.pos >= self.bytes.len() {
            return Ok(None);
        }
        self.line += 1;
        let rest = &self.bytes[self.pos..];
        let end = rest.iter().position(|&b| b == b'\n').ok_or_else(|| self.err("the last line has no terminating newline"))?;
        self.pos += end + 1;
        std::str::from_utf8(&rest[..end]).map(Some).map_err(|_| self.err("a framing line is not UTF-8"))
    }

    /// Exactly `len` witness bytes followed by one `\n`.
    fn take_witness(&mut self, len: usize) -> Result<Vec<u8>, BundleError> {
        let end = self.pos.checked_add(len).filter(|&e| e < self.bytes.len());
        let end = end.ok_or_else(|| self.err(format!("the witness declares {len} bytes but the bundle ends first")))?;
        if self.bytes[end] != b'\n' {
            return Err(self.err(format!("the {len} witness bytes are not followed by a newline")));
        }
        let witness = self.bytes[self.pos..end].to_vec();
        self.line += witness.iter().filter(|&&b| b == b'\n').count() + 1;
        self.pos = end + 1;
        Ok(witness)
    }
}

/// `witness <property> <byte-length> <function>` → (property, length, function).
fn witness_header(r: &Reader, rest: &str) -> Result<(Property, usize, String), BundleError> {
    let mut parts = rest.splitn(3, ' ');
    let (prop, len, function) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let property = Property::from_name(prop).ok_or_else(|| r.err(format!("unknown property `{prop}`")))?;
    let len = len.parse::<usize>().map_err(|_| r.err(format!("`{len}` is not a byte length")))?;
    if function.is_empty() {
        return Err(r.err("a witness record names no function"));
    }
    Ok((property, len, function.to_string()))
}

/// Parse a bundle. Refuses (never guesses) on anything version 1 does not define.
pub fn parse(bytes: &[u8]) -> Result<Bundle, BundleError> {
    let mut r = Reader { bytes, pos: 0, line: 0 };
    match r.next_line()? {
        Some(MAGIC) => {}
        Some(other) => return Err(r.err(format!("expected `{MAGIC}`, found `{other}`"))),
        None => return Err(r.err("the bundle is empty")),
    }
    let mut bundle = Bundle::default();
    while let Some(line) = r.next_line()? {
        let (keyword, rest) = line.split_once(' ').unwrap_or((line, ""));
        match keyword {
            "witness" => {
                let (property, len, function) = witness_header(&r, rest)?;
                let bytes = r.take_witness(len)?;
                bundle.witnesses.push(Witness { property, function, bytes });
            }
            "uncertified" => {
                let (function, reason) = rest.split_once(' ').unwrap_or((rest, ""));
                if function.is_empty() {
                    return Err(r.err("an uncertified record names no function"));
                }
                bundle.uncertified.push((function.to_string(), reason.to_string()));
            }
            key if METADATA_KEYS.contains(&key) => {
                if !bundle.witnesses.is_empty() || !bundle.uncertified.is_empty() {
                    return Err(r.err(format!("metadata `{key}` after the first record")));
                }
                bundle.metadata.push((key.to_string(), rest.to_string()));
            }
            other => return Err(r.err(format!("unknown record `{other}`"))),
        }
    }
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(body: &str) -> Vec<u8> {
        format!("{MAGIC}\n{body}").into_bytes()
    }

    #[test]
    fn a_multi_line_witness_is_framed_by_its_length() {
        let b = parse(&bundle("producer test\nwitness ownership 5 main\nid\nim\n")).expect("a well-formed bundle parses");
        assert_eq!(b.metadata, vec![("producer".into(), "test".into())]);
        assert_eq!(b.witnesses[0].bytes, b"id\nim");
        assert_eq!(verify(&b).1, Outcome::Certified);
    }

    #[test]
    fn no_witness_is_not_an_accept() {
        let b = parse(&bundle("")).expect("a well-formed bundle parses");
        assert_eq!(verify(&b).1, Outcome::Rejected);
    }

    #[test]
    fn an_uncertified_function_makes_the_outcome_incomplete() {
        let b = parse(&bundle("witness names 3 main\n1|1\nuncertified helper outside the subset\n")).expect("a well-formed bundle parses");
        assert_eq!(b.uncertified, vec![("helper".into(), "outside the subset".into())]);
        assert_eq!(verify(&b).1, Outcome::Incomplete);
    }

    #[test]
    fn what_version_one_does_not_define_is_refused() {
        assert!(parse(b"almide-certificate-bundle 2\n").is_err());
        assert!(parse(&bundle("witness ownership 9 main\nid\n")).is_err());
        assert!(parse(&bundle("witness teleport 2 main\nid\n")).is_err());
        assert!(parse(&bundle("signed-by nobody\n")).is_err());
        assert!(parse(&bundle("witness ownership 2 main\nidX")).is_err());
        assert!(parse(&bundle("witness names 3 main\n1|1\nsource late\n")).is_err());
    }
}
