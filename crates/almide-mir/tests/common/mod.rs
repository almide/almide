//! The ownership-certificate pre-flight shared by the almide-mir integration
//! tests: a mirror of the Coq `check` (proofs/OwnershipChecker.v) that
//! `proofs/corpus-wall.sh` runs as the trusted checker. See
//! `ownership_test_block_balance.rs` for the semantics and the test that pins
//! this mirror against the kernel's own examples.

/// One certificate line -> `None` if the kernel would accept it.
pub fn reject_reason(line: &str) -> Option<String> {
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
