//! #4106 family gate (R-hash-06) and security table row 20: the keyed-digest
//! family is a MATRIX, never a point, and its shapes make the wrong primitive
//! unwritable. HMAC exists for {sha256, sha512} × {compute, `is_valid_`} — 4
//! cells, each with exactly the declared signature — and the constant-time
//! compare it rests on exists for the {Bytes, String} pair.
//!
//! The shapes the row rests on, read from the DECLARED surface
//! (`almide compile <module> --json`, the interface every caller type-checks
//! against), not from source text:
//!
//! - the key and the tag are `Secret[Bytes]`: the tag prints as `Secret(***)`
//!   and its only `==` is the constant-time one (C-426), so comparing a MAC
//!   with a leaking `==` needs a visible `secret.reveal`;
//! - no `_hex` cell: a String-returning MAC is a value whose `==` leaks
//!   timing, so the family has none (the hex of a tag is
//!   `hex.encode(secret.reveal(tag))`);
//! - `is_valid_*` takes the tag as `Secret[Bytes]`, and its body compares
//!   through `bytes.is_equal_constant_time`, never `==`.
//!
//! Intentional omissions (shrink-only — a cell may leave this list only by
//! being added to the matrix, with its RFC vectors, in the same PR): the
//! `_hex` cells, the hex verify cells, and HMAC over the broken digests. Any
//! other `hmac` cell outside the matrix fails here too, so the family cannot
//! grow point-wise.
//!
//! The rules are pure functions over the surface and the module text, so the
//! negative controls below run them on a forged surface and see them fail.

use std::collections::BTreeMap;
use std::process::Command;

