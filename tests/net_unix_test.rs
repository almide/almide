//! net's Unix-domain sockets and shared-memory files, natively.
//!
//! One program plays both ends: it listens, connects to itself, and passes a
//! shared-memory file across the socket. What must hold is what a Wayland
//! client leans on — the descriptors arrive in order beside the bytes, as new
//! descriptors of the SAME open files (a write through one is read through
//! the other), taken once; poll sees data, emptiness and the peer's close;
//! and misuse is an `err`, not a crash.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn run(name: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almide-net-unix-{name}"));
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("main.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(almide_bin()).arg("run").arg(&file).output().expect("spawn almide");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[cfg(unix)]
#[test]
fn descriptors_cross_the_socket_as_the_same_files() {
    let sock = std::env::temp_dir().join(format!("almide-net-unix-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&sock);
    let src = r#"
import net

effect fn main() -> Unit = {
  let path = "SOCK"
  let l = net.unix_listen(path)!
  let c = net.unix_connect(path)!
  let s = net.unix_accept(l)!
  let a = net.shm_create(64)!
  let b = net.shm_create(8)!
  net.shm_write(a, 10, bytes.from_string("shared"))!
  net.shm_write(b, 0, bytes.from_string("second"))!
  println("idle ${net.unix_poll(s, 0)!}")
  net.unix_send(c, bytes.from_string("hello"), [a, b])!
  println("ready ${net.unix_poll(s, 1000)!}")
  let got = net.unix_recv(s, 100)!
  let fds = net.unix_take_fds(s)
  println("got ${bytes.to_string_lossy(got)} with ${int.to_string(list.len(fds))}")
  println("first ${bytes.to_string_lossy(net.shm_read(fds[0], 10, 6)!)}")
  println("second ${bytes.to_string_lossy(net.shm_read(fds[1], 0, 6)!)}")
  net.shm_write(fds[0], 0, bytes.from_string("back"))!
  println("seen by the sender ${bytes.to_string_lossy(net.shm_read(a, 0, 4)!)}")
  net.shm_resize(fds[0], 4096)!
  println("size ${int.to_string(bytes.len(net.shm_read(a, 0, 10000)!))}")
  println("again ${int.to_string(list.len(net.unix_take_fds(s)))}")
  net.unix_close(c)!
  println("closed ${net.unix_poll(s, 1000)!} ${int.to_string(bytes.len(net.unix_recv(s, 10)!))}")
  for f in fds { net.unix_close(f)! }
  net.unix_close(a)!
  net.unix_close(b)!
  net.unix_close(s)!
  net.unix_close(l)!
}
"#
    .replace("SOCK", sock.to_str().unwrap());
    let out = run("pass", &src);
    let _ = std::fs::remove_file(&sock);
    let want = "idle false\nready true\ngot hello with 2\nfirst shared\nsecond second\n\
                seen by the sender back\nsize 4096\nagain 0\nclosed true 0\n";
    assert!(out.contains(want), "{out}");
}

#[cfg(unix)]
#[test]
fn misuse_is_an_err() {
    let src = r#"
import net

fn show(r: Result[Int, String]) -> String = match r { ok(_) => "ok", err(e) => e }

effect fn main() -> Unit = {
  println(show(net.unix_connect("/nonexistent/almide.sock")))
  println(show(net.shm_create(0 - 1)))
  println(match net.shm_write(0 - 3, 0, bytes.from_string("x")) { ok(_) => "ok", err(e) => e })
  println(match net.unix_send(0 - 1, bytes.from_string("x"), []) { ok(_) => "ok", err(e) => e })
  let many = 0..<29 |> list.map((_) => 0)
  let f = net.shm_create(1)!
  println(match net.unix_send(f, bytes.from_string("x"), many) { ok(_) => "ok", err(e) => e })
}
"#;
    let out = run("misuse", src);
    for want in [
        "unix_connect(/nonexistent/almide.sock): No such file or directory",
        "shm_create: negative size -1",
        "shm_write: not a file descriptor: -3",
        "unix_send: not a file descriptor: -1",
        "unix_send: 29 descriptors in one message (at most 28)",
    ] {
        assert!(out.contains(want), "missing {want:?} in:\n{out}");
    }
}
