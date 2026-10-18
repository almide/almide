//! The http router's per-request work was O(L²) in the request path length L
//! (stdlib inspection, 2026-10-16): native took 0.29 s user for a 404 at
//! 40k characters and 4.7 s at 160k, so one long request tied up a server.
//! `__bad_escape` recursed over `string.chars(path)` with `list.drop(cs, 1)`
//! — a copy of the rest of the path per character — and `__bind_into`
//! dropped one segment off the request's parts per step the same way.
//!
//! Three gates, all through the binary:
//! - both legs answer the same, byte for byte, for long paths, valid and
//!   malformed percent-escapes, many segments, `{name...}` rest params and
//!   a mount — and the answers are the ones the router gave before;
//! - wasm: the bytes requested from `$alloc` (`ALMIDE_WASM_ALLOC_COUNT=1`,
//!   deterministic) grow about 4x when the path grows 4x, not 16x;
//! - native: one request with a path of a few hundred thousand characters
//!   answers inside a budget a quadratic router misses by an order of
//!   magnitude (each shape took 5–30 s quadratic; linear is milliseconds).
use std::process::Command;
use std::time::{Duration, Instant};

const CORRECT: &str = r#"
import http

effect fn get_user(req: HttpRequest) -> HttpResponse = {
  let id = http.param(req, "id") ?? "-"
  http.response(200, "user ${string.len(id)} ${string.take(id, 12)}")
}

effect fn pair(req: HttpRequest) -> HttpResponse = http.response(
  200,
  "pair " + (http.param(req, "a") ?? "-") + " " + (http.param(req, "b") ?? "-"),
)

effect fn serve_file(req: HttpRequest) -> HttpResponse = {
  let p = http.param(req, "path") ?? "-"
  http.response(200, "file ${string.len(p)} ${string.take(p, 12)} ${string.take_end(p, 12)}")
}

effect fn deep(req: HttpRequest) -> HttpResponse = http.response(200, "deep")

effect fn sub(req: HttpRequest) -> HttpResponse = {
  let p = http.req_path(req)
  http.response(200, "sub ${string.len(p)} ${string.take(p, 12)} ${string.take_end(p, 12)}")
}

fn summary(s: String) -> String = "${string.len(s)}:${string.take(s, 16)}"

effect fn main() -> Unit = {
  let app = http.router([
    http.route("GET /users/{id}", get_user),
    http.route("GET /pair/{a}/x/{b}", pair),
    http.route("GET /files/{path...}", serve_file),
    http.route("POST /files/{path...}", serve_file),
    http.route("GET /a/b/c", deep),
    http.mount("/m", sub)
  ])!
  let paths = [
    "/users/42",
    "/users/%41%62c",
    "/users/a%2Fb",
    "/users/a+b",
    "/users/%",
    "/users/%4",
    "/users/%4g",
    "/users/%zz",
    "/users/x%",
    "/users/%%41",
    "/users/%E3%81%82",
    "/users/%e3%81%82",
    "/users/\u{3042}%",
    "/users/%\u{3042}1",
    "/pair/1/x/2",
    "/pair/1/y/2",
    "/pair/%31/x/%32?q=1",
    "/files",
    "/files/",
    "/files/a/b%20c/d",
    "/files/a//b/",
    "/a/b/c",
    "/a/b/c/",
    "/a/b",
    "/a/b/c/d",
    "/m/x/y?k=v",
    "/m",
    "nostart",
    "/" + string.repeat("x", 5000),
    "/users/" + string.repeat("%41", 3000),
    "/users/" + string.repeat("%41", 3000) + "%4",
    "/q" + string.repeat("/s", 4000),
    "/files" + string.repeat("/s%41", 3000),
    "/files" + string.repeat("/s%41", 3000) + "/%G0",
    "/m" + string.repeat("/t", 3000) + "?z=1"
  ]
  for p in paths {
    let res = app(http.new_request("GET", p, "", map.new()))!
    println("${summary(p)} -> ${http.status_code(res)} ${http.body(res)} ${http.get_header(res, "Allow") ?? ""}")
  }
  let head = app(http.new_request("HEAD", "/files" + string.repeat("/h", 2000), "", map.new()))!
  println("HEAD ${http.status_code(head)} [${http.body(head)}]")
  let del = app(http.new_request("DELETE", "/files" + string.repeat("/h", 2000), "", map.new()))!
  println("DELETE ${http.status_code(del)} ${http.get_header(del, "Allow") ?? ""}")
}
"#;

