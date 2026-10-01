//! Every host-op call site in the emitter names its op through an `OP_*`
//! constant, never a bare integer (#3148, the #2090 class).
//!
//! A host op is not only a dispatch number: the host spells the failing call's
//! name from it (`almide_wasi::fs_op_name`), so the op a call rides IS the name
//! its error carries. #2090 gave `fs.fold_lines` / `fs.for_each_line` their own
//! ops (51/52) for exactly that reason, but the ADR-0006 fallible carriers kept
//! `self.fs_call_1(p, 12)?; // OP_READ_LINES` — a literal the rename could not
//! see — and a missing file read `fs.read_lines("…")` on wasm where native said
//! `fs.fold_lines("…")`. A constant is greppable and is where the op's owner is
//! decided; a literal with a comment is neither. This gate refuses the literal.

use std::path::Path;

/// The emitter calls that hand the host an op number.
const OP_SITES: &[&str] = &[
    "fs_call_0(",
    "fs_call_1(",
    "fs_call_str2(",
    "note_host_op(",
    "fs_range_read(",
    "fs_range_call(",
];

/// The op argument of a call starting at `rest` (just past `(`): the LAST
/// top-level comma-separated argument, which is where every op site takes it.
fn last_arg(rest: &str) -> Option<&str> {
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return Some(rest[start..i].trim()),
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => start = i + 1,
            _ => {}
        }
    }
    None
}

fn is_int_literal(arg: &str) -> bool {
    let a = arg.trim_start_matches('-');
    !a.is_empty() && a.chars().all(|c| c.is_ascii_digit() || c == '_')
}

/// Every `.rs` file under `root`, recursively.
fn rust_sources(root: std::path::PathBuf) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    files
}

/// The op argument of every `site` call on one comment-stripped line.
fn op_args<'a>(code: &'a str, site: &str) -> Vec<&'a str> {
    let mut args = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find(site) {
        let open = from + at + site.len();
        from = open;
        // A definition (`fn fs_call_1(&mut self, …`) is not a site.
        let is_def = code[..open - site.len()].trim_end().ends_with("fn");
        if let Some(arg) = last_arg(&code[open..]).filter(|_| !is_def) {
            args.push(arg);
        }
    }
    args
}

#[test]
fn no_host_op_site_takes_a_bare_integer() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut sites = 0;
    for path in rust_sources(src) {
        let text = std::fs::read_to_string(&path).expect("read source");
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let args: Vec<&str> = OP_SITES.iter().flat_map(|site| op_args(code, site)).collect();
            sites += args.len();
            // The `op` parameter a helper forwards is a name too.
            if args.iter().any(|a| is_int_literal(a)) {
                offenders.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
            }
        }
    }
    // The scan must be looking at real call sites, or it passes vacuously.
    assert!(sites > 40, "the op-site scan found only {sites} sites — the call shapes changed under it");
    assert!(
        offenders.is_empty(),
        "host-op call sites take a bare integer; name the op with an OP_* constant so the \
         call it reports is decided where its owner is (#3148):\n{}",
        offenders.join("\n")
    );
}
