//! Novel-item tests for the decision-site fixes of #4848–#4858
//! (proofs/decision-sites.toml): each builds its subject from
//! tests/support/novel.rs and is its row's `control_test`.
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[path = "support/novel.rs"]
mod novel;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(almide()).args(args).current_dir(dir).stdin(Stdio::null()).output().expect("almide runs")
}

fn text(o: &Output) -> String {
    format!("exit {:?}\n{}{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn project(manifest_extra: &str, files: &[(&str, String)]) -> (tempfile::TempDir, PathBuf) {
    let td = tempfile::TempDir::new().unwrap();
    let proj = td.path().join("proj");
    std::fs::create_dir_all(proj.join("src")).unwrap();
    std::fs::write(proj.join("almide.toml"), format!("[package]\nname = \"novel\"\nversion = \"0.1.0\"\n{manifest_extra}")).unwrap();
    for (name, body) in files {
        let p = proj.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    (td, proj)
}

/// effect.fan-ordered-reach (#4856): fan arms that call fresh fns through fn
/// VALUES — the walk cannot see which fn each value holds — run in arm order:
/// the reader in arm 1 sees what the writer in arm 0 wrote after a sleep.
/// Two shapes: the fn values as parameters of the fan's own fn (an `Rc`
/// capture, so the arms ran inline even before #4856), and the fn values read
/// from a top-level `let` inside a helper the arms call (no capture: before
/// #4856 the arms ran on threads and the reader saw `old`).
#[test]
fn novel_fan_arms_through_fn_values_run_in_order() {
    let (w, r, go) = (novel::ident("write_late"), novel::ident("read_early"), novel::ident("both"));
    let (ops, call) = (novel::ident("ops"), novel::ident("call_op"));
    let writer_reader = format!(
        "import env\nimport fs\n\neffect fn {w}(path: String) -> Result[String, String] = {{\n  env.sleep_ms(80)\n  fs.write_text(path, \"new\")!\n  ok(\"wrote\")\n}}\n\neffect fn {r}(path: String) -> Result[String, String] = ok(fs.read_text(path)!)\n\n"
    );
    let through_params = format!(
        "{writer_reader}effect fn {go}(a: (String) -> Result[String, String], b: (String) -> Result[String, String], p: String) -> Unit = {{\n  let (x, y) = fan {{\n    a(p)!\n    b(p)!\n  }}\n  println(\"${{x}} ${{y}}\")\n}}\n\neffect fn main() -> Unit = {{\n  let d = fs.create_temp_dir(\"novelFan\")!\n  let p = d + \"/f.txt\"\n  fs.write_text(p, \"old\")!\n  {go}({w}, {r}, p)!\n  fs.remove_all(d)!\n}}\n"
    );
    let through_a_top_level_let = format!(
        "{writer_reader}let {ops}: List[(String) -> Result[String, String]] = [{w}, {r}]\n\neffect fn {call}(i: Int, p: String) -> Result[String, String] = {{\n  let f = list.get({ops}, i) ?? ((q) => ok(q))\n  f(p)\n}}\n\neffect fn main() -> Unit = {{\n  let d = fs.create_temp_dir(\"novelFan\")!\n  let p = d + \"/f.txt\"\n  fs.write_text(p, \"old\")!\n  let (x, y) = fan {{\n    {call}(0, p)!\n    {call}(1, p)!\n  }}\n  println(\"${{x}} ${{y}}\")\n  fs.remove_all(d)!\n}}\n"
    );
    for prog in [through_params, through_a_top_level_let] {
        let (_t, proj) = project("", &[("src/main.almd", prog.clone())]);
        for target in [&[][..], &["--target", "wasm"][..]] {
            let o = run(&proj, &[&["run", "src/main.almd"][..], target].concat());
            let t = text(&o);
            assert!(t.contains("wrote new"), "{target:?}: fan arms called through fn values did not run in order:\n{prog}\n{t}");
        }
    }
}

/// verify.wasm-e081-availability (#4857): the wasm availability verdict is
/// reachable from `check --json --target wasm` and from the MCP check tool
/// with `target = "wasm"`, as a JSON row with its code; a target the tool does
/// not know is refused, never read as the default.
#[test]
fn novel_check_json_and_mcp_report_e081_for_a_fresh_program() {
    let f = novel::ident("grab");
    let prog = format!("import unix\n\neffect fn {f}() -> Unit = {{\n  let _ = unix.shm_create(64)\n}}\n\neffect fn main() -> Unit = {f}()!\n");
    let (_t, proj) = project("", &[("src/main.almd", prog)]);
    let o = run(&proj, &["check", "--json", "--target", "wasm", "src/main.almd"]);
    let t = text(&o);
    assert!(!o.status.success(), "check --json --target wasm admitted a wasm-unavailable call:\n{t}");
    assert!(t.lines().any(|l| l.starts_with('{') && l.contains("\"E081\"")), "no E081 JSON row:\n{t}");

    let call = |target: &str| {
        let mut child = Command::new(almide())
            .arg("mcp")
            .current_dir(&proj)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        {
            use std::io::Write;
            let stdin = child.stdin.as_mut().unwrap();
            writeln!(stdin, "{}", serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"novel","version":"1"}}})).unwrap();
            writeln!(stdin, "{}", serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"almide_check","arguments":{"file":"src/main.almd","target":target}}})).unwrap();
        }
        String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).into_owned()
    };
    let wasm = call("wasm");
    assert!(wasm.contains("E081"), "the MCP check tool with target wasm reported no E081:\n{wasm}");
    let fresh = novel::ident("target");
    let unknown = call(&fresh);
    assert!(unknown.contains("unknown target") && unknown.contains(&fresh), "an unknown target was not refused:\n{unknown}");
    // ruling A: the test lane and the run route make one embedded-host decision
    novel_wasm_test_lane_runs_what_run_runs_on_the_embedded_host();
}

