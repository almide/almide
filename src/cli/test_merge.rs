//! One native test crate per run (#4441).
//!
//! `almide test --target native` lowers each test file to its own Rust crate,
//! and each crate inlines every module the file reaches — the runtime, the
//! self-hosted stdlib and the project's own modules. 104 test files meant 104
//! rustc runs over largely the SAME code (teastia: 35 min on CI, against one
//! `almide build` of 196 s).
//!
//! This module folds the per-file crates of one run into ONE crate, without
//! changing a byte of what any file's tests execute:
//!
//! - every file's generated source is split into its top-level items;
//! - an item that two or more files emit with the IDENTICAL text — and whose
//!   every reference to another top-level item is itself shared — is emitted
//!   ONCE at the crate root;
//! - everything else (the file's own fns, its `mod tests`, an item a file
//!   emits differently from another) goes into that file's own module,
//!   `almide_test_file_<i>`, which glob-imports the root (`use super::*`).
//!
//! An item shared at the root is the very text each of its files would have
//! compiled, and it can name only items that are shared too, so it resolves
//! to the same definitions it resolved to in every file's own crate. A file's
//! own items shadow the glob, so they resolve as before as well. The only
//! rewrite is alpha-renaming of compiler temporaries inside one item
//! (`__almide_ir2_src_arg_28518` → `__almide_ir2_src_arg_l0`): the compiler
//! numbers them program-wide, so the same function compiled for two files
//! differs only in those numbers.
//!
//! The runner then runs that one binary once per file, selecting the file's
//! module, so each file still runs in its own process exactly as before.
//! When the merged crate does not build, the caller falls back to the
//! per-file crates: the merge can cost a rustc run, never a verdict.

use std::collections::{HashMap, HashSet};

/// The module a test file's items live in inside the merged crate.
pub(crate) fn file_module(index: usize) -> String {
    format!("almide_test_file_{index}")
}

/// The merged crate's source.
pub(crate) fn merge(sources: &[&str]) -> String {
    let files: Vec<Vec<Item>> = sources.iter().map(|s| split_items(s)).collect();
    let shared = shared_keys(&files);

    let mut out = String::new();
    // Inner attributes (`#![allow(..)]`) belong at the crate root, once each.
    let mut seen: HashSet<&str> = HashSet::new();
    for item in files.iter().flatten().filter(|i| i.inner_attr) {
        if seen.insert(&item.text) {
            out.push_str(&item.text);
            out.push('\n');
        }
    }
    // Shared items, once each, in first-appearance order: a `macro_rules!`
    // precedes its first user in the file that first emitted that user.
    // A key can repeat inside one file (`#[cfg(unix)] mod c` and its
    // `#[cfg(not(unix))]` twin): each occurrence is placed once.
    let mut placed: HashSet<(&str, usize)> = HashSet::new();
    for items in &files {
        let mut occurrence: HashMap<&str, usize> = HashMap::new();
        for item in items.iter().filter(|i| !i.inner_attr && shared.contains(&i.key)) {
            let n = occurrence.entry(&item.key).or_insert(0);
            if placed.insert((&item.key, *n)) {
                out.push_str(&item.text);
                out.push('\n');
            }
            *n += 1;
        }
    }
    for (index, items) in files.iter().enumerate() {
        let module = file_module(index);
        out.push_str(&format!("\n#[allow(unused_imports)]\nmod {module} {{\nuse super::*;\n"));
        for item in items.iter().filter(|i| !i.inner_attr && !shared.contains(&i.key)) {
            out.push_str(&rewrite_crate_paths(&item.raw, &module));
            out.push('\n');
        }
        out.push_str("}\n");
    }
    out
}

