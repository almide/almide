// THROWAWAY probe: print the v1 render around every heap-slot cell.
const MUT: &str = "var gl: List[Int] = []\n\nfn main() -> Unit = {\n  for i in 0..<3 {\n    list.push(gl, i)\n  }\n  println(\"${gl}\")\n}\n";

#[test]
fn dump() {
    let mut srcs: Vec<(String, String)> = [
        "research/benchmark/perf/globalread/globalread.almd",
        "research/benchmark/perf/v1-shapes/heap_global_copy_read.almd",
        "research/benchmark/perf/v1-shapes/heap_global_slots.almd",
    ]
    .iter()
    .map(|p| (p.to_string(), std::fs::read_to_string(p).unwrap()))
    .collect();
    srcs.push(("mutator".into(), MUT.into()));
    for (p, src) in srcs {
        match almide_mir::pipeline::try_render_rust_source(&src) {
            Ok(r) => {
                println!("=== {p}");
                let lines: Vec<&str> = r.lines().collect();
                for (i, l) in lines.iter().enumerate() {
                    if l.contains("HSLOT") {
                        for k in i.saturating_sub(3)..(i + 6).min(lines.len()) {
                            println!("{k:5}| {}", lines[k]);
                        }
                        println!("-----");
                    }
                }
            }
            Err(e) => println!("=== {p} WALL {e:?}"),
        }
    }
}
