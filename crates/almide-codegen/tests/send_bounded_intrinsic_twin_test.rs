//! #3649: an `@intrinsic` whose native body puts a `Send` bound on a generic
//! (it runs its callback on threads) is reached from a stdlib signature whose
//! type variables carry no bound, so the checker admits a non-`Send` type —
//! a Matrix or Bytes (`Rc` underneath), a closure value (`Rc<dyn Fn>`). Each
//! such intrinsic must have a `<name>_seq` twin with no `Send` bound, which
//! RustLowering routes a non-`Send` call to. A new threaded intrinsic without
//! a twin fails here instead of as a rustc E0277 in a user's program.

use almide_codegen::generated::rust_runtime::RUST_RUNTIME_MODULES;

/// The generic clause of `pub fn <name><…>(`, if the runtime defines `name`.
fn generics_of<'a>(name: &str) -> Option<&'a str> {
    let head = format!("pub fn {name}");
    RUST_RUNTIME_MODULES.iter().find_map(|(_, src)| {
        let mut from = 0;
        while let Some(i) = src[from..].find(&head) {
            let rest = &src[from + i + head.len()..];
            from += i + head.len();
            if rest.starts_with('(') {
                return Some("");
            }
            if rest.starts_with('<') {
                let end = rest.find(">(")?;
                return Some(&rest[..=end]);
            }
        }
        None
    })
}

fn stdlib_intrinsics() -> Vec<String> {
    let mut out = Vec::new();
    for &module in almide_lang::stdlib_info::BUNDLED_MODULES {
        let Some(src) = almide_lang::stdlib_info::bundled_source(module) else { continue };
        for line in src.lines() {
            let Some(rest) = line.trim().strip_prefix("@intrinsic(\"") else { continue };
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn every_send_bounded_intrinsic_has_a_sequential_twin() {
    let threaded: Vec<String> = stdlib_intrinsics()
        .into_iter()
        .filter(|n| generics_of(n).is_some_and(|g| g.contains("Send")))
        .collect();
    assert!(
        threaded.iter().any(|n| n == "almide_rt_fs_fold_lines_chunked"),
        "the scan must see the known threaded intrinsic, found {threaded:?}"
    );
    let missing: Vec<String> = threaded
        .iter()
        .filter(|n| !generics_of(&format!("{n}_seq")).is_some_and(|g| !g.contains("Send")))
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "threaded intrinsics without a Send-free `_seq` twin: {missing:?}"
    );
}
