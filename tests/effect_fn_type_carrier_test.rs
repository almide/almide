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
  let res = app(http.new_request("GET", target, "", []))!
  "${http.status(res)} ${http.body(res)} ${http.header(res, "X-Tag") ?? "-"}"
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
        cmd.arg("run").arg(&src).env_remove("ALMIDE_WASM_SKIP_STOCK_AUDIT");
        if leg == "structural" {
            cmd.args(["--target", "wasm"]).env("ALMIDE_WASM_SKIP_STOCK_AUDIT", "1");
        }
        let out = cmd.output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{leg}: {stderr}");
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected, "{leg}: {stderr}");
    }
}

/// A stdlib call slot typed by an ALIAS of an effect fn type (`http.wrap`'s
/// `handler: HttpHandler`) receiving a pure lambda through an `if`, a `match`
/// or a block. A lambda written straight in the slot is checked as an effect
/// body; one reached through a branch types pure and needs the call-site
/// `ok(...)` adapter, which matched the slot only when it was spelled as a
/// literal `effect (A) -> B` — the alias kept native rustc and the wasm leg
/// on two different closure types.
const BRANCHED_HANDLER_ARGS: &str = r#"import http

effect fn hit(h: HttpHandler) -> String = http.body(h(http.new_request("GET", "/", "", []))!)

effect fn run(flag: Bool) -> String = {
  let a = http.wrap(if flag then (req) => http.response(200, "a") else (req) => http.response(201, "b"), [])
  let m = http.wrap(match flag {
    true => (req) => http.response(200, "c"),
    false => (req) => http.response(201, "d"),
  }, [])
  let k = http.wrap({
    let s = "e"
    (req) => http.response(200, s)
  }, [])
  hit(a)! + hit(m)! + hit(k)!
}

effect fn main() -> Unit = {
  println(run(false)!)
  println(run(true)!)
}
"#;

#[test]
fn branched_lambdas_into_an_aliased_effect_slot_run_on_both_legs() {
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("app.almd");
    std::fs::write(&src, BRANCHED_HANDLER_ARGS).unwrap();
    let expected = "bde\nace\n";
    for leg in ["native", "structural"] {
        let mut cmd = Command::new(&bin);
        cmd.arg("run").arg(&src).env_remove("ALMIDE_WASM_SKIP_STOCK_AUDIT");
        if leg == "structural" {
            cmd.args(["--target", "wasm"]).env("ALMIDE_WASM_SKIP_STOCK_AUDIT", "1");
        }
        let out = cmd.output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{leg}: {stderr}");
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected, "{leg}: {stderr}");
    }
}

/// The arms of one branch disagree: one is already an effect handler (a call
/// returning `HttpHandler`), the other a pure lambda. The branch as a whole
/// types as the effect handler, so the whole-value adapter never fired and the
/// pure arm reached the slot unwrapped (rustc E0308: expected `Result<..>`,
/// found `AlmideHttpResponse`; the wasm leg walled `ty-mismatch`). Every
/// route a value reaches an effect slot by is held here: a stdlib call (`if`,
/// `match`, a block's tail), a `let` annotated by the alias, a same-module user
/// fn, and a fn of another user module. A block handed to a user fn's
/// borrowed fn param yields the closure handle, which the borrow derefs.
const MIXED_ARM_HOOKS: &str = r#"import http

effect fn hit(h: HttpHandler) -> String = http.body(h(http.new_request("GET", "/", "", []))!)
"#;

const MIXED_ARM_APP: &str = r#"import http
import hooks

effect fn local_hit(h: HttpHandler) -> String = http.body(h(http.new_request("GET", "/", "", []))!)

effect fn run(flag: Bool) -> String = {
  let a = http.wrap(if flag then http.wrap((req) => http.response(201, "a"), []) else (req) => http.response(200, "b"), [])
  let m = http.wrap(match flag {
    true => http.wrap((req) => http.response(200, "c"), []),
    false => (req) => http.response(201, "d"),
  }, [])
  let g: HttpHandler = if flag then http.wrap((req) => http.response(201, "e"), []) else (req) => http.response(200, "f")
  let u = local_hit(if flag then http.wrap((req) => http.response(201, "g"), []) else (req) => http.response(200, "h"))!
  let v = hooks.hit(if flag then http.wrap((req) => http.response(201, "i"), []) else (req) => http.response(200, "j"))!
  let k = hooks.hit({
    let s = "l"
    if flag then http.wrap((req) => http.response(200, "k"), []) else (req) => http.response(200, s)
  })!
  let n = local_hit({
    let s = "n"
    (req) => http.response(200, s)
  })!
  local_hit(a)! + local_hit(m)! + local_hit(g)! + u + v + k + n
}

effect fn main() -> Unit = {
  println(run(false)!)
  println(run(true)!)
}
"#;

