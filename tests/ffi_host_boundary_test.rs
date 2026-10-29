//! ADR-0031 amendment A2: an `@export(c)` fn takes Region parameters.
//!
//! A Region parameter crosses as the C caller's `(const uint8_t*, size_t)`
//! and is read-only; one the declaration names with `@region(out = p)`
//! crosses as `(uint8_t*, size_t)` and is writable. Either is lent for the
//! one call: every access is checked against the caller's length (an access
//! past it is `err`, ADR-0031 D5), and a copy still held when the call
//! returns stops the process. The C caller here is the package's own
//! `native/host.rs`, which calls the exported symbols through `extern "C"`
//! declarations, so no C toolchain is needed.
//!
//! The pull direction on native (a writable `ffi.alloc` Region lent to an
//! `@extern(c)` import as `(uint8_t*, size_t)`) is
//! `extern_rust_abi_test::a_region_crosses_c_as_pointer_and_length_and_is_released_once`.

use std::path::Path;
use std::process::Command;

fn run_package(tag: &str, almd: &str, host: &str, args: &[&str]) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("almide_ffi_host_boundary_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("native")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("almide.toml"), "[package]\nname = \"ffi_host_boundary\"\n").unwrap();
    std::fs::write(dir.join("src/main.almd"), almd).unwrap();
    std::fs::write(dir.join("native/host.rs"), host).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(&dir)
        .env("ALMIDE_RUN_PROJECT_DIR", dir.join(".run"))
        .args(args)
        .arg(Path::new("src/main.almd"))
        .output()
        .expect("spawn almide");
    let _ = std::fs::remove_dir_all(&dir);
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

const ALMD: &str = r#"import ffi

@export(c, "ffb_sum")
effect fn sum(buf: Region) -> Int = {
  let b = ffi.read(buf, 0, ffi.len(buf))!
  bytes.to_list(b) |> list.fold(0, (a, x) => a + x)
}

@export(c, "ffb_probe")
effect fn probe(buf: Region) -> Int = {
  let past = match ffi.read(buf, 1, ffi.len(buf)) {
    ok(_) => 1,
    err(_) => 0,
  }
  let wrote = match ffi.write(buf, 0, bytes.from_list([9])) {
    ok(_) => 10,
    err(_) => 0,
  }
  let writable = if ffi.is_writable(buf) then 100 else 0
  past + wrote + writable
}

@export(c, "ffb_fill")
@region(out = out)
effect fn fill(out: Region, byte: Int) -> Int = {
  let n = ffi.len(out)
  ffi.write(out, 0, bytes.from_list(list.range(0, n) |> list.map((i) => byte + i)))!
  let past = match ffi.write(out, n, bytes.from_list([1])) {
    ok(_) => 1,
    err(_) => 0,
  }
  n * 10 + past
}

@extern(c, "c", "cf_sum")
fn c_sum() -> Int = _

@extern(c, "c", "cf_sum_buf")
fn c_sum_buf(buf: Region) -> Int = _

@extern(c, "c", "cf_probe")
fn c_probe() -> Int = _

@extern(c, "c", "cf_fill")
fn c_fill() -> Int = _

effect fn main() -> Unit = {
  println("sum ${c_sum()}")
  println("probe ${c_probe()}")
  println("fill ${c_fill()}")
  let tail = ffi.slice(ffi.region(bytes.from_list([1, 2, 3, 40])), 2, 2) ?? ffi.region(bytes.new())
  println("slice ${c_sum_buf(tail)}")
}
"#;

const HOST: &str = r#"#![allow(dead_code, unused_imports, clippy::missing_safety_doc)]

extern "C" {
    fn ffb_sum(p: *const u8, n: usize) -> i32;
    fn ffb_probe(p: *const u8, n: usize) -> i32;
    fn ffb_fill(p: *mut u8, n: usize, byte: i32) -> i32;
}

#[no_mangle]
pub extern "C" fn cf_sum() -> i32 {
    let v = [1u8, 2, 3, 4];
    unsafe { ffb_sum(v.as_ptr(), v.len()) }
}

#[no_mangle]
pub unsafe extern "C" fn cf_sum_buf(p: *const u8, n: usize) -> i32 {
    (0..n).map(|i| unsafe { *p.add(i) } as i32).sum()
}

#[no_mangle]
pub extern "C" fn cf_probe() -> i32 {
    let v = [5u8, 6];
    let r = unsafe { ffb_probe(v.as_ptr(), v.len()) };
    // A read-only lend changed nothing.
    r * 1000 + v[0] as i32
}

#[no_mangle]
pub extern "C" fn cf_fill() -> i32 {
    // The callee is told 3 of the 4 bytes: the 4th must stay 0.
    let mut v = [0u8; 4];
    let r = unsafe { ffb_fill(v.as_mut_ptr(), 3, 20) };
    r * 1000 + v[0] as i32 * 100 + v[1] as i32 + v[2] as i32 + v[3] as i32 * 10000
}
"#;

/// A read-only lend: summed through the caller's length; a read past it and
/// a write through it are both `err`, and it reports read-only. A writable
/// lend (`@region(out = …)`): filled through the caller's length, with a
/// write at the length an `err` and the byte past it untouched.
#[test]
#[cfg(unix)]
fn an_export_c_fn_reads_and_fills_a_lent_region_within_the_callers_length() {
    let (ok, stdout, stderr) = run_package("lend", ALMD, HOST, &["run"]);
    assert!(ok, "the @export(c) Region program failed\n--- stderr ---\n{stderr}");
    // fill: n = 3, the write at 3 refused (past = 0) -> 30; bytes 20, 21, 22, 0.
    assert_eq!(stdout, "sum 10\nprobe 5\nfill 32043\nslice 43\n", "--- stderr ---\n{stderr}");
}