/// native.grant-survey / native.dispatch-gate (#4848): `almide emit --target
/// rust` (and the legacy `almide <file> --target rust`) refuses a fresh
/// dependency whose native code the manifest does not grant, as the build
/// does; with the grant the same source is printed (the control).
#[test]
fn novel_emit_refuses_an_ungranted_native_dependency() {
    let (dep, f, m) = (novel::ident("nat"), novel::ident("number"), novel::ident("natmod"));
    let td = tempfile::TempDir::new().unwrap();
    let write = |p: PathBuf, body: String| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    };
    let d = td.path().join(&dep);
    write(d.join("almide.toml"), format!("[package]\nname = \"{dep}\"\nversion = \"0.1.0\"\n"));
    write(d.join("src/mod.almd"), format!("@extern(rust, \"crate::{m}\", \"{f}\")\nfn {f}() -> Int = _\n"));
    write(d.join(format!("native/{m}.rs")), format!("pub fn {f}() -> i64 {{ 7 }}\n"));
    let app = td.path().join("app");
    let manifest = |grant: &str| format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n{dep} = {{ path = \"../{dep}\" }}\n{grant}");
    write(app.join("almide.toml"), manifest(""));
    write(app.join("src/main.almd"), format!("import {dep}\n\neffect fn main() -> Unit = println(int.to_string({dep}.{f}()))\n"));
    for args in [&["emit", "src/main.almd", "--target", "rust"][..], &["src/main.almd", "--target", "rust"][..]] {
        let o = run(&app, args);
        let t = text(&o);
        assert_eq!(o.status.code(), Some(1), "`almide {}` printed a program whose dependency the build refuses:\n{t}", args.join(" "));
        assert!(t.contains(&format!("dependency `{dep}`")) && t.contains("brings native code"), "{t}");
        assert!(!t.contains("fn main"), "a refused emit printed source:\n{t}");
    }
    write(app.join("almide.toml"), manifest(&format!("\n[permissions]\nnative = [\"{dep}\"]\n")));
    let o = run(&app, &["emit", "src/main.almd", "--target", "rust"]);
    assert!(o.status.success(), "control: the granted dependency emits:\n{}", text(&o));
}

