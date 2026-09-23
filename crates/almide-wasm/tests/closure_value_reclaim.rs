//! #2010 — the last shapes off the bump graveyard: closure envs (ruling B,
//! the drop glue named in the env payload), C-319 cells (refcounted, one
//! credit per capturing env), the self-assign through a module call, and
//! Value blocks (tagged payload glue). Each row builds and drops the shape
//! per iteration and pins a FLAT high-water mark across N=1000 vs N=8000
//! (C-066: the structural leg reclaims what native frees). On the develop
//! build before this change the rows grew 48 / 128 / 64 / 1968 B per round.

mod harness;
use harness::run_wasm;

fn heap_of(src: &str) -> (u64, String) {
    let ir = almide_spine::s5::lower_to_ir("reclaim.almd", src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (out.heap_end.expect("__heap"), out.stdout.trim().to_string())
}

fn flat(name: &str, program: impl Fn(u32) -> String, expect_1000: &str, expect_8000: &str) {
    let (h1, o1) = heap_of(&program(1000));
    let (h8, o8) = heap_of(&program(8000));
    assert_eq!(o1, expect_1000, "{name}: output at N=1000");
    assert_eq!(o8, expect_8000, "{name}: output at N=8000");
    assert_eq!(h1, h8, "{name}: the high-water mark must not grow with N (N=1000 {h1} B, N=8000 {h8} B)");
}

fn looped(n: u32, prelude: &str, body: &str) -> String {
    format!(
        "{prelude}\neffect fn main() -> Unit = {{\n  var total = 0\n  for i in 0..<{n} {{\n{body}\n  }}\n  println(\"${{total}}\")\n}}\n"
    )
}

/// A closure capturing a fresh string, built and called per round: the env
/// block and its captured handle are released through the env's own glue.
#[test]
fn a_closure_env_releases_its_captures() {
    flat(
        "closure env",
        |n| {
            looped(
                n,
                "",
                "    let s = \"k\" + int.to_string(i % 7)\n    let f = (x: Int) => x + string.len(s)\n    total = total + f(1)",
            )
        },
        "3000",
        "24000",
    );
}

/// A C-319 cell captured and mutated by a closure: the cell is released
/// with its last holder (the frame's exit, the env's glue), and every
/// replaced occupant as it is replaced.
#[test]
fn a_captured_cell_is_released_with_its_last_holder() {
    flat(
        "cell",
        |n| {
            looped(
                n,
                "effect fn run(i: Int) -> Int = {\n  var acc: List[String] = []\n  let note = (s: String) => {\n    acc = acc + [s]\n  }\n  note(\"a\" + int.to_string(i % 7))\n  note(\"b\")\n  list.len(acc)\n}",
                "    total = total + run(i)!",
            )
        },
        "2000",
        "16000",
    );
}

/// `s = string.to_upper(s)` / `t = set.insert(t, x)`: a module call never
/// spends the var's credit, so the old occupant is released by the assign.
#[test]
fn a_self_assign_through_a_module_call_releases_the_old_value() {
    flat(
        "self-assign",
        |n| {
            looped(
                n,
                "",
                "    var s = \"ab\" + int.to_string(i % 7)\n    s = string.to_upper(s)\n    var t = set.from_list([\"a\", \"b\" + int.to_string(i % 7)])\n    t = set.insert(t, \"c\")\n    total = total + string.len(s) + set.len(t)",
            )
        },
        "6000",
        "48000",
    );
}

/// Value blocks and Map[String, Value] values: every Value arm (merge, pick,
/// omit, rename, field, set_path, remove_path, keys, to_map) takes its own
/// credit on what it shares, so the tagged glue releases each exactly once.
#[test]
fn value_blocks_release_their_payloads() {
    flat(
        "value",
        |n| {
            looped(
                n,
                "import json",
                "    let k = \"x\" + int.to_string(i % 5)\n    let doc = json.parse(\"{\\\"a\\\":{\\\"b\\\":[1,2,3]},\\\"\" + k + \"\\\":\\\"s\\\"}\") ?? value.null()\n    let d2 = json.set_path(doc, json.index(json.field(json.field(json.root(), \"a\"), \"b\"), 1), value.str(\"v\" + k)) ?? value.null()\n    let d3 = json.set_path(d2, json.field(json.field(json.root(), \"new\"), \"deep\"), value.int(i)) ?? value.null()\n    let d4 = json.remove_path(d3, json.field(json.root(), k))\n    let d5 = value.merge(d4, value.object([(\"m\", value.str(k)), (\"a\", value.null())]))\n    let d6 = value.pick(d5, [\"a\", \"m\", \"new\"])\n    let d7 = value.omit(d6, [\"m\"])\n    let d8 = value.to_camel_case(value.object([(\"snake_key\", value.array([value.str(k)]))]))\n    let f = value.field(d5, \"m\") ?? value.null()\n    let s = value.as_string(f) ?? \"\"\n    let ks = value.keys(d5)\n    let m = json.to_map(d5) ?? [:]\n    let mv: Map[String, Value] = [k: value.str(\"y\" + int.to_string(i % 3))]\n    total = total + string.len(json.stringify(d7)) + string.len(json.stringify(d8)) + string.len(s) + list.len(ks) + map.len(m) + map.len(mv)",
            )
        },
        "56890",
        "462890",
    );
}
