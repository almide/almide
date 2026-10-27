//! Security-table row 5 (memory errors through FFI or raw pointers), after
//! ADR-0031: a pointer a program can read through carries its provenance.
//!
//! - `Handle` is the opaque foreign token. No user-reachable stdlib fn takes
//!   or gives one, so the only Handles a program holds came out of an
//!   `@extern` fn, and nothing but foreign code reads through them. There is
//!   no `Int` → pointer path and no pointer → `Int` path.
//! - `Region` is a view with base, length, permission and owner. It is
//!   minted only where the length is known (`ffi.region(b)`, `ffi.alloc(n)`,
//!   an `@extern` declaring `@region(len = …)`), its fields are opaque, and
//!   every `ffi.read` / `ffi.write` checks `0 <= off` and `off + n <= len`
//!   before it touches memory: an access outside the region is an `err` on
//!   both targets, and a write through a Bytes-made region is refused.
//!
//! Every surface rule is derived from the stdlib's declared signatures and
//! visibility, never from a list of names; every behaviour rule runs a
//! program on the native leg and on the wasm leg.

use std::path::Path;
use std::process::Command;

use almide_frontend::{bundled_sigs, stdlib};
use almide_lang::ast;
use almide_lang::stdlib_info::{is_memory_unsafe_stdlib_fn, BUNDLED_MODULES};
use almide_lang::types::extern_abi::is_region;
use almide_lang::types::{FnSig, Ty};

fn holds(t: &Ty, hit: &dyn Fn(&Ty) -> bool) -> bool {
    hit(t) || t.any_child_recursive(&|c| hit(c))
}

fn is_handle(t: &Ty) -> bool {
    matches!(t, Ty::RawPtr)
}

fn sig_holds(s: &FnSig, hit: &dyn Fn(&Ty) -> bool) -> bool {
    holds(&s.ret, hit) || s.params.iter().any(|(_, t)| holds(t, hit))
}

/// The `local fn`s of a bundled module: reachable from that file only (E420
/// from anywhere else), so not part of any user's surface.
fn local_fns(module: &str) -> Vec<String> {
    let Some(src) = stdlib::get_bundled_source(module) else { return Vec::new() };
    let Some(program) = almide_lang::parse_cached(src) else { return Vec::new() };
    program
        .decls
        .iter()
        .filter_map(|d| match d {
            ast::Decl::Fn { name, visibility: ast::Visibility::Local, .. } => Some(name.as_str().to_string()),
            _ => None,
        })
        .collect()
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

/// The stdlib fns a user file can call: a nameable module (E085 keeps `prim`
/// out), and not `local` to its file.
fn user_sigs() -> Vec<(&'static str, &'static str, FnSig)> {
    all_sigs()
        .into_iter()
        .filter(|(m, f, _)| stdlib::is_user_nameable_module(m) && !local_fns(m).iter().any(|l| l == f))
        .collect()
}

/// D1, D4, D7: no user-reachable stdlib fn takes or gives a Handle — not an
/// address of a Bytes, not an Int cast, not a read through one. The stdlib's
/// own raw operations exist (non-vacuity) and are `local` to stdlib/ffi.almd.
#[test]
fn no_user_reachable_stdlib_fn_holds_a_handle() {
    let reachable: Vec<String> = user_sigs()
        .into_iter()
        .filter(|(_, _, s)| sig_holds(s, &is_handle))
        .map(|(m, f, _)| format!("{m}.{f}"))
        .collect();
    assert!(
        reachable.is_empty(),
        "user-reachable stdlib fns that take or give a Handle — an address a program could read through or \
         forge without a Region's bounds check: {reachable:?}"
    );
    let hidden: Vec<_> = all_sigs()
        .into_iter()
        .filter(|(m, _, s)| stdlib::is_user_nameable_module(m) && sig_holds(s, &is_handle))
        .collect();
    assert!(
        hidden.iter().any(|(m, _, _)| *m == "ffi"),
        "the exclusion is vacuous: ffi's raw operations no longer hold a Handle"
    );
}

/// The floor that mints a Handle from an Int is not user-nameable, by the
/// same declaration E085 reads — and it does exist, so the exclusion above
/// is load-bearing.
#[test]
fn the_floor_that_mints_from_an_int_is_not_user_nameable() {
    let floor_mints: Vec<_> = all_sigs()
        .into_iter()
        .filter(|(m, _, s)| !stdlib::is_user_nameable_module(m) && holds(&s.ret, &is_handle))
        .map(|(m, f, _)| format!("{m}.{f}"))
        .collect();
    assert!(!floor_mints.is_empty(), "no non-user module mints a Handle: the E085 exclusion is vacuous");
}

/// Every user-reachable fn holding a Region is `@memory_unsafe`, so E100 /
/// `import ffi` and the critical profile see it.
#[test]
fn every_stdlib_fn_holding_a_region_is_memory_unsafe() {
    let touching: Vec<_> = user_sigs().into_iter().filter(|(_, _, s)| sig_holds(s, &is_region)).collect();
    assert!(touching.iter().any(|(m, f, _)| *m == "ffi" && *f == "read"), "no Region fn found: {touching:?}");
    let unmarked: Vec<String> = touching
        .iter()
        .filter(|(m, f, _)| !is_memory_unsafe_stdlib_fn(m, f))
        .map(|(m, f, _)| format!("{m}.{f}"))
        .collect();
    assert!(unmarked.is_empty(), "stdlib fns holding a Region without `@memory_unsafe`: {unmarked:?}");
}

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(dir_name: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("almide-raw-ptr-surface").join(dir_name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("main.almd"), src).expect("write");
    dir
}

/// `almide <args> main.almd` in a fresh dir; (success, stdout, stderr).
fn almide_in(dir_name: &str, src: &str, args: &[&str]) -> (bool, String, String) {
    let dir = scratch(dir_name, src);
    let out = Command::new(almide())
        .args(args)
        .arg(Path::new("main.almd"))
        .current_dir(&dir)
        .output()
        .expect("spawn almide");
    (out.status.success(), String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr).to_string())
}

