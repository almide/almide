/// One top-level item: (kind, name, whole text, whether `#[cfg…]`-attributed).
type TopItem<'a> = (ItemKind, &'a str, &'a str, bool);

// ── the runtime index of native_floor.rs, include!-spliced back at module level ──
//
// A pure code move: this file continues its parent verbatim. The split exists
// only so the parent stays under the 800-line ceiling the codopsy gate holds
// this crate to; there is no boundary of meaning here, and `include!` at module
// level is the one splice Rust allows (an impl-item position rejects it).

// ── The runtime index: every top-level item of the embedded runtime ──

struct RtItem {
    text: &'static str,
}

struct RuntimeIndex {
    /// Addresses of assembled fs items, retained across generated loading.
    fs_built: BTreeSet<usize>,
    /// Items defined exactly once at the top level of the runtime modules.
    items: HashMap<&'static str, RtItem>,
    /// Every name the runtime and its prelude define (a type, a macro, a
    /// module, a twice-defined fn, …): reaching one that is not in `items` walls.
    defined: BTreeSet<&'static str>,
    /// The methods the runtime's and the prelude's own traits declare
    /// (`IntoAlmide::into_almide`): a body calling one needs the trait and
    /// its impls, which the header does not carry.
    trait_methods: BTreeSet<&'static str>,
    /// Every fn name the runtime defines at its top level (taken or not).
    fns: BTreeSet<&'static str>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Fn,
    Const,
    /// A `struct` / `enum` / `type` / `union`: its text includes its
    /// attributes (a `#[derive]` travels with it).
    Type,
    /// An `impl` block, named by its self type.
    Impl,
    Other,
}

/// The top-level items of a runtime module: (kind, name, whole text, whether
/// a `#[cfg…]` attribute precedes it).
fn top_level_items(src: &str) -> Vec<TopItem<'_>> {
    scan_items(src, false)
}

/// [`top_level_items`], every item's text with its attributes when
/// `with_attrs` (a `#[cfg]` const or module keeps its gate).
fn scan_items(src: &str, with_attrs: bool) -> Vec<TopItem<'_>> {
    let mut scan = ItemScan { src, toks: lex(src), out: Vec::new(), i: 0, cfg_attr: false, attr_start: None, with_attrs };
    while scan.i < scan.toks.len() {
        if !scan.step() {
            break;
        }
    }
    scan.out
}

/// The scan state of [`top_level_items`]: the token at `i`, and the
/// attributes met since the last item.
struct ItemScan<'a> {
    src: &'a str,
    toks: Vec<Tok<'a>>,
    out: Vec<TopItem<'a>>,
    i: usize,
    cfg_attr: bool,
    attr_start: Option<usize>,
    with_attrs: bool,
}

impl<'a> ItemScan<'a> {
    /// One step of the scan at token `i`; `false` ends it.
    fn step(&mut self) -> bool {
        let (depth, text, start_off) = {
            let t = &self.toks[self.i];
            (t.depth, t.text, t.off)
        };
        if depth != 0 {
            self.i += 1;
            return true;
        }
        if text == "#" {
            self.attribute();
            return true;
        }
        // Visibility and qualifiers.
        let j = self.skip_qualifiers(self.i);
        let Some(kw) = self.toks.get(j).map(|t| t.text) else { return false };
        let (kind, name_at) = match kw {
            "fn" => (ItemKind::Fn, j + 1),
            "const" | "static" => (ItemKind::Const, j + 1),
            "struct" | "enum" | "type" | "union" => (ItemKind::Type, j + 1),
            "trait" | "mod" | "macro_rules" => (ItemKind::Other, j + 1),
            "thread_local" if self.toks.get(j + 1).is_some_and(|t| t.text == "!")
                && self.toks.get(j + 2).is_some_and(|t| t.text == "{") => {
                self.thread_local_item(j, start_off);
                return true;
            }
            "impl" => {
                self.impl_item(j, start_off);
                return true;
            }
            "use" => {
                // No name of its own: skip to its end.
                let end = item_end(&self.toks, j);
                self.finish(end);
                return true;
            }
            _ => {
                self.i += 1;
                return true;
            }
        };
        let name_at = if kw == "macro_rules" { name_at + 1 } else { name_at };
        self.named_item(kind, name_at, start_off)
    }

