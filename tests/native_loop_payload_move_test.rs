//! #4661: a Result's Ok payload moves out of its carrier in a loop whose
//! other arm reads the Err payload.
//!
//! `acc = value.set_path(acc, path, x)!` in a `for` reads the Err side's text
//! in the failing arm and rebinds `acc` to the Ok side in the other. The move
//! out of the carrier is refused while a sibling payload of the same carrier
//! is still lent (`render_native_addr::other_payload_live`); that question
//! was asked of the value's liveness, which follows the loop's back edge to
//! the NEXT round's read of the Err side, so every round copied the whole
//! document where it had moved it: a fold of n steps allocated and copied n²
//! (40,000 keys: 136 s of CPU; 10,000: 5.8 s, where the move takes
//! milliseconds). The sibling is lent only as long as the binding of this
//! round is read (`mir_liveness::bound_read_after`).
//!
//! What this test holds — the rendered text and the blocks allocated, never a
//! wall-clock time:
//! - the loop moves the payload out (`std::mem::take`) and copies nothing;
//! - the blocks the fold allocates grow in proportion to its steps (the
//!   copying fold's grow with their square);
//! - the folds print the same on wasm.
//!
//! The round in which a sibling payload IS read after the move keeps its copy:
//! tests/native_spine_clone_test.rs (`Link(name, xs, rest)` walks).
//!
//! The same for an Option's payload: `match o { some(n) => walk(n.next, ..) }`
//! over `type O = { v: Int, next: O? }` read the record out of the Option by
//! copying it — the whole rest of the chain on every step (200,000 links:
//! about 14 minutes where the standard codegen took 0.03 s) — because a
//! carrier's payload moved out only at the carrier's very last read, and a
//! loop's carrier is rebound and read again. It now leaves as a record's
//! field does (its `Default` stays, the tag still reads `Some`).

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-loop-payload-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of `almide run <src>` on `target`, the certificate required.
fn run(src: &Path, target: Option<&str>, alloc: bool) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    if alloc {
        cmd.env("ALMIDE_ALLOC_COUNT", "1");
    }
    let out = cmd.output().expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// The counting allocator's `allocs` for `source`.
fn allocs(tag: &str, source: &str) -> u64 {
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    let (_, stderr) = run(&src, None, true);
    let _ = std::fs::remove_dir_all(&dir);
    let line = stderr.lines().find(|l| l.starts_with("__ALMD_ALLOC ")).unwrap_or_else(|| panic!("no allocation report:\n{stderr}"));
    line.split_whitespace().find_map(|kv| kv.strip_prefix("allocs=")).and_then(|v| v.parse().ok()).unwrap_or_else(|| panic!("malformed report `{line}`"))
}

/// The object fold of tests/value_json_combinators_linear_test.rs.
fn object_fold(n: usize, step: &str) -> String {
    format!(
        r#"import json
effect fn main() -> Unit = {{
  var acc = value.object([])
  for i in 0..<{n} {{
    acc = value.set_path(acc, value.key(value.root(), "k${{i}}"), value.int(i)){step}
  }}
  acc = value.set_path(acc, value.key(value.root(), "k0"), value.string("again"))!
  let ks = list.map(value.as_object(acc) ?? [], (kv) => kv.0)
  println("${{list.len(ks)}} ${{list.first(ks) ?? "-"}} ${{list.last(ks) ?? "-"}} ${{json.stringify(value.pick(acc, ["k0", "k1"]))}}")
}}
"#
    )
}

/// A list accumulator through a user function's Result, the same loop.
fn list_fold(n: usize) -> String {
    format!(
        r#"fn push(xs: List[String], i: Int) -> Result[List[String], String] =
  if i < 0 then err("negative ${{i}}") else ok(xs + ["item ${{i}}"])

effect fn main() -> Unit = {{
  var acc: List[String] = []
  for i in 0..<{n} {{
    acc = push(acc, i)!
  }}
  println("${{list.len(acc)}} ${{list.last(acc) ?? "-"}}")
}}
"#
    )
}

/// The text of `fn main` in the certified render of `source`.
fn rendered_main(source: &str) -> String {
    let rust = almide_mir::pipeline::try_render_rust_source(source).unwrap_or_else(|e| panic!("left the certified render: {e:?}"));
    let at = rust.find("\nfn main() {").expect("fn main");
    let end = rust[at..].find("\n}\n").expect("the end of main");
    rust[at..at + end].to_string()
}