fn check(dir_name: &str, src: &str) -> (bool, String) {
    let (ok, out, err) = almide_in(dir_name, src, &["check"]);
    (ok, format!("{err}{out}"))
}

/// Every spelling of a forged pointer, and every way into a Region's fields,
/// is refused even in a file that opted in with `import ffi`. The control (a
/// read of an owned region) checks clean, so each refusal is about the
/// forgery, not about `import ffi`.
#[test]
fn a_forged_pointer_or_region_is_refused_in_a_file_that_opted_in() {
    let wrap = |body: &str| {
        format!("import ffi\n\neffect fn main() -> Unit = {{\n  let b = bytes.from_list([1, 2, 3, 4])\n{body}  println(\"ok\")\n}}\n")
    };
    let (ok, err) = check("control", &wrap("  let c = ffi.read(ffi.region(b), 0, 4)!\n  println(int.to_string(bytes.len(c)))\n"));
    assert!(ok, "control: the owned-region read is refused:\n{err}");

    for (what, src) in [
        ("handle literal", wrap("  let p: Handle = 4096\n  println(\"${p}\")\n")),
        ("handle arithmetic", "@extern(c, \"c\", \"tmpfile\")\nfn tmp() -> Handle\n\neffect fn main() -> Unit = {\n  let p = tmp() + 4096\n  println(\"x\")\n}\n".to_string()),
        ("prim cast", wrap("  let p = prim.int_to_ptr(4096)\n")),
        ("ffi.to_int", wrap("  let a = ffi.to_int(ffi.address(b))\n  println(int.to_string(a))\n")),
        ("bytes.data_ptr", wrap("  let a = bytes.data_ptr(b)\n  println(int.to_string(a))\n")),
        ("ffi.address", wrap("  let p = ffi.address(b)\n")),
        ("region field read", wrap("  let r = ffi.region(b)\n  let p = r.base\n")),
        ("region length forged", wrap("  let r = ffi.region(b)\n  let big = { ...r, len: 1000000 }\n  let c = ffi.read(big, 0, 4096)!\n")),
        ("region literal", wrap("  let r: Region = { owner: b, base: ffi.region(b).base, len: 4096, writable: true, keep: () => () }\n")),
        ("raw load", wrap("  let c = ffi.__load(ffi.region(b).base, 0, 4096)\n")),
        (
            "codec field",
            "import ffi\nimport json\n\ntype P: Codec = { p: Handle }\n\neffect fn main() -> Unit = {\n  let v = json.parse(\"{\\\"p\\\": 4096}\")!\n  let x = P.decode(v)!\n  println(\"x\")\n}\n"
                .to_string(),
        ),
    ] {
        let (ok, err) = check(&what.replace(' ', "_").replace('.', "_"), &src);
        assert!(!ok && err.contains("error"), "{what}: a forged pointer or region checked clean:\n{src}\n{err}");
    }
}

