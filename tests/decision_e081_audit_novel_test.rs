//! Row `verify.wasm-e081-availability` (owner ruling 2026-11-26): a stdlib fn
//! the availability table has no row for is decided by the stock-p1 audit of
//! the ops the module EMITS (`almide::wasm_route::stock_p1_unserved`), not
//! admitted for want of a row.
//!
//! - An op no stock preview-1 runtime serves is refused, a FRESH op number no
//!   table or shim names included; a served op and the subprocess family
//!   (host-capability, ADR-0025: built with the private import and a note,
//!   as for a listed process fn) are not.
//! - End to end, with the table disarmed (`ALMIDE_NO_AVAIL_CHECK`, so the
//!   call is a fn no row names as far as the check knows), a program in a
//!   fresh file that calls `unix.connect` (ops a stock runtime cannot serve)
//!   is refused by the audit at build and at `check --target wasm`, while a
//!   program whose ops are all served builds.

#[path = "support/novel.rs"]
mod novel;

use almide::wasm_route::stock_p1_unserved;
use std::process::Command;

fn almide() -> Command {
    Command::new(env!("CARGO_BIN_EXE_almide"))
}

#[test]
fn novel_an_unserved_op_of_a_fn_no_row_names_is_refused_by_the_p1_audit() {
    let fresh = novel::fs_op();
    assert_eq!(stock_p1_unserved(&[fresh]), Some(fresh), "a fresh op number was admitted");
    assert_eq!(stock_p1_unserved(&[1, 26, fresh]), Some(fresh), "a fresh op beside served ones was admitted");
    assert_eq!(stock_p1_unserved(&[1, 26, 29]), None, "served ops were refused");
    for op in almide_wasi::PROC_OPS.chain(almide_wasi::PROC_CHILD_OPS) {
        assert_eq!(stock_p1_unserved(&[op]), None, "subprocess op {op} (host-capability) was refused");
    }

    let dir = std::env::temp_dir().join(novel::path_segment());
    std::fs::create_dir_all(&dir).expect("mkdir");
    let unserved = dir.join(format!("{}.almd", novel::module()));
    std::fs::write(
        &unserved,
        "import unix\n\neffect fn main() -> Unit = {\n  let fd = unix.connect(\"/tmp/e081-audit.sock\")!\n  println(\"${unix.select([fd], 0)!}\")\n}\n",
    )
    .expect("write");
    let served = dir.join(format!("{}.almd", novel::module()));
    std::fs::write(&served, "effect fn main() -> Unit = {\n  println(\"served\")\n}\n").expect("write");
    let out = dir.join("out.wasm");
    let run = |args: &[&str]| {
        let o = almide().args(args).env("ALMIDE_NO_AVAIL_CHECK", "1").output().expect("spawn almide");
        (o.status.success(), String::from_utf8_lossy(&o.stderr).to_string())
    };
    let (u, o) = (unserved.to_str().unwrap(), out.to_str().unwrap());
    for args in [vec!["build", u, "--target", "wasm", "-o", o], vec!["check", u, "--target", "wasm"]] {
        let (ok, err) = run(&args);
        assert!(!ok, "{args:?}: an unserved op with no table row was admitted:\n{err}");
        assert!(err.contains("error[E082]") && err.contains("has no stock-WASI service"), "{args:?}: refused, but not by the p1 audit:\n{err}");
        assert!(!err.contains("error[E081]"), "{args:?}: the disarmed table answered, not the audit:\n{err}");
    }
    let (ok, err) = run(&["build", served.to_str().unwrap(), "--target", "wasm", "-o", o]);
    assert!(ok, "a program whose ops are all served was refused:\n{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
