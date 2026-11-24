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
//! each argument — a local this function owns, never a borrow — by nothing
//! between the list's declaration and the loop's end but its own early
//! release. Anything else stays as rendered.

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

/// One loop to rewrite: the lines it touches, by index into the function.
struct Plan {
    /// The source list's declaration, the loop head, its closing line, and
    /// the `drop` of the list right after it (none: it lives to scope end).
    d: usize,
    h: usize,
    e: usize,
    dl: Option<usize>,
    src: String,
    elem: String,
    item: StreamItem,
    /// `TWIN(ARGS)`.
    call: String,
    /// The list's length reads; those that only size the collected list,
    /// with that list's declaration; the arguments' early releases.
    lens: Vec<usize>,
    sized: Vec<(usize, usize)>,
    released: Vec<usize>,
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
    let lens = length_reads(lines, (d, h, dl), &src)?;
    if !reads_element_as_yielded(&lines[h + 1..e], &elem, item) {
        return None;
    }
    let released = early_releases(lines, d, dl.unwrap_or(e + 1), &args)?;
    let sized = lens.iter().filter_map(|&k| sizing_only(lines, k)).collect();
    let call = format!("{twin}({})", args.join(", "));
    Some(render(lines, &Plan { d, h, e, dl, src, elem, item, call, lens, sized, released }))
}

/// The list is named only by its declaration `d`, its length reads before
/// the loop head `h` (returned), the head and its drop `dl`.
fn length_reads(lines: &[String], (d, h, dl): (usize, usize, Option<usize>), src: &str) -> Option<Vec<usize>> {
    let len_read = format!("{src}.len() as i64");
    let mut lens = Vec::new();
    for (k, l) in lines.iter().enumerate() {
        if k == d || k == h || Some(k) == dl || is_note(l) || !mentions(l, src) {
            continue;
        }
        match let_parts(l) {
            Some((_, "i64", r)) if r == len_read && k > d && k < h => lens.push(k),
            _ => return None,
        }
    }
    Some(lens)
}

/// The loop `body` reads the element only as the stream yields it.
fn reads_element_as_yielded(body: &[String], elem: &str, item: StreamItem) -> bool {
    let star = format!("(*{elem})");
    let read = match item {
        StreamItem::Str => format!("&{star}[..]"),
        StreamItem::Int => star,
    };
    !body.iter().any(|l| mentions(&l.replace(&read, ""), elem))
}

/// Each argument is a value this function owns outright: a local declared
/// with an owned type. A borrow (`&…`, a cell guard's reference) may name a
/// block something else writes while the loop runs, which the built list
/// never saw; an owned local named by nothing during the loop cannot change.
/// Between the declaration `d` and `end`, each is named only by its own early
/// release (returned: it moves after the loop).
fn early_releases(lines: &[String], d: usize, end: usize, args: &[&str]) -> Option<Vec<usize>> {
    let vars: Vec<&str> = args.iter().map(|a| a.strip_prefix('&').unwrap_or(a)).filter(|a| is_var(a)).collect();
    let owned = |v: &str| {
        (0..d).rev().find_map(|k| let_parts(&lines[k]).filter(|(n, _, _)| *n == v))
            .is_some_and(|(_, ty, _)| !ty.starts_with('&') && !ty.contains("Ref") && !ty.contains("Cell"))
    };
    if !vars.iter().all(|v| owned(v)) {
        return None;
    }
    let mut released = Vec::new();
    for (k, l) in lines.iter().enumerate().take(end).skip(d + 1).filter(|(_, l)| !is_note(l)) {
        for v in &vars {
            if l.trim() == format!("drop({v});") {
                released.push(k);
            } else if mentions(l, v) {
                return None;
            }
        }
    }
    Some(released)
}

/// The length read at `lines[k]` when its one use sizes a collected list
/// (that list's declaration): the list then grows as the stream fills it,
/// rather than walking the stream twice to count it.
fn sizing_only(lines: &[String], k: usize) -> Option<(usize, usize)> {
    let (v, _, _) = let_parts(&lines[k])?;
    let reserve = format!("{{ let mut r = AlmideList::new(); if r.try_reserve_exact({v}.max(0) as usize).is_err() {{");
    let uses: Vec<usize> = (0..lines.len()).filter(|&j| j != k && !is_note(&lines[j]) && mentions(&lines[j], v)).collect();
    let [j] = uses[..] else { return None };
    let (_, ty, rhs) = let_parts(&lines[j])?;
    (ty.starts_with("AlmideList<") && rhs.starts_with(&reserve) && !mentions(&rhs.replacen(&reserve, "", 1), v)).then_some((k, j))
}

