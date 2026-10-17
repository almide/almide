//! The wasm lowering of the subprocess family frames its argument list in
//! work linear in the list.
//!
//! `__proc_argv` (stdlib/process_wasm.almd) concatenated the cells with
//! `list.fold("", (acc, c) => acc + c)`, copying the growing frame once per
//! argument: 400 arguments allocated 0.4 MB, 3200 allocated 25.7 MB, and 256
//! calls with 1600 arguments ran the canonical allocator out of memory.
//! Found by the stdlib inspection (`xtarget-fuzz inspect process.exec`,
//! complexity at n=160000).
#![cfg(unix)]
use std::process::Command;

fn allocated_bytes(args: usize) -> u64 {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("argv.almd");
    let src = format!(
        r#"import process

effect fn main() -> Unit = {{
  let args = list.range(0, {args}) |> list.map((i) => "aé")
  let r = process.exec("/bin/echo", args)!
  println("${{string.len(r)}}")
}}
"#
    );
    std::fs::write(&file, src).expect("source");
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .args(["--target", "wasm"])
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("spawn");
    let out = String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr);
    assert!(o.status.success(), "{out}");
    assert!(out.starts_with(&format!("{}\n", args * 3)), "echoed {args} arguments: {out}");
    let field = out.split_whitespace().find_map(|w| w.strip_prefix("bytes=")).unwrap_or_else(|| panic!("no alloc report: {out}"));
    field.parse().expect("bytes")
}

#[test]
fn framing_four_times_the_arguments_allocates_about_four_times_the_bytes() {
    let (small, large) = (allocated_bytes(800), allocated_bytes(3200));
    // Linear framing gives ~4x; the quadratic fold gave ~16x.
    assert!(large < small * 6, "800 args: {small} B, 3200 args: {large} B");
}
