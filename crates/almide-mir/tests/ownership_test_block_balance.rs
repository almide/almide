//! Ownership balance for TEST-BLOCK bodies — the class the kernel-proven
//! checker caught and nothing local did.
//!
//! `proofs/corpus-wall.sh` hands every in-profile function's ownership
//! certificate to the Coq-extracted checker, which accepts iff the refcount
//! run neither faults nor leaks. That checker needs an opam/coqc toolchain, so
//! in a working copy without one the whole PCC chain is skipped — the local
//! `WALL OK` / `RATCHET OK` lines say nothing about it. A leak reachable only
//! from a `test` block stayed invisible that way until a push went red.
//!
//! The shape: inside a test block the L9 fork keeps `!` as UNWRAP instead of
//! instantiating the fallible HOF form, so `list.map(xs, (x) => f(x)!)` takes
//! `lower_scalar_ok_payload_unwrap` — which built the callee's heap `Result`,
//! read the scalar payload at @12, and returned WITHOUT releasing the block.
//! One abandoned Result per element, and the witness was a bare `i`.
//!
//! This mirrors the Coq `check` (proofs/OwnershipChecker.v): `i`/`a` = +1,
//! `d`/`m` = −1, `r` = −1 valid only at rc 1, `b` = a use needing rc > 0,
//! `(..)` = a loop body that must PRESERVE rc, `{a|b}` = a one-shot branch
//! whose arms must AGREE. It is a pre-flight, not the trusted checker — the
//! `mirrors_the_kernel_examples` test below pins it against the same examples
//! the extracted checker self-tests on, so drift shows up here rather than in
//! CI.

/// One certificate line -> `None` if the kernel would accept it.
fn reject_reason(line: &str) -> Option<String> {
    #[derive(Clone, Copy, PartialEq)]
    enum Op {
        Inc,
        Dec,
        Reuse,
        Borrow,
    }
    fn op_of(c: char) -> Option<Op> {
        match c.to_ascii_lowercase() {
            'i' | 'a' => Some(Op::Inc),
            'd' | 'm' => Some(Op::Dec),
            'r' => Some(Op::Reuse),
            'b' => Some(Op::Borrow),
            _ => None,
        }
    }
    fn exec(ops: &[Op], mut rc: i64) -> Option<i64> {
        for op in ops {
            match op {
                Op::Inc => rc += 1,
                Op::Dec => {
                    if rc <= 0 {
                        return None;
                    }
                    rc -= 1;
                }
                Op::Reuse => {
                    if rc != 1 {
                        return None;
                    }
                    rc = 0;
                }
                Op::Borrow => {
                    if rc <= 0 {
                        return None;
                    }
                }
            }
        }
        Some(rc)
    }
    fn ops_of(s: &str) -> Vec<Op> {
        s.chars().filter_map(op_of).collect()
    }

    let mut rc: i64 = 0;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '(' => {
                let Some(end) = chars[i..].iter().position(|c| *c == ')').map(|p| i + p) else {
                    return Some("unclosed loop".into());
                };
                let body: String = chars[i + 1..end].iter().collect();
                match exec(&ops_of(&body), rc) {
                    Some(r) if r == rc => {}
                    Some(r) => return Some(format!("loop does not preserve rc ({rc} -> {r})")),
                    None => return Some("loop body faults".into()),
                }
                i = end + 1;
            }
            '{' => {
                let Some(end) = chars[i..].iter().position(|c| *c == '}').map(|p| i + p) else {
                    return Some("unclosed branch".into());
                };
                let inner: String = chars[i + 1..end].iter().collect();
                let (t, e) = inner.split_once('|').unwrap_or((inner.as_str(), ""));
                match (exec(&ops_of(t), rc), exec(&ops_of(e), rc)) {
                    (Some(a), Some(b)) if a == b => rc = a,
                    (Some(a), Some(b)) => {
                        return Some(format!("branch arms disagree ({a} vs {b})"))
                    }
                    _ => return Some("branch arm faults".into()),
                }
                i = end + 1;
            }
            c => {
                if let Some(op) = op_of(c) {
                    match exec(&[op], rc) {
                        Some(r) => rc = r,
                        None => return Some("fault (double-free / use-after-free)".into()),
                    }
                }
                i += 1;
            }
        }
    }
    (rc != 0).then(|| format!("leak: rc = {rc}"))
}

/// The same witnesses the extracted checker self-tests on in CI. If the Coq
/// semantics move, this fails first and names the drift.
#[test]
fn mirrors_the_kernel_examples() {
    for (w, accept) in [
        ("iadd", true),
        ("i", false),
        ("i{a|a}dd", true),
        ("i{a|}d", false),
        ("iidd", true),
        ("idd", false),
    ] {
        assert_eq!(
            reject_reason(w).is_none(),
            accept,
            "witness {w:?}: pre-flight disagrees with the kernel example"
        );
    }
}

#[test]
fn test_block_bodies_own_every_heap_object_they_acquire() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // The L9 fixture is the one that surfaced the leak; it is a `test`-only
    // shape, so `debug_dump_mir` (which skips test fns) cannot see it.
    let source = std::fs::read_to_string(root.join("spec/lang/fallible_lambda_test.almd"))
        .expect("read the L9 fixture");
    let certs =
        almide_mir::pipeline::ownership_certificates(&source).expect("the L9 fixture must lower");
    assert!(
        certs.iter().any(|(n, _)| n.contains("L9")),
        "the L9 test body must reach the certificate view — if it stopped \
         lowering, this gate went vacuous"
    );
    let bad: Vec<String> = certs
        .iter()
        .flat_map(|(name, cert)| {
            cert.lines()
                .filter_map(move |l| reject_reason(l).map(|r| format!("{name}: {l:?} — {r}")))
        })
        .collect();
    assert!(
        bad.is_empty(),
        "unbalanced ownership witness(es):\n  {}",
        bad.join("\n  ")
    );
}
