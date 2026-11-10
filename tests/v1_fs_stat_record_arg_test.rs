//! `fs.stat`'s result handed to a `FileStat` / `Result[FileStat, String]`
//! parameter on the native render. The host call's result was the four fields
//! as a slot list — enough for field reads, but not the record a parameter of
//! the declared type takes, so `show_stat(dir, fs.stat(p))` was refused ("arg
//! of another type (Rec(4)) to a record param (RecRef(1))",
//! `spec/embedded_cross/fs_stat_glob_structural.almd`). The result is now the
//! program's generated `FileStat` record whenever its labels and kinds are the
//! runtime struct's.
use std::process::Command;

/// The stat record as a direct argument, inside a Result argument, through a
/// list of Results — beside a user record that is also named `FileStat`.
const STAT_ARGS: &str = r#"import fs

type FileStat = { name: String, n: Int }

fn mine(s: FileStat) -> String = "${s.name}/${s.n}"

fn theirs(s: fs.FileStat) -> String = "dir=${s.is_dir} file=${s.is_file}"

fn both(r: Result[fs.FileStat, String], m: FileStat) -> String = match r {
  ok(s) => theirs(s) + " " + mine(m),
  err(e) => "err " + mine(m),
}

effect fn main() -> Unit = {
  let m = FileStat { name: "x", n: 3 }
  println(mine(m))
  println(theirs(fs.stat(".")!))
  println(both(fs.stat("."), m))
  println(both(fs.stat("/definitely-not-here"), FileStat { name: "y", n: 4 }))
  let stats = [".", ".."] |> list.map((p) => fs.stat(p))
  for r in stats {
    println(both(r, m))
  }
}
"#;

const STAT_ARGS_STDOUT: &str = "x/3\ndir=true file=false\ndir=true file=false x/3\nerr y/4\ndir=true file=false x/3\ndir=true file=false x/3\n";

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| format!("{e:?}")))
        .expect("spawn")
        .join()
        .expect("render thread")
}

#[test]
fn a_stat_result_is_the_declared_record_a_parameter_takes() {
    let out = render(STAT_ARGS).unwrap_or_else(|e| panic!("the stat argument walls: {e}"));
    assert!(!out.contains("vec![s.size"), "the slot-list form is not used beside a declared record");
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("stat_args.almd");
    std::fs::write(&file, STAT_ARGS).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra).current_dir(dir.path()).output().expect("almide run");
        assert!(run.status.success(), "{extra:?}: {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout), STAT_ARGS_STDOUT, "{extra:?}");
    }
}

#[test]
fn the_stat_and_glob_fixture_renders_on_the_native_route() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/embedded_cross/fs_stat_glob_structural.almd");
    let src = std::fs::read_to_string(path).expect("fixture");
    render(&src).unwrap_or_else(|e| panic!("the fixture walls: {e}"));
}
