//! A loop that only appends to a list it holds keeps the list's length in a
//! local (ADR-0037 implementation notes: appends in loops).
//!
//! A native `List` / `Bytes` is one shared block whose length and owner count
//! live in the block header (ADR-0037 D1), so each `push` reloads the length
//! and capacity, rechecks the count and stores the length back: an element
//! store through the data pointer may alias the header as far as the backend
//! knows. A loop whose every mention of the list `vL` is one append of one
//! element is spelled with an appender bound just before it, in a block that
//! ends right after it:
//!
//! ```text
//! {
//! let mut vL_p = vL.pusher();     // data, length, capacity in locals
//! loop {
//!     vL_p.push(vX);              // was `vL.push(vX);`
//!     vL_p.push_lane(vY);         // was `let mut vD: T = APPEND(vL, vY); … vL = vD;`
//! }
//! }                               // the length is written back here
//! ```
//!
//! The two append spellings are the render's own: `vL.push(e)` is the list
//! push (the IR's `xs = xs + [e]` window), and `APPEND(vL, e)` rebound to `vL`
//! is the owned twin of a stdlib fn DECLARED `@appends` (its result is its
//! reused list with the `Int` lane `e` appended as one element, converted as
//! the render converts that lane — read from the declaration, never from a
//! name). The first push makes the block unique with room, where `push`
//! would copy a shared block; nothing else touches `vL` inside the loop — no
//! read, clone, move, call or other write — so nobody observes the stale
//! header while the appender lives, and leaving the block by any path drops
//! the appender, which writes the length back.

use crate::render_native_loops::{indent_of, is_note, is_var, mentions};
use std::sync::OnceLock;

/// Bind an appender before every loop whose mentions of a list are all appends.
pub(crate) fn hoisted_appenders(out: String) -> String {
    let twins = appending_twins();
    let mut lines: Vec<String> = out.lines().map(str::to_string).collect();
    let mut at = 0;
    while let Some(s) = (at..lines.len()).find(|&i| is_opener(&lines[i])) {
        at = s + 1;
        // A loop the render left unclosed has no body to scan (`unwrap_or(s)`: none).
        let e = close_of(&lines, s).unwrap_or(s);
        let body: &[String] = lines.get(s + 1..e).unwrap_or(&[]);
        // A closure that may outlive a pass or run on another thread keeps the handle.
        let moving = body.iter().any(|l| l.contains("move ") || l.contains("spawn"));
        let found = candidates(body, twins).into_iter().filter(|_| !moving).find_map(|v| only_appends(&lines[s..=e], &v, twins).map(|r| (v, r)));
        let Some((v, rebinds)) = found else { continue };
        let pad = " ".repeat(indent_of(&lines[s]));
        let p = format!("{v}_p");
        for l in &mut lines[s + 1..e] {
            let t = l.trim_start();
            let ind = l.len() - t.len();
            if let Some(x) = t.strip_prefix(&format!("{v}.push(")) {
                *l = format!("{}{p}.push({x}", &l[..ind]);
            } else if let Some(arg) = twin_arg(t, &v, twins) {
                *l = format!("{}{p}.push_lane({arg});", &l[..ind]);
            } else if rebinds.iter().any(|d| t == format!("{v} = {d};")) {
                *l = format!("{}// {v} = {}: pushed", &l[..ind], &t[v.len() + 3..t.len() - 1]);
            }
        }
        lines.insert(e + 1, format!("{pad}}}"));
        lines.splice(s..s, [format!("{pad}{{"), format!("{pad}let mut {p} = {v}.pusher();")]);
        at = s + 3;
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

/// The lists the body appends to, in first-seen order.
fn candidates(body: &[String], twins: &[&str]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for t in body.iter().map(|l| l.trim_start()) {
        let v = t.split_once(".push(").map(|(v, _)| v).filter(|v| is_var(v)).or_else(|| twin_call(t, twins).map(|(v, _)| v));
        if let Some(v) = v.filter(|v| !seen.iter().any(|x| x == v)) {
            seen.push(v.to_string());
        }
    }
    seen
}

/// `let mut vD: T = TWIN(vL, ARG);` with TWIN a declared append: `(vL, ARG)`.
fn twin_call<'a>(t: &'a str, twins: &[&str]) -> Option<(&'a str, &'a str)> {
    let (_, rhs) = t.strip_prefix("let mut ")?.split_once(" = ")?;
    let (callee, args) = rhs.strip_suffix(");")?.split_once('(')?;
    let (v, arg) = args.split_once(", ")?;
    (twins.contains(&callee) && is_var(v)).then_some((v, arg))
}

/// The appended lane of a declared-append line on `v`.
fn twin_arg<'a>(t: &'a str, v: &str, twins: &[&str]) -> Option<&'a str> {
    twin_call(t, twins).filter(|(w, _)| *w == v).map(|(_, a)| a)
}