/// The keys emitted once at the root: identical text in every file that
/// emits them (two or more), and every top-level name they reference shared.
fn shared_keys(files: &[Vec<Item>]) -> HashSet<String> {
    // key -> (holders, the texts each holder emits under that key)
    let mut defs: HashMap<&str, Vec<(usize, Vec<&str>)>> = HashMap::new();
    for (fi, items) in files.iter().enumerate() {
        for item in items.iter().filter(|i| !i.inner_attr) {
            let entry = defs.entry(&item.key).or_default();
            match entry.last_mut() {
                Some((f, texts)) if *f == fi => texts.push(&item.text),
                _ => entry.push((fi, vec![&item.text])),
            }
        }
    }
    let top_level: Vec<HashSet<&str>> = files
        .iter()
        .map(|items| items.iter().flat_map(|i| i.names.iter().map(String::as_str)).collect())
        .collect();
    let mut keys_of_name: HashMap<&str, HashSet<&str>> = HashMap::new();
    for item in files.iter().flatten() {
        for n in &item.names {
            keys_of_name.entry(n).or_default().insert(&item.key);
        }
    }
    let mut shared: HashSet<&str> = defs
        .iter()
        .filter(|(_, holders)| holders.len() >= 2 && holders.iter().all(|(_, t)| *t == holders[0].1))
        .map(|(k, _)| *k)
        .collect();
    // A root item named like a prelude name would shadow it in every file's
    // module, the ones that never defined it included.
    let first_item: HashMap<&str, &Item> = files.iter().flatten().rev().map(|i| (i.key.as_str(), i)).collect();
    shared.retain(|k| !first_item[k].names.iter().any(|n| PRELUDE_LIKE.contains(&n.as_str())));
    // An identifier is a reference to a top-level item when every file that
    // emits the item defines that name at top level (in a file that does
    // not, the same text cannot be naming an item); a prelude-like name a
    // file shadows counts whenever any holder defines it.
    let deps: HashMap<&str, Vec<&str>> = defs
        .iter()
        .map(|(k, holders)| {
            let item = first_item[k];
            let refs = item
                .idents
                .iter()
                .map(String::as_str)
                .filter(|id| {
                    let mut has = holders.iter().map(|(f, _)| top_level[*f].contains(id));
                    if PRELUDE_LIKE.contains(id) { has.any(|b| b) } else { has.all(|b| b) }
                })
                .collect();
            (*k, refs)
        })
        .collect();
    // An impl that stays per-file in two or more files would be a second impl
    // of one trait (or inherent method) for a root type: its self type stays
    // per-file too.
    let mut unshareable_names: HashSet<&str> = HashSet::new();
    loop {
        let mut changed = false;
        for (k, holders) in &defs {
            if holders.len() >= 2 && !shared.contains(k) {
                for n in &first_item[k].impl_self {
                    changed |= unshareable_names.insert(n);
                }
            }
        }
        let dropped: Vec<&str> = shared
            .iter()
            .copied()
            .filter(|k| {
                let item = first_item[k];
                item.names.iter().any(|n| unshareable_names.contains(n.as_str()))
                    || deps[k].iter().any(|d| {
                        keys_of_name.get(d).is_some_and(|ks| ks.iter().any(|dk| dk != k && !shared.contains(dk)))
                    })
            })
            .collect();
        for k in &dropped {
            shared.remove(k);
        }
        if dropped.is_empty() && !changed {
            break;
        }
    }
    shared.into_iter().map(String::from).collect()
}

/// Names Rust resolves through the prelude or the extern prelude: a file that
/// defines one at top level shadows it for every item in the file.
const PRELUDE_LIKE: &[&str] = &[
    "std", "core", "alloc", "Option", "Some", "None", "Result", "Ok", "Err", "Vec", "String", "Box", "ToString",
    "ToOwned", "Clone", "Copy", "Send", "Sync", "Sized", "Unpin", "Drop", "Fn", "FnMut", "FnOnce", "drop", "Default",
    "Eq", "PartialEq", "Ord", "PartialOrd", "Iterator", "IntoIterator", "Extend", "DoubleEndedIterator",
    "ExactSizeIterator", "AsRef", "AsMut", "Into", "From", "TryFrom", "TryInto", "FromIterator",
];

