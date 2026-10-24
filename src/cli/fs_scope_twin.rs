//! The scoped twin of the native fs runtime (#4084, C-393).
//!
//! Under `[permissions] fs` every fs call must run beneath the granted root's
//! held descriptor (almide-rt-core fs_scoped_core.rs), not as the plain
//! `std::fs` call on a path the scope checked a moment before. The runtime's
//! own fs functions stay as they are — their source is what every stdlib
//! inspection stamp fingerprints, and a program without the key runs them
//! unchanged — so a scoped build calls a TWIN of them instead: the same
//! function bodies, derived here from the runtime's embedded source, with
//! every `std::fs::` call spelled `almide_sfs::` (the facade with std's names
//! and shapes, bounded by the installed scope) and the path predicates
//! `.exists()` / `.is_dir()` / `.is_file()` / `.is_symlink()` spelled through
//! `almide_sfs::FsScopedPathExt`. The twin is the closure of the guarded entry
//! points over the module's top-level fns they name, so helpers the twin
//! needs are twinned too and everything else resolves to the runtime's own
//! items through `use super::*`.
//!
//! Derived from the source, never from a list of fn names: a runtime fs fn
//! added later is twinned by the same rewrite, and [`twin_leaks`] names any
//! fs-reaching spelling the rewrite does not cover (the unit test below runs
//! it over the real runtime source).

use std::collections::{BTreeMap, BTreeSet};

/// The module the twin is emitted as; guard macros call
/// `__almide_scoped_fs::<fn>`.
pub(crate) const TWIN_MOD: &str = "__almide_scoped_fs";

/// One top-level fn of the runtime source: its name and its text
/// (attributes included).
struct Item {
    name: String,
    text: String,
}

/// The twin module for `entries` over the fs runtime `src`, and the entry
/// names it holds.
pub(crate) fn scoped_twin(src: &str, entries: &BTreeSet<String>) -> (String, BTreeSet<String>) {
    let items = top_level_fns(src);
    let by_name: BTreeMap<&str, &Item> = items.iter().map(|i| (i.name.as_str(), i)).collect();
    let mut keep: BTreeSet<String> = entries.iter().filter(|e| by_name.contains_key(e.as_str())).cloned().collect();
    let mut work: Vec<String> = keep.iter().cloned().collect();
    while let Some(n) = work.pop() {
        for id in idents(&by_name[n.as_str()].text) {
            if by_name.contains_key(id) && keep.insert(id.to_string()) {
                work.push(id.to_string());
            }
        }
    }
    let mut out = format!(
        "\n// The scoped twin of the fs runtime (#4084): src/cli/fs_scope_twin.rs.\n#[allow(unused_imports, dead_code, unused_variables, unused_mut, unused_parens)]\nmod {TWIN_MOD} {{\nuse super::*;\nuse std::path::Path;\nuse almide_sfs::FsScopedPathExt as _;\n"
    );
    for i in &items {
        if keep.contains(&i.name) {
            out.push_str(&rewrite(&i.text));
            out.push('\n');
        }
    }
    out.push_str("}\n");
    let twinned = entries.iter().filter(|e| keep.contains(*e)).cloned().collect();
    (out, twinned)
}

/// The fs-reaching rewrite of one fn's text.
fn rewrite(text: &str) -> String {
    let mut s = text.replace("std::fs::", "almide_sfs::");
    for m in ["exists", "is_dir", "is_file", "is_symlink"] {
        s = s.replace(&format!(".{m}()"), &format!(".__almide_s_{m}()"));
    }
    s
}

/// Spellings in a rewritten twin that reach the filesystem without going
/// through the facade: `std::fs` / `fs::` paths and `Path`'s own fs methods.
#[cfg(test)]
fn twin_leaks(twin: &str) -> Vec<String> {
    let mut out = Vec::new();
    for pat in [
        "std::fs::",
        "use std::fs",
        ".read_dir(",
        ".canonicalize(",
        ".symlink_metadata(",
        ".read_link(",
        ".try_exists(",
        "File::create(",
        "std::os::",
        "libc::",
    ] {
        if twin.contains(pat) {
            out.push(pat.to_string());
        }
    }
    for line in twin.lines() {
        // `Path::metadata` follows; only a File's (fd-based) is allowed.
        if line.contains(".metadata()") && !line.contains("file.metadata()") {
            out.push(format!("unexpected .metadata(): {}", line.trim()));
        }
    }
    out
}