    /// An attribute: `#[...]` (skip to its closing bracket).
    fn attribute(&mut self) {
        let toks = &self.toks;
        let start = self.i;
        let mut j = start + 1;
        while j < toks.len() && !(toks[j].text == "]" && toks[j].depth == 0) {
            j += 1;
        }
        self.cfg_attr |= toks[start..j].iter().any(|t| t.text == "cfg" || t.text == "cfg_attr");
        self.attr_start.get_or_insert(toks[start].off);
        self.i = j + 1;
    }

    /// The token after the visibility and qualifiers starting at `i`.
    fn skip_qualifiers(&self, i: usize) -> usize {
        let toks = &self.toks;
        let mut j = i;
        if toks[j].text == "pub" {
            j += 1;
            if toks.get(j).is_some_and(|t| t.text == "(") {
                while j < toks.len() && toks[j].text != ")" {
                    j += 1;
                }
                j += 1;
            }
        }
        while toks.get(j).is_some_and(|t| matches!(t.text, "const" | "unsafe" | "extern" | "async") && toks.get(j + 1).is_some_and(|n| matches!(n.text, "fn" | "unsafe" | "extern" | "\"C\""))) {
            j += 1;
        }
        j
    }

    /// The source offset just past token `end`.
    fn end_off(&self, end: usize) -> usize {
        self.toks.get(end).map(|t| t.off + t.text.len()).unwrap_or(self.src.len())
    }

    /// Past an item ending at token `end`: the next item has no attributes yet.
    fn finish(&mut self, end: usize) {
        self.i = end + 1;
        self.cfg_attr = false;
        self.attr_start = None;
    }

    /// A std thread-local declaration travels as one macro, selected by any
    /// static it defines. Conditional declarations remain unavailable; copying
    /// only an inner static would lose both its TLS semantics and its attributes.
    fn thread_local_item(&mut self, j: usize, start_off: usize) {
        let end = item_end(&self.toks, j);
        let end_off = self.end_off(end);
        let inner = &self.src[self.toks[j + 2].off + 1..self.toks[end].off];
        let items = top_level_items(inner);
        let conditional = self.cfg_attr || items.iter().any(|(_, _, _, cfg)| *cfg);
        let valid = !items.is_empty() && items.iter().all(|(kind, _, text, _)|
            *kind == ItemKind::Const && lex(text).iter().take(3).any(|t| t.text == "static"));
        let from = self.attr_start.unwrap_or(start_off);
        let whole = &self.src[from..end_off];
        for (_, name, _, _) in items {
            self.out.push((ItemKind::Const, name, whole, conditional || !valid));
        }
        self.finish(end);
    }

    /// An `impl` block whose keyword is token `j`.
    fn impl_item(&mut self, j: usize, start_off: usize) {
        let end = item_end(&self.toks, j);
        let end_off = self.end_off(end);
        if let Some(name) = impl_self_type(&self.toks[j..=end.min(self.toks.len() - 1)]) {
            let src: &'a str = self.src;
            let from = if self.with_attrs { self.attr_start.unwrap_or(start_off) } else { start_off };
            self.out.push((ItemKind::Impl, name, &src[from..end_off], self.cfg_attr));
        }
        self.finish(end);
    }

    /// A named item of `kind` whose name is token `name_at`; `false` ends the scan.
    fn named_item(&mut self, kind: ItemKind, name_at: usize, start_off: usize) -> bool {
        let Some(name) = self.toks.get(name_at).map(|t| t.text) else { return false };
        // A `const`/`static` ends at its `;`: its initializer may hold a block
        // (`X: T = T { … };`).
        let end = if kind == ItemKind::Const { const_end(&self.toks, name_at) } else { item_end(&self.toks, name_at) };
        let end_off = self.end_off(end);
        // A type keeps its derives; a fn its `#[inline]` (the runtime crate
        // is a crate of its own: a non-generic fn crosses into the program
        // only where the runtime marks it inline, as the standard codegen's
        // one-crate build inlines it).
        let from = if self.with_attrs || matches!(kind, ItemKind::Type | ItemKind::Fn) { self.attr_start.unwrap_or(start_off) } else { start_off };
        let src: &'a str = self.src;
        self.out.push((kind, name, &src[from..end_off], self.cfg_attr));
        self.finish(end);
        true
    }
}

