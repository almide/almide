//! Four shapes the certified native render refused (#4640), each against wasm
//! under the ownership certificate with no fallback to the standard codegen.
//!
//! - `==` over a type that holds a Map (a record field, a tuple slot, an
//!   Option or Result payload, a Map's own heap values): the slot equality
//!   engine had no comparison for the Map, so the fn was refused. The
//!   comparison is now a typed helper written as ordinary IR.
//! - `==` over a variant whose payload is a list of anonymous records: the
//!   list had no element comparison, and an element that reaches the list
//!   again built its helper twice.
//! - a list whose element is a pair holding a list of heap values
//!   (`List[(String, List[Value])]`): no element class, so a fold that
//!   appends to one was refused.
//! - a module-level list literal that names other immutable globals.
use std::process::Command;

/// The program on the native leg under the ownership certificate with no
/// fallback to the standard codegen, and on wasm: both print `expected`.
fn renders_and_agrees(name: &str, source: &str, expected: &str) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, source).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "{name} {extra:?}");
    }
}

/// A record with two Map fields under Result and Option, a Map of lists, a tuple with a Map slot.
const MAP_EQ: &str = r##"type Spec = { name: String, tags: Map[String, String], memo: Map[Int, List[Int]] }

fn mk(n: String) -> Result[Spec, String] =
  if n == "" then err("empty") else ok({ name: n, tags: ["k": n, "z": "1"], memo: [1: [2, 3], 4: [5]] })

fn flipped(n: String) -> Result[Spec, String] =
  ok({ name: n, tags: ["z": "1", "k": n], memo: [4: [5], 1: [2, 3]] })

fn same(a: Option[Spec], b: Option[Spec]) -> Bool = a == b

effect fn main() -> Unit = {
  let a: Result[Spec, String] = mk("x")
  let b: Result[Spec, String] = flipped("x")
  let c: Result[Spec, String] = mk("y")
  let e: Result[Spec, String] = mk("")
  println(if a == b then "eq" else "ne")
  println(if a == c then "eq" else "ne")
  println(if e == e then "eq" else "ne")
  println(if a != e then "ne" else "eq")
  let m: Map[String, List[String]] = ["p": ["a", "b"], "q": ["c"]]
  let n: Map[String, List[String]] = ["q": ["c"], "p": ["a", "b"]]
  let o: Map[String, List[String]] = ["p": ["a"], "q": ["c"]]
  var hits = 0
  for i in 0..<3 {
    if m == n then { hits = hits + 1 } else { () }
    if m == o then { hits = hits + 10 } else { () }
  }
  println(int.to_string(hits))
  let s: Option[Spec] = some({ name: "n", tags: map.new(), memo: map.new() })
  println(if same(s, none) then "eq" else "ne")
  println(if same(s, s) then "eq" else "ne")
  let t: (Int, Map[String, String]) = (1, ["a": "b"])
  let u: (Int, Map[String, String]) = (1, ["a": "c"])
  println(if t == u then "eq" else "ne")
  println(if t == t then "eq" else "ne")
}
"##;

const MAP_EQ_STDOUT: &str = "eq\nne\neq\nne\n3\nne\neq\nne\neq\n";

/// A recursive variant whose payload is a list of anonymous records that hold the variant.
const ANON_RECORD_LIST_EQ: &str = r##"type Nbt =
  | End
  | Byte(Int)
  | NbtFloat(Float)
  | NbtList(Int, List[Nbt])
  | Compound(List[{ name: String, value: Nbt }])

fn get(nbt: Nbt, key: String) -> Option[Nbt] = {
  match nbt {
    Compound(entries) => {
      var found: Option[Nbt] = none
      for entry in entries {
        if entry.name == key then { found = some(entry.value) } else { () }
      }
      found
    },
    _ => none,
  }
}

effect fn main() -> Unit = {
  let c = Compound([{ name: "a", value: Byte(1) }, { name: "b", value: Compound([{ name: "c", value: NbtList(1, [Byte(2), NbtFloat(1.5)]) }]) }])
  let d = Compound([{ name: "a", value: Byte(1) }, { name: "b", value: Compound([{ name: "c", value: NbtList(1, [Byte(2), NbtFloat(2.5)]) }]) }])
  let none_nbt: Option[Nbt] = none
  println(if get(c, "zz") == none_nbt then "eq" else "ne")
  println(if get(c, "a") == some(Byte(1)) then "eq" else "ne")
  println(if get(c, "b") == get(c, "b") then "eq" else "ne")
  println(if get(c, "b") == get(d, "b") then "eq" else "ne")
  println(if c == c then "eq" else "ne")
  println(if c != d then "ne" else "eq")
}
"##;

