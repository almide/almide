//! Counted loops of the certified native render, spelled as Rust iterators
//! (#4006, the no-perf-loss rule).
//!
//! The MIR lowers a list HOF (and a `for` over a list) to a counted loop:
//!
//! ```text
//! let mut vI: i64 = 0i64;            // the counter, read nowhere else
//! loop {
//!     let vC: bool = vI < vLEN;
//!     if !vC { break; }
//!     … body …
//!     let mut vN: i64 = vI.wrapping_add(vONE);   // vONE is 1
//!     vI = vN;
//! }
//! ```
//!
//! Rendered as written, a map's `Vec::push` per element and the bounds check
//! of each `xs[i]` cost 2-5x the standard codegen's `iter().map().collect()`
//! at opt-level 1. This pass rewrites such a loop, after the function is
//! rendered, as
//!
//! - `for vI in 0..vLEN { … }` (any counted loop whose counter the body
//!   does not write),
//! - `vS.iter().enumerate()` in place of `0..vLEN` when `vLEN` is `vS.len()`
//!   and the body reads `vS` only as `vS[vI]`, which becomes the iterator's
//!   element (no bounds check, and no index past the list either way),
//! - `let mut vR: T = … .map(|vI| { … EXPR }).collect();` when the loop
//!   fills `vR`, pushed exactly once per pass at the end of the body, into
//!   a `Vec::with_capacity(vLEN)` declared for it and read nowhere else.
//!   The closure form also needs a body that leaves the loop by no
//!   `break`/`continue`/`return`/`?` and writes no variable declared outside it.
//!
//! The rewrite changes the spelling, not the program: the same body runs
//! for `vI = 0, 1, …, vLEN - 1` in order, and the pushed values are the
//! collected ones. The MIR, and every witness taken from it, is unchanged.
//! A loop that does not match is left as rendered.

/// Rewrite every matching counted loop of one rendered function body.
pub(crate) fn iterator_loops(out: String) -> String {
    let mut lines: Vec<String> = out.lines().map(str::to_string).collect();
    // Innermost first: a later `loop {` opens inside an earlier one or after it.
    let starts: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].trim() == "loop {").collect();
    for &s in starts.iter().rev() {
        if let Some(new) = rewrite_at(&lines, s) {
            lines = new;
        }
    }
    let mut s = lines.join("\n");
    if out.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// A `// …` line of the render.
fn is_note(l: &str) -> bool {
    l.trim_start().starts_with("//")
}

fn indent_of(l: &str) -> usize {
    l.len() - l.trim_start().len()
}

/// `vNNN` (the render's value names) and nothing else.
fn is_var(s: &str) -> bool {
    s.len() > 1 && s.starts_with('v') && s[1..].bytes().all(|b| b.is_ascii_digit())
}

/// Does `text` mention variable `v` as a whole word?
fn mentions(text: &str, v: &str) -> bool {
    let b = text.as_bytes();
    let mut from = 0;
    while let Some(k) = text[from..].find(v) {
        let at = from + k;
        let end = at + v.len();
        let before_ok = at == 0 || !(b[at - 1].is_ascii_alphanumeric() || b[at - 1] == b'_');
        let after_ok = end == b.len() || !(b[end].is_ascii_alphanumeric() || b[end] == b'_');
        if before_ok && after_ok {
            return true;
        }
        from = end;
    }
    false
}

/// `let mut v: T = rhs;` / `let v: T = rhs;` → (v, T, rhs).
fn let_parts(l: &str) -> Option<(&str, &str, &str)> {
    let t = l.trim();
    let t = t.strip_prefix("let mut ").or_else(|| t.strip_prefix("let "))?;
    let (v, rest) = t.split_once(": ")?;
    let (ty, rhs) = rest.split_once(" = ")?;
    let rhs = rhs.strip_suffix(';')?;
    is_var(v).then_some((v, ty, rhs))
}

/// `v = rhs;` (a plain assignment) → v.
fn assigned_var(l: &str) -> Option<&str> {
    let t = l.trim();
    let (v, _) = t.split_once(" = ")?;
    is_var(v).then_some(v)
}

