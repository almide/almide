//! #2485: `env.args()` is argv[1..] VERBATIM on every leg. A `--` among the
//! program's arguments belongs to the program: `almide run app.almd -- a -- b`
//! consumes its own separator (clap) and forwards `a -- b`, and a built binary
//! run as `./p a -- b` sees exactly that. The native runtime used to search
//! argv for a `--` and drop everything through it — a leftover from a launch
//! shape that no longer exists — so `./p a -- b` answered `["b"]` where the
//! wasm legs (WASI args_get, argv[0] skipped) answered `["a", "--", "b"]`.
//! C-118's statement ("only the real program args") is the promise this pins.
use std::path::Path;
use std::process::Command;

const SOURCE: &str = r#"import env
effect fn main() -> Unit = {
  println("${env.args()}")
}
"#;

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

fn stdout_of(mut cmd: Command) -> String {
    let out = cmd.output().expect("spawn");
    assert!(
        out.status.success(),
        "{:?} exited {:?}: {}",
        cmd,
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

#[test]
fn a_program_argument_spelled_double_dash_reaches_env_args_on_every_leg() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("args.almd");
    std::fs::write(&source, SOURCE).expect("source");
    let want = "[\"a\", \"--\", \"b\"]\n";

    // `almide run`: the CLI's own `--` is consumed, the program's is forwarded.
    let mut run = almide();
    run.arg("run").arg(&source).args(["--", "a", "--", "b"]);
    assert_eq!(stdout_of(run), want, "almide run -- a -- b (native)");

    // A built binary sees its argv with nothing removed.
    let native = dir.path().join("native");
    let mut build = almide();
    build.arg("build").arg(&source).arg("-o").arg(&native);
    stdout_of(build);
    let mut direct = Command::new(&native);
    direct.args(["a", "--", "b"]);
    assert_eq!(stdout_of(direct), want, "./p a -- b");
    let mut leading = Command::new(&native);
    leading.args(["--", "x"]);
    assert_eq!(stdout_of(leading), "[\"--\", \"x\"]\n", "./p -- x keeps the leading separator");
    let mut bare = Command::new(&native);
    bare.args(["--"]);
    assert_eq!(stdout_of(bare), "[\"--\"]\n", "./p -- is one argument, not none");
    assert_eq!(stdout_of(Command::new(&native)), "[]\n", "./p alone");

    if !wasmtime_available() {
        eprintln!("wasmtime not on PATH — the wasm legs of this test did not run");
        return;
    }
    let mut run_wasm = almide();
    run_wasm.arg("run").arg(&source).args(["--target", "wasm", "--", "a", "--", "b"]);
    assert_eq!(stdout_of(run_wasm), want, "almide run --target wasm -- a -- b (embedded host)");

    let core = dir.path().join("core.wasm");
    let mut build_wasm = almide();
    build_wasm.arg("build").arg(&source).args(["--target", "wasm"]).arg("-o").arg(&core);
    stdout_of(build_wasm);
    let mut stock = Command::new("wasmtime");
    stock.arg("run").arg(Path::new(&core)).args(["a", "--", "b"]);
    assert_eq!(stdout_of(stock), want, "wasmtime core.wasm a -- b (stock p1)");
}
