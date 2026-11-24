//! Novel-item tests for the decision-site ledger (proofs/decision-sites.toml),
//! condition c2: an item the site has never seen is refused.
//!
//! Each test builds its subject from tests/support/novel.rs — a fresh name, a
//! fresh module, a fresh runtime symbol — or enumerates the declarations the
//! site reads at run time, so the test covers an item added after it was
//! written. Each is also the `control_test` its row's c4 patch must turn red.
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

fn project(files: &[(&str, String)]) -> (tempfile::TempDir, PathBuf) {
    let td = tempfile::TempDir::new().unwrap();
    let proj = td.path().join("proj");
    std::fs::create_dir_all(proj.join("src")).unwrap();
    std::fs::write(proj.join("almide.toml"), "[package]\nname = \"novel\"\nversion = \"0.1.0\"\n").unwrap();
    for (name, body) in files {
        let p = proj.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }
    (td, proj)
}

fn refused_with(o: &Output, code: &str, what: &str) {
    let t = text(o);
    assert!(!o.status.success(), "{what}: admitted\n{t}");
    assert!(t.contains(code), "{what}: refused without {code}\n{t}");
}

/// effect.e006-isolation: an effect fn with a fresh name, in a fresh module,
/// called from a pure fn.
#[test]
fn novel_e006_fresh_effect_fn_in_a_fresh_module() {
    let m = novel::module();
    let f = novel::ident("act");
    let g = novel::ident("calm");
    let (_t, proj) = project(&[
        (&format!("src/{m}.almd"), format!("effect fn {f}() -> Unit = println(\"effect\")\n")),
        ("src/main.almd", format!("import {m}\n\nfn {g}() -> Unit = {m}.{f}()\n\neffect fn main() -> Unit = {g}()\n")),
    ]);
    refused_with(&run(&proj, &["check", "src/main.almd"]), "E006", "a pure fn calling a fresh module's effect fn");
}

/// critical.extern-decl: an `@extern` fn with a fresh name under `--profile critical`.
#[test]
fn novel_extern_fn_with_a_fresh_name_under_critical() {
    let f = novel::ident("ext");
    let (_t, proj) = project(&[(
        "src/main.almd",
        format!("@extern(rs, \"std::cmp\", \"max\")\nfn {f}(a: Int, b: Int) -> Int\n\nfn main() -> Unit = println(int.to_string({f}(1, 2)))\n"),
    )]);
    refused_with(&run(&proj, &["check", "src/main.almd", "--profile", "critical"]), "E100", "a fresh @extern fn");
}

fn arg_for(ty: &str) -> &'static str {
    match ty {
        "Int" => "0",
        "Bytes" => "bytes.from_string(\"a\")",
        "Region" => "ffi.region(bytes.from_string(\"a\"))",
        "String" => "\"a\"",
        "Bool" => "true",
        _ => "",
    }
}

/// critical.memory-unsafe-call: EVERY public stdlib fn the stdlib declares
/// `@memory_unsafe`, read from the declarations at run time, is refused under
/// `--profile critical` — a fn marked later is covered without editing this.
#[test]
fn novel_every_memory_unsafe_stdlib_fn_under_critical() {
    let declared = novel::stdlib_fns_declared("@memory_unsafe");
    assert!(declared.len() >= 5, "found only {declared:?}");
    let mut judged = 0;
    for (m, f, params) in declared {
        let args: Vec<&str> = params.iter().map(|t| arg_for(t)).collect();
        if args.iter().any(|a| a.is_empty()) {
            continue;
        }
        let (_t, proj) = project(&[(
            "src/main.almd",
            format!("import {m}\n\neffect fn main() -> Unit = {{\n  let _ = {m}.{f}({})\n  println(\"reached\")\n}}\n", args.join(", ")),
        )]);
        let o = run(&proj, &["check", "src/main.almd", "--profile", "critical"]);
        let t = text(&o);
        assert!(!o.status.success(), "{m}.{f} admitted under critical:\n{t}");
        assert!(
            t.contains("E100") && t.contains(&format!("`{m}.{f}` is outside the memory-safety guarantee")),
            "{m}.{f}: not refused as memory-unsafe:\n{t}"
        );
        judged += 1;
    }
    assert!(judged >= 5, "only {judged} @memory_unsafe fns could be called");
}

