//! #4199, native only: the net handle tables are process-wide, and the lock
//! that guards them is never held across a call that waits. Two `fan` arms on
//! their own threads, one blocked in `accept` on `main`'s listener and one
//! connecting to it, must both finish: holding the table lock across the
//! accept would leave the connecting arm waiting on the lock forever. The
//! cross-target transcript (spec/embedded_cross/net_tcp_fan_handles.almd) has
//! no arm waiting on another, because the embedded lane runs arms in turn;
//! this is the shape it cannot carry. Unix-domain sockets take the same rule.
#![cfg(unix)]

use std::process::Command;
use std::time::Duration;

#[path = "support/capped_run.rs"]
mod capped_run;

const PROGRAM: &str = r#"import io
import net
import unix

effect fn serve_tcp(listener: net.TcpListener) -> net.TcpStream = {
  let s = net.tcp_accept(listener)!
  let got = io.read_exact(s, 5)!
  io.write(s, got)!
  s
}

effect fn dial_tcp(port: Int) -> net.TcpStream = {
  let c = net.tcp_connect("127.0.0.1", port)!
  io.write(c, bytes.from_string("hello"))!
  c
}

effect fn serve_unix(listener: unix.Fd) -> unix.Fd = {
  let s = unix.accept(listener)!
  let got = io.read(s, 5)!
  unix.write_fds(s, got, [])!
  s
}

effect fn dial_unix(path: String) -> unix.Fd = {
  let c = unix.connect(path)!
  unix.write_fds(c, bytes.from_string("world"), [])!
  c
}

effect fn main() -> Unit = {
  let l = net.tcp_listen("127.0.0.1", 0)!
  let port = net.tcp_local_port(l)!
  let (srv, cli) = fan { serve_tcp(l), dial_tcp(port) }
  let back = io.read_exact(cli, 5)!
  println("tcp ${srv != cli} ${bytes.to_string(back) ?? "?"}")
  let path = "s.sock"
  let u = unix.listen(path)!
  let (us, uc) = fan { serve_unix(u), dial_unix(path) }
  let uback = io.read(uc, 5)!
  println("unix ${us != uc} ${bytes.to_string(uback) ?? "?"}")
}
"#;

#[test]
fn arms_blocked_in_accept_do_not_hold_up_a_sibling_that_connects() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("fan_net.almd");
    std::fs::write(&file, PROGRAM).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(&file).current_dir(dir.path());
    let out = capped_run::output_capped(cmd, Duration::from_secs(120)).unwrap_or_else(|e| panic!("{e}"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout,
        "tcp true hello\nunix true world\n",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