/// `Some(rebound)` when every mention of `v` in `region` (the loop's lines, opener and close included) is one append
/// naming `v` once, or the rebind `v = vD;` of a declared append's result `vD` that nothing else in the loop names;
/// `rebound` lists those `vD`.
fn only_appends(region: &[String], v: &str, twins: &[&str]) -> Option<Vec<String>> {
    // Whole-word mentions of `v` in `t`: the append's own one, and nothing else.
    let once = |t: &str| {
        let b = t.as_bytes();
        let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
        t.match_indices(v)
            .filter(|(k, _)| (*k == 0 || !word(b[k - 1])) && b.get(k + v.len()).is_none_or(|c| !word(*c)))
            .count()
            == 1
    };
    let mut rebound: Vec<String> = Vec::new();
    let mut rebinds = 0;
    for t in region.iter().filter(|l| !is_note(l) && mentions(l, v)).map(|l| l.trim()) {
        if t.starts_with(&format!("{v}.push(")) && t.ends_with(");") && once(t) {
            continue;
        }
        if let Some(d) = twin_arg(t, v, twins).and_then(|_| t.strip_prefix("let mut ")?.split_once(':').map(|(d, _)| d)) {
            if once(t) && is_var(d) {
                rebound.push(d.to_string());
                continue;
            }
        }
        if t.strip_prefix(&format!("{v} = ")).and_then(|r| r.strip_suffix(';')).is_some_and(is_var) {
            rebinds += 1;
            continue;
        }
        return None;
    }
    // Each declared append's result is named twice in the loop (bound, then rebound to `v`), and is what every rebind
    // of `v` takes.
    let named_twice = |d: &String| region.iter().filter(|l| !is_note(l) && mentions(l, d)).count() == 2;
    let rebinds_ok = rebinds == rebound.len()
        && rebound.iter().all(|d| named_twice(d) && region.iter().any(|l| l.trim() == format!("{v} = {d};")));
    rebinds_ok.then_some(rebound)
}

/// The owned twins (`@reuse`) of the stdlib fns declared `@appends` (the generated declaration table).
fn appending_twins() -> &'static [&'static str] {
    static TWINS: OnceLock<Vec<&'static str>> = OnceLock::new();
    TWINS.get_or_init(|| {
        almide_lang::reuse::REUSE.iter().filter(|r| r.appends && r.param == 0 && r.arity == 2).filter_map(|r| r.owned_twin).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::hoisted_appenders;

    #[test]
    fn a_loop_that_only_pushes_a_list_takes_an_appender() {
        let src = "fn f() {\n    let mut v1: AlmideList<i64> = almide_list![];\n    loop {\n        if !v2 { break; }\n        v1.push(v3);\n    }\n    g(&v1);\n}\n";
        let got = hoisted_appenders(src.to_string());
        assert!(got.contains("    {\n    let mut v1_p = v1.pusher();\n    loop {\n        if !v2 { break; }\n        v1_p.push(v3);\n    }\n    }\n    g(&v1);"), "{got}");
    }

    #[test]
    fn a_declared_append_rebound_to_the_list_pushes_its_lane() {
        let src = "fn f() {\n    loop {\n        let mut v9: AlmdRec1 = almide_rt_bytes_append_fresh_owned(v31, v60);\n        // drop: scope-end\n        v31 = v9;\n    }\n}\n";
        let got = hoisted_appenders(src.to_string());
        assert!(got.contains("let mut v31_p = v31.pusher();\n    loop {\n        v31_p.push_lane(v60);\n        // drop: scope-end\n        // v31 = v9: pushed\n    }\n    }"), "{got}");
    }

    #[test]
    fn any_other_mention_keeps_the_handle() {
        for other in ["g(&v1);", "let mut v5: i64 = almide_rt_list_len(&v1);", "v1.push(v1.len() as i64);", "v1 = v7;"] {
            let src = format!("fn f() {{\n    loop {{\n        v1.push(v3);\n        {other}\n    }}\n}}\n");
            assert_eq!(hoisted_appenders(src.clone()), src, "{other}");
        }
    }
}
