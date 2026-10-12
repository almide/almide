//! The tcp family on native answers from the shared core (#3620, the net/tcp
//! design §4): every failure is the class of the failure, never the
//! platform's errno text; a negative length is an err, not a panic; a short
//! `read_exact` names how many bytes came; `read_timeout` refuses a timeout
//! that is not positive and puts the stream's own timeout back; a listener
//! on port 0 reports the port the OS chose. The embedded and p3 legs reuse
//! these texts, so this pins the side they copy.
use std::process::Command;

const PROGRAM: &str = r#"import net

fn show(label: String, r: Result[Bytes, String]) -> String =
  match r {
    ok(b) => "${label}: ok ${bytes.len(b)}",
    err(e) => "${label}: err ${e}",
  }

effect fn main() -> Unit = {
  let l = net.tcp_listen("127.0.0.1", 0)!
  let port = net.tcp_local_port(l)!
  println("port>0: ${port > 0}")
  let c = net.tcp_connect("127.0.0.1", port)!
  let p = net.tcp_accept(l)!
  println("handles: ${l} ${c} ${p}")
  println(show("negative", net.tcp_read(c, -1)))
  println(show("negative exact", net.tcp_read_exact(c, -3)))
  println(show("zero ms", net.tcp_read_timeout(c, 4, 0)))
  println(show("minus ms", net.tcp_read_timeout(c, 4, -5)))
  println(show("read_timeout", net.tcp_read_timeout(c, 4, 30)))
  net.tcp_set_timeout(c, 40)!
  println(show("read_timeout again", net.tcp_read_timeout(c, 4, 20)))
  println(show("own timeout back", net.tcp_read(c, 4)))
  net.tcp_write(p, bytes.from_string("abc"))!
  net.tcp_close(p)!
  println(show("short exact", net.tcp_read_exact(c, 5)))
  println(show("eof", net.tcp_read(c, 4)))
  net.tcp_close(c)!
  net.tcp_close(c)!
  println(show("closed", net.tcp_read(c, 4)))
  println(show("never issued", net.tcp_read(7, 4)))
  net.tcp_close_listener(l)!
  match net.tcp_connect("127.0.0.1", port) {
    ok(_) => println("refused: ok"),
    err(e) => println("refused: ${e}"),
  }
  match net.tcp_connect("127.0.0.1", 70000) {
    ok(_) => println("range: ok"),
    err(e) => println("range: ${e}"),
  }
  match net.tcp_local_port(l) {
    ok(_) => println("closed listener: ok"),
    err(e) => println("closed listener: ${e}"),
  }
}
"#;

#[test]
fn native_tcp_answers_from_the_shared_core() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("tcp.almd");
    std::fs::write(&file, PROGRAM).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .current_dir(dir.path())
        .output()
        .expect("almide run");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{}", String::from_utf8_lossy(&out.stderr));
    // The refused connect names the port the OS chose, which varies per run.
    let port = stdout.lines().find_map(|l| l.strip_prefix("refused: tcp_connect(127.0.0.1:")).and_then(|r| r.split(')').next()).unwrap_or("?").to_string();
    let expected = [
        "port>0: true".to_string(),
        "handles: 0 0 1".to_string(),
        "negative: err tcp_read: negative length -1".to_string(),
        "negative exact: err tcp_read_exact: negative length -3".to_string(),
        "zero ms: err tcp_read_timeout: timeout must be positive (got 0 ms)".to_string(),
        "minus ms: err tcp_read_timeout: timeout must be positive (got -5 ms)".to_string(),
        "read_timeout: err tcp_read_timeout: timed out".to_string(),
        "read_timeout again: err tcp_read_timeout: timed out".to_string(),
        "own timeout back: err tcp_read: timed out".to_string(),
        "short exact: err tcp_read_exact: connection closed after 3 of 5 bytes".to_string(),
        "eof: ok 0".to_string(),
        "closed: err TcpStream handle 0 is closed".to_string(),
        "never issued: err invalid TcpStream handle: 7".to_string(),
        format!("refused: tcp_connect(127.0.0.1:{port}): connection refused"),
        "range: tcp_connect(127.0.0.1:70000): port out of range 0..=65535".to_string(),
        "closed listener: TcpListener handle 0 is closed".to_string(),
    ];
    assert_eq!(stdout.lines().collect::<Vec<_>>(), expected.iter().map(String::as_str).collect::<Vec<_>>(), "full stdout:\n{stdout}");
}