/// The bounds check and the permission check, on both legs (ADR-0031 D1, D4,
/// D5). Each line is the program's view of one access; the out-of-bounds and
/// read-only lines must be `err`, the in-bounds ones `ok` with the bytes.
const BOUNDS: &str = r#"import ffi

fn show(r: Result[Bytes, String]) -> String = match r {
  ok(b) => "ok " + (bytes.to_list(b) |> list.map((x) => int.to_string(x)) |> list.join(",")),
  err(e) => "err " + e,
}

fn shown(r: Result[Unit, String]) -> String = match r {
  ok(_) => "ok",
  err(e) => "err " + e,
}

effect fn main() -> Unit = {
  let owned = ffi.region(bytes.from_list([1, 2, 3, 4]))
  println("owned in    " + show(ffi.read(owned, 0, 4)))
  println("owned tail  " + show(ffi.read(owned, 2, 2)))
  println("owned past  " + show(ffi.read(owned, 2, 3)))
  println("owned at len " + show(ffi.read(owned, 4, 1)))
  println("owned neg   " + show(ffi.read(owned, -1, 1)))
  println("owned huge  " + show(ffi.read(owned, 9223372036854775807, 1)))
  println("owned write " + shown(ffi.write(owned, 0, bytes.from_list([9]))))
  let fresh = ffi.alloc(4)!
  println("alloc write " + shown(ffi.write(fresh, 1, bytes.from_list([7, 8]))))
  println("alloc read  " + show(ffi.read(fresh, 0, 4)))
  println("alloc past  " + shown(ffi.write(fresh, 3, bytes.from_list([1, 2]))))
  println("alloc neg   " + shown(ffi.write(fresh, -1, bytes.from_list([1]))))
  println("alloc after " + show(ffi.read(fresh, 0, 4)))
}
"#;

const BOUNDS_EXPECTED: &[&str] = &[
    "owned in    ok 1,2,3,4",
    "owned tail  ok 3,4",
    "owned past  err ffi.read: 3 bytes at offset 2 are outside a region of 4 bytes",
    "owned at len err ffi.read: 1 bytes at offset 4 are outside a region of 4 bytes",
    "owned neg   err ffi.read: 1 bytes at offset -1 are outside a region of 4 bytes",
    "owned huge  err ffi.read: 1 bytes at offset 9223372036854775807 are outside a region of 4 bytes",
    "owned write err ffi.write: the region is read-only (ffi.region views a Bytes value)",
    "alloc write ok",
    "alloc read  ok 0,7,8,0",
    "alloc past  err ffi.write: 2 bytes at offset 3 are outside a region of 4 bytes",
    "alloc neg   err ffi.write: 1 bytes at offset -1 are outside a region of 4 bytes",
    "alloc after ok 0,7,8,0",
];

fn run_bounds(leg: &str, args: &[&str]) {
    let (ok, out, err) = almide_in(&format!("bounds_{leg}"), BOUNDS, args);
    assert!(ok, "{leg}: the bounds program failed:\n{err}\n{out}");
    let got: Vec<&str> = out.lines().collect();
    assert_eq!(got, BOUNDS_EXPECTED, "{leg}: a Region access answered differently\n--- stdout ---\n{out}");
}

#[test]
fn a_region_access_past_its_length_is_err_on_the_native_leg() {
    run_bounds("native", &["run"]);
}

#[test]
fn a_region_access_past_its_length_is_err_on_the_wasm_leg() {
    run_bounds("wasm", &["run", "--target", "wasm"]);
}
