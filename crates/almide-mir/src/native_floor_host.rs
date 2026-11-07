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

/// Is `text` fs host module code: a slice of the module's source, or an item
/// [`take_fs_host_module`] built from it?
fn is_fs_host_text_in(idx: &RuntimeIndex, text: &str) -> bool {
    let Some(src) = fs_host_source() else { return false };
    let (s, t) = (src.as_ptr() as usize, text.as_ptr() as usize);
    (t >= s && t + text.len() <= s + src.len()) || idx.fs_built.contains(&t)
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