const ANON_RECORD_LIST_EQ_STDOUT: &str = "eq\neq\neq\nne\neq\nne\n";

/// A fold that appends to and rewrites a list of (String, List[Value]) pairs.
const PAIR_LIST_FOLD: &str = r##"fn merge_turns(turns: List[(String, List[Value])]) -> List[(String, List[Value])] =
  list.fold(list.filter(turns, (t) => not list.is_empty(t.1)), [], (acc, t) => match list.last(acc) {
    some(last) => if last.0 == t.0 then list.update(acc, list.len(acc) - 1, (_) => (last.0, last.1 + t.1)) else acc + [t],
    none => [t],
  })

effect fn main() -> Unit = {
  let ts: List[(String, List[Value])] = [("user", [value.int(1)]), ("user", [value.int(2)]), ("tool", []), ("assistant", [value.string("a")]), ("user", [value.int(3)])]
  let m = merge_turns(ts)
  println(int.to_string(list.len(m)))
  for t in m { println("${t.0} ${list.len(t.1)}") }
}
"##;

const PAIR_LIST_FOLD_STDOUT: &str = "3\nuser 2\nassistant 1\nuser 1\n";

/// Module-level lists whose elements are other immutable globals, one of them a list of lists.
const GLOBAL_LIST: &str = r##"let ST_A = "measured"
let ST_B = "not-run"
let ST_C = ST_A + "+" + ST_B
let STATES = [ST_A, ST_B, "literal", ST_C]
let ROWS = [STATES, [ST_B]]

fn count() -> Int = list.len(STATES)

effect fn main() -> Unit = {
  println(int.to_string(count()))
  println(list.join(STATES, ","))
  for r in ROWS { println(int.to_string(list.len(r))) }
  var total = 0
  var i = 0
  while i < 3 {
    total = total + list.len(STATES)
    i = i + 1
  }
  println(int.to_string(total))
}
"##;

const GLOBAL_LIST_STDOUT: &str = "4\nmeasured,not-run,literal,measured+not-run\n4\n1\n12\n";

#[test]
fn equality_reaches_a_map_inside_a_record_a_tuple_and_a_carrier() {
    renders_and_agrees("map_eq", MAP_EQ, MAP_EQ_STDOUT);
}

#[test]
fn equality_compares_a_list_of_anonymous_records_inside_a_recursive_variant() {
    renders_and_agrees("anon_rec_list_eq", ANON_RECORD_LIST_EQ, ANON_RECORD_LIST_EQ_STDOUT);
}

#[test]
fn a_fold_builds_a_list_of_pairs_that_hold_a_list_of_heap_values() {
    renders_and_agrees("pair_list", PAIR_LIST_FOLD, PAIR_LIST_FOLD_STDOUT);
}

#[test]
fn a_module_level_list_literal_names_other_immutable_globals() {
    renders_and_agrees("global_list", GLOBAL_LIST, GLOBAL_LIST_STDOUT);
}

/// Float values in a Map: `-0.0` equals `0.0` and a NaN equals nothing, bare,
/// in a list, in a tuple and under a record. The runtime map comparison the
/// native render called before read the bits and answered the opposite.
const FLOAT_MAP_EQ: &str = r##"type Weights = { w: Map[String, Float], n: String }

effect fn main() -> Unit = {
  let nan = 0.0 / 0.0
  let a: Map[String, Float] = ["k": -0.0, "z": 1.5]
  let b: Map[String, Float] = ["z": 1.5, "k": 0.0]
  let c: Map[String, Float] = ["k": nan]
  println(if a == b then "eq" else "ne")
  println(if c == c then "eq" else "ne")
  println(if a != c then "ne" else "eq")
  let d: Map[String, List[Float]] = ["k": [-0.0, 1.0], "z": [2.0]]
  let e: Map[String, List[Float]] = ["z": [2.0], "k": [0.0, 1.0]]
  let f: Map[String, List[Float]] = ["k": [nan]]
  println(if d == e then "eq" else "ne")
  println(if f == f then "eq" else "ne")
  let g: Weights = { w: ["a": -0.0], n: "x" }
  let h: Weights = { w: ["a": 0.0], n: "x" }
  println(if g == h then "eq" else "ne")
  let i: Map[String, (Float, Int)] = ["k": (-0.0, 1)]
  let j: Map[String, (Float, Int)] = ["k": (0.0, 1)]
  println(if i == j then "eq" else "ne")
}
"##;

const FLOAT_MAP_EQ_STDOUT: &str = "eq\nne\nne\neq\nne\neq\neq\n";

#[test]
fn a_map_with_float_values_compares_them_as_floats() {
    renders_and_agrees("float_map_eq", FLOAT_MAP_EQ, FLOAT_MAP_EQ_STDOUT);
}
