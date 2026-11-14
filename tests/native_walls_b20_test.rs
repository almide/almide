//! v1 walls, round 20 (family B): shapes the native render refused, now
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

// A move-mode call under `not` in an argument: the write-back hands `doc` and
// `c` fresh references inside the operand block, and they outlive it.
const WRITEBACK_UNDER_NOT: &str = r#"
type Comp = { names: List[String], n: Int }
type Doc = { items: List[String], n: Int }
fn whole(mut c: Comp, t: String) -> List[String] = {
  c.n = c.n + 1
  string.split(t, " ")
}
fn from_parsed(p: List[String], src: String, mut c: Comp) -> Doc = {
  list.push(c.names, src)
  Doc { items: p, n: list.len(c.names) }
}
fn reparse(mut doc: Doc, src: String, f: (String) -> Int, mut c: Comp) -> Bool = {
  doc.items = doc.items + [src]
  c.n = c.n + f(src)
  false
}
fn materialize(d: Doc) -> List[String] = d.items
fn same(a: List[String], b: List[String]) -> Bool = list.join(a, ",") == list.join(b, ",")
fn main() -> Unit = {
  var c = Comp { names: [], n: 0 }
  var doc = from_parsed(whole(c, "a b"), "a b", c)
  println("${not reparse(doc, "zz", (s) => string.len(s), c)}")
  println("${same(materialize(doc), whole(c, "a b zz"))} ${c.n} ${list.join(c.names, ",")}")
}
"#;

#[test]
fn a_write_back_under_not_keeps_the_written_values() {
    both(WRITEBACK_UNDER_NOT, "true\ntrue 4 a b\n");
}
