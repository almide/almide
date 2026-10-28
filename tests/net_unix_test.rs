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
import unix
import io

effect fn main() -> Unit = {
  let path = "SOCK"
  let l = unix.listen(path)!
  let c = unix.connect(path)!
  let s = unix.accept(l)!
  let a = unix.shm_create(64)!
  let b = unix.shm_create(8)!
  unix.shm_write(a, 10, bytes.from_string("shared"))!
  unix.shm_write(b, 0, bytes.from_string("second"))!
  println("idle ${(unix.select([s], 0) |> result.map((xs) => list.len(xs) > 0))!}")
  unix.write_fds(c, bytes.from_string("hello"), [a, b])!
  println("ready ${(unix.select([s], 1000) |> result.map((xs) => list.len(xs) > 0))!}")
  let other = unix.connect(path)!
  let peer = unix.accept(l)!
  let ready = unix.select([peer, s], 1000)!
  println("wait ${int.to_string(list.len(ready))} ${if ready[0] == s then "s" else "peer"}")
  println("wait none ${int.to_string(list.len(unix.select([peer], 0)!))}")
  io.close(other)!
  io.close(peer)!
  let got = io.read(s, 100)!
  let fds = unix.take_fds(s)!
  println("got ${bytes.to_string_lossy(got)} with ${int.to_string(list.len(fds))}")
  println("first ${bytes.to_string_lossy(unix.shm_read(fds[0], 10, 6)!)}")
  println("second ${bytes.to_string_lossy(unix.shm_read(fds[1], 0, 6)!)}")
  unix.shm_write(fds[0], 0, bytes.from_string("back"))!
  println("seen by the sender ${bytes.to_string_lossy(unix.shm_read(a, 0, 4)!)}")
  unix.shm_resize(fds[0], 4096)!
  println("size ${int.to_string(bytes.len(unix.shm_read(a, 0, 10000)!))}")
  println("again ${int.to_string(list.len(unix.take_fds(s)!))}")
  io.close(c)!
  println("closed ${(unix.select([s], 1000) |> result.map((xs) => list.len(xs) > 0))!} ${int.to_string(bytes.len(io.read(s, 10)!))}")
  for f in fds { io.close(f)! }
  io.close(a)!
  io.close(b)!
  io.close(s)!
  io.close(l)!
}
"#
    .replace("SOCK", sock.to_str().unwrap());
    let out = run("pass", &src);
    let _ = std::fs::remove_file(&sock);
    let want = "idle false\nready true\nwait 1 s\nwait none 0\ngot hello with 2\nfirst shared\nsecond second\n\
                seen by the sender back\nsize 4096\nagain 0\nclosed true 0\n";
    assert!(out.contains(want), "{out}");
}

#[cfg(unix)]
#[test]
fn misuse_is_an_err() {
    // A descriptor that was never issued (`-3`, `-1`) is no longer a probe:
    // `unix.Fd` is opaque, so the program cannot name one.
    let src = r#"
import unix

fn show(r: Result[unix.Fd, String]) -> String = match r { ok(_) => "ok", err(e) => e }

effect fn main() -> Unit = {
  println(show(unix.connect("/nonexistent/almide.sock")))
  println(show(unix.shm_create(0 - 1)))
  let f = unix.shm_create(1)!
  let many = 0..<29 |> list.map((_) => f)
  println(match unix.write_fds(f, bytes.from_string("x"), many) { ok(_) => "ok", err(e) => e })
}
"#;
    let out = run("misuse", src);
    for want in [
        "unix_connect(/nonexistent/almide.sock): No such file or directory",
        "shm_create: negative size -1",
        "unix_send: 29 descriptors in one message (at most 28)",
    ] {
        assert!(out.contains(want), "missing {want:?} in:\n{out}");
    }
}
