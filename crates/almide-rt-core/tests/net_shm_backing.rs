//! #4850: a shared-memory region's backing file is storage no path names.
//! `[permissions] fs` bounds the paths a program reaches; a region is not one,
//! so its file must never appear under any path — in a fresh directory the
//! test has never used, the directory stays empty while the region is open
//! and written. Row `net.shm-create-tempfile` of proofs/decision-sites.toml.
#![cfg(unix)]

#[path = "../../../tests/support/novel.rs"]
mod novel;

#[test]
fn novel_shm_backing_file_has_no_name_in_a_fresh_directory() {
    let dir = std::env::temp_dir().join(novel::path_segment());
    std::fs::create_dir(&dir).unwrap();
    let file = almide_rt_core::net_unix_core::almide_shm_open_in(&dir).expect("a region opens");
    std::os::unix::fs::FileExt::write_all_at(&file, b"region bytes", 0).unwrap();
    let names: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
    assert!(names.is_empty(), "the region's backing file is reachable by a path: {names:?}");
    let mut back = [0u8; 12];
    std::os::unix::fs::FileExt::read_exact_at(&file, &mut back, 0).unwrap();
    assert_eq!(&back, b"region bytes");
    drop(file);
    std::fs::remove_dir(&dir).unwrap();
}
