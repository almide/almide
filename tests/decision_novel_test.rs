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

/// effect.e109-value-gate: a pure fn with a fresh name that runs a fresh
/// effect fn of a fresh module only through a value (a record field `main`
/// filled with a closure) is
/// E109 on every route that judges the program, check --effects included
/// (#4847); calling the same effect fn from an effect fn checks.
#[test]
fn novel_a_pure_fn_running_a_fresh_effect_fn_through_a_value_is_refused() {
    let (m, eff, pure) = (novel::module(), novel::ident("eff"), novel::ident("pure"));
    let module = format!("import fs\n\npub effect fn {eff}(p: String) -> Result[String, String] = fs.read_text(p)\n");
    let main = |caller: &str| {
        format!(
            "import self.{m}\n\ntype Reader = {{ read: (String) -> Result[String, String] }}\n\nfn call_it(f: (String) -> Result[String, String]) -> Result[String, String] = f(\"s.txt\")\n\n{caller} {pure}(r: Reader) -> Result[String, String] = call_it(r.read)\n\neffect fn main() -> Unit = {{\n  let r = Reader {{ read: (p) => {m}.{eff}(p) }}\n  println({pure}(r) ?? \"none\")\n}}\n"
        )
    };
    let (_td, proj) = project(&[(&format!("src/{m}.almd"), module.clone()), ("src/main.almd", main("fn"))]);
    for args in [&["check", "src/main.almd"][..], &["check", "--effects", "src/main.almd"][..], &["check", "--json", "src/main.almd"][..], &["run", "src/main.almd", "--target", "wasm"][..], &["emit", "src/main.almd", "--target", "rust"][..]] {
        refused_with(&run(&proj, args), "E109", &format!("{pure} via `almide {}`", args.join(" ")));
    }
    let (_td2, proj2) = project(&[(&format!("src/{m}.almd"), module), ("src/main.almd", main("effect fn"))]);
    let o = run(&proj2, &["check", "src/main.almd"]);
    assert!(o.status.success(), "control: an effect fn running it through a value did not check:\n{}", text(&o));
}

/// verify.verify-command: `almide verify` pins its certificate to the module
/// this build produces; a file of fresh bytes at the default output path of
/// a fresh entry is refused as another module, never certified; with the
/// file gone the bundle names no artifact (nothing is refused).
#[test]
fn novel_verify_refuses_a_fresh_file_that_is_not_the_built_module() {
    let stem = novel::module();
    let (_td, proj) = project(&[(&format!("{stem}.almd"), "effect fn main() -> Unit = println(\"hi\")\n".to_string())]);
    let wasm = proj.join(format!("{stem}.wasm"));
    std::fs::write(&wasm, format!("\0asm{}", novel::ident("bytes"))).unwrap();
    let o = run(&proj, &["verify", &format!("{stem}.almd")]);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(1), "{t}");
    assert!(t.contains("is not the module") && t.contains(&format!("{stem}.wasm")), "a file of fresh bytes was taken as the built module:\n{t}");
    std::fs::remove_file(&wasm).unwrap();
    let o = run(&proj, &["verify", &format!("{stem}.almd")]);
    assert!(!text(&o).contains("is not the module"), "control: with no file there is nothing to mismatch:\n{}", text(&o));
}

/// effect.e095-serve-closure (owner ruling 2026-11-24): the serve app's slot
/// is the one its declaration marks `@instance_closed`, carried through a
/// wrapper's inferred slot. A fresh module wraps `http.serve` under a fresh
/// fn name, and the app its caller passes reads a fresh local: refused, on
/// the route that runs nothing (check) and on the one that builds.
#[test]
fn novel_a_fresh_serve_wrapper_whose_app_reads_a_local_is_refused() {
    let m = novel::module();
    let w = novel::ident("serve_on");
    let local = novel::ident("boot");
    let (_t, proj) = project(&[
        (&format!("src/{m}.almd"), format!("import http\n\npub effect fn {w}(port: Int, app: HttpHandler) -> Unit = http.serve(port, app)!\n")),
        (
            "src/main.almd",
            format!(
                "import http\nimport self.{m}\n\neffect fn main() -> Unit = {{\n  let {local} = \"x\"\n  {m}.{w}(8080, (req) => http.response(200, {local}))!\n}}\n"
            ),
        ),
    ]);
    for args in [&["check", "src/main.almd"][..], &["build", "src/main.almd", "--target", "wasm", "-o", "app.wasm"][..]] {
        let o = run(&proj, args);
        refused_with(&o, "E095", "a fresh serve wrapper's app reading a fresh local");
        assert!(text(&o).contains(&local), "E095 does not name the local {local}:\n{}", text(&o));
    }
}

