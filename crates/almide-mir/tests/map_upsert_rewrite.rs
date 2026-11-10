//! `m[k] = e(map.get(m, k) ?? d)` is ONE keyed operation on the native
//! render (`native_map_upsert`): a `map.upsert` whose update closure is `e`
//! over the slot's value and whose `init` is `e(d)`. Spelled literally it is
//! a `map.get` followed by a `map.set` of the same key — two probes per
//! update. Every refused shape below keeps that two-step form: `init` runs on
//! every call, so the rule admits only what is total and cannot observe `m`.

fn render(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).expect("renders on v1"))
        .expect("spawn")
        .join()
        .expect("join")
}

/// The rendered `main` of a program whose loop body is `update`.
fn main_with(decls: &str, map_ty: &str, items: &str, update: &str) -> String {
    let code = render(&format!(
        "let bonus = 3\n\nfn main() -> Unit = {{\n  let step = 2\n  var total = 0\n  {decls}\n  var m: {map_ty} = map.new()\n  \
         for w in {items} {{\n    total = total + 1\n    {update}\n  }}\n  println(int.to_string(map.len(m) + total + step))\n}}\n"
    ));
    let at = code.find("fn main() {").unwrap_or_else(|| panic!("no main in:\n{code}"));
    let main = &code[at..];
    main[..main.find("\n}\n").map_or(main.len(), |e| e + 2)].to_string()
}

fn count(main: &str, call: &str) -> usize {
    main.lines().filter(|l| l.contains(call)).count()
}

const WORDS: &str = "[\"a\", \"b\", \"a\"]";

#[test]
fn a_defaulted_read_update_is_one_upsert() {
    for (map_ty, items, update) in [
        ("Map[String, Int]", WORDS, "m[w] = (map.get(m, w) ?? 0) + 1"),
        ("Map[String, Int]", WORDS, "m[w] = step * (map.get(m, w) ?? 1) - 1"),
        ("Map[String, Int]", WORDS, "m[w] = -(map.get(m, w) ?? step)"),
        ("Map[Int, Int]", "[3, 1, 3]", "m[w] = (map.get(m, w) ?? 0) + step"),
        ("Map[Int, Float]", "[3, 1, 3]", "m[w] = -(map.get(m, w) ?? 1.5) * 2.0 / 4.0"),
    ] {
        let main = main_with("", map_ty, items, update);
        assert_eq!(count(&main, "almide_rt_map_upsert"), 1, "`{update}` is not one upsert:\n{main}");
        assert_eq!(count(&main, "almide_rt_map_set("), 0, "`{update}` still writes separately:\n{main}");
        assert_eq!(count(&main, "almide_rt_map_get("), 0, "`{update}` still reads separately:\n{main}");
    }
}

#[test]
fn an_update_outside_the_rule_stays_two_steps() {
    for (decls, update, why) in [
        ("", "m[w] = 100 / ((map.get(m, w) ?? 4) + 1)", "Int `/` can abort, and init runs on every call"),
        ("", "m[w] = (map.get(m, w) ?? 1) % 7", "Int `%` can abort"),
        ("", "m[w] = (map.get(m, w) ?? 0) + (map.get(m, w) ?? 0)", "two reads of the map"),
        ("", "m[w] = (map.get(m, w) ?? 0) + (map.get(m, \"a\") ?? 0)", "a read of another key"),
        ("", "m[w] = (map.get(m, w) ?? 0) + total", "a `var` operand would be captured"),
        ("", "m[w] = (map.get(m, w) ?? 0) + bonus", "a global operand"),
        ("", "m[w] = (map.get(m, w) ?? 0) + string.len(w)", "a call, and a read of the key"),
        ("", "m[w] = int.abs(map.get(m, w) ?? 0)", "a call around the read"),
        ("", "m[\"k\"] = (map.get(m, \"k\") ?? 0) + 1", "the key is not a variable"),
        ("var other: Map[String, Int] = map.new()", "m[w] = (map.get(other, w) ?? 0) + 1", "the read is of another map"),
        ("", "m[w] = 7", "no read at all"),
    ] {
        let main = main_with(decls, "Map[String, Int]", WORDS, update);
        assert_eq!(count(&main, "almide_rt_map_upsert"), 0, "`{update}` was fused ({why}):\n{main}");
        assert_eq!(count(&main, "almide_rt_map_set("), 1, "`{update}` lost its write ({why}):\n{main}");
    }
}