/// prim.e085-intrinsic-attr: a fresh fn binding a fresh runtime symbol.
#[test]
fn novel_user_intrinsic_with_a_fresh_runtime_symbol() {
    let f = novel::ident("bind");
    let sym = novel::runtime_symbol();
    let (_t, proj) = project(&[(
        "src/main.almd",
        format!("@intrinsic(\"{sym}\")\nfn {f}(x: Int) -> Int\n\neffect fn main() -> Unit = println(int.to_string({f}(1)))\n"),
    )]);
    refused_with(&run(&proj, &["check", "src/main.almd"]), "E085", "a fresh @intrinsic");
}

/// prim.stdlib-authority: a `stdlib/<fresh>.almd` layout is not the stdlib.
#[test]
fn novel_stdlib_layout_with_a_fresh_stem() {
    let stem = novel::module();
    let f = novel::ident("w");
    let (_t, proj) = project(&[(
        &format!("stdlib/{stem}.almd"),
        format!("@intrinsic(\"almide_rt_fs_write_text\")\neffect fn {f}(p: String, s: String) -> Result[Unit, String]\n"),
    )]);
    refused_with(&run(&proj, &["check", &format!("stdlib/{stem}.almd")]), "E085", "a stdlib-shaped layout with a fresh stem");
}

/// front.checker-budgets: the doubling chain and the Bool matrix with fresh
/// names; each ends in its budget's code.
#[test]
fn novel_budget_shapes_with_fresh_names() {
    let v = novel::ident("t");
    let mut s = format!("effect fn main() -> Unit = {{\n  let {v}0 = 1\n");
    // 12 doublings: 4096 nodes against the 256-node budget, cheap with or
    // without it (so the control, the budget removed, fails fast).
    for i in 1..=12 {
        s.push_str(&format!("  let {v}{i} = ({v}{}, {v}{})\n", i - 1, i - 1));
    }
    s.push_str("  println(\"ok\")\n}\n");
    let (_t, proj) = project(&[("src/main.almd", s)]);
    refused_with(&run(&proj, &["check", "src/main.almd"]), "E111", "a fresh doubling chain");

    let f = novel::ident("f");
    // 12 columns: 2^12 witness steps against the 400-step budget.
    let width = 12;
    let params: Vec<String> = (0..width).map(|i| format!("{v}{i}: Bool")).collect();
    let subject: Vec<String> = (0..width).map(|i| format!("{v}{i}")).collect();
    let mut arms = String::new();
    for (k, val) in ["true", "false"].iter().enumerate() {
        for i in 0..width {
            let row: Vec<&str> = (0..width).map(|j| if j == i { *val } else { "_" }).collect();
            arms.push_str(&format!("  ({}) => {},\n", row.join(", "), k * width + i));
        }
    }
    let args: Vec<&str> = (0..width).map(|_| "true").collect();
    let m = format!(
        "fn {f}({}) -> Int = match ({}) {{\n{arms}}}\n\neffect fn main() -> Unit = println(\"${{{f}({})}}\")\n",
        params.join(", "),
        subject.join(", "),
        args.join(", ")
    );
    let (_t2, proj2) = project(&[("src/main.almd", m)]);
    refused_with(&run(&proj2, &["check", "src/main.almd"]), "E113", "a fresh Bool matrix");
}

