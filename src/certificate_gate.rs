//! The wasm build gate's judge (#4011, #4836): the `almide-verify` LIBRARY's
//! verdict on the certificate bundle a build recorded and the bytes it pins.
//!
//! Owner ruling 2026-11-24: the verifier runs in process on every wasm route
//! (`build`, `run`, `test`, `bench`, the core module of a component), so a
//! build can never ship unjudged because no `almide-verify` binary was
//! installed. The verifier crate links no compiler crate
//! (crates/almide-verify/Cargo.toml), so what judges here is the same
//! independent code the standalone binary runs; `almide verify` still execs
//! that binary.
//!
//! Every outcome that is not an accept is a refusal: a bundle the verifier
//! cannot read (an unknown property, record or version), one that pins no
//! artifact or other bytes, one with no witness, or one with a REJECTed
//! witness. INCOMPLETE (every witness accepted, some function declared
//! uncertified) is refused too unless the caller passes `allow_incomplete`
//! (`--allow-incomplete-verification`, owner ruling 2026-11-24, C-462): then
//! it passes and the caller reports every uncertified function.

use almide_verify::bundle::{self, Outcome};

/// A certificate the judge let through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pass {
    /// Every witness accepted, nothing declared uncertified.
    Certified,
    /// INCOMPLETE, let through by `allow_incomplete`: the uncertified
    /// functions `(function, the producer's reason)`, for the warning the
    /// caller prints and records.
    IncompleteAllowed(Vec<(String, String)>),
}

/// The flag that lets an INCOMPLETE certificate through (C-462).
pub const ALLOW_INCOMPLETE_FLAG: &str = "--allow-incomplete-verification";

/// One `UNCERTIFIED` line per declared function.
pub fn uncertified_lines(uncertified: &[(String, String)]) -> Vec<String> {
    uncertified.iter().map(|(f, why)| format!("UNCERTIFIED  {f}: {why}")).collect()
}

/// `Ok(Pass::Certified)` when the linked verifier CERTIFIES `bundle_text`
/// over `shipped`; `Ok(Pass::IncompleteAllowed)` when it finds it INCOMPLETE
/// and `allow_incomplete` is set; `Err` with the verdict lines otherwise.
pub fn judge(bundle_text: &[u8], shipped: &[u8], allow_incomplete: bool) -> Result<Pass, String> {
    let who = format!("almide-verify {} (linked)", almide_verify::VERSION);
    let parsed = bundle::parse(bundle_text).map_err(|e| format!("{who} could not read this build's certificate: {e}"))?;
    let Some(artifact) = parsed.artifact.as_ref() else {
        return Err(format!("{who}: this build's certificate pins no artifact"));
    };
    let pinned = bundle::artifact_matches(artifact, shipped);
    let (verdicts, outcome) = bundle::verify(&parsed);
    let outcome = if pinned { outcome } else { Outcome::Rejected };
    // Every defined body of the module is a function the verdict names.
    let uncovered = almide_verify::coverage::judge(&parsed, Some(shipped)).err();
    let outcome = if uncovered.is_some() { Outcome::Rejected } else { outcome };
    match outcome {
        Outcome::Certified => Ok(Pass::Certified),
        Outcome::Incomplete if allow_incomplete => Ok(Pass::IncompleteAllowed(parsed.uncertified.clone())),
        Outcome::Incomplete => Err(format!(
            "{who} judged this build's certificate INCOMPLETE: every witness was accepted, but the compiler could not certify {} function(s):\n{}\n  \
             an incomplete certificate is refused by default; `{ALLOW_INCOMPLETE_FLAG}` lets it pass with a warning",
            parsed.uncertified.len(),
            uncertified_lines(&parsed.uncertified).join("\n")
        )),
        Outcome::Rejected => {
            let mut lines: Vec<String> = verdicts
                .iter()
                .filter(|v| !v.accepted)
                .map(|v| format!("REJECT  {:<15}  {}", v.property.name(), v.function))
                .collect();
            if !pinned {
                lines.push(format!(
                    "ARTIFACT  sha256 {}: MISMATCH (the bundle names {})",
                    almide_verify::sha256::hex(shipped),
                    artifact.sha256
                ));
            }
            if let Some(why) = &uncovered {
                lines.push(format!("COVERAGE  {why}"));
            }
            if verdicts.is_empty() {
                lines.push("no witness: a bundle that certifies nothing is not an accept".into());
            }
            Err(format!("{who} judged this build's certificate and did not accept it ({}):\n{}", outcome.label(), lines.join("\n")))
        }
    }
}
