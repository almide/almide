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
#[test]
fn novel_fan_arms_through_fn_values_run_in_order() {
    let (w, r, go) = (novel::ident("write_late"), novel::ident("read_early"), novel::ident("both"));
    let prog = format!(
        "import env\nimport fs\n\neffect fn {w}(path: String) -> Result[String, String] = {{\n  env.sleep_ms(80)\n  fs.write_text(path, \"new\")!\n  ok(\"wrote\")\n}}\n\neffect fn {r}(path: String) -> Result[String, String] = ok(fs.read_text(path)!)\n\neffect fn {go}(a: (String) -> Result[String, String], b: (String) -> Result[String, String], p: String) -> Unit = {{\n  let (x, y) = fan {{\n    a(p)!\n    b(p)!\n  }}\n  println(\"${{x}} ${{y}}\")\n}}\n\neffect fn main() -> Unit = {{\n  let d = fs.create_temp_dir(\"novelFan\")!\n  let p = d + \"/f.txt\"\n  fs.write_text(p, \"old\")!\n  {go}({w}, {r}, p)!\n  fs.remove_all(d)!\n}}\n"
    );
    let (_t, proj) = project("", &[("src/main.almd", prog)]);
    for target in [&[][..], &["--target", "wasm"][..]] {
        let o = run(&proj, &[&["run", "src/main.almd"][..], target].concat());
        let t = text(&o);
        assert!(t.contains("wrote new"), "{target:?}: fan arms called through fn values did not run in order:\n{t}");
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