use serde_json::Value;

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn stdlib(name: &str) -> String {
    let p = root().join("stdlib").join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// An interface type as source spells it; a nominal type by its bare name.
fn ty(t: &Value) -> String {
    let inner = |k: &str| ty(&t[k]);
    match t["kind"].as_str().unwrap_or("?") {
        "int" => "Int".into(),
        "bool" => "Bool".into(),
        "string" => "String".into(),
        "unit" => "Unit".into(),
        "bytes" => "Bytes".into(),
        "named" => {
            let full = t["name"].as_str().unwrap_or("?");
            let name = full.rsplit('.').next().unwrap_or(full).to_string();
            match t["args"].as_array() {
                Some(a) if !a.is_empty() => format!("{name}[{}]", a.iter().map(ty).collect::<Vec<_>>().join(", ")),
                _ => name,
            }
        }
        "list" => format!("List[{}]", inner("inner")),
        "result" => format!("Result[{}, {}]", inner("ok"), inner("err")),
        "option" => format!("Option[{}]", inner("inner")),
        other => format!("<{other}>"),
    }
}

/// `name` -> `name(params) -> ret` for every public fn of `module`, read once
/// per process so the tests below judge one surface.
fn declared(module: &str) -> BTreeMap<String, String> {
    static CACHE: std::sync::Mutex<BTreeMap<String, BTreeMap<String, String>>> = std::sync::Mutex::new(BTreeMap::new());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.entry(module.to_string()).or_insert_with(|| read_surface(module)).clone()
}

fn read_surface(module: &str) -> BTreeMap<String, String> {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["compile", module, "--json"])
        .current_dir(root())
        .output()
        .expect("almide compile");
    assert!(o.status.success(), "almide compile {module} --json: {}", String::from_utf8_lossy(&o.stderr));
    let v: Value = serde_json::from_slice(&o.stdout).expect("interface json");
    let mut out = BTreeMap::new();
    for f in v["functions"].as_array().into_iter().flatten() {
        let name = f["name"].as_str().unwrap_or("").to_string();
        if name.starts_with("__") {
            continue;
        }
        let params = f["params"]
            .as_array()
            .map(|a| a.iter().map(|p| format!("{}: {}", p["name"].as_str().unwrap_or("_"), ty(&p["type"]))).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        out.insert(name.clone(), format!("{name}({params}) -> {}", ty(&f["return"])));
    }
    out
}

/// The digests the HMAC matrix covers.
const HMAC_ALGS: &[&str] = &["sha256", "sha512"];

/// Shrink-only: HMAC cells deliberately NOT provided. A `_hex` cell answers
/// a String MAC, whose `==` returns at the first differing byte (row 20);
/// `is_valid_hmac_*_hex` would also give `_hex` a second meaning (the
/// encoding of an INPUT, R-hash-05); md5 and sha1 are not in the stdlib.
const INTENTIONALLY_OMITTED: &[&str] = &[
    "hmac_sha256_hex",
    "hmac_sha512_hex",
    "is_valid_hmac_sha256_hex",
    "is_valid_hmac_sha512_hex",
    "hmac_md5",
    "hmac_sha1",
];

/// Every signature the family rule requires, derived from the declared axes.
fn required_cells() -> Vec<(&'static str, String, String)> {
    let mut cells = Vec::new();
    for alg in HMAC_ALGS {
        let f = format!("hmac_{alg}");
        cells.push(("hash", f.clone(), format!("{f}(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]")));
        let v = format!("is_valid_hmac_{alg}");
        cells.push(("hash", v.clone(), format!("{v}(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool")));
    }
    let ct = "is_equal_constant_time".to_string();
    cells.push(("bytes", ct.clone(), "is_equal_constant_time(a: Bytes, b: Bytes) -> Bool".to_string()));
    cells.push(("string", ct, "is_equal_constant_time(a: String, b: String) -> Bool".to_string()));
    cells
}

/// The family rule over the three declared surfaces: every required cell
/// present with its exact signature, no omitted cell, no `hmac` cell outside
/// the matrix, and no `hmac` cell whose result is not a `Secret`.
fn family_problems(
    hash: &BTreeMap<String, String>,
    bytes: &BTreeMap<String, String>,
    string: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (m, name, sig) in required_cells() {
        let surface = match m {
            "hash" => hash,
            "bytes" => bytes,
            _ => string,
        };
        match surface.get(&name) {
            None => problems.push(format!("{m}: cell missing: {sig}")),
            Some(got) if *got != sig => problems.push(format!("{m}: `{got}` must be `{sig}`")),
            _ => {}
        }
    }
    let declared: Vec<String> =
        HMAC_ALGS.iter().flat_map(|a| [format!("hmac_{a}"), format!("is_valid_hmac_{a}")]).collect();
    for (name, sig) in hash {
        if !name.contains("hmac") {
            continue;
        }
        if INTENTIONALLY_OMITTED.contains(&name.as_str()) {
            problems.push(format!("hash: `{name}` is an intentional omission (R-hash-06, row 20)"));
        } else if !declared.contains(name) {
            problems.push(format!("hash: `{name}` is outside the HMAC matrix — extend HMAC_ALGS, not one cell"));
        }
        if !name.starts_with("is_valid_") && !sig.ends_with("-> Secret[Bytes]") {
            problems.push(format!("hash: `{sig}` answers a MAC that is not a Secret — its `==` would leak timing"));
        }
    }
    problems
}

/// The secret sources: a stdlib read whose value is a credential answers it
/// as a `Secret`, so a token compared with a leaking `==` is not writable
/// without a visible `secret.reveal` (row 20, C-438).
const SECRET_SOURCES: &[(&str, &str, &str)] = &[("env", "secret", "secret(name: String) -> Option[Secret[String]]")];

fn source_problems(surface: &dyn Fn(&str) -> BTreeMap<String, String>) -> Vec<String> {
    let mut problems = Vec::new();
    for (m, name, sig) in SECRET_SOURCES {
        match surface(m).get(*name) {
            None => problems.push(format!("{m}: secret source missing: {sig}")),
            Some(got) if got != sig => problems.push(format!("{m}: `{got}` must be `{sig}`")),
            _ => {}
        }
    }
    problems
}

/// The body of `fn name(` in `src`: from the declaration to the next blank line.
fn body<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let at = src.find(&format!("\nfn {name}("))? + 1;
    let end = src[at..].find("\n\n").map(|e| at + e).unwrap_or(src.len());
    Some(&src[at..end])
}

/// Each `is_valid_` cell compares through the constant-time compare and never
/// through `==` / `!=` (C-423): the verify cell is where the family answers
/// the timing question, so its body is held to it.
fn verify_problems(hash_src: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for alg in HMAC_ALGS {
        let name = format!("is_valid_hmac_{alg}");
        let Some(b) = body(hash_src, &name) else {
            problems.push(format!("hash: no body for `{name}`"));
            continue;
        };
        let rhs = b.split_once(" = ").map(|(_, r)| r).unwrap_or("");
        if !rhs.contains("bytes.is_equal_constant_time(") {
            problems.push(format!("hash: `{name}` does not compare through bytes.is_equal_constant_time"));
        }
        if rhs.contains("==") || rhs.contains("!=") {
            problems.push(format!("hash: `{name}` compares with `==` / `!=`, which returns at the first differing byte"));
        }
    }
    problems
}

