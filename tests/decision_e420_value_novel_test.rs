//! scope.e420-local-fn (#4904): a `local fn` is reachable only from its own
//! file, by every route that resolves it — a call `m.f(x)`, the fn named as a
//! value (`let g = m.f`, `list.map(xs, m.f)`), and a `local fn Type.method`
//! called as a method from another module. The module, the fns and the type
//! are fresh names, so no list in the compiler can know them. The control is
//! the same project with the fns public: every program checks and runs.
use std::path::Path;
use std::process::{Command, Output};

#[path = "support/novel.rs"]
mod novel;

struct Names {
    m: String,
    f: String,
    ty: String,
    meth: String,
}

fn names() -> Names {
    // `Box<hex>`: a fresh UpperCamel type name.
    let ty = format!("Box{}", novel::ident("t").trim_start_matches("t_"));
    Names { m: novel::module(), f: novel::ident("secret"), ty, meth: novel::ident("peek") }
}

/// A project whose module `m` declares `f` and `Type.meth` with `vis`
/// (`local` or empty) and whose entry has `body` as `main`.
fn project(dir: &Path, n: &Names, vis: &str, body: &str) {
    std::fs::create_dir_all(dir.join("src")).expect("mkdir src");
    std::fs::write(dir.join("almide.toml"), "[package]\nname = \"p\"\nversion = \"0.1.0\"\n").expect("toml");
    let module = format!(
        "{vis} fn {f}(x: Int) -> Int = x + 1\ntype {ty} = {{ v: Int }}\n{vis} fn {ty}.{meth}(b: {ty}) -> Int = b.v\nfn make() -> {ty} = {ty} {{ v: 3 }}\n",
        vis = vis,
        f = n.f,
        ty = n.ty,
        meth = n.meth,
    );
    std::fs::write(dir.join("src").join(format!("{}.almd", n.m)), module.trim_start()).expect("module");
    std::fs::write(dir.join("src/main.almd"), format!("import self.{m}\n\neffect fn main() -> Unit = {{\n{body}\n}}\n", m = n.m)).expect("main");
}

fn almide(dir: &Path, sub: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide")).args([sub, "src/main.almd"]).current_dir(dir).output().expect("spawn almide")
}

fn text(o: &Output) -> String {
    format!("exit={:?}\nstdout:\n{}\nstderr:\n{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// Each route that names the fns from the entry module, with what it prints.
fn bodies(n: &Names) -> Vec<(String, &'static str)> {
    let (m, f, meth) = (&n.m, &n.f, &n.meth);
    vec![
        (format!("  println(\"${{{m}.{f}(1)}}\")"), "2\n"),
        (format!("  let g = {m}.{f}\n  println(\"${{g(1)}}\")"), "2\n"),
        (format!("  println(\"${{[1, 2] |> list.map({m}.{f})}}\")"), "[2, 3]\n"),
        (format!("  println(\"${{{m}.make().{meth}()}}\")"), "3\n"),
    ]
}

#[test]
fn novel_a_local_fn_is_refused_on_every_route_from_another_module() {
    let n = names();
    for (body, _) in bodies(&n) {
        let t = tempfile::tempdir().expect("scratch");
        project(t.path(), &n, "local", &body);
        let o = almide(t.path(), "check");
        let txt = text(&o);
        assert!(!o.status.success() && txt.contains("E420"), "expected E420 for:\n{body}\n{txt}");
    }
}

#[test]
fn novel_control_the_same_fns_public_check_and_run() {
    let n = names();
    for (body, want) in bodies(&n) {
        let t = tempfile::tempdir().expect("scratch");
        project(t.path(), &n, "", &body);
        let o = almide(t.path(), "run");
        let txt = text(&o);
        assert!(o.status.success(), "the public control must run:\n{body}\n{txt}");
        assert_eq!(String::from_utf8_lossy(&o.stdout), want, "{body}\n{txt}");
    }
}
