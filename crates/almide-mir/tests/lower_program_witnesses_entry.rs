//! The witness producer's own entry (`pipeline::program_witnesses`), on the
//! three paths the CLI sweep used to drive before `almide verify --mir` moved
//! to the native route's own certificates (#4909): a manifest that bounds the
//! caps witness, a function that walls while its siblings certify, and a
//! whole-program wall.
//!
//! - manifest: `almide.toml [permissions].allow` becomes each effect fn's
//!   declared bound, so `allow = []` leaves the effect fn nothing allowed
//!   while it still uses IO (the rejecting witness), `allow = ["IO"]` allows
//!   it, and a pure fn keeps the same witness either way.
//! - a per-function wall lands in `walled` with its reason; the functions that
//!   lower still get their witnesses and the call-modes witness.
//! - an unresolved import is a whole-program `Err`: nothing to certify.
//! - a top-level `let` reaches the lowering as a global.

use almide_mir::pipeline::{program_witnesses, ProgramWitnesses};

fn witnesses(src: &str, allow: Option<&[String]>) -> ProgramWitnesses {
    let src = src.to_string();
    let allow: Option<Vec<String>> = allow.map(<[String]>::to_vec);
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || program_witnesses(&src, &[], allow.as_deref()).expect("the program lowers"))
        .expect("spawn")
        .join()
        .expect("join")
}

fn caps_of(w: &ProgramWitnesses, name: &str) -> String {
    w.functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("`{name}` has no witness row (walled: {:?})", w.walled))
        .caps
        .clone()
}

const PRINTER: &str = "fn add(a: Int, b: Int) -> Int = a + b\n\
                       effect fn main() -> Unit = println(int.to_string(add(1, 2)))\n";

#[test]
fn a_manifest_bounds_the_effect_fns_caps_witness_and_leaves_a_pure_fn_alone() {
    let none = witnesses(PRINTER, None);
    let empty = witnesses(PRINTER, Some(&[]));
    let io = witnesses(PRINTER, Some(&["IO".to_string()]));

    let (main_none, main_empty, main_io) = (caps_of(&none, "main"), caps_of(&empty, "main"), caps_of(&io, "main"));
    let (allowed_empty, used_empty) = main_empty.split_once('|').expect("caps witness is `allowed|used`");
    assert!(allowed_empty.is_empty(), "allow = [] grants main nothing: `{main_empty}`");
    assert!(!used_empty.is_empty(), "main still uses IO: `{main_empty}`");
    assert_ne!(main_empty, main_none, "the manifest must reach the witness");
    let (allowed_io, used_io) = main_io.split_once('|').expect("caps witness is `allowed|used`");
    assert_eq!(used_io, used_empty, "the manifest changes the bound, never the use");
    assert!(
        used_io.split_whitespace().all(|u| allowed_io.split_whitespace().any(|a| a == u)),
        "allow = [\"IO\"] covers every capability main uses: `{main_io}`"
    );

    // Control: a pure fn's witness is the same under every manifest.
    let add = caps_of(&none, "add");
    assert_eq!(caps_of(&empty, "add"), add);
    assert_eq!(caps_of(&io, "add"), add);
}

/// A variant field the native release does not cover (a list of lists) walls
/// the functions that build or take it apart; the rest of the program lowers.
const ONE_WALL: &str = "type Raw =\n  | RName(String)\n  | RNest(List[List[String]])\n\n\
                        fn show(r: Raw) -> String =\n  match r {\n    RName(s) => s,\n    RNest(xs) => int.to_string(list.len(xs)),\n  }\n\n\
                        fn mk(n: Int) -> Raw = if n > 0 then RNest([[\"a\"], [\"b\"]]) else RName(\"none\")\n\n\
                        fn add(a: Int, b: Int) -> Int = a + b\n\n\
                        effect fn main() -> Unit = {\n  println(show(mk(1)))\n  println(int.to_string(add(1, 2)))\n}\n";

#[test]
fn a_walled_function_lands_in_walled_while_its_siblings_certify() {
    let w = witnesses(ONE_WALL, None);
    assert!(!w.walled.is_empty(), "a list-of-lists variant field must wall a function");
    for (name, why) in &w.walled {
        assert!(!why.is_empty(), "`{name}` walled with no reason");
        assert!(
            w.functions.iter().all(|f| &f.name != name),
            "`{name}` is both walled and certified"
        );
    }
    assert!(w.functions.iter().any(|f| f.name == "add"), "the pure sibling still certifies: {:?}", w.walled);
    assert!(w.call_modes.is_some(), "the lowered functions still get a call-modes witness");

    // Control: without the list-of-lists field nothing walls.
    let clean = ONE_WALL
        .replace("RNest(List[List[String]])", "RNest(List[String])")
        .replace("RNest([[\"a\"], [\"b\"]])", "RNest([\"a\", \"b\"])");
    let c = witnesses(&clean, None);
    assert!(c.walled.is_empty(), "the control walled: {:?}", c.walled);
}

#[test]
fn an_unresolved_import_is_a_whole_program_wall() {
    let src = "import somepkg\n\neffect fn main() -> Unit = println(somepkg.greet(\"x\"))\n".to_string();
    let r = std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || program_witnesses(&src, &[], None).map(|_| ()).map_err(|e| e.to_string()))
        .expect("spawn")
        .join()
        .expect("join");
    let e = r.expect_err("no sibling resolver: the program cannot lower");
    assert!(e.contains("somepkg"), "the wall names the import: {e}");
}

/// A top-level `let` is threaded into every lowering entry as a global, and a
/// function reading it lowers and certifies.
#[test]
fn a_function_reading_a_top_level_let_lowers_with_the_global_threaded_in() {
    let src = "let LIMIT = 41\n\n\
               fn bump(n: Int) -> Int = n + LIMIT\n\n\
               effect fn main() -> Unit = println(int.to_string(bump(1)))\n";
    let w = witnesses(src, None);
    assert!(w.walled.is_empty(), "a top-let reader must not wall: {:?}", w.walled);
    assert!(w.functions.iter().any(|f| f.name == "bump"), "`bump` certifies");
}
