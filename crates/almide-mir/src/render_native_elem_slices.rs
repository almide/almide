//! A loop that indexes a list takes the list's elements as a slice once,
//! before the loop (ADR-0037 implementation notes: element access in loops).
//!
//! A native `List` is one shared block whose length and owner count live in
//! the block header (ADR-0037 D1). An element store through the block's data
//! pointer may alias that header as far as the backend knows, so a loop that
//! writes `xs[i]` through the list handle reloads the length after every
//! store, and each store goes through `make_mut` (D2) and its count check.
//! The render of a scalar element access is
//!
//! ```text
//! let mut vX: i64 = almide_idx_get(&vL, vI);
//! almide_idx_set(&mut vL, vI, vX);
//! ```
//!
//! When every mention of `vL` inside a loop is one of those two forms, the
//! loop is spelled with a slice bound just before it:
//!
//! ```text
//! let vL_w: &mut [i64] = vL.make_mut();    // a loop that writes vL
//! let vL_r: &[i64] = &vL;                  // a loop that only reads vL
//! loop {
//!     let mut vX: i64 = almide_idx_get(vL_w, vI);
//!     almide_idx_set(vL_w, vI, vX);
//! }
//! ```
//!
//! The slice's pointer and length are locals, so the backend keeps them in
//! registers, and the list is made unique once (copy-on-write still holds: a
//! shared block is copied by that one `make_mut`, before any write). Nothing
//! else touches `vL` inside the loop — no push, clone, move, call or
//! reassignment — so its length cannot change while the slice is lent, and
//! the bounds check reads the same length the handle would. The one
//! difference is when a shared block is copied: at the loop's entry instead
//! of at its first write, so a loop over a shared list that writes it on no
//! pass still copies it once. A loop that mentions `vL` any other way is left
//! as rendered.

use crate::render_native_loops::{indent_of, is_note, is_var, mentions};