fn rewrite_at(lines: &[String], s: usize) -> Option<Vec<String>> {
    let ind = indent_of(&lines[s]);
    let pad = " ".repeat(ind);
    let inner = " ".repeat(ind + 4);
    // The header: `let vC: bool = vI < vLEN;` then `if !vC { break; }`.
    let (c, ty, rhs) = let_parts(lines.get(s + 1)?)?;
    if ty != "bool" || indent_of(&lines[s + 1]) != ind + 4 {
        return None;
    }
    let (i, len) = rhs.split_once(" < ")?;
    if !is_var(i) || !is_var(len) || lines.get(s + 2)?.trim() != format!("if !{c} {{ break; }}") {
        return None;
    }
    // The closing brace at the loop's own indentation.
    let e = (s + 1..lines.len()).find(|&k| indent_of(&lines[k]) == ind && lines[k].trim() == "}")?;
    if (s + 1..e).any(|k| !lines[k].trim().is_empty() && indent_of(&lines[k]) <= ind) {
        return None;
    }
    // The step: `let mut vN: i64 = vI.wrapping_add(vONE);` then `vI = vN;`.
    let (n, nty, step) = let_parts(lines.get(e - 2)?)?;
    let one = step.strip_prefix(&format!("{i}.wrapping_add("))?.strip_suffix(')')?;
    if nty != "i64" || lines[e - 1].trim() != format!("{i} = {n};") || !is_var(one) {
        return None;
    }
    // The render's `// vN: …` notes name values without reading them: the
    // body is judged, and rewritten, without them.
    let body: Vec<&String> = lines[s + 3..e - 2].iter().filter(|l| !is_note(l)).collect();
    let body_text: String = body.iter().map(|l| l.as_str()).collect::<Vec<_>>().join("\n");
    // The counter and the bound are read, never written, in the body.
    if body.iter().any(|l| matches!(assigned_var(l), Some(v) if v == i || v == len))
        || body_text.contains(&format!("&mut {i}"))
        || body_text.contains("continue")
    {
        return None;
    }
    // Before the loop: `let mut vI: i64 = 0i64;` and `let mut vONE: i64 = 1i64;`,
    // with nothing writing either in between; after it, no read of `vI`.
    let init = (0..s).rev().find(|&k| let_parts(&lines[k]).is_some_and(|(v, _, _)| v == i))?;
    if let_parts(&lines[init]) != Some((i, "i64", "0i64")) || indent_of(&lines[init]) != ind {
        return None;
    }
    let one_def = (0..s).rev().find(|&k| let_parts(&lines[k]).is_some_and(|(v, _, _)| v == one))?;
    if let_parts(&lines[one_def]).map(|p| p.2) != Some("1i64") {
        return None;
    }
    let between = &lines[init + 1..s];
    if between.iter().filter(|l| !is_note(l)).any(|l| assigned_var(l) == Some(i) || assigned_var(l) == Some(one) || l.contains(&format!("&mut {i}"))) {
        return None;
    }
    let fn_end = (e + 1..lines.len()).find(|&k| lines[k] == "}").unwrap_or(lines.len());
    if lines[e + 1..fn_end].iter().any(|l| !is_note(l) && mentions(l, i)) {
        return None;
    }
    // The iterator source: `let mut vLEN: i64 = vS.len() as i64;` and the body
    // reading `vS` only as `vS[vI]`.
    let source = iter_source(lines, s, len, i, &body);
    // The collected result: the last body line pushes into a list declared
    // `Vec::with_capacity(vLEN…)` before the loop.
    let collected = collected_result(lines, s, len, &body, &body_text);

    // The lines before the loop, without the counter's declaration (and the
    // collected list's, below): both are bound by the iterator instead.
    let mut drop_lines = vec![init];
    let elem = format!("__e_{i}");
    let rewrite_body = |ls: &[&String]| -> Vec<String> {
        ls.iter()
            .map(|l| match &source {
                Some((_, ix)) => l.replace(ix.as_str(), &format!("(*{elem})")),
                None => l.to_string(),
            })
            .collect()
    };
    let counter = format!("{inner}let {i}: i64 = {i} as i64;");
    // The push itself is the closure's value, so only the lines before it are judged.
    let collected = collected.filter(|_| {
        let before = &body[..body.len() - 1];
        closure_safe(before, &before.iter().map(|l| l.as_str()).collect::<Vec<_>>().join("\n"))
    });
    if let Some((_, _, decl, _)) = &collected {
        drop_lines.push(*decl);
    }
    let mut out: Vec<String> = (0..s).filter(|k| !drop_lines.contains(k)).map(|k| lines[k].clone()).collect();
    match collected {
        Some((r, rty, _, expr)) => {
            let head = match &source {
                Some((src, _)) => format!("{pad}let mut {r}: {rty} = {src}.iter().enumerate().map(|({i}, {elem})| {{"),
                None => format!("{pad}let mut {r}: {rty} = (0i64..{len}).map(|{i}: i64| {{"),
            };
            out.push(head);
            if source.is_some() {
                out.push(counter);
            }
            out.extend(rewrite_body(&body[..body.len() - 1]));
            out.push(format!("{inner}{expr}"));
            out.push(format!("{pad}}}).collect();"));
        }
        _ => {
            let head = match &source {
                Some((src, _)) => format!("{pad}for ({i}, {elem}) in {src}.iter().enumerate() {{"),
                None => format!("{pad}for {i} in 0i64..{len} {{"),
            };
            out.push(head);
            if source.is_some() {
                out.push(counter);
            }
            out.extend(rewrite_body(&body));
            out.push(format!("{pad}}}"));
        }
    }
    out.extend(lines[e + 1..].iter().cloned());
    Some(out)
}

