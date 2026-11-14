//! #4006 class E: the native fs host calls are ONE named set.
//!
//! The trusted base names the runtime fs functions a program on the certified
//! native render may call as trusted host calls (proofs/TRUSTED_BASE.md,
//! "Native fs host calls"). The set is written four times: the render's list
//! (`almide_mir::host_ops::NATIVE_FS_HOST_CALLS`, whose index is the witness's
//! host id), the proven checker's (`NativeCalls.native_fs_host_calls`),
//! almide-verify's, and the trusted base's row. This test holds the four to
//! one list in one order, and each name to the runtime symbol of an `fs` host
//! op the render serves from the floor, so the row names exactly what runs.

use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The quoted names of `NativeCalls.<list>`, in order.
fn coq_list(list: &str) -> Vec<String> {
    let v = std::fs::read_to_string(root().join("proofs/NativeCalls.v")).unwrap();
    let start = v.find(&format!("Definition {list} ")).unwrap_or_else(|| panic!("NativeCalls.v defines {list}"));
    let body = &v[start..start + v[start..].find("].").expect("the list ends")];
    body.split('"').skip(1).step_by(2).map(str::to_string).collect()
}

/// The backticked runtime names of the trusted base's row `title`, in order.
fn trusted_base_row(title: &str, prefix: &str) -> Vec<String> {
    let t = std::fs::read_to_string(root().join("proofs/TRUSTED_BASE.md")).unwrap();
    let start = t.find(title).unwrap_or_else(|| panic!("TRUSTED_BASE.md has the row {title}"));
    let row = &t[start..start + t[start..].find("\n\n").unwrap()];
    row.split('`').skip(1).step_by(2).filter(|n| n.starts_with(prefix)).map(str::to_string).collect()
}

#[test]
fn the_four_copies_of_the_named_set_agree() {
    let mir: Vec<String> = almide_mir::host_ops::NATIVE_FS_HOST_CALLS.iter().map(|s| s.to_string()).collect();
    let verify: Vec<String> = almide_verify::NATIVE_FS_HOST_CALLS.iter().map(|s| s.to_string()).collect();
    assert!(!mir.is_empty());
    assert_eq!(coq_list("native_fs_host_calls"), mir, "NativeCalls.v's native_fs_host_calls differs from host_ops::NATIVE_FS_HOST_CALLS");
    assert_eq!(verify, mir, "almide-verify's NATIVE_FS_HOST_CALLS differs from host_ops::NATIVE_FS_HOST_CALLS");
    assert_eq!(trusted_base_row("**Native fs host calls (#4006 class E).**", "almide_rt_fs_"), mir, "the TRUSTED_BASE.md row names a different set");
}

/// The unix class (owner ruling 2026-11-10, before rc1 on 2026-11-13) is
/// written the same four times, and its ids follow the fs set's.
#[test]
fn the_four_copies_of_the_unix_set_agree() {
    let mir: Vec<String> = almide_mir::host_ops::NATIVE_UNIX_HOST_CALLS.iter().map(|s| s.to_string()).collect();
    let verify: Vec<String> = almide_verify::NATIVE_UNIX_HOST_CALLS.iter().map(|s| s.to_string()).collect();
    assert_eq!(mir.len(), 12);
    assert_eq!(coq_list("native_unix_host_calls"), mir, "NativeCalls.v's native_unix_host_calls differs from host_ops::NATIVE_UNIX_HOST_CALLS");
    assert_eq!(verify, mir, "almide-verify's NATIVE_UNIX_HOST_CALLS differs from host_ops::NATIVE_UNIX_HOST_CALLS");
    assert_eq!(trusted_base_row("**Native unix host calls.**", "almide_rt_unix_"), mir, "the TRUSTED_BASE.md row names a different set");
    let fs = almide_mir::host_ops::NATIVE_FS_HOST_CALLS.len();
    for (i, sym) in almide_mir::host_ops::NATIVE_UNIX_HOST_CALLS.iter().enumerate() {
        assert_eq!(almide_mir::host_ops::native_host_id(sym), Some(fs + i));
    }
    assert_eq!(almide_mir::host_ops::native_host_call_count(), fs + mir.len());
}

