//! The runtime routines the anchor file names (proved.rs), from outside the
//! crate. The bytes `--proved-bytes` hands to the proof script's shipped-form
//! decoder are the module's own, role by role; and the whole check refuses a
//! named routine that is not the proof script's list, and holds none when the
//! file names none. A routine that IS held, and where it differs from its
//! list, is the subject of proved.rs's own unit tests, which can read the
//! lists.

use almide_anchor_check::{check, shipped_routines, Differences};

/// Functions, as a `call` names them: 0 the imported `proc_exit`, 1 `$alloc`,
/// 2 `$inc`, 3 the flat release, 4 the free routine. None is the proof's
/// routine: the bodies only have to be told apart.
const MODULE: &str = r#"(module
  (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
  (memory 1)
  (func $alloc (param i32) (result i32) i32.const 7)
  (func $inc (param i32) nop)
  (func $dec (param i32) (local i32 i64) local.get 0 drop)
  (func $free (param i32) local.get 0 call $exit))"#;

/// An anchor file with no event: its `alloc` line, and `rest` after `bounds`.
fn anchors(alloc: &str, rest: &str) -> String {
    format!("almide-anchors 1\nalloc {alloc}\ninit\nbounds\n{rest}")
}

fn module() -> Vec<u8> {
    wat::parse_str(MODULE).expect("wat")
}

#[test]
fn each_named_routine_is_handed_over_as_the_modules_own_bytes() -> Result<(), String> {
    let (routines, exit) = shipped_routines(&module(), &anchors("1", "rcfns 2 3\nrc\nproved 3 4\n"))?;
    let want: Vec<(&'static str, u32, Vec<u8>)> = vec![
        ("alloc", 1, vec![0x41, 0x07, 0x0b]),
        ("inc", 2, vec![0x01, 0x0b]),
        // the locals vector is not part of a body's instructions
        ("dec", 3, vec![0x20, 0x00, 0x1a, 0x0b]),
        ("free", 4, vec![0x20, 0x00, 0x10, 0x00, 0x0b]),
    ];
    assert_eq!(routines, want);
    assert_eq!(exit, Some(0));
    Ok(())
}

#[test]
fn a_routine_the_file_marks_as_not_shipped_is_left_out() -> Result<(), String> {
    let (routines, _) = shipped_routines(&module(), &anchors("1", "rcfns 2 3\nrc\nproved - 4\n"))?;
    let roles: Vec<&str> = routines.iter().map(|r| r.0).collect();
    assert_eq!(roles, ["alloc", "inc", "free"]);
    Ok(())
}

#[test]
fn a_file_that_names_no_routine_hands_over_none() -> Result<(), String> {
    let (routines, exit) = shipped_routines(&module(), &anchors("-", ""))?;
    assert!(routines.is_empty());
    assert_eq!(exit, Some(0));
    Ok(())
}

#[test]
fn a_module_without_the_exit_import_names_none() -> Result<(), String> {
    let wasm = wat::parse_str("(module (func (param i32) (result i32) i32.const 7))").expect("wat");
    let (routines, exit) = shipped_routines(&wasm, &anchors("0", ""))?;
    assert_eq!(routines.len(), 1);
    assert_eq!(exit, None);
    Ok(())
}

/// A role named as a function the module does not define: past the last one,
/// or an import, which has no body to hand over.
#[test]
fn a_named_index_with_no_body_is_refused() {
    for alloc in ["9", "0"] {
        let e = shipped_routines(&module(), &anchors(alloc, "")).unwrap_err();
        assert!(e.contains("which the module does not define"), "refused for another reason: {e}");
    }
}

/// A bundle that hands over its three program records (empty) and
/// certifies no function: the whole check requires them (decision row
/// anchor.check).
const BUNDLE: &[u8] = b"bundle\nwitness init 0 (program)\n\nwitness bounds 0 (program)\n\nwitness fields 0 (program)\n\n";

#[test]
fn the_whole_check_refuses_a_named_allocator_that_is_not_the_proofs_list() {
    let e = check(&module(), BUNDLE, &anchors("1", "")).map(|_| ()).unwrap_err();
    assert!(e.contains("proved: `$alloc` is not the proof's `alloc_bytes`"), "refused for another reason: {e}");
}

#[test]
fn the_whole_check_holds_no_routine_when_the_file_names_none() -> Result<(), String> {
    let report = check(&module(), BUNDLE, &anchors("-", ""))?;
    assert_eq!(report.proved, Vec::<(&'static str, Differences)>::new());
    Ok(())
}
