//! The DEPENDENCY-SHAPED LEG (#2839): every multi-module project in the spec
//! corpus runs twice — once as written, once re-materialized as a path
//! dependency consumed by a generated root package — and both runs must agree.
//!
//! Why: the spec suite runs almost every project as ONE package, so a rule
//! that fires only on a dependency's module keys (`dep.map` where the package
//! itself sees `map`) was untested. #2839 was exactly that: the #2715
//! visibility check skipped a module keyed `map` (a stdlib name) inside its own
//! package, and fired on the same module keyed `gramide.map` once the package
//! was consumed as a dependency. The module-path-vs-entry-path divergences
//! (#2373/#2374/#2375) and the module-identity class (#1087–#1094) had the
//! same shape: a name resolved from a different source depending on how the
//! module was reached.
//!
//! The corpus, discovered (not listed):
//!
//! * **package projects** — every `almide.toml` under `spec/`. The entries
//!   (`src/main.almd`, a root `main.almd`, every `*_test.almd`) move to a
//!   generated root package; everything else becomes the dependency, under
//!   the project's own package name. `import self.X` in an entry becomes
//!   `import <name>.X`, which is exactly how a consumer spells it.
//! * **sibling-library consumers** — every `.almd` under `spec/` that imports
//!   a sibling directory with a `src/` (the `spec/integration/modules` shape,
//!   resolved from the entry's base dir). In the dependency shape each
//!   library is a path dependency, a library importing another library
//!   declares it as its own dependency, and the root declares only what the
//!   entry imports directly (so phantom-dependency rejection is preserved).
//! * **stdlib-name cells** — one package per stdlib module name in
//!   `STDLIB_MODULES` ∪ `BUNDLED_MODULES`, holding a module of that name that
//!   declares a type of the capitalized name (`map.almd` with `type Map`),
//!   beside a module that does not import it and spells the builtin types,
//!   the stdlib module calls, and a builtin-protocol derivation; and one per
//!   builtin protocol (from `register_builtin_protocols`) with a module that
//!   redeclares it. The cells are package projects, so they run in-package
//!   AND dependency-shaped through the same transform.
//!
//! The oracle is agreement, not a golden: the same exit status, the same
//! diagnostic codes (the module names in the messages legitimately carry the
//! package prefix, so codes are compared, not text), and — when nothing was
//! diagnosed — the same stdout. A case that cannot agree for a reason that is
//! not a bug, or that diverges on a filed bug, is listed in
//! `scripts/lib/dep-shaped-skips.txt`. That list is SHRINK-ONLY: this test
//! fails on an entry that no longer diverges (remove it) or no longer names a
//! corpus case, and `scripts/check-dep-shaped-skips.sh` refuses an entry the
//! base branch did not have.
//!
//! `ALMIDE_BIN` runs the legs against another build (an A/B against a
//! release); the default is this workspace's binary.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Discovery must not silently shrink (a sweep that finds nothing is green):
/// the corpus measured 105 cases when the leg landed (14 package-project
/// entries, 38 sibling-library consumers + the relative-path probe, 44
/// stdlib-name cells, 8 builtin-protocol cells).
const CORPUS_FLOOR: usize = 105;

/// A single compiler invocation that runs longer than this is a hang, and is
/// reported as one instead of stalling the shard.
const PER_RUN_LIMIT: Duration = Duration::from_secs(180);

/// The corpus case that is ALSO materialized with relative transitive path
/// dependencies (see `sibling_case`).
const RELATIVE_PATH_PROBE: &str = "spec/integration/modules/diamond_test.almd";