const CORRECT_EXPECTED: &str = r#"
9:/users/42 -> 200 user 2 42 
14:/users/%41%62c -> 200 user 3 Abc 
12:/users/a%2Fb -> 200 user 3 a/b 
10:/users/a+b -> 200 user 3 a+b 
8:/users/% -> 400 Bad Request: malformed percent-encoding in the path 
9:/users/%4 -> 400 Bad Request: malformed percent-encoding in the path 
10:/users/%4g -> 400 Bad Request: malformed percent-encoding in the path 
10:/users/%zz -> 400 Bad Request: malformed percent-encoding in the path 
9:/users/x% -> 400 Bad Request: malformed percent-encoding in the path 
11:/users/%%41 -> 400 Bad Request: malformed percent-encoding in the path 
16:/users/%E3%81%82 -> 200 user 1 あ 
16:/users/%e3%81%82 -> 200 user 1 あ 
9:/users/あ% -> 400 Bad Request: malformed percent-encoding in the path 
10:/users/%あ1 -> 400 Bad Request: malformed percent-encoding in the path 
11:/pair/1/x/2 -> 200 pair 1 2 
11:/pair/1/y/2 -> 404 Not Found 
19:/pair/%31/x/%32? -> 200 pair 1 2 
6:/files -> 200 file 0   
7:/files/ -> 200 file 0   
16:/files/a/b%20c/d -> 200 file 7 a/b c/d a/b c/d 
12:/files/a//b/ -> 200 file 3 a/b a/b 
6:/a/b/c -> 200 deep 
7:/a/b/c/ -> 200 deep 
4:/a/b -> 404 Not Found 
8:/a/b/c/d -> 404 Not Found 
10:/m/x/y?k=v -> 200 sub 8 /x/y?k=v /x/y?k=v 
2:/m -> 200 sub 1 / / 
7:nostart -> 400 Bad Request: the request target must start with / 
5001:/xxxxxxxxxxxxxxx -> 404 Not Found 
9007:/users/%41%41%41 -> 200 user 3000 AAAAAAAAAAAA 
9009:/users/%41%41%41 -> 400 Bad Request: malformed percent-encoding in the path 
8002:/q/s/s/s/s/s/s/s -> 404 Not Found 
15006:/files/s%41/s%41 -> 200 file 8999 sA/sA/sA/sA/ /sA/sA/sA/sA 
15010:/files/s%41/s%41 -> 400 Bad Request: malformed percent-encoding in the path 
6006:/m/t/t/t/t/t/t/t -> 200 sub 6004 /t/t/t/t/t/t /t/t/t/t?z=1 
HEAD 200 []
DELETE 405 GET, HEAD, POST
"#;

/// One request per run: `kind` picks the path's shape, `n` its size.
const SIZED: &str = r#"import env
import http

effect fn get_user(req: HttpRequest) -> HttpResponse = http.response(200, "u ${string.len(http.param(req, "id") ?? "-")}")

effect fn serve_file(req: HttpRequest) -> HttpResponse = http.response(200, "f ${string.len(http.param(req, "path") ?? "-")}")

effect fn deep(req: HttpRequest) -> HttpResponse = http.response(200, "d")

