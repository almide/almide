//! Row 22 (mass assignment, CWE-915) — the input-type gate and its negative
//! control.
//!
//! `http.decode_json[T]` is an `@input_sink`: `T` must be an input type, a
//! `: Decode` record that is not also `Encode` (C-436). The over-post shape —
//! the request body decoded straight into `User: Codec`, so `{"is_admin":
//! true}` makes an admin — is refused at check time (E110). The fixture
//! `spec/wasm_cross/http_decode_json_input_type.almd` pins the safe shape's
//! over-post cell on the native and the wasm leg: the body decodes into
//! `UserInput`, and `is_admin` is `400 Bad Request: unknown key 'is_admin'`.
//!
//! The negative control (`ALMIDE_CONTROL_ADMIT_CODEC_AT_INPUT=1`, the sink
//! admitting a type that is also sent out) must flip the over-post program
//! from E110 to running with `is_admin=true` — proving the refusal is what
//! stands between the request body and the field, not a constant.
//!
//! The flow half (#4232): a handler that skips the sink and parses the body
//! itself, `User.decode(json.parse(http.req_body(req))!)`, is E110 too — the
//! request data is followed to the decode (`almide_frontend::request_flow`).
//! Its control (`ALMIDE_CONTROL_ADMIT_HAND_DECODE=1`) must flip that program
//! to a run that prints `201 m is_admin=true`.
//!
//! Nothing here skips: the binary is the one cargo built for this test
//! (`CARGO_BIN_EXE_almide`), and the wasm leg runs on the host embedded in it
//! (`almide run --target wasm`), so a missing tool is a failure, not a pass.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// `almide <cmd> <src>` with optional extra args and env; (success, stdout, stderr).
fn almide(cmd: &str, src: &Path, extra: &[&str], env: Option<(&str, &str)>) -> (bool, String, String) {
    let mut c = Command::new(almide_bin());
    c.arg(cmd).arg(src).args(extra);
    if let Some((k, v)) = env {
        c.env(k, v);
    }
    let out = c.output().expect("failed to spawn almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

const FIXTURE: &str = "spec/wasm_cross/http_decode_json_input_type.almd";

/// The safe shape's over-post cell: the second body posts `is_admin`.
const OVER_POST_CELL: &str = "HttpResponse { status: 400, body: \"Bad Request: unknown key 'is_admin'\", headers: 1 }";

/// The sink's strictness at depth: a nested record, a list element and a
/// variant payload each refuse an unknown key, naming where it was.
const DEEP_CELLS: &[&str] = &[
    "HttpResponse { status: 400, body: \"Bad Request: unknown key 'verified', at address\", headers: 1 }",
    "HttpResponse { status: 400, body: \"Bad Request: unknown key 'is_admin', at tags[1]\", headers: 1 }",
    "HttpResponse { status: 400, body: \"Bad Request: unknown key 'is_admin', at Rename\", headers: 1 }",
    "HttpResponse { status: 400, body: \"Bad Request: unknown key 'verified', at Pin[0]\", headers: 1 }",
];

/// The vulnerable shape: the body decoded straight into the type the
/// program also sends out.
const OVER_POST_PROGRAM: &str = r#"import http

type User: Codec = { name: String, email: String, is_admin: Bool = false }

effect fn create(req: HttpRequest) -> HttpResponse = match http.decode_json[User](req) {
  ok(u) => http.response(201, "${u.name} is_admin=${u.is_admin}"),
  err(bad) => bad,
}

effect fn main() -> Unit = {
  let res = create(http.new_request("POST", "/users", "{\"name\":\"m\",\"email\":\"e\",\"is_admin\":true}", []))!
  println("${http.status(res)} ${http.body(res)}")
}
"#;

fn write_program(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-input-sink-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn over_post_program() -> PathBuf {
    write_program("over_post.almd", OVER_POST_PROGRAM)
}

/// The bypass (#4232): no sink call, the body parsed and decoded by hand
/// into the type the program also sends out.
const HAND_DECODE_PROGRAM: &str = r#"import http
import json

type User: Codec = { name: String, email: String, is_admin: Bool = false }

effect fn create(req: HttpRequest) -> HttpResponse = {
  let u = User.decode(json.parse(http.req_body(req))!)!
  http.response(201, "${u.name} is_admin=${u.is_admin}")
}

effect fn main() -> Unit = {
  let res = create(http.new_request("POST", "/users", "{\"name\":\"m\",\"email\":\"e\",\"is_admin\":true}", [:]))!
  println("${http.status_code(res)} ${http.body(res)}")
}
"#;

/// The same bypass laundered through bindings, a list, a lambda and a pure
/// helper before the decode.
const LAUNDERED_PROGRAM: &str = r#"import http
import json

type User: Codec = { name: String, email: String, is_admin: Bool = false }

fn texts(t: String) -> List[String] = [t]

effect fn create(req: HttpRequest) -> HttpResponse = {
  let raw = http.req_body(req)
  let users = texts(raw) |> list.map((t) => User.decode(json.parse(t) ?? json.null()))
  http.response(201, "${list.len(users)}")
}

effect fn main() -> Unit = {
  let res = create(http.new_request("POST", "/users", "{}", [:]))!
  println("${http.status_code(res)} ${http.body(res)}")
}
"#;

#[test]
fn the_fixture_refuses_the_over_post_on_both_legs() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let (ok, out, err) = almide("run", &path, &[], None);
    assert!(ok, "native run failed:\n{err}");
    assert!(out.lines().any(|l| l == OVER_POST_CELL), "native: missing `{OVER_POST_CELL}`:\n{out}");
    assert!(out.lines().next().is_some_and(|l| l == "ok m e is_admin=false city=- tags=0"), "native: the accepted body:\n{out}");
    for deep in DEEP_CELLS {
        assert!(out.lines().any(|l| l == *deep), "native: missing nested refusal `{deep}`:\n{out}");
    }
    let (ok, wout, err) = almide("run", &path, &["--target", "wasm"], None);
    assert!(ok, "wasm run failed:\n{err}");
    assert_eq!(wout, out, "the legs disagree");
}

#[test]
fn the_over_post_shape_is_e110() {
    let path = over_post_program();
    let (ok, _, err) = almide("check", &path, &[], None);
    assert!(!ok, "the over-post program checked");
    assert!(err.contains("error[E110]"), "expected E110:\n{err}");
    assert!(err.contains("type UserInput: Decode = { name: String, email: String }"), "the hint names the input type:\n{err}");
}

/// Negative control: with the sink told to admit a `Codec` type, the
/// over-post program compiles and the client sets `is_admin`. If it still
/// fails to compile, the E110 assertion above is not reading the input-type
/// gate and proves nothing.
#[test]
fn admitting_codec_at_the_sink_lets_the_over_post_land() {
    let path = over_post_program();
    let control = Some(("ALMIDE_CONTROL_ADMIT_CODEC_AT_INPUT", "1"));
    let (ok, out, err) = almide("run", &path, &[], control);
    assert!(ok, "control run failed:\n{err}");
    assert_eq!(out.trim_end(), "201 m is_admin=true", "control should land the over-post");
}

#[test]
fn the_hand_decoded_body_is_e110_on_check_and_run() {
    let path = write_program("hand_decode.almd", HAND_DECODE_PROGRAM);
    let (ok, _, err) = almide("check", &path, &[], None);
    assert!(!ok, "the hand-decode program checked");
    assert!(err.contains("error[E110]"), "expected E110:\n{err}");
    assert!(err.contains("`User.decode` decodes request data into `User`"), "the message names the decode:\n{err}");
    for target in [None, Some("wasm")] {
        let extra: Vec<&str> = target.map(|t| vec!["--target", t]).unwrap_or_default();
        let (ok, out, err) = almide("run", &path, &extra, None);
        assert!(!ok, "the hand-decode program ran on {target:?}:\n{out}");
        assert!(err.contains("E110"), "expected E110 on {target:?}:\n{err}");
    }
}

#[test]
fn the_laundered_hand_decode_is_e110() {
    let path = write_program("laundered.almd", LAUNDERED_PROGRAM);
    let (ok, _, err) = almide("check", &path, &[], None);
    assert!(!ok, "the laundered program checked");
    assert!(err.contains("error[E110]"), "expected E110:\n{err}");
}

/// Negative control of the flow half: with the gate told to admit a hand
/// decode, the program compiles and the client sets `is_admin`. If it still
/// fails, the E110 above is not reading the flow gate.
#[test]
fn admitting_the_hand_decode_lets_the_over_post_land() {
    let path = write_program("hand_decode_control.almd", HAND_DECODE_PROGRAM);
    let control = Some(("ALMIDE_CONTROL_ADMIT_HAND_DECODE", "1"));
    let (ok, out, err) = almide("run", &path, &[], control);
    assert!(ok, "control run failed:\n{err}");
    assert_eq!(out.trim_end(), "201 m is_admin=true", "control should land the over-post");
}
