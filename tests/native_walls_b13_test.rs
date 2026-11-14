//! v1 walls, round 13 (family B): shapes the native render refused, now rendered,
//! each checked against its expected output. The tuple folds also pin a hang: the
//! fold counter was redefined in the loop and rendered as a shadowing local.

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

const TF: &str = r#"
fn sums(xs: List[Int]) -> Int = {
  let r = list.fold(xs, (0, 1), (acc, x) => {
    let (a, b) = acc
    (a + x, b * 2)
  })
  r.0 + r.1
}

fn main() -> Unit = {
  println(int.to_string(sums([1, 2, 3])))
}
"#;

#[test]
fn a_scalar_tuple_fold_advances_its_counter() {
    render(TF).expect("renders on the native render");
    assert_eq!(run(TF, None), "14\n");
    assert_eq!(run(TF, Some("wasm")), "14\n");
}

const W1: &str = r#"
fn find_at(data: List[Int], want: Int, pos: Int, total: Int) -> Option[Int] = {
  let positions = list.range(0, 100)
  list.fold(positions, (pos, none), (state, _) => {
    let (p, found) = state
    match found {
      some(_) => state
      none =>
        if p + 2 > total then (p, none)
        else {
          let id = list.get(data, p) ?? -1
          if id == want then (p, some(p))
          else {
            let size = list.get(data, p + 1) ?? 0
            (p + 2 + size, none)
          }
        }
    }
  }).1
}

fn show(o: Option[Int]) -> String = match o { some(x) => "some ${int.to_string(x)}", none => "none" }

fn main() -> Unit = {
  let d = [7, 1, 0, 9, 0, 5, 3, 1]
  println(show(find_at(d, 5, 0, 8)))
  println(show(find_at(d, 9, 0, 8)))
  println(show(find_at(d, 4, 0, 8)))
}
"#;

#[test]
fn an_option_tuple_fold_builds_its_option_from_the_carried_tag() {
    render(W1).expect("renders on the native render");
    assert_eq!(run(W1, None), "some 5\nsome 3\nnone\n");
    assert_eq!(run(W1, Some("wasm")), "some 5\nsome 3\nnone\n");
}

const T1: &str = r#"
fn color(s: String) -> Option[Int] = int.parse(s) |> result.to_option

fn palette_entry(p: (List[Int], List[(Int, Int)]), entry: String) -> (List[Int], List[(Int, Int)]) = {
  let (ansi, extra) = p
  match string.split_once(entry, "=") {
    some((i, v)) => match (int.parse(string.trim(i)), color(string.trim(v))) {
      (ok(n), some(rgb)) =>
        if n >= 0 and n < 4 then (list.set(ansi, n, rgb), extra)
        else if n < 256 then {
          var kept = list.filter(extra, (e) => e.0 != n)
          list.push(kept, (n, rgb))
          (ansi, kept)
        } else p,
      _ => p,
    },
    none => p,
  }
}

fn main() -> Unit = {
  let p0 = ([0, 0, 0, 0], [(9, 9)])
  let p1 = palette_entry(p0, "2=7")
  let p2 = palette_entry(p1, "9 = 5")
  let p3 = palette_entry(p2, "x=1")
  let p4 = palette_entry(p3, "300=1")
  let (a, e) = p4
  println(a |> list.map((x) => int.to_string(x)) |> list.join(","))
  println(e |> list.map((x) => "${int.to_string(x.0)}:${int.to_string(x.1)}") |> list.join(","))
}
"#;

#[test]
fn a_filtered_list_grown_after_its_commit_renders() {
    render(T1).expect("renders on the native render");
    assert_eq!(run(T1, None), "0,0,7,0\n9:5\n");
    assert_eq!(run(T1, Some("wasm")), "0,0,7,0\n9:5\n");
}

const G2: &str = r#"
fn rd(p: String) -> Result[List[Int], String] = if p == "" then err("empty path") else ok([string.len(p), 7])

fn total(xs: List[Int]) -> Result[Int, String] = if list.len(xs) == 0 then err("nothing") else ok(list.fold(xs, 0, (a, x) => a + x))

effect fn load(p: String) -> Result[Int, String] = {
  let data = match rd(p) {
    ok(d) => d,
    err(e) => err(e),
  }
  total(data)
}

effect fn main() -> Unit = {
  for p in ["abc", ""] {
    match load(p) {
      ok(n) => println("ok ${int.to_string(n)}"),
      err(e) => println("err ${e}"),
    }
  }
}
"#;

#[test]
fn an_err_arm_bound_match_stays_in_its_fn_on_wasm() {
    // The branch lift used to move this match into a helper, where the err
    // arm returned from the helper: invalid Rust on one leg, a trap on wasm.
    assert_eq!(run(G2, Some("wasm")), "ok 10\nerr empty path\n");
}

