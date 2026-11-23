//! proc.command-path-resolution (#4844, C-459): a bare command name runs the
//! program the PATH the program was LAUNCH with finds; `env.set("PATH", ..)`
//! never changes which program a name runs. The names here are fresh — no
//! test author listed them — and the directory they are planted in is fresh:
//! a program that puts that directory on PATH and runs the planted name must
//! not find it, on native and on the embedded wasm host, with and without a
//! `[permissions] proc` list naming it. The control launches the same program
//! with the directory already on the launch PATH, where the planted name must
//! run, so the refusal is not an unrunnable plant.
#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

#[path = "support/novel.rs"]
mod novel;

fn plant(dir: &Path, name: &str, says: &str) {
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\nprintf '{says}\\n'\n")).expect("write plant");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod plant");
}

fn run(proj: &Path, wasm: bool, launch_path: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.args(["run", "src/main.almd"]).current_dir(proj);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    if let Some(p) = launch_path {
        cmd.env("PATH", p);
    }
    cmd.output().expect("spawn almide run")
}

fn text(o: &Output) -> String {
    format!("exit={:?}\nstdout:\n{}\nstderr:\n{}", o.status.code(), String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
fn novel_a_name_planted_on_a_path_the_program_set_is_not_found() {
    let t = tempfile::tempdir().expect("scratch");
    let dir = t.path().join(novel::path_segment());
    std::fs::create_dir(&dir).expect("mkdir plant dir");
    let name = novel::ident("planted");
    let marker = novel::ident("ran");
    plant(&dir, &name, &marker);
    let program = format!(
        "import env\nimport process\n\neffect fn main() -> Unit = {{\n  env.set(\"PATH\", \"{dir}\")!\n  match process.exec(\"{name}\", []) {{\n    ok(out) => println(out),\n    err(_) => println(\"not found\"),\n  }}\n}}\n",
        dir = dir.display()
    );
    for manifest in [String::new(), format!("\n[permissions]\nproc = [\"{name}\"]\n")] {
        let proj = t.path().join(novel::path_segment());
        std::fs::create_dir_all(proj.join("src")).expect("mkdir proj");
        std::fs::write(proj.join("almide.toml"), format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n{manifest}")).expect("write manifest");
        std::fs::write(proj.join("src/main.almd"), &program).expect("write program");
        for wasm in [false, true] {
            let o = run(&proj, wasm, None);
            let got = text(&o);
            assert!(o.status.success(), "wasm={wasm} manifest={manifest:?}:\n{got}");
            assert!(String::from_utf8_lossy(&o.stdout).contains("not found"), "wasm={wasm} manifest={manifest:?}: the set PATH was searched:\n{got}");
            assert!(!got.contains(&marker), "wasm={wasm} manifest={manifest:?}: the planted program ran:\n{got}");
        }
        // Control: the same directory on the LAUNCH PATH finds the plant.
        let launch = format!("{}:{}", dir.display(), std::env::var("PATH").unwrap_or_default());
        let o = run(&proj, false, Some(&launch));
        assert!(String::from_utf8_lossy(&o.stdout).contains(&marker), "control: the launch PATH did not find the plant:\n{}", text(&o));
    }
}
