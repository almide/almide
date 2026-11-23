//! Probe only: each defined function compared, instruction for instruction,
//! with the four runtime bodies proofs/StructuralDecode.v proves
//! (`inc_bytes`, `dec_bytes`, `free_bytes`, `alloc_bytes`).

use wasmparser::Operator as Op;

use crate::decode::Body;

/// The proven lists, one instruction per token (generated from the .v file).
const LISTS: &str = include_str!("proven_bodies.txt");

/// One proven body: its name and each instruction's bytes.
pub struct Proven {
    pub name: &'static str,
    pub insts: Vec<Vec<u8>>,
}

/// The proven bodies, in the file's order.
pub fn proven() -> Vec<Proven> {
    let mut out = Vec::new();
    for l in LISTS.lines() {
        let Some((name, rest)) = l.split_once('\t') else { continue };
        let mut insts: Vec<Vec<u8>> = Vec::new();
        for t in rest.split(' ') {
            if t.is_empty() {
                continue;
            }
            insts.push(t.split(',').filter_map(|b| b.parse::<u8>().ok()).collect());
        }
        out.push(Proven { name, insts });
    }
    out
}

/// How one shipped instruction stands against a proven one.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Diff {
    Same,
    /// Both calls, to different function indices.
    Call,
    /// Both `i32.const`, of different values.
    Const,
    /// Both the same global operator, on different globals.
    Global,
    Other,
}

fn diff(module: &[u8], b: &Body<'_>, k: usize, want: &[u8]) -> Diff {
    let hi = b.at.get(k + 1).copied().unwrap_or(b.end);
    let Some(lo) = b.at.get(k).copied() else { return Diff::Other };
    let Some(got) = module.get(lo..hi) else { return Diff::Other };
    if got == want {
        return Diff::Same;
    }
    let first = want.first().copied();
    match &b.ops[k] {
        Op::Call { .. } if first == Some(0x10) => Diff::Call,
        Op::I32Const { .. } if first == Some(0x41) => Diff::Const,
        Op::GlobalGet { .. } if first == Some(0x23) => Diff::Global,
        Op::GlobalSet { .. } if first == Some(0x24) => Diff::Global,
        _ => Diff::Other,
    }
}

/// The instructions `b.ops[i..i + n]`, spelled without spaces.
fn text(b: &Body<'_>, i: usize, n: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    for o in b.ops.iter().skip(i).take(n) {
        parts.push(format!("{o:?}").replace(' ', ""));
    }
    parts.join(".")
}

/// `b` against the proven instructions `want`, as one line: how many
/// instructions are identical, which proven positions differ only in a call
/// target, a constant or a global index, and where the two do not line up
/// (`+j:` instructions the module has before proven position j, `-jxN` N
/// proven instructions the module lacks, `!j:` an instruction that is simply
/// different). `None` when the body is nothing like the proven one, unless
/// `always`.
pub fn compare(module: &[u8], b: &Body<'_>, want: &[Vec<u8>], always: bool) -> Option<String> {
    let n = b.ops.len();
    let m = want.len();
    if !always && (n + 16 < m || m + 16 < n) {
        return None;
    }
    let near = |i: usize, j: usize| i < n && j < m && diff(module, b, i, &want[j]) != Diff::Other;
    let run = |i: usize, j: usize| (0..3usize).all(|d| (i + d >= n && j + d >= m) || near(i + d, j + d));
    let (mut i, mut j) = (0usize, 0usize);
    let mut same = 0usize;
    let mut calls: Vec<String> = Vec::new();
    let mut consts: Vec<String> = Vec::new();
    let mut globals = 0usize;
    let mut edits: Vec<String> = Vec::new();
    while i < n && j < m {
        match diff(module, b, i, &want[j]) {
            Diff::Same => same += 1,
            Diff::Call => {
                let f = match &b.ops[i] {
                    Op::Call { function_index } => *function_index,
                    _ => u32::MAX,
                };
                calls.push(format!("{j}>{f}"));
            }
            Diff::Const => consts.push(j.to_string()),
            Diff::Global => globals += 1,
            Diff::Other => {
                let mut moved = false;
                for k in 1..=6usize {
                    if run(i + k, j) {
                        edits.push(format!("+{j}:{}", text(b, i, k)));
                        i += k;
                        moved = true;
                        break;
                    }
                    if run(i, j + k) {
                        edits.push(format!("-{j}x{k}"));
                        j += k;
                        moved = true;
                        break;
                    }
                }
                if moved {
                    continue;
                }
                edits.push(format!("!{j}:{}", text(b, i, 1)));
                if edits.len() > 12 && !always {
                    return None;
                }
            }
        }
        i += 1;
        j += 1;
    }
    let close = same + calls.len() + consts.len() + globals;
    if !always && (edits.len() > 8 || close * 10 < m * 8) {
        return None;
    }
    edits.truncate(12);
    Some(format!(
        "ops={n}/{m} same={same} call={}[{}] const={}[{}] global={globals} edits={}[{}] tail={}/{}",
        calls.len(),
        calls.join(","),
        consts.len(),
        consts.join(","),
        edits.len(),
        edits.join(";"),
        n - i,
        m - j
    ))
}
