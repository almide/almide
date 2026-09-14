//! The fs errno table is the host's `std::io::Error` `Display`, row for row
//! (#2206, C-215): every leg that cannot call `Display` spells these bytes
//! from the table, so the table must equal `Display` on the hosts the legs
//! are compared on. This runs on every CI host (linux, macos).
use almide_base::fs_errno::{by_os, by_wasi, FS_ERRNOS};

#[test]
fn every_row_is_the_hosts_display_of_its_errno() {
    for r in FS_ERRNOS {
        let want = std::io::Error::from_raw_os_error(r.os).to_string();
        assert_eq!(r.text, want, "{}: the table spells what this host does not", r.name);
    }
}

#[test]
fn every_row_carries_its_own_os_number_as_the_suffix() {
    for r in FS_ERRNOS {
        assert!(r.text.ends_with(&format!("(os error {})", r.os)), "{}: {}", r.name, r.text);
    }
}

#[test]
fn codes_and_names_are_unique_and_both_lookups_round_trip() {
    let mut wasi: Vec<u16> = FS_ERRNOS.iter().map(|r| r.wasi).collect();
    let mut os: Vec<i32> = FS_ERRNOS.iter().map(|r| r.os).collect();
    let mut names: Vec<&str> = FS_ERRNOS.iter().map(|r| r.name).collect();
    wasi.sort_unstable();
    wasi.dedup();
    os.sort_unstable();
    os.dedup();
    names.sort_unstable();
    names.dedup();
    assert_eq!(wasi.len(), FS_ERRNOS.len(), "duplicate WASI code");
    assert_eq!(os.len(), FS_ERRNOS.len(), "duplicate os errno");
    assert_eq!(names.len(), FS_ERRNOS.len(), "duplicate name");
    for r in FS_ERRNOS {
        assert_eq!(by_wasi(r.wasi), Some(r));
        assert_eq!(by_os(r.os), Some(r));
    }
    assert_eq!(by_wasi(55), None, "ENOTEMPTY differs between linux and macos and must stay unspelled");
}
