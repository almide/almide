//! ADR-0033 D1: the family rc redirect ships only in a module that LINKS a
//! fn declared `@family_producer`. A module that links none — including one
//! whose source declares a producer it never reaches — emits exactly the
//! bytes it emits without the declaration; the corpus-wide proof is the
//! size ledgers (golden/size-baseline*.txt), which do not move.

mod harness;
use harness::run_wasm;

fn emit(src: &str) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    almide_wasm::emit_program(&ir).expect("emit")
}

/// `$inc` / `$dec_flat`'s retarget, as family.rs emits it.
const RETARGET: &[u8] = &[
    0x20, 0, 0x28, 2, 0, 0x41, 0, 0x48, 0x04, 0x40, 0x20, 0, 0x20, 0, 0x28, 2, 0, 0x41, 0xff, 0xff, 0xff, 0xff, 0x07, 0x71,
    0x6b, 0x21, 0, 0x0b,
];

fn has_retarget(bytes: &[u8]) -> bool {
    bytes.windows(RETARGET.len()).any(|w| w == RETARGET)
}

// `local`: an entry-program `fn` is public, and a public fn is an export
// (a DCE root, so linked); a local one nobody calls is not linked.
const UNREACHED: &str = r#"ATTR
local fn make(n: Int) -> String = "v" + int.to_string(n)

fn main() -> Unit = {
  let s = "a" + int.to_string(3)
  println(s)
}
"#;

const REACHED: &str = r#"ATTR
fn make(n: Int) -> String = "v" + int.to_string(n)

fn main() -> Unit = {
  let xs = [make(1), make(2), make(3)]
  println(list.join(xs, ","))
}
"#;

fn variant(src: &str, producer: bool) -> String {
    src.replace("ATTR", if producer { "@family_producer" } else { "" })
}

#[test]
fn a_module_that_links_no_producer_keeps_its_bytes() {
    let plain = emit(&variant(UNREACHED, false));
    let declared = emit(&variant(UNREACHED, true));
    assert!(!has_retarget(&plain));
    assert!(!has_retarget(&declared), "an unreached producer must not arm the redirect");
    assert_eq!(plain, declared, "an unreached producer declaration must not move a byte");
}

#[test]
fn a_module_that_links_a_producer_carries_the_redirect_and_runs_the_same() {
    let plain = emit(&variant(REACHED, false));
    let family = emit(&variant(REACHED, true));
    assert!(!has_retarget(&plain), "no producer, no redirect");
    assert!(has_retarget(&family), "a linked producer arms the redirect");
    let (a, b) = (run_wasm(&plain).expect("run plain"), run_wasm(&family).expect("run family"));
    assert_eq!(a.stdout, "v1,v2,v3\n");
    assert_eq!(a.stdout, b.stdout);
    assert_eq!(a.stderr, b.stderr);
    eprintln!("family module: {} bytes, plain: {} bytes (+{})", family.len(), plain.len(), family.len() - plain.len());
}
