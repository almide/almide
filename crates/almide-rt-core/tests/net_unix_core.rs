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

/// The descriptors this process has open, counted from /dev/fd (both Linux and
/// macOS list one entry per open descriptor there).
fn open_descriptors() -> usize {
    std::fs::read_dir("/dev/fd").map(|d| d.count()).unwrap_or(0)
}

/// #4124: a peer that sends a descriptor with every message cannot fill the
/// program's descriptor table through a socket the program only reads. At
/// most `MAX_FDS` (28) received descriptors wait untaken per socket; the
/// ones that arrive while 28 wait are closed on receipt, never held.
#[test]
fn received_descriptors_waiting_untaken_are_bounded_per_socket() -> Result<(), String> {
    let path = sock_path("flood");
    let p = path.to_str().ok_or("socket path is not UTF-8")?;
    let mut peer = AlmideUnixTables::default();
    let mut t = AlmideUnixTables::default();
    let l = t.unix_listen(p)?;
    let c = peer.unix_connect(p)?;
    let s = t.unix_accept(l)?;
    let a = peer.shm_create(8)?;
    let held_before = t.held();
    let open_before = open_descriptors();
    let mut most = 0usize;
    for _ in 0..300 {
        peer.unix_send(c, b"x", &[a])?;
        assert_eq!(t.unix_recv(s, 1)?, b"x");
        most = most.max(open_descriptors());
    }
    assert_eq!(
        t.held(),
        held_before + 28,
        "the table holds 28 waiting descriptors, no more"
    );
    // Other tests in this binary run on other threads and open a few
    // descriptors of their own: the slack covers them, not the 272 extra.
    assert!(
        most <= open_before + 28 + 16,
        "open descriptors grew from {open_before} to {most}"
    );
    let file = std::env::temp_dir().join(format!("arc-{}-flood.txt", std::process::id()));
    std::fs::write(&file, "still opens").map_err(|e| e.to_string())?;
    assert_eq!(std::fs::read_to_string(&file).map_err(|e| e.to_string())?, "still opens");
    let _ = std::fs::remove_file(&file);
    let got = t.unix_take_fds(s);
    assert_eq!(got.len(), 28);
    // Taken, the queue has room again.
    peer.unix_send(c, b"y", &[a])?;
    assert_eq!(t.unix_recv(s, 1)?, b"y");
    assert_eq!(t.unix_take_fds(s).len(), 1);
    let _ = std::fs::remove_file(&path);
    Ok(())
}

/// #4123: a read's `len` is an upper bound, never an allocation size: 2^62
/// answers the bytes there are instead of aborting the process.
#[test]
fn a_huge_read_length_answers_the_bytes_there_are() -> Result<(), String> {
    let path = sock_path("huge");
    let p = path.to_str().ok_or("socket path is not UTF-8")?;
    let mut t = AlmideUnixTables::default();
    let a = t.shm_create(16)?;
    t.shm_write(a, 0, b"0123456789abcdef")?;
    assert_eq!(t.shm_read(a, 0, 1 << 62)?, b"0123456789abcdef");
    assert_eq!(t.shm_read(a, 4, 1 << 62)?, b"456789abcdef");
    let l = t.unix_listen(p)?;
    let c = t.unix_connect(p)?;
    let s = t.unix_accept(l)?;
    t.unix_send(c, b"0123456789abcdef", &[])?;
    assert_eq!(t.unix_recv(s, 1 << 62)?, b"0123456789abcdef");
    for fd in [s, c, l, a] {
        t.unix_close(fd)?;
    }
    let _ = std::fs::remove_file(&path);
    Ok(())
}

/// `unsafe` is denied crate-wide (the workspace forbids it; this crate's own
/// lints deny it so ONE module can be sanctioned): the descriptor syscalls
/// std has no safe form of (sendmsg / recvmsg with SCM_RIGHTS, poll, fcntl)
/// live in net_unix_sys.rs alone, every block under a SAFETY line.
#[test]
fn unsafe_lives_in_the_one_sanctioned_module() {
    // Every .rs file under src/, subdirectories included (src/generated/
    // holds the inlined snapshots, #3361): a directory is walked, not read.
    let mut files = Vec::new();
    let mut dirs = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    assert!(files.iter().any(|p| p.ends_with("net_unix_sys.rs")), "the walk found no sources");
    for path in files {
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
            assert!(matches!(name.as_str(), "net_unix_sys.rs" | "fs_beneath_sys.rs"), "{name}:{}: unsafe outside the sanctioned modules", i + 1);
            if code.contains("unsafe {") || code.contains("unsafe extern") {
                let above = lines[..i].iter().rev().take(4).any(|l| l.trim_start().starts_with("// SAFETY:"));
                assert!(above, "{name}:{}: an unsafe block without a SAFETY line", i + 1);
            }
        }
    }
}
