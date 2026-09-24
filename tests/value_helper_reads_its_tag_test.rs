//! A function that LOADS the Value payload slot must first read the TAG (#2395).
//!
//! A wasm Value is a fixed 16-byte block `[rc][len][cap][tag @SUM_TAG][pad]
//! [payload:8B @SUM_FIELD]`. What the payload slot MEANS is decided entirely by
//! the tag — a pairs-list handle under Object, an i64 under Int, a string
//! handle under Str — and under Null it means nothing at all, because
//! `emit_value_box(tag, None)` allocates the block, stores the tag and returns
//! without writing the slot, so it holds whatever the reused block last held.
//!
//! `emit_value_keys_helper` loaded the slot with no tag test and walked it as a
//! block: an Int's payload as an address, a Str's payload as a pairs list,
//! Null's stale word as either. Native answers `[]` for every non-object, so
//! the wasm leg either trapped or returned a wrong list depending on what was
//! in memory.
//!
//! ## What this gate is, and three ways it could have been built wrong
//!
//! Found by almide-ee's sweep of the whole crate, each one verified here:
//!
//! 1. **Keying on `i32_load(slot_memarg(..SUM_FIELD))` finds ONE site, and it
//!    is not the bug.** The real code binds the memarg to a local first
//!    (`let m_pay = slot_memarg(..SUM_FIELD); … .i32_load(m_pay)`), so the
//!    direct form is invisible. This gate follows the binding, and
//!    `the_gate_fires_on_the_pre_fix_body` holds it to the pre-fix source.
//! 2. **`SUM_TAG` is 0, and `slot_memarg(0)` is all over the crate for
//!    slot-0 reads that are NOT tags** (list elements, closure envs, pair
//!    slots — `emit_value_keys_helper`'s own key read is one). Accepting a
//!    bare `0` as a tag read would have exonerated the very function this
//!    gate exists for. So only the named constant counts, and the two real
//!    tag reads that were spelled `slot_memarg(0)` (`lower_json_to_map`) were
//!    normalised to `slot_memarg(almide_layout::SUM_TAG)` — byte-identical
//!    output, and a guard a tool can see. A guard spelled as a magic number
//!    is the same defect class one level up.
//! 3. **Regex function boundaries invent members.** A doc comment belongs to
//!    the function BELOW it; a line-regex segmenter sweeps it into the one
//!    above and reports a phantom. This splits by brace depth from the `fn`
//!    line, so a doc comment falls outside both.
//!
//! This is a SOURCE-shape gate: it cannot tell a real guard from a mention of
//! the constant, and a function that computes the offset numerically
//! (`slot_memarg(8)`) is invisible to it — a real gap, not a clean bill. The
//! behavioural half is `spec/wasm_cross/value_keys_non_object.almd`, which runs
//! every `value.*` reader over every tag on both legs. Neither is sufficient
//! alone: the fixture cannot see a helper no program reaches, and this cannot
//! see a tag read that guards the wrong thing.

use std::path::PathBuf;

/// Functions that load the payload slot under a guarantee their CALLER
/// establishes rather than their own body. Empty today. A name added here is a
/// claim that the call sites are exhaustive and were checked — say which.
const CALLER_GUARANTEED: &[&str] = &[];

fn wasm_src_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("crates/almide-wasm/src")
}

/// Strip `//` comments and string literals so brace counting cannot be thrown
/// by a `{` inside either.
fn code_only(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_str = false;
    let mut escaped = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '/' if chars.peek() == Some(&'/') => break,
            _ => out.push(c),
        }
    }
    out
}