/// The generated root package's name — never a corpus package name.
const ROOT_PKG: &str = "depshape_root";
/// The package name the synthetic cells use.
const CELL_PKG: &str = "cellpkg";

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from).expect("read_dir") {
        let e = e.expect("entry");
        let p = e.path();
        let dst = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &dst);
        } else {
            std::fs::copy(&p, &dst).expect("copy");
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut es: Vec<_> = std::fs::read_dir(dir).expect("read_dir").map(|e| e.expect("entry").path()).collect();
    es.sort();
    for p in es {
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn rel(p: &Path) -> String {
    p.strip_prefix(repo()).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// The first segment of every `import X...` line (`self` excluded).
fn import_heads(src: &str) -> BTreeSet<String> {
    src.lines()
        .filter_map(|l| l.trim_start().strip_prefix("import "))
        .map(|rest| rest.split(|c: char| c == '.' || c.is_whitespace()).next().unwrap_or("").to_string())
        .filter(|h| !h.is_empty() && h != "self")
        .collect()
}

/// `import self.X` → `import <pkg>.X`, bare `import self` → `import <pkg>`.
fn rewrite_self_imports(src: &str, pkg: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.split_inclusive('\n') {
        let indent = &line[..line.len() - line.trim_start().len()];
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("import self.") {
            out.push_str(&format!("{indent}import {pkg}.{rest}"));
        } else if let Some(rest) = t.strip_prefix("import self") {
            if rest.is_empty() || rest.starts_with(char::is_whitespace) {
                out.push_str(&format!("{indent}import {pkg}{rest}"));
            } else {
                out.push_str(line);
            }
        } else {
            out.push_str(line);
        }
    }
    out
}

fn toml_package_name(toml: &str) -> String {
    toml.lines()
        .find_map(|l| {
            let l = l.trim();
            let v = l.strip_prefix("name")?.trim_start().strip_prefix('=')?.trim();
            Some(v.trim_matches('"').to_string())
        })
        .expect("almide.toml without a package name")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Test,
    Run,
    Check,
}

fn kind_of(src: &str) -> Kind {
    if src.lines().any(|l| l.trim_start().starts_with("test \"")) {
        Kind::Test
    } else if src.contains("fn main(") {
        Kind::Run
    } else {
        Kind::Check
    }
}

/// One case: the same entry file, reached as written (`direct`) and through
/// a path dependency (`dep`). Both legs run with the entry at the same path
/// relative to their working directory, so file names in the output agree.
struct Case {
    id: String,
    kind: Kind,
    entry: String,
    direct: PathBuf,
    dep: PathBuf,
}

/// A package project at `pkg` (its files already in place) → a case per
/// entry. The dependency keeps the package's name; the root is generated.
fn package_cases(id_base: &str, pkg: &Path, scratch: &Path, out: &mut Vec<Case>) {
    let toml = std::fs::read_to_string(pkg.join("almide.toml")).expect("almide.toml");
    assert!(
        !toml.contains("[dependencies]"),
        "{id_base}: a corpus package with its own dependencies needs the leg taught to relocate them"
    );
    let name = toml_package_name(&toml);
    assert_ne!(name, ROOT_PKG);
    let mut files = Vec::new();
    walk(pkg, &mut files);
    let is_entry = |p: &Path| {
        let r = p.strip_prefix(pkg).unwrap().to_string_lossy().replace('\\', "/");
        r == "src/main.almd" || r == "main.almd" || r.ends_with("_test.almd")
    };
    let entries: Vec<PathBuf> = files.iter().filter(|p| is_entry(p)).cloned().collect();

    let direct = scratch.join("direct");
    copy_dir(pkg, &direct);
    let dep_pkg = scratch.join("deps").join(&name);
    copy_dir(pkg, &dep_pkg);
    for e in &entries {
        std::fs::remove_file(dep_pkg.join(e.strip_prefix(pkg).unwrap())).expect("rm entry");
    }
    let app = scratch.join("app");
    let root_toml = toml.replacen(&format!("\"{name}\""), &format!("\"{ROOT_PKG}\""), 1)
        + &format!("\n[dependencies]\n{name} = {{ path = \"../deps/{name}\" }}\n");
    write(&app.join("almide.toml"), &root_toml);
    for e in &entries {
        let r = e.strip_prefix(pkg).unwrap();
        let src = std::fs::read_to_string(e).expect("read entry");
        write(&app.join(r), &rewrite_self_imports(&src, &name));
        out.push(Case {
            id: format!("{id_base}/{}", r.to_string_lossy().replace('\\', "/")),
            kind: kind_of(&src),
            entry: r.to_string_lossy().to_string(),
            direct: direct.clone(),
            dep: app.clone(),
        });
    }
}

/// A sibling library: a directory with a `src/`.
fn is_lib(dir: &Path, name: &str) -> bool {
    dir.join(name).join("src").is_dir()
}

fn lib_imports(dir: &Path, lib: &str) -> BTreeSet<String> {
    let mut files = Vec::new();
    walk(&dir.join(lib).join("src"), &mut files);
    files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "almd"))
        .flat_map(|p| import_heads(&std::fs::read_to_string(p).unwrap_or_default()))
        .filter(|h| h != lib && is_lib(dir, h))
        .collect()
}

/// `relative_transitive`: write a library's own dependencies as `../<lib>`
/// (what a writer would) instead of an absolute path. The corpus uses
/// absolute paths, because a relative one resolves against the working
/// directory (#2844) and would hide every transitive case behind that bug;
/// the one [`RELATIVE_PATH_PROBE`] case keeps the relative spelling measured.
fn sibling_case(entry: &Path, direct_libs: &BTreeSet<String>, scratch: &Path, relative_transitive: bool) -> Case {
    let dir = entry.parent().unwrap();
    let file = entry.file_name().unwrap().to_string_lossy().to_string();
    // The transitive library closure, and each library's own library imports.
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut todo: Vec<String> = direct_libs.iter().cloned().collect();
    while let Some(l) = todo.pop() {
        if graph.contains_key(&l) {
            continue;
        }
        let deps = lib_imports(dir, &l);
        todo.extend(deps.iter().cloned());
        graph.insert(l, deps);
    }
    let src = std::fs::read_to_string(entry).expect("read entry");

    let direct = scratch.join("direct");
    write(&direct.join(&file), &src);
    for l in graph.keys() {
        copy_dir(&dir.join(l), &direct.join(l));
    }

    let deps_dir = scratch.join("deps");
    for (l, deps) in &graph {
        copy_dir(&dir.join(l), &deps_dir.join(l));
        let mut toml = format!("[package]\nname = \"{l}\"\nversion = \"0.1.0\"\n");
        if !deps.is_empty() {
            toml.push_str("\n[dependencies]\n");
            for d in deps {
                let path = if relative_transitive {
                    format!("../{d}")
                } else {
                    deps_dir.join(d).to_string_lossy().replace('\\', "/")
                };
                toml.push_str(&format!("{d} = {{ path = \"{path}\" }}\n"));
            }
        }
        write(&deps_dir.join(l).join("almide.toml"), &toml);
    }
    let app = scratch.join("app");
    let mut toml = format!("[package]\nname = \"{ROOT_PKG}\"\nversion = \"0.1.0\"\n\n[dependencies]\n");
    for l in direct_libs {
        toml.push_str(&format!("{l} = {{ path = \"../deps/{l}\" }}\n"));
    }
    write(&app.join("almide.toml"), &toml);
    write(&app.join(&file), &src);
    let id = if relative_transitive { format!("{}#relative-transitive-path", rel(entry)) } else { rel(entry) };
    Case { id, kind: kind_of(&src), entry: file, direct, dep: app }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// The file every cell carries beside the colliding module: it does NOT
/// import it, and spells the builtin types, the stdlib modules by name, and a
/// builtin-protocol derivation (`derive`).
fn cell_user(derive: &str) -> String {
    format!(
        "type C: {derive} = Red | Green\n\n\
         fn same(a: C, b: C) -> Bool = a == b\n\n\
         fn go() -> Int = {{\n\
         \x20 let m: Map[String, Int] = [\"a\": 1]\n\
         \x20 let l: List[Int] = [1, 2]\n\
         \x20 let o: Option[Int] = some(3)\n\
         \x20 let s: Set[Int] = set.from_list([4])\n\
         \x20 let r: Result[Int, String] = ok(5)\n\
         \x20 let t: String = \"xy\"\n\
         \x20 let f: Float = 1.5\n\
         \x20 map.len(m) + list.len(l) + (o ?? 0) + set.len(s) + (r ?? 0) + string.len(t) + float.to_int(f) + (if same(Red, Red) then 1 else 0)\n\
         }}\n"
    )
}

/// A stdlib-name cell: `src/<module>.almd` declaring `type <Module>` (for
/// `map`, the builtin's own name), beside `user.almd`.
fn stdlib_name_cell(module: &str, dir: &Path) {
    let ty = capitalize(module);
    write(&dir.join("almide.toml"), &format!("[package]\nname = \"{CELL_PKG}\"\nversion = \"0.1.0\"\n"));
    write(
        &dir.join("src").join(format!("{module}.almd")),
        &format!("type {ty} = {{ text: String }}\n\nfn build(t: String) -> {ty} = {ty} {{ text: t }}\n"),
    );
    write(&dir.join("src").join("user.almd"), &cell_user("Eq"));
    write(
        &dir.join("src").join("main.almd"),
        &format!(
            "import self.{module}\nimport self.user\n\neffect fn main() -> Unit = {{\n  println(\"${{{module}.build(\"ok\").text}} ${{user.go()}}\")\n}}\n"
        ),
    );
}

/// A builtin-protocol cell: `src/<proto>.almd` redeclaring `protocol <P>`,
/// beside a `user.almd` that derives the builtin `P`.
fn builtin_protocol_cell(proto: &str, dir: &Path) {
    let module = format!("{}_decl", proto.to_lowercase());
    write(&dir.join("almide.toml"), &format!("[package]\nname = \"{CELL_PKG}\"\nversion = \"0.1.0\"\n"));
    write(
        &dir.join("src").join(format!("{module}.almd")),
        &format!(
            "protocol {proto} {{\n  fn ping(self) -> Int\n}}\n\ntype Q: {proto} = {{ v: Int }}\n\nfn Q.ping(self) -> Int = self.v\n\nfn get() -> Int = Q {{ v: 1 }}.ping()\n"
        ),
    );
    write(&dir.join("src").join("user.almd"), &cell_user(proto));
    write(
        &dir.join("src").join("main.almd"),
        &format!("import self.{module}\nimport self.user\n\neffect fn main() -> Unit = {{\n  println(\"${{{module}.get()}} ${{user.go()}}\")\n}}\n"),
    );
}

fn builtin_protocol_names() -> Vec<String> {
    let mut env = almide_frontend::types::TypeEnv::new();
    almide_frontend::canonicalize::protocols::register_builtin_protocols(&mut env);
    let mut names: Vec<String> = env.protocols.keys().map(|k| k.as_str().to_string()).collect();
    names.sort();
    names
}

fn stdlib_module_names() -> Vec<String> {
    use almide_types::stdlib_info::{BUNDLED_MODULES, STDLIB_MODULES};
    let set: BTreeSet<&str> = STDLIB_MODULES.iter().chain(BUNDLED_MODULES.iter()).copied().collect();
    set.into_iter().map(str::to_string).collect()
}

fn discover(scratch: &Path) -> Vec<Case> {
    let spec = repo().join("spec");
    let mut files = Vec::new();
    walk(&spec, &mut files);
    let mut cases = Vec::new();
    let mut n = 0usize;
    let mut next = || {
        n += 1;
        scratch.join(format!("c{n:03}"))
    };

    // Package projects.
    let pkgs: Vec<PathBuf> =
        files.iter().filter(|p| p.file_name().is_some_and(|f| f == "almide.toml")).map(|p| p.parent().unwrap().to_path_buf()).collect();
    for p in &pkgs {
        package_cases(&rel(p), p, &next(), &mut cases);
    }

    // Sibling-library consumers: outside every package and every library.
    for f in &files {
        if f.extension().is_none_or(|e| e != "almd") || pkgs.iter().any(|p| f.starts_with(p)) {
            continue;
        }
        // inside some `<lib>/src/` — a library, not a consumer
        if f.ancestors().skip(1).any(|a| a.file_name().is_some_and(|n| n == "src")) {
            continue;
        }
        let dir = f.parent().unwrap();
        let src = std::fs::read_to_string(f).unwrap_or_default();
        let libs: BTreeSet<String> = import_heads(&src).into_iter().filter(|h| is_lib(dir, h)).collect();
        if libs.is_empty() {
            continue;
        }
        cases.push(sibling_case(f, &libs, &next(), false));
        if rel(f) == RELATIVE_PATH_PROBE {
            cases.push(sibling_case(f, &libs, &next(), true));
        }
    }

    // Stdlib-name and builtin-protocol cells.
    for m in stdlib_module_names() {
        let d = next();
        stdlib_name_cell(&m, &d.join("src_pkg"));
        package_cases(&format!("cell:stdlib-module:{m}"), &d.join("src_pkg"), &d, &mut cases);
    }
    for p in builtin_protocol_names() {
        let d = next();
        builtin_protocol_cell(&p, &d.join("src_pkg"));
        package_cases(&format!("cell:builtin-protocol:{p}"), &d.join("src_pkg"), &d, &mut cases);
    }
    cases
}

/// What one leg observed.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    exit: Option<i32>,
    codes: Vec<String>,
    /// stdout (and, for a failure with no diagnostic, stderr) — compared only
    /// when nothing was diagnosed, since diagnostic text names module keys.
    body: String,
}

fn diag_codes(text: &str) -> Vec<String> {
    let mut codes = Vec::new();
    for sev in ["error[", "warning["] {
        let mut rest = text;
        while let Some(i) = rest.find(sev) {
            rest = &rest[i + sev.len()..];
            if let Some(j) = rest.find(']') {
                let code = &rest[..j];
                if !code.is_empty() && code.len() <= 8 && code.chars().all(|c| c.is_ascii_alphanumeric()) {
                    codes.push(format!("{}{code}", &sev[..1]));
                }
            }
        }
    }
    codes.sort();
    codes
}

fn run_limited(mut cmd: Command) -> (Option<i32>, String, String) {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("spawn almide");
    let (mut so, mut se) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let to = std::thread::spawn(move || {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut so, &mut s).ok();
        s
    });
    let te = std::thread::spawn(move || {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut se, &mut s).ok();
        s
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().expect("wait") {
            break Some(st);
        }
        if start.elapsed() > PER_RUN_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (out, err) = (to.join().unwrap(), te.join().unwrap());
    match status {
        Some(st) => (st.code(), out, err),
        None => (Some(-999), "HANG".into(), String::new()),
    }
}

fn observe(kind: Kind, cwd: &Path, entry: &str) -> Outcome {
    let verb = match kind {
        Kind::Test => "test",
        Kind::Run => "run",
        Kind::Check => "check",
    };
    let mut cmd = Command::new(almide());
    cmd.current_dir(cwd).args([verb, entry]);
    let (exit, out, err) = run_limited(cmd);
    let mut codes = diag_codes(&out);
    codes.extend(diag_codes(&err));
    // `check` is the only verb that reports warnings on every route; ask it
    // too, so a warning that appears in one shape only is a divergence.
    if kind != Kind::Check {
        let mut c = Command::new(almide());
        c.current_dir(cwd).args(["check", entry]);
        let (_, o, e) = run_limited(c);
        codes.extend(diag_codes(&o).into_iter().chain(diag_codes(&e)).filter(|c| c.starts_with('w')));
    }
    codes.sort();
    codes.dedup();
    let body = if codes.iter().any(|c| c.starts_with('e')) {
        String::new()
    } else {
        let mut b: String = out
            .lines()
            // the route summary (`N via WASM, M via native fallback`) names the
            // leg, not the program's behaviour
            .filter(|l| !l.contains(" via WASM, "))
            .map(|l| format!("{l}\n"))
            .collect();
        if exit != Some(0) {
            b.push_str("--stderr--\n");
            b.push_str(&err);
        }
        b
    };
    Outcome { exit, codes, body }
}

fn load_skips() -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(repo().join("scripts/lib/dep-shaped-skips.txt")).expect("skip list");
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (id, why) = l.split_once(char::is_whitespace).unwrap_or((l, ""));
            (id.to_string(), why.trim().to_string())
        })
        .collect()
}

