//! An Option's `?? fallback` on the certified native render, spelled as one
//! `match` (#4006, the no-perf-loss rule).
//!
//! The MIR lowers `opt ?? fallback` to a tag read, the payload read hoisted
//! above the tag test, and an if-as-value join:
//!
//! ```text
//! let mut vT: i64 = vO.is_some() as i64;
//! let mut vP: S = match vO { Some(v) => v, None => Default::default() };
//! let mut vR: S;
//! if vT != 0 {
//!     let mut vA = vP;
//!     vR = vA;
//! } else {
//!     … fallback …
//!     vR = vE;
//! }
//! ```
//!
//! Rendered as written, the payload is moved only on one arm, so Rust keeps a
//! drop flag for it, tests the tag a second time, and builds a default value
//! for the absent case that the else arm then drops: a per-codepoint read of
//! a string ran 1.033x the standard codegen (`string_index_loop`), which
//! matches the Option once. This pass rewrites the join, after the function
//! is rendered, as
//!
//! ```text
//! let mut vR: S = match vO {
//!     Some(v) => v,
//!     None => {
//!         … fallback …
//!         vE
//!     }
//! };
//! ```
//!
//! when `vP`, `vA` and the tag `vT` are read nowhere else and nothing touches
//! `vO` between its tag read and the match. The present case yields the
//! payload and the absent case runs the fallback, as the join did; the
//! default the hoisted read built for the absent case was never observed.
//! The MIR, and every witness taken from it, is unchanged. A join that does
//! not match is left as rendered.

use crate::render_native_loops::{indent_of, is_note, is_var, mentions};

/// Rewrite every matching `?? fallback` join of one rendered function body.
pub(crate) fn option_or_else(out: String) -> String {
    let mut lines: Vec<String> = out.lines().map(str::to_string).collect();
    let mut k = 0;
    while k < lines.len() {
        if let Some(new) = rewrite_at(&lines, k) {
            lines = new;
        }
        k += 1;
    }
    let mut s = lines.join("\n");
    if out.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// The lines (outside `// …` notes) that mention `v`.
fn readers(lines: &[String], v: &str) -> Vec<usize> {
    (0..lines.len()).filter(|&k| !is_note(&lines[k]) && mentions(&lines[k], v)).collect()
}

fn rewrite_at(lines: &[String], p: usize) -> Option<Vec<String>> {
    // `let mut vP: S = match vO { Some(v) => v, None => Default::default() };`
    let (pv, rest) = lines[p].trim().strip_prefix("let mut ")?.split_once(": ")?;
    let (ty, rhs) = rest.split_once(" = ")?;
    let o = rhs.strip_prefix("match ")?.strip_suffix(" { Some(v) => v, None => Default::default() };")?;
    (is_var(pv) && is_var(o)).then_some(())?;
    let ind = indent_of(&lines[p]);
    let pad = &lines[p][..ind];
    // `let mut vR: S;`, `if vT != 0 {`, `let mut vA = vP;`, `vR = vA;`, `} else {`.
    let r = lines.get(p + 1)?.strip_prefix(pad)?.strip_prefix("let mut ")?.strip_suffix(&format!(": {ty};"))?;
    let t = lines.get(p + 2)?.strip_prefix(pad)?.strip_prefix("if ")?.strip_suffix(" != 0 {")?;
    let a = lines.get(p + 3)?.strip_prefix(pad)?.strip_prefix("    let mut ")?.strip_suffix(&format!(" = {pv};"))?;
    let shape = is_var(r)
        && is_var(t)
        && is_var(a)
        && lines.get(p + 4)?.as_str() == format!("{pad}    {r} = {a};")
        && lines.get(p + 5)?.as_str() == format!("{pad}}} else {{");
    shape.then_some(())?;
    // The else arm, to the first line back at the join's indentation: its
    // closing brace, after the arm's yield `vR = vE;`.
    let end = (p + 6..lines.len())
        .find(|&k| !lines[k].trim().is_empty() && indent_of(&lines[k]) <= ind)
        .filter(|&k| lines[k].as_str() == format!("{pad}}}"))?;
    let e = lines.get(end - 1)?.strip_prefix(pad)?.strip_prefix(&format!("    {r} = "))?.strip_suffix(';')?;
    // The tag read of the same Option, and the values only this join reads.
    let def = (0..p).rev().find(|&k| lines[k].trim_start().starts_with(&format!("let mut {t}: ")))?;
    let only = lines[def].trim() == format!("let mut {t}: i64 = {o}.is_some() as i64;")
        && is_var(e)
        && readers(lines, t) == [def, p + 2]
        && readers(lines, pv) == [p, p + 3]
        && readers(lines, a) == [p + 3, p + 4]
        && !lines[def + 1..p].iter().any(|l| !is_note(l) && mentions(l, o))
        && !lines[p + 6..end - 1].iter().any(|l| !is_note(l) && mentions(l, r));
    only.then_some(())?;
    let mut out: Vec<String> = lines[..p].to_vec();
    out.push(format!("{pad}let mut {r}: {ty} = match {o} {{"));
    out.push(format!("{pad}    Some(v) => v,"));
    out.push(format!("{pad}    None => {{"));
    out.extend(lines[p + 6..end - 1].iter().map(|l| format!("    {l}")));
    out.push(format!("{pad}        {e}"));
    out.push(format!("{pad}    }}"));
    out.push(format!("{pad}}};"));
    out.extend(lines[end + 1..].iter().cloned());
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::option_or_else;

    const JOIN: &str = r#"fn f(v5: i64) -> i64 {
    let mut v8: AlmdRec0 = (almide_rt_string_char_at("abc", v5)).map(AlmideString::from);
    // v10: the address of v8
    let mut v13: i64 = v8.is_some() as i64;
    let mut v14: i64 = 12i64;
    let mut v16: AlmideString = match v8 { Some(v) => v, None => Default::default() };
    let mut v9: AlmideString;
    if v13 != 0 {
        let mut v17 = v16;
        v9 = v17;
    } else {
        let mut v18: AlmideString = AlmideString::new();
        v9 = v18;
    }
    let mut v19: i64 = almide_rt_string_len(&v9);
    v19
}
"#;

    #[test]
    fn a_fallback_join_becomes_one_match() {
        let got = option_or_else(JOIN.to_string());
        assert!(got.contains("    let mut v9: AlmideString = match v8 {\n        Some(v) => v,\n        None => {\n"), "{got}");
        assert!(got.contains("            let mut v18: AlmideString = AlmideString::new();\n            v18\n        }\n    };\n"), "{got}");
        assert!(!got.contains("v16") && !got.contains("v17") && !got.contains("if v13"), "{got}");
    }

    #[test]
    fn a_tag_read_again_keeps_the_join() {
        let src = JOIN.replace("    v19\n", "    v19 + v13\n");
        assert_eq!(option_or_else(src.clone()), src);
    }

    #[test]
    fn a_payload_read_again_keeps_the_join() {
        let src = JOIN.replace("    v19\n", "    almide_rt_string_len(&v16)\n");
        assert_eq!(option_or_else(src.clone()), src);
    }
}