effect fn main() -> Unit = {
  let args = env.args()
  let kind = list.get(args, 0) ?? ""
  let n = int.parse(list.get(args, 1) ?? "0") ?? 0
  let app = http.router([
    http.route("GET /users/{id}", get_user),
    http.route("GET /files/{path...}", serve_file),
    http.route("GET /a/b/c", deep),
  ])!
  let path = match kind {
    "long" => "/x" + string.repeat("a", n),
    "esc" => "/users/" + string.repeat("%41", n),
    "segs" => "/q" + string.repeat("/s", n),
    "rest" => "/files" + string.repeat("/s%41", n),
    _ => "/",
  }
  let res = app(http.new_request("GET", path, "", map.new()))!
  println("${http.status_code(res)} ${http.body(res)}")
}
"#;

/// The shapes, with the answer each gives at size `n`.
fn want(kind: &str, n: u64) -> String {
    match kind {
        // `/users/` with no id is `/users`: no route.
        "long" | "segs" => "404 Not Found\n".to_string(),
        "esc" if n == 0 => "404 Not Found\n".to_string(),
        "esc" => format!("200 u {n}\n"),
        "rest" => format!("200 f {}\n", if n == 0 { 0 } else { 3 * n - 1 }),
        _ => unreachable!(),
    }
}

const SHAPES: [&str; 4] = ["long", "esc", "segs", "rest"];

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

fn run(file: &std::path::Path, wasm: bool) -> String {
    let mut c = almide();
    c.arg("run").arg(file);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let out = c.output().expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_router_answers_the_same_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("router.almd");
    std::fs::write(&file, CORRECT.trim_start()).expect("source");
    let expected = CORRECT_EXPECTED.trim_start();
    assert_eq!(run(&file, false), expected, "native");
    assert_eq!(run(&file, true), expected, "wasm");
}

/// stdout and the bytes the module requested from `$alloc`.
fn wasm_bytes(file: &std::path::Path, kind: &str, n: u64) -> u64 {
    let out = almide()
        .arg("run")
        .arg(file)
        .args(["--target", "wasm", "--", kind, &n.to_string()])
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("wasm run");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), want(kind, n), "{kind} {n}");
    stderr
        .split_whitespace()
        .find_map(|w| w.strip_prefix("bytes=")?.parse().ok())
        .unwrap_or_else(|| panic!("no bytes= in {stderr}"))
}

#[test]
fn the_router_requests_bytes_linear_in_the_path_on_wasm() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("sized.almd");
    std::fs::write(&file, SIZED).expect("source");
    for kind in SHAPES {
        let base = wasm_bytes(&file, kind, 0);
        let small = wasm_bytes(&file, kind, 1000).saturating_sub(base);
        let large = wasm_bytes(&file, kind, 4000).saturating_sub(base);
        // Linear is 4x; the drop-copy recursion measured 15-16x.
        assert!(
            large < small * 6,
            "{kind}: {small} bytes at n=1000, {large} at n=4000 — the router's per-request work is not linear in the path"
        );
    }
}

#[test]
fn the_router_answers_a_long_path_quickly_on_native() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("sized.almd");
    std::fs::write(&file, SIZED).expect("source");
    let exe = dir.path().join("sized");
    let build = almide()
        .arg("build")
        .arg(&file)
        .arg("-o")
        .arg(&exe)
        .arg("--release")
        .output()
        .expect("almide build");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    // Quadratic, these took 30 s, 6 s, 30 s and 10 s; linear, milliseconds.
    for (kind, n) in [("long", 400_000u64), ("esc", 100_000), ("segs", 200_000), ("rest", 60_000)] {
        let mut child = Command::new(&exe)
            .args([kind, &n.to_string()])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("run artifact");
        let t = Instant::now();
        let budget = Duration::from_secs(3);
        while child.try_wait().expect("wait").is_none() {
            if t.elapsed() > budget {
                let _ = child.kill();
                panic!("{kind} at n={n}: no answer within {budget:?} — the router's per-request work is not linear in the path");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let out = child.wait_with_output().expect("output");
        assert!(out.status.success(), "{kind} {n}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), want(kind, n), "{kind} {n}");
    }
}
