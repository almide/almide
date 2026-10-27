//! Security-table row 5 (memory errors through FFI or raw pointers): the
//! part of the row that needs no new type. A `RawPtr` in a user program has
//! exactly two sources:
//!
//! 1. the address of a `Bytes` value the program owns (`ffi.address`, the
//!    deprecated `bytes.as_ptr` / `as_mut_ptr`), and
//! 2. a foreign function's return (`@extern`), which the `Built` line names.
//!
//! No user-reachable path turns an `Int` into a `RawPtr`, adds to one, or
//! decodes one from data. Provenance (base, length, owner — ADR-0031) can
//! only be attached where pointers are minted, so this file pins that the
//! mint sites stay those two. Every rule is derived from the stdlib's
//! declared signatures, never from a list of names:
//!
//! - every user-reachable stdlib fn whose signature holds a `RawPtr` is
//!   declared `@memory_unsafe` (so E100 / `import ffi` and the critical
//!   profile see it the day it lands);
//! - every user-reachable stdlib fn that RETURNS a `RawPtr` takes a `Bytes`
//!   (the memory it points into) — no fn mints one from scalars alone;
//! - the floor that does mint one from an `Int` (`prim`) is not user-nameable,
//!   by the same declaration E085 reads;
//! - the checker refuses the remaining spellings of a forged pointer in a file
//!   that opted in with `import ffi`: arithmetic, an integer literal, a Codec
//!   field, and the `prim` cast.

use std::path::Path;
use std::process::Command;

use almide_frontend::{bundled_sigs, stdlib};
use almide_lang::stdlib_info::{is_memory_unsafe_stdlib_fn, BUNDLED_MODULES};
use almide_lang::types::{FnSig, Ty};

fn holds_raw_ptr(t: &Ty) -> bool {
    matches!(t, Ty::RawPtr) || t.any_child_recursive(&|c| matches!(c, Ty::RawPtr))
}

fn sig_holds_raw_ptr(s: &FnSig) -> bool {
    holds_raw_ptr(&s.ret) || s.params.iter().any(|(_, t)| holds_raw_ptr(t))
}

/// Every (module, fn, sig) of a bundled stdlib module, user-nameable or not.
fn all_sigs() -> Vec<(&'static str, &'static str, FnSig)> {
    let mut out = Vec::new();
    for &m in BUNDLED_MODULES {
        for f in bundled_sigs::module_fn_names(m) {
            if let Some(s) = bundled_sigs::lookup(m, f) {
                out.push((m, f, s));
            }
        }
    }
    out
}

fn user_sigs() -> Vec<(&'static str, &'static str, FnSig)> {
    all_sigs().into_iter().filter(|(m, _, _)| stdlib::is_user_nameable_module(m)).collect()
}

#[test]
fn every_stdlib_fn_whose_signature_holds_a_raw_pointer_is_memory_unsafe() {
    let touching: Vec<_> = user_sigs().into_iter().filter(|(_, _, s)| sig_holds_raw_ptr(s)).collect();
    // Vacuity guard: the rule is about a surface that exists.
    assert!(touching.iter().any(|(m, f, _)| *m == "ffi" && *f == "read"), "no RawPtr fn found: {touching:?}");
    let unmarked: Vec<String> = touching
        .iter()
        .filter(|(m, f, _)| !is_memory_unsafe_stdlib_fn(m, f))
        .map(|(m, f, _)| format!("{m}.{f}"))
        .collect();
    assert!(
        unmarked.is_empty(),
        "stdlib fns that take or give a RawPtr without `@memory_unsafe` — a user file reaches raw memory \
         through them with no `import ffi` and under `--profile critical`: {unmarked:?}"
    );
}

