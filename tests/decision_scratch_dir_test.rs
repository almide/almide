//! #4853: the files a tool hands another process (the certificate gate's
//! artifact and bundle) live in a directory CREATED for them, mode 0700, never
//! at a name that already exists. A fresh stem and seed name a candidate the
//! test pre-plants as a symlink to a victim directory: the helper must skip it,
//! not enter or follow it. Row `verify.gate-build-scratch-files`.
#![cfg(unix)]

#[path = "support/novel.rs"]
mod novel;

#[test]
fn novel_a_planted_scratch_name_is_skipped_never_followed() {
    use std::os::unix::fs::PermissionsExt;
    let base = tempfile::TempDir::new().unwrap();
    let victim = base.path().join(novel::path_segment());
    std::fs::create_dir(&victim).unwrap();
    let stem = novel::path_segment();
    let seed = u128::from(novel::fs_op() as u32);
    let planted = base.path().join(format!("{stem}-{}-{seed}-0", std::process::id()));
    std::os::unix::fs::symlink(&victim, &planted).unwrap();

    let dir = almide_base::private_dir::private_dir_in(base.path(), &stem, seed).expect("a private dir");
    assert_ne!(dir, planted, "the planted name was reused");
    assert!(!std::fs::symlink_metadata(&dir).unwrap().file_type().is_symlink(), "the scratch dir is a link");
    assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700, "the scratch dir is not private");
    std::fs::write(dir.join("artifact.wasm"), b"x").unwrap();
    assert_eq!(std::fs::read_dir(&victim).unwrap().count(), 0, "a write went through the planted link");
}
