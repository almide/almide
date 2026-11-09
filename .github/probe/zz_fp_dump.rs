// Throwaway (probe only): every ledger row's implementation fingerprint.
include!("zz_src.rs");

#[test]
fn zz_dump_fingerprints() {
    let fp = Sources::load();
    let text = std::fs::read_to_string(root().join("proofs/stdlib-inspection.toml")).unwrap();
    let mut out = String::new();
    for line in text.lines() {
        if let Some(n) = line.strip_prefix("name = \"").and_then(|l| l.strip_suffix('"')) {
            out.push_str(&format!("{n}\t{}\n", impl_fingerprint(&fp, n).unwrap_or_default()));
        }
    }
    std::fs::write(std::env::var("FP_OUT").unwrap(), out).unwrap();
}