/// Every unix host op is named, and every named one is served on the floor.
#[test]
fn every_unix_host_op_is_named_and_served() {
    use almide_mir::host_ops::{HOST_OPS, NATIVE_UNIX_HOST_CALLS};
    let rows: Vec<&str> = HOST_OPS.iter().filter(|o| o.module == "unix").map(|o| o.symbol).collect();
    assert_eq!(rows.len(), NATIVE_UNIX_HOST_CALLS.len());
    for sym in NATIVE_UNIX_HOST_CALLS {
        let row = HOST_OPS.iter().find(|o| o.module == "unix" && o.symbol == *sym);
        let row = row.unwrap_or_else(|| panic!("{sym} is the symbol of no unix host op"));
        let call = format!("unix.{}", row.func);
        assert!(
            almide_mir::native_floor::has_native_host_route(&call),
            "{sym} is named but `{call}` is not on the native floor: {:?}",
            almide_mir::native_floor::refusal(&call)
        );
    }
}

#[test]
fn every_named_call_is_an_fs_host_op_the_floor_serves_and_every_served_one_is_named() {
    use almide_mir::host_ops::{HOST_OPS, NATIVE_FS_HOST_CALLS};
    for sym in NATIVE_FS_HOST_CALLS {
        let row = HOST_OPS.iter().find(|o| o.module == "fs" && o.symbol == *sym);
        let row = row.unwrap_or_else(|| panic!("{sym} is the symbol of no fs host op"));
        let call = format!("fs.{}", row.func);
        assert!(
            almide_mir::native_floor::has_native_host_route(&call),
            "{sym} is named but `{call}` is not on the native floor: {:?}",
            almide_mir::native_floor::refusal(&call)
        );
    }
    // The converse: an fs host op the floor serves is named (else the render
    // would refuse it, so the row and the floor cannot drift apart).
    for o in HOST_OPS.iter().filter(|o| o.module == "fs") {
        let call = format!("fs.{}", o.func);
        if almide_mir::native_floor::has_native_host_route(&call) {
            assert!(NATIVE_FS_HOST_CALLS.contains(&o.symbol), "`{call}` is on the floor but {} is not named", o.symbol);
        }
    }
}

#[test]
fn named_host_boundary_accepts_optional_bytes_and_rejects_unknown_ids() {
    use almide_verify::{check, Property, NATIVE_FS_HOST_CALLS, NATIVE_UNIX_HOST_CALLS};
    let id = NATIVE_FS_HOST_CALLS.iter()
        .position(|name| *name == "almide_rt_fs_read_bytes_if_exists").unwrap();
    assert!(check(Property::NativeCalls, format!("||| {id}\nid").as_bytes()));
    let named = NATIVE_FS_HOST_CALLS.len() + NATIVE_UNIX_HOST_CALLS.len();
    // The last unix id is named; the first id past both sets is not.
    assert!(check(Property::NativeCalls, format!("||| {}\nid", named - 1).as_bytes()));
    for unknown in [named, usize::MAX] {
        assert!(!check(Property::NativeCalls, format!("||| {unknown}\nid").as_bytes()));
    }
    // A named host is no exemption from the caller's ownership obligations.
    assert!(!check(Property::NativeCalls, format!("||| {id}\nidd").as_bytes()));
    // Keep the saved poison outside the admitted set when that set grows.
    let poison = include_str!("../proofs/poisoned-certs/native-calls/4006-host-outside-set.cert");
    let host = poison.lines().next().unwrap().strip_prefix("|||").unwrap();
    assert_eq!(host.parse::<usize>().unwrap(), named);
    assert!(!check(Property::NativeCalls, poison.as_bytes()));
}
