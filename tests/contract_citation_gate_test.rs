//! The contract-citation gate (`scripts/check-contract-citations.sh`, #2406) can fail.
//!
//! The gate's job is to refuse a `C-NNN` spelled anywhere in the tree that names no
//! contract, and a `C-` token whose id is not three digits (a line number wearing a
//! contract's clothes, #2403). A gate that has never been seen red is possibly decorative
//! (proofs/gate-verification.toml), so this test forges a scan root — a ledger with two
//! ids and one source file — and asserts each verdict direction:
//!
//!   - a well-formed id absent from the ledger        -> exit 1, named with file:line
//!   - a four-digit id and a one-digit id             -> exit 1, named as malformed
//!   - a clean root, with `RFC-7386` prose and a
//!     `C-01a`-shaped non-token as decoys              -> exit 0
//!
//! The forged ids are ASSEMBLED at runtime rather than spelled here: this file is itself
//! in the gate's scan set, and a literal dangling id in the test source would be the
//! gate's first true positive.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A fresh scan root under the OS temp dir: `docs/contracts/contracts.toml` with the given
/// ids, plus `stdlib/probe.almd` carrying `body`.
fn forge_root(tag: &str, ledger_ids: &[&str], body: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "almide-contract-citation-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("docs/contracts")).unwrap();
    fs::create_dir_all(root.join("stdlib")).unwrap();
    let mut ledger = String::new();
    for id in ledger_ids {
        ledger.push_str(&format!(
            "[[contract]]\nid        = \"{id}\"\ntitle     = \"t\"\nstatus    = \"active\"\n\n"
        ));
    }
    fs::write(root.join("docs/contracts/contracts.toml"), ledger).unwrap();
    fs::write(root.join("stdlib/probe.almd"), body).unwrap();
    root
}

fn run_gate(root: &Path) -> (i32, String) {
    let out = Command::new("bash")
        .arg("scripts/check-contract-citations.sh")
        .env("CONTRACT_CITATION_ROOT", root)
        .current_dir(repo_root())
        .output()
        .expect("spawn bash scripts/check-contract-citations.sh");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

/// `C-` + digits, assembled so the literal never appears in this file.
fn cid(n: &str) -> String {
    format!("C{}{n}", '-')
}

#[test]
fn a_well_formed_citation_absent_from_the_ledger_fails_the_gate() {
    let dangling = cid("904");
    let body = format!(
        "// a comment\n// the silent ok(\"\") class (the fuzz {dangling} class)\nfn f() -> Int = 1\n"
    );
    let root = forge_root("dangling", &["C-001", "C-002"], &body);
    let (code, text) = run_gate(&root);
    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        code, 1,
        "a dangling citation must be exit 1; output:\n{text}"
    );
    assert!(
        text.contains(&format!(
            "stdlib/probe.almd:2: `{dangling}` names no contract"
        )),
        "the verdict must name file:line and the id; output:\n{text}"
    );
    assert!(
        text.contains("1 dangling citation"),
        "the summary must count the one dangling citation; output:\n{text}"
    );
}

#[test]
fn a_four_digit_or_one_digit_id_is_reported_as_malformed() {
    let four = cid("1063");
    let one = cid("1");
    let body = format!("// see {four}\n// and {one}\n// and the real C-001\n");
    let root = forge_root("malformed", &["C-001"], &body);
    let (code, text) = run_gate(&root);
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 1, "a malformed id must be exit 1; output:\n{text}");
    assert!(
        text.contains(&format!(
            "stdlib/probe.almd:1: `{four}` is not a contract id"
        )) && text.contains("line number"),
        "the four-digit id must be named as the line-number class; output:\n{text}"
    );
    assert!(
        text.contains(&format!(
            "stdlib/probe.almd:2: `{one}` is not a contract id"
        )),
        "the one-digit id must be named as malformed; output:\n{text}"
    );
    assert!(
        text.contains("2 malformed, 0 dangling"),
        "the summary must count two malformed and no dangling; output:\n{text}"
    );
}

#[test]
fn a_clean_root_with_the_known_decoys_passes() {
    // RFC-7386 is the known false positive of a bare `C-` search (#2403); `C-01a` is a
    // letter glued onto digits, not a token; a real id resolves.
    let body = "// RFC-7386 merge patch; not C-01a either; contract C-002 is real\n";
    let root = forge_root("clean", &["C-001", "C-002"], body);
    let (code, text) = run_gate(&root);
    let _ = fs::remove_dir_all(&root);
    assert_eq!(code, 0, "a clean root must be exit 0; output:\n{text}");
    // Three citations: the probe's C-002 plus the ledger's own two id rows (the ledger
    // is in the scan set; its ids resolve trivially). The decoys add nothing.
    assert!(
        text.contains("contract-citations: OK — 3 citations in 2 files"),
        "exactly three citations must be counted, none from the decoys; output:\n{text}"
    );
}

#[test]
fn the_real_tree_is_clean_and_the_gate_is_wired() {
    // The gate's own verdict on the checked-in tree, and the two consumers named in
    // proofs/gate-verification.toml actually invoke it.
    let (code, text) = run_gate(&repo_root());
    assert_eq!(
        code, 0,
        "the tree carries a dangling or malformed citation:\n{text}"
    );
    for consumer in [".github/workflows/ci.yml", "lefthook.yml"] {
        let src = fs::read_to_string(repo_root().join(consumer)).unwrap();
        assert!(
            src.contains("scripts/check-contract-citations.sh"),
            "{consumer} must invoke the citation gate"
        );
    }
}
