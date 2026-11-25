//! The native build gate (#4828, #4930; owner ruling 2026-11-24 W2): the
//! certificates the verified native v1 render returns — of the MIR it
//! rendered — are judged by the linked almide-verify library
//! ([`crate::certificate_gate::judge`]) against the exact Rust text that
//! ships, before rustc sees it. A rejection stops the build: the render's own
//! certificate does not hold, which is a compiler bug.

/// The version-5 bundle of a native v1 render: the certificates
/// `(property, function, witness)` of the MIR it rendered, pinned by SHA-256
/// to the Rust text `rust` that the build hands rustc.
pub fn native_bundle(producer: &str, source: &str, rust: &str, certs: &[(String, String, String)]) -> Vec<u8> {
    let line = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut text = format!(
        "almide-certificate-bundle 5\nproducer {}\nsource {}\nartifact sha256 {} render.rs\n",
        line(producer),
        line(source),
        almide_verify::sha256::hex(rust.as_bytes())
    );
    for (property, function, witness) in certs {
        text.push_str(&format!("witness {property} {} {}\n{witness}\n", witness.len(), line(function)));
    }
    text.into_bytes()
}

/// [`crate::certificate_gate::judge`] over a native v1 render's own
/// certificates and text.
pub fn judge_native(producer: &str, source: &str, rust: &str, certs: &[(String, String, String)]) -> Result<(), String> {
    crate::certificate_gate::judge(&native_bundle(producer, source, rust, certs), rust.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::judge_native;

    fn certs(ownership: &str) -> Vec<(String, String, String)> {
        vec![("ownership".to_string(), "main".to_string(), ownership.to_string())]
    }

    /// The control: the judge accepts a balanced witness over the text it is
    /// pinned to, and refuses a leaked or doubled release (the kernel's
    /// verdict) and a witness pinned to other text.
    #[test]
    fn the_native_judge_refuses_a_rejected_or_mispinned_certificate() {
        let rust = "fn main() {}\n";
        assert_eq!(judge_native("t", "s", rust, &certs("id")), Ok(()));
        let leaked = judge_native("t", "s", rust, &certs("i")).expect_err("a leaked object is rejected");
        assert!(leaked.contains("REJECT  ownership"), "{leaked}");
        assert!(judge_native("t", "s", rust, &certs("idd")).is_err(), "a double release is rejected");
        let bundle = super::native_bundle("t", "s", rust, &certs("id"));
        let other = crate::certificate_gate::judge(&bundle, b"fn main() { println!(); }\n").expect_err("other text");
        assert!(other.contains("MISMATCH"), "{other}");
        assert!(judge_native("t", "s", rust, &[]).is_err(), "a bundle with no witness is not an accept");
    }
}