/// The self type of an `impl` block (`impl<T> Trait for Name<T> {` → `Name`),
/// and its trait, if any, as the first token of `impl … for`.
fn impl_self_type<'a>(toks: &[Tok<'a>]) -> Option<&'a str> {
    impl_parts(toks).map(|(_, ty)| ty)
}

fn impl_parts<'a>(toks: &[Tok<'a>]) -> Option<(Option<&'a str>, &'a str)> {
    let base = toks.first()?.depth;
    let mut k = 1;
    // Skip the impl's own generics.
    if toks.get(k).is_some_and(|t| t.text == "<") {
        let mut n = 0i32;
        while let Some(t) = toks.get(k) {
            match t.text {
                "<" => n += 1,
                ">" => n -= 1,
                _ => {}
            }
            k += 1;
            if n == 0 {
                break;
            }
        }
    }
    let head = &toks[k..];
    let open = head.iter().position(|t| t.text == "{" && t.depth == base)?;
    let first_ident = |ts: &[Tok<'a>]| ts.iter().map(|t| t.text).find(|s| is_ident(s) && !matches!(*s, "dyn" | "mut"));
    match head[..open].iter().position(|t| t.text == "for") {
        Some(f) => Some((first_ident(&head[..f]), first_ident(&head[f + 1..open])?)),
        None => Some((None, first_ident(&head[..open])?)),
    }
}

/// The `;` ending a `const`/`static` item, outside any nesting.
fn const_end(toks: &[Tok], from: usize) -> usize {
    let base = toks[from].depth;
    let mut nest = 0i32;
    for (j, t) in toks.iter().enumerate().skip(from) {
        match t.text {
            "(" | "[" => nest += 1,
            ")" | "]" => nest -= 1,
            ";" if t.depth == base && nest == 0 => return j,
            _ => {}
        }
    }
    toks.len().saturating_sub(1)
}

/// The index of the token that ends the item starting at `from`: the `}`
/// closing its first block, or a `;` met before any block, both outside any
/// `()` / `[]` / `<>` nesting (`[u8; 4]` is not an end).
fn item_end(toks: &[Tok], from: usize) -> usize {
    let base = toks[from].depth;
    let mut nest = 0i32;
    let mut j = from;
    while j < toks.len() {
        let t = &toks[j];
        match t.text {
            "(" | "[" => nest += 1,
            ")" | "]" => nest -= 1,
            _ => {}
        }
        if t.depth == base && nest == 0 && t.text == ";" {
            return j;
        }
        if t.text == "{" && t.depth == base && nest == 0 {
            let mut k = j + 1;
            while k < toks.len() && !(toks[k].text == "}" && toks[k].depth == base) {
                k += 1;
            }
            return k;
        }
        j += 1;
    }
    toks.len().saturating_sub(1)
}

/// The Rust param types and return type of a runtime fn item.
fn rust_signature(text: &str) -> Option<(Vec<String>, String)> {
    let toks = lex(text);
    let fn_at = toks.iter().position(|t| t.text == "fn")?;
    let open = toks[fn_at..].iter().position(|t| t.text == "(")? + fn_at;
    // The `)` matching the param list's `(` (a param type may hold parens:
    // `Vec<(K, V)>`).
    let close = {
        let mut n = 0i32;
        (open..toks.len()).find(|&k| {
            match toks[k].text {
                "(" => n += 1,
                ")" => n -= 1,
                _ => {}
            }
            n == 0
        })?
    };
    let inner = &text[toks[open].off + 1..toks[close].off];
    let mut params = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in inner.chars() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            _ => {}
        }
        if c == ',' && depth == 0 {
            params.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        params.push(cur);
    }
    let params = params
        .into_iter()
        .map(|p| p.split_once(':').map(|(_, t)| t.trim().to_string()))
        .collect::<Option<Vec<_>>>()?;
    let body_open = (close + 1..toks.len()).find(|&k| toks[k].text == "{" || toks[k].text == "where")?;
    let after = &text[toks[close].off + 1..toks[body_open].off];
    let ret = after.trim().strip_prefix("->").map(|r| r.trim().to_string()).unwrap_or_default();
    Some((params, ret))
}

fn references_in(rt: &RuntimeIndex, text: &str) -> Result<Vec<String>, String> {
    references_seeded(rt, text, &BTreeSet::new())
}

