use std::collections::BTreeMap;
// Shared by build.rs and the generated-index verification test.
/// What the first scan over the runtime modules' top-level items collects.
#[derive(Default)]
struct RuntimeScan {
    items: HashMap<&'static str, RtItem>,
    seen: BTreeMap<&'static str, usize>,
    /// A module the runtime glob-imports (`mod almide_libm { … } pub use
    /// almide_libm::*;`) puts its names in scope unqualified: each of them
    /// takes the whole module and its glob along.
    glob_mods: Vec<(&'static str, &'static str)>,
    /// Runtime types and their impl blocks: a type travels with its
    /// inherent impls and its impls of std traits.
    types: Vec<(&'static str, &'static str)>,
    impls: Vec<(&'static str, &'static str)>,
    fns: BTreeSet<&'static str>,
}

impl RuntimeScan {
    fn scan() -> Self {
        let mut scan = RuntimeScan::default();
        for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
            for item in top_level_items(src) {
                scan.add(src, item);
            }
        }
        scan
    }

    fn add(&mut self, src: &'static str, (kind, name, text, attributed): TopItem<'static>) {
        if kind == ItemKind::Impl {
            if !attributed {
                self.impls.push((name, text));
            }
            return;
        }
        if kind == ItemKind::Fn {
            self.fns.insert(name);
        }
        if kind == ItemKind::Type && !attributed {
            self.types.push((name, text));
        }
        *self.seen.entry(name).or_default() += 1;
        // A `#[cfg]`-gated item may have a sibling for another target:
        // never take one along.
        if matches!(kind, ItemKind::Fn | ItemKind::Const) && !attributed {
            self.items.insert(name, RtItem { text });
        }
        let is_mod = kind == ItemKind::Other && lex(text).iter().take_while(|t| t.text != name).any(|t| t.text == "mod");
        if is_mod && !attributed && src.contains(&format!("\npub use {name}::*;")) {
            let whole: &'static str = Box::leak(format!("{text}\npub use {name}::*;").into_boxed_str());
            self.glob_mods.push((name, whole));
        }
    }
}

fn build_runtime_index() -> RuntimeIndex {
    let RuntimeScan { mut items, seen, glob_mods, types, impls, fns } = RuntimeScan::scan();
    items.retain(|n, _| seen.get(n) == Some(&1));
    take_glob_mods(&mut items, &seen, glob_mods);
    // Every name the runtime or the v0 prelude defines, at any depth: what
    // a body may reach only as a takeable item. The prelude rides in the
    // runtime crate's source (`emit_runtime_crate`).
    let prelude: &'static str = Box::leak(almide_codegen::emit_runtime_crate().into_boxed_str());
    let mut defined: BTreeSet<&'static str> = defined_names(prelude);
    let mut trait_methods = trait_method_names(prelude);
    let mut runtime_traits: BTreeSet<&'static str> = BTreeSet::new();
    for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
        defined.extend(defined_names(src));
        defined.extend(thread_local_names(src));
        trait_methods.extend(trait_method_names(src));
        runtime_traits.extend(trait_names(src));
    }
    let mut idx = take_prelude_traits(prelude, &runtime_traits, RuntimeIndex { items, defined, trait_methods, fns, fs_built: BTreeSet::new() });
    take_types(&mut idx, &seen, types, &impls);
    take_runtime_traits(&mut idx, &seen, &impls);
    take_fs_host_module(&mut idx);
    idx
}

/// Each glob-imported module defined once travels whole, taken by its name
/// or any name it defines that is not an item of its own.
fn take_glob_mods(
    items: &mut HashMap<&'static str, RtItem>,
    seen: &BTreeMap<&'static str, usize>,
    glob_mods: Vec<(&'static str, &'static str)>,
) {
    for (name, whole) in glob_mods {
        if seen.get(name) != Some(&1) {
            continue;
        }
        let body = &whole[whole.find('{').unwrap_or(0) + 1..whole.rfind("} //").or_else(|| whole.rfind('}')).unwrap_or(whole.len())];
        items.insert(name, RtItem { text: whole });
        for inner in defined_names(body) {
            if !items.contains_key(inner) && !seen.contains_key(inner) {
                items.insert(inner, RtItem { text: whole });
            }
        }
    }
}

