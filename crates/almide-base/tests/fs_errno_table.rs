//! The fs errno table has two kinds of row (#2206, C-273).
//!
//! A DISPLAY row is the host's `std::io::Error` `Display`, verbatim: every leg
//! that cannot call `Display` spells these bytes from the table, so the table
//! must equal `Display` on the hosts the legs are compared on.
//!
//! A CLASSIFIED row (owner ruling 2026-10-17) is an errno whose number or text
//! differs between those hosts — ENOTEMPTY, ENAMETOOLONG, EXDEV. Its text is
//! fixed by the table and carries no number; what must hold on each host is
//! that the host's errno decodes to the row's `ErrorKind`, that the native
//! classifier answers the row's text for it, and that the text is the wording
//! the host's own Display ends with. This runs on every CI host (linux, macos).
use almide_base::fs_errno::{
    by_os, by_wasi, fs_io_error_text, text_by_wasi, wasi_texts, FsClassifiedErrno, FS_CLASSIFIED_ERRNOS, FS_ERRNOS,
};

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
    assert_eq!(by_wasi(55), None, "ENOTEMPTY differs between linux and macos and must not be a Display row");
}

/// The errno number of a classified row on the host the suite runs on.
fn host_number(r: &FsClassifiedErrno) -> i32 {
    if cfg!(target_os = "macos") { r.macos } else { r.linux }
}

#[test]
fn the_classified_rows_are_enotempty_enametoolong_and_exdev() {
    let rows: Vec<(&str, u16, &str)> = FS_CLASSIFIED_ERRNOS.iter().map(|r| (r.name, r.wasi, r.text)).collect();
    assert_eq!(
        rows,
        [
            ("ENOTEMPTY", 55, "Directory not empty"),
            ("ENAMETOOLONG", 37, "File name too long"),
            ("EXDEV", 75, "Cross-device link"),
        ]
    );
}

#[test]
fn a_classified_text_carries_no_number() {
    for r in FS_CLASSIFIED_ERRNOS {
        assert!(!r.text.chars().any(|c| c.is_ascii_digit()), "{}: {}", r.name, r.text);
        assert!(!r.text.contains("os error") && !r.text.contains("errno"), "{}: {}", r.name, r.text);
    }
}

#[test]
fn a_classified_row_is_what_this_hosts_errno_decodes_to() {
    for r in FS_CLASSIFIED_ERRNOS {
        let e = std::io::Error::from_raw_os_error(host_number(r));
        assert_eq!(e.kind(), r.kind, "{}: this host decodes its errno to another kind", r.name);
        assert_eq!(fs_io_error_text(&e), r.text, "{}: the native classifier", r.name);
        let display = e.to_string();
        let wording = display.split(" (os error").next().unwrap_or_default().to_ascii_lowercase();
        assert!(
            wording.ends_with(&r.text.to_ascii_lowercase()),
            "{}: `{}` is not the wording this host's `{display}` ends with",
            r.name,
            r.text
        );
    }
}

#[test]
fn the_classifier_keeps_every_other_error_as_the_hosts_display() {
    for r in FS_ERRNOS {
        let e = std::io::Error::from_raw_os_error(r.os);
        assert_eq!(fs_io_error_text(&e), r.text, "{}", r.name);
    }
    // Only an OS error is classified: a library error of the same kind keeps
    // its own message.
    let lib = std::io::Error::new(std::io::ErrorKind::DirectoryNotEmpty, "a library's own words");
    assert_eq!(fs_io_error_text(&lib), "a library's own words");
}

#[test]
fn every_wasi_code_is_spelled_once_across_both_kinds_of_row() {
    let mut codes: Vec<u16> = wasi_texts().map(|(c, _)| c).collect();
    let n = codes.len();
    assert_eq!(n, FS_ERRNOS.len() + FS_CLASSIFIED_ERRNOS.len());
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), n, "a WASI code is both a Display and a classified row");
    for (code, text) in wasi_texts() {
        assert_eq!(text_by_wasi(code), Some(text));
    }
    assert_eq!(text_by_wasi(55), Some("Directory not empty"));
    assert_eq!(text_by_wasi(58), None, "an errno neither kind spells keeps its leg's numbered fallback");
}