/// fsscope.beneath-walk: a rooted read through a fresh symlink, made inside
/// a fresh root and pointing at a fresh directory outside it, is refused on
/// both targets — the walk from the root's descriptor follows no link.
#[cfg(unix)]
#[test]
fn novel_beneath_walk_through_a_fresh_symlink() {
    let root = novel::path_segment();
    let out = novel::path_segment();
    let link = novel::path_segment();
    let prog = format!(
        "import fs\n\neffect fn main() -> Unit = {{\n  let root = fs.open_root(\"{root}\")!\n  match fs.root_read_text(root, \"{link}/s.txt\") {{\n    ok(v) => println(\"ADMITTED ${{v}}\"),\n    err(e) => println(\"refused: ${{e}}\"),\n  }}\n}}\n"
    );
    let (_t, proj) = project(&[("src/main.almd", prog), (&format!("{out}/s.txt"), "outside".to_string())]);
    std::fs::create_dir_all(proj.join(&root)).unwrap();
    std::os::unix::fs::symlink(format!("../{out}"), proj.join(&root).join(&link)).unwrap();
    for target in [&[][..], &["--target", "wasm"][..]] {
        let o = run(&proj, &[&["run", "src/main.almd"][..], target].concat());
        let t = text(&o);
        assert!(!t.contains("ADMITTED"), "{target:?}: a fresh symlink led out of the root:\n{t}");
        assert!(t.contains(&format!("refused: fs: path escapes root: {link}/s.txt")), "{target:?}:\n{t}");
    }
}

/// deps.cache-only: the MCP check route never fetches a dependency it has not
/// seen — a fresh name at a fresh source is reported, nothing is cloned.
#[test]
fn novel_mcp_check_does_not_fetch_a_fresh_dependency() {
    let dep = novel::ident("dep");
    let src = novel::path_segment();
    let td = tempfile::TempDir::new().unwrap();
    let home = td.path().join("home");
    let app = td.path().join("app");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(
        app.join("almide.toml"),
        format!("[package]\nname = \"novel_app\"\nversion = \"0.1.0\"\n\n[dependencies]\n{dep} = {{ git = \"https://github.com/almide/{src}\", tag = \"v0.1.0\" }}\n"),
    )
    .unwrap();
    std::fs::write(app.join("main.almd"), format!("import {dep}\n\neffect fn main() -> Unit = println(\"x\")\n")).unwrap();
    let mut child = Command::new(almide())
        .arg("mcp")
        .current_dir(&app)
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "{}", serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"novel","version":"1"}}})).unwrap();
        writeln!(stdin, "{}", serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"almide_check","arguments":{"file":"main.almd"}}})).unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("not fetched yet"), "a fresh uncached dependency was not reported as such:\n{t}");
    assert!(!app.join("almide.lock").exists(), "almide_check wrote almide.lock");
    let cache = home.join(".almide").join("cache");
    assert!(std::fs::read_dir(&cache).map(|mut d| d.next().is_none()).unwrap_or(true), "almide_check wrote into the cache");
}

/// critical.profile-arming: a fresh profile name is refused on every check
/// route, as is a fresh `--allow` name under `critical` and `--allow` without
/// the profile; `--profile critical` itself checks.
#[test]
fn novel_a_fresh_profile_name_or_grant_is_refused() {
    let (_td, proj) = project(&[("src/main.almd", "fn main() -> Unit = println(\"hi\")\n".to_string())]);
    let profile = novel::ident("profile");
    for args in [&["check", "src/main.almd"][..], &["check", "--json", "src/main.almd"][..], &["check", "--effects", "src/main.almd"][..]] {
        let mut a = args.to_vec();
        a.extend(["--profile", profile.as_str()]);
        let o = run(&proj, &a);
        let t = text(&o);
        assert!(!o.status.success() && t.contains(&format!("unknown profile `{profile}`")), "`almide {}` took a fresh profile:\n{t}", a.join(" "));
    }
    let grant = novel::ident("Cap");
    let o = run(&proj, &["check", "src/main.almd", "--profile", "critical", "--allow", &grant]);
    let t = text(&o);
    assert!(!o.status.success() && t.contains(&grant), "a fresh --allow name was taken under critical:\n{t}");
    let o = run(&proj, &["check", "src/main.almd", "--allow", "IO"]);
    assert!(!o.status.success() && text(&o).contains("--allow requires --profile critical"), "{}", text(&o));
    let o = run(&proj, &["check", "src/main.almd", "--profile", "critical"]);
    assert!(o.status.success(), "control: --profile critical did not check:\n{}", text(&o));
}