#[test]
fn every_multi_module_project_agrees_in_dependency_shape() {
    let scratch = tempfile::tempdir().expect("tempdir");
    let cases = discover(scratch.path());
    assert!(
        cases.len() >= CORPUS_FLOOR,
        "the dependency-shaped leg discovered {} case(s), below the floor {CORPUS_FLOOR}: discovery lost part of the corpus",
        cases.len()
    );
    let skips = load_skips();

    let results: Mutex<Vec<(String, Outcome, Outcome)>> = Mutex::new(Vec::new());
    let queue = Mutex::new(cases.iter());
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).min(6);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let Some(c) = queue.lock().unwrap().next() else { break };
                let d = observe(c.kind, &c.direct, &c.entry);
                let p = observe(c.kind, &c.dep, &c.entry);
                results.lock().unwrap().push((c.id.clone(), d, p));
            });
        }
    });
    let mut results = results.into_inner().unwrap();
    results.sort_by(|a, b| a.0.cmp(&b.0));

    let ids: BTreeSet<&str> = results.iter().map(|r| r.0.as_str()).collect();
    let mut failures = Vec::new();
    for (id, d, p) in &results {
        let agree = d == p;
        match (agree, skips.get(id)) {
            (true, Some(why)) => failures.push(format!(
                "STALE SKIP {id}: both shapes now agree — remove it from scripts/lib/dep-shaped-skips.txt ({why})"
            )),
            (false, None) => failures.push(format!(
                "DIVERGENCE {id}\n  direct: exit={:?} codes={:?}\n{}\n  dependency-shaped: exit={:?} codes={:?}\n{}",
                d.exit, d.codes, indent(&d.body), p.exit, p.codes, indent(&p.body)
            )),
            _ => {}
        }
    }
    for id in skips.keys() {
        if !ids.contains(id.as_str()) {
            failures.push(format!("STALE SKIP {id}: no longer a corpus case — remove it"));
        }
    }
    eprintln!(
        "dependency-shaped leg: {} case(s), {} skipped, {} failure(s) [{}]",
        results.len(),
        skips.len(),
        failures.len(),
        almide()
    );
    assert!(failures.is_empty(), "{}\n\n{} problem(s)", failures.join("\n\n"), failures.len());
}

fn indent(s: &str) -> String {
    s.lines().map(|l| format!("    | {l}")).collect::<Vec<_>>().join("\n")
}
