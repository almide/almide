//! v1 walls, round 21 (family B): shapes the native render refused, now
//! rendered, each checked against its expected output on the native render
//! and on wasm.

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| e.to_string()))
        .unwrap()
        .join()
        .unwrap()
}

fn run(src: &str, target: Option<&str>) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("p.almd");
    std::fs::write(&file, src).expect("source");
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(&file).env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1");
    if let Some(t) = target {
        cmd.arg("--target").arg(t);
    }
    let out = cmd.output().expect("almide run");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("walled"), "{err}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn both(src: &str, want: &str) {
    render(src).expect("renders on the native render");
    assert_eq!(run(src, None), want);
    assert_eq!(run(src, Some("wasm")), want);
}

// A list of string pairs held in a cell (a closure captures the var) and read
// in place: its typed release ends the borrow, the cell frees the list.
const CELL_PAIR_LIST: &str = r#"
fn absorb(ts: List[String]) -> String = {
  var dvs: List[(String, String)] = []
  var scopes: List[Int] = []
  var out: List[String] = []
  for t in ts {
    if t == "{" then {
      list.push(scopes, list.len(dvs))
    } else if t == "}" then {
      let k = list.last(scopes) ?? 0
      dvs = list.take(dvs, k)
      scopes = list.take(scopes, list.len(scopes) - 1)
    } else {
      list.push(dvs, (t, t + "'"))
      let constrained = (x: String) => list.any(dvs, ((a, b)) => a == x or b == x)
      list.push(out, "${t}:${constrained(t)}:${list.len(dvs)}")
    }
  }
  list.join(out, " ")
}
fn main() -> Unit = println(absorb(["a", "{", "b", "c", "}", "d"]))
"#;

#[test]
fn a_cell_held_pair_list_read_in_place_renders() {
    both(CELL_PAIR_LIST, "a:true:1 b:true:2 c:true:3 d:true:2\n");
}