/// verify.e051-dialect-ahead: a stamp for a fresh epoch far above any this
/// compiler speaks is E051, on the entry and on an imported module.
#[test]
fn novel_a_fresh_future_dialect_stamp_is_refused() {
    let epoch = novel::fs_op().to_string();
    let module = novel::module();
    let (_td, proj) = project(&[("src/main.almd", format!("@dialect({epoch})\nfn main() -> Unit = println(\"hi\")\n"))]);
    for args in [&["check", "src/main.almd"][..], &["run", "src/main.almd", "--target", "wasm"][..], &["emit", "src/main.almd", "--target", "rust"][..]] {
        refused_with(&run(&proj, args), "E051", &format!("@dialect({epoch})"));
    }
    let (_td2, proj2) = project(&[
        (&format!("src/{module}.almd"), format!("@dialect({epoch})\npub fn x() -> Int = 1\n")),
        ("src/main.almd", format!("import self.{module}\n\nfn main() -> Unit = println(int.to_string({module}.x()))\n")),
    ]);
    refused_with(&run(&proj2, &["check", "src/main.almd"]), "E051", &module);
    let (_td3, proj3) = project(&[("src/main.almd", "fn main() -> Unit = println(\"hi\")\n".to_string())]);
    assert!(run(&proj3, &["check", "src/main.almd"]).status.success(), "control: an unstamped file did not check");
}

/// deps.graph-budget: a chain of fresh path dependencies one deeper than the
/// depth budget is refused before anything past the budget is read.
#[test]
fn novel_a_fresh_dependency_chain_past_the_depth_budget_is_refused() {
    let td = tempfile::TempDir::new().unwrap();
    let names: Vec<String> = (0..66).map(|_| novel::ident("hop")).collect();
    let write = |p: PathBuf, body: String| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    };
    for (i, n) in names.iter().enumerate() {
        let next = names.get(i + 1).map(|m| format!("\n[dependencies]\n{m} = {{ path = \"../{m}\" }}\n")).unwrap_or_default();
        write(td.path().join(n).join("almide.toml"), format!("[package]\nname = \"{n}\"\nversion = \"0.1.0\"\n{next}"));
        write(td.path().join(n).join("src/mod.almd"), "pub fn x() -> Int = 1\n".to_string());
    }
    let app = td.path().join("app");
    write(app.join("almide.toml"), format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n{0} = {{ path = \"../{0}\" }}\n", names[0]));
    write(app.join("src/main.almd"), format!("import {0}\n\nfn main() -> Unit = println(int.to_string({0}.x()))\n", names[0]));
    for args in [&["check", "src/main.almd"][..], &["emit", "src/main.almd", "--target", "rust"][..]] {
        let o = run(&app, args);
        let t = text(&o);
        assert!(!o.status.success() && t.contains("dependency chain too deep") && t.contains(&names[63]), "`almide {}` took a chain past the depth budget:\n{t}", args.join(" "));
    }
    // The count budget, lowered on the command line (the one place a budget
    // is set, ADR-0030 D3), refuses the same fresh graph by its size.
    let o = run(&app, &["--budget", "dependency_count=3", "check", "src/main.almd"]);
    let t = text(&o);
    assert!(!o.status.success() && t.contains("too many dependencies"), "a graph past the count budget was taken:\n{t}");
    // Control: the same chain cut to the budget resolves.
    write(td.path().join(&names[60]).join("almide.toml"), format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\n", names[60]));
    let o = run(&app, &["check", "src/main.almd"]);
    assert!(o.status.success(), "control: a chain within the budget did not check:\n{}", text(&o));
}