#[test]
fn mixed_effect_and_pure_arms_into_an_effect_slot_run_on_both_legs() {
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("hooks.almd"), MIXED_ARM_HOOKS).unwrap();
    let src = dir.path().join("app.almd");
    std::fs::write(&src, MIXED_ARM_APP).unwrap();
    let expected = "bdfhjln\nacegikn\n";
    for leg in ["native", "structural"] {
        let mut cmd = Command::new(&bin);
        cmd.current_dir(dir.path()).arg("run").arg(&src).env_remove("ALMIDE_WASM_SKIP_STOCK_AUDIT");
        if leg == "structural" {
            cmd.args(["--target", "wasm"]).env("ALMIDE_WASM_SKIP_STOCK_AUDIT", "1");
        }
        let out = cmd.output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{leg}: {stderr}");
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected, "{leg}: {stderr}");
    }
}

/// Run `src` on both legs and hold each to `expected`. Both legs run before
/// anything is asserted, and a failure names every leg that went wrong, so a
/// native failure cannot hide what the wasm leg did.
fn runs_on_both_legs(src: &str, expected: &str) {
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("app.almd");
    std::fs::write(&file, src).unwrap();
    let mut wrong = Vec::new();
    for leg in ["native", "structural"] {
        let mut cmd = Command::new(&bin);
        cmd.arg("run").arg(&file).env_remove("ALMIDE_WASM_SKIP_STOCK_AUDIT");
        if leg == "structural" {
            cmd.args(["--target", "wasm"]).env("ALMIDE_WASM_SKIP_STOCK_AUDIT", "1");
        }
        let out = cmd.output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() || stdout != expected {
            let stderr = String::from_utf8_lossy(&out.stderr);
            wrong.push(format!("{leg}: exit {:?}, stdout {stdout:?}\n{stderr}", out.status.code()));
        }
    }
    assert!(wrong.is_empty(), "expected {expected:?} on both legs:\n{}", wrong.join("\n"));
}

/// A pure lambda reaching an effect-closure type through DATA, not as a call
/// argument or a branch arm: inside `some` / `ok`, a list, tuple, record or
/// map literal, the fallback of `??`, or returned from a callback whose
/// expected return holds a handler (`option.unwrap_or_else(x, () => (req) =>
/// …)`). The expected type never reached the lambda, so it typed pure while
/// the slot it filled was the carrier — check passed and rustc refused
/// (E0308 `expected Result<AlmideHttpResponse, String>`), the wasm leg walled.
/// Minimized from the stdlib inspection's parity fuzz, plus #3749's list case.
const HANDLERS_THROUGH_DATA: &str = r#"import http

type Boxed = { name: String, h: HttpHandler }

effect fn hit(h: HttpHandler) -> String = http.body(h(http.new_request("GET", "/p", "", []))!)

effect fn main() -> Unit = {
  let none_h: HttpHandler? = none
  let a = option.unwrap_or(none_h, (req) => http.response(200, "a"))
  let b0: Result[HttpHandler?, String] = ok(some((req) => http.response(200, "b")))
  let b = option.unwrap_or_else(
    result.unwrap_or_else(b0, (e) => some((req) => http.response(200, e))),
    () => (req) => http.response(200, "y"),
  )
  let c = option.unwrap_or_else(none_h, () => (req) => http.response(200, "c"))
  let d = http.wrap(
    option.unwrap_or_else(
      some(http.wrap((req) => http.response(200, "d"), [(next) => http.wrap(next, [])])),
      () => (req) => http.response(200, "z"),
    ),
    [],
  )
  let mws: List[HttpMiddleware] = [(next) => (req) => http.response(299, "e")]
  let e = http.wrap((req) => http.response(200, "q"), mws)
  let pair: (String, HttpHandler) = ("f", (req) => http.response(200, "f"))
  let bx = Boxed { name: "g", h: (req) => http.response(200, "g") }
  let hs: Map[String, HttpHandler] = ["h": (req) => http.response(200, "h")]
  let h = map.get(hs, "h") ?? a
  let i = none_h ?? ((req) => http.response(200, "i"))
  let r = http.route("GET /p", option.unwrap_or_else(result.unwrap_or_else(b0, (e) => none), () => (req) => http.response(200, "j")))
  println(hit(a)! + hit(b)! + hit(c)! + hit(d)! + hit(e)! + hit(pair.1)! + hit(bx.h)! + hit(h)! + hit(i)! + hit(r.handler)!)
}
"#;

#[test]
fn pure_lambdas_reaching_an_effect_type_through_data_run_on_both_legs() {
    runs_on_both_legs(HANDLERS_THROUGH_DATA, "abcdefghib\n");
}

