//! The json Codec convenience is decided by RESOLUTION, not spelling (#3493).
//!
//! `json.encode(x)` lowers to `json.stringify(T.encode(x))` and
//! `json.decode[T](s)` to `T.decode(json.parse(s)?)`. Both rewrites matched the
//! member name alone, so they fired for `<any ident>.encode(..)`:
//!
//! - a user module's own `encode` was rewritten to that module's `stringify`
//!   (`fmtx.encode(p)` printed `HIJACKED`), or to an unknown fn (IR-verify ICE);
//! - `util.decode[Int](xs)` became `Int.decode(util.parse(xs)?)` (ICE);
//! - a record value's `encode` field, a user type's UFCS `encode`, and a local
//!   binding that shadows `json` were rewritten to `<local>.stringify` (invalid
//!   Rust natively, a wall or a silently wrong answer on wasm);
//! - `import json as j` then `j.encode(x)` named a module `j` that does not exist.
//!
//! The convenience now applies only when the receiver is not a local and the
//! import table resolves it to the stdlib `json`. Each program runs on both
//! legs and against a twin whose user names are neutral (`enkode`/`dekode`),
//! so a hijack shows as a divergence.

use std::process::Command;

/// A user module with its own `encode`/`decode`, and the `stringify`/`parse`
/// the rewrite used to call in their place.
const MODULE: &str = r#"
type P: Codec = { x: Int }

pub fn ENC(p: P) -> String = "custom:${p.x}"

pub fn DEC[T](xs: List[T]) -> Int = list.len(xs)

pub fn stringify(v: Value) -> String = "HIJACKED"

pub fn parse(s: String) -> Result[Value, String] = err("HIJACKED")
"#;

const MAIN: &str = r#"
import json
import fmtx

type Q: Codec = { y: Int }

type Enc = { ENC: (Q) -> String }

type W = { n: Int }

fn W.ENC(w: W, q: Q) -> String = "ufcs:${w.n}:${q.y}"

fn shadow(q: Q) -> String = {
  let json = Enc { ENC: (q) => "local:${q.y}" }
  json.ENC(q)
}

effect fn main() -> Unit = {
  println(fmtx.ENC(fmtx.P { x: 7 }))
  println("${fmtx.DEC[Int]([1, 2, 3])}")
  let codec = Enc { ENC: (q) => "rec:${q.y}" }
  println(codec.ENC(Q { y: 9 }))
  let w = W { n: 3 }
  println(w.ENC(Q { y: 4 }))
  println(shadow(Q { y: 2 }))
  println(json.encode(Q { y: 5 }))
}
"#;

const MAIN_EXPECTED: &str = "custom:7\n3\nrec:9\nufcs:3:4\nlocal:2\n{\"y\":5}\n";

/// The convenience through an alias of the stdlib json.
const ALIASED: &str = r#"
import json as j

type Q: Codec = { y: Int }

effect fn main() -> Unit = {
  println(j.encode(Q { y: 6 }))
}
"#;

const ALIASED_EXPECTED: &str = "{\"y\":6}\n";

fn spell(src: &str, enc: &str, dec: &str) -> String {
    src.replace("ENC", enc).replace("DEC", dec)
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn log(out: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// Build and run `main` (beside the optional `fmtx` module) on one leg; the stdout.
fn run(main: &str, module: Option<&str>, target: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, main).expect("main");
    if let Some(module) = module {
        std::fs::write(dir.path().join("fmtx.almd"), module).expect("module");
    }
    let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
    let built = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(dir.path())
        .args(["build", "main.almd", "--target", target, "-o"])
        .arg(&artifact)
        .env_remove("ALMIDE_COMPONENT_P3")
        .output()
        .expect("build");
    assert!(built.status.success(), "{target} build:\n{}", log(&built));
    let mut command = if target == "rust" {
        Command::new(&artifact)
    } else {
        let mut c = Command::new("wasmtime");
        c.arg("run").arg(&artifact);
        c
    };
    let out = command.output().expect("run");
    assert!(out.status.success(), "{target} exited {:?}:\n{}", out.status.code(), log(&out));
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn check_leg(target: &str) {
    for (enc, dec) in [("encode", "decode"), ("enkode", "dekode")] {
        let got = run(&spell(MAIN, enc, dec), Some(&spell(MODULE, enc, dec)), target);
        assert_eq!(got, MAIN_EXPECTED, "{target}, user names spelled `{enc}`/`{dec}`");
    }
    assert_eq!(run(ALIASED, None, target), ALIASED_EXPECTED, "{target}, aliased json");
}

#[test]
fn encode_and_decode_resolve_before_the_codec_convenience_on_native() {
    check_leg("rust");
}

#[test]
fn encode_and_decode_resolve_before_the_codec_convenience_on_wasm() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    check_leg("wasm");
}
