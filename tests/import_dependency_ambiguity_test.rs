//! @contract: C-458
//! #4865 (C-458): an import whose first segment names both a module of the
//! project and a `[dependencies]` entry is refused with E121. `import pkg.sub`
//! used to take the local `src/pkg/` while `import pkg` took the dependency,
//! so one name loaded from two places depending on which imports were
//! written. The control: a name that is only local, or only a dependency,
//! resolves as before.
use std::path::Path;
use std::process::{Command, Output};

fn write(p: &Path, s: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, s).unwrap();
}

fn check(dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_almide")).args(["check", "src/main.almd"]).current_dir(dir).output().unwrap()
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

/// A project depending on `name` (a path dependency with a root and a `sub`
/// module); `local` adds the same name under `src/` as a directory or a file.
fn project(root: &Path, name: &str, local: Option<&str>, main: &str) {
    write(&root.join("almide.toml"), &format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n{name} = {{ path = \"deps/{name}\" }}\n"));
    write(&root.join(format!("deps/{name}/almide.toml")), &format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"));
    write(&root.join(format!("deps/{name}/src/mod.almd")), "pub fn root() -> String = \"DEP root\"\n");
    write(&root.join(format!("deps/{name}/src/sub.almd")), "pub fn which() -> String = \"DEP sub\"\n");
    match local {
        Some("dir") => write(&root.join(format!("src/{name}/sub.almd")), "pub fn which() -> String = \"LOCAL sub\"\n"),
        Some("file") => write(&root.join(format!("src/{name}.almd")), "pub fn root() -> String = \"LOCAL root\"\n"),
        _ => {}
    }
    write(&root.join("src/main.almd"), main);
}

#[test]
fn a_name_that_is_both_local_and_a_dependency_is_refused_in_every_import_form() {
    let name = "zq_shared";
    let forms = [
        format!("import {name}.sub\n\neffect fn main() -> Unit = println(sub.which())\n"),
        format!("import {name}\nimport {name}.sub\n\neffect fn main() -> Unit = println({name}.root() + sub.which())\n"),
        format!("import {name}.sub as s\n\neffect fn main() -> Unit = println(s.which())\n"),
        format!("import {name}\n\neffect fn main() -> Unit = println({name}.root())\n"),
    ];
    for local in ["dir", "file"] {
        for main in &forms {
            let td = tempfile::tempdir().unwrap();
            project(td.path(), name, Some(local), main);
            let o = check(td.path());
            assert!(!o.status.success(), "{local} + {main:?} was accepted:\n{}", text(&o));
            assert!(text(&o).contains("error[E121]") && text(&o).contains("rename one of them"), "{local} + {main:?}:\n{}", text(&o));
        }
    }
    // The control: without the local module every form resolves to the dependency.
    for main in &forms {
        let td = tempfile::tempdir().unwrap();
        project(td.path(), name, None, main);
        let o = check(td.path());
        assert!(o.status.success(), "{main:?} without a local module:\n{}", text(&o));
    }
}