/// verify.wasm-e081-availability (#4857, ruling A): `almide test --target
/// wasm` runs a file on the embedded host exactly when `almide run --target
/// wasm` runs the same program there. A fresh fn reaching an op the embedded
/// host serves and the stock p1 shim does not (`unix.shm_create`) runs on both
/// routes; before, the test lane walled it (exit 5) on the shim's audit, a
/// decision the run route does not make. The stock artifact is still refused
/// where there is none (`build --target wasm`, E081: the control).
#[test]
fn novel_wasm_test_lane_runs_what_run_runs_on_the_embedded_host() {
    let f = novel::ident("grab");
    let body = format!("import unix\n\neffect fn {f}() -> Unit = {{\n  let _ = unix.shm_create(64)\n}}\n");
    let (_t, proj) = project(
        "",
        &[
            ("src/main.almd", format!("{body}\neffect fn main() -> Unit = {{\n  {f}()!\n  println(\"ran\")\n}}\n")),
            ("src/main_test.almd", format!("{body}\ntest \"a call the embedded host serves\" {{\n  {f}()!\n  assert(true)\n}}\n")),
        ],
    );
    let r = run(&proj, &["run", "src/main.almd", "--target", "wasm"]);
    assert!(r.status.success() && text(&r).contains("ran"), "run --target wasm did not run the program:\n{}", text(&r));
    let t = run(&proj, &["test", "src/main_test.almd", "--target", "wasm"]);
    let tt = text(&t);
    assert!(t.status.success() && tt.contains("1 passed"), "test --target wasm did not run what run --target wasm runs:\n{tt}");
    let b = run(&proj, &["build", "src/main.almd", "--target", "wasm", "-o", "out.wasm"]);
    assert!(!b.status.success() && text(&b).contains("E081"), "control: the stock artifact was not refused:\n{}", text(&b));
}

/// effect.fan-ordered-reach (c1): on the Rust codegen route the fan pass sees
/// a bodied stdlib call (`env.set` over `__set`) and a module's sibling call
/// as the symbols ResolveCalls / StdlibLowering mint (`almide_rt_<m>_<f>`).
/// The walk follows the record of what each minted symbol names — never its
/// spelling — so both fans run inline, in arm order; the reading-only fan
/// beside them stays threaded (the control half). Observed by emitting the
/// program (`--target rust`): a threaded fan opens its thread group
/// (`let __almide_fan_go = almide_fan_group(..)`; the runtime's own helpers
/// spell the call otherwise).
#[test]
fn novel_fan_arms_reaching_ordered_through_a_minted_symbol_run_in_order() {
    let (m, outer, inner) = (novel::module(), novel::ident("put"), novel::ident("write_it"));
    let (setter, getter, reader) = (novel::ident("set_it"), novel::ident("get_it"), novel::ident("read_it"));
    let module = format!(
        "import fs\n\neffect fn {inner}(path: String) -> Result[String, String] = {{\n  fs.write_text(path, \"new\")!\n  ok(\"wrote\")\n}}\n\neffect fn {outer}(path: String) -> Result[String, String] = ok({inner}(path)!)\n"
    );
    let main = format!(
        "import env\nimport fs\nimport {m}\n\n\
         effect fn {setter}(k: String) -> Result[String, String] = {{\n  env.set(k, \"new\")!\n  ok(\"set\")\n}}\n\n\
         effect fn {getter}(k: String) -> Result[String, String] = ok(env.get(k) ?? \"none\")\n\n\
         effect fn {reader}(path: String) -> Result[String, String] = ok(fs.read_text(path)!)\n\n\
         effect fn main() -> Unit = {{\n  let k = \"NOVEL_FAN_KEY\"\n  let p = \"f.txt\"\n\
         \x20 let (a, b) = fan {{\n    {setter}(k)!\n    {getter}(k)!\n  }}\n\
         \x20 let (c, d) = fan {{\n    {m}.{outer}(p)!\n    {reader}(p)!\n  }}\n\
         \x20 let (e, f) = fan {{\n    {getter}(k)!\n    {reader}(p)!\n  }}\n\
         \x20 println(\"${{a}} ${{b}} ${{c}} ${{d}} ${{e}} ${{f}}\")\n}}\n"
    );
    let (_t, proj) = project("", &[("src/main.almd", main.clone()), (format!("src/{m}.almd").as_str(), module.clone())]);
    let o = run(&proj, &["src/main.almd", "--target", "rust"]);
    let rust = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(o.status.success(), "emit failed:\n{main}\n{}", text(&o));
    let threaded = rust.matches("let __almide_fan_go = almide_fan_group(").count();
    assert_eq!(
        threaded, 1,
        "expected exactly the reading-only fan threaded; {threaded} fans open a thread group (an arm reaching an @ordered fn through a minted symbol was threaded, or the control fan was not):\n{module}\n{main}"
    );
}