/// #3749: a lambda nested in another lambda whose declared type returns an
/// effect closure — the documented middleware spelling `(next) => (x) => …
/// next(x)! …` — against a `let` annotation and against a user fn's param.
const NESTED_MIDDLEWARE_3749: &str = r#"type H = effect (Int) -> Int
fn wrap(h: H, m: (H) -> H) -> H = m(h)
effect fn main() -> Unit = {
  let m: (H) -> H = (next) => (x) => next(x)! + 1
  let base: H = (x) => x * 2
  println("${m(base)(20)!}")
  println("${wrap(base, (next) => (x) => next(x)! + 1)(20)!}")
}
"#;

#[test]
fn nested_middleware_lambdas_take_the_declared_effect_type() {
    runs_on_both_legs(NESTED_MIDDLEWARE_3749, "41\n41\n");
}

/// The expected type of a GENERIC call's result flows back into the
/// arguments that produce it: `map.get_or(m, k, d)` in a `HttpHandler` slot
/// makes `V` the handler, so the map literal's value lambdas and the default
/// are effect closures; `map.values(m)` in a `List[HttpMiddleware]` slot does
/// the same for `m`'s values; and a `let` annotation reaches through a pipe
/// into `list.map`'s callback (#3749's curried middleware built per index).
/// The map shapes passed check and died in native lowering with "internal
/// type resolution failed"; the pipe was refused with E001.
const HANDLERS_THROUGH_GENERIC_RETURNS: &str = r#"import http

effect fn hit(h: HttpHandler) -> String = http.body(h(http.new_request("GET", "/p", "", []))!)

effect fn main() -> Unit = {
  let r = http.route("GET /p", map.get_or(["k0": (req) => http.response(200, "a"), "k1": (req) => http.redirect("/r")], "k0", (req) => http.response(200, "y")))
  let b = hit(map.get_or(["k0": (req) => http.response(200, "x")], "zz", (req) => http.response(200, "b")))!
  let w = http.wrap((req) => http.response(200, "c"), map.values(["k0": (next) => http.wrap(next, []), "k1": (next) => next]))
  let mws: List[HttpMiddleware] = list.range(0, 2) |> list.map((i) => (next) => (req) => http.set_header(next(req)!, "x${i}", "1"))
  let resp = http.wrap((req) => http.response(200, "d"), mws)(http.new_request("GET", "/", "", []))!
  println(hit(r.handler)! + b + hit(w)! + http.body(resp) + (http.header(resp, "x0") ?? "-") + (http.header(resp, "x1") ?? "-"))
}
"#;

#[test]
fn effect_lambdas_shaped_through_a_generic_return_run_on_both_legs() {
    runs_on_both_legs(HANDLERS_THROUGH_GENERIC_RETURNS, "abcd11\n");
}

/// The other side of the generic-return flow: an argument that already holds
/// PURE closures (an unannotated `let m = ["k": (req) => …]`) decides the
/// generic, so the expected result does not turn a later lambda into an
/// effect closure that `m`'s values are not (E001); the whole value is
/// adapted at the effect slot. Plus the same flow through `??`, `get_or`,
/// `map.get`, a fold's init and a `Result`.
const GENERIC_RETURNS_DECIDED_BY_DATA: &str = r#"import http

effect fn hit(h: HttpHandler) -> String = http.body(h(http.new_request("GET", "/p", "", []))!)

effect fn main() -> Unit = {
  let m = ["k": (req) => http.response(200, "z")]
  let a = hit(map.get_or(m, "k", (req) => http.response(200, "y")))!
  let ls = [(req) => http.response(200, "q")]
  let b = hit(list.first(ls) ?? ((req) => http.response(200, "w")))!
  let c = hit(option.unwrap_or(list.get(ls, 5), (req) => http.response(200, "e")))!
  println(a + b + c)
}
"#;
const GENERIC_RETURNS_MORE_SHAPES: &str = r#"import http

effect fn hit(h: HttpHandler) -> String = http.body(h(http.new_request("GET", "/p", "", []))!)

effect fn main() -> Unit = {
  let a = hit(list.first([(req) => http.response(200, "a")]) ?? ((req) => http.response(200, "x")))!
  let b = hit(list.get_or([(req) => http.response(200, "b")], 0, (req) => http.response(200, "x")))!
  let mw = map.values(["k": (next) => next])
  let c = hit(http.wrap((req) => http.response(200, "c"), mw))!
  let d = hit(option.unwrap_or_else(map.get(["k": (req) => http.response(200, "d")], "k"), () => (req) => http.response(200, "x")))!
  let e = hit(list.fold([1, 2], (req) => http.response(200, "e"), (acc, i) => acc))!
  let r0: Result[HttpHandler, String] = ok((req) => http.response(200, "f"))
  let f = hit(result.unwrap_or(r0, (req) => http.response(200, "x")))!
  println(a + b + c + d + e + f)
}
"#;

#[test]
fn a_generic_result_bound_by_pure_data_stays_pure_on_both_legs() {
    runs_on_both_legs(GENERIC_RETURNS_DECIDED_BY_DATA, "zqe\n");
    runs_on_both_legs(GENERIC_RETURNS_MORE_SHAPES, "abcdef\n");
}