/// `vS` and the index text `vS[IDX]` to replace, when `vLEN` is `vS.len()`, the
/// body's one offset is `vI * 8`, and the body reads `vS` only through it.
fn iter_source(lines: &[String], s: usize, len: &str, i: &str, body: &[&String]) -> Option<(String, String)> {
    let def = (0..s).rev().find(|&k| let_parts(&lines[k]).is_some_and(|(v, _, _)| v == len))?;
    let (_, _, rhs) = let_parts(&lines[def])?;
    let src = rhs.strip_suffix(".len() as i64")?;
    if !is_var(src) {
        return None;
    }
    // `let mut vO: i64 = vI.wrapping_mul(vEIGHT);` with `vEIGHT` = 8.
    let (o, eight) = body.iter().find_map(|l| {
        let (o, _, rhs) = let_parts(l)?;
        let k = rhs.strip_prefix(&format!("{i}.wrapping_mul("))?.strip_suffix(')')?;
        Some((o, k))
    })?;
    let eight_line = lines[..s].iter().chain(body.iter().copied()).rev().find(|l| let_parts(l).is_some_and(|(v, _, _)| v == eight))?;
    if let_parts(eight_line).map(|p| p.2) != Some("8i64") {
        return None;
    }
    let ix = format!("{src}[(0usize + (({o} as u64) >> 3u32) as usize)]");
    let reads_only_ix = body.iter().all(|l| !mentions(&l.replace(&ix, ""), src));
    (reads_only_ix && body.iter().any(|l| l.contains(&ix))).then(|| (src.to_string(), ix))
}

/// (result, its type, its declaration line, the pushed expression).
fn collected_result(lines: &[String], s: usize, len: &str, body: &[&String], body_text: &str) -> Option<(String, String, usize, String)> {
    let last = body.last()?.trim();
    let (r, rest) = last.split_once(".push(")?;
    let expr = rest.strip_suffix(");")?;
    if !is_var(r) {
        return None;
    }
    let decl = (0..s).rev().find(|&k| let_parts(&lines[k]).is_some_and(|(v, _, _)| v == r))?;
    let (_, rty, rhs) = let_parts(&lines[decl])?;
    if rhs != format!("Vec::with_capacity({len}.max(0) as usize)") {
        return None;
    }
    // Read nowhere else in the body (the push is its only use) nor between
    // its declaration and the loop.
    let without_push = &body_text[..body_text.len() - body.last()?.len()];
    if mentions(without_push, r) || mentions(expr, r) || lines[decl + 1..s].iter().any(|l| !is_note(l) && mentions(l, r)) {
        return None;
    }
    Some((r.to_string(), rty.to_string(), decl, expr.to_string()))
}

