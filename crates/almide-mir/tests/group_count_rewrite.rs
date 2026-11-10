//! `list.group_by(xs, key) |> map.map((g) => list.len(g))` is ONE counting
//! pass on the native render (`native_group_count`): the runtime's
//! `almide_rt_list_group_count`, which keeps a count per key and builds no
//! group. Spelled literally the chain copies every element for its key,
//! pushes it into a group and copies each group again to measure it. Every
//! refused shape below keeps the two calls.

fn render(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).expect("renders on v1"))
        .expect("spawn")
        .join()
        .expect("join")
}

/// The rendered `main` of a program that prints the map `counts` evaluates to.
fn main_with(items: &str, counts: &str) -> String {
    let code = render(&format!(
        "fn main() -> Unit = {{\n  let xs = {items}\n  let counts = {counts}\n  println(int.to_string(map.len(counts)))\n}}\n"
    ));
    let at = code.find("fn main() {").unwrap_or_else(|| panic!("no main in:\n{code}"));
    let main = &code[at..];
    main[..main.find("\n}\n").map_or(main.len(), |e| e + 2)].to_string()
}

fn count(main: &str, call: &str) -> usize {
    main.lines().filter(|l| l.contains(call)).count()
}

const WORDS: &str = "[\"a\", \"bb\", \"a\"]";

#[test]
fn groups_that_are_only_counted_are_one_counting_pass() {
    for (items, counts) in [
        (WORDS, "list.group_by(xs, (w) => w) |> map.map((g) => list.len(g))"),
        (WORDS, "xs |> list.group_by((w) => string.len(w)) |> map.map((g) => list.len(g))"),
        ("[3, 1, 3]", "map.map(list.group_by(xs, (n) => n % 2), (g) => list.len(g))"),
        ("[3, 1, 3]", "list.group_by(xs, (n) => int.to_string(n)) |> map.map((g) => list.len(g))"),
    ] {
        let main = main_with(items, counts);
        assert_eq!(count(&main, "almide_rt_list_group_count"), 1, "`{counts}` is not one counting pass:\n{main}");
        assert_eq!(count(&main, "almide_rt_list_group_by"), 0, "`{counts}` still builds its groups:\n{main}");
        assert_eq!(count(&main, "almide_rt_map_map_values"), 0, "`{counts}` still walks the groups:\n{main}");
    }
}

#[test]
fn groups_read_for_anything_else_are_built() {
    for (items, counts, why) in [
        (WORDS, "list.group_by(xs, (w) => w) |> map.map((g) => list.len(g) + 1)", "the callback computes on the length"),
        (WORDS, "list.group_by(xs, (w) => w) |> map.map((g) => string.len(list.join(g, \"\")))", "the callback reads the elements"),
        (WORDS, "list.group_by(xs, (w) => w) |> map.map((g) => list.len(xs))", "the length is of another list"),
        (WORDS, "list.group_by(xs, (w) => string.len(w) > 1) |> map.map((g) => list.len(g))", "a Bool key"),
        ("[3, 1, 3]", "list.group_by(xs, (n) => n) |> map.map((g) => list.is_empty(g))", "another callback"),
    ] {
        let main = main_with(items, counts);
        assert_eq!(count(&main, "almide_rt_list_group_count"), 0, "`{counts}` was fused ({why}):\n{main}");
        assert_eq!(count(&main, "almide_rt_list_group_by"), 1, "`{counts}` lost its groups ({why}):\n{main}");
    }
}

/// A group map that is named stays a map of groups: its second reader sees them.
#[test]
fn a_named_group_map_is_built() {
    let code = render(
        "fn main() -> Unit = {\n  let groups = list.group_by([\"a\", \"bb\", \"a\"], (w) => w)\n  \
         let counts = map.map(groups, (g) => list.len(g))\n  \
         println(int.to_string(map.len(counts) + map.len(groups)))\n}\n",
    );
    assert!(!code.contains("almide_rt_list_group_count"), "a named group map was fused away:\n{code}");
}