/// Bind a slice before every loop whose accesses to a list are all scalar element reads and writes.
pub(crate) fn hoisted_slices(out: String) -> String {
    let mut lines: Vec<String> = out.lines().map(str::to_string).collect();
    let mut at = 0;
    while let Some(s) = (at..lines.len()).find(|&i| is_opener(&lines[i])) {
        at = s + 1;
        let Some(e) = close_of(&lines, s) else { continue };
        // A closure that may outlive a pass or run on another thread keeps the handles (a `move` would take the slice).
        if lines[s..=e].iter().any(|l| l.contains("move ") || l.contains("spawn")) {
            continue;
        }
        let mut binds: Vec<String> = Vec::new();
        for v in candidates(&lines[s + 1..e]) {
            let Some(writes) = only_element_access(&lines[s..=e], &v) else { continue };
            let pad = " ".repeat(indent_of(&lines[s]));
            let (name, bind) = if writes {
                (format!("{v}_w"), format!("{pad}let {v}_w: &mut [i64] = {v}.make_mut();"))
            } else {
                (format!("{v}_r"), format!("{pad}let {v}_r: &[i64] = &{v};"))
            };
            for l in &mut lines[s + 1..e] {
                *l = l.replace(&format!("almide_idx_get(&{v}, "), &format!("almide_idx_get({name}, "));
                *l = l.replace(&format!("almide_idx_set(&mut {v}, "), &format!("almide_idx_set({name}, "));
            }
            binds.push(bind);
        }
        if !binds.is_empty() {
            at += binds.len();
            lines.splice(s..s, binds);
        }
    }
    let mut s = lines.join("\n");
    if out.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// `loop {` or `for … {`, the two loop spellings of the render.
fn is_opener(l: &str) -> bool {
    let t = l.trim();
    t == "loop {" || (t.starts_with("for ") && t.ends_with(" {"))
}

/// The line closing the loop opened at `s`: the next `}` at its indentation.
fn close_of(lines: &[String], s: usize) -> Option<usize> {
    let ind = indent_of(&lines[s]);
    (s + 1..lines.len()).find(|&k| indent_of(&lines[k]) == ind && lines[k].trim() == "}")
}

/// The lists the body reads or writes an element of through its handle, in first-seen order.
fn candidates(body: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for l in body {
        for pat in ["almide_idx_get(&", "almide_idx_set(&mut "] {
            let mut rest = l.as_str();
            while let Some(k) = rest.find(pat) {
                rest = &rest[k + pat.len()..];
                let v = rest.split(',').next().unwrap_or("");
                if is_var(v) && !seen.iter().any(|x| x == v) {
                    seen.push(v.to_string());
                }
            }
        }
    }
    seen
}

/// `Some(writes)` when every mention of `v` in `region` (the loop's lines, opener and
/// close included) is a whole rendered element read or write statement naming `v` once.
fn only_element_access(region: &[String], v: &str) -> Option<bool> {
    let get = format!("almide_idx_get(&{v}, ");
    let set = format!("almide_idx_set(&mut {v}, ");
    // Whole-word mentions of `v` in `t`: the access's own one, and nothing else.
    let once = |t: &str| {
        let b = t.as_bytes();
        let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
        t.match_indices(v)
            .filter(|(k, _)| (*k == 0 || !word(b[k - 1])) && b.get(k + v.len()).is_none_or(|c| !word(*c)))
            .count()
            == 1
    };
    let mut writes = false;
    for l in region.iter().filter(|l| !is_note(l) && mentions(l, v)) {
        let t = l.trim();
        let read = t
            .strip_prefix("let mut ")
            .and_then(|r| r.split_once(": i64 = "))
            .is_some_and(|(x, rhs)| is_var(x) && rhs.starts_with(&get) && rhs.ends_with(");"));
        let write = t.starts_with(&set) && t.ends_with(");");
        if !(read || write) || !once(t) {
            return None;
        }
        writes |= write;
    }
    Some(writes)
}

#[cfg(test)]
mod tests {
    use super::hoisted_slices;

    const FLIP: &str = "fn f(mut v0: AlmideList<i64>) {\n    let mut v1: i64 = 0i64;\n    loop {\n        let v2: bool = v1 < v3;\n        if !v2 { break; }\n        let mut v4: i64 = almide_idx_get(&v0, v1);\n        almide_idx_set(&mut v0, v1, v4);\n        v1 = v5;\n    }\n}\n";

    #[test]
    fn a_loop_that_writes_a_list_borrows_its_elements_once() {
        let got = hoisted_slices(FLIP.to_string());
        assert!(got.contains("    let v0_w: &mut [i64] = v0.make_mut();\n    loop {\n"), "{got}");
        assert!(got.contains("almide_idx_get(v0_w, v1)") && got.contains("almide_idx_set(v0_w, v1, v4)"), "{got}");
        assert!(!got.contains("&mut v0,"), "{got}");
    }

    #[test]
    fn a_loop_that_only_reads_a_list_borrows_it_shared() {
        let src = "fn f(v0: AlmideList<i64>) {\n    for v1 in 0i64..v3 {\n        let mut v4: i64 = almide_idx_get(&v0, v1);\n        v6 = v4;\n    }\n}\n";
        let got = hoisted_slices(src.to_string());
        assert!(got.contains("    let v0_r: &[i64] = &v0;\n    for v1 in 0i64..v3 {\n"), "{got}");
        assert!(got.contains("almide_idx_get(v0_r, v1)"), "{got}");
    }

    #[test]
    fn any_other_mention_keeps_the_handle() {
        for other in [
            "v0.push(v4);",
            "let mut v9: AlmideList<i64> = v0.clone();",
            "v7 = almd_g(&v0);",
            "let mut v8: i64 = almide_idx_get(&v0, v0);",
            "almide_idx_set(&mut v0, v1, v0.len() as i64);",
        ] {
            let src = FLIP.replace("        v1 = v5;\n", &format!("        {other}\n        v1 = v5;\n"));
            let got = hoisted_slices(src.clone());
            assert!(!got.contains("v0_w") && !got.contains("v0_r"), "{other}: {got}");
        }
    }

    #[test]
    fn the_outermost_loop_that_qualifies_takes_the_slice() {
        let src = "fn f(mut v0: AlmideList<i64>) {\n    loop {\n        if !v2 { break; }\n        for v1 in 0i64..v3 {\n            almide_idx_set(&mut v0, v1, v4);\n        }\n        let mut v6: i64 = almide_idx_get(&v0, v7);\n    }\n}\n";
        let got = hoisted_slices(src.to_string());
        assert_eq!(got.matches("let v0_w").count(), 1, "{got}");
        assert!(got.contains("    let v0_w: &mut [i64] = v0.make_mut();\n    loop {\n"), "{got}");
    }

    #[test]
    fn a_push_in_the_outer_loop_leaves_the_slice_to_the_inner_one() {
        let src = "fn f(mut v0: AlmideList<i64>) {\n    loop {\n        if !v2 { break; }\n        v0.push(v9);\n        for v1 in 0i64..v3 {\n            almide_idx_set(&mut v0, v1, v4);\n        }\n    }\n}\n";
        let got = hoisted_slices(src.to_string());
        assert!(got.contains("        let v0_w: &mut [i64] = v0.make_mut();\n        for v1 in 0i64..v3 {\n"), "{got}");
        assert!(got.contains("        v0.push(v9);\n"), "{got}");
    }

    #[test]
    fn a_loop_holding_a_moving_closure_keeps_the_handle() {
        let src = FLIP.replace("        v1 = v5;\n", "        let v9 = move |v10: i64| v10;\n        v1 = v5;\n");
        assert_eq!(hoisted_slices(src.clone()), src);
    }
}