/// The declarations E095 and E008 read: every `@concurrent` /
/// `@instance_closed` argument of a bundled stdlib fn names a parameter of
/// it (an unknown name would silently declare no slot), and the serve app is
/// one of the instance-closed slots.
#[test]
fn every_stdlib_slot_attribute_names_a_parameter() {
    let bad = almide_frontend::concurrent_reach::stdlib_slot_attributes_naming_no_parameter();
    assert!(bad.is_empty(), "stdlib slot attributes naming no parameter: {bad:?}");
    assert_eq!(almide_frontend::concurrent_reach::stdlib_closed_slots("http", "serve"), vec![1], "http.serve's app is not instance-closed");
}

/// media.e116-header-write (owner ruling 2026-11-24): the response's header
/// writer refuses an owned header name whatever its source. The name arrives
/// at run time (argv), in a fresh mix of cases, held by a fresh local — no
/// literal the check-time E116 could read — and both writers (`set_header`,
/// `with_headers`) abort on both targets, with the C-275 message.
#[test]
fn novel_a_header_name_known_only_at_run_time_is_refused_by_the_writer() {
    let local = novel::ident("field");
    let mix = novel::ident("case");
    let bits: Vec<bool> = mix.bytes().rev().take(12).map(|b| b % 2 == 1).collect();
    let name: String = "content-type".chars().zip(bits.iter().cycle()).map(|(c, up)| if *up { c.to_ascii_uppercase() } else { c }).collect();
    let (_t, proj) = project(&[(
        "src/main.almd",
        format!(
            "import env\nimport http\n\neffect fn main() -> Unit = {{\n  let a = env.args()\n  let {local} = list.get(a, 0) ?? \"\"\n  let r = match list.get(a, 1) ?? \"\" {{\n    \"list\" => http.with_headers(200, \"x\", [({local}, \"text/html\")]),\n    _ => http.set_header(http.response(200, \"x\"), {local}, \"text/html\"),\n  }}\n  println(http.body(r))\n}}\n"
        ),
    )]);
    let check = run(&proj, &["check", "src/main.almd"]);
    assert!(check.status.success(), "the name is not a literal, so check has nothing to refuse:\n{}", text(&check));
    for target in ["rust", "wasm"] {
        for (how, writer) in [("set", "http.set_header"), ("list", "http.with_headers")] {
            let o = run(&proj, &["run", "src/main.almd", "--target", target, "--", &name, how]);
            refused_with(&o, &format!("{writer}: `{name}` is written from the body's type, never by name"), &format!("{writer} with a run-time name on {target}"));
        }
    }
}

