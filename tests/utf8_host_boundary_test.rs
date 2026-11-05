//! UTF-8 at the host boundary (#4005 A4): the evidence of the [[op]] rows of
//! proofs/utf8-producers.toml.
//!
//! Every String the host hands the guest (an environment value, a file's text,
//! a directory's names, a child's output, a response body) is a String block
//! the wasm leg's readers step through by lead-byte widths. Each probe here puts
//! bytes no UTF-8 holds where the host reads them, runs one program on every
//! leg this machine has, and has the GUEST check what it received: each probe
//! line is `<label> <bytes.is_valid_utf8 of the String it got>`, so a leg that
//! hands over raw bytes prints `false` (and its stdout is not UTF-8 either).
//!
//! The legs whose rows are `open` (the stock preview-1 shim's environment,
//! argv and directory names) are not run here: they fail today, and the fix
//! that closes each row adds its leg to these probes.
#![cfg(target_os = "linux")]

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

/// Bytes no UTF-8 holds: FF, C0, a lone surrogate (ED A0 80) and a lead byte
/// cut off by the end (E2 82).
const BAD: &[u8] = b"a\xffb\xc0z\xed\xa0\x80q\xe2\x82";

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn bad() -> &'static OsStr {
    OsStr::from_bytes(BAD)
}

