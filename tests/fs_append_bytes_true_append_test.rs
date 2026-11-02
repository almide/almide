//! #4382 (C-447): `fs.append_bytes` is a true append on every leg, so
//! appending k bytes costs k, whatever the file already holds.
//!
//! It used to be `fs.read_bytes` + concat + `fs.write_bytes` on native and
//! wasm alike: each append read and rewrote the whole file (O(file size),
//! quadratic over a logging loop) and lost a concurrent writer's bytes.
//!
//! Two checks, each with its control:
//!
//! - **Structure.** The wasm program of one `fs.append_bytes` call reaches
//!   exactly one host op, 66 (the append under its own call name), and no
//!   read or exists op. Control: the old read + write composition, written
//!   out as user code, reaches the read and write ops — so the check can
//!   tell the two apart.
//! - **Cost independent of the file size, on every leg.** Sparse files of 0
//!   bytes, 1 MiB and one byte past the 1 GiB whole-file read limit (C-431)
//!   each take the same appends — three bytes, then 200 KiB (past the p3
//!   adapter's 64 KiB stream slice) — and grow by exactly that, on native,
//!   the embedded wasm host, the stock p1 artifact under wasmtime and the
//!   WASI 0.3 component. An append that read the file first could not answer
//!   ok on the largest one: the control line reads it whole and errs
//!   `File too large` on the same legs. The appended bytes are read back
//!   from the 1 MiB file and compared.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;

const PROBE: &str = r#"import fs

fn res(name: String, r: Result[Unit, String]) -> Unit = println(match r {
  ok(_) => "${name}=ok",
  err(m) => "${name}=err(${m})",
})

effect fn main() -> Unit = {
  let tail = bytes.repeat(bytes.from_list([1, 2, 3, 4, 5, 6, 7, 251]), 25600)
  for p in ["f0.bin", "f1.bin", "f2.bin"] {
    let before = fs.stat(p)!.size
    res("${p} append3", fs.append_bytes(p, bytes.from_list([7, 8, 9])))
    res("${p} append200k", fs.append_bytes(p, tail))
    println("${p} grew=${fs.stat(p)!.size - before}")
  }
  println(match fs.read_bytes("f2.bin") {
    ok(b) => "control f2 read=ok(${bytes.len(b)})",
    err(m) => "control f2 read=err(${m})",
  })
  let f1 = fs.read_bytes("f1.bin")!
  let n = bytes.len(f1)
  let got = bytes.slice(f1, n - 204803, n)
  var want = bytes.from_list([7, 8, 9])
  bytes.append_bytes(want, tail)
  println("f1 tail_same=${got == want} zeros_before=${bytes.get(f1, n - 204804) ?? -1}")
}
"#;

/// One byte past the whole-file read limit (C-431).
const PAST_READ_LIMIT: u64 = (1 << 30) + 1;

fn almide_bin() -> String {
    env!("CARGO_BIN_EXE_almide").to_string()
}

fn sparse(dir: &Path, name: &str, len: u64) {
    let f = std::fs::File::create(dir.join(name)).expect("create");
    f.set_len(len).expect("set_len");
}

fn fresh(dir: &Path) {
    sparse(dir, "f0.bin", 0);
    sparse(dir, "f1.bin", 1 << 20);
    sparse(dir, "f2.bin", PAST_READ_LIMIT);
}

fn checked(o: std::process::Output, what: &str) -> String {
    assert!(o.status.success(), "{what}: exit {:?}\n{}", o.status.code(), String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).expect("utf8 stdout")
}

fn wasmtime_major() -> Option<u32> {
    let o = Command::new("wasmtime").arg("--version").output().ok()?;
    String::from_utf8_lossy(&o.stdout).split_whitespace().nth(1)?.split('.').next()?.parse().ok()
}