/// scope.e033-opaque-outside-module: two fresh dependencies each declare a
/// `mod type` record under one fresh name, so the bare spelling names two
/// opaque types. A root fn reading a field through that bare name is refused
/// (it is foreign here whichever one it names); building the value through
/// the dependency's own fn checks. The ambiguous bare name used to resolve to
/// no record at all, and the read was admitted.
#[test]
fn novel_a_bare_opaque_name_two_fresh_dependencies_share_is_refused() {
    let ty = format!("T{}", novel::ident("vault"));
    let (a, b) = (novel::module(), novel::module());
    let field = novel::ident("key");
    let dep = |name: &str| {
        (
            format!("{name}/almide.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
            format!("{name}/src/mod.almd"),
            format!("mod type {ty} = {{ {field}: String }}\n\npub fn make(s: String) -> {ty} = {{ {field}: s }}\n"),
        )
    };
    let (a_toml, a_toml_text, a_mod, a_mod_text) = dep(&a);
    let (b_toml, b_toml_text, b_mod, b_mod_text) = dep(&b);
    let manifest = format!("[package]\nname = \"novel\"\nversion = \"0.1.0\"\n\n[dependencies]\n{a} = {{ path = \"{a}\" }}\n{b} = {{ path = \"{b}\" }}\n");
    let read = format!("import {a}\nimport {b}\n\nfn peek(v: {ty}) -> String = v.{field}\n\neffect fn main() -> Unit = println(\"x\")\n");
    let api = format!("import {a}\nimport {b}\n\neffect fn main() -> Unit = println(\"${{{a}.make(\\\"x\\\") == {a}.make(\\\"y\\\")}}\")\n");
    for (main, refused) in [(read, true), (api, false)] {
        let (_t, proj) = project(&[
            (&a_toml, a_toml_text.clone()),
            (&a_mod, a_mod_text.clone()),
            (&b_toml, b_toml_text.clone()),
            (&b_mod, b_mod_text.clone()),
            ("src/main.almd", main),
        ]);
        std::fs::write(proj.join("almide.toml"), &manifest).unwrap();
        let o = run(&proj, &["check", "src/main.almd"]);
        if refused {
            refused_with(&o, "E033", "a field read through a bare opaque name two fresh dependencies share");
        } else {
            assert!(o.status.success(), "the dependency's own fn must check:\n{}", text(&o));
        }
    }
}

/// purity.host-ops-classify: a dotted call into a module outside the
/// capability list is judged by what its serving body reaches, never by the
/// module's list. Every host op the floor declares (`@prim_op(host)`, read
/// from stdlib/prim.almd at run time, so an op added later is covered) that
/// has no host-op row is uncounted; a fresh user module's call has no stdlib
/// body and is not.
#[test]
fn novel_every_floor_host_op_without_a_row_is_an_uncounted_host_call() {
    let ops: Vec<String> = novel::stdlib_fns_declared("@prim_op(host)").into_iter().filter(|(m, _, _)| m == "prim").map(|(_, f, _)| format!("prim.{f}")).collect();
    assert!(!ops.is_empty(), "no floor host op found in stdlib/prim.almd");
    let mut uncounted = 0;
    for op in &ops {
        let counted = almide_mir::host_ops::host_op_for_call_name(op).is_some();
        assert_eq!(almide_mir::host_ops::is_uncounted_host_call(op), !counted, "{op}: a floor host op without a row must be uncounted");
        uncounted += usize::from(!counted);
    }
    assert!(uncounted > 0, "every floor host op has a row: the test shows nothing");
    let user = format!("{}.{}", novel::module(), novel::ident("f"));
    assert!(!almide_mir::host_ops::is_uncounted_host_call(&user), "{user}: a user module's call has no stdlib body");
}

/// witness.call-modes-skip: a callee outside the program under a FRESH name —
/// a fresh `__` helper, a fresh `module.fn` — is in no set the render serves,
/// so it gets the out-of-range row the checker rejects, whatever its
/// spelling (almide_mir::certificate::render_served_callee, #4828).
#[test]
fn novel_a_fresh_out_of_program_callee_is_rejected_whatever_its_spelling() {
    use almide_mir::{CallArg, Init, MirFunction, Op, Repr, ValueId};
    let site = |callee: &str| {
        let main = MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: ValueId(0), repr: Repr::Ptr { layout: almide_mir::PLACEHOLDER_LAYOUT }, init: Init::Opaque },
                Op::CallFn { dst: None, name: callee.into(), args: vec![CallArg::Handle(ValueId(0))], result: None },
                Op::Drop { v: ValueId(0) },
            ],
            ..Default::default()
        };
        let program: std::collections::BTreeMap<String, MirFunction> = [(main.name.clone(), main)].into_iter().collect();
        almide_mir::certificate::call_modes_witness(&program, &almide_mir::certificate::render_served_callee)
    };
    for callee in [format!("__{}", novel::ident("helper")), format!("{}.{}", novel::module(), novel::ident("f"))] {
        assert_eq!(site(&callee), "|1 0", "a fresh out-of-program callee `{callee}` was admitted with no row");
    }
}
