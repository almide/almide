//! The tcp family on native answers from the shared core (#3620, the net/tcp
//! design §4): every failure is the class of the failure, never the
//! platform's errno text; a negative length is an err, not a panic; a short
//! `io.read_exact` names how many bytes came (the negative-length and
//! short-read texts are io's, shared by every stream); `read_timeout` refuses a timeout
//! that is not positive and puts the stream's own timeout back; a listener
//! on port 0 reports the port the OS chose. The embedded and p3 legs reuse
//! these texts, so this pins the side they copy. The handles are opaque
//! (`net.TcpStream`, `net.TcpListener`), so neither their numbering nor a
//! handle that was never issued can be written here any more.
use std::process::Command;
use std::time::Duration;

#[path = "support/capped_run.rs"]
mod capped_run;

const PROGRAM: &str = r#"import io
import net

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
  println(show("negative", io.read(c, -1)))
  println(show("negative exact", io.read_exact(c, -3)))
  println(show("zero ms", net.tcp_read_timeout(c, 4, 0)))
  println(show("minus ms", net.tcp_read_timeout(c, 4, -5)))
  println(show("read_timeout", net.tcp_read_timeout(c, 4, 30)))
  net.tcp_set_timeout(c, 40)!
  println(show("read_timeout again", net.tcp_read_timeout(c, 4, 20)))
  println(show("own timeout back", io.read(c, 4)))
  io.write(p, bytes.from_string("abc"))!
  io.close(p)!
  println(show("short exact", io.read_exact(c, 5)))
  println(show("eof", io.read(c, 4)))
  io.close(c)!
  io.close(c)!
  println(show("closed", io.read(c, 4)))
  io.close(l)!
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
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(&file).current_dir(dir.path());
    // Seconds normally; a hang fails by name instead of holding the job.
    let out = capped_run::output_capped(cmd, Duration::from_secs(120)).unwrap_or_else(|e| panic!("native tcp program: {e}"));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "stdout:\n{stdout}\nstderr:\n{}", String::from_utf8_lossy(&out.stderr));
    // The refused connect names the port the OS chose, which varies per run.
    let port = stdout.lines().find_map(|l| l.strip_prefix("refused: tcp_connect(127.0.0.1:")).and_then(|r| r.split(')').next()).unwrap_or("?").to_string();
    let expected = [
        "port>0: true".to_string(),
        "negative: err io.read: negative length -1".to_string(),
        "negative exact: err io.read_exact: negative length -3".to_string(),
        "zero ms: err tcp_read_timeout: timeout must be positive (got 0 ms)".to_string(),
        "minus ms: err tcp_read_timeout: timeout must be positive (got -5 ms)".to_string(),
        "read_timeout: err tcp_read_timeout: timed out".to_string(),
        "read_timeout again: err tcp_read_timeout: timed out".to_string(),
        "own timeout back: err tcp_read: timed out".to_string(),
        "short exact: err io.read_exact: the stream ended after 3 of 5 bytes".to_string(),
        "eof: ok 0".to_string(),
        "closed: err TcpStream handle 0 is closed".to_string(),
        format!("refused: tcp_connect(127.0.0.1:{port}): connection refused"),
        "range: tcp_connect(127.0.0.1:70000): port out of range 0..=65535".to_string(),
        "closed listener: TcpListener handle 0 is closed".to_string(),
    ];
    assert_eq!(stdout.lines().collect::<Vec<_>>(), expected.iter().map(String::as_str).collect::<Vec<_>>(), "full stdout:\n{stdout}");
}