#[test]
fn the_hmac_family_is_the_full_matrix_and_nothing_else() {
    let problems = family_problems(&declared("hash"), &declared("bytes"), &declared("string"));
    assert!(
        problems.is_empty(),
        "R-hash-06 / row 20: the HMAC family is {{sha256, sha512}} × {{compute, is_valid_}} over \
         Secret[Bytes] keys and tags, plus is_equal_constant_time on {{Bytes, String}} (update the rule \
         in L2-hash.md and this gate in the same PR):\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_secret_sources_answer_a_secret() {
    let problems = source_problems(&|m| declared(m));
    assert!(problems.is_empty(), "row 20 / C-438:\n{}", problems.join("\n"));
}

#[test]
fn the_verify_cells_compare_in_constant_time() {
    let problems = verify_problems(&stdlib("hash.almd"));
    assert!(problems.is_empty(), "row 20 / C-423:\n{}", problems.join("\n"));
}

/// Negative controls: the gates must FAIL on a surface with one cell removed,
/// with an omitted `_hex` cell added, with a MAC answered as plain `Bytes`,
/// with a plain-`Bytes` tag, with a point-wise extra cell, and on a verify
/// body that uses `==` — otherwise a green run proves nothing.
#[test]
fn control_the_gates_fail_on_each_forged_shape() {
    let (hash, bytes, string) = (declared("hash"), declared("bytes"), declared("string"));
    let base = family_problems(&hash, &bytes, &string);
    assert!(base.is_empty(), "control baseline must be clean:\n{}", base.join("\n"));
    let fails = |h: &BTreeMap<String, String>, s: &BTreeMap<String, String>, needle: &str| {
        family_problems(h, &bytes, s).iter().any(|p| p.contains(needle))
    };

    let mut dropped = hash.clone();
    dropped.remove("is_valid_hmac_sha512");
    assert!(fails(&dropped, &string, "cell missing"), "control: dropping a cell must fail the gate");

    let mut no_ct = string.clone();
    no_ct.remove("is_equal_constant_time");
    assert!(fails(&hash, &no_ct, "cell missing"), "control: dropping the String compare must fail");

    let mut hex = hash.clone();
    hex.insert("hmac_sha256_hex".into(), "hmac_sha256_hex(key: Secret[Bytes], b: Bytes) -> String".into());
    assert!(fails(&hex, &string, "intentional omission"), "control: a `_hex` cell must fail the gate");
    assert!(fails(&hex, &string, "not a Secret"), "control: a String MAC must fail the gate");

    let mut plain = hash.clone();
    plain.insert("hmac_sha256".into(), "hmac_sha256(key: Secret[Bytes], b: Bytes) -> Bytes".into());
    assert!(fails(&plain, &string, "must be"), "control: a plain-Bytes MAC must fail the gate");
    assert!(fails(&plain, &string, "not a Secret"), "control: a plain-Bytes MAC must fail the gate");

    let mut plain_key = hash.clone();
    plain_key.insert("hmac_sha512".into(), "hmac_sha512(key: Bytes, b: Bytes) -> Secret[Bytes]".into());
    assert!(fails(&plain_key, &string, "must be"), "control: a plain-Bytes key must fail the gate");

    let mut plain_tag = hash.clone();
    plain_tag.insert(
        "is_valid_hmac_sha256".into(),
        "is_valid_hmac_sha256(key: Secret[Bytes], b: Bytes, tag: Bytes) -> Bool".into(),
    );
    assert!(fails(&plain_tag, &string, "must be"), "control: a plain-Bytes tag must fail the gate");

    let mut pointwise = hash.clone();
    pointwise.insert("hmac_sha384".into(), "hmac_sha384(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]".into());
    assert!(fails(&pointwise, &string, "outside the HMAC matrix"), "control: a point-wise extra cell must fail");

    let plain_env = |m: &str| {
        let mut s = declared(m);
        if m == "env" {
            s.insert("secret".into(), "secret(name: String) -> Option[String]".into());
        }
        s
    };
    assert!(
        source_problems(&plain_env).iter().any(|p| p.contains("must be")),
        "control: env.secret answering a plain String must fail the gate"
    );

    let src = stdlib("hash.almd");
    let base = verify_problems(&src);
    assert!(base.is_empty(), "control baseline must be clean:\n{}", base.join("\n"));
    let eq = src.replacen(
        "bytes.is_equal_constant_time(\n  secret.reveal(hmac_sha256(key, b)),\n  secret.reveal(tag),\n)",
        "secret.reveal(hmac_sha256(key, b)) == secret.reveal(tag)",
        1,
    );
    assert_ne!(eq, src, "control: the forged verify body must differ from the source");
    assert!(
        verify_problems(&eq).iter().any(|p| p.contains("is_valid_hmac_sha256")),
        "control: a verify cell that uses `==` must fail the gate"
    );
}