fn render(lines: &[String], p: &Plan) -> Vec<String> {
    let pad = |l: &String| " ".repeat(indent_of(l));
    let releases = || p.released.iter().map(|&r| lines[r].clone());
    let mut out = Vec::with_capacity(lines.len());
    for (k, l) in lines.iter().enumerate() {
        if k == p.d || p.released.contains(&k) || p.sized.iter().any(|&(n, _)| n == k) {
            continue;
        }
        let decl = let_parts(l);
        match decl {
            Some((v, ty, _)) if p.sized.iter().any(|&(_, j)| j == k) => out.push(format!("{}let mut {v}: {ty} = AlmideList::new();", pad(l))),
            Some((v, _, _)) if p.lens.contains(&k) => out.push(format!("{}let mut {v}: i64 = {}.count() as i64;", pad(l), p.call)),
            _ if k == p.h => out.push(l.replace(&format!("{}.iter().enumerate()", p.src), &format!("{}.enumerate()", p.call))),
            _ if k > p.h && k < p.e && p.item == StreamItem::Int => out.push(l.replace(&format!("(*{})", p.elem), &p.elem)),
            _ if Some(k) == p.dl => out.extend(releases()),
            _ => {
                out.push(l.clone());
                if k == p.e && p.dl.is_none() {
                    out.extend(releases());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::streamed_sources;

    fn run(src: &str) -> String {
        streamed_sources(src.lines().map(str::to_string).collect()).join("\n")
    }

    const SPLIT_MAP: &str = r#"    let mut v21: AlmideString = AlmideString::new();
    let mut v23: AlmideList<AlmideString> = crate::almide_str_block::almide_str_list(almide_rt_string_split(&v21, ","));
    drop(v21);
    // v26: v23 + 4 bytes
    let mut v27: i64 = v23.len() as i64;
    let mut v28: AlmideList<AlmideString> = AlmideList::new();
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
        // A length that only sizes the collected list is not counted.
        let sized = run(&SPLIT_MAP.replace("let mut v28: AlmideList<AlmideString> = AlmideList::new();", "let mut v28: AlmideList<AlmideString> = { let mut r = AlmideList::new(); if r.try_reserve_exact(v27.max(0) as usize).is_err() { eprintln!(\"Error: out of memory\"); almide_host_exit(1); } r };"));
        assert!(!sized.contains("v27") && sized.contains("let mut v28: AlmideList<AlmideString> = AlmideList::new();"), "{sized}");
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
    fn a_producer_over_a_borrow_is_kept() {
        // The block behind a borrow may be written while the loop runs
        // (a shared cell): the list was read before any such write.
        let src = r#"    let mut v12: &AlmdRec2 = v11;
    let mut v17: AlmideList<i64> = almide_rt_bytes_to_list(v12);
    for (v20, __e_v20) in v17.iter().enumerate() {
        let v20: i64 = v20 as i64;
        let mut v25: i64 = (*__e_v20);
    }
    drop(v17);"#;
        assert_eq!(run(src), src);
    }

    #[test]
    fn a_producer_without_a_stream_twin_is_kept() {
        let src = SPLIT_MAP.replace("almide_rt_string_split(", "almide_rt_string_lines(");
        assert_eq!(run(&src), src);
    }

    #[test]
    fn an_int_stream_yields_the_value() {
        let src = r#"    let mut v64: AlmdRec5 = almide_rt_bytes_from_string(v58);
    let mut v65: AlmideList<i64> = almide_rt_bytes_to_list(&v64);
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
        // A list left to scope end streams too; one read again does not.
        let scoped = src.replace("    drop(v65);", "    let mut v80: i64 = v70;");
        assert!(run(&scoped).contains("almide_rt_bytes_to_list_iter(&v64).enumerate()"), "{scoped}");
        let read_later = src.replace("    drop(v65);", "    let mut v80: i64 = v65.len() as i64;");
        assert_eq!(run(&read_later), read_later);
    }
}
