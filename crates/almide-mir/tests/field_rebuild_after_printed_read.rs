//! A record rebuilt with one field changed (`cell.memo = map.set(cell.memo,
//! ..)`) moves its other fields into the new record on the native render,
//! also when an earlier interpolation printed one of them (#4585). The
//! interpolation's text is written at its print, before the rebuild; the
//! check that the earlier read is over followed the print and saw that. It
//! refused any earlier read whose text is deferred at all, so the field was
//! copied and the old copy dropped on every rebuild in the loop.

fn render(src: &str) -> String {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).expect("renders on v1"))
        .expect("spawn")
        .join()
        .expect("join")
}

#[test]
fn a_field_printed_before_the_loop_moves_at_each_rebuild() {
    let code = render(
        "type Cell = { name: String, memo: Map[String, Int], words: List[String] }\n\
         fn main() -> Unit = {\n\
         \x20 var cell = { name: \"c\", memo: map.new(), words: [\"a\"] }\n\
         \x20 println(\"${cell.name}\")\n\
         \x20 for i in 0..<10 {\n\
         \x20   cell.memo = map.set(cell.memo, int.to_string(i % 7), i)\n\
         \x20 }\n\
         \x20 println(\"${cell.name} ${map.len(cell.memo)} ${list.len(cell.words)}\")\n\
         }\n",
    );
    let at = code.find("fn main() {").unwrap_or_else(|| panic!("no main in:\n{code}"));
    let main = &code[at..];
    let main = &main[..main.find("\n}\n").map_or(main.len(), |e| e + 2)];
    let lp = main.find("for v").unwrap_or_else(|| panic!("no loop in:\n{main}"));
    let body = &main[lp..];
    let body = &body[..body.find("\n    }\n").unwrap_or_else(|| panic!("no loop end in:\n{main}"))];
    assert!(!body.contains(".clone()"), "a field copied at the rebuild:\n{body}");
}