/// In a file's own module, `crate::x` must still name what it named in the
/// file's own crate: the file's module (which sees the root through its glob).
/// `$crate` (a macro's crate root) and `pub(crate)` are left alone.
fn rewrite_crate_paths(text: &str, module: &str) -> String {
    let toks = lex(text);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (i, t) in toks.iter().enumerate() {
        if t.kind != Tok::Ident || &text[t.start..t.end] != "crate" {
            continue;
        }
        let after_dollar = i > 0 && toks[i - 1].kind == Tok::Punct && &text[toks[i - 1].start..toks[i - 1].end] == "$";
        let path = toks.get(i + 1).zip(toks.get(i + 2)).is_some_and(|(a, b)| {
            &text[a.start..a.end] == ":" && &text[b.start..b.end] == ":" && a.end == b.start
        });
        if !after_dollar && path {
            out.push_str(&text[last..t.end]);
            out.push_str("::");
            out.push_str(module);
            last = t.end;
        }
    }
    out.push_str(&text[last..]);
    out
}

// ── Splitting a generated source into top-level items ─────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tok {
    Ident,
    Punct,
    Literal,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: Tok,
    start: usize,
    end: usize,
}

#[derive(Debug)]
struct Item {
    /// What identifies the item across files: `name:<n>` for a named item,
    /// `impl:<header>` for an impl, the text itself otherwise.
    key: String,
    /// The text emitted when the item is shared (temporaries renumbered).
    text: String,
    /// The text as the file emitted it.
    raw: String,
    /// Top-level names the item defines.
    names: Vec<String>,
    /// Every identifier the item mentions, once each.
    idents: Vec<String>,
    /// The identifiers of an impl's self type.
    impl_self: Vec<String>,
    /// `#![..]`.
    inner_attr: bool,
}

