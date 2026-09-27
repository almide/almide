//! #2664: after lowering, no `effect (A) -> B` fn TYPE survives anywhere a
//! backend reads a type. `normalize_effect_fn_types` rewrites the slot to its
//! carrier `(A) -> Result[B, String]`, but it covered expression, param,
//! return, field and var-table types only: a `let`'s declared type (on the
//! STATEMENT), a pattern binder's type and a call's type arguments kept the
//! effect form while the value bound there was the carrier. The structural
//! wasm leg interns fn signatures by type, so one value got two signatures and
//! the program was refused (`ty-mismatch:Fn`) — every program in which an
//! effect fn value flowed through a declared fn type.
//!
//! The second test runs the shape the spec corpus cannot hold: a TOP-LEVEL
//! `let app = http.wrap(...)`. The incumbent leg walls a top-level handler, so
//! as a spec fixture it would be a new walled-real row; here it is held to
//! native on the structural leg, which serves it since this fix.
use almide::canonicalize;
use almide::check::Checker;
use almide::lexer::Lexer;
use almide::lower::lower_program;
use almide::parser::Parser;
use std::process::Command;

fn lower(input: &str) -> almide::ir::IrProgram {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    checker.infer_program(&mut prog);
    lower_program(&prog, &checker.env, &checker.type_map)
}

/// Every JSON path under `v` holding a `Ty::Fn` with `is_effect: true`.
fn effect_fn_types(v: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(m) => {
            if m.get("kind").and_then(|k| k.as_str()) == Some("fn")
                && m.get("value").and_then(|x| x.get("is_effect")).and_then(|b| b.as_bool()) == Some(true)
            {
                out.push(path.to_string());
            }
            for (k, x) in m {
                effect_fn_types(x, &format!("{path}/{k}"), out);
            }
        }
        serde_json::Value::Array(xs) => {
            for (i, x) in xs.iter().enumerate() {
                effect_fn_types(x, &format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

const PROGRAM: &str = r#"
type Handler = effect (String) -> String
type Pair = { tag: String, h: Handler }
effect fn inner(s: String) -> String = "in " + s
fn mk(tag: String) -> Handler = (s) => tag + inner(s)!
fn id[T](x: T) -> T = x
effect fn main() -> Unit = {
  let h = mk("t:")
  let hs: List[Handler] = [inner, h]
  match list.first(hs) {
    some(k) => println(k("a")!),
    none => println("none"),
  }
  let pr = Pair { tag: "p", h: inner }
  match pr {
    Pair { tag, h: g } => println(tag + g("b")!),
  }
  let (a, b) = (h, inner)
  println(a("c")! + b("d")!)
  for f in hs {
    println(f("e")!)
  }
  let j = id(h)
  println(j("f")!)
}
"#;

#[test]
fn no_effect_fn_type_survives_lowering() {
    let ir = lower(PROGRAM);
    let mut hits = Vec::new();
    // `def_table` is the declaration record (the checker's spelling of each
    // definition), read by name mangling only — never typed from by a backend.
    let parts = [
        ("functions", serde_json::to_value(&ir.functions).unwrap()),
        ("top_lets", serde_json::to_value(&ir.top_lets).unwrap()),
        ("type_decls", serde_json::to_value(&ir.type_decls).unwrap()),
        ("var_table", serde_json::to_value(&ir.var_table).unwrap()),
    ];
    for (name, v) in &parts {
        effect_fn_types(v, name, &mut hits);
    }
    assert!(hits.is_empty(), "effect fn types left in the lowered IR (carrier expected):\n  {}", hits.join("\n  "));
}

const TOP_LEVEL_APP: &str = r#"import http

effect fn hello(req: HttpRequest) -> HttpResponse = http.response(
  200,
  "hi " + (http.param(req, "name") ?? "-"),
)

fn tag(next: HttpHandler) -> HttpHandler = (req) => {
  let res = next(req)!
  http.set_header(res, "X-Tag", "1")
}

let app = http.wrap(http.router([http.route("GET /hi/{name}", hello)]) ?? hello, [tag])

effect fn visit(target: String) -> String = {
  let res = app(http.new_request("GET", target, "", [:]))!
  "${http.status_code(res)} ${http.body(res)} ${http.get_header(res, "X-Tag") ?? "-"}"
}

effect fn main() -> Unit = {
  println(visit("/hi/a%20b")!)
  println(visit("/nope")!)
}
"#;

#[test]
fn top_level_wrapped_router_runs_on_the_structural_leg() {
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("app.almd");
    std::fs::write(&src, TOP_LEVEL_APP).unwrap();
    let expected = "200 hi a b 1\n404 Not Found 1\n";
    for leg in ["native", "structural"] {
        let mut cmd = Command::new(&bin);
        cmd.arg("run").arg(&src).env_remove("ALMIDE_WASM_INCUMBENT").env_remove("ALMIDE_WASM_STRUCTURAL");
        if leg == "structural" {
            cmd.args(["--target", "wasm"]).env("ALMIDE_WASM_STRUCTURAL", "1");
        }
        let out = cmd.output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{leg}: {stderr}");
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected, "{leg}: {stderr}");
    }
}