#[test]
fn no_user_reachable_stdlib_fn_mints_a_raw_pointer_without_a_bytes_source() {
    let mints: Vec<_> = user_sigs().into_iter().filter(|(_, _, s)| holds_raw_ptr(&s.ret)).collect();
    assert!(mints.iter().any(|(m, f, _)| *m == "ffi" && *f == "address"), "no RawPtr mint found: {mints:?}");
    let unsourced: Vec<String> = mints
        .iter()
        .filter(|(_, _, s)| !s.params.iter().any(|(_, t)| matches!(t, Ty::Bytes)))
        .map(|(m, f, s)| format!("{m}.{f}({})", s.params.iter().map(|(_, t)| t.display()).collect::<Vec<_>>().join(", ")))
        .collect();
    assert!(
        unsourced.is_empty(),
        "stdlib fns that return a RawPtr without a Bytes it points into — a pointer with no owner the \
         program can see, i.e. a forged address: {unsourced:?}"
    );
}

/// The exclusion above is load-bearing: the floor really does mint a RawPtr
/// from an Int, and it is the module E085 keeps out of user files.
#[test]
fn the_floor_that_mints_from_an_int_is_not_user_nameable() {
    let floor_mints: Vec<_> = all_sigs()
        .into_iter()
        .filter(|(m, _, s)| {
            !stdlib::is_user_nameable_module(m)
                && holds_raw_ptr(&s.ret)
                && !s.params.iter().any(|(_, t)| matches!(t, Ty::Bytes))
        })
        .map(|(m, f, _)| format!("{m}.{f}"))
        .collect();
    assert!(!floor_mints.is_empty(), "the exclusion is vacuous: no non-user module mints a RawPtr from scalars");
}

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// `almide check main.almd` in a fresh dir; (success, stderr + stdout).
fn check(dir_name: &str, src: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join("almide-raw-ptr-surface").join(dir_name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("main.almd"), src).expect("write");
    let out = Command::new(almide())
        .arg("check")
        .arg(Path::new("main.almd"))
        .current_dir(&dir)
        .output()
        .expect("spawn almide check");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stderr), String::from_utf8_lossy(&out.stdout));
    (out.status.success(), text)
}

/// Every spelling of a forged pointer is refused even in a file that opted
/// into raw memory, so the only RawPtrs a program holds are the two minted
/// kinds above. The control (`ffi.read` of an owned buffer) checks clean, so
/// a refusal is about the forgery, not about `import ffi`.
#[test]
fn a_forged_pointer_is_refused_in_a_file_that_opted_in() {
    let wrap = |body: &str| {
        format!("import ffi\n\neffect fn main() -> Unit = {{\n  let b = bytes.from_list([1, 2, 3, 4])\n{body}  println(\"ok\")\n}}\n")
    };
    let (ok, err) = check("control", &wrap("  let c = ffi.read(ffi.address(b), 4)!\n  println(int.to_string(bytes.len(c)))\n"));
    assert!(ok, "control: the owned-buffer read is refused:\n{err}");

    for (what, src) in [
        ("arithmetic", wrap("  let c = ffi.read(ffi.address(b) + 4096, 4)!\n  println(int.to_string(bytes.len(c)))\n")),
        ("difference", wrap("  let d = ffi.address(b) - ffi.address(b)\n  println(int.to_string(d))\n")),
        ("integer literal", wrap("  let p: RawPtr = 4096\n  let c = ffi.read(p, 4)!\n  println(int.to_string(bytes.len(c)))\n")),
        ("prim cast", wrap("  let c = ffi.read(prim.int_to_ptr(4096), 4)!\n  println(int.to_string(bytes.len(c)))\n")),
        (
            "codec field",
            "import ffi\nimport json\n\ntype P: Codec = { p: RawPtr }\n\neffect fn main() -> Unit = {\n  let v = json.parse(\"{\\\"p\\\": 4096}\")!\n  let x = P.decode(v)!\n  let c = ffi.read(x.p, 4)!\n  println(int.to_string(bytes.len(c)))\n}\n"
                .to_string(),
        ),
    ] {
        let (ok, err) = check(&what.replace(' ', "_"), &src);
        assert!(!ok && err.contains("error"), "{what}: a forged RawPtr checked clean:\n{src}\n{err}");
    }
}
