//! #4201: the bounded whole-file reader every leg's `fs.read_*` shares —
//! a regular file only, at most `FS_READ_MAX_BYTES` (the cap exercised here
//! at small values, so the boundary costs nothing to test).

use almide_base::fs_errno::{
    FS_READ_MAX_BYTES, FS_READ_NOT_REGULAR_TEXT, FS_READ_TOO_LARGE_TEXT, fs_io_error_text, fs_read_bounded_from,
    fs_read_bounded_path, fs_read_bounded_refusal,
};
use std::path::PathBuf;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("almide-fs-read-bounded-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch dir");
    d
}

#[test]
fn the_cap_is_one_gib_and_the_texts_name_it() {
    assert_eq!(FS_READ_MAX_BYTES, 1 << 30);
    assert!(FS_READ_TOO_LARGE_TEXT.contains(&FS_READ_MAX_BYTES.to_string()));
}

#[test]
fn a_read_of_exactly_the_cap_answers_and_one_byte_more_errs() {
    let data = [7u8; 16];
    assert_eq!(fs_read_bounded_from(&mut &data[..], 16).expect("at the cap").len(), 16);
    assert_eq!(fs_read_bounded_from(&mut &data[..15], 16).expect("under the cap").len(), 15);
    let e = fs_read_bounded_from(&mut &data[..], 15).expect_err("past the cap");
    assert_eq!(fs_io_error_text(&e), FS_READ_TOO_LARGE_TEXT);
    assert!(fs_read_bounded_refusal(&e));
    // An endless reader is bounded too: it stops at cap + 1 bytes.
    let e = fs_read_bounded_from(&mut std::io::repeat(0), 1024).expect_err("endless");
    assert_eq!(e.to_string(), FS_READ_TOO_LARGE_TEXT);
}

#[test]
fn a_regular_file_reads_whole_and_past_the_cap_errs_by_its_stat() {
    let d = scratch("reg");
    let f = d.join("f.bin");
    std::fs::write(&f, b"hello").expect("write");
    assert_eq!(fs_read_bounded_path(&f, 5).expect("at the cap"), b"hello");
    let e = fs_read_bounded_path(&f, 4).expect_err("past the cap");
    assert_eq!(e.to_string(), FS_READ_TOO_LARGE_TEXT);
    // A sparse file one byte past the real cap: refused by its stat, before
    // a byte is read (the test would otherwise allocate 1 GiB).
    let big = d.join("sparse.bin");
    std::fs::File::create(&big).and_then(|h| h.set_len(FS_READ_MAX_BYTES + 1)).expect("sparse");
    let e = fs_read_bounded_path(&big, FS_READ_MAX_BYTES).expect_err("sparse past the cap");
    assert_eq!(e.to_string(), FS_READ_TOO_LARGE_TEXT);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_directory_keeps_its_eisdir() {
    let d = scratch("dir");
    let e = fs_read_bounded_path(&d, FS_READ_MAX_BYTES).expect_err("a directory");
    assert_eq!(fs_io_error_text(&e), "Is a directory (os error 21)");
    assert!(!fs_read_bounded_refusal(&e));
    let _ = std::fs::remove_dir_all(&d);
}

#[cfg(unix)]
#[test]
fn a_device_is_not_a_regular_file() {
    let e = fs_read_bounded_path(std::path::Path::new("/dev/zero"), FS_READ_MAX_BYTES).expect_err("/dev/zero");
    assert_eq!(fs_io_error_text(&e), FS_READ_NOT_REGULAR_TEXT);
    let e = fs_read_bounded_path(std::path::Path::new("/dev/null"), FS_READ_MAX_BYTES).expect_err("/dev/null");
    assert_eq!(e.to_string(), FS_READ_NOT_REGULAR_TEXT);
}

/// A FIFO with no writer: the open must not block (O_NONBLOCK) and the read
/// is refused. Run on a thread with a deadline so a regression fails by name
/// instead of hanging the suite.
#[cfg(unix)]
#[test]
fn a_fifo_is_refused_without_blocking() {
    let d = scratch("fifo");
    let p = d.join("pipe");
    let made = std::process::Command::new("mkfifo").arg(&p).status().expect("mkfifo");
    assert!(made.success());
    let (tx, rx) = std::sync::mpsc::channel();
    let q = p.clone();
    std::thread::spawn(move || {
        let _ = tx.send(fs_read_bounded_path(&q, FS_READ_MAX_BYTES).map_err(|e| e.to_string()));
    });
    let got = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("the read of a FIFO blocked");
    assert_eq!(got, Err(FS_READ_NOT_REGULAR_TEXT.to_string()));
    let _ = std::fs::remove_dir_all(&d);
}
