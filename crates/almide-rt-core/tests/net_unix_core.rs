//! The unix-socket and shared-memory core (#3620): one table of the
//! descriptors a run holds, which both native (per thread) and the embedded
//! wasm host (per run) keep. These pin what the table itself decides — a
//! descriptor the run does not hold is refused before any system call, and
//! closing a socket closes the descriptors it received that were never taken
//! — and the crate's one sanctioned `unsafe` site.
#![cfg(unix)]

use almide_rt_core::net_unix_core::AlmideUnixTables;

fn sock_path(name: &str) -> std::path::PathBuf {
    let p = std::path::PathBuf::from(format!("/tmp/arc-{}-{name}.sock", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn a_descriptor_the_run_does_not_hold_is_refused() {
    let mut t = AlmideUnixTables::default();
    assert_eq!(t.unix_close(0), Err("unix_close: descriptor 0 is not open in this program".to_string()));
    assert_eq!(t.unix_poll(2, 0), Err("unix_poll: descriptor 2 is not open in this program".to_string()));
    assert_eq!(t.shm_read(-1, 0, 1), Err("shm_read: not a file descriptor: -1".to_string()));
    assert_eq!(t.unix_wait(&[1 << 40], 0), Err("unix_wait: not a file descriptor: 1099511627776".to_string()));
    let f = t.shm_create(4).expect("shm_create");
    t.unix_close(f).expect("first close");
    assert_eq!(t.unix_close(f), Err(format!("unix_close: descriptor {f} is not open in this program")));
    assert!(t.unix_take_fds(f).is_empty());
}

#[test]
fn closing_a_socket_closes_the_descriptors_it_received_and_nobody_took() {
    let path = sock_path("untaken");
    let p = path.to_str().unwrap();
    let mut t = AlmideUnixTables::default();
    let l = t.unix_listen(p).unwrap();
    let c = t.unix_connect(p).unwrap();
    let s = t.unix_accept(l).unwrap();
    let a = t.shm_create(8).unwrap();
    t.unix_send(c, b"x", &[a]).unwrap();
    assert_eq!(t.unix_recv(s, 4).unwrap(), b"x");
    let held_before = t.held();
    t.unix_close(s).unwrap();
    // s and the one descriptor that arrived on it.
    assert_eq!(t.held(), held_before - 2);
    for fd in [c, l, a] {
        t.unix_close(fd).unwrap();
    }
    assert_eq!(t.held(), 0);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn received_descriptors_are_held_once_taken() {
    let path = sock_path("taken");
    let p = path.to_str().unwrap();
    let mut t = AlmideUnixTables::default();
    let l = t.unix_listen(p).unwrap();
    let c = t.unix_connect(p).unwrap();
    let s = t.unix_accept(l).unwrap();
    let a = t.shm_create(8).unwrap();
    t.shm_write(a, 0, b"abc").unwrap();
    t.unix_send(c, b"hi", &[a, a]).unwrap();
    assert_eq!(t.unix_recv(s, 10).unwrap(), b"hi");
    let got = t.unix_take_fds(s);
    assert_eq!(got.len(), 2);
    assert!(got.iter().all(|f| *f != a && *f >= 0));
    assert_eq!(t.shm_read(got[1], 0, 3).unwrap(), b"abc");
    assert!(t.unix_take_fds(s).is_empty());
    for fd in got.into_iter().chain([s, c, l, a]) {
        t.unix_close(fd).unwrap();
    }
    assert_eq!(t.held(), 0);
    let _ = std::fs::remove_file(&path);
}

/// `unsafe` is denied crate-wide (the workspace forbids it; this crate's own
/// lints deny it so ONE module can be sanctioned): the descriptor syscalls
/// std has no safe form of (sendmsg / recvmsg with SCM_RIGHTS, poll, fcntl)
/// live in net_unix_sys.rs alone, every block under a SAFETY line.
#[test]
fn unsafe_lives_in_the_one_sanctioned_module() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let opens = ["unsafe {", "unsafe fn", "unsafe extern", "unsafe impl", "allow(unsafe_code)"]
                .iter()
                .any(|p| code.contains(p));
            if !opens {
                continue;
            }
            assert_eq!(name, "net_unix_sys.rs", "{name}:{}: unsafe outside the sanctioned module", i + 1);
            if code.contains("unsafe {") || code.contains("unsafe extern") {
                let above = lines[..i].iter().rev().take(4).any(|l| l.trim_start().starts_with("// SAFETY:"));
                assert!(above, "{name}:{}: an unsafe block without a SAFETY line", i + 1);
            }
        }
    }
}
