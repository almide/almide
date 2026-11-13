//! A counted reduction over a range, spelled as the range's `fold` (#4661).
//!
//! A `list.range |> filter |> map |> fold` chain fuses to one counted loop
//! (`native_reduce_fusion`) whose body keeps or steps one scalar:
//!
//! ```text
//! let mut vI: i64 = 0i64;
//! loop {
//!     let mut vC: i64 = ((vI as u64) < (vN as u64)) as i64;
//!     if vC == 0 { break; }
//!     … if keep { vT = step(vA, …) } else { vT = vA } …
//!     vA = vT;
//!     let mut vK: i64 = vI.wrapping_add(vONE);    // vONE is 1
//!     vI = vK;
//! }
//! ```
//!
//! Written as a loop, the backend turns the keep-or-step into a select and
//! computes the step on every round, kept or not; a filter that keeps a third
//! of the elements then ran 1.4x the standard codegen's iterator chain on
//! arm64, which branches over the step. The same rounds as the range's own
//! fold keep the branch:
//!
//! ```text
//! vA = (0u64..(vN as u64)).fold(vA, |mut vA: i64, vI: u64| -> i64 {
//!     let vI: i64 = vI as i64;
//!     … body …
//!     vA
//! });
//! ```
//!
//! The rounds are the loop's: `vI` runs 0, 1, … while `(vI as u64) < (vN as
//! u64)`, the body in the same order on the same values, the accumulator
//! handed from round to round. A Float accumulator is stepped in the same
//! order, so its bits are the loop's.
//!
//! Only a body that writes exactly one variable declared outside it — the
//! accumulator, a scalar — and leaves by no `break` / `continue` / `return` /
//! `?` is rewritten, and only when the counter is read nowhere after the
//! loop. Anything else stays as rendered.

use crate::render_native_loops::{assigned_var, indent_of, is_note, is_var, let_parts, mentions};

/// Rewrite every matching reduction loop of one rendered function body.
pub(crate) fn fold_loops(out: String) -> String {
    let mut lines: Vec<String> = out.lines().map(str::to_string).collect();
    let starts: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].trim() == "loop {").collect();
    let mut changed = false;
    for &s in starts.iter().rev() {
        if let Some(new) = rewrite_at(&lines, s) {
            lines = new;
            changed = true;
        }
    }
    if !changed {
        return out;
    }
    let mut s = lines.join("\n");
    if out.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// The declaration of `v` before line `s`: its line and type.
fn declared_before<'a>(lines: &'a [String], s: usize, v: &str) -> Option<(usize, &'a str, &'a str)> {
    let k = (0..s).rev().find(|&k| let_parts(&lines[k]).is_some_and(|(x, _, _)| x == v))?;
    let (_, ty, rhs) = let_parts(&lines[k])?;
    Some((k, ty, rhs))
}

fn rewrite_at(lines: &[String], s: usize) -> Option<Vec<String>> {
    let ind = indent_of(&lines[s]);
    let (pad, inner) = (" ".repeat(ind), " ".repeat(ind + 4));
    // The header: `let mut vC: i64 = ((vI as u64) < (vN as u64)) as i64;` then `if vC == 0 { break; }`.
    let (c, cty, test) = let_parts(lines.get(s + 1)?)?;
    let (i, n) = test.strip_prefix("((")?.strip_suffix(" as u64)) as i64")?.split_once(" as u64) < (")?;
    if cty != "i64" || !is_var(i) || !is_var(n) || lines.get(s + 2)?.trim() != format!("if {c} == 0 {{ break; }}") {
        return None;
    }
    let e = (s + 1..lines.len())
        .find(|&k| !lines[k].trim().is_empty() && indent_of(&lines[k]) <= ind)
        .filter(|&k| indent_of(&lines[k]) == ind && lines[k].trim() == "}")?;
    // The step: `let mut vK: i64 = vI.wrapping_add(vONE);` then `vI = vK;`.
    let (k, kty, step) = let_parts(lines.get(e - 2)?)?;
    let one = step.strip_prefix(&format!("{i}.wrapping_add("))?.strip_suffix(')')?;
    if kty != "i64" || !is_var(one) || lines[e - 1].trim() != format!("{i} = {k};") {
        return None;
    }
    // The counter starts at 0 beside the loop, steps by 1, and is read nowhere after it.
    let (init, ity, irhs) = declared_before(lines, s, i)?;
    if (ity, irhs) != ("i64", "0i64") || indent_of(&lines[init]) != ind || declared_before(lines, s, one)?.2 != "1i64" {
        return None;
    }
    let touched = |l: &String| !is_note(l) && (matches!(assigned_var(l), Some(v) if v == i || v == one || v == n) || l.contains(&format!("&mut {i}")));
    let fn_end = (e + 1..lines.len()).find(|&j| lines[j] == "}").unwrap_or(lines.len());
    if lines[init + 1..s].iter().any(touched) || lines[e + 1..fn_end].iter().any(|l| !is_note(l) && mentions(l, i)) {
        return None;
    }
    let body: Vec<&String> = lines[s + 3..e - 2].iter().filter(|l| !is_note(l)).collect();
    let text: String = body.iter().map(|l| l.as_str()).collect::<Vec<_>>().join("\n");
    if ["break", "continue", "return", "?;", "?)", "std::process::exit", "&mut ", "loop {", "for ", "while "].iter().any(|w| text.contains(w)) || mentions(&text, c) {
        return None;
    }
    // Exactly one variable declared outside the body is written in it.
    let inside: Vec<&str> = body.iter().filter_map(|l| let_parts(l).map(|p| p.0)).collect();
    let mut outer: Vec<&str> = body.iter().filter_map(|l| assigned_var(l)).filter(|v| !inside.contains(v)).collect();
    outer.dedup();
    outer.sort_unstable();
    outer.dedup();
    let [acc] = outer.as_slice() else { return None };
    if *acc == i || *acc == n || *acc == one {
        return None;
    }
    let (_, ty, _) = declared_before(lines, s, acc)?;
    if !matches!(ty, "i64" | "f64") {
        return None;
    }
    // A method that edits its receiver would edit a captured outer value.
    let edits = ["push(", "pop(", "insert(", "remove(", "clear(", "truncate(", "extend(", "swap(", "sort", "retain(", "put(", "set_"];
    if body.iter().any(|l| edits.iter().any(|m| l.contains(&format!(".{m}")))) {
        return None;
    }
    let mut out: Vec<String> = (0..s).filter(|j| *j != init).map(|j| lines[j].clone()).collect();
    out.push(format!("{pad}{acc} = (0u64..({n} as u64)).fold({acc}, |mut {acc}: {ty}, {i}: u64| -> {ty} {{"));
    out.push(format!("{inner}let {i}: i64 = {i} as i64;"));
    out.extend(lines[s + 3..e - 2].iter().cloned());
    out.push(format!("{inner}{acc}"));
    out.push(format!("{pad}}});"));
    out.extend(lines[e + 1..].iter().cloned());
    Some(out)
}

#[cfg(test)]
#[path = "render_native_fold_loops_test.rs"]
mod tests;
