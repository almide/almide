//! Novel-item tests for the decision-site rows that cite the bugs-final fixes
//! (#4815 #4816 #4819 #4820 #4823 #4824 #4829 #4840 #4841), re-read against
//! the merged tree (proofs/decision-sites.toml, batch 1a). Each test builds its
//! subject from tests/support/novel.rs — a fresh name, a fresh module, or every
//! declared item enumerated at run time — shows it refused (or decided as the
//! row says), carries its positive control, and is its row's `control_test`.
//!
//! Programs are written as templates with `@X@` holes (`fill`), so no Almide
//! brace is ever a Rust format brace.
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[path = "support/novel.rs"]
mod novel;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(almide())
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .env("NO_COLOR", "1")
        .env_remove("ALMIDE_SKIP_VERSION_CHECK")
        .output()
        .expect("almide runs")
}

fn text(o: &Output) -> String {
    format!("exit {:?}\n{}{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

fn fill(template: &str, holes: &[(&str, &str)]) -> String {
    let mut s = template.to_string();
    for (k, v) in holes {
        s = s.replace(k, v);
    }
    s
}

fn put(p: &Path, body: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn manifest(extra: &str) -> String {
    format!("[package]\nname = \"novel\"\nversion = \"0.1.0\"\n{extra}")
}

/// `<scratch>/proj/` with `almide.toml` = `manifest` and `files` under it.
struct Tree {
    _td: tempfile::TempDir,
    root: PathBuf,
    proj: PathBuf,
}

fn tree(manifest: &str, files: &[(&str, String)]) -> Tree {
    let td = tempfile::TempDir::new().unwrap();
    let root = std::fs::canonicalize(td.path()).unwrap();
    let proj = root.join("proj");
    put(&proj.join("almide.toml"), manifest);
    for (name, body) in files {
        put(&proj.join(name), body);
    }
    Tree { _td: td, root, proj }
}

/// `args` run from inside the project (`{main}` = `src/main.almd`) and from
/// its parent (`{main}` = `proj/src/main.almd`).
fn both_sides(t: &Tree, args: &[&str]) -> Vec<(String, Output)> {
    let mut out = Vec::new();
    for (side, cwd, main) in [("inside", &t.proj, "src/main.almd"), ("outside", &t.root, "proj/src/main.almd")] {
        let a: Vec<String> = args.iter().map(|s| s.replace("{main}", main)).collect();
        let a: Vec<&str> = a.iter().map(String::as_str).collect();
        out.push((format!("{side}: almide {}", a.join(" ")), run(cwd, &a)));
    }
    out
}

fn inside(t: &Tree, args: &[&str]) -> Output {
    let a: Vec<String> = args.iter().map(|s| s.replace("{main}", "src/main.almd")).collect();
    let a: Vec<&str> = a.iter().map(String::as_str).collect();
    run(&t.proj, &a)
}

fn refused(what: &str, o: &Output, needles: &[&str]) {
    let t = text(o);
    assert!(!o.status.success(), "{what}: ADMITTED:\n{t}");
    for n in needles {
        assert!(t.contains(n), "{what}: refused, but without `{n}`:\n{t}");
    }
}

fn admitted(what: &str, o: &Output) {
    assert!(o.status.success(), "{what}: control refused:\n{}", text(o));
}

/// Two characters per contiguous run of `pred` over all of Unicode: the run's
/// first and its last. Enumerated at run time from the table the site reads,
/// so a range added to the table after this test was written is covered.
fn run_ends(pred: impl Fn(char) -> bool) -> Vec<char> {
    let mut out = Vec::new();
    let mut prev: Option<char> = None;
    for n in 0u32..=0x10FFFF {
        let Some(c) = char::from_u32(n) else {
            continue;
        };
        let hit = pred(c);
        match (prev, hit) {
            (None, true) => out.push(c),
            (Some(p), false) if out.last() != Some(&p) => out.push(p),
            _ => {}
        }
        prev = if hit { Some(c) } else { None };
    }
    if let Some(p) = prev
        && out.last() != Some(&p)
    {
        out.push(p);
    }
    out
}

/// The characters E101 refuses raw (C-418's table less TAB/LF/CR). NUL is
/// left out: a raw NUL is refused too, but it is not a character a text
/// editor or a reviewer handles, and it says nothing a neighbour does not.
fn hidden_chars() -> Vec<char> {
    run_ends(|c| c != '\0' && almide_lang::lexer::is_hidden_source_char(c))
}

/// The files of a fresh module with a fresh fn that reads a file
/// (`FS.read`), and of an entry that reaches it only through the module.
fn fs_through_a_fresh_module() -> Vec<(String, String)> {
    let (m, f) = (novel::module(), novel::ident("reach"));
    let module = fill("import fs\n\neffect fn @F@(p: String) -> String = fs.read_text(p)!\n", &[("@F@", &f)]);
    let main = fill("import @M@\n\neffect fn main() -> Unit = println(@M@.@F@(\"s.txt\")!)\n", &[("@M@", &m), ("@F@", &f)]);
    vec![("src/main.almd".to_string(), main), (format!("src/{m}.almd"), module), ("s.txt".to_string(), "novel\n".to_string())]
}

fn tree_owned(manifest: &str, files: &[(String, String)]) -> Tree {
    let borrowed: Vec<(&str, String)> = files.iter().map(|(p, b)| (p.as_str(), b.clone())).collect();
    tree(manifest, &borrowed)
}

// ── [permissions] gate ──────────────────────────────────────────────────

/// perm.allow-gate (#4815): a capability reached only through a fresh user
/// module is refused by every route that judges the program — check, check
/// --json, check --effects (now the whole-program gate), run and emit — from
/// inside and outside the project. Control: granting `FS` admits it.
#[test]
fn novel_allow_gate_refuses_a_capability_reached_through_a_fresh_module() {
    let files = fs_through_a_fresh_module();
    let t = tree_owned(&manifest("\n[permissions]\nallow = [\"IO\"]\n"), &files);
    for args in [
        &["check", "{main}"][..],
        &["check", "--json", "{main}"][..],
        &["check", "--effects", "{main}"][..],
        &["run", "{main}"][..],
        &["emit", "{main}", "--target", "rust"][..],
    ] {
        for (what, o) in both_sides(&t, args) {
            refused(&what, &o, &["capability violation", "FS.read"]);
        }
    }
    let t = tree_owned(&manifest("\n[permissions]\nallow = [\"IO\", \"FS\"]\n"), &files);
    admitted("check with FS granted", &inside(&t, &["check", "{main}"]));
}

/// perm.check-route-gate (#4815): `almide check` judges the whole program —
/// a fresh module's capability and a fresh module's process command outside
/// `[permissions] proc` — from inside and outside the project. Control: the
/// listed command checks clean (check never runs it).
#[test]
fn novel_check_judges_a_fresh_module_against_the_entrys_manifest() {
    let files = fs_through_a_fresh_module();
    let t = tree_owned(&manifest("\n[permissions]\nallow = [\"IO\"]\n"), &files);
    for (what, o) in both_sides(&t, &["check", "{main}"]) {
        refused(&what, &o, &["capability violation"]);
    }
    let (pm, f, listed, other) = (novel::module(), novel::ident("spawn"), novel::ident("cmd"), novel::ident("cmd"));
    let spawn = |cmd: &str| fill("import process\n\neffect fn @F@() -> String = process.exec(\"@C@\", [])!\n", &[("@F@", &f), ("@C@", cmd)]);
    let main = fill("import @M@\n\neffect fn main() -> Unit = println(@M@.@F@()!)\n", &[("@M@", &pm), ("@F@", &f)]);
    let proc_manifest = manifest(&format!("\n[permissions]\nproc = [\"{listed}\"]\n"));
    let t = tree_owned(&proc_manifest, &[("src/main.almd".into(), main.clone()), (format!("src/{pm}.almd"), spawn(&other))]);
    for (what, o) in both_sides(&t, &["check", "{main}"]) {
        refused(&what, &o, &[other.as_str(), "is not in [permissions] proc"]);
    }
    let t = tree_owned(&proc_manifest, &[("src/main.almd".into(), main), (format!("src/{pm}.almd"), spawn(&listed))]);
    admitted("check of a listed command", &inside(&t, &["check", "{main}"]));
}

/// perm.check-effects-gate (#4815): `check --effects` runs the whole-program
/// gate, so a capability reached only through a fresh imported module is
/// refused there too (the per-fn report reads the entry's IR alone).
/// Control: granted, it reports `Permissions OK`.
#[test]
fn novel_check_effects_refuses_a_capability_of_a_fresh_imported_module() {
    let files = fs_through_a_fresh_module();
    let t = tree_owned(&manifest("\n[permissions]\nallow = [\"IO\"]\n"), &files);
    for (what, o) in both_sides(&t, &["check", "--effects", "{main}"]) {
        refused(&what, &o, &["capability violation", "FS.read"]);
    }
    let t = tree_owned(&manifest("\n[permissions]\nallow = [\"IO\", \"FS\"]\n"), &files);
    let o = inside(&t, &["check", "--effects", "{main}"]);
    admitted("check --effects with FS granted", &o);
    assert!(text(&o).contains("Permissions OK"), "{}", text(&o));
}

/// perm.allow-vocabulary (#4829, #4815): a fresh name in `allow` is no
/// capability and is refused — on every route, from inside the project (the
/// manifest gate) and from outside it (the entry's manifest, read by the one
/// reader). Control: a declared parent name checks clean.
#[test]
fn novel_a_fresh_capability_name_is_refused_on_every_route() {
    let fresh = novel::ident("Cap");
    let main = "effect fn main() -> Unit = println(\"hi\")\n".to_string();
    let t = tree(&manifest(&format!("\n[permissions]\nallow = [\"IO\", \"{fresh}\"]\n")), &[("src/main.almd", main.clone())]);
    for args in [
        &["check", "{main}"][..],
        &["check", "--json", "{main}"][..],
        &["check", "--effects", "{main}"][..],
        &["run", "{main}"][..],
        &["run", "{main}", "--target", "wasm"][..],
        &["emit", "{main}", "--target", "rust"][..],
    ] {
        for (what, o) in both_sides(&t, args) {
            refused(&what, &o, &["unknown capability", fresh.as_str()]);
        }
    }
    let t = tree(&manifest("\n[permissions]\nallow = [\"IO\"]\n"), &[("src/main.almd", main)]);
    for (what, o) in both_sides(&t, &["check", "{main}"]) {
        admitted(&what, &o);
    }
}

/// perm.empty-allow-deny-all: `allow = []` grants nothing, so a fresh fn's
/// standard output is a capability violation on every route and side; an
/// absent `allow` is unchecked (the declared choice, #4101) and `IO` grants
/// it (the controls).
#[test]
fn novel_an_empty_allow_refuses_every_capability_a_fresh_fn_reaches() {
    let f = novel::ident("say");
    let main = fill("effect fn @F@() -> Unit = println(\"hi\")\n\neffect fn main() -> Unit = @F@()!\n", &[("@F@", &f)]);
    let t = tree(&manifest("\n[permissions]\nallow = []\n"), &[("src/main.almd", main.clone())]);
    for args in [&["check", "{main}"][..], &["check", "--json", "{main}"][..], &["check", "--effects", "{main}"][..], &["run", "{main}"][..]] {
        for (what, o) in both_sides(&t, args) {
            refused(&what, &o, &["capability violation"]);
        }
    }
    for extra in ["", "\n[permissions]\nallow = [\"IO\"]\n"] {
        let t = tree(&manifest(extra), &[("src/main.almd", main.clone())]);
        admitted(&format!("check with `{}`", extra.trim()), &inside(&t, &["check", "{main}"]));
    }
}

/// perm.manifest-lookup: the manifest that governs an entry is the nearest at
/// or above it, and only else the working directory's. The entry sits two
/// fresh directories below its project; the working directory holds a
/// manifest that grants everything. From outside, the entry's own `allow =
/// ["IO"]` still refuses its file read. Control: an entry with no project of
/// its own is governed by the working directory's manifest.
#[test]
fn novel_the_nearest_manifest_above_a_fresh_entry_governs_it() {
    let (a, b) = (novel::path_segment(), novel::path_segment());
    let entry = format!("src/{a}/{b}/main.almd");
    let prog = "import fs\n\neffect fn main() -> Unit = println(fs.read_text(\"s.txt\")!)\n".to_string();
    let t = tree(&manifest("\n[permissions]\nallow = [\"IO\"]\n"), &[(entry.as_str(), prog.clone())]);
    put(&t.root.join("almide.toml"), &manifest("\n[permissions]\nallow = [\"IO\", \"FS\"]\n"));
    put(&t.root.join("s.txt"), "outer\n");
    let outside = format!("proj/{entry}");
    for args in [
        vec!["check", outside.as_str()],
        vec!["check", "--json", outside.as_str()],
        vec!["run", outside.as_str()],
        vec!["run", outside.as_str(), "--target", "wasm"],
        vec!["emit", outside.as_str(), "--target", "rust"],
    ] {
        let o = run(&t.root, &args);
        refused(&format!("outside: almide {}", args.join(" ")), &o, &["capability violation"]);
        assert!(!text(&o).contains("outer"), "the file was read:\n{}", text(&o));
    }
    // Control: no project of its own — the working directory's governs.
    put(&t.root.join("loose/main.almd"), &prog);
    admitted("an entry with no project under the cwd's manifest", &run(&t.root, &["check", "loose/main.almd"]));
}

/// manifest.parse-failure-dropped (#4829): a manifest the reader refuses in a
/// fresh way — a hyphenated fresh name, a fresh `native` that is not a list,
/// a fresh `[trust].media` that is not a table — governs the command all the
/// same: from outside the project every route ends with its error and the
/// program never runs. Control: the same project with a manifest that parses
/// is governed by it (refused by `allow`, then admitted by a grant).
#[test]
fn novel_a_manifest_that_does_not_parse_refuses_every_route_from_outside() {
    let secret = novel::ident("secret");
    let prog = "import fs\n\neffect fn main() -> Unit = println(fs.read_text(\"proj/s.txt\")!)\n".to_string();
    let perm = "\n[permissions]\nallow = [\"IO\"]\n";
    let cases = [
        format!("[package]\nname = \"{}-x\"\nversion = \"0.1.0\"\n{perm}", novel::ident("pkg")),
        format!("{}native = \"{}\"\n", manifest(perm), novel::ident("nat")),
        format!("{}\n[trust]\nmedia = \"{}\"\n", manifest(perm), novel::ident("media")),
    ];
    for m in &cases {
        let t = tree(m, &[("src/main.almd", prog.clone()), ("s.txt", secret.clone())]);
        for args in [
            &["check", "proj/src/main.almd"][..],
            &["check", "--json", "proj/src/main.almd"][..],
            &["check", "--effects", "proj/src/main.almd"][..],
            &["run", "proj/src/main.almd"][..],
            &["run", "proj/src/main.almd", "--target", "wasm"][..],
            &["emit", "proj/src/main.almd", "--target", "rust"][..],
        ] {
            let o = run(&t.root, args);
            let what = format!("almide {} with\n{m}", args.join(" "));
            refused(&what, &o, &["almide.toml"]);
            assert!(!text(&o).contains(&secret), "{what}: the program ran:\n{}", text(&o));
        }
    }
    let t = tree(&manifest(perm), &[("src/main.almd", prog.clone()), ("s.txt", secret.clone())]);
    refused("a manifest that parses", &run(&t.root, &["check", "proj/src/main.almd"]), &["capability violation"]);
    let t = tree(&manifest("\n[permissions]\nallow = [\"IO\", \"FS\"]\n"), &[("src/main.almd", prog), ("s.txt", secret.clone())]);
    let o = run(&t.root, &["run", "proj/src/main.almd"]);
    admitted("granted", &o);
    assert!(text(&o).contains(&secret), "{}", text(&o));
}

/// manifest.gate-check-manifest (#4829): before any command, a fresh
/// unknown `allow` name is refused ON ITS LINE (`almide.toml:<n>:`) by the
/// one manifest check — from inside the project (the dispatch gate) and from
/// outside it (the governing reader runs the same check). Control: a known
/// name passes the gate.
#[test]
fn novel_the_manifest_gate_refuses_a_fresh_allow_name_on_its_line() {
    let fresh = novel::ident("Cap");
    // Line 6: [package] (1-3), blank (4), [permissions] (5), allow (6).
    let m = manifest(&format!("\n[permissions]\nallow = [\"{fresh}\"]\n"));
    let main = "fn main() -> Unit = println(\"hi\")\n".to_string();
    let t = tree(&m, &[("src/main.almd", main.clone()), ("src/main_test.almd", "test \"t\" {\n  assert(true)\n}\n".to_string())]);
    for args in [&["check", "{main}"][..], &["check", "--json", "{main}"][..], &["run", "{main}"][..], &["build", "{main}", "-o", "out"][..]] {
        for (what, o) in both_sides(&t, args) {
            refused(&what, &o, &["almide.toml:6:", fresh.as_str()]);
        }
    }
    refused("inside: almide test", &inside(&t, &["test", "src/main_test.almd"]), &["almide.toml:6:", fresh.as_str()]);
    let t = tree(&manifest("\n[permissions]\nallow = [\"IO\"]\n"), &[("src/main.almd", main)]);
    admitted("a known name", &inside(&t, &["check", "{main}"]));
}

/// manifest.permission-list-shape: every list key (`allow`, `proc`, `fs`,
/// `native`, `[trust].owners`) written as a fresh value of any other TOML
/// shape is refused on its line by the reader, never read as empty; the
/// gate refuses it on the command line from both sides. Control: the
/// array-of-strings spelling is not refused for its shape.
#[test]
fn novel_a_permission_list_of_another_shape_is_refused() {
    let td = tempfile::TempDir::new().unwrap();
    let path = td.path().join("almide.toml");
    let tables = [("permissions", "allow"), ("permissions", "proc"), ("permissions", "fs"), ("permissions", "native"), ("trust", "owners")];
    for (table, key) in tables {
        let fresh = novel::ident("v");
        for shape in [format!("\"{fresh}\""), format!("{{ {fresh} = 1 }}"), "[1]".to_string(), format!("[\"{fresh}\", 2]"), "true".to_string()] {
            let text = manifest(&format!("\n[{table}]\n{key} = {shape}\n"));
            std::fs::write(&path, &text).unwrap();
            match almide::project::parse_toml(&path) {
                Ok(_) => panic!("[{table}].{key} = {shape} was read:\n{text}"),
                Err(e) => assert!(e.contains("must be an array of strings"), "[{table}].{key} = {shape}: {e}"),
            }
        }
        // Control: an array of strings is not refused for its shape.
        std::fs::write(&path, manifest(&format!("\n[{table}]\n{key} = []\n"))).unwrap();
        if let Err(e) = almide::project::parse_toml(&path) {
            assert!(!e.contains("must be an array of strings"), "[{table}].{key} = []: {e}");
        }
    }
    let t = tree(&manifest(&format!("\n[permissions]\nproc = \"{}\"\n", novel::ident("cmd"))), &[("src/main.almd", "fn main() -> Unit = println(\"hi\")\n".to_string())]);
    for (what, o) in both_sides(&t, &["check", "{main}"]) {
        refused(&what, &o, &["must be an array of strings"]);
    }
}

/// manifest.control-text (#4820): every character of the C-418 table,
/// enumerated at run time (both ends of every range), written as a TOML
/// escape into a fresh key or a fresh dependency string, is refused by the
/// manifest reader; a fresh dependency's own manifest holding one is refused
/// naming the dependency (it was swallowed into "no manifest"). Control: the
/// same manifests without the character are read.
#[test]
fn novel_a_control_or_bidi_character_in_a_manifest_is_refused() {
    let td = tempfile::TempDir::new().unwrap();
    let path = td.path().join("almide.toml");
    let chars = run_ends(almide_base::err_text::almide_rt_errtext_is_escaped);
    assert!(chars.len() >= 10, "the escape table enumerated to {chars:?}");
    for c in &chars {
        let esc = format!("\\U{:08X}", *c as u32);
        let (dep, key) = (novel::ident("dep"), novel::ident("key"));
        for text in [
            manifest(&format!("\n[dependencies]\n{dep} = {{ path = \"../a{esc}b\" }}\n")),
            manifest(&format!("\"{key}{esc}\" = \"v\"\n")),
        ] {
            std::fs::write(&path, &text).unwrap();
            match almide::project::parse_toml(&path) {
                Ok(_) => panic!("U+{:04X} was read:\n{text}", *c as u32),
                Err(e) => assert!(e.contains("control or bidi"), "U+{:04X}: {e}", *c as u32),
            }
        }
    }
    std::fs::write(&path, manifest(&format!("\n[dependencies]\n{} = {{ path = \"../ab\" }}\n", novel::ident("dep")))).unwrap();
    assert!(almide::project::parse_toml(&path).is_ok(), "control: the plain manifest is refused");

    // A fresh path dependency whose manifest holds a bidi control.
    let (dep, f) = (novel::ident("dep"), novel::ident("number"));
    let dep_manifest = |desc: &str| format!("[package]\nname = \"{dep}\"\nversion = \"0.1.0\"\ndescription = \"{desc}\"\n");
    let main = fill("import @D@\n\neffect fn main() -> Unit = println(int.to_string(@D@.@F@()))\n", &[("@D@", &dep), ("@F@", &f)]);
    let app = manifest(&format!("\n[dependencies]\n{dep} = {{ path = \"{dep}\" }}\n"));
    let t = tree(&app, &[("src/main.almd", main.clone())]);
    put(&t.proj.join(&dep).join("almide.toml"), &dep_manifest("a\\u202Eb"));
    put(&t.proj.join(&dep).join("src/mod.almd"), &fill("fn @F@() -> Int = 7\n", &[("@F@", &f)]));
    for args in [&["run", "{main}"][..], &["emit", "{main}", "--target", "rust"][..]] {
        refused(&format!("almide {}", args.join(" ")), &inside(&t, args), &["control or bidi", format!("dependency `{dep}`").as_str()]);
    }
    put(&t.proj.join(&dep).join("almide.toml"), &dep_manifest("ab"));
    let o = inside(&t, &["run", "{main}"]);
    admitted("a clean dependency manifest", &o);
    assert!(text(&o).contains('7'), "{}", text(&o));
}

/// deps.dep-manifest-swallow (#4820): a fresh path dependency whose manifest
/// the reader refuses — a hyphenated name, a duplicated key — ends the build
/// naming the dependency; before #4820 it was read as "no manifest" (its
/// name became the import key, its dependencies were dropped). Control: the
/// same dependency with a manifest that parses builds.
#[test]
fn novel_a_fresh_dependencys_manifest_error_is_not_swallowed() {
    let (dep, f) = (novel::ident("dep"), novel::ident("number"));
    let main = fill("import @D@\n\neffect fn main() -> Unit = println(int.to_string(@D@.@F@()))\n", &[("@D@", &dep), ("@F@", &f)]);
    let app = manifest(&format!("\n[dependencies]\n{dep} = {{ path = \"{dep}\" }}\n"));
    for bad in [
        format!("[package]\nname = \"{dep}-x\"\nversion = \"0.1.0\"\n"),
        format!("[package]\nname = \"{dep}\"\nversion = \"0.1.0\"\nversion = \"0.2.0\"\n"),
    ] {
        let t = tree(&app, &[("src/main.almd", main.clone())]);
        put(&t.proj.join(&dep).join("almide.toml"), &bad);
        put(&t.proj.join(&dep).join("src/mod.almd"), &fill("fn @F@() -> Int = 7\n", &[("@F@", &f)]));
        for args in [&["run", "{main}"][..], &["emit", "{main}", "--target", "rust"][..]] {
            refused(&format!("almide {} with a dependency manifest\n{bad}", args.join(" ")), &inside(&t, args), &[format!("dependency `{dep}`").as_str()]);
        }
    }
    let t = tree(&app, &[("src/main.almd", main)]);
    put(&t.proj.join(&dep).join("almide.toml"), &format!("[package]\nname = \"{dep}\"\nversion = \"0.1.0\"\n"));
    put(&t.proj.join(&dep).join("src/mod.almd"), &fill("fn @F@() -> Int = 7\n", &[("@F@", &f)]));
    admitted("a dependency manifest that parses", &inside(&t, &["run", "{main}"]));
}

/// manifest.compiler-version-pin: a fresh minimum far above this compiler is
/// refused on every compiling command inside the project. (From outside the
/// project the pin is #4846, open — not asserted here.) Control: a minimum
/// below this compiler passes.
#[test]
fn novel_a_fresh_future_compiler_pin_is_refused_on_every_compiling_command() {
    let digits: u32 = novel::ident("pin").bytes().map(u32::from).sum();
    let pin = format!("{}.{}.0", 900 + digits % 90, digits % 17);
    let main = "fn main() -> Unit = println(\"hi\")\n".to_string();
    let test = "test \"t\" {\n  assert(true)\n}\n".to_string();
    let pinned = |v: &str| format!("[package]\nname = \"novel\"\nversion = \"0.1.0\"\nalmide = \"{v}\"\n");
    let t = tree(&pinned(&pin), &[("src/main.almd", main.clone()), ("src/main_test.almd", test.clone())]);
    for args in [
        &["check", "{main}"][..],
        &["check", "--json", "{main}"][..],
        &["run", "{main}"][..],
        &["run", "{main}", "--target", "wasm"][..],
        &["build", "{main}", "-o", "out"][..],
        &["emit", "{main}", "--target", "rust"][..],
        &["test", "src/main_test.almd"][..],
    ] {
        let want = format!("requires almide >= {pin}");
        refused(&format!("almide {}", args.join(" ")), &inside(&t, args), &[want.as_str()]);
    }
    let t = tree(&pinned("0.0.1"), &[("src/main.almd", main)]);
    admitted("a pin below this compiler", &inside(&t, &["check", "{main}"]));
}

// ── process / proc list ─────────────────────────────────────────────────

/// perm.proc-signal-kill: every stdlib fn the sources declare
/// `@signals_process`, enumerated at run time, needs `kill` on `[permissions]
/// proc`: a list naming only a fresh command refuses each one on check,
/// check --json, check --effects and emit, from both sides. Control: `kill`
/// on the list admits it (checked, never run).
#[test]
fn novel_every_declared_signalling_fn_needs_kill_on_the_list() {
    let declared = novel::stdlib_fns_declared("@signals_process");
    assert!(!declared.is_empty(), "no stdlib fn declares @signals_process");
    let mut judged = 0;
    for (module, name, params) in declared {
        let args: Option<Vec<&str>> = params
            .iter()
            .map(|p| match p.as_str() {
                "Int" => Some("0"),
                "String" => Some("\"\""),
                "Bool" => Some("false"),
                _ => None,
            })
            .collect();
        let Some(args) = args else { continue };
        judged += 1;
        let main = format!("import {module}\n\neffect fn main() -> Unit = {{\n  let _ = {module}.{name}({})\n}}\n", args.join(", "));
        let t = tree(&manifest(&format!("\n[permissions]\nproc = [\"{}\"]\n", novel::ident("cmd"))), &[("src/main.almd", main.clone())]);
        for a in [&["check", "{main}"][..], &["check", "--json", "{main}"][..], &["check", "--effects", "{main}"][..], &["emit", "{main}", "--target", "rust"][..]] {
            for (what, o) in both_sides(&t, a) {
                refused(&format!("{module}.{name}: {what}"), &o, &["`kill` is not in [permissions] proc"]);
            }
        }
        let t = tree(&manifest(&format!("\n[permissions]\nproc = [\"{}\", \"kill\"]\n", novel::ident("cmd"))), &[("src/main.almd", main)]);
        admitted(&format!("{module}.{name} with kill listed"), &inside(&t, &["check", "{main}"]));
    }
    assert!(judged > 0, "no declared signalling fn could be called with scalar arguments");
}

/// perm.proc-grant-without-list (#4815): granting `Proc` with no command list
/// is refused under `--profile critical` by check and (now) check --json;
/// without the profile it is a warning. Control: a list naming a fresh
/// command passes the profile.
#[test]
fn novel_proc_granted_without_a_list_is_refused_under_critical() {
    let main = "fn main() -> Unit = println(\"hi\")\n".to_string();
    let t = tree(&manifest("\n[permissions]\nallow = [\"IO\", \"Proc\"]\n"), &[("src/main.almd", main.clone())]);
    for a in [&["check", "--profile", "critical", "{main}"][..], &["check", "--json", "--profile", "critical", "{main}"][..]] {
        for (what, o) in both_sides(&t, a) {
            refused(&what, &o, &["grants `Proc` with no `proc = [...]` command list"]);
        }
    }
    let o = inside(&t, &["check", "{main}"]);
    admitted("without the profile (a warning)", &o);
    assert!(text(&o).contains("warning: [permissions].allow grants `Proc`"), "{}", text(&o));
    let t = tree(&manifest(&format!("\n[permissions]\nallow = [\"IO\", \"Proc\"]\nproc = [\"{}\"]\n", novel::ident("cmd"))), &[("src/main.almd", main)]);
    admitted("a listed command under critical", &inside(&t, &["check", "--profile", "critical", "{main}"]));
}

// ── effect categories of foreign bodies ─────────────────────────────────

/// effect.foreign-top (#4815): an `@inline_rust` fn outside the stdlib — a
/// fresh fn in the entry, and a fresh fn in a fresh path dependency — is
/// every category, so `allow = ["IO"]` refuses its caller on check, check
/// --json, check --effects and emit, from both sides. Control: the same fns
/// without the template check clean.
#[test]
fn novel_a_fresh_inline_rust_fn_outside_the_stdlib_is_every_category() {
    let (f, dep) = (novel::ident("tpl"), novel::ident("dep"));
    let entry = |attr: &str| fill("@A@fn @F@(n: Int) -> Int = n\n\neffect fn main() -> Unit = println(int.to_string(@F@(1)))\n", &[("@A@", attr), ("@F@", &f)]);
    let tpl = "@inline_rust(\"{ {n} }\")\n";
    let perm = "\n[permissions]\nallow = [\"IO\"]\n";
    let routes = [&["check", "{main}"][..], &["check", "--json", "{main}"][..], &["check", "--effects", "{main}"][..], &["emit", "{main}", "--target", "rust"][..]];
    let t = tree(&manifest(perm), &[("src/main.almd", entry(tpl))]);
    for a in routes {
        for (what, o) in both_sides(&t, a) {
            refused(&format!("entry template: {what}"), &o, &["capability violation"]);
        }
    }
    let t = tree(&manifest(perm), &[("src/main.almd", entry(""))]);
    admitted("the entry fn without the template", &inside(&t, &["check", "{main}"]));

    let main = fill("import @D@\n\neffect fn main() -> Unit = println(int.to_string(@D@.@F@(1)))\n", &[("@D@", &dep), ("@F@", &f)]);
    // The template is native code (#4100): granted; the control has none to grant.
    let app = |native: &str| manifest(&format!("\n[dependencies]\n{dep} = {{ path = \"{dep}\" }}\n\n[permissions]\n{native}allow = [\"IO\"]\n"));
    let grant = format!("native = [\"{dep}\"]\n");
    for (attr, refuse) in [(tpl, true), ("", false)] {
        let t = tree(&app(if refuse { grant.as_str() } else { "" }), &[("src/main.almd", main.clone())]);
        put(&t.proj.join(&dep).join("almide.toml"), &format!("[package]\nname = \"{dep}\"\nversion = \"0.1.0\"\n"));
        put(&t.proj.join(&dep).join("src/mod.almd"), &fill("@A@fn @F@(n: Int) -> Int = n\n", &[("@A@", attr), ("@F@", &f)]));
        let o = inside(&t, &["check", "{main}"]);
        if refuse {
            refused("dependency template: check", &o, &["capability violation"]);
        } else {
            admitted("the dependency fn without the template", &o);
        }
    }
}

// ── @pure ───────────────────────────────────────────────────────────────

/// pure.e092-decl (#4824): `@pure` on a fresh fn whose body cannot be
/// checked — an `@inline_rust` template (in the entry and in a fresh user
/// module) or an `effect fn` — is E092 on check and check --json. Control: a
/// fresh `@pure` fn with an Almide body checks clean.
#[test]
fn novel_pure_on_a_fresh_foreign_or_effect_fn_is_refused() {
    let (f, m) = (novel::ident("touch"), novel::module());
    let tpl = "@inline_rust(\"{ let _ = {p}; 1i64 }\")\n";
    let decl = |attrs: &str, eff: &str| fill("@pure\n@A@@E@fn @F@(p: String) -> Int = 1\n", &[("@A@", attrs), ("@E@", eff), ("@F@", &f)]);
    let call = fill("fn main() -> Unit = println(int.to_string(@F@(\"x\")))\n", &[("@F@", &f)]);
    for (what, src) in [("inline_rust", format!("{}\n{call}", decl(tpl, ""))), ("effect fn", format!("{}\neffect fn main() -> Unit = println(int.to_string(@F@(\"x\")!))\n", decl("", "effect ")).replace("@F@", &f))] {
        let t = tree(&manifest(""), &[("src/main.almd", src)]);
        for a in [&["check", "{main}"][..], &["check", "--json", "{main}"][..]] {
            refused(&format!("{what}: almide {}", a.join(" ")), &inside(&t, a), &["E092", f.as_str()]);
        }
    }
    let main = fill("import @M@\n\nfn main() -> Unit = println(int.to_string(@M@.@F@(\"x\")))\n", &[("@M@", &m), ("@F@", &f)]);
    let t = tree(&manifest(""), &[("src/main.almd", main), (format!("src/{m}.almd").as_str(), decl(tpl, ""))]);
    refused("a fresh module's @pure template", &inside(&t, &["check", "{main}"]), &["E092", "@inline_rust"]);
    let t = tree(&manifest(""), &[("src/main.almd", format!("{}\n{call}", decl("", "")))]);
    admitted("a @pure fn with an Almide body", &inside(&t, &["check", "{main}"]));
}

/// pure.e092-reach (#4824): a fresh `@pure` fn that reaches a fresh
/// `@inline_rust` fn through a fresh module's plain fn is E092 naming the
/// chain. Control: the same chain ending in an Almide body checks clean.
#[test]
fn novel_pure_reaching_a_fresh_template_through_a_fresh_module_is_refused() {
    let (m, a, b, c) = (novel::module(), novel::ident("clean"), novel::ident("hop"), novel::ident("touch"));
    let module = |attr: &str| fill("fn @B@(p: String) -> Int = @C@(p) + 1\n\n@T@fn @C@(p: String) -> Int = 1\n", &[("@B@", &b), ("@C@", &c), ("@T@", attr)]);
    let main = fill("import @M@\n\n@pure\nfn @A@(p: String) -> Int = @M@.@B@(p) + 1\n\nfn main() -> Unit = println(int.to_string(@A@(\"x\")))\n", &[("@M@", &m), ("@A@", &a), ("@B@", &b)]);
    let t = tree(&manifest(""), &[("src/main.almd", main.clone()), (format!("src/{m}.almd").as_str(), module("@inline_rust(\"{ let _ = {p}; 1i64 }\")\n"))]);
    for args in [&["check", "{main}"][..], &["check", "--json", "{main}"][..]] {
        refused(&format!("almide {}", args.join(" ")), &inside(&t, args), &["E092", c.as_str()]);
    }
    let t = tree(&manifest(""), &[("src/main.almd", main), (format!("src/{m}.almd").as_str(), module(""))]);
    admitted("the chain ending in an Almide body", &inside(&t, &["check", "{main}"]));
}

// ── @bounded / --profile critical ───────────────────────────────────────

/// bounded.bare-callee (#4823): a bare callee is judged by what it resolves
/// to. Fresh fns of a fresh user module, selectively imported — one named
/// `print_<fresh>`, one `assert_<fresh>`, one plain — are refused (E074 /
/// E076) under `@bounded` and `--profile critical`. Controls: a fresh local
/// `@bounded` fn and the output builtin are admitted.
#[test]
fn novel_a_selectively_imported_fresh_fn_is_judged_by_its_target() {
    let m = novel::module();
    let names = [format!("print_{}", novel::ident("r")), format!("assert_{}", novel::ident("r")), novel::ident("plain")];
    let module: String = names.iter().map(|n| format!("fn {n}(n: Int) -> Int = n + 1\n\n")).collect();
    for n in &names {
        let bounded = fill(
            "import @M@.{@N@}\n\n@bounded\nfn @G@(k: Int) -> Int = @N@(k)\n\nfn main() -> Unit = println(int.to_string(@G@(1)))\n",
            &[("@M@", &m), ("@N@", n), ("@G@", &novel::ident("g"))],
        );
        let t = tree(&manifest(""), &[("src/main.almd", bounded), (format!("src/{m}.almd").as_str(), module.clone())]);
        for a in [&["check", "{main}"][..], &["check", "--json", "{main}"][..], &["run", "{main}"][..]] {
            refused(&format!("@bounded calling {n}: almide {}", a.join(" ")), &inside(&t, a), &["E074"]);
        }
        let crit = fill("import @M@.{@N@}\n\nfn main() -> Unit = println(int.to_string(@N@(1)))\n", &[("@M@", &m), ("@N@", n)]);
        let t = tree(&manifest(""), &[("src/main.almd", crit), (format!("src/{m}.almd").as_str(), module.clone())]);
        let o = inside(&t, &["check", "--profile", "critical", "{main}"]);
        refused(&format!("critical calling {n}"), &o, &[]);
        assert!(text(&o).contains("E074") || text(&o).contains("E076"), "{n}: {}", text(&o));
    }
    let (h, g) = (novel::ident("h"), novel::ident("g"));
    let ok = fill("@bounded\nfn @H@(n: Int) -> Int = n + 1\n\n@bounded\nfn @G@(k: Int) -> Int = @H@(k)\n\nfn main() -> Unit = println(int.to_string(@G@(1)))\n", &[("@H@", &h), ("@G@", &g)]);
    let t = tree(&manifest(""), &[("src/main.almd", ok)]);
    admitted("a local @bounded callee", &inside(&t, &["check", "{main}"]));
    admitted("a local @bounded callee under critical", &inside(&t, &["check", "--profile", "critical", "{main}"]));
}

/// bounded.bare-callee (#4878): a bare callee is decided by what the checker
/// BOUND it to, never by its spelling. A fresh fn-typed parameter, a `let`
/// and a lambda parameter, each spelled like a fresh profile fn, called
/// directly and as a pipe stage, are indirect callees: E074 under `--profile
/// critical` and in a `@bounded` fn, although a top-level fn of that name
/// exists. Through the parameter, `spin` recurses (`spin` -> `go` -> `id` =
/// `spin`), which the call graph never saw. Controls: the same files check
/// clean without the profile, and a direct call of the top-level fn is
/// admitted under it. #4882 (same root): the checker types a pipe stage by
/// the same binding, so a parameter whose type differs from the top-level
/// fn's checks and runs as the parameter (it checked clean as the top-level
/// fn, then failed in codegen: native invalid Rust, wasm E082).
#[test]
fn novel_a_value_spelled_like_a_profile_fn_is_an_indirect_callee() {
    let (id, go, spin, h) = (novel::ident("id"), novel::ident("go"), novel::ident("spin"), novel::ident("h"));
    let subs = [("@I@", id.as_str()), ("@G@", go.as_str()), ("@S@", spin.as_str()), ("@H@", h.as_str())];
    let shapes = [
        ("a parameter", "fn @G@(@I@: (Int) -> Int, n: Int) -> Int = @I@(n)\n\nfn @S@(n: Int) -> Int = @G@(@S@, n + 1)\n\nfn main() -> Unit = println(int.to_string(@G@(@I@, 1)))\n"),
        ("a parameter as a pipe stage", "fn @G@(@I@: (Int) -> Int, n: Int) -> Int = n |> @I@\n\nfn main() -> Unit = println(int.to_string(@G@(@I@, 1)))\n"),
        ("a parameter as a pipe call", "fn @G@(@I@: (Int, Int) -> Int, n: Int) -> Int = n |> @I@(1)\n\nfn @H@(a: Int, b: Int) -> Int = a + b\n\nfn main() -> Unit = println(int.to_string(@G@(@H@, 1)))\n"),
        ("a let", "fn @G@(n: Int) -> Int = {\n  let @I@ = @H@\n  @I@(n)\n}\n\nfn @H@(n: Int) -> Int = n + 2\n\nfn main() -> Unit = println(int.to_string(@G@(1)))\n"),
    ];
    for (what, body) in shapes {
        let src = fill(&format!("fn @I@(n: Int) -> Int = n\n\n{body}"), &subs);
        let t = tree(&manifest(""), &[("src/main.almd", src.clone())]);
        admitted(&format!("{what}: without the profile"), &inside(&t, &["check", "{main}"]));
        for a in [&["check", "--profile", "critical", "{main}"][..], &["check", "--json", "--profile", "critical", "{main}"][..]] {
            refused(&format!("{what} spelled like a profile fn: almide {}", a.join(" ")), &inside(&t, a), &["E074"]);
        }
        let bounded = src.replacen(&format!("fn {go}("), &format!("@bounded\nfn {go}("), 1);
        let t = tree(&manifest(""), &[("src/main.almd", bounded)]);
        refused(&format!("{what} spelled like a profile fn, @bounded"), &inside(&t, &["check", "{main}"]), &["E074"]);
    }
    let ok = fill("fn @I@(n: Int) -> Int = n\n\nfn @G@(n: Int) -> Int = @I@(n) + (n |> @I@)\n\nfn main() -> Unit = println(int.to_string(@G@(1)))\n", &subs);
    let t = tree(&manifest(""), &[("src/main.almd", ok)]);
    admitted("the top-level fn called directly under critical", &inside(&t, &["check", "--profile", "critical", "{main}"]));
    // #4882, the same root: a pipe stage spelled like a fn-typed parameter
    // is typed by the PARAMETER (its result and arity differ from the
    // same-named top-level fn's) and runs it, natively and on wasm.
    let pipe_subs = [("@F@", novel::ident("f")), ("@G@", novel::ident("go")), ("@K@", novel::ident("go2")), ("@S@", novel::ident("sh"))];
    let pipe_subs: Vec<(&str, &str)> = pipe_subs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let pipe_src = fill(
        "fn @F@(n: Int) -> Int = n + 100\n\nfn @G@(@F@: (Int) -> String, n: Int) -> String = n |> @F@\n\nfn @K@(@F@: (Int, Int) -> Int, n: Int) -> Int = n |> @F@(1)\n\nfn @S@(n: Int) -> String = \"s${int.to_string(n)}\"\n\nfn main() -> Unit = {\n  println(@G@(@S@, 5))\n  println(int.to_string(@K@((a, b) => a * 10 + b, 5)))\n  println(int.to_string(@F@(1)))\n}\n",
        &pipe_subs,
    );
    let t = tree(&manifest(""), &[("src/main.almd", pipe_src)]);
    admitted("#4882 check", &inside(&t, &["check", "{main}"]));
    for a in [&["run", "{main}"][..], &["run", "{main}", "--target", "wasm"][..]] {
        let o = inside(&t, a);
        admitted(&a.join(" "), &o);
        assert_eq!(String::from_utf8_lossy(&o.stdout), "s5\n51\n101\n", "{}", a.join(" "));
    }
}

/// E073 (bounded.shape-rules): recursion is found through every form the
/// profile judges as a direct call of a user fn — a call, a pipe stage, a
/// pipe call, an unwrapped pipe stage and UFCS through a value receiver —
/// self and mutual, with fresh names. The call graph used to follow call
/// nodes only, so `n |> spin` recursed under `--profile critical`. Control:
/// the same forms without a cycle are admitted.
#[test]
fn novel_recursion_through_every_direct_call_form_is_refused() {
    let (a, b) = (novel::ident("a"), novel::ident("b"));
    let subs = [("@A@", a.as_str()), ("@B@", b.as_str())];
    let forms = [
        ("a call", "@B@(n)"),
        ("a pipe stage", "n |> @B@"),
        ("a pipe call", "n |> @B@()"),
        ("UFCS through a value", "n.@B@()"),
    ];
    for (what, call) in forms {
        let self_rec = fill(&format!("fn @B@(n: Int) -> Int = if n > 0 then {call} else 0\n\nfn main() -> Unit = println(int.to_string(@B@(1)))\n"), &subs);
        let mutual = fill(&format!("fn @A@(n: Int) -> Int = if n > 0 then {} else 0\n\nfn @B@(n: Int) -> Int = @A@(n - 1)\n\nfn main() -> Unit = println(int.to_string(@A@(1)))\n", call), &subs);
        for (shape, src) in [("self", self_rec), ("mutual", mutual)] {
            let t = tree(&manifest(""), &[("src/main.almd", src)]);
            admitted(&format!("{what}, {shape}: without the profile"), &inside(&t, &["check", "{main}"]));
            refused(&format!("{what}, {shape} recursion under critical"), &inside(&t, &["check", "--profile", "critical", "{main}"]), &["E073"]);
        }
        let acyclic = fill(&format!("fn @B@(n: Int) -> Int = n + 1\n\nfn @A@(n: Int) -> Int = {call}\n\nfn main() -> Unit = println(int.to_string(@A@(1)))\n"), &subs);
        let t = tree(&manifest(""), &[("src/main.almd", acyclic)]);
        admitted(&format!("{what}, acyclic, under critical"), &inside(&t, &["check", "--profile", "critical", "{main}"]));
    }
}

/// bounded.member-module-gate (#4840): a receiver the checker typed as a
/// VALUE is an indirect callee, whatever its spelling. A parameter named
/// like EVERY registry module (enumerated at run time from STDLIB_MODULES and
/// BUNDLED_MODULES), and like a fresh module, holding a record whose field is
/// called, is E074 at that call under `--profile critical`; the same file
/// checks clean without the profile. Control: a real pure-module member is
/// admitted.
#[test]
fn novel_a_value_named_like_any_module_is_an_indirect_callee() {
    use almide_lang::stdlib_info::{BUNDLED_MODULES, STDLIB_MODULES};
    let mut names: Vec<String> = STDLIB_MODULES.iter().chain(BUNDLED_MODULES.iter()).map(|m| m.to_string()).collect();
    names.sort();
    names.dedup();
    names.retain(|n| n.chars().next().is_some_and(|c| c.is_ascii_lowercase()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
    names.push(novel::module());
    let total = names.len();
    let mut judged = 0;
    let mut not_a_binding = Vec::new();
    for n in &names {
        let g = novel::ident("go");
        let src = fill(
            "type Holder = { run: (Int) -> Int }\n\nfn @G@(@N@: Holder, k: Int) -> Int = @N@.run(k)\n\nfn main() -> Unit = {\n  let b = Holder { run: (x) => x + 1 }\n  println(int.to_string(@G@(b, 1)))\n}\n",
            &[("@G@", &g), ("@N@", n)],
        );
        let t = tree(&manifest(""), &[("src/main.almd", src)]);
        if !inside(&t, &["check", "{main}"]).status.success() {
            // `n` cannot be a parameter name (a keyword or a reserved form).
            not_a_binding.push(n.clone());
            continue;
        }
        judged += 1;
        let o = inside(&t, &["check", "--json", "--profile", "critical", "{main}"]);
        refused(&format!("a value named `{n}` under critical"), &o, &[]);
        let at_the_call = String::from_utf8_lossy(&o.stdout).lines().any(|l| l.contains("\"E074\"") && l.contains("\"line\":3"));
        assert!(at_the_call, "a value named `{n}`: no E074 at the member call (line 3):\n{}", text(&o));
    }
    assert!(judged * 2 >= total, "only {judged} of {total} names could be bound: {not_a_binding:?}");
    let ok = "fn main() -> Unit = println(int.to_string(int.abs(0 - 3)))\n".to_string();
    let t = tree(&manifest(""), &[("src/main.almd", ok)]);
    admitted("a real pure-module member under critical", &inside(&t, &["check", "--profile", "critical", "{main}"]));
}

/// critical.entry-scope (#4823): the critical profile judges the ENTRY; a
/// fresh user module is reachable from it only as a profile callee, which it
/// is not — a member call and a selectively imported bare call are both
/// E074. The module's own body is not profiled: importing a fresh module
/// whose fn recurses checks clean. Control: the same fn defined in the entry
/// is admitted.
#[test]
fn novel_critical_profiles_the_entry_and_refuses_a_fresh_module_callee() {
    let (m, h, r) = (novel::module(), novel::ident("inc"), novel::ident("down"));
    let module = fill("fn @H@(n: Int) -> Int = n + 1\n\nfn @R@(n: Int) -> Int = if n <= 0 then 0 else @R@(n - 1)\n", &[("@H@", &h), ("@R@", &r)]);
    let modpath = format!("src/{m}.almd");
    for main in [
        fill("import @M@\n\nfn main() -> Unit = println(int.to_string(@M@.@H@(1)))\n", &[("@M@", &m), ("@H@", &h)]),
        fill("import @M@.{@H@}\n\nfn main() -> Unit = println(int.to_string(@H@(1)))\n", &[("@M@", &m), ("@H@", &h)]),
    ] {
        let t = tree(&manifest(""), &[("src/main.almd", main.clone()), (modpath.as_str(), module.clone())]);
        for a in [&["check", "--profile", "critical", "{main}"][..], &["check", "--json", "--profile", "critical", "{main}"][..]] {
            for (what, o) in both_sides(&t, a) {
                refused(&format!("{what}\n{main}"), &o, &["E074"]);
            }
        }
    }
    let untouched = fill("import @M@\n\nfn main() -> Unit = println(\"x\")\n", &[("@M@", &m)]);
    let t = tree(&manifest(""), &[("src/main.almd", untouched), (modpath.as_str(), module)]);
    admitted("an imported module's own body is not profiled", &inside(&t, &["check", "--profile", "critical", "{main}"]));
    let local = fill("fn @H@(n: Int) -> Int = n + 1\n\nfn main() -> Unit = println(int.to_string(@H@(1)))\n", &[("@H@", &h)]);
    let t = tree(&manifest(""), &[("src/main.almd", local)]);
    admitted("the fn defined in the entry", &inside(&t, &["check", "--profile", "critical", "{main}"]));
}

// ── fs scope ────────────────────────────────────────────────────────────

/// fsscope.component-preopens (#4816): the embedded component host preopens
/// only the granted roots. A fresh granted directory is read; a fresh
/// directory beside it in the same project (so inside the cwd the host
/// preopens without the key) is not. Control: without the key the cwd is
/// preopened and the second file is read.
#[test]
fn novel_the_component_host_preopens_only_a_fresh_granted_root() {
    let (g, d) = (novel::path_segment(), novel::path_segment());
    let (gs, ds) = (novel::ident("granted"), novel::ident("denied"));
    let prog = fill(
        "import fs\nimport http\n\neffect fn main() -> Unit = {\n  let r: Result[HttpResponse, String] = http.get_response(\"http://127.0.0.1:9/x\")\n  println(\"${result.is_ok(r)}\")\n  println(fs.read_text(\"@G@/a.txt\") ?? \"unreadable\")\n  println(fs.read_text(\"@D@/b.txt\") ?? \"unreadable\")\n}\n",
        &[("@G@", &g), ("@D@", &d)],
    );
    let t = tree(&manifest(&format!("\n[permissions]\nfs = [\"./{g}\"]\n")), &[("main.almd", prog), (format!("{g}/a.txt").as_str(), format!("{gs}\n")), (format!("{d}/b.txt").as_str(), format!("{ds}\n"))]);
    let o = run(&t.proj, &["run", "main.almd", "--target", "wasm"]);
    admitted("the component run", &o);
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert_eq!(out, format!("false\n{gs}\n\nunreadable\n"), "the component host reached outside its preopens");
    std::fs::remove_file(t.proj.join("almide.toml")).unwrap();
    let o = run(&t.proj, &["run", "main.almd", "--target", "wasm"]);
    admitted("the component run without the key", &o);
    assert!(String::from_utf8_lossy(&o.stdout).contains(&ds), "control: without the key the cwd is not preopened:\n{}", text(&o));
}

// ── source text ─────────────────────────────────────────────────────────

/// front.e101-hidden-chars (#4820): every character E101 refuses, enumerated
/// at run time from the C-418 table (both ends of every range), written raw
/// into a comment of a fresh file, is E101 on check and check --json; a fresh
/// AST `.json` entry holding one is E101 too (it was not judged). Controls:
/// the files without the character check clean; the escape `\u{202E}` in a
/// string literal is not refused.
#[test]
fn novel_every_hidden_character_is_refused_raw_in_source_and_in_an_ast_entry() {
    let td = tempfile::TempDir::new().unwrap();
    let dir = td.path();
    let chars = hidden_chars();
    assert!(chars.len() >= 10, "the hidden set enumerated to {chars:?}");
    for c in &chars {
        let f = novel::ident("hid");
        let file = format!("{f}.almd");
        std::fs::write(dir.join(&file), format!("// a{c}b\nfn main() -> Unit = println(\"{f}\")\n")).unwrap();
        for a in [vec!["check", file.as_str()], vec!["check", "--json", file.as_str()]] {
            refused(&format!("U+{:04X}: almide {}", *c as u32, a.join(" ")), &run(dir, &a), &["E101"]);
        }
    }
    let f = novel::ident("clean");
    std::fs::write(dir.join("clean.almd"), format!("// ab\nfn main() -> Unit = println(\"{f}\")\n")).unwrap();
    admitted("no raw hidden character", &run(dir, &["check", "clean.almd"]));
    std::fs::write(dir.join("escaped.almd"), format!("fn main() -> Unit = println(\"{f}\\u{{202E}}\")\n")).unwrap();
    admitted("the escape, which is ASCII in the source", &run(dir, &["check", "escaped.almd"]));

    // The AST entry: emitted from a clean fresh file, then one string given a raw RLO.
    let o = run(dir, &["clean.almd", "--emit-ast"]);
    admitted("--emit-ast", &o);
    let ast = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(ast.contains(&f), "the fresh string is not in the AST:\n{ast}");
    std::fs::write(dir.join("clean.json"), &ast).unwrap();
    admitted("the clean AST entry", &run(dir, &["check", "clean.json"]));
    std::fs::write(dir.join("hidden.json"), ast.replacen(&f, &format!("{f}\u{202E}"), 1)).unwrap();
    for a in [&["check", "hidden.json"][..], &["run", "hidden.json"][..]] {
        refused(&format!("AST entry: almide {}", a.join(" ")), &run(dir, a), &["E101"]);
    }
}

/// front.e101-confusables: a fresh identifier mixing Latin with a Cyrillic
/// look-alike, and a fresh all-Cyrillic spelling beside its ASCII skeleton
/// twin in one file, are E101. Controls: the ASCII pair, and the
/// single-script Cyrillic spelling alone, check clean.
#[test]
fn novel_a_fresh_confusable_identifier_is_refused() {
    let digits: String = novel::ident("d").bytes().map(|b| char::from(b'0' + b % 10)).collect();
    let (latin, cyr, mixed) = (format!("ace{digits}"), format!("асе{digits}"), format!("vаl{digits}"));
    let body = |a: &str, b: &str| format!("fn main() -> Unit = {{\n  let {a} = 1\n  let {b} = 2\n  println(int.to_string({a} + {b}))\n}}\n");
    let one = |a: &str| format!("fn main() -> Unit = {{\n  let {a} = 1\n  println(int.to_string({a}))\n}}\n");
    let td = tempfile::TempDir::new().unwrap();
    for (name, src, refuse) in [
        ("mixed", one(&mixed), true),
        ("skeleton pair", body(&latin, &cyr), true),
        ("ascii pair", body(&latin, &format!("{latin}x")), false),
        ("single script", one(&cyr), false),
    ] {
        let file = format!("{}.almd", novel::ident("conf"));
        std::fs::write(td.path().join(&file), &src).unwrap();
        for a in [vec!["check", file.as_str()], vec!["check", "--json", file.as_str()]] {
            let o = run(td.path(), &a);
            if refuse {
                refused(&format!("{name}: almide {}", a.join(" ")), &o, &["E101"]);
            } else {
                admitted(&format!("{name}: almide {}", a.join(" ")), &o);
            }
        }
    }
}

/// front.diagnostic-echo (#4819): every hidden character, enumerated at run
/// time, written raw where the parser stops at it, reaches no output of check
/// or check --json raw — the echoed line, the message quoting it and the
/// JSON rows all write its escape. A test name written with an escape of a
/// sample of them is reported escaped by `almide test`. (The LSP route still
/// sends the message raw: issue draft issue-lsp-hidden-message.)
#[test]
fn novel_a_hidden_character_never_reaches_compiler_output_raw() {
    let td = tempfile::TempDir::new().unwrap();
    let dir = td.path();
    let chars = hidden_chars();
    for c in &chars {
        let f = novel::ident("v");
        std::fs::write(dir.join("p.almd"), format!("fn main() -> Unit = {{\n  let {f} = {c}1\n}}\n")).unwrap();
        for a in [&["check", "p.almd"][..], &["check", "--json", "p.almd"][..]] {
            let o = run(dir, a);
            let t = text(&o);
            assert!(!o.status.success(), "U+{:04X}: admitted:\n{t}", *c as u32);
            assert!(!t.contains(*c), "U+{:04X}: `almide {}` printed it raw:\n{}", *c as u32, a.join(" "), t.escape_debug());
        }
    }
    for c in [chars[0], chars[chars.len() / 2], chars[chars.len() - 1]] {
        let name = novel::ident("name");
        std::fs::write(dir.join("t_test.almd"), format!("test \"{name}\\u{{{:04X}}}tail\" {{\n  assert(false)\n}}\n", c as u32)).unwrap();
        for a in [&["test", "t_test.almd"][..], &["test", "t_test.almd", "--json"][..]] {
            let t = text(&run(dir, a));
            assert!(t.contains(&name), "the failing test is not reported:\n{t}");
            assert!(!t.contains(c), "U+{:04X}: `almide {}` printed it raw:\n{}", c as u32, a.join(" "), t.escape_debug());
        }
    }
}

/// front.nesting-budget (#4841, #4818): nesting past the `nesting` budget is
/// refused whatever constructs make it up. A depth of 600 built from a fresh
/// mix of parentheses, interpolation holes, list literals and calls (the mix
/// drawn from a novel value), 600 nested types and 600 nested patterns each
/// end at the budget; so does 5000 of the mix. Control: a depth of 60 of the
/// same mix is not refused for nesting.
#[test]
fn novel_a_fresh_mix_of_nesting_ends_at_the_budget() {
    let td = tempfile::TempDir::new().unwrap();
    let dir = td.path();
    let seed: Vec<u8> = (0..8).flat_map(|_| novel::ident("n").into_bytes()).collect();
    let mix = |depth: usize| {
        let forms = [("(", ")"), ("\"${", "}\""), ("[", "]"), ("some(", ")")];
        let picks: Vec<usize> = (0..depth).map(|i| usize::from(seed[i % seed.len()]) % forms.len()).collect();
        let open: String = picks.iter().map(|&p| forms[p].0).collect();
        let close: String = picks.iter().rev().map(|&p| forms[p].1).collect();
        format!("fn main() -> Unit = {{\n  let _ = {open}1{close}\n  println(\"ok\")\n}}\n")
    };
    let ty = format!("fn main() -> Unit = {{\n  let _: {}Int{} = []\n  println(\"ok\")\n}}\n", "List[".repeat(600), "]".repeat(600));
    let pat = format!("fn f(x: Option[Int]) -> Int = match x {{\n  {}n{} => 1,\n  _ => 0,\n}}\n\nfn main() -> Unit = println(int.to_string(f(none)))\n", "some(".repeat(600), ")".repeat(600));
    for (what, src) in [("mix 600", mix(600)), ("mix 5000", mix(5000)), ("types 600", ty), ("patterns 600", pat)] {
        let file = format!("{}.almd", novel::ident("deep"));
        std::fs::write(dir.join(&file), src).unwrap();
        let o = run(dir, &["check", file.as_str()]);
        refused(what, &o, &["nesting too deep"]);
    }
    let file = format!("{}.almd", novel::ident("shallow"));
    std::fs::write(dir.join(&file), mix(60)).unwrap();
    let o = run(dir, &["check", file.as_str()]);
    assert!(!text(&o).contains("nesting too deep"), "control: 60 levels hit the budget:\n{}", text(&o));
}

/// input.e110-input-sink (#4884): a request-decoded type is judged by its
/// nominal identity, never by a same-named type of another module. The entry
/// and a fresh module each declare a fresh type name, every pairing of
/// `: Decode` / `: Codec`; request data is decoded in the module (through the
/// `http.decode_json` sink and by hand, `T.decode(json.parse(..)!)`) and in
/// the entry. E110 fires exactly where the decoding side's own type is
/// `Codec`.
#[test]
fn novel_a_decoded_type_is_judged_by_its_own_declaration_not_its_name() {
    let (m, t) = (novel::module(), novel::ident("T"));
    let t = format!("{}{}", t[..1].to_uppercase(), &t[1..]);
    let decoders = [
        ("decode_json", "effect fn create(req: HttpRequest) -> HttpResponse = match http.decode_json[@T@](req) {\n  ok(u) => http.response(201, \"${u.name} ${u.is_admin}\"),\n  err(bad) => bad,\n}\n"),
        ("hand decode", "effect fn create(req: HttpRequest) -> HttpResponse = {\n  let u = @T@.decode(json.parse(http.body(req))!)!\n  http.response(201, \"${u.name} ${u.is_admin}\")\n}\n"),
    ];
    for root_p in ["Decode", "Codec"] {
        for mod_p in ["Decode", "Codec"] {
            for (how, dec) in decoders {
                // decoding in the module
                let module = fill(&format!("import http\nimport json\n\ntype @T@: {mod_p} = {{ name: String, is_admin: Bool = false }}\n\n{dec}"), &[("@T@", &t)]);
                let main = fill(&format!("import http\nimport self.@M@\n\ntype @T@: {root_p} = {{ name: String, is_admin: Bool = false }}\n\nfn mk() -> @T@ = @T@ {{ name: \"r\" }}\n\neffect fn main() -> Unit = {{\n  println(mk().name)\n  println(http.body(@M@.create(http.new_request(\"POST\", \"/u\", \"{{}}\", []))!))\n}}\n"), &[("@T@", &t), ("@M@", &m)]);
                let tr = tree(&manifest(""), &[("src/main.almd", main), (format!("src/{m}.almd").as_str(), module)]);
                let o = inside(&tr, &["check", "{main}"]);
                let what = format!("{how} in the module: entry {root_p}, module {mod_p}");
                if mod_p == "Codec" { refused(&what, &o, &["E110"]) } else { admitted(&what, &o) }
                // decoding in the entry
                let module = fill(&format!("type @T@: {mod_p} = {{ name: String, is_admin: Bool = false }}\n\nfn mk() -> @T@ = @T@ {{ name: \"m\" }}\n\nfn name(u: @T@) -> String = u.name\n"), &[("@T@", &t)]);
                let main = fill(&format!("import http\nimport json\nimport self.@M@\n\ntype @T@: {root_p} = {{ name: String, is_admin: Bool = false }}\n\n{dec}\neffect fn main() -> Unit = {{\n  println(@M@.name(@M@.mk()))\n  println(http.body(create(http.new_request(\"POST\", \"/u\", \"{{}}\", []))!))\n}}\n"), &[("@T@", &t), ("@M@", &m)]);
                let tr = tree(&manifest(""), &[("src/main.almd", main), (format!("src/{m}.almd").as_str(), module)]);
                let o = inside(&tr, &["check", "{main}"]);
                let what = format!("{how} in the entry: entry {root_p}, module {mod_p}");
                if root_p == "Codec" { refused(&what, &o, &["E110"]) } else { admitted(&what, &o) }
            }
        }
    }
}

/// #4886: a derived method was keyed only by the bare type name, so a
/// module's `User.decode` (registered first) answered the entry's own
/// `User.decode(..)` call: the entry's value was typed as the module's
/// record. Each program now reads its own type's derived methods: the entry
/// by the bare key, a module by its canonical `module.User` key, and two
/// modules' same-named types do not share one. Checked, then run on both
/// targets, the output naming whose fields and defaults were used.
#[test]
fn novel_a_derived_method_belongs_to_its_own_type_not_a_same_named_one() {
    let (m, n, t) = (novel::module(), novel::ident("other"), novel::ident("T"));
    let t = format!("{}{}", t[..1].to_uppercase(), &t[1..]);
    for mod_p in ["Decode", "Codec"] {
        let main = fill("import json\nimport self.@M@\nimport self.@N@\n\ntype @T@: Codec = { name: String, age: Int = 7 }\n\neffect fn main() -> Unit = {\n  let u = @T@.decode(json.parse(\"{\\\"name\\\": \\\"x\\\"}\")!)!\n  println(\"entry ${u.name} ${u.age}\")\n  println(json.stringify(u.encode()))\n  println(@M@.show(@M@.load()!))\n  println(@N@.show(@N@.load()!))\n}\n", &[("@T@", &t), ("@M@", &m), ("@N@", &n)]);
        let module = fill(&format!("import json\n\ntype @T@: {mod_p} = {{ title: String, admin: Bool = true }}\n\neffect fn load() -> @T@ = @T@.decode(json.parse(\"{{\\\"title\\\": \\\"m\\\"}}\")!)!\n\nfn show(u: @T@) -> String = \"module ${{u.title}} ${{u.admin}}\"\n"), &[("@T@", &t)]);
        let other = fill("import json\n\ntype @T@: Codec = { size: Int, tag: String = \"o\" }\n\neffect fn load() -> @T@ = @T@.decode(json.parse(\"{\\\"size\\\": 3}\")!)!\n\nfn show(u: @T@) -> String = \"other ${u.size} ${u.tag} ${json.stringify(u.encode())}\"\n", &[("@T@", &t)]);
        let tr = tree(&manifest(""), &[("src/main.almd", main), (format!("src/{m}.almd").as_str(), module), (format!("src/{n}.almd").as_str(), other)]);
        let what = format!("entry {t}: Codec, module {t}: {mod_p}, other module {t}: Codec");
        admitted(&format!("{what}: check"), &inside(&tr, &["check", "{main}"]));
        for target in ["rust", "wasm"] {
            let o = inside(&tr, &["run", "{main}", "--target", target]);
            admitted(&format!("{what}: run --target {target}"), &o);
            let out = String::from_utf8_lossy(&o.stdout).to_string();
            for want in ["entry x 7", "\"age\":7", "module m true", "other 3 o", "\"size\":3"] {
                assert!(out.contains(want), "{what}: run --target {target}: no `{want}` in:\n{out}");
            }
        }
    }
}

/// #4886, the direct call: the entry declares its own `T` and calls a module
/// `T`'s DERIVED method on a module value (`m.load()!.encode()`). The bare
/// slot `T.encode` is the entry's, so the call must link the module type's
/// own method: before, native emitted invalid Rust and wasm refused (E082).
/// The control line calls nothing derived and runs either way; the `Decode`
/// entry (no `encode` of its own) leaves the bare slot to the module.
#[test]
fn novel_a_module_value_calls_its_own_derived_method_beside_a_same_named_entry_type() {
    let (m, t) = (novel::module(), novel::ident("T"));
    let t = format!("{}{}", t[..1].to_uppercase(), &t[1..]);
    let module = fill("import json\n\ntype @T@: Codec, Repr = { size: Int, tag: String = \"o\" }\n\neffect fn load() -> @T@ = @T@.decode(json.parse(\"{\\\"size\\\": 3}\")!)!\n", &[("@T@", &t)]);
    for entry_p in ["Codec", "Codec, Repr", "Decode"] {
        let main = fill(&format!("import json\nimport self.@M@\n\ntype @T@: {entry_p} = {{ name: String, age: Int = 7 }}\n\neffect fn main() -> Unit = {{\n  let u = @T@.decode(json.parse(\"{{\\\"name\\\": \\\"x\\\"}}\")!)!\n  println(\"entry ${{u.name}} ${{u.age}}\")\n  let o = @M@.load()!\n  println(\"control ${{o.size}} ${{o.tag}}\")\n  println(json.stringify(o.encode()))\n  println(o.repr())\n}}\n"), &[("@T@", &t), ("@M@", &m)]);
        let tr = tree(&manifest(""), &[("src/main.almd", main), (format!("src/{m}.almd").as_str(), module.clone())]);
        let what = format!("entry {t}: {entry_p}, module {t}: Codec, Repr, derived call on a module value");
        admitted(&format!("{what}: check"), &inside(&tr, &["check", "{main}"]));
        for target in ["rust", "wasm"] {
            let o = inside(&tr, &["run", "{main}", "--target", target]);
            admitted(&format!("{what}: run --target {target}"), &o);
            let out = String::from_utf8_lossy(&o.stdout).to_string();
            for want in ["entry x 7", "control 3 o", "{\"size\":3,\"tag\":\"o\"}", "size: 3"] {
                assert!(out.contains(want), "{what}: run --target {target}: no `{want}` in:\n{out}");
            }
        }
    }
}