/// Split a Rust source file into `(fn name, body)` by brace depth from each
/// top-level `fn` line. Anything between one function's close and the next
/// function's `fn` line — doc comments, `const`s, blank lines — belongs to
/// neither.
fn functions(src: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    let mut idx = 0usize;
    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim_start();
        let is_fn_line = (trimmed.starts_with("fn ")
            || trimmed.starts_with("pub fn ")
            || trimmed.starts_with("pub(crate) fn "))
            && line.contains('(');
        if !is_fn_line {
            idx += 1;
            continue;
        }
        let name = trimmed
            .rsplit_once("fn ")
            .map(|(_, r)| r)
            .unwrap_or(trimmed)
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()
            .unwrap_or("")
            .to_string();
        let start = idx;
        let mut depth = 0i32;
        let mut opened = false;
        while idx < lines.len() {
            for c in code_only(lines[idx]).chars() {
                match c {
                    '{' => {
                        depth += 1;
                        opened = true;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            idx += 1;
            if opened && depth <= 0 {
                break;
            }
        }
        if !name.is_empty() {
            out.push((name, lines[start..idx].join("\n")));
        }
    }
    out
}

/// Does this body LOAD from the Value payload slot — directly, or through a
/// local the body bound to that memarg?
fn loads_the_payload_slot(body: &str) -> bool {
    let direct = ["i32_load", "i64_load", "f64_load", "i32_load8_u"]
        .iter()
        .any(|op| {
            body.contains(&format!("{op}(slot_memarg(almide_layout::SUM_FIELD))"))
                || body.contains(&format!("{op}(slot_memarg(SUM_FIELD))"))
        });
    if direct {
        return true;
    }
    // `let m_pay = slot_memarg(almide_layout::SUM_FIELD);` … `.i32_load(m_pay)`
    for line in body.lines() {
        let code = code_only(line);
        let Some(rest) = code.trim_start().strip_prefix("let ") else {
            continue;
        };
        let Some((binding, init)) = rest.split_once('=') else {
            continue;
        };
        if !init.contains("slot_memarg(") || !init.contains("SUM_FIELD") {
            continue;
        }
        let name = binding.trim().trim_start_matches("mut ").trim();
        if name.is_empty() {
            continue;
        }
        if ["i32_load", "i64_load", "f64_load", "i32_load8_u"]
            .iter()
            .any(|op| body.contains(&format!("{op}({name})")))
        {
            return true;
        }
    }
    false
}

fn reads_the_tag(body: &str) -> bool {
    body.contains("SUM_TAG")
}

fn offenders(src: &str) -> Vec<String> {
    functions(src)
        .into_iter()
        .filter(|(name, body)| {
            loads_the_payload_slot(body)
                && !reads_the_tag(body)
                && !CALLER_GUARANTEED.contains(&name.as_str())
        })
        .map(|(name, _)| name)
        .collect()
}

/// The pre-fix opening of `emit_value_keys_helper`, verbatim. If a change to
/// this gate stops flagging THIS, the gate has stopped measuring the defect it
/// was built for — which is how the first version of it was written, and how
/// ee's first scanner behaved.
const PRE_FIX_BODY: &str = r#"
pub(crate) fn emit_value_keys_helper() -> Function {
    let (v, p, end, dst, cur) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let m_pay = slot_memarg(almide_layout::SUM_FIELD);
    let mut f = Function::new([(4, ValType::I32)]);
    let mut i = f.instructions();
    i.local_get(v).i32_load(m_pay).local_set(v); // pairs list
    i.local_get(v).i32_load(len_memarg()).call(F_ALLOC).local_set(dst);
    i.local_get(p).i32_load(raw8()).i32_load(slot_memarg(0));
    i.local_get(dst);
    i.end();
    f
}
"#;

#[test]
fn the_gate_fires_on_the_pre_fix_body() {
    assert_eq!(
        offenders(PRE_FIX_BODY),
        vec!["emit_value_keys_helper".to_string()],
        "this gate no longer flags the source it was built for. Note the \
         pre-fix body DOES contain `slot_memarg(0)` — its pair-key read — so a \
         gate that accepted a bare 0 as a tag read would report nothing here."
    );
}

#[test]
fn the_gate_clears_a_guarded_body() {
    let guarded = PRE_FIX_BODY.replace(
        "let mut i = f.instructions();",
        "let mut i = f.instructions();\n    i.local_get(v)\
         .i32_load(slot_memarg(almide_layout::SUM_TAG));",
    );
    assert!(
        offenders(&guarded).is_empty(),
        "the guarded form must pass, or the gate is unsatisfiable"
    );
}

#[test]
fn a_doc_comment_belongs_to_the_function_below_it() {
    // ee's phantom: a line-regex segmenter sweeps the NEXT function's doc
    // comment backwards into the previous body, inventing a member.
    let src = "\
fn clean() -> u32 {
    let m_pay = slot_memarg(almide_layout::SUM_FIELD);
    i.i32_load(m_pay);
    i.i32_load(slot_memarg(almide_layout::SUM_TAG));
    0
}

/// Mentions SUM_FIELD in prose and loads nothing.
fn other() -> u32 {
    1
}
";
    let fns = functions(src);
    assert_eq!(fns.len(), 2, "expected two functions, got {fns:?}");
    assert!(
        !fns[0].1.contains("Mentions SUM_FIELD in prose"),
        "the doc comment of `other` was swept into `clean`"
    );
    assert!(offenders(src).is_empty());
}

#[test]
fn every_payload_slot_load_in_the_wasm_crate_reads_the_tag() {
    let dir = wasm_src_dir();
    let mut checked = 0usize;
    let mut all: Vec<String> = Vec::new();
    let mut readers: Vec<String> = Vec::new();

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    entries.sort();

    for path in &entries {
        let src = std::fs::read_to_string(path).expect("read");
        checked += 1;
        for (name, body) in functions(&src) {
            if loads_the_payload_slot(&body) {
                readers.push(name.clone());
                if !reads_the_tag(&body) && !CALLER_GUARANTEED.contains(&name.as_str()) {
                    all.push(format!("{}::{name}", path.file_name().unwrap().to_string_lossy()));
                }
            }
        }
    }

    assert!(checked > 10, "only {checked} files scanned — wrong directory?");
    // Non-vacuity: if SUM_FIELD is renamed or the emitter restructured, this
    // fires instead of going quietly green on an empty candidate set.
    assert!(
        readers.len() >= 4,
        "only {} payload-slot loaders found ({readers:?}) — has SUM_FIELD been \
         renamed, or the loads moved behind a helper? This gate must not pass \
         vacuously.",
        readers.len()
    );

    assert!(
        all.is_empty(),
        "these functions load the Value payload slot (SUM_FIELD) without \
         reading the tag (SUM_TAG): {all:?}\n\n\
         The payload slot only means what the tag says it means, and under tag \
         Null it is never written at all — loading it unguarded walks whatever \
         the reused block last held (#2395). Either guard on the tag, or add \
         the name to CALLER_GUARANTEED naming the call sites that establish it."
    );
}