/// A Rust lexer exact enough to find item boundaries and identifiers:
/// comments, (raw / byte) strings, char literals vs lifetimes, numbers.
fn lex(s: &str) -> Vec<Token> {
    let b = s.as_bytes();
    let n = b.len();
    let mut toks = Vec::new();
    let mut i = 0;
    let char_at = |i: usize| s[i..].chars().next().unwrap_or('\0');
    while i < n {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            i = s[i..].find('\n').map_or(n, |j| i + j);
            continue;
        }
        if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            let mut depth = 0;
            while i < n {
                if b[i] == b'/' && i + 1 < n && b[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && i + 1 < n && b[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            continue;
        }
        // Raw strings: r"..", r#".."#, br"..", cr"..".
        let raw_at = match c {
            b'r' => Some(i + 1),
            b'b' | b'c' if i + 1 < n && b[i + 1] == b'r' => Some(i + 2),
            _ => None,
        };
        if let Some(mut j) = raw_at {
            let hashes_start = j;
            while j < n && b[j] == b'#' {
                j += 1;
            }
            if j < n && b[j] == b'"' {
                // (`r#ident`, a raw identifier, has no quote and falls through.)
                let close: String = std::iter::once('"').chain(std::iter::repeat_n('#', j - hashes_start)).collect();
                let end = s[j + 1..].find(&close).map_or(n, |k| j + 1 + k + close.len());
                toks.push(Token { kind: Tok::Literal, start: i, end });
                i = end;
                continue;
            }
        }
        // Strings: "..", b"..", c"..".
        let str_at = match c {
            b'"' => Some(i),
            b'b' | b'c' if i + 1 < n && b[i + 1] == b'"' => Some(i + 1),
            _ => None,
        };
        if let Some(q) = str_at {
            let mut j = q + 1;
            while j < n && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            let end = (j + 1).min(n);
            toks.push(Token { kind: Tok::Literal, start: i, end });
            i = end;
            continue;
        }
        // Char literals ('a', '\n', b'x') and lifetimes ('a, 'static).
        let quote_at = match c {
            b'\'' => Some(i),
            b'b' if i + 1 < n && b[i + 1] == b'\'' => Some(i + 1),
            _ => None,
        };
        if let Some(q) = quote_at {
            let j = q + 1;
            if j < n && b[j] == b'\\' {
                let end = s[j + 2..].find('\'').map_or(n, |k| j + 2 + k + 1);
                toks.push(Token { kind: Tok::Literal, start: i, end });
                i = end;
                continue;
            }
            if j < n {
                let ch = char_at(j);
                let after = j + ch.len_utf8();
                if after < n && b[after] == b'\'' {
                    toks.push(Token { kind: Tok::Literal, start: i, end: after + 1 });
                    i = after + 1;
                    continue;
                }
            }
            // A lifetime: the quote and the identifier after it.
            let mut k = j;
            while k < n && is_ident_continue(char_at(k)) {
                k += char_at(k).len_utf8();
            }
            toks.push(Token { kind: Tok::Punct, start: i, end: k.max(j) });
            i = k.max(j);
            continue;
        }
        let ch = char_at(i);
        if is_ident_start(ch) {
            let start = i;
            if c == b'r' && i + 2 < n && b[i + 1] == b'#' && is_ident_start(char_at(i + 2)) {
                i += 2;
            }
            while i < n && is_ident_continue(char_at(i)) {
                i += char_at(i).len_utf8();
            }
            toks.push(Token { kind: Tok::Ident, start, end: i });
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < n {
                let d = b[i];
                if d.is_ascii_alphanumeric() || d == b'_' {
                    i += 1;
                } else if d == b'.' && i + 1 < n && b[i + 1].is_ascii_digit() {
                    i += 1;
                } else {
                    break;
                }
            }
            toks.push(Token { kind: Tok::Literal, start, end: i });
            continue;
        }
        let len = ch.len_utf8();
        toks.push(Token { kind: Tok::Punct, start: i, end: i + len });
        i += len;
    }
    toks
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// The item keyword, past attributes, visibility and qualifiers.
fn item_keyword<'a>(s: &'a str, toks: &[Token]) -> Option<(&'a str, usize)> {
    let text = |t: &Token| &s[t.start..t.end];
    let mut i = 0;
    while i < toks.len() {
        let w = text(&toks[i]);
        match w {
            "#" => {
                i += 1;
                if i < toks.len() && text(&toks[i]) == "!" {
                    i += 1;
                }
                let mut depth = 0;
                while i < toks.len() {
                    match text(&toks[i]) {
                        "[" => depth += 1,
                        "]" => {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            "pub" => {
                i += 1;
                if i < toks.len() && text(&toks[i]) == "(" {
                    while i < toks.len() && text(&toks[i]) != ")" {
                        i += 1;
                    }
                    i += 1;
                }
            }
            "unsafe" | "async" | "default" => i += 1,
            "extern" => {
                i += 1;
                if i < toks.len() && toks[i].kind == Tok::Literal {
                    i += 1;
                }
                if i < toks.len() && text(&toks[i]) == "{" {
                    return Some(("extern_block", i));
                }
            }
            "const" if i + 1 < toks.len() && matches!(text(&toks[i + 1]), "fn" | "unsafe" | "async" | "extern") => i += 1,
            _ => return Some((w, i)),
        }
    }
    None
}

/// Split a generated source into its top-level items.
fn split_items(s: &str) -> Vec<Item> {
    let toks = lex(s);
    let mut ranges = Vec::new();
    let mut begin = 0;
    let mut depth = 0i32;
    for (i, t) in toks.iter().enumerate() {
        let w = &s[t.start..t.end];
        if t.kind != Tok::Punct {
            continue;
        }
        let mut end_here = false;
        match w {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => {
                depth -= 1;
                if depth == 0 {
                    let cur = &toks[begin..=i];
                    let inner_attr = cur.len() >= 2 && &s[cur[0].start..cur[0].end] == "#" && &s[cur[1].start..cur[1].end] == "!";
                    if w == "]" {
                        end_here = inner_attr;
                    } else if w == "}" {
                        let kw = item_keyword(s, cur).map(|(k, _)| k);
                        end_here = !matches!(kw, Some("const" | "static" | "type" | "use" | "let"));
                    }
                }
            }
            ";" if depth == 0 => end_here = true,
            _ => {}
        }
        if end_here {
            ranges.push(begin..i + 1);
            begin = i + 1;
        }
    }
    if begin < toks.len() {
        ranges.push(begin..toks.len());
    }
    // The file's top-level names first: a name the file defines at top level
    // is never renumbered as a temporary.
    let none = HashSet::new();
    let top: HashSet<String> = ranges.iter().flat_map(|r| describe(s, &toks[r.clone()], &none).names).collect();
    let top: HashSet<&str> = top.iter().map(String::as_str).collect();
    ranges.into_iter().map(|r| describe(s, &toks[r], &top)).collect()
}

fn describe(s: &str, toks: &[Token], top_level: &HashSet<&str>) -> Item {
    let text = |t: &Token| &s[t.start..t.end];
    let raw = s[toks[0].start..toks[toks.len() - 1].end].to_string();
    let inner_attr = toks.len() >= 2 && text(&toks[0]) == "#" && text(&toks[1]) == "!";
    let mut idents: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    for t in toks.iter().filter(|t| t.kind == Tok::Ident) {
        let w = text(t);
        if seen.insert(w) {
            idents.push(w.to_string());
        }
    }
    let shared_text = renumber_temporaries(s, toks, top_level);
    let ident_after = |i: usize| toks.get(i).filter(|t| t.kind == Tok::Ident).map(|t| text(t).to_string());
    let mut names = Vec::new();
    let mut impl_self = Vec::new();
    let key;
    match item_keyword(s, toks) {
        Some(("fn" | "struct" | "enum" | "union" | "trait" | "type" | "const" | "static" | "mod", k)) => {
            let k = if toks.get(k + 1).is_some_and(|t| text(t) == "mut") { k + 1 } else { k };
            names.extend(ident_after(k + 1).filter(|n| n != "_"));
            key = match names.first() {
                Some(n) => format!("name:{n}"),
                None => shared_text.clone(),
            };
        }
        Some(("macro_rules", k)) => {
            names.extend(ident_after(k + 2));
            key = match names.first() {
                Some(n) => format!("macro:{n}"),
                None => shared_text.clone(),
            };
        }
        Some(("impl", k)) => {
            let body = toks[k..].iter().position(|t| text(t) == "{").map_or(toks.len(), |p| k + p);
            let header: Vec<&str> = toks[k..body].iter().map(text).collect();
            let self_from = header.iter().position(|w| *w == "for").map(|p| p + 1).unwrap_or_else(|| {
                // `impl<..> Type`: past the generics.
                if header.get(1) == Some(&"<") {
                    let mut depth = 0;
                    for (j, w) in header.iter().enumerate().skip(1) {
                        match *w {
                            "<" => depth += 1,
                            ">" => {
                                depth -= 1;
                                if depth == 0 {
                                    return j + 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                1
            });
            impl_self.extend(toks[k + self_from.min(body - k)..body].iter().filter(|t| t.kind == Tok::Ident).map(|t| text(t).to_string()));
            key = format!("impl:{}", header.join(" "));
        }
        Some(("use", _)) => {
            names.extend(use_names(s, toks));
            key = format!("use:{shared_text}");
        }
        Some(("extern_block", _)) => {
            for (i, t) in toks.iter().enumerate() {
                if matches!(text(t), "fn" | "static") {
                    names.extend(ident_after(i + 1));
                }
            }
            key = shared_text.clone();
        }
        _ => {
            // An item macro (`thread_local! { static X .. }`, a derive-like
            // generator): what it defines is what follows a defining keyword.
            for (i, t) in toks.iter().enumerate() {
                if matches!(text(t), "static" | "fn" | "struct" | "enum" | "const" | "type" | "mod" | "trait") {
                    names.extend(ident_after(i + 1).filter(|n| n != "_"));
                }
            }
            key = shared_text.clone();
        }
    }
    Item { key, text: shared_text, raw, names, idents, impl_self, inner_attr }
}

/// The names a `use` item binds: each path's last segment or its alias
/// (`a::{self, B as C}` binds `a` and `C`).
fn use_names(s: &str, toks: &[Token]) -> Vec<String> {
    let text = |i: usize| toks.get(i).map_or("", |t| &s[t.start..t.end]);
    let mut names = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for i in 0..toks.len() {
        match text(i) {
            "{" => open.push(i),
            "}" => {
                open.pop();
            }
            _ => {}
        }
        if toks[i].kind != Tok::Ident || matches!(text(i), "use" | "pub" | "crate" | "super" | "as") {
            continue;
        }
        if !matches!(text(i + 1), "," | "}" | ";") {
            continue;
        }
        if text(i) == "self" {
            // `a::{self}` binds `a`: the segment before the enclosing `::{`.
            if let Some(&b) = open.last() {
                if b >= 3 && toks[b - 3].kind == Tok::Ident {
                    names.push(text(b - 3).to_string());
                }
            }
            continue;
        }
        names.push(text(i).to_string());
    }
    names
}

/// Renumber the compiler temporaries inside one item, `__<stem>_<digits>`, in
/// order of first appearance — they are program-wide counters, so the same
/// function compiled for two files differs only there. A temporary is local
/// to the item it appears in; a name the file defines at top level (a
/// synthesized `__almd_scoped_1`, say) is left alone.
fn renumber_temporaries(s: &str, toks: &[Token], top_level: &HashSet<&str>) -> String {
    let text = |i: usize| toks.get(i).map_or("", |t| &s[t.start..t.end]);
    // A name something outside this item could reach — an item the item
    // defines inside itself, a path segment, a field — keeps its spelling.
    let mut pinned: HashSet<&str> = HashSet::new();
    for (i, t) in toks.iter().enumerate().filter(|(_, t)| t.kind == Tok::Ident) {
        let before = if i > 0 { text(i - 1) } else { "" };
        let reachable = matches!(before, "fn" | "struct" | "enum" | "union" | "trait" | "type" | "const" | "static" | "mod" | "." | ":")
            || text(i + 1) == ":" && text(i + 2) == ":";
        if reachable && !(before == ":" && i > 1 && text(i - 2) != ":") {
            pinned.insert(&s[t.start..t.end]);
        }
    }
    let mut out = String::with_capacity(toks[toks.len() - 1].end - toks[0].start);
    let mut map: HashMap<&str, String> = HashMap::new();
    let mut last = toks[0].start;
    for t in toks.iter().filter(|t| t.kind == Tok::Ident) {
        let w = &s[t.start..t.end];
        let Some(stem) = temporary_stem(w).filter(|_| !top_level.contains(w) && !pinned.contains(w)) else { continue };
        let next = map.len();
        let renamed = map.entry(w).or_insert_with(|| format!("{stem}l{next}"));
        out.push_str(&s[last..t.start]);
        out.push_str(renamed);
        last = t.end;
    }
    out.push_str(&s[last..toks[toks.len() - 1].end]);
    out
}

/// `__almide_ir2_src_arg_28518` → `Some("__almide_ir2_src_arg_")`.
fn temporary_stem(w: &str) -> Option<&str> {
    if !w.starts_with("__") {
        return None;
    }
    let digits = w.bytes().rev().take_while(u8::is_ascii_digit).count();
    let stem = &w[..w.len() - digits];
    (digits > 0 && stem.ends_with('_')).then_some(stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(src: &str) -> Vec<String> {
        split_items(src).into_iter().map(|i| i.key).collect()
    }

    #[test]
    fn items_split_at_their_real_ends() {
        let src = r##"#![allow(dead_code)]
use std::fmt;
/// doc
#[derive(Clone)]
pub struct A { x: i64 }
struct B(i64);
static S: [u8; 2] = { [1, 2] };
const C: &str = "}{;";
fn f<'a>(x: &'a str) -> char { let c = '}'; let _ = r#"};"#; c }
impl fmt::Display for A { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", self.x) } }
macro_rules! m { ($a:expr) => { $a }; }
thread_local! { static T: u8 = 0; }
extern "C" { fn ext(); }
mod tests { use super::*; }
fn main() {}
"##;
        assert_eq!(
            keys(src),
            vec![
                "#![allow(dead_code)]".to_string(),
                "use:use std::fmt;".into(),
                "name:A".into(),
                "name:B".into(),
                "name:S".into(),
                "name:C".into(),
                "name:f".into(),
                "impl:impl fmt : : Display for A".into(),
                "macro:m".into(),
                "thread_local! { static T: u8 = 0; }".into(),
                "extern \"C\" { fn ext(); }".into(),
                "name:tests".into(),
                "name:main".into(),
            ]
        );
        let items = split_items(src);
        assert_eq!(items[7].impl_self, vec!["A".to_string()]);
        assert_eq!(items[9].names, vec!["T".to_string()]);
        assert_eq!(items[10].names, vec!["ext".to_string()]);
    }

    #[test]
    fn temporaries_renumber_per_item() {
        let a = split_items("fn f() { let __x_arg_17 = 1; let __x_arg_9 = __x_arg_17; }");
        let b = split_items("fn f() { let __x_arg_4 = 1; let __x_arg_5 = __x_arg_4; }");
        assert_eq!(a[0].text, b[0].text);
        assert_eq!(a[0].text, "fn f() { let __x_arg_l0 = 1; let __x_arg_l1 = __x_arg_l0; }");
    }

    #[test]
    fn identical_items_go_to_the_root_once_and_own_items_to_the_file_module() {
        let one = "fn shared() -> i64 { 1 }\nfn own() -> i64 { shared() }\nmod tests { use super::*; #[test] fn t() { own(); } }\nfn main() {}\n";
        let two = "fn shared() -> i64 { 1 }\nfn own() -> i64 { 2 }\nmod tests { use super::*; #[test] fn t() { own(); } }\nfn main() {}\n";
        let merged = merge(&[one, two]);
        assert_eq!(merged.matches("fn shared()").count(), 1, "{merged}");
        assert_eq!(merged.matches("fn main()").count(), 1, "{merged}");
        assert_eq!(merged.matches("fn own()").count(), 2, "{merged}");
        assert!(merged.find("fn shared()").unwrap() < merged.find("mod almide_test_file_0").unwrap());
    }

    #[test]
    fn an_item_naming_a_per_file_item_stays_per_file() {
        // `uses` is identical in both files but calls `differs`, which is not.
        let one = "fn differs() -> i64 { 1 }\nfn uses() -> i64 { differs() }\n";
        let two = "fn differs() -> i64 { 2 }\nfn uses() -> i64 { differs() }\n";
        let merged = merge(&[one, two]);
        assert_eq!(merged.matches("fn uses()").count(), 2, "{merged}");
    }

    #[test]
    fn a_name_only_one_file_defines_does_not_pin_a_shared_item() {
        // `x` is a local in `g` everywhere; one file also has a top-level `x`.
        let one = "fn g() -> i64 { let x = 1; x }\nfn x() {}\n";
        let two = "fn g() -> i64 { let x = 1; x }\n";
        let merged = merge(&[one, two]);
        assert_eq!(merged.matches("fn g()").count(), 1, "{merged}");
    }

    #[test]
    fn a_per_file_impl_pins_its_self_type_per_file() {
        let one = "struct T;\nimpl T { fn m(&self) -> i64 { 1 } }\n";
        let two = "struct T;\nimpl T { fn m(&self) -> i64 { 2 } }\n";
        let merged = merge(&[one, two]);
        assert_eq!(merged.matches("struct T;").count(), 2, "{merged}");
    }

    #[test]
    fn crate_paths_in_a_file_module_name_the_file_module() {
        let out = rewrite_crate_paths("fn f() { crate::g(); $crate::h(); } pub(crate) fn k() {}", "almide_test_file_3");
        assert_eq!(out, "fn f() { crate::almide_test_file_3::g(); $crate::h(); } pub(crate) fn k() {}");
    }
}
