//! #1950: a native binary whose reader closes early (`prog | head`) must end
//! quietly under SIGPIPE, not panic with "failed printing to stdout: Broken
//! pipe" (exit 101). Rust's std ignores SIGPIPE at startup; the native `main`
//! wrapper restores the default disposition before the user's body runs, for
//! an effect main and a plain main alike.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn rustc_available() -> bool {
    Command::new("rustc").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Build `src`, run it with stdout piped, read two lines, close the pipe and
/// wait: the process must stop through SIGPIPE (never a panic exit).
fn assert_quiet_under_closed_pipe(name: &str, src: &str) {
    let dir = std::env::temp_dir().join(format!("almide-sigpipe-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let almd = dir.join(format!("{name}.almd"));
    std::fs::write(&almd, src).expect("write");
    let bin = dir.join(name);
    let build = Command::new(almide_bin())
        .args(["build", almd.to_str().unwrap(), "-o", bin.to_str().unwrap()])
        .output()
        .expect("spawn almide");
    assert!(build.status.success(), "build failed:\n{}", String::from_utf8_lossy(&build.stderr));

    let mut child = Command::new(&bin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn program");
    {
        let stdout = child.stdout.take().expect("stdout");
        let mut lines = BufReader::new(stdout).lines();
        assert_eq!(lines.next().unwrap().unwrap(), "1");
        assert_eq!(lines.next().unwrap().unwrap(), "2");
        // The reader goes away here, like `head` exiting.
    }
    let out = child.wait_with_output().expect("wait");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("panicked") && !stderr.contains("Broken pipe"),
        "{name}: the program panicked on the closed pipe:\n{stderr}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(out.status.signal(), Some(13), "{name}: expected SIGPIPE, got {:?}", out.status);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

const EFFECT_MAIN: &str =
    "effect fn main() -> Unit = { for i in list.range(1, 200001) { println(int.to_string(i)) } }\n";
const PLAIN_MAIN: &str =
    "fn main() -> Unit = { for i in list.range(1, 200001) { println(int.to_string(i)) } }\n";

#[test]
fn effect_main_stops_quietly_when_the_reader_closes() {
    if !rustc_available() {
        eprintln!("skipping: rustc not available");
        return;
    }
    assert_quiet_under_closed_pipe("pipe_effect", EFFECT_MAIN);
}

#[test]
fn plain_main_stops_quietly_when_the_reader_closes() {
    if !rustc_available() {
        eprintln!("skipping: rustc not available");
        return;
    }
    assert_quiet_under_closed_pipe("pipe_plain", PLAIN_MAIN);
}