#[test]
fn append_bytes_reaches_only_the_append_op() {
    let ops = |src: &str| {
        let ir = almide::wasm_leg::lower_to_ir("t.almd", src).expect("lowers on the wasm leg");
        almide_wasm::emit_program_with_ops(&ir).expect("emits").1
    };
    let append = ops("import fs\neffect fn main() -> Unit = fs.append_bytes(\"a.bin\", bytes.from_list([1, 2]))!\n");
    assert_eq!(append.into_iter().collect::<Vec<_>>(), vec![66], "fs.append_bytes must send the append op alone");
    // Control: the old composition reads the file and rewrites it.
    let old = ops(
        "import fs\neffect fn main() -> Unit = {\n  let p = \"a.bin\"\n  var all = if fs.exists(p) then fs.read_bytes(p)! else bytes.new()\n  bytes.append_bytes(all, bytes.from_list([1, 2]))\n  fs.write_bytes(p, all)!\n}\n",
    );
    assert!(old.contains(&15), "control: the composition writes with op 15, got {old:?}");
    assert!(old.iter().any(|op| matches!(op, 14 | 63)), "control: the composition reads the file, got {old:?}");
    assert!(!old.contains(&66), "control: the composition sends no append op, got {old:?}");
}

#[test]
fn appending_does_not_read_the_file_on_every_leg() {
    let t = tempfile::tempdir().expect("scratch");
    let dir = std::fs::canonicalize(t.path()).expect("canonical");
    std::fs::write(dir.join("probe.almd"), PROBE).unwrap();
    let run = |args: &[&str]| {
        fresh(&dir);
        let o = Command::new(almide_bin()).arg("run").arg("probe.almd").args(args).current_dir(&dir).env("PWD", &dir).output();
        checked(o.expect("spawn almide"), &format!("almide run {args:?}"))
    };

    let native = run(&[]);
    let grew = 3 + 204800;
    let expected = format!(
        "f0.bin append3=ok\nf0.bin append200k=ok\nf0.bin grew={grew}\n\
         f1.bin append3=ok\nf1.bin append200k=ok\nf1.bin grew={grew}\n\
         f2.bin append3=ok\nf2.bin append200k=ok\nf2.bin grew={grew}\n"
    );
    assert!(native.starts_with(&expected), "native:\n{native}");
    assert!(
        native.contains("control f2 read=err(fs.read_bytes(\"f2.bin\"): File too large"),
        "control: the largest file must be past the read limit on native:\n{native}"
    );
    assert!(native.ends_with("f1 tail_same=true zeros_before=0\n"), "native:\n{native}");

    assert_eq!(run(&["--target", "wasm"]), native, "the embedded wasm host differs from native");

    let Some(major) = wasmtime_major() else {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime is not on PATH");
        eprintln!("SKIP stock legs: no wasmtime CLI on PATH");
        return;
    };
    // The stock p1 artifact: the fs service's fdflags APPEND.
    let p1 = dir.join("probe.wasm");
    checked(
        Command::new(almide_bin()).args(["build", "probe.almd", "--target", "wasm", "-o"]).arg(&p1).current_dir(&dir).output().expect("spawn"),
        "stock p1 build",
    );
    fresh(&dir);
    let stock = checked(
        Command::new("wasmtime").args(["run", "--dir=/"]).arg(format!("--env=PWD={}", dir.display())).arg(&p1).current_dir(&dir).output().expect("spawn"),
        "stock p1 run",
    );
    assert_eq!(stock, native, "the stock p1 artifact differs from native");

    // The WASI 0.3 component: the p3 adapter's append-via-stream.
    if major < 46 {
        assert!(std::env::var_os("ALMIDE_EXPECT_TOOLS").is_none(), "ALMIDE_EXPECT_TOOLS: wasmtime {major} is below the p3 floor (46)");
        eprintln!("SKIP p3 leg: wasmtime {major} is below the p3 floor (46)");
        return;
    }
    let p3 = dir.join("probe.p3.wasm");
    checked(
        Command::new(almide_bin())
            .args(["build", "probe.almd", "--target", "wasm", "--component", "-o"])
            .arg(&p3)
            .env("ALMIDE_COMPONENT_P3", "1")
            .current_dir(&dir)
            .output()
            .expect("spawn"),
        "p3 component build",
    );
    fresh(&dir);
    let component = checked(
        Command::new("wasmtime").args(["run", "--dir=/", "-S", "inherit-env=y"]).arg(&p3).current_dir(&dir).env("PWD", &dir).output().expect("spawn"),
        "p3 component run",
    );
    assert_eq!(component, native, "the WASI 0.3 component differs from native");
}