/// A trait the prelude alone defines travels whole: its declaration and
/// every prelude impl of it, taken by any of its methods or its name.
/// (An impl a runtime module adds is not taken, so a body that reaches
/// the method through such a type fails the compile gate, never a
/// user's build.)
fn take_prelude_traits(prelude: &'static str, runtime_traits: &BTreeSet<&'static str>, idx: RuntimeIndex) -> RuntimeIndex {
    let RuntimeIndex { mut items, defined, mut trait_methods, fns, fs_built } = idx;
    let traits: Vec<_> = prelude_traits(prelude)
        .into_iter()
        .filter(|(name, ..)| !runtime_traits.contains(name) && !items.contains_key(name))
        .collect();
    for (name, decl, _, methods) in &traits {
        items.insert(name, RtItem { text: decl });
        for m in methods {
            trait_methods.remove(m);
            items.entry(m).or_insert(RtItem { text: decl });
        }
    }
    // An impl for a type the header cannot carry (it reaches such a
    // name) stays behind: no carried body holds a value of that type.
    let provisional = RuntimeIndex { items, defined, trait_methods, fns, fs_built };
    let mut taken: Vec<(&'static str, &'static str, Vec<&'static str>)> = Vec::new();
    for (name, decl, impls, methods) in traits {
        let mut whole = String::from(decl);
        for imp in impls.into_iter().filter(|imp| references_in(&provisional, imp).is_ok()) {
            whole.push('\n');
            whole.push_str(imp);
        }
        taken.push((name, &*Box::leak(whole.into_boxed_str()), methods));
    }
    let RuntimeIndex { mut items, defined, trait_methods, fns, fs_built } = provisional;
    for (name, whole, methods) in taken {
        items.insert(name, RtItem { text: whole });
        for m in methods {
            items.insert(m, RtItem { text: whole });
        }
    }
    RuntimeIndex { items, defined, trait_methods, fns, fs_built }
}

/// A runtime type travels with its inherent impls and its impls of std
/// traits (a trait the runtime or the prelude defines stays behind, as
/// a runtime module's impl of a prelude trait does above). Added last,
/// so the prelude traits' impl filter above never takes an impl for one.
fn take_types(
    idx: &mut RuntimeIndex,
    seen: &BTreeMap<&'static str, usize>,
    types: Vec<(&'static str, &'static str)>,
    impls: &[(&'static str, &'static str)],
) {
    for (name, decl) in types {
        if seen.get(name) != Some(&1) || idx.items.contains_key(name) {
            continue;
        }
        let mut whole = String::from(decl);
        for (_, imp) in impls.iter().filter(|(n, _)| *n == name) {
            let trait_of = impl_parts(&lex(imp)).and_then(|(t, _)| t);
            if trait_of.is_some_and(|t| idx.defined.contains(t)) {
                continue;
            }
            whole.push('\n');
            whole.push_str(imp);
        }
        idx.items.insert(name, RtItem { text: Box::leak(whole.into_boxed_str()) });
    }
}

/// A runtime trait defined once travels whole too: its declaration and
/// every runtime impl of it (`AlmideMapKey` and its blanket impls), taken
/// by its name or any of its methods.
fn take_runtime_traits(idx: &mut RuntimeIndex, seen: &BTreeMap<&'static str, usize>, impls: &[(&'static str, &'static str)]) {
    for (_, src) in almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES {
        for (name, decl, _, methods) in prelude_traits(src) {
            if seen.get(name) != Some(&1) || idx.items.contains_key(name) {
                continue;
            }
            let mut whole = String::from(decl);
            for (_, imp) in impls {
                if impl_parts(&lex(imp)).and_then(|(t, _)| t) == Some(name) {
                    whole.push('\n');
                    whole.push_str(imp);
                }
            }
            let whole: &'static str = Box::leak(whole.into_boxed_str());
            idx.items.insert(name, RtItem { text: whole });
            for m in methods {
                idx.trait_methods.remove(m);
                idx.items.entry(m).or_insert(RtItem { text: whole });
            }
        }
    }
}

/// Make the fs host module's `#[cfg]` twins, its modules and its types
/// takeable: each name defined more than once or under a gate becomes one
/// item of all its definitions (attributes kept), a module one item of its
/// whole text, a type one item with every impl the module gives it of a trait
/// the runtime does not define (as `take_types` carries a type). Every other
/// top-level fn and const stays the item it was.
fn take_fs_host_module(idx: &mut RuntimeIndex) {
    let Some(src) = fs_host_source() else { return };
    let items = scan_items(src, true);
    let mut defs: BTreeMap<&'static str, Vec<(ItemKind, &'static str, bool)>> = BTreeMap::new();
    for &(kind, name, text, attributed) in &items {
        if kind != ItemKind::Impl {
            defs.entry(name).or_default().push((kind, text, attributed));
        }
    }
    let mut built = BTreeSet::new();
    for (name, ds) in defs {
        let is_mod = ds.iter().any(|(k, t, _)| *k == ItemKind::Other && lex(t).iter().take_while(|x| x.text != name).any(|x| x.text == "mod"));
        let is_type = ds.iter().any(|(k, ..)| *k == ItemKind::Type);
        let twinned = ds.len() > 1 || ds.iter().any(|(_, _, a)| *a);
        if !(is_mod || is_type || twinned) {
            continue;
        }
        let mut whole: Vec<&str> = ds.iter().map(|(_, t, _)| *t).collect();
        if is_type {
            for &(kind, self_ty, text, _) in &items {
                if kind != ItemKind::Impl || self_ty != name {
                    continue;
                }
                let toks = lex(text);
                let at = toks.iter().position(|t| t.text == "impl").unwrap_or(0);
                let trait_of = impl_parts(&toks[at..]).and_then(|(t, _)| t);
                if !trait_of.is_some_and(|t| idx.defined.contains(t)) {
                    whole.push(text);
                }
            }
        }
        let text: &'static str = Box::leak(whole.join("\n").into_boxed_str());
        built.insert(text.as_ptr() as usize);
        idx.items.insert(name, RtItem { text });
    }
    // A top-level `thread_local! { … }` travels whole, by any static it defines.
    let toks = lex(src);
    for (i, t) in toks.iter().enumerate() {
        if t.depth == 0 && t.text == "thread_local" && toks.get(i + 1).is_some_and(|n| n.text == "!") {
            let end = item_end(&toks, i);
            let text = &src[t.off..toks.get(end).map_or(src.len(), |e| e.off + e.text.len())];
            for name in thread_local_names(text) {
                idx.items.insert(name, RtItem { text });
            }
        }
    }
    idx.fs_built = built;
}

