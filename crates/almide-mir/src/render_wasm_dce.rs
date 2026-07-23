// Fixed-preamble dead-import/dead-function elimination.
//
// The v1 wasm renderer's preamble (`render_wasm_p3::preamble`) is a single
// hand-written WAT blob: 17 WASI imports + ~54 runtime helper functions,
// unconditionally spliced into EVERY module regardless of what the program
// actually calls (`hello.almd`'s `println("Hello, World!")` used to link
// `path_open`/`fd_readdir`/`clock_time_get`/... — filesystem, directory and
// clock imports no `println`-only program can reach). `wasm-opt` could trim
// this externally, but a v1-VERIFIED module is shipped byte-verbatim (the
// trust-spine proves exactly those bytes, and `wasm-opt` is an untrusted
// transform outside that proof — see `cli/build.rs`'s `produced_by_v1`
// comment) — so the elimination has to happen INSIDE the renderer, over
// units the trust-spine already emitted, not as an external unverified pass.
//
// This is a pure REMOVAL over already-rendered text: reachability walks the
// `$name` references the program's own emitted code and the preamble's own
// functions make, and anything unreached is dropped. Soundness follows from
// how this codebase renders calls: every call site in the WAT output is a
// symbolic `$name` (never a raw numeric function/import index), so
// `wat::parse_str` re-resolves indices from whatever text survives — a name
// still referenced is always still defined; nothing here can turn a valid
// reference into a dangling one.

/// A single top-level unit of the fixed preamble.
enum PreambleSeg {
    /// Verbatim, never-removed text: the `(module` header, `(memory ...)`,
    /// `(data ...)` segments, and `(global ...)` declarations.
    Fixed(String),
    /// An `(import "mod" "name" (func $sym ...))` or `(func $sym ...)` block,
    /// keyed by the wasm symbol it declares.
    Removable { name: String, text: String },
}

/// Find the index right after the `)` that matches the `(` at `text[start]`.
/// Skips the contents of `"..."` string literals (WASI import names / data
/// bytes can be arbitrary) so a literal paren inside one never desyncs the
/// depth count; this codebase's WAT output has no backslash-escaped quotes.
fn match_paren(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_str = !in_str,
            b'(' if !in_str => depth += 1,
            b')' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

/// Every distinct `$name` token appearing anywhere in `text` (call targets,
/// but also incidentally local variable/param names and globals — those
/// simply never match a preamble unit's name, so including them costs
/// nothing but a few no-op set entries).
fn dollar_names(text: &str) -> std::collections::BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut names = std::collections::BTreeSet::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > start {
                names.insert(text[start..end].to_string());
            }
            i = end;
        } else {
            i += 1;
        }
    }
    names
}

/// Split a fully-assembled preamble string (module header, imports,
/// memory/data/globals, funcs — in textual order, extern imports already
/// spliced in) into segments, isolating each top-level `(import ...)` /
/// `(func $name ...)` as its own [`PreambleSeg::Removable`]. Scanning always
/// jumps past a matched block's full extent, so nested content (a func body's
/// own `(call $x ...)`) is never mistaken for a new top-level unit.
fn split_preamble_segments(pre: &str) -> Vec<PreambleSeg> {
    // Byte-slice (not `&str`) pattern matching: the preamble's WAT comments
    // contain multi-byte UTF-8 (em dashes etc.), so a plain `i += 1` byte
    // walk can land mid-character — `&pre[i..]` would then panic on the next
    // iteration's `starts_with`. `bytes.starts_with` never has that problem;
    // only the final `&pre[i..end]` extraction needs `i`/`end` to be char
    // boundaries, which they always are (both sit right on an ASCII `(`/`)`).
    let bytes = pre.as_bytes();
    let mut segs = Vec::new();
    let mut fixed_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let starts_unit = bytes[i..].starts_with(b"(import ") || bytes[i..].starts_with(b"(func $");
        if !starts_unit {
            i += 1;
            continue;
        }
        let end = match_paren(pre, i);
        let block = &pre[i..end];
        match first_dollar_name(block) {
            // A block with no `$name` at all can't be referenced by anything
            // downstream — keep it as Fixed so it's never a candidate for
            // (accidental) removal.
            None => {
                i = end;
            }
            Some(name) => {
                // The DECLARED symbol is the FIRST `$name` in appearance order:
                // for `(func $x ...)` that's `$x` itself; for `(import ".."
                // ".." (func $x ...))` it's also `$x` (nothing with a `$`
                // precedes it).
                if fixed_start < i {
                    segs.push(PreambleSeg::Fixed(pre[fixed_start..i].to_string()));
                }
                segs.push(PreambleSeg::Removable { name, text: block.to_string() });
                fixed_start = end;
                i = end;
            }
        }
    }
    if fixed_start < pre.len() {
        segs.push(PreambleSeg::Fixed(pre[fixed_start..].to_string()));
    }
    segs
}

/// The first `$name` token in `block`, in appearance (not sorted) order — the
/// declared symbol of an `(import ...)` / `(func $name ...)` block is always
/// its first `$`-prefixed token.
fn first_dollar_name(block: &str) -> Option<String> {
    let bytes = block.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > start {
                return Some(block[start..end].to_string());
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

/// Drop every `(import ...)` / `(func $name ...)` in the fixed preamble that
/// nothing in the rendered module can reach. `used_text` is everything the
/// preamble precedes in the final module (data + closure table + program
/// functions + mg helpers + `_start` + public exports) — the program's own
/// emitted code, whose `$name` references seed the reachability walk. A
/// preamble function that itself calls other preamble functions (`$__div_trap`
/// calling `$fd_write`/`$proc_exit`) pulls its own callees in transitively, so
/// e.g. any program with a bounds/overflow check keeps exactly the WASI
/// imports that check's trap path needs — nothing more.
pub(crate) fn filter_unreachable_preamble(pre: &str, used_text: &str) -> String {
    let segs = split_preamble_segments(pre);
    let mut by_name: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
    for seg in &segs {
        if let PreambleSeg::Removable { name, text } = seg {
            by_name.insert(name.as_str(), text.as_str());
        }
    }

    let mut reachable: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut frontier: Vec<String> = dollar_names(used_text)
        .into_iter()
        .filter(|n| by_name.contains_key(n.as_str()))
        .collect();
    reachable.extend(frontier.iter().cloned());
    while let Some(name) = frontier.pop() {
        let Some(text) = by_name.get(name.as_str()) else { continue };
        for callee in dollar_names(text) {
            if by_name.contains_key(callee.as_str()) && reachable.insert(callee.clone()) {
                frontier.push(callee);
            }
        }
    }

    segs.into_iter()
        .map(|seg| match seg {
            PreambleSeg::Fixed(text) => text,
            PreambleSeg::Removable { name, text } => {
                if reachable.contains(&name) {
                    text
                } else {
                    String::new()
                }
            }
        })
        .collect()
}
