//! #4214: when a program exits, every child it started with `process.start`
//! and has neither waited on nor stopped is stopped the way `process.stop`
//! stops it, before the exit status is reported — on each exit route: main
//! returns, `process.exit`, and an effect error ending main. A child already
//! waited on or stopped is left as it was, and the exit is neither delayed
//! to the child's own end nor turned into an error.
//!
//! Each program prints the pid of a `sleep 30` it started and leaves running;
//! after `almide run` returns, that pid must no longer exist.
#![cfg(unix)]
use std::process::Command;
use std::time::{Duration, Instant};

/// `ALMIDE_BIN` runs the cells against another build (A/B); the default is
/// this workspace's binary.
fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// `(stdout, stderr without the `[almide]` banner lines, exit code, wall time)`.
fn run(source: &str) -> (String, String, Option<i32>, Duration) {
    run_on(source, &[])
}

/// [`run`] with extra `almide run` flags (`--target wasm`: the embedded host).
fn run_on(source: &str, extra: &[&str]) -> (String, String, Option<i32>, Duration) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("exit_children.almd");
    std::fs::write(&file, source).expect("write source");
    let start = Instant::now();
    let out = Command::new(almide()).arg("run").arg(&file).args(extra).output().expect("run almide");
    let elapsed = start.elapsed();
    let stderr: String = String::from_utf8_lossy(&out.stderr)
        .lines()
        .filter(|l| !l.starts_with("[almide]"))
        .map(|l| format!("{l}\n"))
        .collect();
    (String::from_utf8_lossy(&out.stdout).into_owned(), stderr, out.status.code(), elapsed)
}

/// Whether a process `pid` exists (`kill -0`, the shell builtin).
fn alive(pid: i64) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("kill -0 {pid} 2>/dev/null"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The pid on the stdout line `<tag> <pid>`.
fn pid_of(stdout: &str, tag: &str) -> i64 {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(tag).and_then(|r| r.strip_prefix(' ')))
        .and_then(|p| p.trim().parse().ok())
        .unwrap_or_else(|| panic!("no `{tag} <pid>` line in stdout:\n{stdout}"))
}

