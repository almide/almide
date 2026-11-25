//! #4920: a host read is built in the list the program gets.
//!
//! The generated crate splices net_core.rs and net_unix_core.rs flat, where
//! `AlmideList` / `almide_list!` are the runtime's own list (ADR-0037). This
//! test splices them the same way against almide-str's list, so the cores'
//! results are that list itself: the native wrapper hands it on as is, with
//! no `AlmideList::from(Vec)` copy after the read. The crate build (and the
//! embedded wasm host) keeps them as `Vec`.

#[allow(dead_code, unused_imports)]
mod spliced_net {
    use almide_str::{almide_list, AlmideList};
    include!("../src/generated/err_text.rs");
    include!("../src/net_core.rs");
}

#[cfg(unix)]
#[allow(dead_code, unused_imports)]
mod spliced_unix {
    use almide_str::{almide_list, AlmideList};
    include!("../src/generated/err_text.rs");
    include!("../src/net_unix_core.rs");
}

#[test]
fn a_tcp_read_answers_its_own_buffer_as_the_runtime_list() {
    let mut t = spliced_net::AlmideNetTables::default();
    let l = t.tcp_listen("127.0.0.1", 0).expect("listen");
    let port = t.tcp_local_port(l).expect("port");
    let c = t.tcp_connect("127.0.0.1", port).expect("connect");
    let s = t.tcp_accept(l).expect("accept");
    t.tcp_write(c, b"hello").expect("write");
    let got: almide_str::AlmideList<u8> = t.tcp_read(s, 100).expect("read");
    assert!(!got.is_empty() && b"hello".starts_with(&got), "{:?}", &got[..]);
    // The block is the read's buffer, sized for the 100 bytes asked: a copy
    // made after the read would be sized to the bytes that arrived.
    assert_eq!(got.capacity(), 100);
    t.tcp_write(c, b"!").expect("write");
    let rest: almide_str::AlmideList<u8> = t.tcp_read_timeout(s, 64, 5000).expect("read_timeout");
    assert_eq!(rest.capacity(), 64);
}

#[cfg(unix)]
#[test]
fn a_shared_memory_read_and_the_fd_lists_are_the_runtime_list() {
    let mut t = spliced_unix::AlmideUnixTables::default();
    let f = t.shm_create(8).expect("shm_create");
    t.shm_write(f, 2, b"abc").expect("shm_write");
    let got: almide_str::AlmideList<u8> = t.shm_read(f, 2, 3).expect("shm_read");
    assert_eq!(&got[..], b"abc");
    let none: almide_str::AlmideList<i64> = t.unix_take_fds(f);
    assert!(none.is_empty());
    let ready: almide_str::AlmideList<i64> = t.unix_wait(&[f], 0).expect("unix_wait");
    assert!(ready.len() <= 1);
    t.unix_close(f).expect("close");
}
