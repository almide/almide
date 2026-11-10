//! A heap module-level `let` compared in an `if` condition and read again
//! after it (`if list.take(t, 2) == ALERT and room >= span(w, ALERT) then
//! ALERT + ..`). The comparison built the global's block as a temporary of
//! the condition and freed it there, while the name stayed cached as that
//! block, so the later readers were handed the freed value and the ownership
//! certifier refused the function (use after free). The cache entry now ends
//! with the block: each later reader builds its own.
use std::process::Command;

/// A list global and a string global, each compared in a condition and read
/// again in the rest of the condition and in the taken arm, in a loop.
const SHAPE: &str = r##"let ALERT: List[Int] = [0x2022, 0x20]
let TAG: String = "alert"

fn span(w: Int, xs: List[Int]) -> Int = w * list.len(xs)

fn fit_tail(w: Int, title: List[Int], room: Int) -> List[Int] =
  if span(w, title) <= room then title else list.drop(title, list.len(title) - room / w)

fn fit_end(w: Int, title: List[Int], room: Int) -> List[Int] =
  if list.take(title, 2) == ALERT and room >= span(w, ALERT) + w
  then ALERT + fit_tail(w, list.drop(title, 2), room - span(w, ALERT))
  else fit_tail(w, title, room)

fn label(s: String, loud: Bool) -> String =
  if string.take(s, 5) == TAG and loud then TAG + "!" + string.drop(s, 5) else s + ":" + TAG

fn show(xs: List[Int]) -> String = list.join(list.map(xs, (c) => int.to_string(c)), ",")

fn main() -> Unit = {
  println(show(fit_end(1, [0x2022, 0x20, 0x61, 0x62, 0x63, 0x64, 0x65], 5)))
  println(show(fit_end(1, [0x2022, 0x20, 0x61], 2)))
  println(show(fit_end(1, [0x61, 0x62, 0x63, 0x64, 0x65], 3)))
  println(label("alert: disk", true) + " " + label("alert: disk", false) + " " + label("note", true))
  var n = 0
  for i in 0..<2000 {
    n = n + list.len(fit_end(1, [0x2022, 0x20, 0x61, 0x62, 0x63, 0x64, 0x65], 4 + i % 3)) + string.len(label("alert", i % 2 == 0))
  }
  println(int.to_string(n))
}
"##;

const SHAPE_STDOUT: &str = "8226,32,99,100,101\n32,97\n99,100,101\nalert!: disk alert: disk:alert note:alert\n26999\n";

/// Readers the cache serves as before: straight-line reuse, a global read in
/// both arms, and a comparison with no later reader.
const CONTROL: &str = r##"let ALERT: List[Int] = [0x2022, 0x20]
let CAP: Int = 3

fn straight(xs: List[Int]) -> Int = {
  let a = list.len(ALERT)
  let b = list.len(xs + ALERT)
  a * 100 + b + list.len(ALERT)
}

fn arms(xs: List[Int]) -> List[Int] =
  if list.len(xs) > CAP then ALERT else xs + ALERT

fn eq_only(xs: List[Int]) -> String =
  if xs == ALERT then "alert" else "plain"

fn main() -> Unit = {
  println(int.to_string(straight([1, 2, 3])))
  println(int.to_string(list.len(arms([1, 2, 3, 4]))) + " " + int.to_string(list.len(arms([1]))))
  println(eq_only([0x2022, 0x20]) + " " + eq_only([1]))
}
"##;

const CONTROL_STDOUT: &str = "207\n2 3\nalert plain\n";

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| format!("{e:?}")))
        .expect("spawn")
        .join()
        .expect("render thread")
}

fn runs_unwalled(name: &str, src: &str, stdout: &str) {
    render(src).unwrap_or_else(|e| panic!("{name} walls: {e}"));
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{name} {extra:?}");
    }
}

#[test]
fn a_global_compared_in_a_condition_is_rebuilt_for_the_readers_after_it() {
    runs_unwalled("global_condition_frame", SHAPE, SHAPE_STDOUT);
}

#[test]
fn straight_line_and_per_arm_readers_of_a_global_are_unchanged() {
    runs_unwalled("global_condition_frame_control", CONTROL, CONTROL_STDOUT);
}
