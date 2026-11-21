//! Cross-target evidence for the `args` parsing rules (#4144, C-181): a long
//! flag name derives no short form (`-d` sets neither `--dry-run` nor
//! `--delete`), a one-letter name is the declared short form, and `--` ends
//! option parsing (everything after it is positional, never a flag or an
//! option), and `positional(options)` leaves out the value after each named
//! `--option` (`--out report.txt input.txt` is `input.txt`). Each case runs the built program with real argv on native, on the
//! embedded wasm host (`almide run --target wasm`) and on a stock `wasmtime
//! run` of the built artifact, and asserts the exact expected output on each.
//!
//! Argv-reading programs cannot live in the spec suites (the harness passes no
//! per-file arguments), so this is the executable evidence.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn stdout_of(mut cmd: Command) -> String {
    let out = cmd.current_dir(repo_root()).output().expect("spawn");
    assert!(
        out.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn build(file: &Path, out: &Path, wasm: bool) {
    let mut c = Command::new(almide());
    c.args(["build", file.to_str().unwrap(), "-o", out.to_str().unwrap()]);
    if wasm {
        c.args(["--target", "wasm"]);
    }
    let st = c.current_dir(repo_root()).status().expect("build");
    assert!(st.success(), "build failed (wasm={wasm})");
}

const PROGRAM: &str = r#"import args

effect fn main() -> Unit = {
  println("dry_run=${args.has_flag("dry-run")!} delete=${args.has_flag("delete")!} d=${args.has_flag("d")!}")
  println("out=${args.option("out")! ?? "none"}")
  println("positional=${list.join(args.positional(["out"])!, ",")}")
  println("unnamed=${list.join(args.positional([])!, ",")}")
}
"#;

/// (argv, expected stdout)
const CASES: &[(&[&str], &str)] = &[
    // No derived short form: `-d` is neither --dry-run nor --delete; only the
    // one-letter name "d" answers to it.
    (
        &["-d"],
        "dry_run=false delete=false d=true\nout=none\npositional=\nunnamed=\n",
    ),
    (
        &["--dry-run"],
        "dry_run=true delete=false d=false\nout=none\npositional=\nunnamed=\n",
    ),
    // `--` ends option parsing: what follows is positional, never a flag.
    (
        &["--", "-d", "--delete"],
        "dry_run=false delete=false d=false\nout=none\npositional=-d,--delete\nunnamed=-d,--delete\n",
    ),
    (
        &["--dry-run", "--", "--delete", "x"],
        "dry_run=true delete=false d=false\nout=none\npositional=--delete,x\nunnamed=--delete,x\n",
    ),
    // ...and never an option: `--out` after `--` is not read, nor is a value
    // that would follow a trailing `--out` across the marker.
    (
        &["in.txt", "--", "--out=z"],
        "dry_run=false delete=false d=false\nout=none\npositional=in.txt,--out=z\nunnamed=in.txt,--out=z\n",
    ),
    (
        &["--out", "--", "z"],
        "dry_run=false delete=false d=false\nout=none\npositional=z\nunnamed=z\n",
    ),
    (
        &["--out=r.txt", "in.txt", "--", "--", "-"],
        "dry_run=false delete=false d=false\nout=r.txt\npositional=in.txt,--,-\nunnamed=in.txt,--,-\n",
    ),
    // A named option's value is not positional (#4144), wherever it stands;
    // with no names the value is an ordinary argument.
    (
        &["--out", "report.txt", "input.txt"],
        "dry_run=false delete=false d=false\nout=report.txt\npositional=input.txt\nunnamed=report.txt,input.txt\n",
    ),
    (
        &["input.txt", "--out", "report.txt", "second.txt"],
        "dry_run=false delete=false d=false\nout=report.txt\npositional=input.txt,second.txt\nunnamed=input.txt,report.txt,second.txt\n",
    ),
    // A flag is not an option: the argument after it stays positional.
    (
        &["--dry-run", "input.txt"],
        "dry_run=true delete=false d=false\nout=none\npositional=input.txt\nunnamed=input.txt\n",
    ),
];

#[test]
fn args_parse_rules_hold_on_native_and_both_wasm_legs() {
    run_cases(PROGRAM, CASES);
}

/// #4866: every reader treats a one-letter name the same way. `has_flag("o")`
/// answered to `-o` while `option("o")` did not read `-o v`, and
/// `positional(["o"])` kept the value. And a name written with its dashes is
/// an err naming the dash-free form, whatever argv holds; it was silently
/// never matched. Each case failed on the old stdlib.
const SHORT_PROGRAM: &str = r#"import args

fn shown(r: Result[String, String]) -> String = match r {
  ok(v) => "ok ${v}",
  err(e) => "err ${e}",
}

effect fn main() -> Unit = {
  println("o=${args.option("o")! ?? "none"} flag_o=${args.has_flag("o")!} out=${args.option("out")! ?? "none"}")
  println("positional=${list.join(args.positional(["o", "out"])!, ",")}")
  let dashed: Result[String?, String] = args.option("--out")
  println(shown(dashed |> result.map((v) => v ?? "none")))
  let short_dashed: Result[Bool, String] = args.has_flag("-o")
  println(shown(short_dashed |> result.map((b) => "${b}")))
  let names: Result[List[String], String] = args.positional(["out", "--o"])
  println(shown(names |> result.map((v) => list.join(v, ","))))
  let empty: Result[String?, String] = args.option("")
  println(shown(empty |> result.map((v) => v ?? "none")))
}
"#;

const SHORT_ERRS: &str = "err args.option: write the name without its dashes: \"out\"
err args.has_flag: write the name without its dashes: \"o\"
err args.positional: write the name without its dashes: \"o\"
err args.option: the name is empty: write it without dashes, as in \"out\"
";

const SHORT_CASES: &[(&[&str], &str)] = &[
    (&["-o", "v", "in.txt"], "o=v flag_o=true out=none\npositional=in.txt\n"),
    (&["-o=w", "in.txt"], "o=w flag_o=false out=none\npositional=in.txt\n"),
    (&["--o", "v", "--out", "r", "x"], "o=v flag_o=true out=r\npositional=x\n"),
    // `--out` given on the command line still does not match the dashed name.
    (&["--out=r"], "o=none flag_o=false out=r\npositional=\n"),
    (&["--", "-o", "v"], "o=none flag_o=false out=none\npositional=-o,v\n"),
];

#[test]
fn one_letter_names_read_alike_and_dashed_names_are_refused_on_native_and_both_wasm_legs() {
    let cases: Vec<(&[&str], String)> = SHORT_CASES.iter().map(|(argv, want)| (*argv, format!("{want}{SHORT_ERRS}"))).collect();
    let cases: Vec<(&[&str], &str)> = cases.iter().map(|(argv, want)| (*argv, want.as_str())).collect();
    run_cases(SHORT_PROGRAM, &cases);
}

/// Build `program` natively and for wasm, then run every case on the native
/// binary, the embedded wasm host and (when on PATH) a stock wasmtime.
fn run_cases(program: &str, cases: &[(&[&str], &str)]) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("args_parse.almd");
    std::fs::write(&file, program).expect("write program");
    let native_bin = dir.path().join("args_parse");
    build(&file, &native_bin, false);
    let stock = wasmtime_available();
    let wasm_file = dir.path().join("args_parse.wasm");
    if stock {
        build(&file, &wasm_file, true);
    } else {
        eprintln!("SKIP stock-p1 leg: wasmtime not on PATH — enforced on Linux CI");
    }

    for (argv, want) in cases {
        let mut n = Command::new(&native_bin);
        n.args(*argv);
        assert_eq!(stdout_of(n), *want, "native, argv {argv:?}");

        let mut e = Command::new(almide());
        e.args(["run", file.to_str().unwrap(), "--target", "wasm", "--"])
            .args(*argv);
        assert_eq!(stdout_of(e), *want, "embedded wasm, argv {argv:?}");

        if stock {
            let mut w = Command::new("wasmtime");
            w.args(["run", wasm_file.to_str().unwrap()]).args(*argv);
            assert_eq!(stdout_of(w), *want, "stock wasm, argv {argv:?}");
        }
    }
}
