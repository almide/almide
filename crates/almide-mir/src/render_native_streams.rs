//! A list built only to be walked, walked as its producer's stream (#4612).
//!
//! `string.split(s, ",") |> list.map(f)` builds the pieces as a list, one
//! block per piece, and the map loop then reads each piece once and drops the
//! list: a short line paid a list and a block per piece for slices of `s` it
//! already had. After the counted loops are spelled as iterators
//! (`render_native_loops`), such a loop reads
//!
//! ```text
//! let mut vS: Vec<AlmideString> = …almide_str_list(SYMBOL(ARGS));
//! drop(vA);                                   // an argument released early
//! let mut vN: i64 = vS.len() as i64;          // the list's length, if read
//! vR.extend(vS.iter().enumerate().map(|(vI, __e_vI)| { … &(*__e_vI)[..] … }));
//! drop(vS);
//! ```
//!
//! When the runtime body `SYMBOL` has a streaming twin (`SYMBOL_iter`,
//! [`crate::native_floor::stream_twin`]: the same params, an iterator of the
//! items the body collects — the body is the twin collected), the loop walks
//! the twin instead: the list is never built, its length is the stream's
//! count, and an argument the twin borrows is released after the loop rather
//! than before it. Nothing here names a stdlib fn; the twin is the property,
//! declared once beside the body.
//!
//! The loop must read the element only as the stream yields it: a string
//! element only through `&(*e)[..]` (the twin yields the `&str` itself), an
//! Int element only as `(*e)` (the twin yields the Int, so `(*e)` becomes
//! `e`). The list must be named by nothing but its declaration, its length
//! reads before the loop, the loop head and the `drop` right after the loop
//! (or none: the list then lives to scope end, named by no later line), and
//! each argument by nothing between the list's declaration and the loop's
//! end but its own early release. Anything else stays as rendered.

use crate::native_floor::StreamItem;
use crate::render_native_loops::{indent_of, is_note, is_var, let_parts, mentions};

/// Rewrite every loop of one rendered function body that walks a list its
/// producer could have streamed.
pub(crate) fn streamed_sources(mut lines: Vec<String>) -> Vec<String> {
    let heads: Vec<usize> = (0..lines.len()).filter(|&h| lines[h].contains(".iter().enumerate()")).collect();
    for &h in heads.iter().rev() {
        if let Some(new) = stream_at(&lines, h) {
            lines = new;
        }
    }
    lines
}

/// The loop head at `lines[h]`: its source list, its element name, and the
/// line closing the loop.
fn loop_head(lines: &[String], h: usize) -> Option<(String, String, usize)> {
    let head = lines[h].trim();
    let ind = indent_of(&lines[h]);
    let (before, _) = head.split_once(".iter().enumerate()")?;
    let (src, close) = if let Some(rest) = before.strip_prefix("for (") {
        let (_, src) = rest.split_once(") in ")?;
        (src, "}")
    } else {
        let (_, src) = before.split_once(".extend(")?;
        (src, "}));")
    };
    let elem = head.split_once(", __e_")?.1.split(')').next()?;
    let elem = format!("__e_{elem}");
    if !is_var(src) || !is_var(&elem[4..]) {
        return None;
    }
    let e = (h + 1..lines.len()).find(|&k| !lines[k].trim().is_empty() && indent_of(&lines[k]) <= ind)?;
    (indent_of(&lines[e]) == ind && lines[e].trim() == close).then(|| (src.to_string(), elem, e))
}

/// `SYMBOL(ARGS)` (inside the string-list wrapper or bare) → the symbol and
/// its argument texts, each a value name, a borrow of one, or a string literal.
fn producer_call(rhs: &str) -> Option<(&str, Vec<&str>)> {
    let call = rhs
        .strip_prefix("crate::almide_str_block::almide_str_list(")
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or(rhs);
    let (symbol, rest) = call.split_once('(')?;
    let inner = rest.strip_suffix(')')?;
    if symbol.is_empty() || !symbol.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let mut args = Vec::new();
    let (mut start, mut quoted, mut escaped) = (0, false, false);
    for (k, c) in inner.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ',' if !quoted => {
                args.push(inner[start..k].trim());
                start = k + 1;
            }
            _ => {}
        }
    }
    if !inner.trim().is_empty() {
        args.push(inner[start..].trim());
    }
    let simple = |a: &&str| {
        is_var(a.strip_prefix('&').unwrap_or(a)) || (a.len() >= 2 && a.starts_with('"') && a.ends_with('"'))
    };
    args.iter().all(simple).then_some((symbol, args))
}

