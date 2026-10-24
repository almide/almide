//! #4113 (C-397): temp files and directories are created exclusively under an
//! OS-entropy name, private to their owner. Its own test binary: the
//! `[permissions] fs` scope is process-wide, and tests/fs_scoped_core.rs
//! installs one.
#![cfg(unix)]

use almide_rt_core::fs_beneath_core::*;

/// #4113: a temp name already taken — by a symlink another user planted at
/// it — is never followed or truncated: the create is exclusive (O_EXCL),
/// the next name is tried, and the planted target is untouched. Negative
/// control: the pre-#4113 `std::fs::write(path, "")` truncated the target.
#[test]
fn a_planted_symlink_at_the_temp_name_is_skipped_never_written_through() {
    let t = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(t.path()).unwrap();
    std::fs::write(base.join("victim"), "keep").unwrap();
    std::os::unix::fs::symlink(base.join("victim"), base.join("tmpAAAA")).unwrap();
    std::fs::create_dir(base.join("tmpDDDD")).unwrap();
    let mut names = ["AAAA", "BBBB"].into_iter().map(str::to_string);
    let f = fs_temp_create_in(&base, "tmp", false, &mut || names.next().unwrap()).unwrap();
    assert!(f.ends_with("/tmpBBBB"), "{f}");
    assert_eq!(std::fs::read_to_string(base.join("victim")).unwrap(), "keep", "written through the planted link");
    assert!(std::fs::symlink_metadata(base.join("tmpAAAA")).unwrap().file_type().is_symlink());
    let mut names = ["DDDD", "EEEE"].into_iter().map(str::to_string);
    let d = fs_temp_create_in(&base, "tmp", true, &mut || names.next().unwrap()).unwrap();
    assert!(d.ends_with("/tmpEEEE"), "an existing directory was reused: {d}");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(std::fs::metadata(&d).unwrap().permissions().mode() & 0o777, 0o700);
    // Every name taken: the last try's EEXIST, nothing created.
    let r = fs_temp_create_in(&base, "tmp", false, &mut || "AAAA".to_string());
    assert!(matches!(&r, Err((p, e)) if p.ends_with("/tmpAAAA") && e.kind() == std::io::ErrorKind::AlreadyExists), "{r:?}");
    // Two fresh names from the OS source differ and are 16 hex digits.
    let (a, b) = (fs_temp_suffix(), fs_temp_suffix());
    assert!(a != b && a.len() == 16 && a.chars().all(|c| c.is_ascii_hexdigit()), "{a} {b}");
}