fn references_seeded<'t>(rt: &RuntimeIndex, text: &'t str, own: &BTreeSet<&'t str>) -> Result<Vec<String>, String> {
    let toks = lex(text);
    let header = header_items();
    let mut out = Vec::new();
    let mut locals: BTreeSet<&str> = own.clone();
    for (i, t) in toks.iter().enumerate() {
        let s = t.text;
        if !is_ident(s) {
            continue;
        }
        let at = IdentAt {
            s,
            prev: i.checked_sub(1).map(|k| toks[k].text),
            range: i.checked_sub(2).is_some_and(|k| toks[k].text == "." && toks[k + 1].text == "." && toks[k + 1].off == toks[k].off + 1),
            next: toks.get(i + 1).map(|t| t.text),
            after_next: toks.get(i + 2).map(|t| t.text),
        };
        // Inside the fs host module, `super::NAME` (from one of its modules)
        // and `&mut NAME` (a fn handed over by reference) name an item of it.
        let prev2 = i.checked_sub(2).map(|k| toks[k].text);
        let fs_item_ref = !own.is_empty() && matches!((at.prev, prev2), (Some("::"), Some("super")) | (Some("mut"), Some("&")));
        let step = match by_position(rt, &at) {
            Some(_) if fs_item_ref => by_name(rt, header, &locals, &at),
            Some(step) => step,
            None => by_name(rt, header, &locals, &at),
        };
        match step {
            RefStep::Fail => return Err(s.to_string()),
            RefStep::Take => out.push(s.to_string()),
            RefStep::Local => {
                locals.insert(s);
            }
            RefStep::Skip => {}
        }
    }
    Ok(out)
}

/// An identifier token of a runtime body and its neighbours' texts.
struct IdentAt<'t> {
    s: &'t str,
    prev: Option<&'t str>,
    /// The token before is the second `.` of a `..` range (`s[..f(x)]`), not
    /// a method or field access.
    range: bool,
    next: Option<&'t str>,
    after_next: Option<&'t str>,
}

/// What [`references_in`] does with one identifier.
enum RefStep {
    /// It reaches a name the native header cannot carry.
    Fail,
    /// It reaches a takeable runtime item.
    Take,
    /// It binds a name of the body.
    Local,
    /// Nothing to take.
    Skip,
}

/// The identifier's step read off its position alone (after a `.`, a
/// path, `fn`, or as a binding), if that decides it.
fn by_position(rt: &RuntimeIndex, at: &IdentAt) -> Option<RefStep> {
    let (s, next) = (at.s, at.next);
    // A range's `..` is no access: the name after it is read by its name.
    let prev = if at.range { None } else { at.prev };
    if prev == Some(".") && rt.trait_methods.contains(s) {
        return Some(RefStep::Fail); // a method of the runtime's own traits
    }
    if prev == Some(".") && rt.items.contains_key(s) && !rt.defined.contains(s) {
        return Some(RefStep::Take); // a method of a prelude trait that travels whole
    }
    if matches!(prev, Some("." | "::" | "'")) {
        return Some(RefStep::Skip); // a method, a field, a path tail, a lifetime
    }
    if prev == Some("fn") {
        return Some(RefStep::Skip); // an item's own name
    }
    // A closure param sits between bars (`|e|`, `|a, b|`); the name after
    // the closing bar is the closure's body.
    let closure_param = prev == Some("|") && matches!(next, Some("|" | "," | ":"));
    if matches!(prev, Some("let" | "mut" | "ref")) || closure_param || (next == Some(":") && prev != Some("::")) {
        return Some(RefStep::Local); // a binding, a param, a field or label name
    }
    None
}

/// The step of an identifier its position did not decide, read off what
/// the name is.
fn by_name(rt: &RuntimeIndex, header: &BTreeSet<&'static str>, locals: &BTreeSet<&str>, at: &IdentAt) -> RefStep {
    let (s, next) = (at.s, at.next);
    if header.contains(s) {
        return RefStep::Skip;
    }
    if rt.items.contains_key(s) {
        return RefStep::Take;
    }
    if locals.contains(s) {
        return RefStep::Skip;
    }
    // A lowercase name used as a value (no call, path or macro after it)
    // is a binding of this body: a pattern or closure param the token
    // scan did not see bind it. An item it could name would be a `const`
    // or `static`, which are UPPERCASE.
    // (`c != x` lexes as `c`, `!`, `=`: only `name!(` / `![` / `!{` is a macro.)
    let macro_call = next == Some("!") && at.after_next.is_some_and(|t| matches!(t, "(" | "[" | "{"));
    let used_as_item = matches!(next, Some("(" | "::")) || macro_call || s.chars().next().is_some_and(|c| c.is_uppercase());
    if rt.defined.contains(s) && used_as_item {
        return RefStep::Fail;
    }
    // A runtime fn that is not takeable (a `#[cfg]` twin) passed as a value.
    if rt.defined.contains(s) && !rt.items.contains_key(s) && !locals.contains(s) && rt.fns.contains(s) {
        return RefStep::Fail;
    }
    RefStep::Skip
}