fn stream_at(lines: &[String], h: usize) -> Option<Vec<String>> {
    let (src, elem, e) = loop_head(lines, h)?;
    // The list dies at the loop: released right after it, or at scope end
    // (no later line names it, checked below).
    let dl = (e + 1..lines.len()).find(|&k| !is_note(&lines[k])).filter(|&k| lines[k].trim() == format!("drop({src});"));
    let d = (0..h).rev().find(|&k| let_parts(&lines[k]).is_some_and(|(v, _, _)| v == src))?;
    let (_, _, rhs) = let_parts(&lines[d])?;
    let (symbol, args) = producer_call(rhs)?;
    let (twin, item) = crate::native_floor::stream_twin(symbol)?;
    // The list is named only by its declaration, its length reads before the
    // loop, the loop head and its drop.
    let len_read = format!("{src}.len() as i64");
    let mut lens = Vec::new();
    for (k, l) in lines.iter().enumerate() {
        if k == d || k == h || Some(k) == dl || is_note(l) || !mentions(l, &src) {
            continue;
        }
        match let_parts(l) {
            Some((_, "i64", r)) if r == len_read && k > d && k < h => lens.push(k),
            _ => return None,
        }
    }
    // The element is read only as the stream yields it.
    let body = &lines[h + 1..e];
    let star = format!("(*{elem})");
    let read = match item {
        StreamItem::Str => format!("&{star}[..]"),
        StreamItem::Int => star.clone(),
    };
    if body.iter().any(|l| mentions(&l.replace(&read, ""), &elem)) {
        return None;
    }
    // Each argument is named by nothing from the list's declaration to its
    // drop but its own early release, which moves after the loop.
    let vars: Vec<&str> = args.iter().map(|a| a.strip_prefix('&').unwrap_or(a)).filter(|a| is_var(a)).collect();
    let mut released = Vec::new();
    for (k, l) in lines.iter().enumerate().take(dl.unwrap_or(e + 1)).skip(d + 1) {
        if is_note(l) {
            continue;
        }
        for v in &vars {
            if l.trim() == format!("drop({v});") {
                released.push(k);
            } else if mentions(l, v) {
                return None;
            }
        }
    }
    let call = format!("{twin}({})", args.join(", "));
    let mut out = Vec::with_capacity(lines.len());
    for (k, l) in lines.iter().enumerate() {
        if k == d || released.contains(&k) {
            continue;
        }
        if lens.contains(&k) {
            let (v, _, _) = let_parts(l)?;
            out.push(format!("{}let mut {v}: i64 = {call}.count() as i64;", " ".repeat(indent_of(l))));
        } else if k == h {
            out.push(l.replace(&format!("{src}.iter().enumerate()"), &format!("{call}.enumerate()")));
        } else if k > h && k < e && item == StreamItem::Int {
            out.push(l.replace(&star, &elem));
        } else if Some(k) == dl {
            out.extend(released.iter().map(|&r| lines[r].clone()));
        } else {
            out.push(l.clone());
            if k == e && dl.is_none() {
                out.extend(released.iter().map(|&r| lines[r].clone()));
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::streamed_sources;

    fn run(src: &str) -> String {
        streamed_sources(src.lines().map(str::to_string).collect()).join("\n")
    }

    const SPLIT_MAP: &str = r#"    let mut v23: Vec<AlmideString> = crate::almide_str_block::almide_str_list(almide_rt_string_split(&v21, ","));
    drop(v21);
    // v26: v23 + 4 bytes
    let mut v27: i64 = v23.len() as i64;
    let mut v28: Vec<AlmideString> = Vec::new();
    v28.extend(v23.iter().enumerate().map(|(v30, __e_v30)| {
        let v30: i64 = v30 as i64;
        let mut v35: &str = &(*__e_v30)[..];
        let mut v40: AlmideString = rt_string_to_upper(v35);
        v40
    }));
    drop(v23);
    let mut v47: i64 = 0i64;"#;

    #[test]
    fn a_split_walked_once_and_dropped_is_streamed() {
        let got = run(SPLIT_MAP);
        assert!(got.contains("v28.extend(almide_rt_string_split_iter(&v21, \",\").enumerate().map(|(v30, __e_v30)| {"), "{got}");
        assert!(got.contains("let mut v27: i64 = almide_rt_string_split_iter(&v21, \",\").count() as i64;"), "{got}");
        assert!(!got.contains("let mut v23") && !got.contains("drop(v23)"), "{got}");
        // The argument is released after the loop, not before it.
        let at = got.find("drop(v21);").expect("released");
        assert!(at > got.find("}));").unwrap(), "{got}");
    }

    #[test]
    fn a_split_whose_element_is_copied_or_whose_list_lives_on_is_kept() {
        for src in [
            SPLIT_MAP.replace("let mut v40: AlmideString = rt_string_to_upper(v35);", "let mut v40: AlmideString = (*__e_v30).clone();"),
            SPLIT_MAP.replace("    drop(v23);\n", "    let mut v50: i64 = v23.len() as i64;\n    drop(v23);\n"),
            SPLIT_MAP.replace("    drop(v21);\n", "    let mut v22: AlmideString = v21;\n"),
        ] {
            assert_eq!(run(&src), src);
        }
    }

    #[test]
    fn a_producer_without_a_stream_twin_is_kept() {
        let src = SPLIT_MAP.replace("almide_rt_string_split(", "almide_rt_string_lines(");
        assert_eq!(run(&src), src);
    }

    #[test]
    fn an_int_stream_yields_the_value() {
        let src = r#"    let mut v65: Vec<i64> = almide_rt_bytes_to_list(&v64);
    let mut v70: i64 = 0i64;
    for (v53, __e_v53) in v65.iter().enumerate() {
        let v53: i64 = v53 as i64;
        let mut v66: i64 = (*__e_v53);
        v70 = v70.wrapping_add(v66);
    }
    drop(v65);"#;
        let got = run(src);
        assert!(got.contains("for (v53, __e_v53) in almide_rt_bytes_to_list_iter(&v64).enumerate() {"), "{got}");
        assert!(got.contains("let mut v66: i64 = __e_v53;") && !got.contains("v65"), "{got}");
    }
}
