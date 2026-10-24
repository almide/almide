//! #4106 family gate (R-hash-06): the keyed-digest family is a MATRIX, never
//! a point. HMAC exists for {sha256, sha512} × {raw, `_hex`} × {compute,
//! `is_valid_`} — 8 cells, each with exactly the declared signature — and the
//! constant-time compare it rests on exists for the {Bytes, String} pair.
//!
//! Intentional omissions (shrink-only — a cell may leave this list only by
//! being added to the matrix, with its RFC vectors, in the same PR): HMAC over
//! the broken digests, `hmac_md5` / `hmac_sha1`. Any other `hmac` cell outside
//! the matrix fails here too, so the family cannot grow point-wise.
//!
//! The rule is a pure function over the module texts so the negative control
//! below can run it on a text with one cell removed and see it fail.

fn stdlib(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// The digests the HMAC matrix covers, with the output length of each.
const HMAC_ALGS: &[&str] = &["sha256", "sha512"];

/// Shrink-only: HMAC cells deliberately NOT provided.
const INTENTIONALLY_OMITTED: &[&str] = &["hmac_md5", "hmac_sha1"];

/// Every signature the family rule requires, derived from the declared axes.
fn required_cells() -> Vec<(&'static str, String)> {
    let mut cells = Vec::new();
    for alg in HMAC_ALGS {
        cells.push(("hash", format!("fn hmac_{alg}(key: Bytes, b: Bytes) -> Bytes")));
        cells.push(("hash", format!("fn hmac_{alg}_hex(key: Bytes, b: Bytes) -> String")));
        cells.push(("hash", format!("fn is_valid_hmac_{alg}(key: Bytes, b: Bytes, tag: Bytes) -> Bool")));
        cells.push(("hash", format!("fn is_valid_hmac_{alg}_hex(key: Bytes, b: Bytes, tag: String) -> Bool")));
    }
    cells.push(("bytes", "fn is_equal_constant_time(a: Bytes, b: Bytes) -> Bool".to_string()));
    cells.push(("string", "fn is_equal_constant_time(a: String, b: String) -> Bool".to_string()));
    cells
}

/// The family rule over the three module texts: every required cell present,
/// no omitted cell present, no `hmac` cell outside the matrix.
fn family_problems(hash: &str, bytes: &str, string: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for (m, sig) in required_cells() {
        let src = match m {
            "hash" => hash,
            "bytes" => bytes,
            _ => string,
        };
        if !src.lines().any(|l| l.starts_with(&sig)) {
            problems.push(format!("{m}: cell missing: {sig}"));
        }
    }
    let declared: Vec<String> = HMAC_ALGS
        .iter()
        .flat_map(|a| {
            [
                format!("hmac_{a}"),
                format!("hmac_{a}_hex"),
                format!("is_valid_hmac_{a}"),
                format!("is_valid_hmac_{a}_hex"),
            ]
        })
        .collect();
    for line in hash.lines() {
        let Some(rest) = line.strip_prefix("fn ") else { continue };
        let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !name.contains("hmac") {
            continue;
        }
        if INTENTIONALLY_OMITTED.contains(&name.as_str()) {
            problems.push(format!("hash: `{name}` is an intentional omission (R-hash-06)"));
        } else if !declared.contains(&name) {
            problems.push(format!("hash: `{name}` is outside the HMAC matrix — extend HMAC_ALGS, not one cell"));
        }
    }
    problems
}

#[test]
fn the_hmac_family_is_the_full_matrix_and_nothing_else() {
    let problems = family_problems(&stdlib("hash.almd"), &stdlib("bytes.almd"), &stdlib("string.almd"));
    assert!(
        problems.is_empty(),
        "R-hash-06: the HMAC family is {{sha256, sha512}} × {{raw, _hex}} × {{compute, is_valid_}} plus \
         is_equal_constant_time on {{Bytes, String}} (update the rule in L2-hash.md and this gate in the \
         same PR):\n{}",
        problems.join("\n")
    );
}

/// Negative control: the gate must FAIL on a text with one cell removed, on a
/// text that adds an omitted cell, and on a point-wise extra cell — otherwise a
/// green run proves nothing.
#[test]
fn control_the_gate_fails_when_a_cell_is_dropped_or_added_pointwise() {
    let hash = stdlib("hash.almd");
    let (bytes, string) = (stdlib("bytes.almd"), stdlib("string.almd"));
    let dropped = hash.replace("fn is_valid_hmac_sha512_hex(", "fn removed_cell(");
    assert!(
        family_problems(&dropped, &bytes, &string).iter().any(|p| p.contains("is_valid_hmac_sha512_hex")),
        "control: dropping a cell must fail the gate"
    );
    let no_ct = string.replace("fn is_equal_constant_time(", "fn removed_cell(");
    assert!(!family_problems(&hash, &bytes, &no_ct).is_empty(), "control: dropping the String cell must fail");
    let omitted = format!("{hash}\nfn hmac_sha1(key: Bytes, b: Bytes) -> Bytes = key\n");
    assert!(
        family_problems(&omitted, &bytes, &string).iter().any(|p| p.contains("intentional omission")),
        "control: adding an omitted cell must fail the gate"
    );
    let pointwise = format!("{hash}\nfn hmac_sha384(key: Bytes, b: Bytes) -> Bytes = key\n");
    assert!(
        family_problems(&pointwise, &bytes, &string).iter().any(|p| p.contains("outside the HMAC matrix")),
        "control: a point-wise extra cell must fail the gate"
    );
}