/// The identifiers of `text` (strings and comments included: an extra
/// helper twinned costs nothing).
fn idents(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|w| w.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_'))
}

/// The top-level fns of `src`: a line at column 0 opening `fn` (optionally
/// `pub`), with the `#[..]` lines right above it, through its body's closing
/// brace.
fn top_level_fns(src: &str) -> Vec<Item> {
    let lines: Vec<&str> = src.split_inclusive('\n').collect();
    let mut starts = Vec::with_capacity(lines.len());
    let mut off = 0;
    for l in &lines {
        starts.push(off);
        off += l.len();
    }
    let mut out = Vec::new();
    let mut k = 0;
    while k < lines.len() {
        let l = lines[k];
        let head = l.strip_prefix("pub(crate) ").or_else(|| l.strip_prefix("pub ")).unwrap_or(l);
        let Some(rest) = head.strip_prefix("fn ") else {
            k += 1;
            continue;
        };
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        let mut a = k;
        while a > 0 && lines[a - 1].starts_with("#[") {
            a -= 1;
        }
        let Some(end) = body_end(src, starts[k]) else {
            k += 1;
            continue;
        };
        out.push(Item { name, text: src[starts[a]..end].to_string() });
        k = lines.iter().enumerate().skip(k).find(|(i, _)| starts[*i] >= end).map_or(lines.len(), |(i, _)| i);
    }
    out
}

/// The byte just past the brace that closes the first `{` after `from`,
/// skipping strings, chars and comments.
fn body_end(src: &str, from: usize) -> Option<usize> {
    let b = src.as_bytes();
    let mut i = from;
    let mut depth = 0i32;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 1;
            }
            b'r' if (b.get(i + 1) == Some(&b'"') || b.get(i + 1) == Some(&b'#')) && !b[i.saturating_sub(1)].is_ascii_alphanumeric() => {
                let mut j = i + 1;
                let mut hashes = 0;
                while b.get(j) == Some(&b'#') {
                    hashes += 1;
                    j += 1;
                }
                if b.get(j) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                let close = format!("\"{}", "#".repeat(hashes));
                i = j + 1 + src[j + 1..].find(&close)? + close.len() - 1;
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'\'' => {
                if b.get(i + 1) == Some(&b'\\') {
                    i += 2;
                    while i < b.len() && b[i] != b'\'' {
                        i += 1;
                    }
                } else if b.get(i + 2) == Some(&b'\'') {
                    i += 2;
                }
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs_src() -> &'static str {
        almide::codegen::generated::rust_runtime::RUST_RUNTIME_MODULES.iter().find(|(m, _)| *m == "fs").map(|(_, s)| *s).expect("fs runtime")
    }

    /// Every guarded fs fn of the real runtime is twinned, and the twin
    /// reaches the filesystem only through the facade.
    #[test]
    fn the_twin_covers_every_guarded_fn_and_reaches_fs_only_through_the_facade() {
        let entries: BTreeSet<String> =
            super::super::build::fs_runtime_guards().into_iter().filter(|(_, g)| *g != "__almide_fs_root").map(|(n, _)| n).collect();
        assert!(entries.len() > 30, "{entries:?}");
        let (twin, twinned) = scoped_twin(fs_src(), &entries);
        assert_eq!(twinned, entries, "a guarded fn is missing from the twin");
        assert_eq!(twin_leaks(&twin), Vec::<String>::new(), "{twin}");
        assert!(twin.contains("almide_sfs::read_to_string"));
        assert!(twin.contains(".__almide_s_is_dir()"));
    }

    #[test]
    fn fn_items_are_cut_at_their_own_closing_brace() {
        let src = "fn a() { let s = \"}\"; let c = '}'; let r = r#\"}\"#; /* } */ }\n// x\npub fn b<'a>(x: &'a str) -> &'a str {\n    x\n}\nstruct S;\n";
        let items = top_level_fns(src);
        assert_eq!(items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert!(items[0].text.ends_with("*/ }"));
        assert!(items[1].text.ends_with("}"));
    }
}