/// Assert `pid` is gone. A child killed past its grace may still be a zombie
/// for a moment after its parent ended (init reaps it), so look for a while.
fn assert_gone(pid: i64, what: &str) {
    let until = Instant::now() + Duration::from_secs(5);
    while alive(pid) {
        if Instant::now() >= until {
            let _ = Command::new("sh").arg("-c").arg(format!("kill -9 {pid} 2>/dev/null")).status();
            panic!("{what}: the unwaited child {pid} outlived the program");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

const SLEEPER: &str = r#"process.ProcessCommand { cmd: "sleep", args: ["30"] }"#;

#[test]
fn an_unwaited_child_is_stopped_when_main_returns() {
    let (stdout, stderr, code, elapsed) = run(&format!(
        "import process\n\neffect fn main() -> Unit = {{\n  let c = process.start({SLEEPER})!\n  println(\"child \" + int.to_string(process.child_pid(c)!))\n}}\n"
    ));
    assert_eq!(code, Some(0), "stdout: {stdout}\nstderr: {stderr}");
    assert_eq!(stderr, "");
    assert_gone(pid_of(&stdout, "child"), "main returns");
    assert!(elapsed < Duration::from_secs(20), "the exit waited for the child: {elapsed:?}");
}

#[test]
fn an_unwaited_child_is_stopped_by_process_exit() {
    let (stdout, stderr, code, elapsed) = run(&format!(
        "import process\n\neffect fn main() -> Unit = {{\n  let c = process.start({SLEEPER})!\n  println(\"child \" + int.to_string(process.child_pid(c)!))\n  process.exit(3)\n}}\n"
    ));
    assert_eq!(code, Some(3), "stdout: {stdout}\nstderr: {stderr}");
    assert_eq!(stderr, "");
    assert_gone(pid_of(&stdout, "child"), "process.exit");
    assert!(elapsed < Duration::from_secs(20), "the exit waited for the child: {elapsed:?}");
}

#[test]
fn an_unwaited_child_is_stopped_when_an_effect_error_ends_main() {
    let (stdout, stderr, code, elapsed) = run(&format!(
        "import process\n\neffect fn fail() -> Int = err(\"boom\")\n\neffect fn main() -> Unit = {{\n  let c = process.start({SLEEPER})!\n  println(\"child \" + int.to_string(process.child_pid(c)!))\n  let n = fail()!\n  println(int.to_string(n))\n}}\n"
    ));
    assert_eq!(code, Some(1), "stdout: {stdout}\nstderr: {stderr}");
    assert_eq!(stderr, "Error: boom\n");
    assert_gone(pid_of(&stdout, "child"), "effect error");
    assert!(elapsed < Duration::from_secs(20), "the exit waited for the child: {elapsed:?}");
}

#[test]
fn children_already_waited_on_or_stopped_are_left_alone() {
    let (stdout, stderr, code, _) = run(&format!(
        "import process\n\neffect fn main() -> Unit = {{\n  let done = process.start(process.ProcessCommand {{ cmd: \"sh\", args: [\"-c\", \"exit 4\"] }})!\n  println(\"waited \" + int.to_string(process.wait(done)!.code))\n  let stopped = process.start({SLEEPER})!\n  println(\"stopped \" + int.to_string(process.child_pid(stopped)!))\n  let st = process.stop(stopped, 200)!\n  println(\"stop code \" + int.to_string(st.code))\n  let c = process.start({SLEEPER})!\n  println(\"child \" + int.to_string(process.child_pid(c)!))\n}}\n"
    ));
    assert_eq!(code, Some(0), "stdout: {stdout}\nstderr: {stderr}");
    assert_eq!(stderr, "");
    assert_eq!(pid_of(&stdout, "waited"), 4, "{stdout}");
    assert_eq!(pid_of(&stdout, "stop code"), -1, "{stdout}");
    assert_gone(pid_of(&stdout, "stopped"), "an explicitly stopped child");
    assert_gone(pid_of(&stdout, "child"), "main returns after a wait and a stop");
}

/// C-437's fixture observes itself: run with its own path as the argument it
/// runs that binary as the parent once per exit route and reports whether the child the
/// parent left running still exists after the parent ended.
#[test]
fn the_c437_fixture_sees_every_route_stop_the_child() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/native_only/process_start_exit_stops_children.almd");
    let dir = tempfile::tempdir().expect("tempdir");
    let bin = dir.path().join("process_start_exit_stops_children");
    let build = Command::new(almide()).arg("build").arg(fixture).arg("-o").arg(&bin).output().expect("build");
    assert!(build.status.success(), "build failed: {}", String::from_utf8_lossy(&build.stderr));
    // Timed apart from the build: an exit that waited for its `sleep 30`
    // would also find the child gone afterwards, so the clock is the witness.
    let start = Instant::now();
    let out = Command::new(&bin).arg(&bin).output().expect("run the fixture");
    let elapsed = start.elapsed();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stdout: {stdout}\nstderr: {stderr}");
    assert_eq!(
        stdout,
        "return: exit 0, stderr [], child running at exit, then stopped\n\
         exit: exit 3, stderr [], child running at exit, then stopped\n\
         error: exit 1, stderr [Error: boom], child running at exit, then stopped\n",
        "stderr: {stderr}"
    );
    assert!(elapsed < Duration::from_secs(20), "an exit waited for its child: {elapsed:?}");
}

/// #4347 / #4348: the embedded wasm host (`almide run --target wasm`) stops
/// the run's unwaited children at its end on every route native does, with
/// the same exit code and stderr.
#[test]
fn the_embedded_wasm_host_stops_an_unwaited_child_on_every_exit_route() {
    let routes = [
        ("return", "", "", Some(0), ""),
        ("exit", "", "  process.exit(3)\n", Some(3), ""),
        ("error", "effect fn fail() -> Int = err(\"boom\")\n\n", "  let n = fail()!\n  println(int.to_string(n))\n", Some(1), "Error: boom\n"),
    ];
    for (route, head, tail, want_code, want_err) in routes {
        let (stdout, stderr, code, elapsed) = run_on(
            &format!(
                "import process\n\n{head}effect fn main() -> Unit = {{\n  let c = process.start({SLEEPER})!\n  println(\"child \" + int.to_string(process.child_pid(c)!))\n{tail}}}\n"
            ),
            &["--target", "wasm"],
        );
        assert_eq!(code, want_code, "{route}: stdout: {stdout}\nstderr: {stderr}");
        assert_eq!(stderr, want_err, "{route}");
        assert_gone(pid_of(&stdout, "child"), &format!("wasm leg, {route}"));
        assert!(elapsed < Duration::from_secs(20), "{route}: the exit waited for the child: {elapsed:?}");
    }
}

/// The C-445 fixture on both legs: run with a route and `pid`, it prints the
/// pid of the child it leaves running; after `almide run` ends the pid must
/// be gone, native and embedded wasm alike.
#[test]
fn the_embedded_cross_fixture_sees_both_legs_stop_the_child() {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/embedded_cross/process_start_run_end_stops_children.almd");
    for leg in [&[][..], &["--target", "wasm"][..]] {
        for (route, want_code) in [("return", Some(0)), ("exit", Some(3)), ("error", Some(1))] {
            let start = Instant::now();
            let out = Command::new(almide()).arg("run").arg(fixture).args(leg).arg("--").arg(route).arg("pid").output().expect("run almide");
            let elapsed = start.elapsed();
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert_eq!(out.status.code(), want_code, "{leg:?} {route}: stdout: {stdout}\nstderr: {}", String::from_utf8_lossy(&out.stderr));
            assert!(stdout.contains("child running: true"), "{leg:?} {route}: {stdout}");
            assert_gone(pid_of(&stdout, "pid"), &format!("{leg:?} {route}"));
            assert!(elapsed < Duration::from_secs(60), "{leg:?} {route}: the exit waited for the child: {elapsed:?}");
        }
    }
}