/// A body that can run as a `map` closure: it leaves only by finishing, and it
/// writes nothing declared outside it.
fn closure_safe(body: &[&String], body_text: &str) -> bool {
    if ["break", "continue", "return", "?;", "?)", "std::process::exit"].iter().any(|k| body_text.contains(k)) {
        return false;
    }
    let declared: Vec<&str> = body.iter().filter_map(|l| let_parts(l).map(|p| p.0)).collect();
    let writes_outer = body.iter().any(|l| matches!(assigned_var(l), Some(v) if !declared.contains(&v)));
    let borrows_outer = body_text.match_indices("&mut ").any(|(k, _)| {
        let v: String = body_text[k + 5..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        !declared.contains(&v.as_str())
    });
    let outer_method = body.iter().any(|l| {
        ["push(", "pop(", "insert(", "remove(", "clear(", "truncate(", "extend(", "swap(", "sort", "retain("]
            .iter()
            .any(|m| l.match_indices(&format!(".{m}")).any(|(k, _)| {
                let head = &l[..k];
                let v = head.rsplit(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("");
                is_var(v) && !declared.contains(&v)
            }))
    });
    !writes_outer && !borrows_outer && !outer_method
}

#[cfg(test)]
mod tests {
    use super::iterator_loops;

    const MAP: &str = "    let mut v4: Vec<i64> = almd_build(v3);
    let mut v8: i64 = v4.len() as i64;
    let mut v9: Vec<i64> = Vec::with_capacity(v8.max(0) as usize);
    let mut v11: i64 = 0i64;
    let mut v12: i64 = 1i64;
    loop {
        let v13: bool = v11 < v8;
        if !v13 { break; }
        let mut v15: i64 = 8i64;
        let mut v14: i64 = v11.wrapping_mul(v15);
        // v18: v4 + 12 bytes
        let mut v16: i64 = v4[(0usize + ((v14 as u64) >> 3u32) as usize)];
        // v23: v9 + 12 bytes
        let mut v21: i64 = v16.wrapping_add(v12);
        v9.push(v21);
        let mut v25: i64 = v11.wrapping_add(v12);
        v11 = v25;
    }
    let mut v29: i64 = v9.len() as i64;
}
";

    #[test]
    fn a_map_loop_over_a_list_collects_from_its_iterator() {
        let got = iterator_loops(MAP.to_string());
        assert!(got.contains("let mut v9: Vec<i64> = v4.iter().enumerate().map(|(v11, __e_v11)| {"), "{got}");
        assert!(got.contains("let mut v16: i64 = (*__e_v11);"), "{got}");
        assert!(got.contains("        v21\n    }).collect();"), "{got}");
        assert!(!got.contains("v9.push") && !got.contains("Vec::with_capacity") && !got.contains("let mut v11: i64 = 0i64;"), "{got}");
    }

    #[test]
    fn a_fold_loop_becomes_a_for_over_the_list() {
        let src = MAP.replace("        v9.push(v21);\n", "        v9 = v21;\n").replace("Vec<i64> = Vec::with_capacity(v8.max(0) as usize)", "i64 = 0i64");
        let got = iterator_loops(src);
        assert!(got.contains("for (v11, __e_v11) in v4.iter().enumerate() {"), "{got}");
        assert!(got.contains("        v9 = v21;"), "{got}");
    }

    #[test]
    fn a_counter_read_after_the_loop_keeps_the_loop() {
        let src = MAP.replace("let mut v29: i64 = v9.len() as i64;", "let mut v29: i64 = v11;");
        assert_eq!(iterator_loops(src.clone()), src);
    }

    #[test]
    fn a_body_that_writes_the_source_keeps_the_index() {
        let src = MAP.replace("        v9.push(v21);\n", "        v4.push(v21);\n        v9.push(v21);\n");
        let got = iterator_loops(src);
        assert!(got.contains("for v11 in 0i64..v8 {"), "{got}");
        assert!(got.contains("v4[(0usize"), "{got}");
    }

    #[test]
    fn a_break_in_the_body_keeps_the_push() {
        let src = MAP.replace("        v9.push(v21);\n", "        if v21 > 3 { break; }\n        v9.push(v21);\n");
        let got = iterator_loops(src);
        assert!(got.contains("for (v11, __e_v11) in v4.iter().enumerate() {") && got.contains("v9.push(v21);"), "{got}");
    }
}
