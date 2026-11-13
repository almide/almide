// ── the native host modules of native_floor.rs (#4006 class E), include!-spliced back at module level ──
//
// The native host calls (proofs/TRUSTED_BASE.md, "Native fs host calls",
// owner ruling 2026-11-05, and "Native unix host calls", owner ruling
// 2026-11-10/13): a native program calls the runtime's own fs or unix code as
// a TRUSTED host call, the way the wasm leg calls its host's imports. That
// code is a whole runtime module (runtime/rs/src/fs.rs or unix.rs with the
// rt-core files it includes, as the standard codegen emits it), written
// against platform `#[cfg]` twins and path-reached modules
// (`almide_sfs::read_bounded`, `almide_fs_beneath_sys::Handle`,
// `almide_unix_sys`) that the item resolver refuses anywhere else. Inside a
// host module it takes them along: a name with `#[cfg]` twins travels as
// every twin together, each under its own gate, and a module travels whole,
// so the build compiles exactly the definitions the standard build compiles
// for the target.
//
// Only the calls named for a module's class (`crate::host_ops::NATIVE_HOST_CLASSES`)
// may carry that module's code: a floor body outside the set whose closure
// reaches it is refused, and every call the render makes to one of them is
// recorded as a host call in the native-calls witness, whose checker refuses
// a host call outside the named sets (proofs/NativeCalls.v, `hosts_ok`).

/// The runtime modules the native host calls are served by, in the order of
/// `crate::host_ops::NATIVE_HOST_CLASSES` (a unit test holds the two equal;
/// this file is also spliced into the build script, which has no host_ops).
const HOST_MODULES: &[&str] = &["fs", "unix"];

/// The whole text of the host module `module`, as the runtime table holds it.
fn host_source(module: &str) -> Option<&'static str> {
    almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES
        .iter()
        .find(|(m, _)| *m == module)
        .map(|(_, s)| *s)
}

/// The host module `text` is code of: a slice of the module's source, or an
/// item [`take_host_module`] built from it.
fn host_module_of_text_in(idx: &RuntimeIndex, text: &str) -> Option<&'static str> {
    let t = text.as_ptr() as usize;
    if let Some(m) = idx.host_built.get(&t) {
        return Some(m);
    }
    HOST_MODULES.iter().copied().find(|m| {
        host_source(m).is_some_and(|src| {
            let s = src.as_ptr() as usize;
            t >= s && t + text.len() <= s + src.len()
        })
    })
}

/// Every name the host module `module` defines, at any depth and under any
/// `#[cfg]`: inside the module's text they name its own items.
fn host_own_names(module: &str) -> &'static BTreeSet<&'static str> {
    static OWN: OnceLock<Vec<(&'static str, BTreeSet<&'static str>)>> = OnceLock::new();
    static NONE: BTreeSet<&'static str> = BTreeSet::new();
    let all = OWN.get_or_init(|| {
        HOST_MODULES
            .iter()
            .map(|m| {
                let mut out = BTreeSet::new();
                if let Some(src) = host_source(m) {
                    out.extend(defined_names(src));
                    out.extend(thread_local_names(src));
                }
                (*m, out)
            })
            .collect()
    });
    all.iter().find(|(m, _)| *m == module).map_or(&NONE, |(_, s)| s)
}