/// A lent Region kept past its call would read the caller's memory after the
/// caller may have freed it. Kept in the fn's own body it is E119 at check
/// time (`a_lent_region_escape_is_e119`); kept by a helper the export calls,
/// which the check does not follow, the wrapper stops the process before the
/// caller regains control.
#[test]
#[cfg(unix)]
fn a_lent_region_kept_past_its_call_stops_the_process() {
    let almd = r#"import ffi

var kept: Region = ffi.region(bytes.new())

fn stash(r: Region) -> Int = {
  kept = r
  ffi.len(kept)
}

@export(c, "ffb_keep")
fn keep(buf: Region) -> Int = stash(buf)

@extern(c, "c", "cf_keep")
fn c_keep() -> Int = _

effect fn main() -> Unit = {
  let n = c_keep()
  println("returned ${n}")
}
"#;
    let host = r#"#![allow(dead_code, unused_imports, clippy::missing_safety_doc)]
extern "C" { fn ffb_keep(p: *const u8, n: usize) -> i32; }
#[no_mangle]
pub extern "C" fn cf_keep() -> i32 { let v = [1u8, 2, 3]; unsafe { ffb_keep(v.as_ptr(), v.len()) } }
"#;
    let (ok, stdout, stderr) = run_package("escape", almd, host, &["run"]);
    assert!(!ok, "a lent Region outlived its call and the program carried on\n{stdout}");
    assert!(stderr.contains("is still held after the call returned"), "--- stderr ---\n{stderr}");
    assert!(!stdout.contains("returned"), "{stdout}");
}

/// The declaration rules (E090): an `@export(c)` fn cannot return a Region;
/// `@region(out = p)` names a Region parameter; an `@export(c)` clause takes
/// only `out`; and `@region` on a fn that neither binds nor exports C is
/// refused rather than ignored.
#[test]
fn export_c_region_declarations_are_checked() {
    let almd = r#"import ffi

@export(c, "r1")
fn r1(n: Int) -> Region = ffi.region(bytes.new())

@export(c, "r2")
@region(out = n)
fn r2(buf: Region, n: Int) -> Int = n

@export(c, "r3")
@region(out = nope)
fn r3(buf: Region) -> Int = 0

@export(c, "r4")
@region(len = buf)
fn r4(buf: Region) -> Int = 0

@region(out = buf)
fn r5(buf: Region) -> Int = 0

@export(c, "ok1")
@region(out = buf)
fn ok1(buf: Region, n: Int) -> Int = n

effect fn main() -> Unit = println("x")
"#;
    let (ok, stdout, stderr) = run_package("e090", almd, "", &["check"]);
    let out = format!("{stdout}{stderr}");
    assert!(!ok, "check accepted bad @export(c) Region declarations:\n{out}");
    for want in [
        "fn 'r1': a Region return has no C form",
        "`out = n` names a parameter of type Int, not Region",
        "`out = nope` names no parameter of 'r3'",
        "unknown argument `len`",
        "fn 'r5': `@region` is only read on",
    ] {
        assert!(out.contains(want), "no E090 for `{want}`\n{out}");
    }
    assert_eq!(out.matches("error[E090]").count(), 5, "one E090 per bad declaration, none for ok1\n{out}");
}

/// E119: every way a lent Region leaves an `@export(c)` fn's body is refused
/// at check time — a top-level `var` (through a `let` alias, a slice and a
/// container too), a closure, a return type holding one — and reading it,
/// copying its bytes out, and a local alias are not.
#[test]
fn a_lent_region_escape_is_e119() {
    let almd = r#"import ffi

var kept: Region = ffi.region(bytes.new())
var many: List[Region] = []
var saved: Bytes = bytes.new()

@export(c, "e1")
fn e1(buf: Region) -> Int = {
  kept = buf
  0
}

@export(c, "e2")
effect fn e2(buf: Region) -> Int = {
  let part = ffi.slice(buf, 0, 1)!
  kept = part
  0
}

@export(c, "e3")
fn e3(buf: Region) -> Int = {
  let xs = [buf]
  many = xs
  0
}

@export(c, "e4")
fn e4(buf: Region) -> Int = {
  let f = () => ffi.len(buf)
  f()
}

@export(c, "e5")
fn e5(buf: Region) -> List[Region] = [buf]

@export(c, "ok1")
effect fn ok1(buf: Region) -> Int = {
  let alias = buf
  var also = alias
  also = buf
  saved = ffi.read(also, 0, ffi.len(buf))!
  ffi.len(alias)
}

effect fn main() -> Unit = println("x")
"#;
    let (ok, stdout, stderr) = run_package("e119", almd, "", &["check"]);
    let out = format!("{stdout}{stderr}");
    assert!(!ok, "check accepted escaping lent Regions:\n{out}");
    for want in [
        "fn 'e1' escapes its call: it is stored in the top-level var `kept`",
        "fn 'e2' escapes its call: it is stored in the top-level var `kept`",
        "fn 'e3' escapes its call: it is stored in the top-level var `many`",
        "fn 'e4' escapes its call: it is captured by a closure",
        "fn 'e5' escapes its call: it is returned",
    ] {
        assert!(out.contains(want), "no E119 for `{want}`\n{out}");
    }
    assert!(!out.contains("'ok1'"), "a read, a copy and local aliases were refused\n{out}");
}