/// The `for` loop of `main`: from its header to the first line back at the
/// header's indentation.
fn first_loop(main: &str) -> String {
    let at = main.find("\n    for ").expect("a for loop in main");
    let end = main[at + 1..].find("\n    }\n").expect("the loop's end");
    main[at..at + 1 + end].to_string()
}

#[test]
fn the_ok_payload_moves_out_of_its_carrier_every_round() {
    for (what, source) in [("object fold", object_fold(8, "!")), ("list fold", list_fold(8))] {
        let body = first_loop(&rendered_main(&source));
        assert!(body.contains("std::mem::take(v)"), "{what}: the Ok payload is not moved out:\n{body}");
        assert!(!body.contains(".clone()"), "{what}: the loop copies a value:\n{body}");
    }
}

#[test]
fn the_fold_allocates_in_proportion_to_its_steps() {
    for (what, src) in [
        ("object fold", (|n| object_fold(n, "!")) as fn(usize) -> String),
        ("object fold, `?? fallback`", |n| object_fold(n, " ?? value.null()")),
        ("list fold", list_fold),
    ] {
        let (small, large) = (allocs("small", &src(1000)), allocs("large", &src(4000)));
        // Four times the steps: four times the blocks when each step moves
        // its accumulator, sixteen times when each copies it.
        assert!(
            large < small * 6,
            "{what}: {large} blocks at 4,000 steps against {small} at 1,000 — a copy of the accumulator per step?"
        );
    }
}

#[test]
fn the_folds_print_the_same_on_wasm() {
    if Command::new("wasmtime").arg("--version").output().is_err() {
        eprintln!("skip: wasmtime is not installed");
        return;
    }
    for (tag, source, want) in [
        ("obj", object_fold(300, "!"), "300 k0 k299 {\"k0\":\"again\",\"k1\":1}\n"),
        ("obj-or", object_fold(300, " ?? value.null()"), "300 k0 k299 {\"k0\":\"again\",\"k1\":1}\n"),
        ("list", list_fold(300), "300 item 299\n"),
    ] {
        let dir = scratch(tag);
        let src = dir.join("prog.almd");
        std::fs::write(&src, source).unwrap();
        for target in [None, Some("wasm")] {
            let (stdout, stderr) = run(&src, target, false);
            assert_eq!(stdout, want, "{tag} on {target:?}:\n{stderr}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The walk of tests/recursive_type_depth_test.rs over an Option-linked record.
fn option_chain(n: usize) -> String {
    format!(
        r#"type O = {{ v: Int, next: O? }}

fn sum_o(o: O?, acc: Int) -> Int = match o {{
  some(n) => sum_o(n.next, acc + n.v),
  none => acc,
}}

effect fn main() -> Unit = {{
  let o = list.range(0, {n}) |> list.fold(O {{ v: 0, next: none }}, (acc, i) => O {{ v: i, next: some(acc) }})
  println("${{sum_o(some(o), 0)}}")
}}
"#
    )
}

#[test]
fn an_option_chain_walk_moves_each_link_out() {
    let rust = almide_mir::pipeline::try_render_rust_source(&option_chain(8)).unwrap_or_else(|e| panic!("left the certified render: {e:?}"));
    let at = rust.find("fn almd_sum_o(").expect("the walk");
    let body = &rust[at..at + rust[at..].find("\n}\n").expect("its end")];
    assert!(body.contains("Some(v) => std::mem::take(v)"), "the link is not moved out of its Option:\n{body}");
    assert!(!body.contains(".clone()"), "the walk copies a value:\n{body}");
}

#[test]
fn an_option_chain_walk_allocates_in_proportion_to_its_links() {
    let (small, large) = (allocs("chain-small", &option_chain(1000)), allocs("chain-large", &option_chain(4000)));
    // Four times the links: four times the blocks, sixteen times when every
    // step copies the rest of the chain.
    assert!(large < small * 6, "{large} blocks at 4,000 links against {small} at 1,000 — a copy of the chain per step?");
}

#[test]
fn an_option_chain_walk_prints_the_same_on_wasm() {
    if Command::new("wasmtime").arg("--version").output().is_err() {
        eprintln!("skip: wasmtime is not installed");
        return;
    }
    let dir = scratch("chain");
    let src = dir.join("prog.almd");
    std::fs::write(&src, option_chain(3000)).unwrap();
    for target in [None, Some("wasm")] {
        let (stdout, stderr) = run(&src, target, false);
        assert_eq!(stdout, "4498500\n", "on {target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