/// The traits `src` defines at its top level.
fn trait_names(src: &str) -> BTreeSet<&str> {
    top_level_items(src)
        .into_iter()
        .filter(|(kind, name, text, _)| *kind == ItemKind::Other && lex(text).iter().take_while(|t| t.text != *name).any(|t| t.text == "trait"))
        .map(|(_, name, _, _)| name)
        .collect()
}

/// Each trait `src` defines at its top level (with no `#[cfg]`), as (name,
/// its declaration, every top-level impl of it in `src`, its method names).
#[allow(clippy::type_complexity)]
fn prelude_traits(src: &'static str) -> Vec<(&'static str, &'static str, Vec<&'static str>, Vec<&'static str>)> {
    let toks = lex(src);
    // Top-level impls: `impl … Trait for …` → the trait name before `for`.
    let mut impls: BTreeMap<&'static str, Vec<&'static str>> = BTreeMap::new();
    let mut i = 0;
    while i < toks.len() {
        if toks[i].depth == 0 && toks[i].text == "impl" {
            let end = item_end(&toks, i);
            let for_at = (i..end).find(|&k| toks[k].text == "for" && toks[k].depth == 0);
            if let Some(f) = for_at.filter(|f| *f > i + 1) {
                let tr = toks[f - 1].text;
                let end_off = toks.get(end).map(|t| t.off + t.text.len()).unwrap_or(src.len());
                impls.entry(tr).or_default().push(&src[toks[i].off..end_off]);
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    let mut out = Vec::new();
    for (kind, name, text, attributed) in top_level_items(src) {
        let ltoks = lex(text);
        if kind != ItemKind::Other || attributed || !ltoks.iter().take_while(|t| t.text != name).any(|t| t.text == "trait") {
            continue;
        }
        let methods: Vec<&'static str> = ltoks.windows(2).filter(|w| w[0].text == "fn").map(|w| w[1].text).collect();
        out.push((name, text, impls.get(name).cloned().unwrap_or_default(), methods));
    }
    out
}

/// The fn names the top-level `trait` items of `src` declare.
fn trait_method_names(src: &str) -> BTreeSet<&str> {
    let mut out = BTreeSet::new();
    for (kind, name, text, _) in top_level_items(src) {
        let toks = lex(text);
        if kind == ItemKind::Other && toks.iter().take_while(|t| t.text != name).any(|t| t.text == "trait") {
            out.extend(toks.windows(2).filter(|w| w[0].text == "fn").map(|w| w[1].text));
        }
    }
    out
}

