//! #2755 (#1696 step 4) — AGGREGATES, DESTRUCTURING and INTERPOLATION in the
//! structural witness, on the flat alphabet `{i, a, d, m}`. A tuple or record
//! literal is a fresh block whose slot stores are the payload-store hook's
//! (`am` for a shared Var, `im` for a moved temporary); a `let (a, b) = t`
//! binds VIEWS of the named subject's slots; an interpolation reads its parts
//! and its captured block is a fresh value. The shapes whose sites no hook
//! records decline with a counted reason instead of certifying.

const PROGRAM: &str = r#"type Pt = { name: String, n: Int, tag: String = "t" }

fn pair(s: String) -> (String, Int) = (s, 1)

fn pair_fresh(n: Int) -> (List[Int], Int) = ([n], n)

fn point(s: String) -> Pt = Pt { name: s, n: 2 }

fn left(p: (String, Int)) -> String = {
  let (a, _) = p
  a
}

fn left_call(s: String) -> Int = {
  let (a, n) = pair(s)
  string.len(a) + n
}

fn hello(name: String, k: Int) -> String = "hi ${name} ${k}!"

fn shout(name: String) -> Unit = println("hey ${name}")

fn fl(x: Float) -> String = "x=${x}"

fn subj(a: Int, b: Int) -> Int = match (a, b) {
  (0, _) => 0,
  _ => 1,
}

effect fn main() -> Unit = {
  println("${left(pair("x"))} ${list.len(pair_fresh(3).0)} ${point("p").name}")
  println("${left_call("ab")} ${hello("y", 2)} ${fl(1.5)} ${subj(0, 1)}")
  shout("z")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("aggregates.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn aggregates_witness_exactly_and_unhooked_shapes_decline() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let expect = [
        // The borrowed param shares into the tuple's slot; the block moves out.
        ("pair", "am\nim\n"),
        // A fresh list literal moves into the slot; the block moves out.
        ("pair_fresh", "im\nim\n"),
        // A record literal's field store is the same share-and-move, and the
        // omitted field's literal default is born and moves in.
        ("point", "am\nim\nim\n"),
        // The borrowed param holds no credit (an empty line); the binder is a
        // view of its slot whose returned share moves out.
        ("left", "\nam\n"),
        // The named subject (arg_temps.rs) is pair's owned result, released at
        // the exit; the binders are views that are only read.
        ("left_call", "\nid\n\n"),
        // The interpolation reads its parts; the captured block moves out.
        ("hello", "\nim\n"),
        // A printed interpolation builds no block at all.
        ("shout", "\n"),
    ];
    for (name, cert) in expect {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed; got {:?}", w.get(name)));
        assert_eq!(got, cert, "{name}");
        assert!(accepted(got), "{name}: the portable checker must accept {got:?}");
    }
    // A Float part's formatted block is copied into the line and released
    // (#2973, fixed): born and released at the part (`id`); the captured
    // interpolation moves out.
    let fl = w.get("fl").map(String::as_str);
    assert_eq!(fl, Some("id\nim\n"));
    assert!(accepted(fl.unwrap()), "fl: the portable checker must accept {fl:?}");
    // A tuple literal as a match subject: arg_temps.rs names the constructed
    // subject first (#2971), so the Bind hook records the fresh block and the
    // exit plan releases it — certified, and the checker accepts it.
    let subj = w.get("subj").map(String::as_str);
    assert_eq!(subj, Some("id\n"));
    assert!(accepted(subj.unwrap()), "subj: the portable checker must accept {subj:?}");
}