/// The names the native header defines for a floor body.
/// `text` with every top-level item it opens made `pub` (a fn, const,
/// static, type, trait or module that is not already): the runtime part is a
/// module, or a crate of its own, that the program reaches through a glob.
pub(crate) fn pub_items(text: &str) -> String {
    let toks = lex(text);
    // (offset, length replaced, replacement)
    let mut edits: Vec<(usize, usize, &str)> = Vec::new();
    let mut i = 0;
    let mut expect_item = true;
    let mut nested = false;
    while i < toks.len() {
        let t = &toks[i];
        if t.depth != 0 {
            nested = true;
            i += 1;
            continue;
        }
        if nested {
            // Back at depth 0 after an item's braces: the next item.
            nested = false;
            expect_item = true;
        }
        if t.text == "#" {
            // An attribute: skip to its closing bracket.
            i = attr_close(&toks, i) + 1;
            continue;
        }
        if expect_item {
            expect_item = false;
            if let Some(edit) = pub_edit(&toks, i) {
                edits.push(edit);
            }
        }
        if t.text == ";" {
            expect_item = true;
        }
        i += 1;
    }
    let mut out = text.to_string();
    for (off, len, rep) in edits.into_iter().rev() {
        out.replace_range(off..off + len, rep);
    }
    out
}

/// The `]` closing the attribute whose `#` is token `i` (or the end).
fn attr_close(toks: &[Tok], i: usize) -> usize {
    let mut j = i + 1;
    let mut brackets = 0i32;
    while j < toks.len() {
        match toks[j].text {
            "[" => brackets += 1,
            "]" => {
                brackets -= 1;
                if brackets == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    j
}

/// The edit making the item that opens at token `i` `pub`, if it needs one.
fn pub_edit(toks: &[Tok], i: usize) -> Option<(usize, usize, &'static str)> {
    let t = &toks[i];
    if t.text == "pub" && toks.get(i + 1).is_some_and(|n| n.text == "(") {
        // `pub(crate)` / `pub(super)`: visible from the program too.
        return toks[i + 1..].iter().find(|n| n.text == ")").map(|close| (t.off, close.off + 1 - t.off, "pub"));
    }
    let opens = matches!(t.text, "fn" | "const" | "static" | "struct" | "enum" | "type" | "trait" | "mod" | "union" | "unsafe" | "use")
        && !(t.text == "unsafe" && toks.get(i + 1).is_some_and(|n| n.text == "impl"));
    opens.then_some((t.off, 0, "pub "))
}

#[cfg(test)]
mod thread_local_item_tests {
    use super::*;
    #[test]
    fn macro_statics_share_one_complete_declaration_and_keep_conditional_gates() {
        let src = "thread_local! { static A: Cell<i64> = Cell::new(0); static B: RefCell<Vec<i64>> = RefCell::new(Vec::new()); } fn after() {}";
        let items = top_level_items(src);
        assert_eq!(items.iter().map(|(_, n, _, c)| (*n, *c)).collect::<Vec<_>>(), [("A", false), ("B", false), ("after", false)]);
        assert_eq!(items[0].2, items[1].2);
        assert!(items[0].2.starts_with("thread_local!"));
        for gated in [
            "#[cfg(unix)] thread_local! { static A: Cell<i64> = Cell::new(0); }",
            "thread_local! { #[cfg(unix)] static A: Cell<i64> = Cell::new(0); static B: i64 = 0; }",
        ] { assert!(top_level_items(gated).iter().all(|(_, _, _, cfg)| *cfg)); }
        assert_eq!(top_level_items("other_macro! { static A: i64 = 0; }").len(), 0);
    }
}
