//! @contract: C-459
//! #4844 (C-459): a process command name runs the program the PATH the
//! program was launched with finds. The program below writes two executables
//! into a directory — one named `echo`, one named like nothing on the launch
//! PATH — puts only that directory on PATH with `env.set`, and runs both
//! names. The launch PATH's `echo` must answer, and the other name must not
//! be found, on native and on the embedded wasm host alike, with and without
//! a `[permissions] proc` list. Before, the native spawn resolved through the
//! PATH `env.set` wrote and ran both planted programs.
use std::path::Path;
use std::process::Command;

const PROGRAM: &str = r#"import fs
import env
import process

effect fn main() -> Unit = {
  let dir = "${fs.temp_dir()}/almide-c459-${process.pid()}"
  fs.mkdir_p(dir)!
  fs.write_text("${dir}/echo", "#!/bin/sh\nprintf PLANTED\n")!
  fs.write_text("${dir}/almide-c459-planted", "#!/bin/sh\nprintf PLANTED\n")!
  let _ = process.exec("chmod", ["+x", "${dir}/echo", "${dir}/almide-c459-planted"])!
  env.set("PATH", dir)!
  println(process.exec("echo", ["launch"]) ?? "echo refused")
  println(process.exec("almide-c459-planted", []) ?? "not found")
  fs.remove_all(dir)!
}
"#;

fn run(dir: &Path, wasm: bool) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.args(["run", "app.almd"]);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let out = cmd
        .current_dir(dir)
        .output()
        .expect("spawn almide");
    format!(
        "exit={:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[cfg(unix)]
#[test]
fn a_command_name_runs_what_the_launch_path_finds_on_both_legs() {
    let expected = "exit=Some(0)\nstdout:\nlaunch\n\nnot found\n";
    for manifest in [None, Some("[permissions]\nproc = [\"echo\", \"chmod\", \"almide-c459-planted\"]\n")] {
        let td = tempfile::tempdir().expect("tempdir");
        std::fs::write(td.path().join("app.almd"), PROGRAM).expect("write program");
        if let Some(m) = manifest {
            std::fs::write(td.path().join("almide.toml"), format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n{m}")).expect("write manifest");
        }
        for wasm in [false, true] {
            let got = run(td.path(), wasm);
            assert!(got.starts_with(expected), "wasm={wasm} with manifest {manifest:?}:\n{got}");
        }
    }
}