/// The major version of the wasmtime on PATH, 0 when there is none.
fn wasmtime_major() -> u32 {
    let Ok(o) = Command::new("wasmtime").arg("--version").output() else {
        return 0;
    };
    let v = String::from_utf8_lossy(&o.stdout).to_string();
    v.split_whitespace().nth(1).and_then(|x| x.split('.').next()).and_then(|m| m.parse().ok()).unwrap_or(0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Leg {
    Native,
    Embedded,
    /// `almide build --target wasm` on stock wasmtime: the preview-1 shims.
    Stock,
    /// The WASI 0.3 component on stock wasmtime.
    P3,
}

struct Probe<'a> {
    src: &'a Path,
    cwd: &'a Path,
    envs: Vec<(&'a OsStr, &'a OsStr)>,
    args: Vec<&'a OsStr>,
}

/// Run the probe on `leg`; `None` when this machine lacks the leg's runtime.
fn run(p: &Probe<'_>, leg: Leg, scratch: &Path) -> Option<Output> {
    let mut c = match leg {
        Leg::Native | Leg::Embedded => {
            let mut c = Command::new(almide());
            c.arg("run").arg(p.src);
            if leg == Leg::Embedded {
                c.args(["--target", "wasm"]);
            }
            if !p.args.is_empty() {
                c.arg("--").args(&p.args);
            }
            c
        }
        Leg::Stock | Leg::P3 => {
            if wasmtime_major() < if leg == Leg::P3 { 46 } else { 1 } {
                eprintln!("SKIP {leg:?}: no wasmtime on PATH that runs it");
                return None;
            }
            let wasm = scratch.join(format!("probe-{leg:?}.wasm"));
            let mut b = Command::new(almide());
            b.arg("build").arg(p.src).args(["--target", "wasm"]);
            if leg == Leg::P3 {
                b.arg("--component").env("ALMIDE_COMPONENT_P3", "1");
            }
            let built = b.arg("-o").arg(&wasm).output().expect("almide build");
            assert!(built.status.success(), "{leg:?} build failed:\n{}", String::from_utf8_lossy(&built.stderr));
            let mut c = Command::new("wasmtime");
            c.arg("run");
            if leg == Leg::P3 {
                c.args(["--dir=/", "-S", "inherit-env=y"]);
            } else {
                c.arg("--dir=.");
            }
            c.arg(&wasm).args(&p.args);
            c
        }
    };
    c.current_dir(p.cwd).stdin(Stdio::null());
    for (k, v) in &p.envs {
        c.env(k, v);
    }
    Some(c.output().expect("spawn"))
}

/// The probe's verdict lines on `leg`: every one `<label> true`, and every
/// label of `labels` among them; panics with the leg's output otherwise.
fn assert_valid(leg: Leg, out: &Output, labels: &[&str]) {
    let stdout = String::from_utf8(out.stdout.clone()).unwrap_or_else(|_| {
        panic!("{leg:?}: the guest printed bytes that are not UTF-8: {:?}", String::from_utf8_lossy(&out.stdout))
    });
    assert!(
        out.status.success(),
        "{leg:?}: exit {:?}\nstdout:\n{stdout}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<&str> = stdout.lines().collect();
    for label in labels {
        assert!(lines.iter().any(|l| l.split(' ').next() == Some(*label)), "{leg:?}: no `{label}` verdict:\n{stdout}");
    }
    for l in lines {
        assert!(l.ends_with(" true"), "{leg:?}: a String the guest received is not valid UTF-8: {l}\n{stdout}");
    }
}

fn write(dir: &Path, name: &str, src: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, src).expect("write probe");
    p
}

/// The helpers every probe prints through: a String's verdict, a Result's.
const SHOW: &str = r#"
fn v(label: String, s: String) -> Unit = println("${label} ${bytes.is_valid_utf8(bytes.from_string(s))}")

fn vr(label: String, r: Result[String, String]) -> Unit = match r {
  ok(s) => v(label, s),
  err(e) => v(label, e),
}

fn vl(label: String, r: Result[List[String], String]) -> Unit = match r {
  ok(xs) => v(label, xs |> list.join("/")),
  err(e) => v(label, e),
}
"#;

/// env.get / env.vars / env.cwd / fs.temp_dir with an environment value, the
/// working directory's name and TMPDIR all holding invalid bytes (ops 26, 77,
/// 33, 28).
#[test]
fn environment_text_reaches_the_guest_valid() {
    let t = tempfile::tempdir().expect("tempdir");
    let cwd = t.path().join(bad());
    std::fs::create_dir(&cwd).expect("a directory with an invalid name");
    let src = write(
        t.path(),
        "env_probe.almd",
        &format!(
            r#"import env
import fs
{SHOW}
effect fn main() -> Unit = {{
  v("get", env.get("U8_PROBE") ?? "")
  v("vars", env.vars()! |> map.to_list |> list.map(((k, x)) => k + "=" + x) |> list.join(";"))
  vr("cwd", env.cwd())
  v("temp", fs.temp_dir())
}}
"#
        ),
    );
    let probe = Probe {
        src: &src,
        cwd: &cwd,
        envs: vec![(OsStr::new("U8_PROBE"), bad()), (OsStr::from_bytes(b"U8_\xffKEY"), bad()), (OsStr::new("TMPDIR"), cwd.as_os_str()), (OsStr::new("PWD"), cwd.as_os_str())],
        args: vec![],
    };
    for leg in [Leg::Native, Leg::Embedded] {
        if let Some(out) = run(&probe, leg, t.path()) {
            assert_valid(leg, &out, &["get", "vars", "cwd", "temp"]);
        }
    }
}

/// env.args with an argument holding invalid bytes (op 29): the CLI may refuse
/// the argument, but whatever reaches the guest is valid.
#[test]
fn argv_reaches_the_guest_valid_or_not_at_all() {
    let t = tempfile::tempdir().expect("tempdir");
    let src = write(
        t.path(),
        "args_probe.almd",
        &format!(
            r#"import env
{SHOW}
effect fn main() -> Unit = {{
  v("args", env.args() |> list.join(" "))
}}
"#
        ),
    );
    let probe = Probe { src: &src, cwd: t.path(), envs: vec![], args: vec![bad(), OsStr::new("ok")] };
    for leg in [Leg::Native, Leg::Embedded] {
        let Some(out) = run(&probe, leg, t.path()) else { continue };
        let stdout = String::from_utf8(out.stdout.clone()).unwrap_or_else(|_| panic!("{leg:?}: stdout is not UTF-8"));
        if out.status.success() {
            assert_valid(leg, &out, &["args"]);
        } else {
            assert!(stdout.is_empty(), "{leg:?}: a refused argv still ran the program:\n{stdout}");
        }
    }
}

/// fs.list_dir / walk / glob / root_list_dir and the temp creators over a
/// directory holding a file whose name is invalid UTF-8 (ops 11, 23, 39, 122,
/// 10, 21).
#[test]
fn directory_names_reach_the_guest_valid() {
    let t = tempfile::tempdir().expect("tempdir");
    let d = t.path().join("d");
    std::fs::create_dir_all(d.join("sub")).expect("mkdir");
    let mut name = BAD.to_vec();
    name.extend_from_slice(b".txt");
    std::fs::write(d.join(OsStr::from_bytes(&name)), b"x").expect("a file with an invalid name");
    std::fs::write(d.join("sub").join(OsStr::from_bytes(&name)), b"x").expect("a nested one");
    let src = write(
        t.path(),
        "names_probe.almd",
        &format!(
            r#"import fs
{SHOW}
effect fn main() -> Unit = {{
  vl("list", fs.list_dir("d"))
  vl("walk", fs.walk("d"))
  vl("glob", fs.glob("d/*"))
  match fs.open_root("d") {{
    ok(r) => vl("root_list", fs.root_list_dir(r, "sub")),
    err(e) => v("root_list", e),
  }}
  vr("temp_dir", fs.create_temp_dir("u8"))
  vr("temp_file", fs.create_temp_file("u8"))
}}
"#
        ),
    );
    let probe = Probe { src: &src, cwd: t.path(), envs: vec![], args: vec![] };
    for leg in [Leg::Native, Leg::Embedded, Leg::P3] {
        if let Some(out) = run(&probe, leg, t.path()) {
            assert_valid(leg, &out, &["list", "walk", "glob", "root_list", "temp_dir", "temp_file"]);
        }
    }
}

/// Every text reader over a file holding invalid bytes (ops 1, 12, 13, 24, 51,
/// 52, 61, 62, 120): an err, or valid text, never the raw bytes.
#[test]
fn file_text_reaches_the_guest_valid() {
    let t = tempfile::tempdir().expect("tempdir");
    let mut body = b"good line\n".to_vec();
    body.extend_from_slice(BAD);
    body.extend_from_slice(b"\nlast\n");
    std::fs::write(t.path().join("bad.txt"), &body).expect("write");
    let src = write(
        t.path(),
        "text_probe.almd",
        &format!(
            r#"import fs
{SHOW}
effect fn main() -> Unit = {{
  vr("read_text", fs.read_text("bad.txt"))
  vl("read_lines", fs.read_lines("bad.txt"))
  match fs.read_text_if_exists("bad.txt") {{
    ok(t) => v("read_text_if_exists", t ?? ""),
    err(e) => v("read_text_if_exists", e),
  }}
  match fs.read_lines_if_exists("bad.txt") {{
    ok(ls) => v("read_lines_if_exists", (ls ?? []) |> list.join("/")),
    err(e) => v("read_lines_if_exists", e),
  }}
  let f: Result[String, String] = fs.fold_lines("bad.txt", "", (acc, l) => acc + l)
  vr("fold_lines", f)
  let r: Result[String, String] = fs.fold_lines_range("bad.txt", 0, 3, "", (acc, l) => acc + l)
  vr("fold_lines_range", r)
  let c: Result[List[String], String] = fs.fold_lines_chunked("bad.txt", 2, "", (acc, l) => acc + l)
  vl("fold_lines_chunked", c)
  match fs.for_each_line("bad.txt", (l) => v("for_each_line", l)) {{
    ok(_) => v("for_each_line", ""),
    err(e) => v("for_each_line", e),
  }}
  match fs.open_root(".") {{
    ok(root) => vr("root_read_text", fs.root_read_text(root, "bad.txt")),
    err(e) => v("root_read_text", e),
  }}
}}
"#
        ),
    );
    let probe = Probe { src: &src, cwd: t.path(), envs: vec![], args: vec![] };
    for leg in [Leg::Native, Leg::Embedded, Leg::Stock, Leg::P3] {
        if let Some(out) = run(&probe, leg, t.path()) {
            assert_valid(leg, &out, &["read_text", "read_lines", "read_text_if_exists", "read_lines_if_exists", "fold_lines", "fold_lines_range", "fold_lines_chunked", "for_each_line", "root_read_text"]);
        }
    }
}

/// A child process that writes invalid bytes (ops 80..=90): process.exec,
/// output, and the streaming readers of a started child.
#[test]
fn child_output_reaches_the_guest_valid() {
    let t = tempfile::tempdir().expect("tempdir");
    let src = write(
        t.path(),
        "proc_probe.almd",
        &format!(
            r#"import process
{SHOW}
fn cmd() -> process.ProcessCommand = process.ProcessCommand {{ cmd: "sh", args: ["-c", "cat bad.bin; cat bad.bin >&2"] }}

effect fn main() -> Unit = {{
  vr("exec", process.exec("sh", ["-c", "cat bad.bin"]))
  match process.output(cmd()) {{
    ok(st) => {{
      v("output_stdout", st.stdout)
      v("output_stderr", st.stderr)
    }},
    err(e) => {{
      v("output_stdout", e)
      v("output_stderr", e)
    }},
  }}
  match process.start(cmd()) {{
    ok(child) => {{
      match process.wait(child) {{
        ok(st) => v("wait", st.stdout + st.stderr),
        err(e) => v("wait", e),
      }}
      v("read_new", process.read_new_stdout(child)! + process.read_new_stderr(child)!)
      match process.stop(child, 100) {{
        ok(st) => v("stop", st.stdout + st.stderr),
        err(e) => v("stop", e),
      }}
    }},
    err(e) => {{
      v("wait", e)
      v("read_new", e)
      v("stop", e)
    }},
  }}
}}
"#
        ),
    );
    std::fs::write(t.path().join("bad.bin"), BAD).expect("write");
    let probe = Probe { src: &src, cwd: t.path(), envs: vec![], args: vec![] };
    for leg in [Leg::Native, Leg::Embedded] {
        if let Some(out) = run(&probe, leg, t.path()) {
            assert_valid(leg, &out, &["exec", "output_stdout", "output_stderr", "wait", "read_new", "stop"]);
        }
    }
}

/// Serve `BAD` as the body of every reply on a loopback port.
fn serve_bad() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            std::thread::spawn(move || {
                let mut s: TcpStream = s;
                let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
                let mut buf = [0u8; 8192];
                let mut got = Vec::new();
                while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => got.extend_from_slice(&buf[..n]),
                    }
                }
                let mut out = b"HTTP/1.1 200 OK\r\nX-Bad: ".to_vec();
                out.extend_from_slice(BAD);
                out.extend_from_slice(format!("\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", BAD.len()).as_bytes());
                out.extend_from_slice(BAD);
                let _ = s.write_all(&out);
            });
        }
    });
    port
}

