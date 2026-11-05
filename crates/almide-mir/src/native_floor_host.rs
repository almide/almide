// ── the native fs host module of native_floor.rs (#4006 class E), include!-spliced back at module level ──
//
// The native fs host calls (proofs/TRUSTED_BASE.md, "Native fs host calls";
// owner ruling 2026-11-05): a native program calls the runtime's own fs code
// as a TRUSTED host call, the way the wasm leg calls its host's fs imports.
// That code is the runtime's `fs` module (runtime/rs/src/fs.rs with the
// rt-core files it includes, as the standard codegen emits it), which is
// written against platform `#[cfg]` twins and path-reached modules
// (`almide_sfs::read_bounded`, `almide_fs_beneath_sys::Handle`) that the item
// resolver refuses anywhere else. Inside the fs module it takes them along:
// a name with `#[cfg]` twins travels as every twin together, each under its
// own gate, and a module travels whole, so the build compiles exactly the
// definitions the standard build compiles for the target.
//
// Only the calls named in [`crate::host_ops::NATIVE_FS_HOST_CALLS`] may carry
// fs module code: a floor body outside that set whose closure reaches it is
// refused here, and every call the render makes to one of them is recorded as
// a host call in the native-calls witness, whose checker refuses a host call
// outside the named set (proofs/NativeCalls.v, `hosts_ok`).

/// The runtime module the native fs host calls are served by.
const FS_HOST_MODULE: &str = "fs";

/// The whole text of the fs host module, as the runtime table holds it.
fn fs_host_source() -> Option<&'static str> {
    almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES
        .iter()
        .find(|(m, _)| *m == FS_HOST_MODULE)
        .map(|(_, s)| *s)
}

/// The items [`take_fs_host_module`] built (twin groups, modules, types with
/// their impls): fs module code that is not a slice of its source.
static FS_HOST_BUILT: OnceLock<BTreeSet<usize>> = OnceLock::new();

/// Is `text` fs host module code: a slice of the module's source, or an item
/// [`take_fs_host_module`] built from it?
fn is_fs_host_text(text: &str) -> bool {
    let Some(src) = fs_host_source() else { return false };
    let (s, t) = (src.as_ptr() as usize, text.as_ptr() as usize);
    (t >= s && t + text.len() <= s + src.len()) || FS_HOST_BUILT.get().is_some_and(|b| b.contains(&t))
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
    let _ = FS_HOST_BUILT.set(built);
}

/// `Err` when the closure of `symbol` carries the fs host module and
/// `symbol` is not one of the named native fs host calls.
fn fs_host_gate(symbol: &str, items: &[&'static str]) -> Result<(), String> {
    if items.iter().any(|t| is_fs_host_text(t)) && !crate::host_ops::is_native_fs_host_call(symbol) {
        return Err(format!(
            "`{symbol}` reaches the native fs host module, which only the named native fs host calls may (proofs/TRUSTED_BASE.md)"
        ));
    }
    Ok(())
}

/// The text a runtime fn's signature is read from: its own item, or, for a
/// fn of the fs host module (whose name resolves to the whole module), its
/// own definition inside the module.
fn signature_text(rt: &'static RuntimeIndex, symbol: &str) -> Option<&'static str> {
    let item = rt.items.get(symbol)?.text;
    if !is_fs_host_text(item) {
        return Some(item);
    }
    static FNS: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    FNS.get_or_init(|| {
        let mut out = HashMap::new();
        let mut twice = BTreeSet::new();
        for (kind, name, text, attributed) in fs_host_source().map(top_level_items).unwrap_or_default() {
            if kind == ItemKind::Fn && !attributed && out.insert(name, text).is_some() {
                twice.insert(name);
            }
        }
        out.retain(|n, _| !twice.contains(n));
        out
    })
    .get(symbol)
    .copied()
}

/// Every name the fs host module defines, at any depth and under any
/// `#[cfg]`: inside the module's text they name its own items.
fn fs_host_own_names() -> &'static BTreeSet<&'static str> {
    static OWN: OnceLock<BTreeSet<&'static str>> = OnceLock::new();
    OWN.get_or_init(|| {
        let Some(src) = fs_host_source() else { return BTreeSet::new() };
        let mut out = defined_names(src);
        out.extend(thread_local_names(src));
        out
    })
}

#[cfg(test)]
mod fs_host_tests {
    use super::*;

    /// A floor body outside the named set whose closure reaches fs host code
    /// is refused; a named one is admitted; one carrying no fs code is free.
    #[test]
    fn only_a_named_call_may_carry_fs_host_code() {
        let fs_item = runtime_index().items.get("almide_rt_fs_write").expect("fs.write_text's body is an item").text;
        assert!(is_fs_host_text(fs_item));
        assert!(fs_host_gate("almide_rt_fs_write", &[fs_item]).is_ok());
        let refused = fs_host_gate("almide_rt_string_len", &[fs_item]).unwrap_err();
        assert!(refused.contains("native fs host module"), "{refused}");
        assert!(fs_host_gate("almide_rt_string_len", &["fn f() {}"]).is_ok());
    }

    /// The module's `#[cfg]` twins travel together: the name resolves to an
    /// item holding every gated definition.
    #[test]
    fn a_cfg_twinned_fs_name_carries_every_twin() {
        let src = fs_host_source().expect("the runtime has an fs module");
        let twinned = scan_items(src, true)
            .into_iter()
            .filter(|(k, _, _, a)| *k != ItemKind::Impl && *a)
            .map(|(_, n, _, _)| n)
            .next()
            .expect("the fs module has a #[cfg] item");
        let item = runtime_index().items.get(twinned).expect("the twinned name is takeable").text;
        assert!(is_fs_host_text(item));
        assert!(item.contains("#[cfg"), "{twinned}: the gate travels with the item");
    }
}