/// A response whose body and header hold invalid bytes (ops 43..=49, 53..=59,
/// 65): the text clients, the call handle, the full response.
#[test]
fn response_text_reaches_the_guest_valid() {
    let port = serve_bad();
    let base = format!("http://127.0.0.1:{port}");
    let t = tempfile::tempdir().expect("tempdir");
    let src = write(
        t.path(),
        "http_probe.almd",
        &format!(
            r#"import http
{SHOW}
effect fn main() -> Unit = {{
  vr("get", http.get("{base}/a"))
  vr("request", http.request("POST", "{base}/b", "x", []))
  match http.start(http.new_request("GET", "{base}/c", "", []), {{ total_ms: 10000, idle_ms: 10000 }}) {{
    ok(call) => {{
      match http.wait(call) {{
        ok(resp) => v("wait", http.body(resp) + (http.header(resp, "x-bad") ?? "")),
        err(e) => v("wait", e),
      }}
      v("read_new", http.read_new(call))
    }},
    err(e) => {{
      v("wait", e)
      v("read_new", e)
    }},
  }}
}}
"#
        ),
    );
    let probe = Probe { src: &src, cwd: t.path(), envs: vec![], args: vec![] };
    for leg in [Leg::Native, Leg::Embedded] {
        if let Some(out) = run(&probe, leg, t.path()) {
            assert_valid(leg, &out, &["get", "request", "wait", "read_new"]);
        }
    }
}
